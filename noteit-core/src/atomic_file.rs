use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Replaces `path` with `bytes` so a reader only ever sees the whole old file
/// or the whole new one.
///
/// **The commit point is the rename.** Everything before it — creating the temp
/// file, writing it, syncing it, and the rename itself — decides whether the
/// stored file changes at all. If any of that fails the target is untouched,
/// the temp file is removed, and this reports the failure; nothing was written
/// and no caller may believe otherwise.
///
/// Once the rename has returned, the target *is* the new file for every reader
/// from that moment on, and no later step can put the old one back. So this
/// returns `Ok` from the rename onwards. Syncing the parent directory is what
/// makes that rename survive a power loss, and it comes after the commit point:
/// a failure there means the write happened but may not be durable, which is
/// reported as a warning rather than as a failed write. Calling it a failure
/// would be worse than useless — the caller would roll back, or refuse to go
/// on, while the file on disk already holds the new content.
///
/// Nothing has to remember that a sync was missed. A directory sync flushes
/// every pending entry in that directory, so the next successful write of any
/// file in it makes the earlier rename durable too.
///
/// The parent directory must already exist. Creating it is left to the caller:
/// the store's directories are made once at startup, and a notes directory that
/// has since vanished is a fault to report rather than one to paper over.
///
/// `what` names the file in the messages, so a failure says which one it was.
pub fn write_atomic(path: &Path, bytes: &[u8], what: &str) -> Result<(), String> {
    write_atomic_inner(path, bytes, what, false)
}

/// Publishes a complete file only if its destination is absent. Unlike an
/// update, creation must never replace an existing name. The commit point is
/// `hard_link`, the same no-clobber mechanism used by trash restoration.
/// The temporary file is private, exclusive and on the destination filesystem.
/// There is no existence check and no rename fallback.
pub(crate) fn create_atomic(path: &Path, bytes: &[u8], what: &str) -> Result<(), String> {
    create_atomic_before_publish(path, bytes, what, || {})
}

fn create_atomic_before_publish(
    path: &Path,
    bytes: &[u8],
    what: &str,
    before_publish: impl FnOnce(),
) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| format!("{what}: no parent"))?;
    let temp = parent.join(format!(".tmp.create.{}", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(crate::permissions::PRIVATE_FILE_MODE);
    }
    let mut file = options
        .open(&temp)
        .map_err(|error| format!("Failed to prepare {what}: {error}"))?;
    let published = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| {
            before_publish();
            fs::hard_link(&temp, path)
        });
    drop(file);
    if let Err(error) = fs::remove_file(&temp) {
        eprintln!("Failed to remove creation temporary file: {error}");
    }
    published.map_err(|error| {
        format!("Failed to create {what} without replacing a destination: {error}")
    })?;
    sync_directory_after_commit(parent, what);
    Ok(())
}

/// Makes a directory entry that has already changed — a rename, a link, a
/// removal — durable.
///
/// Post-commit by nature, and reported the same way [`write_atomic`] reports
/// its own sync: as a warning, never as a failure. The change is already
/// visible to every reader, so calling this a failed operation would have the
/// caller roll back something the filesystem has already done.
pub fn sync_directory_after_commit(directory: &Path, what: &str) {
    if let Err(error) = sync_directory(directory, false) {
        eprintln!(
            "{what} was changed, but could not be synced, \
             so the change may not survive a power loss: {error}"
        );
    }
}

/// The same write, with the post-commit directory sync forced to fail.
///
/// That failure cannot be provoked from outside the process: once the rename
/// has returned, nothing a test can do to the filesystem reaches back into the
/// sync that follows it. Compiled out of every real build.
#[cfg(any(test, feature = "test-support"))]
pub fn write_atomic_with_failing_sync(path: &Path, bytes: &[u8], what: &str) -> Result<(), String> {
    write_atomic_inner(path, bytes, what, true)
}

