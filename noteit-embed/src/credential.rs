//! Finding the provider's key, and making sure it is the only thing that ever
//! knows it.
//!
//! ## Where the credential comes from, and where it deliberately does not
//!
//! §10 of `docs/semantic-retrieval.md` left the resolution order "to confirm in
//! the subphase that implements it". This is that subphase, and the decision is
//! ADR-060. Two sources, in order:
//!
//! 1. **this process's own environment** — `OPENAI_API_KEY`, `GEMINI_API_KEY`,
//!    `VOYAGE_API_KEY`, the names the vendors themselves document;
//! 2. **a restricted file**, `$XDG_CONFIG_HOME/note-it/credentials.toml`,
//!    which must be a regular file owned by this user with mode `0600`.
//!
//! And one that is deliberately absent: the **spawner never forwards a key**.
//! `noteit-embedding-remote` builds this process's environment with
//! `env_clear()` and an explicit allowlist that contains no credential name, so
//! source 1 is empty in the ordinary product path and the file is what answers.
//! That is what makes "`noteit-mcp` does not receive `OPENAI_API_KEY`"
//! (§23) a fact about the code rather than a hope: the MCP process never reads
//! one, so it has nothing to pass on, and
//! `noteit-embed/tests/credential_isolation.rs` reads
//! `/proc/<worker>/environ` to prove it.
//!
//! Source 1 survives because a person running `noteit-embed` by hand — an
//! opt-in smoke test against a real API (§48) — needs some way in, and
//! the vendors' own variable is the least surprising one.
//!
//! **Why not the Secret Service.** The specification prefers a keyring over a
//! file, and this build does not implement one. The reason is the shape of this
//! particular process: it is the single component with a TLS stack and a route
//! to the internet, and its whole justification is that its attack surface is
//! small enough to read. A keyring client adds a D-Bus connection, an IPC
//! surface and a dependency subtree to exactly that process. The restricted
//! file is about sixty lines and every one of its refusals is testable without
//! a session bus. ADR-060 records this as a deliberate, revisitable trade and
//! not as an omission — a keyring source can be added as a *third* entry
//! without changing anything else here.
//!
//! ## The type
//!
//! [`Credential`] has no `Debug`, no `Display`, no `Serialize` and no accessor
//! that returns the secret as anything but a borrowed `&str` used to build one
//! header. Every one of those is a way a key reaches a log line, a panic
//! message, a snapshot or an error chain (§24), and the cheapest defence
//! is a type that cannot be printed at all.

use noteit_embed_protocol::ProviderId;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The environment variable each provider documents for its own key.
pub const fn environment_variable(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::OpenAi => "OPENAI_API_KEY",
        ProviderId::Gemini => "GEMINI_API_KEY",
        ProviderId::Voyage => "VOYAGE_API_KEY",
    }
}

/// Every variable name this product associates with a secret.
///
/// Used by the spawner to assert that none of them is being forwarded, and by
/// the redaction tests to know what to look for.
pub const CREDENTIAL_VARIABLES: [&str; 3] = ["OPENAI_API_KEY", "GEMINI_API_KEY", "VOYAGE_API_KEY"];

/// A provider key, and nothing that can print one.
///
/// Not `Debug`, not `Display`, not `Clone` into anything that outlives the
/// request. `expose` is the single way out and its name says what it is for,
/// so a review can find every use of it with one grep.
pub struct Credential(String);

impl Credential {
    /// For tests and for the one caller that builds an `Authorization` header.
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The secret itself. The only accessor, deliberately awkward to name.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// A `Debug` that cannot leak, because a struct holding one will derive it.
///
/// Without this, `#[derive(Debug)]` anywhere upstream would print the key into
/// whatever formatted it. With it, the same derive prints a constant.
impl std::fmt::Debug for Credential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Credential(<redacted>)")
    }
}

/// Overwrites the bytes on the way out.
///
/// Not a guarantee — the allocator, a `String` that reallocated while being
/// built, and the kernel's own copy of this process's environment are all
/// outside the reach of a `Drop`. It costs nothing and removes the copy this
/// code is responsible for, which is the part it can actually answer for.
impl Drop for Credential {
    fn drop(&mut self) {
        // SAFETY of intent rather than of memory: writing zeroes over the
        // bytes of a `String` that is about to be dropped is ordinary safe
        // Rust; the compiler is free to elide it, which is why the doc comment
        // above refuses to call this a guarantee.
        let bytes = unsafe { self.0.as_bytes_mut() };
        bytes.fill(0);
    }
}

