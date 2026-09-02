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

use noteit_core::settings::{AppConfig, ConfigLoadOutcome};
use noteit_core::state::{AppState, LayerMode, StateLoadOutcome};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::tempdir;

fn permissions_of(path: &Path) -> u32 {
    fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

#[test]
fn r006_config_1_missing_returns_default_and_saves() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");
    assert!(!path.exists());

    let outcome = AppConfig::load_detailed(&path);
    assert!(matches!(outcome, ConfigLoadOutcome::Missing(_)));
    assert_eq!(outcome.value(), AppConfig::default());
    assert!(path.exists(), "Missing config file should be initialized on disk");
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
            assert!(!err.is_empty(), "I/O read failure must report error details");
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
    new_config.save_to_file(&path).expect("save should succeed after quarantine");

    // Verify a quarantine file was created holding corrupt_content
    let quarantine_files: Vec<_> = fs::read_dir(tmp.path())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".corrupted."))
        .collect();
    assert_eq!(quarantine_files.len(), 1, "Must find exactly 1 quarantine file");
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
    let tmp = tempdir().expect("tempdir");
    let config_dir = tmp.path().join("readonly_config_dir");
    fs::create_dir(&config_dir).expect("create dir");
    let path = config_dir.join("config.toml");

    let precious_corrupt_data = b"precious data with syntax error = [[[";
    fs::write(&path, precious_corrupt_data).expect("write corrupt");

    // Make directory read-only so quarantine creation fails
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o500)).expect("make dir read-only");

    let new_config = AppConfig::default();
    let save_res = new_config.save_to_file(&path);

    // Restore permissions so cleanup works
    let _ = fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700));

    // Save must fail closed
    assert!(save_res.is_err(), "Save must fail when preservation cannot be guaranteed");
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

    let mut state = AppState::default();
    state.active_layer_mode = LayerMode::Desktop;
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

    let mut new_state = AppState::default();
    new_state.active_layer_mode = LayerMode::Hidden;
    new_state.save_to_file(&path).expect("save state over corrupted file");

    let quarantine_files: Vec<_> = fs::read_dir(tmp.path())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".corrupted."))
        .collect();
    assert_eq!(quarantine_files.len(), 1, "Must find exactly 1 quarantine file");
    assert_eq!(
        fs::read(quarantine_files[0].path()).expect("read quarantine"),
        corrupt_json
    );

    let reloaded = AppState::load_from_file(&path);
    assert_eq!(reloaded.active_layer_mode, LayerMode::Hidden);
}

#[test]
fn r006_state_7_preservation_failure_fail_safe_original_not_overwritten() {
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

    assert!(save_res.is_err(), "Save must fail when preservation cannot be guaranteed");
    assert_eq!(
        fs::read(&path).expect("read original state"),
        precious_corrupt_json,
        "Original state file must NEVER be overwritten if preservation fails"
    );
}
