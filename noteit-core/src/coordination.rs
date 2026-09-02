//! Exactly one Note-it writer per store.
//!
//! An atomic write keeps a *file* whole. It does nothing at all about two
//! processes that each read a note, each change their own copy, and each write
//! it back: both writes succeed, both files are intact, and one person's
//! edit is gone. Nothing in the storage layer can see that happen, because
//! from where it stands both writes were correct.
//!
//! So the exclusion has to live above the file. This module is the whole of
//! it, and both adapters use this one implementation:
//!
//! - the desktop application takes the lease at startup and holds it for as
//!   long as it can save anything at all;
//! - the CLI tries to take it for the length of one command, and when it
//!   cannot, it does not write — it asks whoever holds it to write instead.
//!
//! **The lease is an advisory lock on a file, taken with `flock`, not the
//! existence of a file.** That distinction is the reason this works:
//!
//! - a lock file left behind by a process that crashed is just a file, and the
//!   next writer takes the lease immediately;
//! - a process that dies, is killed, or panics releases the lease at once,
//!   because the kernel closes its descriptors;
//! - no PID is trusted, no timestamp is compared, and no staleness is guessed.
//!
//! ## Where it lives
//!
//! In the runtime directory, never in the store. A lock and a socket describe
//! *this boot* — they are meaningless after a restart, must never be backed
//! up, and have no business sitting next to the notes. `$XDG_RUNTIME_DIR` is
//! exactly the directory the specification defines for them.
//!
//! One store gets one coordination directory, named after the store:
//!
//! ```text
//! $XDG_RUNTIME_DIR/note-it/<store key>/
//!     store           the notes directory this key stands for
//!     writer.lock     the lease
//!     control.sock    the authority's private socket
//! ```
//!
//! Keying by store is not decoration. A test store and the real store are two
//! different stores with two different writers, and both are legitimate at the
//! same time; sharing one lease between them would have an isolated test
//! deadlock against the application the user is actually using. The key is the
//! [`crate::hashing`] digest of the notes directory path, so it is the same in
//! every process without anything having to be agreed on at run time.
//!
//! ## What is checked before anything is created
//!
//! The runtime directory is the user's own and is supposed to be private, but
//! "supposed to be" is not a guarantee this module is willing to inherit:
//!
//! - neither directory may be a symlink, so nothing can redirect the lease or
//!   the socket out of the runtime tree;
//! - both must be owned by this user;
//! - both are created `0700` and tightened to `0700` if they were looser;
//! - the socket is `0600` on top of a `0700` directory, so the directory
//!   already refuses everyone else before the socket's own mode is reached.
//!
//! Any of those failing is a refusal, never a warning: coordination that
//! cannot be trusted is worse than no coordination, because the caller would
//! write believing it was alone.

use crate::hashing::fnv1a_64_hex;
use crate::storage::StorePaths;
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Protocol timeout budget hierarchy constants (R-011).
///
/// Note-it coordinates write mutations between client processes (such as the CLI)
/// and running server/daemon processes (such as the Desktop application) across
/// local domain sockets, event queues, and WebView editor boundaries.
///
/// To prevent deadlocks, race conditions, and hung processes, the timeout budget
/// obeys a strict, mathematically ordered causal hierarchy:
///
/// 1. `PROTOCOL_FREEZE_TIMEOUT` (4s): Maximum duration allowed for an open WebView
///    editor to flush pending inputs and freeze its document state.
/// 2. `PROTOCOL_ACK_TIMEOUT` (4s): Maximum duration allowed for an open note window
///    to acknowledge that an external mutation was applied.
/// 3. `PROTOCOL_CLI_AUTHORITY_TIMEOUT` (15s): Total duration the CLI client will wait
///    for the running write authority to process, commit, and respond to a request.
/// 4. `PROTOCOL_DESKTOP_WORKER_TIMEOUT` (30s): Upper bound for asynchronous worker
///    thread reply dispatch within the desktop authority.
///
/// Invariant:
/// PROTOCOL_FREEZE_TIMEOUT <= PROTOCOL_ACK_TIMEOUT < PROTOCOL_CLI_AUTHORITY_TIMEOUT < PROTOCOL_DESKTOP_WORKER_TIMEOUT
pub const PROTOCOL_FREEZE_TIMEOUT: Duration = Duration::from_millis(4000);
pub const PROTOCOL_ACK_TIMEOUT: Duration = Duration::from_millis(4000);
pub const PROTOCOL_CLI_AUTHORITY_TIMEOUT: Duration = Duration::from_secs(15);
pub const PROTOCOL_DESKTOP_WORKER_TIMEOUT: Duration = Duration::from_secs(30);

