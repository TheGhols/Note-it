//! Where the worker listens, and everything that must be true of that place
//! before it does.
//!
//! §46 is a list of things a socket path can already be when a process
//! goes to bind it — a live socket, a regular file, a symlink, a directory, a
//! stale socket from a worker that died, a directory somebody else can write
//! to — and each of them is a different answer. The rule here is the one the
//! rest of Note-it already uses for the store: refuse rather than repair, and
//! never follow a symlink to find out what is really there.
//!
//! ## The path
//!
//! `$XDG_RUNTIME_DIR/note-it/embed-<n>.sock`, and never inside the note store.
//! The runtime directory is the right place for a socket for the reason it
//! exists: it is per-user, mode `0700`, and cleared when the session ends, so a
//! stale socket cannot outlive a reboot and a second user cannot reach this
//! one. `notes/`, `trash/` and `backups/` are refused by
//! [`SocketPath::validate`] rather than by convention, because a socket inside
//! the store would be swept by a backup, counted by an integrity check and
//! confused with somebody's writing.

use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// The mode the socket is left with: this user, and nobody else.
pub const SOCKET_MODE: u32 = 0o600;

/// The mode the directory holding it must not exceed.
pub const DIRECTORY_MODE_MASK: u32 = 0o077;

/// Why a socket path cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketError {
    /// The parent directory does not exist, is not a directory, is not owned
    /// by this user, or is writable by somebody else.
    Directory,
    /// Something is already at the path and it is not a socket this worker may
    /// replace: a regular file, a directory, or a symlink.
    Occupied,
    /// A live worker is already listening there.
    InUse,
    /// The socket could not be created.
    Io,
}

impl std::fmt::Display for SocketError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Directory => "o diretório do socket não é seguro para este usuário",
            Self::Occupied => "o caminho do socket já está ocupado por outro objeto",
            Self::InUse => "outro worker já está escutando neste caminho",
            Self::Io => "o socket não pôde ser criado",
        })
    }
}

impl std::error::Error for SocketError {}

/// A path that has been checked, and the checks that were made.
#[derive(Debug)]
pub struct SocketPath(PathBuf);

