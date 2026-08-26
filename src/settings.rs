use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_color: default_color(),
            default_font_size: default_font_size(),
            default_width: default_note_width(),
            default_height: default_note_height(),
            autosave_interval_ms: default_autosave_interval_ms(),
        }
    }
}

impl AppConfig {
    pub fn load_from_file(path: &Path) -> Self {
        if !path.exists() {
            let config = Self::default();
            let _ = config.save_to_file(path);
            return config;
        }

        fs::read_to_string(path)
            .ok()
            .and_then(|content| toml::from_str::<AppConfig>(&content).ok())
            .unwrap_or_default()
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
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