const MAX_SYMLINK_DEPTH: usize = 40;

const ERR_ELOOP: i32 = 40;

fn is_filesystem_loop(error: &io::Error) -> bool {
    error.raw_os_error() == Some(ERR_ELOOP)
}

/// Resolves a store directory path to its canonical physical path.
///
/// If the directory exists on disk, standard canonicalization (`realpath`) is used,
/// resolving any symlinks, redundant slashes, and `.` or `..` segments.
///
/// If the directory does not exist yet (e.g. before initial directory creation), its
/// existing ancestors are canonicalized and trailing path segments are normalized with
/// symlink traversal and loop detection.
pub fn canonicalize_store_directory(path: &Path) -> Result<PathBuf, CoordinationError> {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                CoordinationError::Unavailable(format!(
                    "could not determine current directory for store resolution: {error}"
                ))
            })?
            .join(path)
    };

    match fs::canonicalize(&abs_path) {
        Ok(canonical) => Ok(canonical),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut visited = HashSet::new();
            resolve_path_with_symlinks(&abs_path, &mut visited, 0)
        }
        Err(error) if is_filesystem_loop(&error) => Err(CoordinationError::Unsafe(format!(
            "symlink loop detected while resolving store directory {}: {error}",
            abs_path.display()
        ))),
        Err(error) => Err(CoordinationError::Unavailable(format!(
            "could not resolve store directory {}: {error}",
            abs_path.display()
        ))),
    }
}

fn resolve_path_with_symlinks(
    path: &Path,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<PathBuf, CoordinationError> {
    if depth > MAX_SYMLINK_DEPTH {
        return Err(CoordinationError::Unsafe(format!(
            "too many levels of symbolic links while resolving {}",
            path.display()
        )));
    }

    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                current.push(prefix.as_os_str());
            }
            Component::RootDir => {
                current.push(Component::RootDir.as_os_str());
            }
            Component::CurDir => {
                // Ignore `.`
            }
            Component::ParentDir => {
                // `..` - pop from current
                current.pop();
            }
            Component::Normal(name) => {
                let candidate = current.join(name);
                match fs::symlink_metadata(&candidate) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        let target = fs::read_link(&candidate).map_err(|error| {
                            CoordinationError::Unavailable(format!(
                                "could not read symbolic link {}: {error}",
                                candidate.display()
                            ))
                        })?;
                        let target_path = if target.is_absolute() {
                            target
                        } else {
                            current.join(target)
                        };
                        if let Ok(c) = fs::canonicalize(&target_path) {
                            current = c;
                        } else {
                            if !visited.insert(candidate.clone()) {
                                return Err(CoordinationError::Unsafe(format!(
                                    "symlink loop detected at {}",
                                    candidate.display()
                                )));
                            }
                            current = resolve_path_with_symlinks(&target_path, visited, depth + 1)?;
                        }
                    }
                    Ok(_) => {
                        if let Ok(c) = fs::canonicalize(&candidate) {
                            current = c;
                        } else {
                            current = candidate;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        current = candidate;
                    }
                    Err(error) if is_filesystem_loop(&error) => {
                        return Err(CoordinationError::Unsafe(format!(
                            "symlink loop detected at {}: {error}",
                            candidate.display()
                        )));
                    }
                    Err(error) => {
                        return Err(CoordinationError::Unavailable(format!(
                            "could not inspect path {}: {error}",
                            candidate.display()
                        )));
                    }
                }
            }
        }
    }
    Ok(current)
}