/// Why no usable credential was found.
///
/// Three facts, no paths and no values. `Unreadable` deliberately does not say
/// *which* file or *what* the error was: this type is one `?` away from the
/// wire, and a path is the sort of thing that ends up in somebody's issue
/// report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    /// Neither source had one.
    Missing,
    /// The file exists and its permissions are wider than the owner.
    Insecure,
    /// The file exists and could not be read or parsed.
    Unreadable,
}

/// Where the credentials file lives.
///
/// The config directory and not the data directory, by the same rule the rest
/// of Note-it follows — and never inside `notes/`, `trash/` or `backups/`.
pub fn credentials_path(config_dir: &Path) -> PathBuf {
    config_dir.join("credentials.toml")
}

/// Resolves this provider's credential, or says why it could not.
///
/// The environment first, because a person who set one meant it. Then the
/// file, which has to earn being read: a symlink is refused rather than
/// followed, a file owned by somebody else is refused, and any bit outside
/// `0600` is refused. `chmod 644` on a key is not a warning worth printing and
/// carrying on from — it is a key that other accounts on this machine can
/// read.
pub fn resolve(provider: ProviderId, config_dir: &Path) -> Result<Credential, CredentialError> {
    if let Some(secret) = from_environment(provider) {
        return Ok(secret);
    }
    from_file(provider, &credentials_path(config_dir))
}

fn from_environment(provider: ProviderId) -> Option<Credential> {
    let raw = std::env::var(environment_variable(provider)).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(Credential::new(trimmed))
}

/// Reads one key out of the restricted file.
///
/// The format is deliberately the narrowest thing that could be called TOML:
/// `key = "value"` lines, `#` comments, blank lines. A real TOML parser would
/// be a dependency in the one process that talks to the internet, for a file
/// with three possible keys — and every escape sequence it understood would be
/// a way to write a byte into a header.
pub fn from_file(provider: ProviderId, path: &Path) -> Result<Credential, CredentialError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Err(CredentialError::Missing),
    };
    // Refused and not followed. Hashing a path's contents means nothing if the
    // path can point somewhere else between two runs, and the same is true of
    // trusting a path's *permissions*: a symlink's own mode is not the mode of
    // what it points at.
    if !metadata.is_file() {
        return Err(CredentialError::Insecure);
    }
    if metadata.uid() != nix_uid() {
        return Err(CredentialError::Insecure);
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(CredentialError::Insecure);
    }
    // A credentials file is three short lines. A ceiling here is what stops a
    // gigabyte at this path from becoming a gigabyte in this process.
    if metadata.len() > 64 * 1024 {
        return Err(CredentialError::Unreadable);
    }
    let text = fs::read_to_string(path).map_err(|_| CredentialError::Unreadable)?;
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
        if value.is_empty() {
            return Err(CredentialError::Missing);
        }
        // A header value may not carry a control character, and a key that
        // does is not a key — it is an attempt to write a second header.
        if value
            .bytes()
            .any(|byte| byte < 0x20 || byte == 0x7f || byte > 0x7e)
        {
            return Err(CredentialError::Unreadable);
        }
        return Ok(Credential::new(value));
    }
    Err(CredentialError::Missing)
}

