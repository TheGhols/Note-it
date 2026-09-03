//! Comprehensive tests for R-006: Preserving corrupted config and state files.
//!
//! Tests the 7 required scenarios for AppConfig and the 7 required scenarios for AppState:
//! 1. Missing file -> returns default safely
//! 2. Valid file -> loads as expected
//! 3. Legacy valid file -> preserves backwards compatibility
//! 4. Malformed file -> safely quarantined to .corrupted.<timestamp> with mode 0600, not silently lost
//! 5. Unreadable file -> reports read error, not confused with missing
//! 6. Preservation before replacement -> proves quarantined bytes match original before new write
//! 7. Preservation failure -> fail-safe: original corrupted file is never overwritten

use noteit_core::settings::{resolve_startup_config, AppConfig, ConfigLoadOutcome};
use noteit_core::state::{resolve_startup_state, AppState, LayerMode, StateLoadOutcome};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::tempdir;

fn permissions_of(path: &Path) -> u32 {
    fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

fn is_running_as_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "0")
        .unwrap_or(false)
}

#[test]
fn r006_config_1_missing_returns_default_and_saves() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");
    assert!(!path.exists());

    let outcome = AppConfig::load_detailed(&path);
    assert!(matches!(outcome, ConfigLoadOutcome::Missing(_)));
    assert_eq!(outcome.value(), AppConfig::default());
    assert!(
        path.exists(),
        "Missing config file should be initialized on disk"
    );
}

#[test]
fn r006_config_2_valid_loads_properly() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");

    let original = AppConfig {
        theme: "dark".to_string(),
        default_color: "green".to_string(),
        default_font_size: 18,
        ..AppConfig::default()
    };
    original.save_to_file(&path).expect("save config");

    let outcome = AppConfig::load_detailed(&path);
    match outcome {
        ConfigLoadOutcome::Valid(loaded) => {
            assert_eq!(loaded.theme, "dark");
            assert_eq!(loaded.default_color, "green");
            assert_eq!(loaded.default_font_size, 18);
        }
        other => panic!("Expected ConfigLoadOutcome::Valid, got {other:?}"),
    }
}

#[test]
fn r006_config_3_legacy_valid_loads_with_defaults_for_missing_fields() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");

    // Legacy format without ui_scale_percent or capture_delimiter
    let legacy_toml = concat!(
        "default_color = \"pink\"\n",
        "default_font_size = 14\n",
        "default_width = 300\n",
        "default_height = 250\n",
        "autosave_interval_ms = 400\n",
    );
    fs::write(&path, legacy_toml).expect("write legacy");

    let outcome = AppConfig::load_detailed(&path);
    match outcome {
        ConfigLoadOutcome::Valid(loaded) => {
            assert_eq!(loaded.default_color, "pink");
            assert_eq!(loaded.default_font_size, 14);
            assert_eq!(loaded.theme, "system"); // Default filled in
            assert_eq!(loaded.ui_scale_percent, 100);
        }
        other => panic!("Expected ConfigLoadOutcome::Valid, got {other:?}"),
    }
}

#[test]
fn r006_config_4_malformed_quarantines_and_returns_default_without_silent_loss() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");

    let garbage_bytes = b"theme = [unclosed syntax... @#$%^&*";
    fs::write(&path, garbage_bytes).expect("write garbage");

    let outcome = AppConfig::load_detailed(&path);
    match outcome {
        ConfigLoadOutcome::CorruptedRecovered {
            value,
            quarantine_path,
            error,
        } => {
            assert_eq!(value, AppConfig::default());
            assert!(!error.is_empty());
            assert!(quarantine_path.exists(), "Quarantine file must exist");
            assert_eq!(
                fs::read(&quarantine_path).expect("read quarantine"),
                garbage_bytes,
                "Quarantined bytes must match original byte-for-byte"
            );
            assert_eq!(
                permissions_of(&quarantine_path),
                0o600,
                "Quarantine file must be private (0600)"
            );
        }
        other => panic!("Expected ConfigLoadOutcome::CorruptedRecovered, got {other:?}"),
    }
}

