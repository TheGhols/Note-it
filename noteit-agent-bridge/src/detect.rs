//! Which AI clients this machine actually has.
//!
//! Nothing is downloaded and nothing is installed. The AI client belongs to
//! the person, not to Note-it (§33 of the phase); all this does is look.

use crate::event::ProviderId;
use std::path::{Path, PathBuf};

/// An AI client that is present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub provider: ProviderId,
    pub path: PathBuf,
}

/// The providers with adapters, in the order an interface should offer them.
pub const SUPPORTED: &[ProviderId] = &[ProviderId::Claude, ProviderId::Gemini, ProviderId::Codex];

/// Every supported client found on `PATH`.
///
/// An interface must offer only what this returns: a provider without an
/// adapter, or with an adapter but no client, is one that cannot work, and
/// offering it would be offering a failure.
pub fn available() -> Vec<Installed> {
    available_in(std::env::var_os("PATH").as_deref().map(Path::new))
}

/// The same, against a `PATH` a test supplies.
pub fn available_in(path: Option<&Path>) -> Vec<Installed> {
    let Some(path) = path else {
        return Vec::new();
    };
    SUPPORTED
        .iter()
        .filter_map(|provider| {
            find(path, provider.program()).map(|path| Installed {
                provider: *provider,
                path,
            })
        })
        .collect()
}

/// The first executable called `program` in a `PATH`-shaped string.
fn find(path: &Path, program: &str) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|directory| directory.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(candidate: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    candidate
        .metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_path_finds_nothing_rather_than_guessing() {
        assert!(available_in(None).is_empty());
    }

    #[test]
    fn a_directory_named_like_a_client_is_not_a_client() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("claude")).unwrap();
        assert!(available_in(Some(root.path())).is_empty());
    }

    #[test]
    fn a_file_that_is_not_executable_is_not_a_client() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("gemini"), "#!/bin/sh\n").unwrap();
        assert!(available_in(Some(root.path())).is_empty());
    }
}