/// The lock file inside a store's coordination directory.
pub const LOCK_FILE_NAME: &str = "writer.lock";
/// The authority's socket inside a store's coordination directory.
pub const SOCKET_FILE_NAME: &str = "control.sock";
/// Records which store a coordination directory belongs to.
///
/// Written for two reasons. It makes a runtime directory readable by a person
/// instead of being a wall of digests, and it lets the isolated test harness
/// find and remove exactly the directory belonging to its throwaway store, so
/// a test leaves nothing behind in the real runtime tree.
pub const STORE_MARKER_FILE_NAME: &str = "store";

const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;

/// Why coordination could not be established.
///
/// Distinct from every other failure in the system because it means nothing
/// was attempted: no note was read, nothing was written, and the caller may
/// say so without qualification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinationError {
    /// The runtime directory could not be created or reached.
    Unavailable(String),
    /// Something in the runtime tree is not what it must be: a symlink where a
    /// directory belongs, another user's directory, a lock path that is not a
    /// regular file.
    Unsafe(String),
}

impl fmt::Display for CoordinationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(detail) => write!(formatter, "{detail}"),
            Self::Unsafe(detail) => write!(formatter, "{detail}"),
        }
    }
}

impl std::error::Error for CoordinationError {}

/// Where one store's writer lease and control socket live.
///
/// Resolving canonical store paths ensures that all equivalent filesystem
/// representations (symlinks, `.` segments, `..` traversals, redundant separators)
/// share the exact same coordination directory, lease, and authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteCoordinationPaths {
    runtime_root: PathBuf,
    store_dir: PathBuf,
    store_key: String,
    notes_dir: PathBuf,
}

impl WriteCoordinationPaths {
    /// The coordination paths for a store.
    pub fn for_store(paths: &StorePaths) -> Result<Self, CoordinationError> {
        Self::for_parts(&paths.runtime_dir, &paths.notes_dir)
    }

    /// The same, from the two directories that decide it.
    pub fn for_parts(runtime_root: &Path, notes_dir: &Path) -> Result<Self, CoordinationError> {
        let canonical_notes_dir = canonicalize_store_directory(notes_dir)?;
        let store_key = fnv1a_64_hex(canonical_notes_dir.as_os_str().as_encoded_bytes());
        Ok(Self {
            store_dir: runtime_root.join(&store_key),
            runtime_root: runtime_root.to_path_buf(),
            store_key,
            notes_dir: canonical_notes_dir,
        })
    }

    /// The digest naming this store's coordination directory.
    pub fn store_key(&self) -> &str {
        &self.store_key
    }

    /// `$XDG_RUNTIME_DIR/note-it`, shared by every store on this session.
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    /// This store's own coordination directory.
    pub fn store_dir(&self) -> &Path {
        &self.store_dir
    }

    /// The file the lease is taken on.
    pub fn lock_path(&self) -> PathBuf {
        self.store_dir.join(LOCK_FILE_NAME)
    }

    /// The authority's private socket.
    pub fn socket_path(&self) -> PathBuf {
        self.store_dir.join(SOCKET_FILE_NAME)
    }

    /// The file naming the store this directory belongs to.
    pub fn store_marker_path(&self) -> PathBuf {
        self.store_dir.join(STORE_MARKER_FILE_NAME)
    }

    /// The canonical physical directory where notes live.
    pub fn notes_dir(&self) -> &Path {
        &self.notes_dir
    }

