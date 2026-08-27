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

impl LayerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            LayerMode::Overlay => "overlay",
            LayerMode::Desktop => "desktop",
            LayerMode::Hidden => "hidden",
        }
    }

    /// Switch used by the menu, the keyboard shortcut and `note-it toggle`.
    /// A hidden application comes back as an overlay, matching the CLI.
    pub fn toggled(self) -> Self {
        match self {
            LayerMode::Desktop => LayerMode::Overlay,
            LayerMode::Overlay => LayerMode::Desktop,
            LayerMode::Hidden => LayerMode::Overlay,
        }
    }
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
    /// Whether the note is currently reduced to its header bar.
    #[serde(default)]
    pub collapsed: bool,
    /// Geometry to restore when the note is expanded again. Only meaningful
    /// while `collapsed` is true; `None` means no custom size was recorded.
    #[serde(default)]
    pub expanded_width: Option<i32>,
    #[serde(default)]
    pub expanded_height: Option<i32>,
    /// View scale of the note content, as a percentage. A view preference:
    /// it never touches the document or the note's `updated_at`.
    #[serde(default = "default_zoom_percent")]
    pub zoom_percent: u16,
}

pub const MIN_ZOOM_PERCENT: u16 = 75;
pub const MAX_ZOOM_PERCENT: u16 = 200;

fn default_zoom_percent() -> u16 {
    100
}

/// Keeps a zoom request inside the supported range. Values arriving from the
/// WebView or from a hand-edited state file are never trusted directly.
pub fn clamp_zoom_percent(value: u16) -> u16 {
    value.clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT)
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
            collapsed: false,
            expanded_width: None,
            expanded_height: None,
            zoom_percent: default_zoom_percent(),
        }
    }
}

impl NoteWindowState {
    /// Collapses the note down to `collapsed_height`, or expands it back to the
    /// geometry recorded when it was collapsed.
    ///
    /// Position is never touched, so a note dragged while collapsed expands
    /// exactly where the user left it. Returns `true` when the state changed.
    pub fn apply_collapsed(&mut self, collapsed: bool, collapsed_height: i32) -> bool {
        if collapsed == self.collapsed {
            return false;
        }

        if collapsed {
            self.expanded_width = Some(self.width);
            self.expanded_height = Some(self.height);
            self.height = collapsed_height;
        } else {
            if let Some(width) = self.expanded_width.take() {
                self.width = width;
            }
            // A collapsed note carries the header height, never a usable
            // expanded height, so fall back to the default instead of it.
            self.height = self.expanded_height.take().unwrap_or_else(default_height);
        }

        self.collapsed = collapsed;
        true
    }
}

