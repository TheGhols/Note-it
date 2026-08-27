use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
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

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create configuration directory: {e}"))?;
        }

        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config to TOML: {e}"))?;

        let mut file = File::create(path)
            .map_err(|e| format!("Failed to write config file {}: {e}", path.display()))?;
        file.write_all(toml_str.as_bytes())
            .map_err(|e| format!("Failed to write config file content: {e}"))?;

        Ok(())
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