#[test]
fn r006_config_5_unreadable_io_error_reported_without_treating_as_missing() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");

    // Create a directory where config.toml belongs so fs::read returns an I/O error
    fs::create_dir(&path).expect("occupy with dir");

    let outcome = AppConfig::load_detailed(&path);
    match outcome {
        ConfigLoadOutcome::ReadFailed(err) => {
            assert!(
                !err.is_empty(),
                "I/O read failure must report error details"
            );
        }
        other => panic!("Expected ConfigLoadOutcome::ReadFailed, got {other:?}"),
    }
}

#[test]
fn r006_config_6_preservation_before_replacement_proves_bytes_quarantined() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");

    let corrupt_content = b"corrupted = [[[bad toml";
    fs::write(&path, corrupt_content).expect("write corrupt");

    // Save a new config over the corrupted file
    let new_config = AppConfig {
        theme: "light".to_string(),
        ..AppConfig::default()
    };
    new_config
        .save_to_file(&path)
        .expect("save should succeed after quarantine");

    // Verify a quarantine file was created holding corrupt_content
    let quarantine_files: Vec<_> = fs::read_dir(tmp.path())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".corrupted."))
        .collect();
    assert_eq!(
        quarantine_files.len(),
        1,
        "Must find exactly 1 quarantine file"
    );
    assert_eq!(
        fs::read(quarantine_files[0].path()).expect("read quarantine"),
        corrupt_content
    );

    // Verify new config is now at path
    let loaded = AppConfig::load_from_file(&path);
    assert_eq!(loaded.theme, "light");
}

#[test]
fn r006_config_7_preservation_failure_fail_safe_original_not_overwritten() {
    if is_running_as_root() {
        eprintln!(
            "Skipping read-only directory permission test when running as root (CAP_DAC_OVERRIDE)"
        );
        return;
    }

    let tmp = tempdir().expect("tempdir");
    let config_dir = tmp.path().join("readonly_config_dir");
    fs::create_dir(&config_dir).expect("create dir");
    let path = config_dir.join("config.toml");

    let precious_corrupt_data = b"precious data with syntax error = [[[";
    fs::write(&path, precious_corrupt_data).expect("write corrupt");

    // Make directory read-only so quarantine creation fails
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o500))
        .expect("make dir read-only");

    let new_config = AppConfig::default();
    let save_res = new_config.save_to_file(&path);

    // Restore permissions so cleanup works
    let _ = fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700));

    // Save must fail closed
    assert!(
        save_res.is_err(),
        "Save must fail when preservation cannot be guaranteed"
    );
    assert_eq!(
        fs::read(&path).expect("read original"),
        precious_corrupt_data,
        "Original file must NEVER be overwritten if preservation fails"
    );
}

#[test]
fn r006_state_1_missing_returns_default() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");
    assert!(!path.exists());

    let outcome = AppState::load_detailed(&path);
    assert!(matches!(outcome, StateLoadOutcome::Missing(_)));
    assert_eq!(outcome.value(), AppState::default());
}

#[test]
fn r006_state_2_valid_loads_properly() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");

    let state = AppState {
        active_layer_mode: LayerMode::Desktop,
        ..AppState::default()
    };
    state.save_to_file(&path).expect("save state");

    let outcome = AppState::load_detailed(&path);
    match outcome {
        StateLoadOutcome::Valid(loaded) => {
            assert_eq!(loaded.active_layer_mode, LayerMode::Desktop);
        }
        other => panic!("Expected StateLoadOutcome::Valid, got {other:?}"),
    }
}

#[test]
fn r006_state_3_legacy_valid_loads_with_defaults() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");

    // Valid legacy state with just empty notes map
    fs::write(&path, "{\"notes\":{}}").expect("write legacy state");

    let outcome = AppState::load_detailed(&path);
    match outcome {
        StateLoadOutcome::Valid(loaded) => {
            assert_eq!(loaded.active_layer_mode, LayerMode::Overlay); // Default
            assert!(loaded.notes.is_empty());
        }
        other => panic!("Expected StateLoadOutcome::Valid, got {other:?}"),
    }
}

