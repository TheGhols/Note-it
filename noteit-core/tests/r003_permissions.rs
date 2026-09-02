//! Adversarial tests for R-003: Private permissions independent of umask.
//!
//! Verifies that Note-it directories and files are born private (`0700` and `0600`)
//! even when executed under an adversarial, wide-open process umask (e.g. `0000`).

use noteit_core::model::NoteDocument;
use noteit_core::permissions::{PRIVATE_DIR_MODE, PRIVATE_FILE_MODE};
use noteit_core::settings::AppConfig;
use noteit_core::state::AppState;
use noteit_core::{NoteItCore, StorageManager, StorePaths};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::tempdir;

extern "C" {
    fn umask(mask: u32) -> u32;
}

struct UmaskGuard(u32);

impl UmaskGuard {
    fn set(mode: u32) -> Self {
        let old = unsafe { umask(mode) };
        Self(old)
    }
}

impl Drop for UmaskGuard {
    fn drop(&mut self) {
        unsafe { umask(self.0) };
    }
}

fn mode_of(path: &Path) -> u32 {
    fs::symlink_metadata(path)
        .unwrap_or_else(|e| panic!("failed to get metadata for {}: {e}", path.display()))
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn r003_directories_and_files_are_private_under_permissive_umask() {
    // Set umask to 0000 (wide open: default files would be 0666, dirs 0777)
    let _guard = UmaskGuard::set(0o000);

    let tmp = tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();

    let notes_dir = root.join("data/note-it/notes");
    let config_dir = root.join("config/note-it");
    let state_dir = root.join("state/note-it");
    let runtime_dir = root.join("runtime/note-it");

    let paths = StorePaths::from_custom_paths(
        notes_dir.clone(),
        config_dir.clone(),
        state_dir.clone(),
        runtime_dir.clone(),
    );

    let storage = StorageManager::from_paths(paths).expect("open storage");
    let core = NoteItCore::from_storage(storage);

    // 1. Directory creation under umask 0000
    core.storage().ensure_directories().expect("ensure dirs");

    assert_eq!(mode_of(&core.paths().data_dir), PRIVATE_DIR_MODE, "data_dir must be 0700");
    assert_eq!(mode_of(&core.paths().notes_dir), PRIVATE_DIR_MODE, "notes_dir must be 0700");
    assert_eq!(mode_of(&core.paths().trash_dir), PRIVATE_DIR_MODE, "trash_dir must be 0700");
    assert_eq!(mode_of(&core.paths().backups_dir), PRIVATE_DIR_MODE, "backups_dir must be 0700");
    assert_eq!(mode_of(&core.paths().assets_dir), PRIVATE_DIR_MODE, "assets_dir must be 0700");
    assert_eq!(mode_of(&core.paths().config_dir), PRIVATE_DIR_MODE, "config_dir must be 0700");
    assert_eq!(mode_of(&core.paths().state_dir), PRIVATE_DIR_MODE, "state_dir must be 0700");
    assert_eq!(mode_of(&core.paths().runtime_dir), PRIVATE_DIR_MODE, "runtime_dir must be 0700");

    // Assert zero group/other permissions on directories
    for dir in [
        &core.paths().data_dir,
        &core.paths().notes_dir,
        &core.paths().trash_dir,
        &core.paths().backups_dir,
        &core.paths().assets_dir,
        &core.paths().config_dir,
        &core.paths().state_dir,
        &core.paths().runtime_dir,
    ] {
        assert_eq!(mode_of(dir) & 0o077, 0, "Dir {} has leaked permissions: {:o}", dir.display(), mode_of(dir));
    }

    // 2. Note file creation under umask 0000
    let mut doc = NoteDocument::new_empty();
    doc.content = "Nota privada criada sob umask 000".into();
    core.storage().save_note_atomic(&doc).expect("save note");

    let note_path = core.storage().note_path(&doc.metadata.id);
    assert_eq!(mode_of(&note_path), PRIVATE_FILE_MODE, "Note file must be 0600");
    assert_eq!(mode_of(&note_path) & 0o077, 0, "Note file must not have group/other permissions");

    // 3. Config file creation under umask 0000
    let config_path = core.storage().config_file_path();
    let config = AppConfig::default();
    config.save_to_file(&config_path).expect("save config");

    assert_eq!(mode_of(&config_path), PRIVATE_FILE_MODE, "Config file must be 0600");
    assert_eq!(mode_of(&config_path) & 0o077, 0, "Config file must not have group/other permissions");

    // 4. State file creation under umask 0000
    let state_path = core.storage().state_file_path();
    let state = AppState::default();
    state.save_to_file(&state_path).expect("save state");

    assert_eq!(mode_of(&state_path), PRIVATE_FILE_MODE, "State file must be 0600");
    assert_eq!(mode_of(&state_path) & 0o077, 0, "State file must not have group/other permissions");

    // 5. Backup snapshot creation under umask 0000
    let snapshot_dir = core.storage().create_backup_now().expect("create snapshot");
    assert_eq!(mode_of(&snapshot_dir), PRIVATE_DIR_MODE, "Snapshot dir must be 0700");
    assert_eq!(mode_of(&snapshot_dir) & 0o077, 0);

    let manifest_path = snapshot_dir.join("manifest.json");
    assert_eq!(mode_of(&manifest_path), PRIVATE_FILE_MODE, "Manifest must be 0600");
    assert_eq!(mode_of(&manifest_path) & 0o077, 0);

    let backed_note = snapshot_dir.join(format!("notes/{}.md", doc.metadata.id));
    assert_eq!(mode_of(&backed_note), PRIVATE_FILE_MODE, "Backed note must be 0600");
}

#[test]
fn r003_atomic_temp_file_is_created_private_immediately() {
    let _guard = UmaskGuard::set(0o000);

    let tmp = tempdir().expect("tempdir");
    let temp_file_path = tmp.path().join(".tmp.test_immediate_privacy");

    // Create file using noteit-core private file creation
    let file = noteit_core::permissions::create_private_file(&temp_file_path)
        .expect("create private file");

    let meta = file.metadata().expect("metadata");
    let mode = meta.permissions().mode() & 0o777;

    assert_eq!(
        mode, PRIVATE_FILE_MODE,
        "Temporary atomic file must be 0600 IMMEDIATELY upon creation, found {:o}",
        mode
    );
    assert_eq!(mode & 0o077, 0, "Temp file must never be world or group readable");
}
