//! Private filesystem permission management for Note-it stores.
//!
//! Note-it data belongs privately to the user. To prevent accidental disclosure
//! across users on shared workstations or under permissive umasks (e.g. `0000` or `0022`),
//! private directories and files are explicitly created with restricted permissions:
//! - Directories: `0700` (`rwx------`)
//! - Regular files: `0600` (`rw-------`)
//!
//! Permissions are enforced at the moment of creation and reaffirmed post-creation.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

pub const PRIVATE_DIR_MODE: u32 = 0o700;
pub const PRIVATE_FILE_MODE: u32 = 0o600;

/// Creates a directory and any missing parents with private permissions (`0700`).
///
/// If `path` already exists, this function leaves its existing permissions alone
/// to respect historical files without performing mass silent `chmod`s.
/// When newly creating directories, all newly created levels in the hierarchy
/// receive `0700`.
pub fn create_private_dir_all(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        return Ok(());
    }
    if path.exists() {
        return Err(format!(
            "Path {} exists but is not a directory",
            path.display()
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        use std::os::unix::fs::PermissionsExt;

        // Identify missing ancestor directories before creation
        let mut missing = Vec::new();
        let mut curr = Some(path);
        while let Some(p) = curr {
            if !p.exists() {
                missing.push(p.to_path_buf());
                curr = p.parent();
            } else {
                break;
            }
        }

        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        builder.mode(PRIVATE_DIR_MODE);
        builder
            .create(path)
            .map_err(|e| format!("Failed to create private directory {}: {e}", path.display()))?;

        // Enforce 0700 on all newly created levels
        for dir in missing {
            if let Ok(meta) = fs::symlink_metadata(&dir) {
                if meta.is_dir() && !meta.file_type().is_symlink() {
                    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(PRIVATE_DIR_MODE));
                }
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)
            .map_err(|e| format!("Failed to create private directory {}: {e}", path.display()))
    }
}

/// Creates or opens a private file (`0600`) for writing.
///
/// On Unix, `OpenOptions::mode(0600)` ensures the file is created with restricted
/// permissions from the very first instant, avoiding any world-readable window
/// between creation and subsequent permission calls under permissive umasks.
pub fn create_private_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::fs::PermissionsExt;

        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(PRIVATE_FILE_MODE)
            .open(path)?;

        let _ = file.set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE));
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        File::create(path)
    }
}

/// Atomically writes bytes to a private file with mode `0600`.
pub fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = create_private_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Copies a regular file to a destination, guaranteeing the destination file is
/// born private (`0600`) regardless of the source file's permissions or ambient umask.
///
/// Refuses symbolic links and non-regular files (fail-closed).
/// Does NOT mutate the source file's permissions or metadata.
pub fn copy_private_file(source: &Path, destination: &Path) -> Result<u64, String> {
    let meta = fs::symlink_metadata(source)
        .map_err(|e| format!("Failed to inspect source file {}: {e}", source.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "Refusing to copy symbolic link {}: backups do not follow links",
            source.display()
        ));
    }
    if !meta.file_type().is_file() {
        return Err(format!(
            "Refusing to copy non-regular file {}",
            source.display()
        ));
    }

    let mut reader = File::open(source)
        .map_err(|e| format!("Failed to open source file {}: {e}", source.display()))?;
    let mut writer = create_private_file(destination).map_err(|e| {
        format!(
            "Failed to create private destination file {}: {e}",
            destination.display()
        )
    })?;

    let bytes_copied = io::copy(&mut reader, &mut writer).map_err(|e| {
        format!(
            "Failed to copy bytes from {} to {}: {e}",
            source.display(),
            destination.display()
        )
    })?;
    writer.sync_all().map_err(|e| {
        format!(
            "Failed to sync destination file {}: {e}",
            destination.display()
        )
    })?;

    Ok(bytes_copied)
}