#[test]
fn r006_state_4_malformed_quarantines_and_returns_default_without_silent_loss() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");

    let bad_json = b"{\"notes\": { truncated json...";
    fs::write(&path, bad_json).expect("write bad json");

    let outcome = AppState::load_detailed(&path);
    match outcome {
        StateLoadOutcome::CorruptedRecovered {
            value,
            quarantine_path,
            error,
        } => {
            assert_eq!(value, AppState::default());
            assert!(!error.is_empty());
            assert!(quarantine_path.exists(), "Quarantine file must exist");
            assert_eq!(
                fs::read(&quarantine_path).expect("read quarantine"),
                bad_json
            );
            assert_eq!(
                permissions_of(&quarantine_path),
                0o600,
                "Quarantine file must be 0600"
            );
        }
        other => panic!("Expected StateLoadOutcome::CorruptedRecovered, got {other:?}"),
    }
}

#[test]
fn r006_state_5_unreadable_io_error_reported_safely() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");

    // Occupy path with directory so reading fails with I/O error
    fs::create_dir(&path).expect("occupy with dir");

    let outcome = AppState::load_detailed(&path);
    match outcome {
        StateLoadOutcome::ReadFailed(err) => {
            assert!(!err.is_empty());
        }
        other => panic!("Expected StateLoadOutcome::ReadFailed, got {other:?}"),
    }
}

#[test]
fn r006_state_6_preservation_before_replacement_proves_bytes_quarantined() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");

    let corrupt_json = b"{\"notes\": { bad syntax !!!";
    fs::write(&path, corrupt_json).expect("write corrupt json");

    let new_state = AppState {
        active_layer_mode: LayerMode::Hidden,
        ..AppState::default()
    };
    new_state
        .save_to_file(&path)
        .expect("save state over corrupted file");

    let quarantine_files: Vec<_> = fs::read_dir(tmp.path())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".corrupted."))
        .collect();
    assert_eq!(
        quarantine_files.len(),
        1,
        "Must find exactly 1 quarantine file"
    );
    assert_eq!(
        fs::read(quarantine_files[0].path()).expect("read quarantine"),
        corrupt_json
    );

    let reloaded = AppState::load_from_file(&path);
    assert_eq!(reloaded.active_layer_mode, LayerMode::Hidden);
}

#[test]
fn r006_state_7_preservation_failure_fail_safe_original_not_overwritten() {
    if is_running_as_root() {
        eprintln!(
            "Skipping read-only directory permission test when running as root (CAP_DAC_OVERRIDE)"
        );
        return;
    }

    let tmp = tempdir().expect("tempdir");
    let state_dir = tmp.path().join("readonly_state_dir");
    fs::create_dir(&state_dir).expect("create dir");
    let path = state_dir.join("state.json");

    let precious_corrupt_json = b"{\"precious_unparseable\": 123456";
    fs::write(&path, precious_corrupt_json).expect("write corrupt state");

    // Make directory read-only so quarantine creation fails
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o500)).expect("make dir read-only");

    let new_state = AppState::default();
    let save_res = new_state.save_to_file(&path);

    // Restore permissions so cleanup works
    let _ = fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o700));

    assert!(
        save_res.is_err(),
        "Save must fail when preservation cannot be guaranteed"
    );
    assert_eq!(
        fs::read(&path).expect("read original state"),
        precious_corrupt_json,
        "Original state file must NEVER be overwritten if preservation fails"
    );
}

#[test]
fn r006_application_startup_callsite_path_safely_quarantines_and_recovers() {
    let tmp = tempdir().expect("tempdir");
    let config_path = tmp.path().join("config.toml");
    let state_path = tmp.path().join("state.json");

    let bad_config = b"invalid = [toml unclosed";
    let bad_state = b"invalid = {json unclosed";
    fs::write(&config_path, bad_config).unwrap();
    fs::write(&state_path, bad_state).unwrap();

    // Call identical startup resolvers used by runtime in src/app.rs
    let config_outcome = AppConfig::load_detailed(&config_path);
    let resolved_config = resolve_startup_config(config_outcome);
    assert!(resolved_config.can_persist);
    assert_eq!(resolved_config.config, AppConfig::default());
    assert!(resolved_config.log_message.is_some());

    // Verify quarantine file exists on disk holding original bad bytes
    let quarantine_configs: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("config.toml.corrupted.")
        })
        .collect();
    assert_eq!(quarantine_configs.len(), 1);
    assert_eq!(fs::read(quarantine_configs[0].path()).unwrap(), bad_config);

    let state_outcome = AppState::load_detailed(&state_path);
    let resolved_state = resolve_startup_state(state_outcome);
    assert!(resolved_state.can_persist);
    assert_eq!(resolved_state.state, AppState::default());
    assert!(resolved_state.log_message.is_some());

    let quarantine_states: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("state.json.corrupted.")
        })
        .collect();
    assert_eq!(quarantine_states.len(), 1);
    assert_eq!(fs::read(quarantine_states[0].path()).unwrap(), bad_state);
}

