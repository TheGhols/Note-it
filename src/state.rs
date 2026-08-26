use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LayerMode {
    #[default]
    Overlay,
    Desktop,
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteWindowState {
    #[serde(default = "default_x")]
    pub x: i32,
    #[serde(default = "default_y")]
    pub y: i32,
    #[serde(default = "default_width")]
    pub width: i32,
    #[serde(default = "default_height")]
    pub height: i32,
    #[serde(default = "default_true")]
    pub is_open: bool,
    #[serde(default)]
    pub monitor: Option<String>,
}

fn default_x() -> i32 {
    100
}

fn default_y() -> i32 {
    100
}

fn default_width() -> i32 {
    360
}

fn default_height() -> i32 {
    300
}

fn default_true() -> bool {
    true
}

impl Default for NoteWindowState {
    fn default() -> Self {
        Self {
            x: 100,
            y: 100,
            width: 360,
            height: 300,
            is_open: true,
            monitor: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AppState {
    #[serde(default)]
    pub active_layer_mode: LayerMode,
    #[serde(default)]
    pub notes: HashMap<Uuid, NoteWindowState>,
}

impl AppState {
    pub fn load_from_file(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }

        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str::<AppState>(&content).ok())
            .unwrap_or_default()
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create state directory: {e}"))?;
        }

        let serialized = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize app state: {e}"))?;

        let temp_path = path.with_extension(format!("tmp.{}", std::process::id()));
        {
            let mut file = File::create(&temp_path)
                .map_err(|e| format!("Failed to create state temp file: {e}"))?;
            file.write_all(serialized.as_bytes())
                .map_err(|e| format!("Failed to write state file: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("Failed to sync state file: {e}"))?;
        }

        fs::rename(&temp_path, path)
            .map_err(|e| format!("Failed to atomically rename state file: {e}"))?;
        if let Some(parent) = path.parent() {
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|e| format!("Failed to sync state directory: {e}"))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_app_state_persistence() {
        let tmp = tempdir().expect("tempdir");
        let state_path = tmp.path().join("state.json");

        let mut state = AppState::default();
        let note_id = Uuid::new_v4();
        state.active_layer_mode = LayerMode::Overlay;
        state.notes.insert(
            note_id,
            NoteWindowState {
                x: 200,
                y: 150,
                width: 360,
                height: 300,
                is_open: true,
                monitor: Some("DP-1".to_string()),
            },
        );

        state.save_to_file(&state_path).expect("Save state");
        assert!(state_path.exists());

        let loaded = AppState::load_from_file(&state_path);
        assert_eq!(loaded.active_layer_mode, LayerMode::Overlay);
        assert_eq!(loaded.notes.get(&note_id), state.notes.get(&note_id));
    }

    #[test]
    fn test_legacy_state_migration_and_defaults() {
        let legacy_json = r#"{
            "active_layer_mode": "desktop",
            "notes": {
                "00000000-0000-0000-0000-000000000001": {}
            }
        }"#;

        let parsed: AppState = serde_json::from_str(legacy_json).expect("parse legacy json");
        assert_eq!(parsed.active_layer_mode, LayerMode::Desktop);

        let note_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let win = parsed.notes.get(&note_id).expect("note exists");
        assert_eq!(win.x, 100);
        assert_eq!(win.y, 100);
        assert_eq!(win.width, 360);
        assert_eq!(win.height, 300);
        assert!(win.is_open);
        assert_eq!(win.monitor, None);
    }

    #[test]
    fn test_hide_does_not_alter_is_open() {
        let mut state = AppState::default();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        state.notes.insert(
            id1,
            NoteWindowState {
                x: 100,
                y: 100,
                width: 360,
                height: 300,
                is_open: true,
                monitor: None,
            },
        );
        state.notes.insert(
            id2,
            NoteWindowState {
                x: 200,
                y: 200,
                width: 400,
                height: 350,
                is_open: false,
                monitor: None,
            },
        );

        // Hide application
        state.active_layer_mode = LayerMode::Hidden;

        // is_open MUST remain unchanged
        assert!(state.notes.get(&id1).unwrap().is_open);
        assert!(!state.notes.get(&id2).unwrap().is_open);
    }

    #[test]
    fn test_close_note_sets_is_open_false() {
        let mut state = AppState::default();
        let id = Uuid::new_v4();

        state.notes.insert(
            id,
            NoteWindowState {
                x: 150,
                y: 180,
                width: 380,
                height: 320,
                is_open: true,
                monitor: Some("HDMI-A-1".to_string()),
            },
        );

        // Close individual note
        state.notes.get_mut(&id).unwrap().is_open = false;

        assert!(!state.notes.get(&id).unwrap().is_open);
        assert_eq!(state.notes.get(&id).unwrap().x, 150);
        assert_eq!(state.notes.get(&id).unwrap().y, 180);
        assert_eq!(state.notes.get(&id).unwrap().width, 380);
        assert_eq!(state.notes.get(&id).unwrap().height, 320);
        assert_eq!(
            state.notes.get(&id).unwrap().monitor.as_deref(),
            Some("HDMI-A-1")
        );
    }
}
