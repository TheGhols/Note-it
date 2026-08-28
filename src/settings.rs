use crate::atomic_file::write_atomic;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Interface themes, in the order the menu offers them.
///
/// A theme dresses the application's own chrome — menus, popovers, borders,
/// focus rings — and never the paper a note is written on: a yellow note stays
/// yellow under the dark theme, and a black one stays black under the light
/// one.
pub const THEMES: &[&str] = &["system", "light", "dark"];
pub const DEFAULT_THEME: &str = "system";

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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_color: default_color(),
            default_font_size: default_font_size(),
            default_width: default_note_width(),
            default_height: default_note_height(),
            autosave_interval_ms: default_autosave_interval_ms(),
            theme: default_theme(),
        }
    }
}

impl AppConfig {
    pub fn load_from_file(path: &Path) -> Self {
        if !path.exists() {
            let config = Self::default();
            if let Err(error) = config.save_to_file(path) {
                eprintln!("Failed to persist default configuration: {error}");
            }
            return config;
        }

        fs::read_to_string(path)
            .ok()
            .and_then(|content| toml::from_str::<AppConfig>(&content).ok())
            .unwrap_or_default()
    }

    /// Writes the configuration under the same commit-point rule as a note and
    /// the window state: see [`crate::atomic_file::write_atomic`].
    ///
    /// This used to write straight over the real file, which truncates it
    /// first, so an interrupted write left a half-written `config.toml` — and
    /// loading falls back to the defaults without a word, silently resetting
    /// every preference. The file is now replaced whole or not at all.
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create configuration directory: {e}"))?;
        }

        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config to TOML: {e}"))?;

        write_atomic(path, toml_str.as_bytes(), "the configuration")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
