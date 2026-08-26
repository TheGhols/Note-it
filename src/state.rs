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
            let _ = fs::create_dir_all(parent);
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
}