    /// Creates and validates the coordination directories.
    ///
    /// Called by every path that is about to take, or try to take, the lease.
    /// It is idempotent, and it refuses rather than repairs anything it does
    /// not understand.
    pub fn prepare(&self) -> Result<(), CoordinationError> {
        create_private_directory(&self.runtime_root)?;
        create_private_directory(&self.store_dir)?;
        create_private_directory(&self.notes_dir)?;

        // Written *inside* the directory that was just made or validated, so
        // its owner is this process by construction. That is what the two
        // directories are then compared against: it answers "who am I" without
        // a system call this crate would otherwise have to reach outside the
        // standard library for.
        let marker = self.store_marker_path();
        write_marker(&marker, &self.notes_dir)?;
        let owner = fs::metadata(&marker)
            .map_err(|error| {
                CoordinationError::Unavailable(format!(
                    "could not read {}: {error}",
                    marker.display()
                ))
            })?
            .uid();

        assert_owned_private_directory(&self.runtime_root, owner)?;
        assert_owned_private_directory(&self.store_dir, owner)?;
        Ok(())
    }
}

/// An exclusive claim on a store, held for as long as this value lives.
///
/// Dropping it releases the claim, and so does the process ending for any
/// reason at all. There is deliberately no way to release it early by name and
/// no way to break someone else's: a lease that could be forced would not be
/// a guarantee.
#[derive(Debug)]
pub struct WriterLease {
    file: File,
    path: PathBuf,
}

impl WriterLease {
    /// Takes the lease if it is free, and answers `None` at once if it is not.
    ///
    /// Never waits. A caller that is willing to wait says so by calling this
    /// again — see [`Self::acquire_within`] — so nothing can block on a lease
    /// forever by accident.
    pub fn try_acquire(paths: &WriteCoordinationPaths) -> Result<Option<Self>, CoordinationError> {
        paths.prepare()?;
        Self::try_acquire_prepared(paths)
    }

    /// The same, for a caller that has already prepared the directories.
    pub fn try_acquire_prepared(
        paths: &WriteCoordinationPaths,
    ) -> Result<Option<Self>, CoordinationError> {
        let path = paths.lock_path();
        assert_plain_file_or_absent(&path)?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(PRIVATE_FILE_MODE)
            .open(&path)
            .map_err(|error| {
                CoordinationError::Unavailable(format!(
                    "could not open the writer lock at {}: {error}",
                    path.display()
                ))
            })?;

        match file.try_lock() {
            Ok(()) => Ok(Some(Self { file, path })),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(error)) => Err(CoordinationError::Unavailable(format!(
                "could not test the writer lock at {}: {error}",
                path.display()
            ))),
        }
    }

    /// Takes the lease, retrying until `deadline` before giving up.
    ///
    /// The window exists for the two races that are real and short: a desktop
    /// instance that is starting and has not claimed the store yet, and
    /// another CLI command finishing one direct write. Both are milliseconds.
    /// It is a bounded wait and never a blocking one — answering "the store is
    /// busy" is a correct outcome, and hanging is not.
    pub fn acquire_within(
        paths: &WriteCoordinationPaths,
        deadline: std::time::Instant,
        poll: std::time::Duration,
    ) -> Result<Option<Self>, CoordinationError> {
        paths.prepare()?;
        loop {
            if let Some(lease) = Self::try_acquire_prepared(paths)? {
                return Ok(Some(lease));
            }
            if std::time::Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(poll);
        }
    }

    /// The file the lease is held on, so a holder can keep it alive explicitly.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WriterLease {
    fn drop(&mut self) {
        // Closing the descriptor releases the lock on its own; unlocking first
        // simply makes the release the visible thing that happens here rather
        // than a side effect of a field going out of scope.
        let _ = self.file.unlock();
    }
}

