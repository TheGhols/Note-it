//! Tests for R-008: Metadata scan and listing anomalies must not be silently swallowed.
//!
//! Verifies that:
//! 1. Symlinks in notes directory produce typed ReadWarningKind::SymlinkRefused instead of silent continue.
//! 2. Unreadable files produce typed ReadWarningKind::UnreadableNote.
//! 3. Malformed front matter produces typed ReadWarning.
//! 4. Human CLI routes warnings to stderr and clean data to stdout.
//! 5. Machine CLI (--json) produces a single valid JSON document on stdout containing both data and warnings envelope.

use noteit_core::filter::NoteFilter;
use noteit_core::metadata::NoteMetadata;
use noteit_core::model::NoteDocument;
use noteit_core::warning::ReadWarningKind;
use noteit_core::{NoteItCore, StorageManager, StorePaths, Uuid};
use std::fs;
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn noteit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_noteit"))
}

fn run_cli(args: &[&str], root: &Path) -> (i32, String, String) {
    let mut cmd = Command::new(noteit_bin());
    cmd.args(args);

    let data = root.join("data");
    let config = root.join("config");
    let state = root.join("state");
    let runtime = root.join("runtime");

    cmd.env("XDG_DATA_HOME", &data);
    cmd.env("XDG_CONFIG_HOME", &config);
    cmd.env("XDG_STATE_HOME", &state);
    cmd.env("XDG_RUNTIME_DIR", &runtime);
    cmd.env("NO_COLOR", "1");

    let output = cmd.output().expect("execute noteit binary");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn setup_store(root: &Path) -> (NoteItCore, PathBuf) {
    let notes_dir = root.join("data/note-it/notes");
    let config_dir = root.join("config/note-it");
    let state_dir = root.join("state/note-it");
    let runtime_dir = root.join("runtime/note-it");

    let paths =
        StorePaths::from_custom_paths(notes_dir.clone(), config_dir, state_dir, runtime_dir);

    let storage = StorageManager::from_paths(paths).expect("open storage");
    let core = NoteItCore::from_storage(storage);
    core.storage().ensure_directories().expect("ensure dirs");
    (core, notes_dir)
}

#[test]
fn r008_1_symlink_in_notes_produces_typed_warning_and_valid_data_loads() {
    let tmp = tempdir().expect("tempdir");
    let (core, notes_dir) = setup_store(tmp.path());

    // 1. Legitimate note
    let mut valid = NoteDocument::new_empty();
    valid.content = "# Nota Legítima".into();
    core.storage().save_note_atomic(&valid).expect("save valid");

    // 2. Symlink in notes directory pointing to target
    let target = tmp.path().join("outside.md");
    fs::write(&target, "# Fora").expect("write outside");
    let symlink_path = notes_dir.join(format!("{}.md", Uuid::new_v4()));
    symlink(&target, &symlink_path).expect("create symlink");

    // Core listing must return the valid note AND emit SymlinkRefused warning
    let batch = core
        .list_summaries(&NoteFilter::default(), None)
        .expect("list summaries");
    assert_eq!(
        batch.items.len(),
        1,
        "Only legitimate note should be in items"
    );
    assert_eq!(batch.items[0].id, valid.metadata.id);

    let symlink_warnings: Vec<_> = batch
        .warnings
        .iter()
        .filter(|w| w.kind == ReadWarningKind::SymlinkRefused)
        .collect();
    assert_eq!(
        symlink_warnings.len(),
        1,
        "Symlink must produce exactly one SymlinkRefused warning, got: {:?}",
        batch.warnings
    );
    assert!(symlink_warnings[0].message.contains("link simbólico"));
}

#[test]
fn r008_2_unreadable_file_produces_typed_warning() {
    let tmp = tempdir().expect("tempdir");
    let (core, notes_dir) = setup_store(tmp.path());

    // 1. Legitimate note
    let mut valid = NoteDocument::new_empty();
    valid.content = "# Nota Legítima".into();
    core.storage().save_note_atomic(&valid).expect("save valid");

    // 2. Unreadable note (permissions 0000)
    let bad_id = Uuid::new_v4();
    let bad_path = notes_dir.join(format!("{bad_id}.md"));
    fs::write(&bad_path, "conteudo").expect("write bad note");
    fs::set_permissions(&bad_path, fs::Permissions::from_mode(0o000)).expect("chmod 000");

    let batch = core
        .list_summaries(&NoteFilter::default(), None)
        .expect("list");

    // Cleanup permissions before assertions so tempdir teardown succeeds
    let _ = fs::set_permissions(&bad_path, fs::Permissions::from_mode(0o600));

    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].id, valid.metadata.id);

    let unreadable_warnings: Vec<_> = batch
        .warnings
        .iter()
        .filter(|w| w.kind == ReadWarningKind::UnreadableNote)
        .collect();
    assert_eq!(unreadable_warnings.len(), 1);
    assert_eq!(unreadable_warnings[0].note_id, Some(bad_id));
}

