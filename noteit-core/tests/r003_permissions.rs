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

    assert_eq!(
        mode_of(&core.paths().data_dir),
        PRIVATE_DIR_MODE,
        "data_dir must be 0700"
    );
    assert_eq!(
        mode_of(&core.paths().notes_dir),
        PRIVATE_DIR_MODE,
        "notes_dir must be 0700"
    );
    assert_eq!(
        mode_of(&core.paths().trash_dir),
        PRIVATE_DIR_MODE,
        "trash_dir must be 0700"
    );
    assert_eq!(
        mode_of(&core.paths().backups_dir),
        PRIVATE_DIR_MODE,
        "backups_dir must be 0700"
    );
    assert_eq!(
        mode_of(&core.paths().assets_dir),
        PRIVATE_DIR_MODE,
        "assets_dir must be 0700"
    );
    assert_eq!(
        mode_of(&core.paths().config_dir),
        PRIVATE_DIR_MODE,
        "config_dir must be 0700"
    );
    assert_eq!(
        mode_of(&core.paths().state_dir),
        PRIVATE_DIR_MODE,
        "state_dir must be 0700"
    );
    assert_eq!(
        mode_of(&core.paths().runtime_dir),
        PRIVATE_DIR_MODE,
        "runtime_dir must be 0700"
    );

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
        assert_eq!(
            mode_of(dir) & 0o077,
            0,
            "Dir {} has leaked permissions: {:o}",
            dir.display(),
            mode_of(dir)
        );
    }

    // 2. Note file creation under umask 0000
    let mut doc = NoteDocument::new_empty();
    doc.content = "Nota privada criada sob umask 000".into();
    core.storage().save_note_atomic(&doc).expect("save note");

    let note_path = core.storage().note_path(&doc.metadata.id);
    assert_eq!(
        mode_of(&note_path),
        PRIVATE_FILE_MODE,
        "Note file must be 0600"
    );
    assert_eq!(
        mode_of(&note_path) & 0o077,
        0,
        "Note file must not have group/other permissions"
    );

    // 3. Config file creation under umask 0000
    let config_path = core.storage().config_file_path();
    let config = AppConfig::default();
    config.save_to_file(&config_path).expect("save config");

    assert_eq!(
        mode_of(&config_path),
        PRIVATE_FILE_MODE,
        "Config file must be 0600"
    );
    assert_eq!(
        mode_of(&config_path) & 0o077,
        0,
        "Config file must not have group/other permissions"
    );

    // 4. State file creation under umask 0000
    let state_path = core.storage().state_file_path();
    let state = AppState::default();
    state.save_to_file(&state_path).expect("save state");

    assert_eq!(
        mode_of(&state_path),
        PRIVATE_FILE_MODE,
        "State file must be 0600"
    );
    assert_eq!(
        mode_of(&state_path) & 0o077,
        0,
        "State file must not have group/other permissions"
    );

    // 5. Backup snapshot creation under umask 0000
    let snapshot_dir = core.storage().create_backup_now().expect("create snapshot");
    assert_eq!(
        mode_of(&snapshot_dir),
        PRIVATE_DIR_MODE,
        "Snapshot dir must be 0700"
    );
    assert_eq!(mode_of(&snapshot_dir) & 0o077, 0);

    let manifest_path = snapshot_dir.join("manifest.json");
    assert_eq!(
        mode_of(&manifest_path),
        PRIVATE_FILE_MODE,
        "Manifest must be 0600"
    );
    assert_eq!(mode_of(&manifest_path) & 0o077, 0);

    let backed_note = snapshot_dir.join(format!("notes/{}.md", doc.metadata.id));
    assert_eq!(
        mode_of(&backed_note),
        PRIVATE_FILE_MODE,
        "Backed note must be 0600"
    );
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
    assert_eq!(
        mode & 0o077,
        0,
        "Temp file must never be world or group readable"
    );
}

fn is_running_as_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "0")
        .unwrap_or(false)
}

