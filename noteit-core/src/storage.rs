use crate::atomic_file::write_atomic;
use crate::backup::{self, BackupSources, SnapshotKind};
use crate::diagnostics;
use crate::metadata::{
    semantic_identity, MetadataCatalog, PropertyKeyCatalogEntry, TagCatalogEntry,
};
use crate::model::{NoteDocument, NoteFrontMatterWrapper};
use crate::study::{self, Rating, StudyState};
use crate::trash::{self, TrashEntry};
use chrono::{DateTime, Duration, Utc};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Hard ceiling for the front matter needed by recency and metadata catalogs.
///
/// Valid Tags + Properties fit comfortably below this even at every V1 limit.
/// The reader stops at the real closing delimiter and never reads the body.
/// A hand-written header beyond the ceiling falls back to `mtime` for recency
/// rather than turning an unbounded file into listing work.
pub const MAX_FRONT_MATTER_BYTES: usize = 256 * 1024;

/// One note as the ordering sees it: which note, and when it was last written
/// in.
struct Listed {
    id: Uuid,
    edited_at: SystemTime,
}

/// Newest first; ties fall back to the identifier, so the order is stable and
/// never depends on the order the directory happened to hand the files over.
fn newest_first(notes: &mut [Listed]) {
    notes.sort_by(|left, right| {
        right
            .edited_at
            .cmp(&left.edited_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}

/// When the note's own front matter says its text last changed.
///
/// `None` for a file with no front matter, front matter that cannot be read,
/// or a note written before the field existed. Every one of those is a note
/// whose ordering falls back to the file's modification time — the rule every
/// note followed before there was a field to read.
fn recorded_edit_time(raw: &str) -> Option<DateTime<Utc>> {
    serde_yaml::from_str::<NoteFrontMatterWrapper>(raw)
        .ok()?
        .note_it
        .updated_at
}

/// Reads only the YAML between exact front-matter delimiter lines.
fn read_front_matter(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut consumed = reader.read_line(&mut line).ok()?;
    if line.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }

    let mut yaml = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).ok()?;
        if read == 0 {
            return None;
        }
        consumed = consumed.checked_add(read)?;
        if consumed > MAX_FRONT_MATTER_BYTES {
            return None;
        }
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Some(yaml);
        }
        yaml.push_str(&line);
    }
}

/// When the last automatic snapshot happened, as this process knows it.
///
/// Read from the backups directory once and kept in memory afterwards, so the
/// twenty-four hour question costs nothing to ask again. That matters because
/// it is asked before every persistent mutation, and an autosave happens every
/// few hundred milliseconds while someone is typing: the check has to be free
/// when the answer is "not yet", which it is for all but one save a day.
///
/// Nothing here wakes up on its own. There is no timer and no thread; a
/// backup only ever happens because something was about to be written.
#[derive(Debug, Default)]
struct BackupSchedule {
    /// False until the backups directory has been read this session.
    loaded: bool,
    last_success: Option<DateTime<Utc>>,
    /// The last attempt of any outcome, so a store whose backups cannot be
    /// written is not retried on every keystroke.
    last_attempt: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct StorageManager {
    notes_dir: PathBuf,
    /// Deleted notes, waiting to be restored. A sibling of `notes_dir`, so the
    /// move between them is always within one filesystem.
    trash_dir: PathBuf,
    /// Local snapshots. Never a backup source: see [`crate::backup`].
    backups_dir: PathBuf,
    /// Images the notes hold, one directory per note. A sibling of `notes/`
    /// and `trash/`, which is what lets a note's own `../assets/…` reference
    /// resolve the same way from either of them.
    assets_dir: PathBuf,
    config_dir: PathBuf,
    state_dir: PathBuf,
    runtime_dir: PathBuf,
    /// Shared by every clone of this handle, because the windows each hold one
    /// and there is only one store between them.
    backup_schedule: Rc<RefCell<BackupSchedule>>,
    /// Makes the post-commit directory sync fail, so the one failure that
    /// happens *after* the rename can be exercised. It cannot be provoked from
    /// outside the process: once the rename has returned, nothing a test can do
    /// to the filesystem reaches back into the sync that follows it. Compiled
    /// out of every real build.
    #[cfg(any(test, feature = "test-support"))]
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

        let manager = Self::assemble(notes_dir, config_dir, state_dir, runtime_dir);
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
        let manager = Self::assemble(notes_dir, config_dir, state_dir, runtime_dir);
        manager.ensure_directories()?;
        Ok(manager)
    }

    /// The trash and the backups are siblings of the notes directory, which is
    /// what the layout on disk already is: one `note-it` data directory holding
    /// `notes/`, `trash/` and `backups/`. Deriving them rather than passing
    /// them keeps the three from ever being configured apart, and keeps
    /// `notes/` and `trash/` on one filesystem, which the move between them
    /// depends on.
    fn assemble(
        notes_dir: PathBuf,
        config_dir: PathBuf,
        state_dir: PathBuf,
        runtime_dir: PathBuf,
    ) -> Self {
        let data_dir = notes_dir
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        Self {
            trash_dir: data_dir.join("trash"),
            backups_dir: data_dir.join("backups"),
            assets_dir: data_dir.join(crate::assets::ASSETS_DIRECTORY),
            notes_dir,
            config_dir,
            state_dir,
            runtime_dir,
            backup_schedule: Rc::new(RefCell::new(BackupSchedule::default())),
            #[cfg(any(test, feature = "test-support"))]
            fail_directory_sync: false,
        }
    }

    /// The same store, reached through a handle whose post-commit directory
    /// sync always fails.
    #[doc(hidden)]
    #[cfg(any(test, feature = "test-support"))]
    pub fn failing_directory_sync(mut self) -> Self {
        self.fail_directory_sync = true;
        self
    }

