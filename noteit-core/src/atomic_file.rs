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
        let mut file = File::create(temp_path).map_err(|e| {
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
        .map_err(|e| format!("Failed to atomically replace {what}: {e}"))
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