/// Creates a directory `0700`, or checks the one already there.
fn create_private_directory(path: &Path) -> Result<(), CoordinationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(CoordinationError::Unsafe(format!(
                    "the runtime path {} is a symbolic link; \
                     nothing was created and no lease was taken",
                    path.display()
                )));
            }
            if !metadata.is_dir() {
                return Err(CoordinationError::Unsafe(format!(
                    "the runtime path {} is not a directory",
                    path.display()
                )));
            }
            tighten_directory(path, &metadata)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            DirBuilder::new()
                .recursive(true)
                .mode(PRIVATE_DIRECTORY_MODE)
                .create(path)
                .map_err(|error| {
                    CoordinationError::Unavailable(format!(
                        "could not create the runtime directory {}: {error}",
                        path.display()
                    ))
                })?;
        }
        Err(error) => {
            return Err(CoordinationError::Unavailable(format!(
                "could not inspect the runtime directory {}: {error}",
                path.display()
            )));
        }
    }
    Ok(())
}

/// Narrows a directory that exists but is readable by more than its owner.
///
/// A runtime directory made by an older version — or by `create_dir_all` under
/// a permissive umask — is the user's own and holding it against them would
/// only mean refusing to run. Narrowing it is the repair that costs nothing
/// and closes the hole.
fn tighten_directory(path: &Path, metadata: &fs::Metadata) -> Result<(), CoordinationError> {
    let mode = metadata.permissions().mode() & 0o777;
    if mode == PRIVATE_DIRECTORY_MODE {
        return Ok(());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE)).map_err(|error| {
        CoordinationError::Unsafe(format!(
            "the runtime directory {} is mode {mode:04o} and could not be \
                 narrowed to 0700: {error}",
            path.display()
        ))
    })
}

fn assert_owned_private_directory(path: &Path, owner: u32) -> Result<(), CoordinationError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        CoordinationError::Unavailable(format!("could not inspect {}: {error}", path.display()))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CoordinationError::Unsafe(format!(
            "the runtime path {} is not a plain directory",
            path.display()
        )));
    }
    if metadata.uid() != owner {
        return Err(CoordinationError::Unsafe(format!(
            "the runtime directory {} belongs to another user; \
             no lease was taken and nothing was written",
            path.display()
        )));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(CoordinationError::Unsafe(format!(
            "the runtime directory {} is reachable by other users",
            path.display()
        )));
    }
    Ok(())
}

/// The lock path must be a regular file or nothing at all.
///
/// Opening it is what takes the lease, so a symlink here would let anything
/// that could write into the directory decide which file the lease is really
/// held on — and two processes locking two different files are not excluding
/// each other at all.
fn assert_plain_file_or_absent(path: &Path) -> Result<(), CoordinationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(CoordinationError::Unsafe(
            format!("the writer lock at {} is a symbolic link", path.display()),
        )),
        Ok(metadata) if !metadata.is_file() => Err(CoordinationError::Unsafe(format!(
            "the writer lock at {} is not a regular file",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CoordinationError::Unavailable(format!(
            "could not inspect the writer lock at {}: {error}",
            path.display()
        ))),
    }
}

fn write_marker(path: &Path, notes_dir: &Path) -> Result<(), CoordinationError> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(CoordinationError::Unsafe(format!(
                "the store marker at {} is a symbolic link",
                path.display()
            )));
        }
    }
    let mut contents = notes_dir.as_os_str().as_encoded_bytes().to_vec();
    contents.push(b'\n');
    crate::permissions::write_private_file(path, &contents).map_err(|error| {
        CoordinationError::Unavailable(format!(
            "could not record the store marker at {}: {error}",
            path.display()
        ))
    })
}

