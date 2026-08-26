use crate::model::NoteDocument;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StorageManager {
    notes_dir: PathBuf,
    config_dir: PathBuf,
    state_dir: PathBuf,
    runtime_dir: PathBuf,
}

impl StorageManager {
    pub fn new() -> Result<Self, String> {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("note-it");
        let notes_dir = data_dir.join("notes");

        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("note-it");

        let state_dir = dirs::state_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/state"))
            .join("note-it");

        let runtime_dir = dirs::runtime_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("note-it");

        let manager = Self {
            notes_dir,
            config_dir,
            state_dir,
            runtime_dir,
        };

        manager.ensure_directories()?;
        Ok(manager)
    }

    #[allow(dead_code)]
    pub fn with_custom_paths(
        notes_dir: PathBuf,
        config_dir: PathBuf,
        state_dir: PathBuf,
        runtime_dir: PathBuf,
    ) -> Result<Self, String> {
        let manager = Self {
            notes_dir,
            config_dir,
            state_dir,
            runtime_dir,
        };
        manager.ensure_directories()?;
        Ok(manager)
    }

    pub fn ensure_directories(&self) -> Result<(), String> {
        fs::create_dir_all(&self.notes_dir)
            .map_err(|e| format!("Failed to create notes directory: {e}"))?;
        fs::create_dir_all(&self.config_dir)
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
        fs::create_dir_all(&self.state_dir)
            .map_err(|e| format!("Failed to create state directory: {e}"))?;
        fs::create_dir_all(&self.runtime_dir)
            .map_err(|e| format!("Failed to create runtime directory: {e}"))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn notes_dir(&self) -> &Path {
        &self.notes_dir
    }

    pub fn state_file_path(&self) -> PathBuf {
        self.state_dir.join("state.json")
    }

    pub fn config_file_path(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn note_path(&self, id: &Uuid) -> PathBuf {
        self.notes_dir.join(format!("{id}.md"))
    }

    pub fn save_note_atomic(&self, doc: &NoteDocument) -> Result<PathBuf, String> {
        let serialized = doc.serialize()?;
        let target_path = self.note_path(&doc.metadata.id);

        let temp_filename = format!(
            ".tmp.{}.{}",
            doc.metadata.id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let temp_path = self.notes_dir.join(temp_filename);

        {
            let mut file = File::create(&temp_path)
                .map_err(|e| format!("Failed to create temp file {}: {e}", temp_path.display()))?;
            file.write_all(serialized.as_bytes())
                .map_err(|e| format!("Failed to write to temp file: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("Failed to sync temp file: {e}"))?;
        }

        fs::rename(&temp_path, &target_path)
            .map_err(|e| format!("Failed to rename temp file to target: {e}"))?;
        File::open(&self.notes_dir)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| format!("Failed to sync notes directory: {e}"))?;

        Ok(target_path)
    }

    pub fn load_note(&self, id: &Uuid) -> Result<NoteDocument, String> {
        let path = self.note_path(id);
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read note {}: {e}", path.display()))?;
        NoteDocument::parse(&content)
    }

    pub fn list_notes(&self) -> Result<Vec<Uuid>, String> {
        let entries = fs::read_dir(&self.notes_dir)
            .map_err(|e| format!("Failed to read notes directory: {e}"))?;

        let mut note_ids = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(id) = Uuid::parse_str(stem) {
                        note_ids.push(id);
                    }
                }
            }
        }

        Ok(note_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_storage_atomic_save_and_load() {
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let config_dir = tmp.path().join("config");
        let state_dir = tmp.path().join("state");
        let runtime_dir = tmp.path().join("runtime");

        let manager =
            StorageManager::with_custom_paths(notes_dir, config_dir, state_dir, runtime_dir)
                .expect("Init storage manager");

        let mut doc = NoteDocument::new_empty();
        doc.content = "Atomic save content test".to_string();

        let path = manager.save_note_atomic(&doc).expect("Atomic save");
        assert!(path.exists());

        let loaded = manager.load_note(&doc.metadata.id).expect("Load note");
        assert_eq!(loaded.content, doc.content);
        assert_eq!(loaded.metadata.id, doc.metadata.id);

        let list = manager.list_notes().expect("List notes");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], doc.metadata.id);
    }
}