#[test]
fn r006_config_8_unreadable_regular_file_fails_closed_and_preserves_original() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");

    let precious_bytes = b"precious_key = 'irreplaceable user configuration'\n";
    fs::write(&path, precious_bytes).expect("write regular config file");

    // Parent dir remains fully writable (0700)
    assert!(tmp.path().is_dir());

    // Ensure it is a regular file
    let meta = fs::symlink_metadata(&path).expect("metadata");
    assert!(meta.file_type().is_file(), "Target must be a regular file");

    // Make the regular file itself unreadable
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod 000");

    if is_running_as_root() {
        eprintln!("TEST REGISTERED: passed by harness; SCENARIO EXECUTED: NO; REASON: root/CAP_DAC_OVERRIDE");
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        return;
    }

    // 1. load_detailed must report ReadFailed
    let outcome = AppConfig::load_detailed(&path);
    assert!(
        matches!(outcome, ConfigLoadOutcome::ReadFailed(_)),
        "Expected ReadFailed on unreadable regular file, got {outcome:?}"
    );

    // 2. save_to_file must FAIL CLOSED and refuse to overwrite
    let default_config = AppConfig::default();
    let save_res = default_config.save_to_file(&path);
    assert!(
        save_res.is_err(),
        "save_to_file must fail closed when unable to read existing regular file"
    );

    // Restore permissions for inspection
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("restore 0600");

    // 3. Bytes on disk must remain 100% intact
    assert_eq!(
        fs::read(&path).expect("read path"),
        precious_bytes,
        "Original regular file must not be overwritten"
    );

    // 4. No quarantine debris or temp files
    let dir_entries: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        dir_entries.len(),
        1,
        "No unexpected files should be created in directory"
    );
    assert_eq!(dir_entries[0].file_name(), "config.toml");
}

#[test]
fn r006_config_9_deterministic_read_failure_fails_closed_and_preserves_disk_file() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");

    let precious_bytes = b"precious_key = 'never replace me on error'\n";
    fs::write(&path, precious_bytes).expect("write regular config file");

    // Reader that injects a simulated PermissionDenied / I/O error deterministically across all environments
    let failing_reader = |_p: &Path| -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "deterministic simulated permission denial",
        ))
    };

    // 1. load_detailed_with_reader must report ReadFailed
    let outcome = AppConfig::load_detailed_with_reader(&path, failing_reader);
    match &outcome {
        ConfigLoadOutcome::ReadFailed(err) => {
            assert!(err.contains("deterministic simulated permission denial"));
        }
        other => panic!("Expected ReadFailed, got {other:?}"),
    }

    // 2. save_to_file_with_reader must FAIL CLOSED
    let default_config = AppConfig::default();
    let save_res = default_config.save_to_file_with_reader(&path, failing_reader);
    assert!(
        save_res.is_err(),
        "save_to_file must return Err on unreadable existing file"
    );
    let err_msg = save_res.unwrap_err();
    assert!(err_msg.contains("unable to read existing content for preservation"));

    // 3. Disk file must remain untouched
    assert_eq!(
        fs::read(&path).expect("read real file"),
        precious_bytes,
        "Real file on disk must be completely unmodified"
    );

    // 4. No quarantine files created
    let dir_entries: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(dir_entries.len(), 1);
}