#[test]
fn r003_historical_0644_source_file_produces_0600_in_snapshot_and_source_remains_0644() {
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
    core.storage().ensure_directories().expect("ensure dirs");

    // 1. Create a note file and deliberately set historical mode 0644
    let mut doc = NoteDocument::new_empty();
    doc.content = "Nota com permissão histórica 0644".into();
    core.storage().save_note_atomic(&doc).expect("save note");
    let note_path = core.storage().note_path(&doc.metadata.id);
    fs::set_permissions(&note_path, fs::Permissions::from_mode(0o644)).expect("set 0644 on note");
    assert_eq!(mode_of(&note_path), 0o644, "Source note must be 0644");

    // 2. Create config with historical mode 0644
    let config_path = core.storage().config_file_path();
    let config = AppConfig::default();
    config.save_to_file(&config_path).expect("save config");
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o644))
        .expect("set 0644 on config");
    assert_eq!(mode_of(&config_path), 0o644, "Source config must be 0644");

    // 3. Create state with historical mode 0644
    let state_path = core.storage().state_file_path();
    let state = AppState::default();
    state.save_to_file(&state_path).expect("save state");
    fs::set_permissions(&state_path, fs::Permissions::from_mode(0o644)).expect("set 0644 on state");
    assert_eq!(mode_of(&state_path), 0o644, "Source state must be 0644");

    // 4. Create an asset with historical mode 0644
    let asset_dir = core.paths().assets_dir.join(doc.metadata.id.to_string());
    fs::create_dir_all(&asset_dir).expect("create asset dir");
    let asset_path = asset_dir.join("00000000-0000-4000-8000-000000000001.png");
    fs::write(&asset_path, b"fake png bytes").expect("write asset");
    fs::set_permissions(&asset_path, fs::Permissions::from_mode(0o644)).expect("set 0644 on asset");
    assert_eq!(mode_of(&asset_path), 0o644, "Source asset must be 0644");

    // 5. Create backup snapshot
    let snapshot_dir = core.storage().create_backup_now().expect("create backup");

    // 6. INVARIANT CHECK: In the snapshot, ALL directories are 0700 and ALL files are 0600!
    assert_eq!(
        mode_of(&snapshot_dir),
        0o700,
        "Snapshot root dir must be 0700"
    );
    assert_eq!(
        mode_of(&snapshot_dir.join("notes")),
        0o700,
        "Snapshot notes dir must be 0700"
    );
    assert_eq!(
        mode_of(&snapshot_dir.join("assets")),
        0o700,
        "Snapshot assets dir must be 0700"
    );

    let backed_note = snapshot_dir.join(format!("notes/{}.md", doc.metadata.id));
    assert_eq!(
        mode_of(&backed_note),
        0o600,
        "Snapshot note MUST BE 0600 even though source was 0644"
    );

    let backed_config = snapshot_dir.join("config.toml");
    assert_eq!(
        mode_of(&backed_config),
        0o600,
        "Snapshot config MUST BE 0600 even though source was 0644"
    );

    let backed_state = snapshot_dir.join("state.json");
    assert_eq!(
        mode_of(&backed_state),
        0o600,
        "Snapshot state MUST BE 0600 even though source was 0644"
    );

    let backed_asset = snapshot_dir.join(format!(
        "assets/{}/00000000-0000-4000-8000-000000000001.png",
        doc.metadata.id
    ));
    assert_eq!(
        mode_of(&backed_asset),
        0o600,
        "Snapshot asset MUST BE 0600 even though source was 0644"
    );

    let backed_manifest = snapshot_dir.join("manifest.json");
    assert_eq!(
        mode_of(&backed_manifest),
        0o600,
        "Snapshot manifest MUST BE 0600"
    );

    // 7. SOURCE INVARIANT: Source files must remain UNTOUCHED (0644)
    assert_eq!(
        mode_of(&note_path),
        0o644,
        "Source note permissions must not be mutated"
    );
    assert_eq!(
        mode_of(&config_path),
        0o644,
        "Source config permissions must not be mutated"
    );
    assert_eq!(
        mode_of(&state_path),
        0o644,
        "Source state permissions must not be mutated"
    );
    assert_eq!(
        mode_of(&asset_path),
        0o644,
        "Source asset permissions must not be mutated"
    );
}