    pub fn ensure_directories(&self) -> Result<(), String> {
        fs::create_dir_all(&self.notes_dir)
            .map_err(|e| format!("Failed to create notes directory: {e}"))?;
        fs::create_dir_all(&self.trash_dir)
            .map_err(|e| format!("Failed to create trash directory: {e}"))?;
        fs::create_dir_all(&self.backups_dir)
            .map_err(|e| format!("Failed to create backups directory: {e}"))?;
        fs::create_dir_all(&self.assets_dir)
            .map_err(|e| format!("Failed to create assets directory: {e}"))?;
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

    #[allow(dead_code)]
    pub fn trash_dir(&self) -> &Path {
        &self.trash_dir
    }

    pub fn assets_dir(&self) -> &Path {
        &self.assets_dir
    }

    #[allow(dead_code)]
    pub fn backups_dir(&self) -> &Path {
        &self.backups_dir
    }

    pub fn state_file_path(&self) -> PathBuf {
        self.state_dir.join("state.json")
    }

    /// Durable review history. A sibling of notes and assets in XDG data,
    /// never mixed into the operational window state.
    pub fn study_file_path(&self) -> PathBuf {
        self.notes_dir
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("study.json")
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
    ///
    /// This is also where a day's first persistent change asks whether the
    /// store has been backed up recently, so the snapshot is taken *before* the
    /// change rather than after it: what a backup is for is going back to how
    /// things were, and the moment worth being able to go back to is the one
    /// before an edit. A backup that cannot be made is reported and the save
    /// goes ahead — a snapshot is an extra layer of safety, and turning its
    /// failure into a failed save would cost the edit the backup exists to
    /// protect.
    pub fn save_note_atomic(&self, doc: &NoteDocument) -> Result<PathBuf, String> {
        self.back_up_before_mutation();
        let serialized = doc.serialize()?;
        let target_path = self.note_path(&doc.metadata.id);
        let what = format!("note {}", doc.metadata.id);

        #[cfg(any(test, feature = "test-support"))]
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

    /// Moves a note into the trash, where it can be restored from.
    ///
    /// See [`crate::trash`] for the move itself. Everything the application
    /// does about a deleted note goes through here, so there is exactly one
    /// place that knows the trash is a directory next to `notes/`.
    pub fn move_note_to_trash(&self, id: &Uuid) -> Result<(), String> {
        // A deletion is the change most worth having a way back from.
        self.back_up_before_mutation();
        trash::move_to_trash(&self.notes_dir, &self.trash_dir, id, Utc::now())
    }

    /// Brings a note back out of the trash. Never over a live note: see
    /// [`crate::trash::restore_from_trash`].
    pub fn restore_note_from_trash(&self, id: &Uuid) -> Result<(), trash::RestoreError> {
        trash::restore_from_trash(&self.notes_dir, &self.trash_dir, id)
    }

    /// Everything in the trash, most recently deleted first. Reading only.
    pub fn list_trash(&self) -> Vec<TrashEntry> {
        trash::list_trash(&self.trash_dir)
    }

    /// Whether a note identifier is currently in the trash.
    #[allow(dead_code)]
    pub fn is_trashed(&self, id: &Uuid) -> bool {
        trash::holds(&self.trash_dir, id)
    }

    /// A snapshot the user asked for, right now.
    ///
    /// Unlike the automatic one this is never skipped and never throttled: a
    /// person asking for a safety point before doing something is asking for
    /// one now. It satisfies the twenty-four hour rule too, because it is a
    /// backup — nothing distinguishes it from an automatic snapshot except the
    /// word in its manifest.
    pub fn create_backup_now(&self) -> Result<PathBuf, String> {
        let now = Utc::now();
        let result = backup::create_snapshot(
            &self.backups_dir,
            &self.backup_sources(),
            SnapshotKind::Manual,
            now,
            backup::SNAPSHOT_RETENTION,
        );
        self.record_backup_attempt(now, result.is_ok());
        result
    }

    /// Reads study metadata without changing it. Missing is the empty history;
    /// damaged or newer data fails closed in [`crate::study`].
    pub fn load_study(&self) -> Result<StudyState, String> {
        study::load(&self.study_file_path())
    }

    /// Commits one rating. The clock and local civil date are chosen in the
    /// host, and the returned value exists only after the atomic write did.
    pub fn rate_study(&self, review_key: &str, rating: Rating) -> Result<StudyState, String> {
        self.back_up_before_mutation();
        study::rate_now(&self.study_file_path(), review_key, rating)
    }

    fn backup_sources(&self) -> BackupSources {
        BackupSources {
            notes_dir: self.notes_dir.clone(),
            trash_dir: self.trash_dir.clone(),
            assets_dir: self.assets_dir.clone(),
            config_file: self.config_file_path(),
            state_file: self.state_file_path(),
            study_file: self.study_file_path(),
        }
    }

    /// Takes the day's first snapshot, if one is owed.
    ///
    /// Called at the start of a persistent mutation and nowhere else, so a
    /// daemon nobody is using does no work at all, and a daemon left open for
    /// a week still produces a snapshot the moment its owner starts typing
    /// again. A failure is written to the diagnostic log and to `stderr`; the
    /// mutation that asked carries on regardless.
    fn back_up_before_mutation(&self) {
        let now = Utc::now();
        if !self.automatic_backup_owed(now) {
            return;
        }

        let result = backup::create_snapshot(
            &self.backups_dir,
            &self.backup_sources(),
            SnapshotKind::Automatic,
            now,
            backup::SNAPSHOT_RETENTION,
        );
        self.record_backup_attempt(now, result.is_ok());
        match result {
            Ok(path) => diagnostics::log(format_args!(
                "event=backup-created kind=automatic path={}",
                path.display()
            )),
            Err(error) => eprintln!(
                "The automatic backup could not be created, so the store is still \
                 protected only by the snapshots already on disk. The note is saved \
                 normally and the backup will be attempted again: {error}"
            ),
        }
    }

    /// Whether an automatic snapshot is owed, reading the backups directory at
    /// most once per session.
    fn automatic_backup_owed(&self, now: DateTime<Utc>) -> bool {
        let mut schedule = self.backup_schedule.borrow_mut();
        if !schedule.loaded {
            schedule.last_success = backup::last_snapshot_time(&self.backups_dir);
            schedule.loaded = true;
        }
        backup::automatic_backup_due(
            schedule.last_success,
            now,
            Duration::hours(backup::AUTOMATIC_BACKUP_INTERVAL_HOURS),
        ) && backup::retry_allowed(
            schedule.last_attempt,
            now,
            Duration::minutes(backup::AUTOMATIC_BACKUP_RETRY_MINUTES),
        )
    }

    fn record_backup_attempt(&self, now: DateTime<Utc>, succeeded: bool) {
        let mut schedule = self.backup_schedule.borrow_mut();
        schedule.loaded = true;
        schedule.last_attempt = Some(now);
        if succeeded {
            schedule.last_success = Some(now);
        }
    }

    pub fn load_note(&self, id: &Uuid) -> Result<NoteDocument, String> {
        let path = self.note_path(id);
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read note {}: {e}", path.display()))?;
        NoteDocument::parse(&content)
    }

    /// Every `.md` in the store whose name is a note identifier, with the
    /// file's own modification time. Nothing is opened here: this is the
    /// directory and nothing more.
    fn note_files(&self) -> Result<Vec<(Uuid, SystemTime)>, String> {
        let entries = fs::read_dir(&self.notes_dir)
            .map_err(|e| format!("Failed to read notes directory: {e}"))?;

        let mut files = Vec::new();
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
                .unwrap_or(UNIX_EPOCH);
            files.push((id, modified));
        }
        Ok(files)
    }

    /// Note identifiers ordered by the last change to their **text**, most
    /// recent first.
    ///
    /// Used to decide which note to bring back when every note has been
    /// closed, and to order what search shows — one idea of "most recent"
    /// everywhere rather than two that disagree.
    ///
    /// The ordering key is the note's own `updated_at`, not the file's
    /// modification time. Phase 3.4R defined `updated_at` as the last change
    /// to the note's content, and appearance — colour, paper, pattern
    /// intensity, font size — deliberately does not move it. But every one of
    /// those rewrites the file, so ordering by `mtime` meant recolouring a
    /// note counted as writing in it. Reading the field the contract is
    /// already written in is what makes the two agree.
    ///
    /// A note whose front matter has no `updated_at`, cannot be parsed or
    /// cannot be read falls back to the file's modification time: the best
    /// evidence left, and the rule every note followed before there was a
    /// field to read. Nothing here writes, and no failure here is fatal — an
    /// unreadable header costs that note its timestamp, not the listing.
    pub fn list_notes_by_recency(&self) -> Result<Vec<Uuid>, String> {
        let mut notes: Vec<Listed> = self
            .note_files()?
            .into_iter()
            .map(|(id, modified)| Listed {
                id,
                edited_at: read_front_matter(&self.note_path(&id))
                    .as_deref()
                    .and_then(recorded_edit_time)
                    .map(SystemTime::from)
                    .unwrap_or(modified),
            })
            .collect();

        newest_first(&mut notes);
        Ok(notes.into_iter().map(|note| note.id).collect())
    }

    /// Derives autocomplete catalogs from live note front matter.
    ///
    /// There is no sidecar or index to invalidate. Trash is excluded because
    /// only `notes_dir` is traversed; restoring a file makes it appear again.
    pub fn metadata_catalog(&self) -> MetadataCatalog {
        let mut tags: BTreeMap<String, (String, usize)> = BTreeMap::new();
        let mut keys: BTreeMap<String, (String, usize)> = BTreeMap::new();
        let ids = self.list_notes_by_recency().unwrap_or_default();

        for id in ids {
            let Some(front_matter) = read_front_matter(&self.note_path(&id)) else {
                continue;
            };
            let Ok(wrapper) = serde_yaml::from_str::<NoteFrontMatterWrapper>(&front_matter) else {
                continue;
            };
            for tag in wrapper.tags.as_slice() {
                let identity = semantic_identity(tag);
                tags.entry(identity)
                    .and_modify(|entry| entry.1 += 1)
                    .or_insert_with(|| (tag.clone(), 1));
            }
            for property in wrapper.properties.as_slice() {
                let identity = semantic_identity(&property.key);
                keys.entry(identity)
                    .and_modify(|entry| entry.1 += 1)
                    .or_insert_with(|| (property.key.clone(), 1));
            }
        }

        let mut tags: Vec<_> = tags
            .into_values()
            .map(|(tag, note_count)| TagCatalogEntry { tag, note_count })
            .collect();
        tags.sort_by(|left, right| {
            right
                .note_count
                .cmp(&left.note_count)
                .then_with(|| semantic_identity(&left.tag).cmp(&semantic_identity(&right.tag)))
        });

        let mut property_keys: Vec<_> = keys
            .into_values()
            .map(|(key, note_count)| PropertyKeyCatalogEntry { key, note_count })
            .collect();
        property_keys.sort_by(|left, right| {
            right
                .note_count
                .cmp(&left.note_count)
                .then_with(|| semantic_identity(&left.key).cmp(&semantic_identity(&right.key)))
        });

        MetadataCatalog {
            tags,
            property_keys,
        }
    }

    /// **Every** note's own text, newest first, ready to be searched.
    ///
    /// All of them, with no ceiling. Search says it looks in every note, and a
    /// scan limit would quietly make that untrue for whichever note fell past
    /// it — the note nobody would ever be told was skipped. What is capped is
    /// the *result* list ([`crate::search::MAX_RESULTS`]), because a hundred
    /// rows is what a person can read, not what a machine can scan.
    ///
    /// Only the note's body is returned: the stored metadata is Note-it's
    /// bookkeeping, not something anyone typed, and a search for `paper` must
    /// not return every note in the store.
    ///
    /// A note that has vanished or cannot be read is skipped rather than
    /// failing the whole scan: one unreadable file must not stop search
    /// working. Nothing here writes, so searching never touches a note.
    pub fn read_note_bodies_by_recency(&self) -> Vec<(Uuid, String)> {
        self.read_bodies(usize::MAX)
    }

    /// The text of the `limit` most recently written-in notes: what the empty
    /// query lists.
    ///
    /// Capped because the listing itself is capped. Reading past what the
    /// palette will show would be reading files nobody is going to see, and
    /// unlike a search it would answer no question.
    pub fn read_recent_note_bodies(&self, limit: usize) -> Vec<(Uuid, String)> {
        self.read_bodies(limit)
    }

    fn read_bodies(&self, limit: usize) -> Vec<(Uuid, String)> {
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
    use crate::metadata::{NoteMetadata, NoteProperty};
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
    fn notes_are_listed_with_the_most_recently_edited_first() {
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

        // Writing in the oldest note again moves it to the front.
        let mut refreshed = manager.load_note(&ids[0]).expect("load oldest");
        refreshed.content = "touched".to_string();
        refreshed.touch_content_modified();
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
        doc.user_metadata = NoteMetadata::try_new(
            ["Medicina".into()],
            [NoteProperty {
                key: "fonte".into(),
                value: "Harrison".into(),
            }],
        )
        .expect("metadata");
        manager.save_note_atomic(&doc).expect("save");

        let bodies = manager.read_note_bodies_by_recency();
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
        assert!(!bodies[0].1.contains("Medicina"));
        assert!(!bodies[0].1.contains("Harrison"));
    }

    #[test]
    fn front_matter_beyond_4096_bytes_still_uses_updated_at_for_recency() {
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");

        let mut older = NoteDocument::new_empty();
        older.content = "older body".into();
        older.metadata.updated_at = Some(at(1_000));
        let older_id = older.metadata.id;
        let mut raw = older.serialize().expect("serialize");
        raw = raw.replacen(
            "---\n\nolder body",
            &format!("future_blob: {}\n---\n\nolder body", "x".repeat(8_000)),
            1,
        );
        assert!(
            NoteDocument::split_front_matter(&raw)
                .0
                .expect("header")
                .len()
                > 4096
        );
        fs::write(manager.note_path(&older_id), raw).expect("write large header");

        let mut newer = NoteDocument::new_empty();
        newer.content = "newer body".into();
        newer.metadata.updated_at = Some(at(2_000));
        let newer_id = newer.metadata.id;
        place(&manager, &newer);

        // Make mtime say the opposite. Falling back at 4096 would put `older`
        // first; reading through the delimiter keeps the semantic timestamp.
        stamp(&manager.note_path(&newer_id), 10);
        stamp(&manager.note_path(&older_id), 20);
        assert_eq!(
            manager.list_notes_by_recency().expect("list"),
            [newer_id, older_id]
        );
    }

    #[test]
    fn catalogs_are_derived_from_live_notes_and_restore_naturally() {
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");

        let mut first = NoteDocument::new_empty();
        first.user_metadata = NoteMetadata::try_new(
            ["Medicina".into(), "PBL".into()],
            [NoteProperty {
                key: "Status".into(),
                value: "revisando".into(),
            }],
        )
        .expect("metadata");
        manager.save_note_atomic(&first).expect("save first");

        let mut second = NoteDocument::new_empty();
        second.user_metadata = NoteMetadata::try_new(
            ["medicina".into(), "Hotel".into()],
            [NoteProperty {
                key: "status".into(),
                value: "novo".into(),
            }],
        )
        .expect("metadata");
        manager.save_note_atomic(&second).expect("save second");

        let catalog = manager.metadata_catalog();
        assert_eq!(catalog.tags[0].note_count, 2);
        assert_eq!(semantic_identity(&catalog.tags[0].tag), "medicina");
        assert_eq!(catalog.property_keys[0].note_count, 2);

        manager
            .move_note_to_trash(&first.metadata.id)
            .expect("trash");
        let without_trash = manager.metadata_catalog();
        assert!(!without_trash.tags.iter().any(|entry| entry.tag == "PBL"));
        assert_eq!(
            without_trash
                .tags
                .iter()
                .find(|entry| semantic_identity(&entry.tag) == "medicina")
                .unwrap()
                .note_count,
            1
        );

        manager
            .restore_note_from_trash(&first.metadata.id)
            .expect("restore");
        let restored = manager.metadata_catalog();
        assert!(restored.tags.iter().any(|entry| entry.tag == "PBL"));
        assert_eq!(
            restored
                .tags
                .iter()
                .find(|entry| semantic_identity(&entry.tag) == "medicina")
                .unwrap()
                .note_count,
            2
        );
    }

    #[test]
    fn save_trash_restore_and_backup_preserve_semantic_metadata_bytes() {
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");
        let mut doc = NoteDocument::new_empty();
        doc.content = "# Caso clínico".into();
        doc.user_metadata = NoteMetadata::try_new(
            ["Urgência".into(), "Saúde".into()],
            [NoteProperty {
                key: "fonte".into(),
                value: "Harrison".into(),
            }],
        )
        .expect("metadata");
        let id = doc.metadata.id;
        manager.save_note_atomic(&doc).expect("save");
        let original = fs::read(manager.note_path(&id)).expect("original bytes");

        let snapshot = manager.create_backup_now().expect("snapshot");
        assert_eq!(
            fs::read(snapshot.join("notes").join(format!("{id}.md"))).expect("backup bytes"),
            original
        );

        manager.move_note_to_trash(&id).expect("trash");
        assert_eq!(
            fs::read(manager.trash_dir().join(format!("{id}.md"))).expect("trash bytes"),
            original
        );
        manager.restore_note_from_trash(&id).expect("restore");
        assert_eq!(
            fs::read(manager.note_path(&id)).expect("restored bytes"),
            original
        );
        assert_eq!(
            manager
                .load_note(&id)
                .expect("parse restored")
                .user_metadata,
            doc.user_metadata
        );
    }

    #[test]
    fn deriving_catalogs_for_a_thousand_notes_is_fast_and_writes_nothing() {
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let manager = StorageManager::with_custom_paths(
            notes_dir.clone(),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");
        for index in 0..1_000 {
            let mut doc = NoteDocument::new_empty();
            doc.user_metadata = NoteMetadata::try_new(
                [format!("Área {}", index % 20), "Projeto".into()],
                [NoteProperty {
                    key: format!("campo {}", index % 10),
                    value: index.to_string(),
                }],
            )
            .expect("metadata");
            place(&manager, &doc);
        }
        let before = fingerprint(&notes_dir);
        let started = std::time::Instant::now();
        let catalog = manager.metadata_catalog();
        let elapsed = started.elapsed();
        println!("metadata catalog for 1000 notes: {elapsed:?}");
        assert_eq!(catalog.tags.len(), 21);
        assert_eq!(catalog.property_keys.len(), 10);
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "catalog took {elapsed:?}"
        );
        assert_eq!(
            fingerprint(&notes_dir),
            before,
            "catalog derivation wrote the store"
        );
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

        let bodies = manager.read_note_bodies_by_recency();
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
            let bodies = manager.read_note_bodies_by_recency();
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

    /// A fixed instant, so an ordering test never depends on the clock.
    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).expect("a valid instant")
    }

    /// Writes a note straight into the store, without the two fsyncs an
    /// atomic save costs. Used where a test needs thousands of notes: five
    /// thousand fsyncs measure the filesystem, not the code under test.
    fn place(manager: &StorageManager, doc: &NoteDocument) {
        fs::write(
            manager.note_path(&doc.metadata.id),
            doc.serialize().expect("serialize"),
        )
        .expect("write note");
    }

    /// Stamps a file's modification time, so a fallback ordering is decided by
    /// the test rather than by how fast the test ran.
    fn stamp(path: &Path, seconds: u64) {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open to stamp");
        file.set_times(
            fs::FileTimes::new().set_modified(UNIX_EPOCH + std::time::Duration::from_secs(seconds)),
        )
        .expect("stamp the file");
    }

    /// Name, size and modification time of everything in the store.
    fn fingerprint(notes_dir: &Path) -> Vec<(std::ffi::OsString, u64, Option<SystemTime>)> {
        let mut entries: Vec<_> = fs::read_dir(notes_dir)
            .expect("read notes dir")
            .flatten()
            .map(|entry| {
                let metadata = entry.metadata().ok();
                (
                    entry.file_name(),
                    metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                    metadata.and_then(|m| m.modified().ok()),
                )
            })
            .collect();
        entries.sort();
        entries
    }

    #[test]
    fn a_note_past_the_old_scan_ceiling_is_still_searched() {
        // 3.8R. Search said it looked in every note and read the first five
        // thousand, so a store one note larger held a note nobody could ever
        // find and nobody would ever be told had been skipped. The *result*
        // list is still capped, because a hundred rows is what a person reads.
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        // `updated_at` is what the ordering reads, so setting it puts the
        // needle exactly where the test needs it: dead last.
        for index in 0..5_000 {
            let mut doc = NoteDocument::new_empty();
            doc.content = format!("# Nota {index}\n\nnada de interessante nesta");
            doc.metadata.updated_at = Some(at(1_000_000 + index));
            place(&manager, &doc);
        }

        let mut needle = NoteDocument::new_empty();
        needle.content = "# Fim da fila\n\na agulha transjugular está aqui".to_string();
        needle.metadata.updated_at = Some(at(1));
        place(&manager, &needle);

        let ids = manager.list_notes_by_recency().expect("list");
        assert_eq!(ids.len(), 5_001);
        assert_eq!(
            *ids.last().expect("a last note"),
            needle.metadata.id,
            "the needle has to sit past the old ceiling for this test to mean anything",
        );

        let bodies = manager.read_note_bodies_by_recency();
        assert_eq!(
            bodies.len(),
            5_001,
            "every note is read, not the first 5 000"
        );

        let results = crate::search::search_notes(
            "transjugular",
            bodies.iter().map(|(id, body)| (*id, body.as_str())),
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note_id, needle.metadata.id);
        assert_eq!(results[0].matched_text, "transjugular");

        // The listing an empty query shows is still capped: what grew is the
        // search's reach, not every read in the application.
        assert_eq!(
            manager
                .read_recent_note_bodies(crate::search::MAX_RESULTS)
                .len(),
            crate::search::MAX_RESULTS,
        );
    }

    #[test]
    fn changing_how_a_note_looks_does_not_make_it_the_most_recent() {
        // 3.8R. `updated_at` is the last change to the note's *text*, and an
        // appearance change deliberately never moves it — but it does rewrite
        // the file. Ordering by the file's own timestamp therefore counted
        // recolouring a note as writing in it, and the first row of the quick
        // switcher was whichever note had last been repainted.
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let mut older = NoteDocument::new_empty();
        older.content = "nota B, escrita primeiro".to_string();
        older.metadata.updated_at = Some(at(1_000));
        manager.save_note_atomic(&older).expect("save B");

        let mut newer = NoteDocument::new_empty();
        newer.content = "nota A, escrita depois".to_string();
        newer.metadata.updated_at = Some(at(2_000));
        manager.save_note_atomic(&newer).expect("save A");

        assert_eq!(
            manager.list_notes_by_recency().expect("list")[0],
            newer.metadata.id,
            "A had its content edited last",
        );

        // Now repaint B — colour, paper, pattern intensity and font size, all
        // of it — long after A was written in. Nothing goes through
        // `touch_content_modified`, because none of it is content.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut repainted = manager.load_note(&older.metadata.id).expect("load B");
        let recorded_edit = repainted.metadata.updated_at;
        repainted.metadata.color = "black".to_string();
        repainted.metadata.paper_type = "grid-large".to_string();
        repainted.metadata.paper_intensity = "strong".to_string();
        repainted.metadata.font_size = 22;
        manager.save_note_atomic(&repainted).expect("repaint B");

        // The repaint really did land, and really did rewrite the file last...
        let stored = manager.load_note(&older.metadata.id).expect("reload B");
        assert_eq!(stored.metadata.color, "black");
        assert_eq!(stored.metadata.paper_type, "grid-large");
        assert_eq!(stored.metadata.updated_at, recorded_edit);
        let repainted_file = fs::metadata(manager.note_path(&older.metadata.id))
            .and_then(|m| m.modified())
            .expect("B mtime");
        let untouched_file = fs::metadata(manager.note_path(&newer.metadata.id))
            .and_then(|m| m.modified())
            .expect("A mtime");
        assert!(
            repainted_file >= untouched_file,
            "the repaint has to be the newest write for this test to mean anything",
        );

        // ...and the ordering did not move, because nobody wrote in B.
        let ordering = manager.list_notes_by_recency().expect("list again");
        assert_eq!(ordering[0], newer.metadata.id, "a repaint is not an edit");
        assert_eq!(ordering[1], older.metadata.id);
    }

    #[test]
    fn a_note_with_no_readable_edit_time_falls_back_to_the_file() {
        // Three kinds of note have no `updated_at` to read: one written before
        // the field existed, one with no front matter at all, and one whose
        // front matter cannot be parsed. Each falls back to the file's own
        // timestamp, which is the rule every note followed before there was a
        // field to read. None of them panics, and none of them is dropped.
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let manager = StorageManager::with_custom_paths(
            notes_dir.clone(),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let mut modern = NoteDocument::new_empty();
        modern.content = "nota atual".to_string();
        modern.metadata.updated_at = Some(at(3_000));
        place(&manager, &modern);
        stamp(&manager.note_path(&modern.metadata.id), 1);

        let legacy = Uuid::from_bytes([0xA1; 16]);
        fs::write(
            manager.note_path(&legacy),
            concat!(
                "---\n",
                "note_it:\n",
                "  version: 1\n",
                "  id: a1a1a1a1-a1a1-a1a1-a1a1-a1a1a1a1a1a1\n",
                "  color: blue\n",
                "  font_size: 15\n",
                "---\n\n",
                "nota anterior ao campo\n",
            ),
        )
        .expect("write legacy");
        stamp(&manager.note_path(&legacy), 2_000);

        let bare = Uuid::from_bytes([0xB2; 16]);
        fs::write(manager.note_path(&bare), "sem front matter nenhum\n").expect("write bare");
        stamp(&manager.note_path(&bare), 1_000);

        let broken = Uuid::from_bytes([0xC3; 16]);
        fs::write(
            manager.note_path(&broken),
            "---\nnote_it: [isto não é o formato]\n---\n\ncorpo mesmo assim\n",
        )
        .expect("write broken");
        stamp(&manager.note_path(&broken), 4_000);

        // The modern note is placed by its own `updated_at` (3 000), the other
        // three by their files (4 000, 2 000, 1 000). One total order, and the
        // same one every time.
        let ordering = manager.list_notes_by_recency().expect("list");
        assert_eq!(ordering, vec![broken, modern.metadata.id, legacy, bare]);
        assert_eq!(
            manager.list_notes_by_recency().expect("list again"),
            ordering
        );

        // ...and every one of them is still searchable.
        let bodies = manager.read_note_bodies_by_recency();
        assert_eq!(bodies.len(), 4);
        assert!(bodies
            .iter()
            .any(|(_, body)| body == "sem front matter nenhum"));
        assert!(bodies.iter().any(|(_, body)| body == "corpo mesmo assim"));
    }

    #[test]
    fn notes_edited_at_the_same_instant_keep_a_stable_order() {
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        // Same instant for all three, so only the tie-break decides.
        let ids = [
            Uuid::from_bytes([0x03; 16]),
            Uuid::from_bytes([0x01; 16]),
            Uuid::from_bytes([0x02; 16]),
        ];
        for id in ids {
            let mut doc = NoteDocument::new_with_id(id);
            doc.content = format!("nota {id}");
            doc.metadata.updated_at = Some(at(7_777));
            place(&manager, &doc);
        }

        let mut expected = ids;
        expected.sort();
        for _ in 0..5 {
            assert_eq!(
                manager.list_notes_by_recency().expect("list"),
                expected.to_vec(),
                "a tie must resolve the same way every time",
            );
        }
    }

    #[test]
    fn listing_and_searching_never_write_to_the_store() {
        // Reading a note's header to find out when it was last written in is
        // still reading. Neither listing nor searching may leave a mark.
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let manager = StorageManager::with_custom_paths(
            notes_dir.clone(),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        for index in 0..20 {
            let mut doc = NoteDocument::new_empty();
            doc.content = format!("# Nota {index}\n\ncom biópsia dentro");
            manager.save_note_atomic(&doc).expect("save");
        }

        let before = fingerprint(&notes_dir);
        for _ in 0..3 {
            manager.list_notes_by_recency().expect("list");
            let bodies = manager.read_note_bodies_by_recency();
            crate::search::search_notes(
                "biopsia",
                bodies.iter().map(|(id, body)| (*id, body.as_str())),
            );
            manager.read_recent_note_bodies(crate::search::MAX_RESULTS);
        }

        assert_eq!(fingerprint(&notes_dir), before, "a read modified the store");
        assert!(temp_debris_in(&notes_dir).is_empty());
    }

    #[test]
    fn a_very_large_note_is_searched_correctly_and_never_written() {
        // 3.8R. The query, the result list and the snippet are all capped; the
        // *note* is not, and nothing here pretends otherwise. What is claimed
        // is what this asserts: a note far larger than anyone writes is
        // searched to its end, comes back with its accents intact, does not
        // panic and is not touched.
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let manager = StorageManager::with_custom_paths(
            notes_dir.clone(),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        // ~2 MB, accented throughout, with the word only at the very end — so
        // a scan that stopped early would find nothing.
        let filler = "Coração, fígado, pulmão e rins. ".repeat(64_000);
        let mut huge = NoteDocument::new_empty();
        huge.content = format!("# Nota enorme\n\n{filler}\nbiópsia transjugular ao fim\n");
        manager
            .save_note_atomic(&huge)
            .expect("save the large note");
        assert!(huge.content.len() > 2_000_000);

        let before = fingerprint(&notes_dir);
        let bodies = manager.read_note_bodies_by_recency();
        let results = crate::search::search_notes(
            "transjugular",
            bodies.iter().map(|(id, body)| (*id, body.as_str())),
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note_id, huge.metadata.id);
        assert_eq!(results[0].match_count, 1);
        // Folded to find, original to show: the accents survived the round trip
        // through the fold and back to a source offset.
        assert!(results[0].snippet.contains("biópsia transjugular"));
        assert!(results[0].snippet.contains("Coração"));
        assert_eq!(results[0].matched_text, "transjugular");
        assert!(results[0].snippet.chars().count() <= crate::search::MAX_SNIPPET_CHARS + 2);

        // Accent-folded queries reach the end of it too...
        let found = crate::search::search_notes(
            "biopsia",
            bodies.iter().map(|(id, body)| (*id, body.as_str())),
        );
        assert_eq!(found[0].matched_text, "biópsia");

        assert_eq!(
            fingerprint(&notes_dir),
            before,
            "a search modified the store"
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

    // ------------------------------------------------------------------
    // Phase 3.9 — a note in the trash is not a note.
    // ------------------------------------------------------------------

    /// A store with one note in it, and the note's identifier.
    fn store_with_one_note(tmp: &tempfile::TempDir, body: &str) -> (StorageManager, Uuid) {
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let mut doc = NoteDocument::new_empty();
        doc.content = body.to_string();
        manager.save_note_atomic(&doc).expect("save note");
        (manager, doc.metadata.id)
    }

    fn search_finds(manager: &StorageManager, query: &str) -> Vec<Uuid> {
        let bodies = manager.read_note_bodies_by_recency();
        crate::search::search_notes(query, bodies.iter().map(|(id, body)| (*id, body.as_str())))
            .into_iter()
            .map(|result| result.note_id)
            .collect()
    }

    #[test]
    fn presenting_and_searching_a_formatted_note_writes_nothing_and_moves_no_date() {
        // 3.9UX.R.1. Presentation reads. Naming a note in a list, quoting it
        // in a result and answering a query all happen without opening a file
        // for writing, so nothing about how a note is *shown* can charge the
        // reader an edit.
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("Init storage manager");

        let mut doc = NoteDocument::new_empty();
        doc.content = concat!(
            "# <mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">",
            "<span data-note-it-color=\"#64748B\" style=\"color:#64748B\">teste de verdade</span>",
            "</mark>\n\n**OBSERVAÇÃO:** MARCADOR-8391\n\n",
            "<!-- esse é um comentário de teste -->",
        )
        .to_string();
        doc.metadata.updated_at = Some(at(1_000));
        manager.save_note_atomic(&doc).expect("save note");

        let path = manager.note_path(&doc.metadata.id);
        let bytes_before = fs::read(&path).expect("read note");
        let modified_before = fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .expect("modification time");

        for query in ["", "verdade", "MARCADOR-8391", "data-note-it-color"] {
            let bodies = manager.read_note_bodies_by_recency();
            let notes = bodies.iter().map(|(id, body)| (*id, body.as_str()));
            let results = if query.is_empty() {
                crate::search::recent_notes(notes)
            } else {
                crate::search::search_notes(query, notes)
            };
            for result in &results {
                assert!(!result.label.contains("data-note-it-"));
                assert!(!result.snippet.contains("data-note-it-"));
            }
        }

        // The attribute is storage, so it finds nothing; the words find the
        // note, and the label is the phrase rather than the span around it.
        assert!(search_finds(&manager, "data-note-it-color").is_empty());
        assert_eq!(
            search_finds(&manager, "teste de verdade"),
            vec![doc.metadata.id]
        );

        assert_eq!(fs::read(&path).expect("read note"), bytes_before);
        assert_eq!(
            fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .expect("modification time"),
            modified_before,
        );
        let reread = NoteDocument::parse(&String::from_utf8(bytes_before).expect("utf8"))
            .expect("parse note");
        assert_eq!(reread.metadata.updated_at, Some(at(1_000)));
        assert_eq!(reread.content, doc.content);
    }

    #[test]
    fn a_trashed_note_is_not_searchable() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "MARCADOR-LIXEIRA-8391 no corpo");

        assert_eq!(search_finds(&manager, "MARCADOR-LIXEIRA-8391"), vec![id]);

        manager.move_note_to_trash(&id).expect("move to trash");

        assert!(
            search_finds(&manager, "MARCADOR-LIXEIRA-8391").is_empty(),
            "a note in the trash must not be findable"
        );
        assert!(manager.is_trashed(&id));
    }

    #[test]
    fn a_trashed_note_is_not_in_recent_notes() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "MARCADOR-LIXEIRA-8391");

        assert_eq!(manager.list_notes_by_recency().expect("list"), vec![id]);

        manager.move_note_to_trash(&id).expect("move to trash");

        assert!(
            manager.list_notes_by_recency().expect("list").is_empty(),
            "the quick switcher lists live notes only"
        );
        assert!(
            manager
                .read_recent_note_bodies(crate::search::MAX_RESULTS)
                .is_empty(),
            "an empty query must not offer a deleted note"
        );
    }

    #[test]
    fn restored_note_becomes_searchable_again() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "MARCADOR-LIXEIRA-8391 no corpo");
        let before = fs::read(manager.note_path(&id)).expect("read before");
        let updated_at = manager.load_note(&id).expect("load").metadata.updated_at;

        manager.move_note_to_trash(&id).expect("move to trash");
        assert!(search_finds(&manager, "MARCADOR-LIXEIRA-8391").is_empty());

        manager.restore_note_from_trash(&id).expect("restore");

        assert_eq!(search_finds(&manager, "MARCADOR-LIXEIRA-8391"), vec![id]);
        assert_eq!(manager.list_notes_by_recency().expect("list"), vec![id]);
        assert_eq!(
            fs::read(manager.note_path(&id)).expect("read after"),
            before
        );
        // Restoring is not editing: the quick switcher must not treat a
        // recovered note as one that was just written in.
        assert_eq!(
            manager.load_note(&id).expect("load").metadata.updated_at,
            updated_at
        );
        assert!(!manager.is_trashed(&id));
        assert!(manager.list_trash().is_empty());
    }

    #[test]
    fn the_trash_listing_is_what_the_interface_receives() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "# Título\n\nMARCADOR-LIXEIRA-8391");
        manager.move_note_to_trash(&id).expect("move to trash");