/// Whether a "collapse everything" request should collapse or expand.
///
/// Anything still expanded means the user wants them all out of the way;
/// only once every note is already collapsed does the same action bring them
/// all back. With no notes at all there is nothing to do.
pub fn next_collapse_all(collapsed_flags: &[bool]) -> Option<bool> {
    if collapsed_flags.is_empty() {
        return None;
    }
    Some(collapsed_flags.iter().any(|collapsed| !collapsed))
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
                ..NoteWindowState::default()
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
        assert!(!win.collapsed);
        assert_eq!(win.expanded_width, None);
        assert_eq!(win.expanded_height, None);
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
                ..NoteWindowState::default()
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
                ..NoteWindowState::default()
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
                ..NoteWindowState::default()
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

    #[test]
    fn collapsing_preserves_the_expanded_geometry_and_position() {
        let mut note = NoteWindowState {
            x: 640,
            y: 210,
            width: 508,
            height: 552,
            ..NoteWindowState::default()
        };

        assert!(note.apply_collapsed(true, 30));
        assert!(note.collapsed);
        assert_eq!(note.height, 30);
        assert_eq!(note.width, 508);
        assert_eq!(note.expanded_width, Some(508));
        assert_eq!(note.expanded_height, Some(552));

        // Dragging the collapsed bar only moves it.
        note.x = 120;
        note.y = 880;

        assert!(note.apply_collapsed(false, 30));
        assert!(!note.collapsed);
        assert_eq!((note.width, note.height), (508, 552));
        assert_eq!((note.x, note.y), (120, 880));
        assert_eq!(note.expanded_width, None);
        assert_eq!(note.expanded_height, None);
    }

    #[test]
    fn repeated_collapse_requests_do_not_overwrite_the_expanded_geometry() {
        let mut note = NoteWindowState {
            width: 420,
            height: 480,
            ..NoteWindowState::default()
        };

        assert!(note.apply_collapsed(true, 30));
        assert!(!note.apply_collapsed(true, 30));
        assert_eq!(note.expanded_height, Some(480));
        assert_eq!(note.height, 30);

        assert!(note.apply_collapsed(false, 30));
        assert_eq!(note.height, 480);
        assert!(!note.apply_collapsed(false, 30));
        assert_eq!(note.height, 480);
    }

    #[test]
    fn expanding_without_a_recorded_height_falls_back_to_the_default() {
        // A hand-edited state file can claim `collapsed` without the companion
        // geometry; expanding must never restore the header-bar height.
        let mut note = NoteWindowState {
            height: 30,
            collapsed: true,
            expanded_width: None,
            expanded_height: None,
            ..NoteWindowState::default()
        };

        assert!(note.apply_collapsed(false, 30));
        assert_eq!(note.height, default_height());
        assert_eq!(note.width, 360);
    }

    #[test]
    fn collapsed_state_survives_a_save_and_load_round_trip() {
        let tmp = tempdir().expect("tempdir");
        let state_path = tmp.path().join("state.json");
        let note_id = Uuid::new_v4();

        let mut state = AppState::default();
        let mut note = NoteWindowState {
            x: 300,
            y: 400,
            width: 480,
            height: 620,
            ..NoteWindowState::default()
        };
        note.apply_collapsed(true, 30);
        state.notes.insert(note_id, note);
        state.save_to_file(&state_path).expect("save state");

        let reloaded = AppState::load_from_file(&state_path);
        let restored = reloaded.notes.get(&note_id).expect("note survives restart");
        assert!(restored.collapsed);
        assert_eq!(restored.height, 30);
        assert_eq!(restored.expanded_height, Some(620));
        assert_eq!(restored.expanded_width, Some(480));
        assert_eq!((restored.x, restored.y), (300, 400));
    }

    #[test]
    fn zoom_defaults_to_one_hundred_percent_for_older_state_files() {
        let legacy_json = r#"{
            "active_layer_mode": "overlay",
            "notes": { "00000000-0000-0000-0000-000000000002": { "x": 10, "y": 20 } }
        }"#;

        let parsed: AppState = serde_json::from_str(legacy_json).expect("parse legacy json");
        let note_id = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        assert_eq!(parsed.notes.get(&note_id).unwrap().zoom_percent, 100);
    }

    #[test]
    fn zoom_is_clamped_to_the_supported_range() {
        assert_eq!(clamp_zoom_percent(100), 100);
        assert_eq!(clamp_zoom_percent(75), MIN_ZOOM_PERCENT);
        assert_eq!(clamp_zoom_percent(200), MAX_ZOOM_PERCENT);
        assert_eq!(clamp_zoom_percent(0), MIN_ZOOM_PERCENT);
        assert_eq!(clamp_zoom_percent(1), MIN_ZOOM_PERCENT);
        assert_eq!(clamp_zoom_percent(10_000), MAX_ZOOM_PERCENT);
    }

    #[test]
    fn zoom_survives_a_state_round_trip_without_touching_geometry() {
        let tmp = tempdir().expect("tempdir");
        let state_path = tmp.path().join("state.json");
        let note_id = Uuid::new_v4();

        let mut state = AppState::default();
        state.notes.insert(
            note_id,
            NoteWindowState {
                x: 200,
                y: 150,
                width: 420,
                height: 380,
                zoom_percent: 130,
                ..NoteWindowState::default()
            },
        );
        state.save_to_file(&state_path).expect("save state");

        let restored = AppState::load_from_file(&state_path);
        let note = restored.notes.get(&note_id).expect("note survives");
        assert_eq!(note.zoom_percent, 130);
        assert_eq!(
            (note.x, note.y, note.width, note.height),
            (200, 150, 420, 380)
        );
    }

    #[test]
    fn layer_mode_toggle_matches_the_command_line_switch() {
        assert_eq!(LayerMode::Overlay.toggled(), LayerMode::Desktop);
        assert_eq!(LayerMode::Desktop.toggled(), LayerMode::Overlay);
        // Hidden is not part of the two-state switch; it restores the overlay.
        assert_eq!(LayerMode::Hidden.toggled(), LayerMode::Overlay);
        assert_eq!(LayerMode::Overlay.as_str(), "overlay");
        assert_eq!(LayerMode::Desktop.as_str(), "desktop");
        assert_eq!(LayerMode::Hidden.as_str(), "hidden");
    }

    #[test]
    fn switching_layer_mode_leaves_every_note_untouched() {
        let mut state = AppState {
            active_layer_mode: LayerMode::Overlay,
            ..AppState::default()
        };
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let note = NoteWindowState {
            x: 300,
            y: 200,
            width: 480,
            height: 360,
            is_open: true,
            monitor: Some("eDP-1".to_string()),
            collapsed: true,
            expanded_width: Some(480),
            expanded_height: Some(600),
            zoom_percent: 130,
        };
        state.notes.insert(first, note.clone());
        state.notes.insert(second, note.clone());

        // The switch is a global application setting.
        state.active_layer_mode = state.active_layer_mode.toggled();
        assert_eq!(state.active_layer_mode, LayerMode::Desktop);

        for id in [first, second] {
            let after = state.notes.get(&id).expect("note kept");
            assert_eq!(after, &note, "layer switch must not touch note state");
            assert!(after.is_open);
            assert!(after.collapsed);
            assert_eq!(after.zoom_percent, 130);
        }

        state.active_layer_mode = state.active_layer_mode.toggled();
        assert_eq!(state.active_layer_mode, LayerMode::Overlay);
        assert_eq!(state.notes.get(&first), Some(&note));
    }

    #[test]
    fn the_layer_mode_survives_a_restart() {
        let tmp = tempdir().expect("tempdir");
        let state_path = tmp.path().join("state.json");

        for mode in [LayerMode::Desktop, LayerMode::Overlay] {
            let state = AppState {
                active_layer_mode: mode,
                ..AppState::default()
            };
            state.save_to_file(&state_path).expect("save state");
            assert_eq!(
                AppState::load_from_file(&state_path).active_layer_mode,
                mode
            );
        }
    }

    #[test]
    fn collapsing_everything_starts_from_whatever_is_still_expanded() {
        // A, B expanded and C collapsed: the request collapses all three.
        assert_eq!(next_collapse_all(&[false, false, true]), Some(true));
        assert_eq!(next_collapse_all(&[false]), Some(true));
        assert_eq!(next_collapse_all(&[true, true, false]), Some(true));
    }

    #[test]
    fn collapsing_everything_again_expands_them_all() {
        assert_eq!(next_collapse_all(&[true, true, true]), Some(false));
        assert_eq!(next_collapse_all(&[true]), Some(false));
    }

    #[test]
    fn collapsing_everything_does_nothing_without_notes() {
        assert_eq!(next_collapse_all(&[]), None);
    }
}