/// Sets up an isolated store rooted at `root` and returns its core handle.
fn open_isolated_store(root: &Path) -> NoteItCore {
    let paths = StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        root.join("runtime/note-it"),
    );
    let storage = StorageManager::from_paths(paths).expect("open storage");
    let core = NoteItCore::from_storage(storage);
    core.storage().ensure_directories().expect("ensure dirs");
    core
}

/// A snapshot that cannot copy a note it can see must not be committed at all.
///
/// The note is a regular file the process may list but not read, which is the
/// shape a snapshot must never paper over: the entry is known to belong in the
/// backup, and its bytes could not be obtained.
#[test]
fn r003_backup_fails_closed_when_a_note_cannot_be_copied() {
    let tmp = tempdir().expect("tempdir");
    let core = open_isolated_store(tmp.path());
    let notes_dir = core.paths().notes_dir.clone();

    let mut doc = NoteDocument::new_empty();
    doc.content = "Nota legitima".into();
    core.storage().save_note_atomic(&doc).expect("save note");

    let unreadable_note = notes_dir.join(format!("{}.md", noteit_core::Uuid::new_v4()));
    fs::write(&unreadable_note, b"conteudo de nota").expect("write note");
    fs::set_permissions(&unreadable_note, fs::Permissions::from_mode(0o000)).expect("chmod 000");

    if is_running_as_root() {
        eprintln!("TEST REGISTERED: passed by harness; SCENARIO EXECUTED: NO; REASON: root/CAP_DAC_OVERRIDE");
        let _ = fs::set_permissions(&unreadable_note, fs::Permissions::from_mode(0o600));
        return;
    }

    let before = noteit_core::backup::list_snapshots(&core.paths().backups_dir).len();

    let backup_res = core.storage().create_backup_now();
    assert!(
        backup_res.is_err(),
        "Backup MUST FAIL CLOSED when a note cannot be read/copied"
    );

    let after = noteit_core::backup::list_snapshots(&core.paths().backups_dir).len();
    assert_eq!(
        before, after,
        "No new snapshot may be committed when a note copy fails"
    );

    fs::set_permissions(&unreadable_note, fs::Permissions::from_mode(0o600)).unwrap();
}

/// A snapshot that cannot even inspect an entry must not be committed either.
///
/// Denying traversal on the configuration directory makes `symlink_metadata` on
/// `config.toml` fail with `PermissionDenied` rather than `NotFound`, so the
/// backup cannot tell an absent file from one it simply could not look at.
#[test]
fn r003_backup_fails_closed_when_an_entry_cannot_be_inspected() {
    let tmp = tempdir().expect("tempdir");
    let core = open_isolated_store(tmp.path());

    let mut doc = NoteDocument::new_empty();
    doc.content = "Nota legitima".into();
    core.storage().save_note_atomic(&doc).expect("save note");

    let config_path = core.storage().config_file_path();
    AppConfig::default()
        .save_to_file(&config_path)
        .expect("save config");
    let config_dir = config_path.parent().expect("config parent").to_path_buf();
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o000)).expect("chmod 000");

    if is_running_as_root() {
        eprintln!("TEST REGISTERED: passed by harness; SCENARIO EXECUTED: NO; REASON: root/CAP_DAC_OVERRIDE");
        let _ = fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700));
        return;
    }

    // The precondition the test rests on: inspection fails, and not because the
    // file is missing.
    let probe = fs::symlink_metadata(&config_path)
        .expect_err("config.toml must not be inspectable through a 0000 directory");
    assert_ne!(
        probe.kind(),
        std::io::ErrorKind::NotFound,
        "the scenario needs an inspection failure, not an absent file"
    );

    let before = noteit_core::backup::list_snapshots(&core.paths().backups_dir).len();

    let backup_res = core.storage().create_backup_now();
    assert!(
        backup_res.is_err(),
        "Backup MUST FAIL CLOSED when an entry cannot be inspected"
    );
    let message = backup_res.unwrap_err();
    assert!(
        message.contains("Failed to inspect"),
        "the failure must name the inspection that could not be completed, got: {message}"
    );

    let after = noteit_core::backup::list_snapshots(&core.paths().backups_dir).len();
    assert_eq!(
        before, after,
        "No new snapshot may be committed when an entry cannot be inspected"
    );

    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700)).unwrap();
}