#[test]
fn r006_state_8_unreadable_regular_file_fails_closed_and_preserves_original() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");

    let precious_state_bytes = b"{\"precious_window\": [1, 2, 3]}\n";
    fs::write(&path, precious_state_bytes).expect("write regular state file");

    assert!(tmp.path().is_dir());
    let meta = fs::symlink_metadata(&path).expect("metadata");
    assert!(meta.file_type().is_file(), "Target must be a regular file");

    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod 000");

    if is_running_as_root() {
        eprintln!("TEST REGISTERED: passed by harness; SCENARIO EXECUTED: NO; REASON: root/CAP_DAC_OVERRIDE");
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        return;
    }

    let outcome = AppState::load_detailed(&path);
    assert!(
        matches!(outcome, StateLoadOutcome::ReadFailed(_)),
        "Expected ReadFailed on unreadable regular state file, got {outcome:?}"
    );

    let default_state = AppState::default();
    let save_res = default_state.save_to_file(&path);
    assert!(
        save_res.is_err(),
        "save_to_file must fail closed when unable to read existing state file"
    );

    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("restore 0600");

    assert_eq!(
        fs::read(&path).expect("read path"),
        precious_state_bytes,
        "Original regular state file must not be overwritten"
    );

    let dir_entries: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(dir_entries.len(), 1);
    assert_eq!(dir_entries[0].file_name(), "state.json");
}

#[test]
fn r006_state_9_deterministic_read_failure_fails_closed_and_preserves_disk_file() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");

    let precious_state_bytes = b"{\"precious_data\": true}\n";
    fs::write(&path, precious_state_bytes).expect("write regular state file");

    let failing_reader = |_p: &Path| -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "deterministic state read failure",
        ))
    };

    let outcome = AppState::load_detailed_with_reader(&path, failing_reader);
    match &outcome {
        StateLoadOutcome::ReadFailed(err) => {
            assert!(err.contains("deterministic state read failure"));
        }
        other => panic!("Expected ReadFailed, got {other:?}"),
    }

    let default_state = AppState::default();
    let save_res = default_state.save_to_file_with_reader(&path, failing_reader);
    assert!(
        save_res.is_err(),
        "save_to_file must fail closed when unable to read existing state file"
    );
    let err_msg = save_res.unwrap_err();
    assert!(err_msg.contains("unable to read existing content for preservation"));

    assert_eq!(
        fs::read(&path).expect("read real file"),
        precious_state_bytes,
        "Real state file on disk must be completely unmodified"
    );

    let dir_entries: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(dir_entries.len(), 1);
}

#[test]
fn r006_startup_resolvers_block_persistence_on_read_failure_or_preservation_failure() {
    // ConfigLoadOutcome::ReadFailed blocks persistence
    let rec = resolve_startup_config(ConfigLoadOutcome::ReadFailed("read err".into()));
    assert!(!rec.can_persist, "ReadFailed must block persistence");
    assert!(rec.log_message.unwrap().contains("Persistência bloqueada"));

    // ConfigLoadOutcome::CorruptedPreservationFailed blocks persistence
    let rec = resolve_startup_config(ConfigLoadOutcome::CorruptedPreservationFailed {
        error: "disk full".into(),
    });
    assert!(
        !rec.can_persist,
        "CorruptedPreservationFailed must block persistence"
    );

    // StateLoadOutcome::ReadFailed blocks persistence
    let rec = resolve_startup_state(StateLoadOutcome::ReadFailed("read err".into()));
    assert!(!rec.can_persist, "ReadFailed must block persistence");
    assert!(rec.log_message.unwrap().contains("Persistência bloqueada"));

    // StateLoadOutcome::CorruptedPreservationFailed blocks persistence
    let rec = resolve_startup_state(StateLoadOutcome::CorruptedPreservationFailed {
        error: "disk full".into(),
    });
    assert!(
        !rec.can_persist,
        "CorruptedPreservationFailed must block persistence"
    );
}

// ---------------------------------------------------------------------------
// R-006 (4.0R.R2.R1.2): the check made before overwriting asks the same
// question the loader asks.
//
// A file can be well-formed TOML or JSON and still not be a configuration or a
// state file. The loader has always called that corruption and preserved the
// bytes. The guard in front of the save asked only whether the syntax parsed,
// so it called the same file valid and replaced it — the loss the whole
// quarantine mechanism exists to prevent, reached through the other door.
// ---------------------------------------------------------------------------

