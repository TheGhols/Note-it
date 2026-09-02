//! Quarantine and preservation mechanism for corrupted store files.
//!
//! When a configuration, state, or other managed store file cannot be parsed or
//! is partially corrupted, Note-it must never treat it as missing/empty and must
//! never silently overwrite it with default values.
//!
//! Instead, the exact original bytes are preserved in an identifiable quarantine file
//! beside the original (e.g. `<filename>.corrupted.<timestamp>`).
//! Quarantine files are strictly private (`0600`).
//! If preservation fails, fail-safe behavior forbids overwriting the original file.

use std::fs;
use std::path::{Path, PathBuf};

/// Preserves a corrupted or unparseable file by saving its exact original bytes to a
/// dedicated quarantine file beside the original.
///
/// Returns the path to the quarantine file upon success.
/// The quarantine file is created with private permissions (`0600`).
pub fn quarantine_corrupted_file(path: &Path, raw_bytes: &[u8]) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Path has no parent: {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Invalid file name: {}", path.display()))?;

    let now = chrono::Utc::now();
    let timestamp_str = now.format("%Y%m%dT%H%M%SZ").to_string();
    let mut candidate = parent.join(format!("{file_name}.corrupted.{timestamp_str}"));
    let mut counter = 1;
    while candidate.exists() {
        candidate = parent.join(format!("{file_name}.corrupted.{timestamp_str}.{counter}"));
        counter += 1;
    }

    crate::permissions::write_private_file(&candidate, raw_bytes)
        .map_err(|e| format!("Failed to write quarantine file at {}: {e}", candidate.display()))?;

    // Verify written bytes match original bytes byte-for-byte
    let written = fs::read(&candidate)
        .map_err(|e| format!("Failed to re-read quarantine file at {}: {e}", candidate.display()))?;
    if written != raw_bytes {
        let _ = fs::remove_file(&candidate);
        return Err(format!(
            "Quarantine verification failed: bytes did not match for {}",
            candidate.display()
        ));
    }

    Ok(candidate)
}
