use crate::atomic_file::write_atomic;
use crate::autopaste::DEFAULT_CAPTURE_DELIMITER;
use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Interface themes, in the order the menu offers them.
///
/// A theme dresses the application's own chrome — menus, popovers, borders,
/// focus rings — and never the paper a note is written on: a yellow note stays
/// yellow under the dark theme, and a black one stays black under the light
/// one.
pub const THEMES: &[&str] = &["system", "light", "dark"];
pub const DEFAULT_THEME: &str = "system";
pub const MIN_UI_SCALE_PERCENT: u16 = 90;
pub const MAX_UI_SCALE_PERCENT: u16 = 160;
pub const DEFAULT_UI_SCALE_PERCENT: u16 = 100;

pub fn clamp_ui_scale_percent(value: u16) -> u16 {
    value.clamp(MIN_UI_SCALE_PERCENT, MAX_UI_SCALE_PERCENT)
}

fn normalize_ui_scale_integer(value: i64) -> u16 {
    value.clamp(
        i64::from(MIN_UI_SCALE_PERCENT),
        i64::from(MAX_UI_SCALE_PERCENT),
    ) as u16
}

fn deserialize_ui_scale<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let value = toml::Value::deserialize(deserializer)?;
    Ok(value
        .as_integer()
        .map(normalize_ui_scale_integer)
        .unwrap_or(DEFAULT_UI_SCALE_PERCENT))
}

/// Resolves a stored theme to the supported set, falling back to following the
/// environment so a hand-edited `config.toml` cannot leave the UI unstyled.
pub fn theme_name(value: &str) -> &'static str {
    THEMES
        .iter()
        .find(|name| **name == value)
        .copied()
        .unwrap_or(DEFAULT_THEME)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default = "default_color")]
    pub default_color: String,
    #[serde(default = "default_font_size")]
    pub default_font_size: u32,
    #[serde(default = "default_note_width")]
    pub default_width: i32,
    #[serde(default = "default_note_height")]
    pub default_height: i32,
    #[serde(default = "default_autosave_interval_ms")]
    pub autosave_interval_ms: u32,
    /// Appearance of the application chrome, shared by every note.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Scale of application chrome, shared by every note and never applied to
    /// the Markdown document or its per-note zoom.
    #[serde(
        default = "default_ui_scale_percent",
        deserialize_with = "deserialize_ui_scale"
    )]
    pub ui_scale_percent: u16,
    /// What AutoPaste puts between the note's existing content and a capture.
    ///
    /// A preference and not a mode: it says how captures should be laid out,
    /// which is worth remembering across a restart. Whether AutoPaste is *on*
    /// is deliberately not stored anywhere at all — see ADR-031.
    #[serde(default = "default_capture_delimiter")]
    pub capture_delimiter: String,
}

fn default_color() -> String {
    "yellow".to_string()
}

fn default_font_size() -> u32 {
    15
}

fn default_note_width() -> i32 {
    360
}

fn default_note_height() -> i32 {
    300
}

fn default_autosave_interval_ms() -> u32 {
    300
}

fn default_theme() -> String {
    DEFAULT_THEME.to_string()
}

fn default_ui_scale_percent() -> u16 {
    DEFAULT_UI_SCALE_PERCENT
}