fn write_atomic_inner(
    path: &Path,
    bytes: &[u8],
    what: &str,
    fail_directory_sync: bool,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Failed to write {what}: it has no parent directory"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("Failed to write {what}: its name is not valid UTF-8"))?;
    let temp_path = parent.join(format!(".tmp.{file_name}.{}", std::process::id()));

    // Before the commit point.
    if let Err(error) = write_and_rename(bytes, &temp_path, path, what) {
        // Best effort: if this cannot be removed either, the write has already
        // failed and the error worth reporting is that one.
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    // After it. The file on disk is already the new one.
    if let Err(error) = sync_directory(parent, fail_directory_sync) {
        eprintln!(
            "{what} was written, but its directory could not be synced, \
             so the change may not survive a power loss: {error}"
        );
    }

    Ok(())
}

/// Everything up to and including the commit point: on success the target file
/// is the new content, on failure it is untouched.
fn write_and_rename(
    bytes: &[u8],
    temp_path: &Path,
    target_path: &Path,
    what: &str,
) -> Result<(), String> {
    {
        let mut file = crate::permissions::create_private_file(temp_path).map_err(|e| {
            format!(
                "Failed to create the temp file for {what} at {}: {e}",
                temp_path.display()
            )
        })?;
        file.write_all(bytes)
            .map_err(|e| format!("Failed to write the temp file for {what}: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync the temp file for {what}: {e}"))?;
    }

    fs::rename(temp_path, target_path)
        .map_err(|e| format!("Failed to atomically replace {what}: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(
            target_path,
            fs::Permissions::from_mode(crate::permissions::PRIVATE_FILE_MODE),
        );
    }

    Ok(())
}

/// Makes a rename that already happened durable. Post-commit: see
/// [`write_atomic`] for why its failure is not a failed write.
fn sync_directory(directory: &Path, fail_directory_sync: bool) -> Result<(), String> {
    if fail_directory_sync {
        return Err("simulated directory sync failure".to_string());
    }

    File::open(directory)
        .and_then(|handle| handle.sync_all())
        .map_err(|e| format!("Failed to sync directory {}: {e}", directory.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creation_publishes_complete_content_and_cleans_temporary() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        create_atomic_before_publish(&path, b"complete note", "test", || {
            assert!(!path.exists(), "final name must not expose a partial write");
            let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
            assert_eq!(entries.len(), 1);
            let temp = entries[0].as_ref().unwrap().path();
            assert_eq!(temp.parent(), path.parent());
            assert_eq!(fs::read(temp).unwrap(), b"complete note");
        })
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"complete note");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn collision_at_the_publication_point_never_overwrites() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        // Force the collision after temp sync, immediately before hard_link.
        let result = create_atomic_before_publish(&path, b"loser", "test", || {
            fs::write(&path, b"winner").unwrap();
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"winner");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(create_atomic(&path, b"another loser", "test").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"winner");
    }

    #[test]
    fn concurrent_creations_have_one_winner_and_no_overwrite() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let one = scope.spawn(|| {
                create_atomic_before_publish(&path, b"one", "test", || {
                    barrier.wait();
                })
            });
            let two = scope.spawn(|| {
                create_atomic_before_publish(&path, b"two", "test", || {
                    barrier.wait();
                })
            });
            (one.join().unwrap(), two.join().unwrap())
        });
        assert_ne!(results.0.is_ok(), results.1.is_ok());
        let expected = if results.0.is_ok() { b"one" } else { b"two" };
        assert_eq!(fs::read(&path).unwrap(), expected);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    fn debris_in(directory: &Path) -> Vec<String> {
        fs::read_dir(directory)
            .expect("read the directory")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".tmp."))
            .collect()
    }

    #[test]
    fn a_write_replaces_the_file_and_leaves_no_debris() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("file.json");

        write_atomic(&path, b"first", "the file").expect("first write");
        assert_eq!(fs::read(&path).expect("read"), b"first");

        write_atomic(&path, b"second", "the file").expect("second write");
        assert_eq!(fs::read(&path).expect("read"), b"second");
        assert!(debris_in(path.parent().expect("parent")).is_empty());
    }

    #[test]
    fn a_write_that_cannot_be_completed_leaves_the_old_file_alone() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("file.json");
        write_atomic(&path, b"original", "the file").expect("seed the file");

        // A directory sitting where the file belongs: the temp file is written
        // and the rename onto it fails. That is path resolution rather than a
        // permission bit, so it fails for every user, root included.
        let blocked = tmp.path().join("blocked.json");
        fs::create_dir(&blocked).expect("occupy the target path");

        write_atomic(&blocked, b"new", "the blocked file")
            .expect_err("renaming a file over a directory must fail");

        assert_eq!(fs::read(&path).expect("read"), b"original");
        assert!(
            debris_in(tmp.path()).is_empty(),
            "a failed write left temp files behind: {:?}",
            debris_in(tmp.path())
        );
    }

    #[test]
    fn a_directory_sync_that_fails_after_the_rename_is_still_a_completed_write() {
        // 3.5R. The one failure that happens *past* the commit point. The
        // rename already replaced the file, so the write did happen and only
        // its durability is in doubt. Reporting it as a failed write would
        // leave the caller rolling back, or refusing to go on, over a file the
        // disk already holds.
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("file.json");
        write_atomic(&path, b"before", "the file").expect("seed the file");

        write_atomic_with_failing_sync(&path, b"after", "the file")
            .expect("a rename that succeeded is a write, whatever the sync did");

        assert_eq!(fs::read(&path).expect("read"), b"after");
        assert!(debris_in(tmp.path()).is_empty());
    }
}