/// Every `*.corrupted.*` file beside `path`, with its bytes.
fn quarantined_beside(path: &Path) -> Vec<(String, Vec<u8>)> {
    let parent = path.parent().expect("parent");
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut found: Vec<(String, Vec<u8>)> = fs::read_dir(parent)
        .expect("read dir")
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{name}.corrupted."))
        })
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("read quarantine"),
            )
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Valid TOML, and not a configuration: `theme` is a string.
const SEMANTICALLY_INVALID_CONFIG: &[u8] = b"theme = 12345\ndefault_color = 'yellow'\n";

/// Valid JSON, and not a state file: `notes` is a map.
const SEMANTICALLY_INVALID_STATE: &[u8] = b"{\"notes\": \"not-an-object\"}\n";

#[test]
fn r006_cfg_a_a_valid_configuration_is_replaced_without_quarantine() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");

    let original = AppConfig {
        theme: "dark".to_string(),
        ..AppConfig::default()
    };
    original.save_to_file(&path).expect("write a valid config");
    assert!(matches!(
        AppConfig::load_detailed(&path),
        ConfigLoadOutcome::Valid(_)
    ));

    let next = AppConfig {
        theme: "light".to_string(),
        ..AppConfig::default()
    };
    next.save_to_file(&path)
        .expect("a valid config is replaced normally");

    assert_eq!(
        AppConfig::load_from_file(&path).theme,
        "light",
        "the new configuration is what is on disk"
    );
    assert!(
        quarantined_beside(&path).is_empty(),
        "replacing a valid configuration quarantines nothing"
    );
}

#[test]
fn r006_cfg_b_syntactically_valid_but_semantically_invalid_config_is_preserved_first() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");
    fs::write(&path, SEMANTICALLY_INVALID_CONFIG).expect("write the odd config");

    // The precondition: this is well-formed TOML, and it is not a configuration.
    let text = std::str::from_utf8(SEMANTICALLY_INVALID_CONFIG).expect("utf-8");
    assert!(
        toml::from_str::<toml::Value>(text).is_ok(),
        "the scenario needs syntactically valid TOML"
    );
    assert!(
        toml::from_str::<AppConfig>(text).is_err(),
        "the scenario needs it to be invalid as a configuration"
    );

    let before = fs::read(&path).expect("original bytes");
    assert_eq!(before, SEMANTICALLY_INVALID_CONFIG);

    AppConfig::default()
        .save_to_file(&path)
        .expect("the save proceeds once the original has been preserved");

    let quarantined = quarantined_beside(&path);
    assert_eq!(
        quarantined.len(),
        1,
        "the original must be preserved before it is replaced, found {quarantined:?}"
    );
    assert_eq!(
        quarantined[0].1, before,
        "the quarantined bytes must be the original bytes, unchanged"
    );

    // And the loader agrees the file that was there was not a configuration.
    fs::write(&path, SEMANTICALLY_INVALID_CONFIG).expect("restore for the loader");
    assert!(
        matches!(
            AppConfig::load_detailed(&path),
            ConfigLoadOutcome::CorruptedRecovered { .. }
        ),
        "loader and save must agree about what counts as a configuration"
    );
}

#[test]
fn r006_cfg_c_preservation_failure_blocks_replacing_a_semantically_invalid_config() {
    if is_running_as_root() {
        eprintln!("TEST REGISTERED: passed by harness; SCENARIO EXECUTED: NO; REASON: root/CAP_DAC_OVERRIDE");
        return;
    }

    let tmp = tempdir().expect("tempdir");
    let dir = tmp.path().join("config");
    fs::create_dir(&dir).expect("create dir");
    let path = dir.join("config.toml");
    fs::write(&path, SEMANTICALLY_INVALID_CONFIG).expect("write the odd config");

    // Nothing new may be created here, so the quarantine cannot be written.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).expect("read-only dir");
    let outcome = AppConfig::default().save_to_file(&path);
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));

    assert!(
        outcome.is_err(),
        "a save that cannot preserve the original must not replace it"
    );
    assert_eq!(
        fs::read(&path).expect("read back"),
        SEMANTICALLY_INVALID_CONFIG,
        "the original must remain byte-identical"
    );
    assert!(
        quarantined_beside(&path).is_empty(),
        "a failed preservation leaves no partial quarantine behind"
    );
}