fn default_capture_delimiter() -> String {
    DEFAULT_CAPTURE_DELIMITER.to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_color: default_color(),
            default_font_size: default_font_size(),
            default_width: default_note_width(),
            default_height: default_note_height(),
            autosave_interval_ms: default_autosave_interval_ms(),
            theme: default_theme(),
            ui_scale_percent: default_ui_scale_percent(),
            capture_delimiter: default_capture_delimiter(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigLoadOutcome {
    Missing(AppConfig),
    Valid(AppConfig),
    CorruptedRecovered {
        value: AppConfig,
        quarantine_path: PathBuf,
        error: String,
    },
    CorruptedPreservationFailed {
        error: String,
    },
    ReadFailed(String),
}

impl ConfigLoadOutcome {
    pub fn value(&self) -> AppConfig {
        match self {
            ConfigLoadOutcome::Missing(c) => c.clone(),
            ConfigLoadOutcome::Valid(c) => c.clone(),
            ConfigLoadOutcome::CorruptedRecovered { value, .. } => value.clone(),
            ConfigLoadOutcome::CorruptedPreservationFailed { .. } => AppConfig::default(),
            ConfigLoadOutcome::ReadFailed(_) => AppConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load_from_file(path: &Path) -> Self {
        Self::load_detailed(path).value()
    }

    pub fn load_detailed(path: &Path) -> ConfigLoadOutcome {
        if !path.exists() {
            let config = Self::default();
            if let Err(error) = config.save_to_file(path) {
                eprintln!("Failed to persist default configuration: {error}");
            }
            return ConfigLoadOutcome::Missing(config);
        }

        let raw_bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Failed to read configuration at {}: {e}", path.display());
                return ConfigLoadOutcome::ReadFailed(e.to_string());
            }
        };

        let content_str = match std::str::from_utf8(&raw_bytes) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Configuration at {} is not valid UTF-8: {e}", path.display());
                return Self::handle_corruption(path, &raw_bytes, &e.to_string());
            }
        };

        match toml::from_str::<AppConfig>(content_str) {
            Ok(config) => ConfigLoadOutcome::Valid(config),
            Err(parse_err) => {
                eprintln!("Configuration at {} is malformed: {parse_err}", path.display());
                Self::handle_corruption(path, &raw_bytes, &parse_err.to_string())
            }
        }
    }

    fn handle_corruption(path: &Path, raw_bytes: &[u8], reason: &str) -> ConfigLoadOutcome {
        match crate::quarantine::quarantine_corrupted_file(path, raw_bytes) {
            Ok(quarantine_path) => {
                eprintln!(
                    "Warning: Configuration at {} was corrupted ({reason}). Original preserved at {}",
                    path.display(),
                    quarantine_path.display()
                );
                ConfigLoadOutcome::CorruptedRecovered {
                    value: Self::default(),
                    quarantine_path,
                    error: reason.to_string(),
                }
            }
            Err(quarantine_err) => {
                eprintln!(
                    "Error: Configuration at {} is corrupted ({reason}) and preservation failed: {quarantine_err}. Original file will NOT be overwritten.",
                    path.display()
                );
                ConfigLoadOutcome::CorruptedPreservationFailed {
                    error: format!("{reason}; preservation failed: {quarantine_err}"),
                }
            }
        }
    }

    /// Writes the configuration under the same commit-point rule as a note and
    /// the window state: see [`crate::atomic_file::write_atomic`].
    ///
    /// The file is replaced whole or not at all.
    /// Invariant (R-006): If an existing file at `path` is currently malformed or
    /// corrupted, it must be preserved via quarantine before replacement.
    /// If preservation fails, fail-closed: do not overwrite the corrupted file.
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            crate::permissions::create_private_dir_all(parent)
                .map_err(|e| format!("Failed to create configuration directory: {e}"))?;
        }

        // Before replacing an existing file, if it is currently malformed/corrupted,
        // ensure it has been safely quarantined so unparsed data is never lost!
        if path.exists() {
            if let Ok(existing_bytes) = fs::read(path) {
                let is_valid = std::str::from_utf8(&existing_bytes)
                    .ok()
                    .and_then(|s| toml::from_str::<toml::Value>(s).ok())
                    .is_some();
                if !is_valid {
                    crate::quarantine::quarantine_corrupted_file(path, &existing_bytes).map_err(
                        |e| {
                            format!(
                                "Refusing to overwrite corrupted configuration at {}: preservation failed: {e}",
                                path.display()
                            )
                        },
                    )?;
                }
            }
        }

        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config to TOML: {e}"))?;

        write_atomic(path, toml_str.as_bytes(), "the configuration")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autopaste::delimiter_name;
    use tempfile::tempdir;

    #[test]
    fn a_configuration_written_before_themes_existed_follows_the_system() {
        let legacy = concat!(
            "default_color = \"blue\"\n",
            "default_font_size = 15\n",
            "default_width = 360\n",
            "default_height = 300\n",
            "autosave_interval_ms = 300\n",
        );

        let parsed: AppConfig = toml::from_str(legacy).expect("legacy config must keep loading");
        assert_eq!(parsed.default_color, "blue");
        assert_eq!(parsed.theme, DEFAULT_THEME);
        assert_eq!(parsed.theme, "system");
    }

    #[test]
    fn an_unknown_theme_degrades_to_following_the_system() {
        for unknown in ["", "solarized", "DARK", "light "] {
            assert_eq!(theme_name(unknown), DEFAULT_THEME);
        }
        for name in THEMES {
            assert_eq!(theme_name(name), *name);
        }
    }

    #[test]
    fn a_configuration_is_replaced_whole_or_not_at_all() {
        // 3.5R. The configuration was written straight over the real file with
        // `File::create`, which truncates first. A crash or a full disk part
        // way through left a half-written `config.toml`, and loading falls
        // back to the defaults without a word — silently resetting the theme
        // and every other preference. It is now written the way the notes and
        // the state are: to a temp file, then renamed into place.
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");

        let config = AppConfig {
            theme: "dark".to_string(),
            default_color: "blue".to_string(),
            ..AppConfig::default()
        };
        config.save_to_file(&path).expect("save the configuration");

        // A directory where the file belongs: the rename cannot land, and the
        // configuration already stored must survive untouched.
        let blocked = tmp.path().join("blocked.toml");
        fs::create_dir(&blocked).expect("occupy the config path");
        AppConfig::default()
            .save_to_file(&blocked)
            .expect_err("a save that cannot be completed must be reported");

        let reloaded = AppConfig::load_from_file(&path);
        assert_eq!(reloaded.theme, "dark");
        assert_eq!(reloaded.default_color, "blue");

        // Nothing is left lying beside it.
        let debris: Vec<String> = fs::read_dir(tmp.path())
            .expect("read the directory")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".tmp."))
            .collect();
        assert!(
            debris.is_empty(),
            "a save left temp files behind: {debris:?}"
        );
    }

    #[test]
    fn a_configuration_written_before_autopaste_existed_uses_a_blank_line() {
        // Every configuration on disk predates this phase. None of them may
        // need migrating, and none of them may lose a preference by loading.
        let legacy = concat!(
            "default_color = \"blue\"\n",
            "default_font_size = 15\n",
            "default_width = 360\n",
            "default_height = 300\n",
            "autosave_interval_ms = 300\n",
            "theme = \"dark\"\n",
        );

        let parsed: AppConfig = toml::from_str(legacy).expect("legacy config must keep loading");
        assert_eq!(parsed.default_color, "blue");
        assert_eq!(parsed.theme, "dark");
        assert_eq!(parsed.capture_delimiter, DEFAULT_CAPTURE_DELIMITER);
        assert_eq!(parsed.capture_delimiter, "blankLine");
    }

    #[test]
    fn the_capture_delimiter_survives_a_restart_and_nothing_else_does() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");

        for delimiter in crate::autopaste::CAPTURE_DELIMITERS {
            let config = AppConfig {
                capture_delimiter: (*delimiter).to_string(),
                theme: "dark".to_string(),
                ..AppConfig::default()
            };
            config.save_to_file(&path).expect("save config");

            let reloaded = AppConfig::load_from_file(&path);
            assert_eq!(reloaded.capture_delimiter, *delimiter);
            assert_eq!(reloaded.theme, "dark");
        }

        // Whether AutoPaste was on is not a thing this file can say. Nothing
        // reactivates a clipboard watcher across a restart.
        let written = fs::read_to_string(&path).expect("read the configuration");
        for forbidden in ["autopaste_active", "autopaste_enabled", "capture_target"] {
            assert!(!written.contains(forbidden), "config carries {forbidden:?}");
        }
        assert!(!written.contains("true"));
    }

    #[test]
    fn a_corrupted_delimiter_degrades_instead_of_losing_the_file() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            concat!(
                "default_color = \"green\"\n",
                "default_font_size = 15\n",
                "default_width = 360\n",
                "default_height = 300\n",
                "autosave_interval_ms = 300\n",
                "theme = \"light\"\n",
                "capture_delimiter = \"a-regex-nobody-supports\"\n",
            ),
        )
        .expect("write a hand-edited configuration");

        let loaded = AppConfig::load_from_file(&path);
        // The unknown value is kept in the struct as read; what resolves it is
        // the same allowlist the theme uses, at the point of use.
        assert_eq!(delimiter_name(&loaded.capture_delimiter), "blankLine");
        // ...and the rest of the file survived rather than resetting.
        assert_eq!(loaded.default_color, "green");
        assert_eq!(loaded.theme, "light");
    }

    #[test]
    fn the_theme_survives_a_restart() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");

        for theme in THEMES {
            let config = AppConfig {
                theme: (*theme).to_string(),
                ..AppConfig::default()
            };
            config.save_to_file(&path).expect("save config");

            let reloaded = AppConfig::load_from_file(&path);
            assert_eq!(reloaded.theme, *theme);
            // A global preference: it never leaks into a note's own settings.
            assert_eq!(reloaded.default_color, config.default_color);
        }
    }

    #[test]
    fn interface_scale_defaults_for_old_configs_and_normalizes_bad_values() {
        let legacy = concat!(
            "default_color = \"blue\"\n",
            "default_font_size = 15\n",
            "default_width = 360\n",
            "default_height = 300\n",
            "autosave_interval_ms = 300\n",
            "theme = \"dark\"\n",
        );
        let parsed: AppConfig = toml::from_str(legacy).expect("old config");
        assert_eq!(parsed.ui_scale_percent, DEFAULT_UI_SCALE_PERCENT);

        for (stored, expected) in [
            ("90", 90),
            ("110", 110),
            ("120", 120),
            ("140", 140),
            ("160", 160),
            ("0", MIN_UI_SCALE_PERCENT),
            ("10000", MAX_UI_SCALE_PERCENT),
            ("\"large\"", DEFAULT_UI_SCALE_PERCENT),
        ] {
            let source = format!("ui_scale_percent = {stored}\n");
            let config: AppConfig = toml::from_str(&source).expect("field is isolated");
            assert_eq!(config.ui_scale_percent, expected, "stored {stored}");
        }
    }

    #[test]
    fn interface_scale_survives_an_atomic_restart_without_becoming_note_state() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        let config = AppConfig {
            ui_scale_percent: 140,
            ..AppConfig::default()
        };
        config.save_to_file(&path).expect("save config");
        assert_eq!(AppConfig::load_from_file(&path).ui_scale_percent, 140);

        let written = fs::read_to_string(path).expect("read config");
        assert!(written.contains("ui_scale_percent = 140"));
        assert!(!written.contains("zoom_percent"));
        assert!(!written.contains("updated_at"));
    }
}