        let entries = manager.list_trash();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].note_id, id);
        assert_eq!(entries[0].label, "Título");
        assert!(entries[0].snippet.contains("MARCADOR-LIXEIRA-8391"));
        assert!(entries[0].deleted_at.is_some());
    }

    // ------------------------------------------------------------------
    // Phase 3.9 — when the store is backed up, and what a failure costs.
    // ------------------------------------------------------------------

    fn snapshot_count(manager: &StorageManager) -> usize {
        crate::backup::list_snapshots(manager.backups_dir()).len()
    }

    /// Replaces the trash directory with a file, so reading it as a directory
    /// fails on path resolution — which fails for every user, root included —
    /// and the backup cannot be built.
    fn break_the_backup(manager: &StorageManager) {
        fs::remove_dir_all(manager.trash_dir()).expect("remove the trash directory");
        fs::write(manager.trash_dir(), "não é um diretório").expect("occupy the trash path");
    }

    #[test]
    fn the_first_change_of_the_day_is_backed_up_before_it_is_written() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "primeiro conteúdo");

        // The snapshot was taken before that first save, so it holds the store
        // as it was: empty. That is the point of taking it first.
        let snapshots = crate::backup::list_snapshots(manager.backups_dir());
        assert_eq!(snapshots.len(), 1);
        assert!(
            !snapshots[0].path.join(format!("notes/{id}.md")).exists(),
            "the snapshot must predate the change it protects against"
        );

        // And the save itself went through.
        assert_eq!(
            manager.load_note(&id).expect("load").content,
            "primeiro conteúdo"
        );
    }

    #[test]
    fn an_automatic_backup_is_not_repeated_inside_the_24h_window() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "conteúdo");
        assert_eq!(snapshot_count(&manager), 1);

        for round in 0..5 {
            let mut doc = manager.load_note(&id).expect("load");
            doc.content = format!("conteúdo {round}");
            doc.touch_content_modified();
            manager.save_note_atomic(&doc).expect("save");
        }

        assert_eq!(
            snapshot_count(&manager),
            1,
            "an autosave is not a reason to take another snapshot"
        );
    }

    #[test]
    fn a_new_backup_can_be_created_after_the_interval() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "conteúdo");
        // Only the one taken before the first save.
        assert_eq!(snapshot_count(&manager), 1);

        // Age it: the store's own record of when it was last backed up is the
        // newest snapshot, so a snapshot dated more than a day ago is exactly
        // the situation a returning user is in. Nothing here waits.
        for snapshot in crate::backup::list_snapshots(manager.backups_dir()) {
            fs::remove_dir_all(&snapshot.path).expect("remove");
        }
        crate::backup::create_snapshot(
            manager.backups_dir(),
            &manager.backup_sources(),
            crate::backup::SnapshotKind::Automatic,
            Utc::now() - Duration::hours(25),
            crate::backup::SNAPSHOT_RETENTION,
        )
        .expect("seed an old snapshot");

        // A new session reads that record from disk.
        let returning = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("reopen the store");

        let mut doc = returning.load_note(&id).expect("load");
        doc.content = "conteúdo de hoje".to_string();
        doc.touch_content_modified();
        returning.save_note_atomic(&doc).expect("save");

        assert_eq!(snapshot_count(&returning), 2);
        // And the new one holds the note as it was before today's edit.
        let snapshots = crate::backup::list_snapshots(returning.backups_dir());
        let newest = snapshots.last().expect("newest snapshot");
        assert!(
            !fs::read_to_string(newest.path.join(format!("notes/{id}.md")))
                .expect("the note inside the snapshot")
                .contains("conteúdo de hoje"),
            "the snapshot has to hold the note as it was before today's edit"
        );
    }

    #[test]
    fn a_recent_snapshot_leaves_the_returning_session_alone() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "conteúdo");
        for snapshot in crate::backup::list_snapshots(manager.backups_dir()) {
            fs::remove_dir_all(&snapshot.path).expect("remove");
        }
        crate::backup::create_snapshot(
            manager.backups_dir(),
            &manager.backup_sources(),
            crate::backup::SnapshotKind::Automatic,
            Utc::now() - Duration::hours(1),
            crate::backup::SNAPSHOT_RETENTION,
        )
        .expect("seed a recent snapshot");

        let returning = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("reopen the store");

        let mut doc = returning.load_note(&id).expect("load");
        doc.content = "editado".to_string();
        doc.touch_content_modified();
        returning.save_note_atomic(&doc).expect("save");

        assert_eq!(snapshot_count(&returning), 1);
    }

    #[test]
    fn failed_backup_does_not_block_a_normal_note_save() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "conteúdo");
        break_the_backup(&manager);
        // Owed again, and impossible: the next save asks for a snapshot, and
        // the snapshot cannot be built.
        for snapshot in crate::backup::list_snapshots(manager.backups_dir()) {
            fs::remove_dir_all(&snapshot.path).expect("remove");
        }
        manager.backup_schedule.borrow_mut().last_success = None;
        manager.backup_schedule.borrow_mut().last_attempt = None;

        let mut doc = manager.load_note(&id).expect("load");
        doc.content = "editado apesar do backup".to_string();
        doc.touch_content_modified();

        manager
            .save_note_atomic(&doc)
            .expect("a snapshot that cannot be made must never cost the edit");

        assert_eq!(
            manager.load_note(&id).expect("reload").content,
            "editado apesar do backup"
        );
        assert_eq!(snapshot_count(&manager), 0);
    }

    #[test]
    fn a_manual_backup_reports_what_happened() {
        let tmp = tempdir().expect("tempdir");
        let (manager, _) = store_with_one_note(&tmp, "conteúdo");

        let path = manager.create_backup_now().expect("manual backup");
        assert!(path.join("manifest.json").is_file());
        assert_eq!(snapshot_count(&manager), 2);

        break_the_backup(&manager);
        manager
            .create_backup_now()
            .expect_err("a manual backup says so when it cannot be made");
        assert_eq!(snapshot_count(&manager), 2);
    }

    #[test]
    fn a_note_moved_to_the_trash_is_backed_up_first() {
        let tmp = tempdir().expect("tempdir");
        let (manager, id) = store_with_one_note(&tmp, "MARCADOR-LIXEIRA-8391");
        // Clear the snapshot the first save took, and make one owed again.
        for snapshot in crate::backup::list_snapshots(manager.backups_dir()) {
            fs::remove_dir_all(&snapshot.path).expect("remove");
        }
        let returning = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("reopen the store");

        returning.move_note_to_trash(&id).expect("move to trash");

        let snapshots = crate::backup::list_snapshots(returning.backups_dir());
        assert_eq!(snapshots.len(), 1);
        assert!(
            snapshots[0].path.join(format!("notes/{id}.md")).is_file(),
            "the snapshot taken before a deletion must still hold the note"
        );
    }

    #[test]
    fn study_is_separate_from_state_and_the_catalog_tracks_trash_and_restore() {
        let tmp = tempdir().expect("tempdir");
        let manager = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("store");

        let mut a = NoteDocument::new_empty();
        a.content = "# A\n\nPergunta :: Resposta".to_string();
        manager.save_note_atomic(&a).expect("save A");
        let mut b = NoteDocument::new_empty();
        b.content = "# B\n\nTermo ::: Definição".to_string();
        manager.save_note_atomic(&b).expect("save B");

        fs::write(manager.state_file_path(), "{\"notes\":{}}\n").expect("operational state");
        let note_before = fs::read(manager.note_path(&a.metadata.id)).expect("note bytes");
        let state_before = fs::read(manager.state_file_path()).expect("state bytes");
        let key = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        manager
            .rate_study(key, Rating::Easy)
            .expect("persist rating");

        assert_eq!(manager.study_file_path(), tmp.path().join("study.json"));
        assert_ne!(manager.study_file_path(), manager.state_file_path());
        assert_eq!(
            fs::read(manager.note_path(&a.metadata.id)).unwrap(),
            note_before
        );
        assert_eq!(fs::read(manager.state_file_path()).unwrap(), state_before);
        assert_eq!(manager.load_study().unwrap().cards[key].review_count, 1);

        let live = manager.read_note_bodies_by_recency();
        assert_eq!(
            live.len(),
            2,
            "closed notes are ordinary catalog candidates"
        );
        manager.move_note_to_trash(&b.metadata.id).expect("trash B");
        assert_eq!(manager.read_note_bodies_by_recency().len(), 1);
        assert_eq!(manager.load_study().unwrap().cards[key].review_count, 1);

        manager
            .restore_note_from_trash(&b.metadata.id)
            .expect("restore B");
        assert_eq!(manager.read_note_bodies_by_recency().len(), 2);
        assert_eq!(manager.load_study().unwrap().cards[key].review_count, 1);
    }
}
