use crate::atomic_file::write_atomic;
use crate::model::NoteDocument;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StorageManager {
    notes_dir: PathBuf,
    config_dir: PathBuf,
    state_dir: PathBuf,
    runtime_dir: PathBuf,
    /// Makes the post-commit directory sync fail, so the one failure that
    /// happens *after* the rename can be exercised. It cannot be provoked from
    /// outside the process: once the rename has returned, nothing a test can do
    /// to the filesystem reaches back into the sync that follows it. Compiled
    /// out of every real build.
    #[cfg(test)]
    fail_directory_sync: bool,
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
            #[cfg(test)]
            fail_directory_sync: false,
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
            #[cfg(test)]
            fail_directory_sync: false,
        };
        manager.ensure_directories()?;
        Ok(manager)
    }

    /// The same store, reached through a handle whose post-commit directory
    /// sync always fails.
    #[cfg(test)]
    pub(crate) fn failing_directory_sync(mut self) -> Self {
        self.fail_directory_sync = true;
        self
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
    /// The commit point is the rename: see [`crate::atomic_file::write_atomic`]
    /// for the rule this, the window state and the configuration all share.
    pub fn save_note_atomic(&self, doc: &NoteDocument) -> Result<PathBuf, String> {
        let serialized = doc.serialize()?;
        let target_path = self.note_path(&doc.metadata.id);
        let what = format!("note {}", doc.metadata.id);

        #[cfg(test)]
        if self.fail_directory_sync {
            crate::atomic_file::write_atomic_with_failing_sync(
                &target_path,
                serialized.as_bytes(),
                &what,
            )?;
            return Ok(target_path);
        }

        write_atomic(&target_path, serialized.as_bytes(), &what)?;
        Ok(target_path)
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

    /// Every note's own text, newest first, ready to be searched.
    ///
    /// Reads the files and splits the front matter off; the YAML is never
    /// parsed, because search wants what a reader wrote and nothing Note-it
    /// records about it. That also keeps a scan of a thousand notes to a
    /// thousand reads rather than a thousand reads and a thousand YAML parses.
    ///
    /// The order is [`list_notes_by_recency`](Self::list_notes_by_recency) —
    /// the store's existing rule, which since Phase 3.4R follows the last real
    /// edit. Reusing it means a reader meets one idea of "most recent"
    /// everywhere in the application rather than two that disagree.
    ///
    /// A note that has vanished or cannot be read is skipped rather than
    /// failing the whole scan: one unreadable file must not stop search
    /// working. Nothing here writes, so searching never touches a note.
    pub fn read_note_bodies_by_recency(&self, limit: usize) -> Vec<(Uuid, String)> {
        let ids = match self.list_notes_by_recency() {
            Ok(ids) => ids,
            Err(error) => {
                eprintln!("Failed to list notes for search: {error}");
                return Vec::new();
            }
        };

        ids.into_iter()
            .take(limit)
            .filter_map(|id| {
                let raw = fs::read_to_string(self.note_path(&id)).ok()?;
                Some((id, NoteDocument::body_of(&raw).to_string()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Temp files a save should never leave behind.
    fn temp_debris_in(notes_dir: &Path) -> Vec<String> {
        fs::read_dir(notes_dir)
            .expect("read the notes directory")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".tmp."))
            .collect()
    }

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

        let debris = temp_debris_in(manager.notes_dir());
        assert!(
            debris.is_empty(),
            "a failed save left temp files behind: {debris:?}"
        );
    }

    #[test]
    fn a_directory_sync_that_fails_after_the_rename_is_still_a_completed_save() {
        // 3.4R.2. The one failure that happens *past* the commit point: the
        // rename already replaced the file, so the save did happen, and only
        // its durability is in doubt. Reporting it as a failed save would
        // leave every caller describing a note the file no longer holds.
        let tmp = tempdir().expect("tempdir");
        let store = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let mut doc = NoteDocument::new_empty();
        doc.content = "conteúdo A".to_string();
        store.save_note_atomic(&doc).expect("initial save");
        let id = doc.metadata.id;

        let unsyncable = store.clone().failing_directory_sync();
        doc.content = "conteúdo B".to_string();
        let path = unsyncable
            .save_note_atomic(&doc)
            .expect("a rename that succeeded is a save, whatever the sync did");

        // The rename really did replace the file...
        assert_eq!(path, store.note_path(&id));
        assert_eq!(store.load_note(&id).expect("reload").content, "conteúdo B");
        // ...and nothing was left behind.
        assert!(temp_debris_in(store.notes_dir()).is_empty());

        // The next successful save syncs the directory again, which is what
        // makes the earlier rename durable too — there is no missed sync to
        // remember and nothing to retry by hand.
        doc.content = "conteúdo C".to_string();
        store
            .save_note_atomic(&doc)
            .expect("a later save syncs normally");
        assert_eq!(store.load_note(&id).expect("reload").content, "conteúdo C");
        assert!(temp_debris_in(store.notes_dir()).is_empty());
    }

    #[test]
    fn search_reads_the_note_and_never_its_front_matter() {
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let mut doc = NoteDocument::new_empty();
        doc.content = "# Biópsia hepática\n\ncorpo da nota".to_string();
        manager.save_note_atomic(&doc).expect("save");

        let bodies = manager.read_note_bodies_by_recency(10);
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].0, doc.metadata.id);
        assert_eq!(bodies[0].1, "# Biópsia hepática\n\ncorpo da nota");

        // The stored file really does carry the metadata this skipped.
        let raw = fs::read_to_string(manager.note_path(&doc.metadata.id)).expect("read");
        assert!(raw.contains("note_it:"));
        assert!(raw.contains("created_at:"));
        for internal in ["note_it", "created_at", "updated_at", "paper_type"] {
            assert!(
                !bodies[0].1.contains(internal),
                "{internal} reached the searchable body"
            );
        }
    }

    #[test]
    fn reading_bodies_survives_a_note_that_vanished_or_cannot_be_parsed() {
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let manager = StorageManager::with_custom_paths(
            notes_dir.clone(),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let mut good = NoteDocument::new_empty();
        good.content = "nota legível".to_string();
        manager.save_note_atomic(&good).expect("save");

        // A file with an id but no front matter at all: still a note, and its
        // whole text is searchable.
        let orphan = Uuid::new_v4();
        fs::write(manager.note_path(&orphan), "sem front matter\n").expect("write orphan");

        // ...and a directory where a note should be, which cannot be read.
        let broken = Uuid::new_v4();
        fs::create_dir(manager.note_path(&broken)).expect("occupy a note path");

        let bodies = manager.read_note_bodies_by_recency(10);
        let ids: Vec<Uuid> = bodies.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&good.metadata.id));
        assert!(ids.contains(&orphan));
        assert!(!ids.contains(&broken), "an unreadable note is skipped");
        assert!(bodies.iter().any(|(_, body)| body == "sem front matter"));
    }

    #[test]
    fn searching_a_thousand_notes_is_fast_and_writes_nothing() {
        // The evidence behind having no index. A thousand notes is far more
        // than a post-it application accumulates, and the whole scan — listing,
        // reading, folding, matching, snippets — is measured end to end.
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let manager = StorageManager::with_custom_paths(
            notes_dir.clone(),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let count = 1_000;
        let filler = "Texto de contexto para dar corpo à nota. ".repeat(20);
        for index in 0..count {
            let mut doc = NoteDocument::new_empty();
            doc.content = if index % 100 == 0 {
                format!("# Biópsia hepática {index}\n\n{filler}\nagulha de punção\n")
            } else {
                format!("# Nota {index}\n\n{filler}\n")
            };
            manager.save_note_atomic(&doc).expect("save note");
        }

        let before: Vec<_> = fs::read_dir(&notes_dir)
            .expect("read notes dir")
            .flatten()
            .map(|entry| {
                (
                    entry.file_name(),
                    entry.metadata().and_then(|m| m.modified()).ok(),
                )
            })
            .collect();

        let mut timings = Vec::new();
        for query in ["biopsia", "nota", "inexistente-xyz", "punção"] {
            let started = std::time::Instant::now();
            let bodies = manager.read_note_bodies_by_recency(5_000);
            let results = crate::search::search_notes(
                query,
                bodies.iter().map(|(id, body)| (*id, body.as_str())),
            );
            let elapsed = started.elapsed();
            timings.push((query, elapsed, results.len()));
        }

        for (query, elapsed, hits) in &timings {
            println!("search {count} notes for {query:?}: {elapsed:?} ({hits} hits)");
            assert!(
                *elapsed < std::time::Duration::from_secs(2),
                "searching {count} notes for {query:?} took {elapsed:?}"
            );
        }

        // `biopsia` finds the ten notes that have it, accents and case folded.
        assert_eq!(timings[0].2, 10);
        // `nota` is in every one, and the result list is still capped.
        assert_eq!(timings[1].2, crate::search::MAX_RESULTS);
        assert_eq!(timings[2].2, 0);
        assert_eq!(timings[3].2, 10);

        // Searching is reading: not one file was written or even touched.
        let after: Vec<_> = fs::read_dir(&notes_dir)
            .expect("read notes dir")
            .flatten()
            .map(|entry| {
                (
                    entry.file_name(),
                    entry.metadata().and_then(|m| m.modified()).ok(),
                )
            })
            .collect();
        let mut before_sorted = before;
        let mut after_sorted = after;
        before_sorted.sort();
        after_sorted.sort();
        assert_eq!(before_sorted, after_sorted, "a search modified the store");
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