#[test]
fn r006_cfg_d_an_unreadable_config_still_blocks_the_save() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");
    fs::write(&path, SEMANTICALLY_INVALID_CONFIG).expect("write");

    let failing_reader = |_p: &Path| -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "deterministic read failure",
        ))
    };

    let outcome = AppConfig::default().save_to_file_with_reader(&path, failing_reader);
    assert!(outcome.is_err(), "an unreadable original is never replaced");
    assert_eq!(
        fs::read(&path).expect("read back"),
        SEMANTICALLY_INVALID_CONFIG,
        "the original must remain byte-identical"
    );
}

#[test]
fn r006_state_a_a_valid_state_is_replaced_without_quarantine() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");

    let original = AppState {
        active_layer_mode: LayerMode::Overlay,
        ..AppState::default()
    };
    original.save_to_file(&path).expect("write a valid state");
    assert!(matches!(
        AppState::load_detailed(&path),
        StateLoadOutcome::Valid(_)
    ));

    AppState::default()
        .save_to_file(&path)
        .expect("a valid state is replaced normally");
    assert!(
        quarantined_beside(&path).is_empty(),
        "replacing a valid state quarantines nothing"
    );
}

#[test]
fn r006_state_b_syntactically_valid_but_semantically_invalid_state_is_preserved_first() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");
    fs::write(&path, SEMANTICALLY_INVALID_STATE).expect("write the odd state");

    let text = std::str::from_utf8(SEMANTICALLY_INVALID_STATE).expect("utf-8");
    assert!(
        serde_json::from_str::<serde_json::Value>(text).is_ok(),
        "the scenario needs syntactically valid JSON"
    );
    assert!(
        serde_json::from_str::<AppState>(text).is_err(),
        "the scenario needs it to be invalid as a state file"
    );

    let before = fs::read(&path).expect("original bytes");

    AppState::default()
        .save_to_file(&path)
        .expect("the save proceeds once the original has been preserved");

    let quarantined = quarantined_beside(&path);
    assert_eq!(
        quarantined.len(),
        1,
        "the original must be preserved before it is replaced, found {quarantined:?}"
    );
    assert_eq!(
        quarantined[0].1, before,
        "the quarantined bytes must be the original bytes, unchanged"
    );

    fs::write(&path, SEMANTICALLY_INVALID_STATE).expect("restore for the loader");
    assert!(
        matches!(
            AppState::load_detailed(&path),
            StateLoadOutcome::CorruptedRecovered { .. }
        ),
        "loader and save must agree about what counts as a state file"
    );
}

#[test]
fn r006_state_c_preservation_failure_blocks_replacing_a_semantically_invalid_state() {
    if is_running_as_root() {
        eprintln!("TEST REGISTERED: passed by harness; SCENARIO EXECUTED: NO; REASON: root/CAP_DAC_OVERRIDE");
        return;
    }

    let tmp = tempdir().expect("tempdir");
    let dir = tmp.path().join("state");
    fs::create_dir(&dir).expect("create dir");
    let path = dir.join("state.json");
    fs::write(&path, SEMANTICALLY_INVALID_STATE).expect("write the odd state");

    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).expect("read-only dir");
    let outcome = AppState::default().save_to_file(&path);
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));

    assert!(
        outcome.is_err(),
        "a save that cannot preserve the original must not replace it"
    );
    assert_eq!(
        fs::read(&path).expect("read back"),
        SEMANTICALLY_INVALID_STATE,
        "the original must remain byte-identical"
    );
    assert!(quarantined_beside(&path).is_empty());
}

#[test]
fn r006_state_d_an_unreadable_state_still_blocks_the_save() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("state.json");
    fs::write(&path, SEMANTICALLY_INVALID_STATE).expect("write");

    let failing_reader = |_p: &Path| -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "deterministic read failure",
        ))
    };

    let outcome = AppState::default().save_to_file_with_reader(&path, failing_reader);
    assert!(outcome.is_err(), "an unreadable original is never replaced");
    assert_eq!(
        fs::read(&path).expect("read back"),
        SEMANTICALLY_INVALID_STATE,
        "the original must remain byte-identical"
    );
}