#[test]
fn r008_3_human_cli_routes_warnings_to_stderr_and_data_to_stdout() {
    let tmp = tempdir().expect("tempdir");
    let (core, notes_dir) = setup_store(tmp.path());

    let mut valid = NoteDocument::new_empty();
    valid.content = "# Nota Visível para Humano".into();
    core.storage().save_note_atomic(&valid).expect("save valid");

    // Add a symlink anomaly
    let target = tmp.path().join("target.md");
    fs::write(&target, "alvo").expect("target");
    symlink(&target, notes_dir.join(format!("{}.md", Uuid::new_v4()))).expect("symlink");

    let (exit_code, stdout, stderr) = run_cli(&["listar"], tmp.path());
    assert_eq!(exit_code, 0, "Command should succeed");

    // Data must be on stdout
    assert!(
        stdout.contains("Nota Visível para Humano"),
        "Stdout must contain valid note, got: {stdout}"
    );

    // Warning must be on stderr, not stdout
    assert!(
        stderr.contains("Aviso") || stderr.contains("link simbólico") || stderr.contains("warning"),
        "Stderr must contain warning, got: {stderr}"
    );
    assert!(
        !stdout.contains("link simbólico"),
        "Stdout must not be polluted with warning logs, got: {stdout}"
    );
}

#[test]
fn r008_4_machine_json_includes_warnings_in_envelope_without_log_pollution() {
    let tmp = tempdir().expect("tempdir");
    let (core, notes_dir) = setup_store(tmp.path());

    let mut valid = NoteDocument::new_empty();
    valid.content = "# Nota Para JSON".into();
    core.storage().save_note_atomic(&valid).expect("save valid");

    // Add a symlink anomaly
    let target = tmp.path().join("target.md");
    fs::write(&target, "alvo").expect("target");
    symlink(&target, notes_dir.join(format!("{}.md", Uuid::new_v4()))).expect("symlink");

    let (exit_code, stdout, stderr) = run_cli(&["listar", "--json"], tmp.path());
    assert_eq!(exit_code, 0, "CLI --json should succeed");
    assert!(
        stderr.is_empty(),
        "Machine mode must not write loose logs to stderr: {stderr}"
    );

    // Parse stdout as a single JSON document
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("Stdout must be a valid JSON document: {e}\nStdout was:\n{stdout}")
    });

    assert_eq!(parsed["status"], "warning");
    assert!(parsed["error"].is_null());
    assert!(parsed["data"].is_object());

    let warnings = parsed["warnings"]
        .as_array()
        .expect("warnings array in json envelope");
    assert!(
        !warnings.is_empty(),
        "Warnings array must contain the scan anomaly"
    );
    assert_eq!(warnings[0]["code"], "symlink_refused");
}

#[test]
fn r008_5_tags_command_human_and_json_with_one_good_and_one_corrupted() {
    let tmp = tempdir().expect("tempdir");
    let (core, notes_dir) = setup_store(tmp.path());

    // 1. Good note with tag 'projeto'
    let mut good = NoteDocument::new_empty();
    good.content = "# Nota Boa\nTexto da nota".into();
    good.user_metadata = NoteMetadata::try_new(vec!["projeto".into()], vec![]).expect("metadata");
    core.storage()
        .save_note_atomic(&good)
        .expect("save good note");

    // 2. Corrupted front-matter note
    let bad_id = Uuid::new_v4();
    let bad_path = notes_dir.join(format!("{bad_id}.md"));
    fs::write(
        &bad_path,
        "---\ncorrupted_yaml: [unclosed list\n---\n# Corrupted content\n",
    )
    .expect("write bad note");

    // 3. Symlink in notes directory
    let outside_target = tmp.path().join("outside.md");
    fs::write(&outside_target, "# Outside").expect("write outside");
    let symlink_path = notes_dir.join(format!("{}.md", Uuid::new_v4()));
    symlink(&outside_target, &symlink_path).expect("create symlink");

    // Test Human CLI: `noteit tags`
    let (exit_code, stdout, stderr) = run_cli(&["tags"], tmp.path());
    assert_eq!(exit_code, 0, "Human tags command should succeed");
    assert!(
        stdout.contains("projeto"),
        "Stdout must contain valid tag 'projeto', got: {stdout}"
    );
    assert!(
        stderr.contains("Aviso")
            || stderr.contains("corrompido")
            || stderr.contains("link simbólico"),
        "Stderr must contain scan warnings, got: {stderr}"
    );
    assert!(
        !stdout.contains("corrompido") && !stdout.contains("link simbólico"),
        "Stdout must not be polluted with warnings, got: {stdout}"
    );

    // Test Machine CLI: `noteit tags --json`
    let (exit_code_json, stdout_json, stderr_json) = run_cli(&["tags", "--json"], tmp.path());
    assert_eq!(exit_code_json, 0, "JSON tags command should succeed");
    assert!(
        stderr_json.is_empty(),
        "Stderr must be empty in machine mode: {stderr_json}"
    );

    let parsed: serde_json::Value = serde_json::from_str(&stdout_json)
        .unwrap_or_else(|e| panic!("Stdout must be valid JSON: {e}\nRaw stdout: {stdout_json}"));
    assert_eq!(parsed["status"], "warning");
    assert!(parsed["error"].is_null());

    let tags = parsed["data"]["tags"]
        .as_array()
        .expect("tags array in JSON data");
    assert_eq!(tags.len(), 1, "Only valid note tags should be present");
    assert_eq!(tags[0]["name"], "projeto");
    assert_eq!(tags[0]["note_count"], 1);

    let warnings = parsed["warnings"].as_array().expect("warnings array");
    assert_eq!(
        warnings.len(),
        2,
        "Expected exactly 2 warnings (corrupted FM + symlink), got: {:?}",
        warnings
    );

    let codes: Vec<_> = warnings
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"corrupted_front_matter"));
    assert!(codes.contains(&"symlink_refused"));
}

