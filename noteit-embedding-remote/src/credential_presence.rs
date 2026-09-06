//! Whether a key exists, answered without ever holding one.
//!
//! §53 asks a diagnostic to be able to say *"credential present: yes/no,
//! never the value"*. This module is the yes/no, and it is deliberately unable
//! to be anything else: there is no function here that returns a secret, and
//! the one that looks at the file compares a line's shape and drops it.
//!
//! ## Why this is not `noteit-embed::credential`
//!
//! Because the process asking is not the process that uses it. `noteit-cli`
//! and `noteit-mcp` link this crate and must not link the worker — that is the
//! whole point of the split — so they cannot call the resolver. The duplication
//! is one filename and one line shape, and `scripts/check-embed-boundary`
//! checks that the two crates still agree on both rather than leaving it to
//! whoever edits one of them.
//!
//! ## What it looks at, and what it does not
//!
//! The file only. The worker's *environment* is not consulted, because the
//! worker's environment is built by `crate::worker::spawn` from an allowlist
//! that contains no credential name — so a key in this process's environment
//! is precisely the thing that will **not** reach the worker, and reporting it
//! as "present" would be reporting the wrong answer confidently.

use noteit_embed_protocol::ProviderId;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The file the worker reads its keys from.
///
/// Named here and in `noteit-embed/src/credential.rs`, and the boundary gate
/// requires the two to be the same string.
pub const CREDENTIALS_FILE: &str = "credentials.toml";

/// Where that file is.
pub fn credentials_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CREDENTIALS_FILE)
}

/// What a diagnostic may know about a provider's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialPresence {
    /// A usable line for this provider is there.
    Present,
    /// The file exists and has no line for this provider, or there is no file.
    Absent,
    /// The file is there and the worker will refuse it: wrong owner, or a mode
    /// wider than `0600`.
    ///
    /// Distinguished from `Absent` because "you have not set a key" and "your
    /// key file is readable by everybody on this machine" are different things
    /// for a person to be told, and the second is the one worth acting on.
    Insecure,
}

impl CredentialPresence {
    pub fn is_present(self) -> bool {
        matches!(self, Self::Present)
    }
}

/// Whether the worker would find a key for this provider.
///
/// Returns three states and no secret. The value is compared against emptiness
/// inside this function and is never returned, never logged and never stored:
/// the only thing that leaves is one of three words.
pub fn presence(provider: ProviderId, config_dir: &Path) -> CredentialPresence {
    let path = credentials_path(config_dir);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return CredentialPresence::Absent;
    };
    // The same three refusals the worker applies, so a status cannot say
    // "present" about a file the worker will reject — the class of disagreement
    // 4.3C.R1 already had to fix once between a diagnostic and a loader.
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return CredentialPresence::Insecure;
    }
    if metadata.uid() != current_uid() {
        return CredentialPresence::Insecure;
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return CredentialPresence::Insecure;
    }
    if metadata.len() > 64 * 1024 {
        return CredentialPresence::Insecure;
    }
    let Ok(text) = fs::read_to_string(&path) else {
        return CredentialPresence::Absent;
    };
    let wanted = provider.as_str();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != wanted {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or(value)
            .trim();
        // The only thing asked of the value, and the only thing that leaves.
        return if value.is_empty() {
            CredentialPresence::Absent
        } else {
            CredentialPresence::Present
        };
    }
    CredentialPresence::Absent
}

fn current_uid() -> u32 {
    fs::metadata("/proc/self")
        .map(|metadata| metadata.uid())
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temporary(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "noteit-remote-presence-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");
        base
    }

    fn write(directory: &Path, body: &str, mode: u32) {
        let path = credentials_path(directory);
        let mut file = fs::File::create(&path).expect("create");
        file.write_all(body.as_bytes()).expect("write");
        drop(file);
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("chmod");
    }

    #[test]
    fn a_present_key_is_present_and_its_value_never_leaves() {
        let directory = temporary("present");
        write(
            &directory,
            "openai = \"NOTEIT_TEST_SECRET_NEVER_EXPOSE_7F4A2\"\n",
            0o600,
        );
        let answer = presence(ProviderId::OpenAi, &directory);
        assert_eq!(answer, CredentialPresence::Present);
        // The type has three inhabitants and none of them is a string.
        let printed = format!("{answer:?}");
        assert!(!printed.contains("7F4A2"), "{printed}");
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_provider_without_a_line_is_absent() {
        let directory = temporary("other");
        write(&directory, "openai = \"algo\"\n", 0o600);
        assert_eq!(
            presence(ProviderId::Voyage, &directory),
            CredentialPresence::Absent
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn no_file_is_absent_rather_than_an_error() {
        let directory = temporary("none");
        for provider in ProviderId::ALL {
            assert_eq!(presence(provider, &directory), CredentialPresence::Absent);
        }
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_world_readable_file_is_reported_as_insecure_and_not_as_present() {
        // The distinction that matters: "you have no key" and "everybody on
        // this machine can read your key" are different things to be told.
        let directory = temporary("insecure");
        write(&directory, "openai = \"algo\"\n", 0o644);
        assert_eq!(
            presence(ProviderId::OpenAi, &directory),
            CredentialPresence::Insecure
        );
        assert!(!presence(ProviderId::OpenAi, &directory).is_present());
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn an_empty_value_is_absent() {
        let directory = temporary("emptyvalue");
        write(&directory, "openai = \"\"\n", 0o600);
        assert_eq!(
            presence(ProviderId::OpenAi, &directory),
            CredentialPresence::Absent
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn the_environment_is_not_consulted() {
        // A key in *this* process's environment is precisely what the spawner
        // refuses to forward, so reporting it as present would be confidently
        // wrong.
        let directory = temporary("env");
        std::env::set_var("OPENAI_API_KEY", "NOTEIT_TEST_SECRET_NEVER_EXPOSE_7F4A2");
        assert_eq!(
            presence(ProviderId::OpenAi, &directory),
            CredentialPresence::Absent
        );
        std::env::remove_var("OPENAI_API_KEY");
        fs::remove_dir_all(&directory).ok();
    }
}