/// This process's real user id.
fn nix_uid() -> u32 {
    // `/proc/self` is owned by the process's real user, which is the same
    // question `getuid()` answers without a `libc` dependency in this crate.
    fs::metadata("/proc/self")
        .map(|metadata| metadata.uid())
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_credentials(directory: &Path, body: &str, mode: u32) -> PathBuf {
        let path = credentials_path(directory);
        let mut file = fs::File::create(&path).expect("create");
        file.write_all(body.as_bytes()).expect("write");
        drop(file);
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("chmod");
        path
    }

    fn temporary() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "noteit-embed-cred-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");
        base
    }

    #[test]
    fn a_key_is_read_from_a_locked_down_file() {
        let directory = temporary();
        let path = write_credentials(
            &directory,
            "# um comentário\nopenai = \"sk-NOTEIT_TEST_SECRET_NEVER_EXPOSE_7F4A2\"\n",
            0o600,
        );
        let secret = from_file(ProviderId::OpenAi, &path).expect("read");
        assert_eq!(secret.expose(), "sk-NOTEIT_TEST_SECRET_NEVER_EXPOSE_7F4A2");
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn each_provider_gets_its_own_line() {
        let directory = temporary();
        let path = write_credentials(
            &directory,
            "openai = \"a-openai\"\ngemini = \"b-gemini\"\nvoyage = \"c-voyage\"\n",
            0o600,
        );
        assert_eq!(
            from_file(ProviderId::OpenAi, &path)
                .expect("openai")
                .expose(),
            "a-openai"
        );
        assert_eq!(
            from_file(ProviderId::Gemini, &path)
                .expect("gemini")
                .expose(),
            "b-gemini"
        );
        assert_eq!(
            from_file(ProviderId::Voyage, &path)
                .expect("voyage")
                .expose(),
            "c-voyage"
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_world_readable_file_is_refused_rather_than_warned_about() {
        let directory = temporary();
        for mode in [0o644, 0o640, 0o604, 0o666, 0o777, 0o601] {
            let path = write_credentials(&directory, "openai = \"secret\"\n", mode);
            assert_eq!(
                from_file(ProviderId::OpenAi, &path).unwrap_err(),
                CredentialError::Insecure,
                "mode {mode:o} was accepted"
            );
        }
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn owner_only_modes_are_accepted() {
        let directory = temporary();
        for mode in [0o600, 0o400] {
            let path = write_credentials(&directory, "openai = \"secret\"\n", mode);
            assert!(
                from_file(ProviderId::OpenAi, &path).is_ok(),
                "mode {mode:o} was refused"
            );
        }
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_symlink_is_refused_and_not_followed() {
        let directory = temporary();
        let real = directory.join("elsewhere.toml");
        fs::write(&real, "openai = \"secret\"\n").expect("write");
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).expect("chmod");
        let link = credentials_path(&directory);
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        assert_eq!(
            from_file(ProviderId::OpenAi, &link).unwrap_err(),
            CredentialError::Insecure
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_directory_at_the_path_is_refused() {
        let directory = temporary();
        fs::create_dir_all(credentials_path(&directory)).expect("mkdir");
        assert_eq!(
            from_file(ProviderId::OpenAi, &credentials_path(&directory)).unwrap_err(),
            CredentialError::Insecure
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn an_absent_file_is_missing_and_not_an_error() {
        let directory = temporary();
        assert_eq!(
            from_file(ProviderId::OpenAi, &credentials_path(&directory)).unwrap_err(),
            CredentialError::Missing
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_provider_with_no_line_is_missing() {
        let directory = temporary();
        let path = write_credentials(&directory, "openai = \"secret\"\n", 0o600);
        assert_eq!(
            from_file(ProviderId::Voyage, &path).unwrap_err(),
            CredentialError::Missing
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn an_empty_value_is_missing_rather_than_an_empty_key() {
        let directory = temporary();
        for body in ["openai = \"\"\n", "openai =   \n", "openai = \"   \"\n"] {
            let path = write_credentials(&directory, body, 0o600);
            assert_eq!(
                from_file(ProviderId::OpenAi, &path).unwrap_err(),
                CredentialError::Missing,
                "{body:?} produced a key"
            );
        }
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_value_carrying_a_control_character_is_refused() {
        // A newline or a carriage return in a header value is how a second
        // header gets written by somebody who only controls one.
        let directory = temporary();
        for body in [
            // Real control bytes, not the text `\u{7f}`: this parser reads no
            // escape sequences on purpose, so a test that wrote one would be
            // asserting about six printable characters.
            "openai = \"a\u{7}b\"\n",
            "openai = \"a\u{7f}b\"\n",
            "openai = \"a\rb\"\n",
            "openai = \"a\tb\"\n",
            // A byte above ASCII cannot go in a header value either.
            "openai = \"a\u{e7}\"\n",
        ] {
            let path = write_credentials(&directory, body, 0o600);
            assert_eq!(
                from_file(ProviderId::OpenAi, &path).unwrap_err(),
                CredentialError::Unreadable,
                "{body:?} produced a key"
            );
        }
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn an_enormous_file_is_refused_before_it_is_read() {
        let directory = temporary();
        let path = write_credentials(&directory, &"#".repeat(70 * 1024), 0o600);
        assert_eq!(
            from_file(ProviderId::OpenAi, &path).unwrap_err(),
            CredentialError::Unreadable
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn the_debug_of_a_credential_is_a_constant() {
        let secret = Credential::new("NOTEIT_TEST_SECRET_NEVER_EXPOSE_7F4A2");
        let printed = format!("{secret:?}");
        assert_eq!(printed, "Credential(<redacted>)");
        assert!(!printed.contains("7F4A2"));
    }

    #[test]
    fn the_documented_variable_names_are_the_vendors_own() {
        assert_eq!(environment_variable(ProviderId::OpenAi), "OPENAI_API_KEY");
        assert_eq!(environment_variable(ProviderId::Gemini), "GEMINI_API_KEY");
        assert_eq!(environment_variable(ProviderId::Voyage), "VOYAGE_API_KEY");
        for provider in ProviderId::ALL {
            assert!(CREDENTIAL_VARIABLES.contains(&environment_variable(provider)));
        }
    }
}