#[test]
fn r008_6_propriedades_command_human_and_json_with_one_good_and_one_corrupted() {
    let tmp = tempdir().expect("tempdir");
    let (core, notes_dir) = setup_store(tmp.path());

    // 1. Good note with property 'prioridade = alta'
    let mut good = NoteDocument::new_empty();
    good.content = "# Nota com Propriedade\nTexto".into();
    good.user_metadata = NoteMetadata::try_new(
        vec![],
        vec![noteit_core::metadata::NoteProperty {
            key: "prioridade".into(),
            value: "alta".into(),
        }],
    )
    .expect("metadata");
    core.storage()
        .save_note_atomic(&good)
        .expect("save good note");

    // 2. Corrupted front-matter note
    let bad_id = Uuid::new_v4();
    let bad_path = notes_dir.join(format!("{bad_id}.md"));
    fs::write(
        &bad_path,
        "---\ncorrupted_yaml: {invalid\n---\n# Corrupted content\n",
    )
    .expect("write bad note");

    // 3. Symlink
    let outside_target = tmp.path().join("outside_prop.md");
    fs::write(&outside_target, "# Outside").expect("write outside");
    let symlink_path = notes_dir.join(format!("{}.md", Uuid::new_v4()));
    symlink(&outside_target, &symlink_path).expect("create symlink");

    // Human CLI: `noteit propriedades`
    let (exit_code, stdout, stderr) = run_cli(&["propriedades"], tmp.path());
    assert_eq!(exit_code, 0);
    assert!(
        stdout.contains("prioridade"),
        "Stdout must list 'prioridade'"
    );
    assert!(
        stderr.contains("Aviso")
            || stderr.contains("corrompido")
            || stderr.contains("link simbólico"),
        "Stderr must contain scan warnings: {stderr}"
    );

    // Machine CLI: `noteit propriedades --json`
    let (exit_code_json, stdout_json, stderr_json) =
        run_cli(&["propriedades", "--json"], tmp.path());
    assert_eq!(exit_code_json, 0);
    assert!(
        stderr_json.is_empty(),
        "Stderr must be empty: {stderr_json}"
    );

    let parsed: serde_json::Value = serde_json::from_str(&stdout_json)
        .unwrap_or_else(|e| panic!("Stdout must be valid JSON: {e}\nRaw stdout: {stdout_json}"));
    assert_eq!(parsed["status"], "warning");
    assert!(parsed["error"].is_null());

    let props = parsed["data"]["properties"]
        .as_array()
        .expect("properties array in JSON data");
    assert_eq!(props.len(), 1);
    assert_eq!(props[0]["key"], "prioridade");
    assert_eq!(props[0]["note_count"], 1);

    let warnings = parsed["warnings"].as_array().expect("warnings array");
    assert_eq!(
        warnings.len(),
        2,
        "Expected exactly 2 warnings (corrupted FM + symlink), got: {:?}",
        warnings
    );

    let codes: Vec<_> = warnings
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"corrupted_front_matter"));
    assert!(codes.contains(&"symlink_refused"));
}

#[test]
fn r008_7_metadata_catalog_with_warnings_direct_domain_api() {
    let tmp = tempdir().expect("tempdir");
    let (core, notes_dir) = setup_store(tmp.path());

    let mut good = NoteDocument::new_empty();
    good.content = "# Nota".into();
    good.user_metadata = NoteMetadata::try_new(
        vec!["tag1".into()],
        vec![noteit_core::metadata::NoteProperty {
            key: "key1".into(),
            value: "val1".into(),
        }],
    )
    .expect("metadata");
    core.storage().save_note_atomic(&good).expect("save");

    let bad_id = Uuid::new_v4();
    fs::write(
        notes_dir.join(format!("{bad_id}.md")),
        "---\ninvalid: [\n---",
    )
    .expect("write bad");

    let (catalog, warnings) = core.metadata_catalog_with_warnings();
    assert_eq!(catalog.tags.len(), 1);
    assert_eq!(catalog.tags[0].tag, "tag1");
    assert_eq!(catalog.property_keys.len(), 1);
    assert_eq!(catalog.property_keys[0].key, "key1");

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, ReadWarningKind::CorruptedFrontMatter);
    assert_eq!(warnings[0].note_id, Some(bad_id));
}
