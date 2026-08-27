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

    /// Writes a note, or leaves the one already on disk exactly as it was.
    ///
    /// Either the rename lands and the note is the new one, or it does not and
    /// the note is still the old one; there is no state in between. A failure
    /// also takes the half-written temp file with it, because nothing else
    /// ever collects one and it would sit in the notes directory forever.
    ///
    /// The result is what tells the caller which of the two happened, so a
    /// caller must not treat a document as stored until this has returned
    /// `Ok`.
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

        match self.write_then_rename(&serialized, &temp_path, &target_path) {
            Ok(()) => Ok(target_path),
            Err(error) => {
                // Best effort: if this cannot be removed either, the save has
                // already failed and the error worth reporting is that one.
                let _ = fs::remove_file(&temp_path);
                Err(error)
            }
        }
    }

    fn write_then_rename(
        &self,
        serialized: &str,
        temp_path: &Path,
        target_path: &Path,
    ) -> Result<(), String> {
        {
            let mut file = File::create(temp_path)
                .map_err(|e| format!("Failed to create temp file {}: {e}", temp_path.display()))?;
            file.write_all(serialized.as_bytes())
                .map_err(|e| format!("Failed to write to temp file: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("Failed to sync temp file: {e}"))?;
        }

        fs::rename(temp_path, target_path)
            .map_err(|e| format!("Failed to rename temp file to target: {e}"))?;
        File::open(&self.notes_dir)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| format!("Failed to sync notes directory: {e}"))
    }

    pub fn load_note(&self, id: &Uuid) -> Result<NoteDocument, String> {
        let path = self.note_path(id);
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read note {}: {e}", path.display()))?;
        NoteDocument::parse(&content)
    }

    /// Note identifiers ordered by last write, most recent first.
    ///
    /// Used to decide which note to bring back when every note has been
    /// closed. The modification time comes from the file itself, so nothing
    /// has to be parsed and the ordering still reflects the last save.
    pub fn list_notes_by_recency(&self) -> Result<Vec<Uuid>, String> {
        let entries = fs::read_dir(&self.notes_dir)
            .map_err(|e| format!("Failed to read notes directory: {e}"))?;

        let mut notes: Vec<(Uuid, std::time::SystemTime)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|stem| Uuid::parse_str(stem).ok())
            else {
                continue;
            };
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            notes.push((id, modified));
        }

        // Newest first; ties fall back to the identifier so the order is stable.
        notes.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        Ok(notes.into_iter().map(|(id, _)| id).collect())
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

        let list = manager.list_notes_by_recency().expect("List notes");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], doc.metadata.id);
    }

    #[test]
    fn test_scale_listing_200_notes_without_ui_instantiation() {
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let config_dir = tmp.path().join("config");
        let state_dir = tmp.path().join("state");
        let runtime_dir = tmp.path().join("runtime");

        let manager =
            StorageManager::with_custom_paths(notes_dir, config_dir, state_dir, runtime_dir)
                .expect("Init storage manager");

        let count = 200;
        let mut ids = Vec::with_capacity(count);
        for i in 0..count {
            let mut doc = NoteDocument::new_empty();
            doc.content = format!("Scale test note {i}");
            manager.save_note_atomic(&doc).expect("save note");
            ids.push(doc.metadata.id);
        }

        let listed = manager.list_notes_by_recency().expect("list notes");
        assert_eq!(listed.len(), count);

        // Verify that in a background scenario, filtering is_open produces zero active notes
        // if no state is marked open, or only those marked open
        let mut state = crate::state::AppState::default();
        // Mark only 2 notes as is_open=true
        state.notes.insert(
            ids[0],
            crate::state::NoteWindowState {
                is_open: true,
                ..Default::default()
            },
        );
        state.notes.insert(
            ids[1],
            crate::state::NoteWindowState {
                is_open: true,
                ..Default::default()
            },
        );
        for id in &ids[2..] {
            state.notes.insert(
                *id,
                crate::state::NoteWindowState {
                    is_open: false,
                    ..Default::default()
                },
            );
        }

        let open_notes: Vec<Uuid> = listed
            .into_iter()
            .filter(|id| state.notes.get(id).map(|s| s.is_open).unwrap_or(true))
            .collect();

        assert_eq!(open_notes.len(), 2);
        assert!(open_notes.contains(&ids[0]));
        assert!(open_notes.contains(&ids[1]));
    }

    #[test]
    fn notes_are_listed_with_the_most_recently_saved_first() {
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let mut ids = Vec::new();
        for index in 0..3 {
            let mut doc = NoteDocument::new_empty();
            doc.content = format!("note {index}");
            manager.save_note_atomic(&doc).expect("save note");
            ids.push(doc.metadata.id);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let by_recency = manager.list_notes_by_recency().expect("list by recency");
        assert_eq!(by_recency.len(), 3);
        assert_eq!(by_recency[0], ids[2], "the newest save must come first");
        assert_eq!(by_recency[2], ids[0]);

        // Saving the oldest note again moves it to the front.
        let mut refreshed = manager.load_note(&ids[0]).expect("load oldest");
        refreshed.content = "touched".to_string();
        std::thread::sleep(std::time::Duration::from_millis(20));
        manager.save_note_atomic(&refreshed).expect("resave");

        let after = manager.list_notes_by_recency().expect("list again");
        assert_eq!(after[0], ids[0]);
    }

    #[test]
    fn a_save_that_cannot_be_completed_leaves_no_temp_file_behind() {
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let mut doc = NoteDocument::new_empty();
        doc.content = "conteúdo que não chega ao disco".to_string();

        // A directory sitting where the note file belongs: the temp file is
        // written and the rename onto it fails. That is path resolution rather
        // than a permission bit, so it fails for every user, root included.
        fs::create_dir(manager.note_path(&doc.metadata.id)).expect("occupy the note path");

        manager
            .save_note_atomic(&doc)
            .expect_err("renaming a file over a directory must fail");

        let debris: Vec<String> = fs::read_dir(manager.notes_dir())
            .expect("read the notes directory")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".tmp."))
            .collect();
        assert!(
            debris.is_empty(),
            "a failed save left temp files behind: {debris:?}"
        );
    }

    #[test]
    fn listing_by_recency_ignores_unrelated_files() {
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let manager = StorageManager::with_custom_paths(
            notes_dir.clone(),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let doc = NoteDocument::new_empty();
        manager.save_note_atomic(&doc).expect("save note");
        fs::write(notes_dir.join("not-a-note.txt"), "ignored").expect("write stray file");
        fs::write(notes_dir.join("not-a-uuid.md"), "ignored").expect("write stray note");

        assert_eq!(
            manager.list_notes_by_recency().expect("list"),
            vec![doc.metadata.id]
        );
    }
}