impl SocketPath {
    /// Checks everything §46 asks about, and answers with a path or a
    /// reason.
    ///
    /// The order matters. The directory is judged first, because a path inside
    /// a directory anybody can write to is not made safe by anything found at
    /// the path itself — a race replaces it between the check and the bind.
    pub fn validate(path: &Path) -> Result<Self, SocketError> {
        let parent = path.parent().ok_or(SocketError::Directory)?;

        // Never inside the store. A rule about capability, not about tidiness:
        // a socket under `notes/` would be walked by the scanner, swept into a
        // backup and counted by an integrity check.
        for forbidden in ["notes", "trash", "backups"] {
            if parent
                .components()
                .any(|component| component.as_os_str().to_str() == Some(forbidden))
            {
                return Err(SocketError::Directory);
            }
        }

        let directory = fs::symlink_metadata(parent).map_err(|_| SocketError::Directory)?;
        if !directory.is_dir() {
            return Err(SocketError::Directory);
        }
        if directory.uid() != current_uid() {
            return Err(SocketError::Directory);
        }
        // Group- or world-writable is enough for somebody else to replace the
        // socket with their own and be talked to as if they were the worker.
        if directory.permissions().mode() & DIRECTORY_MODE_MASK & 0o022 != 0 {
            return Err(SocketError::Directory);
        }

        match fs::symlink_metadata(path) {
            Err(_) => Ok(Self(path.to_path_buf())),
            Ok(existing) => {
                // A symlink is refused and not followed, by the same rule the
                // artifact loader applies: what a name points at may change
                // between the check and the use.
                if existing.file_type().is_symlink() {
                    return Err(SocketError::Occupied);
                }
                if !existing.file_type().is_socket() {
                    // A regular file or a directory at this path is somebody
                    // else's object, and removing it would be this process
                    // deleting something it did not create.
                    return Err(SocketError::Occupied);
                }
                // A socket that nothing is listening on is the remains of a
                // worker that died, and it is safe to replace precisely
                // because connecting to it fails. A socket that answers is a
                // live worker and this one must not take its place.
                match UnixStream::connect(path) {
                    Ok(_) => Err(SocketError::InUse),
                    Err(_) => Ok(Self(path.to_path_buf())),
                }
            }
        }
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Binds, with the socket never visible at a wider mode than it ends at.
    ///
    /// The permissions are applied immediately after the bind. There is a
    /// window between the two in which the socket exists at the process umask,
    /// and it is closed by the directory check above rather than by wishing:
    /// the parent is `0700`-ish and owned by this user, so nobody else can
    /// reach the path during that window in the first place.
    pub fn bind(&self) -> Result<UnixListener, SocketError> {
        if fs::symlink_metadata(&self.0).is_ok() {
            // Only ever a stale socket: `validate` refused every other kind of
            // object, and refused a live one.
            fs::remove_file(&self.0).map_err(|_| SocketError::Io)?;
        }
        let listener = UnixListener::bind(&self.0).map_err(|_| SocketError::Io)?;
        fs::set_permissions(&self.0, fs::Permissions::from_mode(SOCKET_MODE))
            .map_err(|_| SocketError::Io)?;
        Ok(listener)
    }

    /// Removes the socket on the way out, so the next worker sees nothing
    /// rather than something stale.
    pub fn remove(&self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Whether the peer on this connection is this same user.
///
/// `SO_PEERCRED` would be the direct answer and needs `libc`; this crate has
/// none and is not gaining one for it. The property is obtained structurally
/// instead: the socket is mode `0600` inside a directory owned by this user
/// and not writable by anybody else, so a connection existing at all means the
/// peer could open that path. That is the same argument the store's own
/// authority socket rests on (ADR-045).
pub fn peer_is_trusted(_stream: &UnixStream) -> bool {
    true
}

fn current_uid() -> u32 {
    fs::metadata("/proc/self")
        .map(|metadata| metadata.uid())
        .unwrap_or(u32::MAX)
}

/// The directory a worker's socket belongs in.
pub fn runtime_directory() -> io::Result<PathBuf> {
    let base = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        // No runtime directory means no session-scoped, mode-0700, per-user
        // place to put a socket. Refused rather than falling back to `/tmp`,
        // which is shared: the worker not starting is a degraded search, and a
        // socket somebody else can reach is a different kind of problem.
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "XDG_RUNTIME_DIR is not set",
            ))
        }
    };
    Ok(base.join("note-it"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "noteit-embed-sock-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).expect("chmod");
        base
    }

    #[test]
    fn a_clean_path_in_a_private_directory_is_accepted() {
        let directory = temporary("clean");
        let path = directory.join("embed.sock");
        let validated = SocketPath::validate(&path).expect("validate");
        let listener = validated.bind().expect("bind");
        let mode = fs::symlink_metadata(&path)
            .expect("stat")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, SOCKET_MODE, "the socket is readable by somebody else");
        drop(listener);
        validated.remove();
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_regular_file_at_the_path_is_refused_rather_than_removed() {
        let directory = temporary("regular");
        let path = directory.join("embed.sock");
        fs::write(&path, b"nao sou um socket").expect("write");
        assert_eq!(
            SocketPath::validate(&path).unwrap_err(),
            SocketError::Occupied
        );
        // And it is still there: this process does not delete what it did not
        // make.
        assert!(path.is_file());
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_directory_at_the_path_is_refused() {
        let directory = temporary("dir");
        let path = directory.join("embed.sock");
        fs::create_dir(&path).expect("mkdir");
        assert_eq!(
            SocketPath::validate(&path).unwrap_err(),
            SocketError::Occupied
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_symlink_at_the_path_is_refused_and_not_followed() {
        let directory = temporary("symlink");
        let target = directory.join("real.sock");
        let listener = UnixListener::bind(&target).expect("bind");
        let path = directory.join("embed.sock");
        std::os::unix::fs::symlink(&target, &path).expect("symlink");
        assert_eq!(
            SocketPath::validate(&path).unwrap_err(),
            SocketError::Occupied
        );
        drop(listener);
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_stale_socket_is_replaced_and_a_live_one_is_not() {
        let directory = temporary("stale");
        let path = directory.join("embed.sock");

        // Live: something is listening, so a second worker must refuse.
        let first = UnixListener::bind(&path).expect("bind");
        assert_eq!(SocketPath::validate(&path).unwrap_err(), SocketError::InUse);

        // Stale: the listener is gone but the inode remains, which is exactly
        // what a crashed worker leaves behind.
        drop(first);
        let validated = SocketPath::validate(&path).expect("a stale socket may be replaced");
        let second = validated.bind().expect("bind");
        drop(second);
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_world_writable_directory_is_refused() {
        let directory = temporary("wide");
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o777)).expect("chmod");
        assert_eq!(
            SocketPath::validate(&directory.join("embed.sock")).unwrap_err(),
            SocketError::Directory
        );
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).expect("chmod");
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_missing_directory_is_refused() {
        let directory = temporary("missing");
        let path = directory.join("gone").join("embed.sock");
        assert_eq!(
            SocketPath::validate(&path).unwrap_err(),
            SocketError::Directory
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_path_inside_the_note_store_is_refused() {
        let directory = temporary("store");
        for forbidden in ["notes", "trash", "backups"] {
            let inside = directory.join(forbidden);
            fs::create_dir_all(&inside).expect("mkdir");
            fs::set_permissions(&inside, fs::Permissions::from_mode(0o700)).expect("chmod");
            assert_eq!(
                SocketPath::validate(&inside.join("embed.sock")).unwrap_err(),
                SocketError::Directory,
                "a socket was allowed inside {forbidden}/"
            );
        }
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn removing_leaves_nothing_stale_behind() {
        let directory = temporary("remove");
        let path = directory.join("embed.sock");
        let validated = SocketPath::validate(&path).expect("validate");
        let listener = validated.bind().expect("bind");
        drop(listener);
        validated.remove();
        assert!(fs::symlink_metadata(&path).is_err());
        fs::remove_dir_all(&directory).ok();
    }
}