/// Narrows a socket that has just been bound.
///
/// The directory is already `0700`, so this is the second lock on the same
/// door rather than the only one — which is what makes the unavoidable gap
/// between `bind` and this call harmless.
pub fn narrow_socket_file(path: &Path) -> Result<(), CoordinationError> {
    fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_FILE_MODE)).map_err(|error| {
        CoordinationError::Unsafe(format!(
            "could not narrow the control socket at {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn paths_in(root: &Path) -> WriteCoordinationPaths {
        WriteCoordinationPaths::for_parts(&root.join("runtime"), &root.join("data/notes"))
            .expect("paths_in")
    }

    #[test]
    fn one_process_takes_the_lease_and_a_second_claim_is_refused() {
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());

        let first = WriterLease::try_acquire(&paths)
            .expect("prepare")
            .expect("the first claim must succeed");
        assert!(
            WriterLease::try_acquire(&paths).expect("prepare").is_none(),
            "a second writer took a lease that was already held"
        );
        drop(first);
    }

    #[test]
    fn releasing_the_lease_lets_the_next_writer_have_it() {
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());

        let first = WriterLease::try_acquire(&paths)
            .expect("prepare")
            .expect("first");
        drop(first);

        let second = WriterLease::try_acquire(&paths).expect("prepare");
        assert!(
            second.is_some(),
            "the released lease was not available again"
        );
    }

    #[test]
    fn a_lock_file_left_behind_is_not_a_held_lease() {
        // The whole reason this is a lock and not a file: a process that
        // crashed leaves the file exactly where it was, and the next writer
        // must not be shut out by it.
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());
        paths.prepare().expect("prepare");
        fs::write(paths.lock_path(), b"stale").expect("leave a lock file behind");

        let lease = WriterLease::try_acquire(&paths)
            .expect("prepare")
            .expect("a stale file must not keep a writer out");
        drop(lease);
    }

    #[test]
    fn a_lease_dropped_by_a_panicking_scope_is_released() {
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());

        let result = std::panic::catch_unwind(|| {
            let _lease = WriterLease::try_acquire(&paths)
                .expect("prepare")
                .expect("lease");
            panic!("the writer died holding the lease");
        });
        assert!(result.is_err());

        assert!(
            WriterLease::try_acquire(&paths).expect("prepare").is_some(),
            "a lease survived the death of the scope that held it"
        );
    }

    #[test]
    fn two_stores_never_share_one_lease() {
        // An isolated test store and the real store are two writers, both
        // legitimate at once. Sharing a lease would have one wait for the
        // other for no reason at all.
        let tmp = tempdir().expect("tempdir");
        let runtime = tmp.path().join("runtime");
        let first = WriteCoordinationPaths::for_parts(&runtime, &tmp.path().join("a/notes"))
            .expect("first");
        let second = WriteCoordinationPaths::for_parts(&runtime, &tmp.path().join("b/notes"))
            .expect("second");
        assert_ne!(first.store_key(), second.store_key());

        let held = WriterLease::try_acquire(&first)
            .expect("prepare")
            .expect("first");
        assert!(
            WriterLease::try_acquire(&second)
                .expect("prepare")
                .is_some(),
            "two different stores contended for one lease"
        );
        drop(held);
    }

    #[test]
    fn the_same_store_always_resolves_to_the_same_key() {
        let runtime = PathBuf::from("/run/user/1000/note-it");
        let notes = PathBuf::from("/home/someone/.local/share/note-it/notes");
        let first = WriteCoordinationPaths::for_parts(&runtime, &notes).expect("first");
        let second = WriteCoordinationPaths::for_parts(&runtime, &notes).expect("second");
        assert_eq!(first.store_key(), second.store_key());
        assert_eq!(first.lock_path(), second.lock_path());
        assert_eq!(first.socket_path(), second.socket_path());
    }

    #[test]
    fn r001_case_a_canonical_vs_dotted_shares_key_and_lease() {
        let tmp = tempdir().expect("tempdir");
        let runtime = tmp.path().join("runtime");
        let real_notes = tmp.path().join("notes");
        let dotted_notes = tmp.path().join("./notes");

        let plain = WriteCoordinationPaths::for_parts(&runtime, &real_notes).expect("plain");
        let dotted = WriteCoordinationPaths::for_parts(&runtime, &dotted_notes).expect("dotted");

        assert_eq!(
            plain.store_key(),
            dotted.store_key(),
            "Case A: canonical and dotted paths must yield the exact same store key"
        );
        assert_eq!(plain.lock_path(), dotted.lock_path());
        assert_eq!(plain.socket_path(), dotted.socket_path());

        // Test authority/lease exclusion: plain writer holds lease, dotted writer must be locked out
        let lease = WriterLease::try_acquire(&plain)
            .expect("prepare")
            .expect("acquire plain");
        let dotted_attempt = WriterLease::try_acquire(&dotted).expect("prepare");
        assert!(
            dotted_attempt.is_none(),
            "dotted alias must not acquire an independent lease on the same store"
        );
        drop(lease);
    }

    #[test]
    fn r001_case_b_canonical_vs_redundant_components_shares_key_and_lease() {
        let tmp = tempdir().expect("tempdir");
        let runtime = tmp.path().join("runtime");
        let real_notes = tmp.path().join("notes");
        let redundant_notes = tmp.path().join("./././notes");

        let plain = WriteCoordinationPaths::for_parts(&runtime, &real_notes).expect("plain");
        let redundant =
            WriteCoordinationPaths::for_parts(&runtime, &redundant_notes).expect("redundant");

        assert_eq!(
            plain.store_key(),
            redundant.store_key(),
            "Case B: canonical and redundant component paths must yield the same store key"
        );
        assert_eq!(plain.lock_path(), redundant.lock_path());
    }

    #[test]
    fn r001_case_c_canonical_vs_parent_traversal_shares_key_and_lease() {
        let tmp = tempdir().expect("tempdir");
        let runtime = tmp.path().join("runtime");
        let real_notes = tmp.path().join("notes");
        let up_notes = tmp.path().join("sub/../notes");

        let plain = WriteCoordinationPaths::for_parts(&runtime, &real_notes).expect("plain");
        let up = WriteCoordinationPaths::for_parts(&runtime, &up_notes).expect("up");

        assert_eq!(
            plain.store_key(),
            up.store_key(),
            "Case C: canonical and parent traversal paths must yield the same store key"
        );
        assert_eq!(plain.lock_path(), up.lock_path());

        let lease = WriterLease::try_acquire(&plain)
            .expect("prepare")
            .expect("acquire plain");
        assert!(
            WriterLease::try_acquire(&up).expect("prepare").is_none(),
            "parent traversal alias must not acquire an independent lease on the same store"
        );
        drop(lease);
    }

    #[test]
    fn r001_case_d_canonical_vs_symlink_shares_key_and_lease() {
        let tmp = tempdir().expect("tempdir");
        let runtime = tmp.path().join("runtime");
        let real_notes = tmp.path().join("real_store/notes");
        fs::create_dir_all(&real_notes).expect("create real store");

        let symlink_notes = tmp.path().join("symlink_notes");
        std::os::unix::fs::symlink(&real_notes, &symlink_notes).expect("create symlink");

        let plain = WriteCoordinationPaths::for_parts(&runtime, &real_notes).expect("plain");
        let linked = WriteCoordinationPaths::for_parts(&runtime, &symlink_notes).expect("linked");

        assert_eq!(
            plain.store_key(),
            linked.store_key(),
            "Case D: symlink pointing to store must yield the exact same store key"
        );
        assert_eq!(plain.lock_path(), linked.lock_path());
        assert_eq!(plain.socket_path(), linked.socket_path());

        // Plain writer acquires lease; linked writer must be locked out
        let lease = WriterLease::try_acquire(&plain)
            .expect("prepare")
            .expect("acquire plain");
        assert!(
            WriterLease::try_acquire(&linked)
                .expect("prepare")
                .is_none(),
            "symlink alias must not acquire an independent lease on the same store"
        );
        drop(lease);

        // Conversely, linked writer acquires lease; plain writer must be locked out
        let lease_linked = WriterLease::try_acquire(&linked)
            .expect("prepare")
            .expect("acquire linked");
        assert!(
            WriterLease::try_acquire(&plain).expect("prepare").is_none(),
            "plain path must not acquire an independent lease while symlink alias holds it"
        );
        drop(lease_linked);
    }

    #[test]
    fn symlink_loop_in_store_directory_is_refused() {
        let tmp = tempdir().expect("tempdir");
        let runtime = tmp.path().join("runtime");
        let loop1 = tmp.path().join("loop1");
        let loop2 = tmp.path().join("loop2");

        std::os::unix::fs::symlink(&loop2, &loop1).expect("symlink 1");
        std::os::unix::fs::symlink(&loop1, &loop2).expect("symlink 2");

        let err = WriteCoordinationPaths::for_parts(&runtime, &loop1)
            .expect_err("symlink loop must fail explicitly");
        assert!(
            matches!(err, CoordinationError::Unsafe(_)),
            "must be CoordinationError::Unsafe, got: {err:?}"
        );
    }

    #[test]
    fn resolving_paths_creates_nothing() {
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());
        assert!(!paths.runtime_root().exists());
        assert!(!paths.store_dir().exists());
        assert!(!paths.lock_path().exists());
    }

    #[test]
    fn preparing_makes_private_directories_and_records_the_store() {
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());
        paths.prepare().expect("prepare");

        for directory in [paths.runtime_root(), paths.store_dir()] {
            let mode = fs::metadata(directory)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700, "{} is not private", directory.display());
        }
        let marker = fs::read_to_string(paths.store_marker_path()).expect("marker");
        assert_eq!(marker.trim_end(), paths.notes_dir().to_string_lossy());
    }

    #[test]
    fn a_runtime_directory_left_world_readable_is_narrowed() {
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());
        fs::create_dir_all(paths.store_dir()).expect("create loosely");
        fs::set_permissions(paths.runtime_root(), fs::Permissions::from_mode(0o755))
            .expect("loosen");

        paths
            .prepare()
            .expect("prepare must narrow rather than refuse");
        let mode = fs::metadata(paths.runtime_root())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn a_symlinked_runtime_directory_is_refused() {
        let tmp = tempdir().expect("tempdir");
        let elsewhere = tmp.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).expect("elsewhere");
        let runtime = tmp.path().join("runtime");
        std::os::unix::fs::symlink(&elsewhere, &runtime).expect("symlink");

        let paths = WriteCoordinationPaths::for_parts(&runtime, &tmp.path().join("data/notes"))
            .expect("for_parts");
        let error = paths
            .prepare()
            .expect_err("a symlinked runtime must be refused");
        assert!(matches!(error, CoordinationError::Unsafe(_)), "{error:?}");
    }

    #[test]
    fn a_symlinked_lock_path_is_refused() {
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());
        paths.prepare().expect("prepare");
        let target = tmp.path().join("victim");
        fs::write(&target, b"").expect("victim");
        std::os::unix::fs::symlink(&target, paths.lock_path()).expect("symlink the lock");

        let error = WriterLease::try_acquire(&paths).expect_err("a symlinked lock must be refused");
        assert!(matches!(error, CoordinationError::Unsafe(_)), "{error:?}");
    }

    #[test]
    fn a_bounded_wait_gives_up_rather_than_hanging() {
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());
        let _held = WriterLease::try_acquire(&paths)
            .expect("prepare")
            .expect("lease");

        let started = std::time::Instant::now();
        let waited = WriterLease::acquire_within(
            &paths,
            started + std::time::Duration::from_millis(120),
            std::time::Duration::from_millis(10),
        )
        .expect("prepare");
        assert!(waited.is_none());
        assert!(started.elapsed() >= std::time::Duration::from_millis(100));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn a_waiting_writer_takes_the_lease_the_moment_it_is_released() {
        let tmp = tempdir().expect("tempdir");
        let paths = paths_in(tmp.path());
        let held = WriterLease::try_acquire(&paths)
            .expect("prepare")
            .expect("lease");

        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(60));
            drop(held);
        });

        let waited = WriterLease::acquire_within(
            &paths,
            std::time::Instant::now() + std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(10),
        )
        .expect("prepare");
        assert!(waited.is_some(), "the released lease was never handed on");
        releaser.join().expect("releaser");
    }
}
