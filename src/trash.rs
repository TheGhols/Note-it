//! A deletion that can be taken back.
//!
//! Deleting a note means moving its file out of the active store and into
//! `trash/`, next to it. The note stops being a note — it is not listed, not
//! searched, not restored on startup, not summoned — while its bytes stay
//! exactly as they were, because nothing here reads, parses or rewrites them.
//!
//! **The move is the commit point**, the same rule the rest of the store
//! follows (see [`crate::atomic_file::write_atomic`]). Everything before it can
//! fail with the note still live and untouched; from it onwards the note is in
//! the trash, and nothing that fails afterwards may report otherwise.
//!
//! Two different tools move the file, because the two directions carry
//! different risks:
//!
//! - **To the trash, `rename`.** One syscall, so there is no window in which
//!   the note is both live and deleted. The destination is a trash entry for
//!   the same identifier, which cannot exist while the note is live.
//! - **Back out of it, `hard_link` then `remove_file`.** `rename` would replace
//!   a live note of the same identifier without a word, and checking first only
//!   narrows the window rather than closing it. `hard_link` refuses to create a
//!   name that already exists, atomically and without a check to race, so the
//!   live note cannot be overwritten. It is also the strictest possible
//!   preservation of the file: the restored name is the same inode, not a copy
//!   of it. If the trash name cannot be removed afterwards the note is
//!   restored, which is what was asked, and the leftover entry is reported.
//!
//! Both directories are siblings under the same `note-it` data directory, so a
//! link between them is always within one filesystem.
//!
//! When a note was deleted is written **beside** the file, never into it: a
//! `<uuid>.json` sidecar. The Markdown is the note, and recording bookkeeping
//! inside it would mean the file that comes back is not the file that went in.
//! A missing sidecar costs the entry its exact date and nothing else — the
//! file's own modification time answers instead.

use crate::atomic_file::{sync_directory_after_commit, write_atomic};
use crate::model::NoteDocument;
use crate::search::{label_for, opening_of};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// What the trash records about a note, kept outside the Markdown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrashRecord {
    #[serde(default = "default_version")]
    pub version: u32,
    pub deleted_at: DateTime<Utc>,
}

fn default_version() -> u32 {
    1
}

/// One note in the trash, as the interface receives it.
///
/// The same shape a search result has, for the same reason: the page is handed
/// an identifier and text, never a path and never anything it could turn into
/// one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub note_id: Uuid,
    /// The note's first line, exactly as search names a note.
    pub label: String,
    /// The opening of the note. Text, always rendered as text.
    pub snippet: String,
    /// When it was moved to the trash, or `None` when nothing readable says.
    pub deleted_at: Option<DateTime<Utc>>,
}

pub fn note_file_name(id: &Uuid) -> String {
    format!("{id}.md")
}

fn record_file_name(id: &Uuid) -> String {
    format!("{id}.json")
}

/// Moves a live note into the trash.
///
/// The rename is the commit point. Before it, any failure leaves the note in
/// `notes/` exactly as it was; after it, the note is in the trash whatever
/// else goes wrong, and the sidecar and the directory syncs are reported as
/// warnings rather than turned into a failure that never happened.
pub fn move_to_trash(
    notes_dir: &Path,
    trash_dir: &Path,
    id: &Uuid,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let source = notes_dir.join(note_file_name(id));
    let destination = trash_dir.join(note_file_name(id));

    if !source.is_file() {
        return Err(format!(
            "note {id} is no longer in the notes directory, so it cannot be moved to the trash"
        ));
    }

    fs::create_dir_all(trash_dir)
        .map_err(|e| format!("Failed to create the trash directory: {e}"))?;

    // A trash entry for a live note cannot exist in normal operation. If one
    // does, something outside Note-it put it there and replacing it would
    // destroy whatever it holds.
    if destination.exists() {
        return Err(format!(
            "the trash already holds an entry for note {id}; nothing was moved"
        ));
    }

    // The commit point.
    fs::rename(&source, &destination)
        .map_err(|e| format!("Failed to move note {id} to the trash: {e}"))?;

    // Past it. The note is in the trash from here on.
    let record = TrashRecord {
        version: 1,
        deleted_at: now,
    };
    match serde_json::to_string_pretty(&record) {
        Ok(serialized) => {
            if let Err(error) = write_atomic(
                &trash_dir.join(record_file_name(id)),
                serialized.as_bytes(),
                &format!("the trash record for note {id}"),
            ) {
                eprintln!(
                    "Note {id} was moved to the trash, but the date it was deleted \
                     could not be recorded: {error}"
                );
            }
        }
        Err(error) => eprintln!("Failed to serialize the trash record for note {id}: {error}"),
    }

    sync_directory_after_commit(notes_dir, "the notes directory");
    sync_directory_after_commit(trash_dir, "the trash directory");
    Ok(())
}

/// Why a restore did not happen.
///
/// The reasons are told apart because the interface has to say different
/// things about them: an occupied identifier is a refusal that protected a
/// live note, and saying "could not restore" about it would describe a failure
/// rather than the guarantee that was kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    /// A live note already carries this identifier. Neither file was changed.
    Occupied,
    /// The entry is not in the trash any more.
    Missing,
    /// Anything else, with what the filesystem said.
    Failed(String),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreError::Occupied => write!(
                formatter,
                "a note with that identifier is already in the notes directory; \
                 nothing was restored and neither file was changed"
            ),
            RestoreError::Missing => write!(formatter, "the note is no longer in the trash"),
            RestoreError::Failed(error) => write!(formatter, "{error}"),
        }
    }
}

/// Brings a note back out of the trash.
///
/// The link is the commit point, and it is what makes the promise: creating a
/// name that already exists fails, so a live note carrying the same identifier
/// is never replaced. Removing the trash entry afterwards is past the commit
/// point — the note is restored either way, and a leftover entry is reported
/// rather than dressed up as a failed restore.
pub fn restore_from_trash(
    notes_dir: &Path,
    trash_dir: &Path,
    id: &Uuid,
) -> Result<(), RestoreError> {
    let source = trash_dir.join(note_file_name(id));
    let destination = notes_dir.join(note_file_name(id));

    if !source.is_file() {
        return Err(RestoreError::Missing);
    }

    fs::create_dir_all(notes_dir)
        .map_err(|e| RestoreError::Failed(format!("Failed to create the notes directory: {e}")))?;

    // The commit point. `hard_link` refuses an existing destination without a
    // check to race, so this is where "never overwrite a live note" is decided.
    if let Err(error) = fs::hard_link(&source, &destination) {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            return Err(RestoreError::Occupied);
        }
        return Err(RestoreError::Failed(format!(
            "Failed to restore note {id}: {error}"
        )));
    }

    // Past it. The note is live from here on.
    if let Err(error) = fs::remove_file(&source) {
        eprintln!(
            "Note {id} was restored, but its trash entry could not be removed, \
             so the note is still listed in the trash: {error}"
        );
    }
    let record = trash_dir.join(record_file_name(id));
    if record.exists() {
        if let Err(error) = fs::remove_file(&record) {
            eprintln!("Failed to remove the trash record for note {id}: {error}");
        }
    }

    sync_directory_after_commit(notes_dir, "the notes directory");
    sync_directory_after_commit(trash_dir, "the trash directory");
    Ok(())
}

/// Everything recoverable in the trash, most recently deleted first.
///
/// Reading, and nothing else: no file is opened for writing, no timestamp
/// moves and no missing sidecar is created. Anything in the directory that is
/// not a note file named after an identifier is skipped, so a `README` someone
/// dropped in there costs nothing and an unreadable entry costs only itself.
pub fn list_trash(trash_dir: &Path) -> Vec<TrashEntry> {
    let Ok(entries) = fs::read_dir(trash_dir) else {
        return Vec::new();
    };

    let mut listed: Vec<(TrashEntry, DateTime<Utc>)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // Only regular files, and never through a symlink: the trash is a
        // directory of note files, not a place to follow a link out of.
        if !fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_file()) {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| Uuid::parse_str(stem).ok())
        else {
            continue;
        };

        let raw = fs::read_to_string(&path).unwrap_or_default();
        let body = NoteDocument::body_of(&raw);
        let deleted_at = deleted_at_of(trash_dir, &id, &path);

        listed.push((
            TrashEntry {
                note_id: id,
                label: label_for(body),
                snippet: opening_of(body),
                deleted_at,
            },
            // Ordering falls back to the epoch for an entry with no readable
            // date at all, so it sorts last instead of failing the listing.
            deleted_at.unwrap_or_else(|| DateTime::<Utc>::from(std::time::UNIX_EPOCH)),
        ));
    }

    listed.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.note_id.cmp(&right.0.note_id))
    });
    listed.into_iter().map(|(entry, _)| entry).collect()
}

/// When an entry was deleted: the sidecar if it can be read, otherwise the
/// file's own modification time. Never a guess, and never written back.
fn deleted_at_of(trash_dir: &Path, id: &Uuid, note_path: &Path) -> Option<DateTime<Utc>> {
    let record = read_record(&trash_dir.join(record_file_name(id)));
    if let Some(record) = record {
        return Some(record.deleted_at);
    }
    fs::metadata(note_path)
        .and_then(|meta| meta.modified())
        .ok()
        .map(DateTime::<Utc>::from)
}

fn read_record(path: &Path) -> Option<TrashRecord> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<TrashRecord>(&raw).ok()
}

/// Whether a note identifier is currently in the trash.
pub fn holds(trash_dir: &Path, id: &Uuid) -> bool {
    trash_dir.join(note_file_name(id)).is_file()
}

/// The trash entry file for a note. Used by the backup, which copies the trash
/// as it stands rather than interpreting it.
#[allow(dead_code)]
pub fn entry_path(trash_dir: &Path, id: &Uuid) -> PathBuf {
    trash_dir.join(note_file_name(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use tempfile::tempdir;

    struct Store {
        _tmp: tempfile::TempDir,
        notes: PathBuf,
        trash: PathBuf,
    }

    fn store() -> Store {
        let tmp = tempdir().expect("tempdir");
        let notes = tmp.path().join("notes");
        let trash = tmp.path().join("trash");
        fs::create_dir_all(&notes).expect("notes dir");
        fs::create_dir_all(&trash).expect("trash dir");
        Store {
            _tmp: tmp,
            notes,
            trash,
        }
    }

    fn write_note(dir: &Path, id: &Uuid, body: &str) -> PathBuf {
        let path = dir.join(note_file_name(id));
        let raw = format!(
            "---\nnote_it:\n  version: 1\n  id: {id}\n  color: yellow\n  \
             paper_type: lined\n  paper_intensity: strong\n  font_size: 15\n  \
             created_at: 2026-01-01T00:00:00Z\n  updated_at: 2026-02-02T10:11:12Z\n---\n\n{body}\n"
        );
        fs::write(&path, raw).expect("write note");
        path
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-29T09:30:00Z")
            .expect("fixed instant")
            .with_timezone(&Utc)
    }

    #[test]
    fn moving_a_note_to_trash_preserves_its_bytes() {
        let store = store();
        let id = Uuid::new_v4();
        let path = write_note(&store.notes, &id, "MARCADOR-LIXEIRA-8391\n\n- [x] tarefa");
        let before = fs::read(&path).expect("read before");

        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");

        assert!(!path.exists(), "the note must leave the active store");
        let moved = store.trash.join(note_file_name(&id));
        assert_eq!(fs::read(&moved).expect("read after"), before);
    }

    #[test]
    fn trashing_does_not_change_updated_at() {
        let store = store();
        let id = Uuid::new_v4();
        write_note(&store.notes, &id, "conteúdo");

        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");

        let raw = fs::read_to_string(store.trash.join(note_file_name(&id))).expect("read");
        let parsed = NoteDocument::parse(&raw).expect("parse");
        assert_eq!(
            parsed.metadata.updated_at,
            Some(
                DateTime::parse_from_rfc3339("2026-02-02T10:11:12Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        assert_eq!(
            parsed.metadata.created_at,
            Some(
                DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
    }

    #[test]
    fn restoring_a_note_preserves_its_uuid() {
        let store = store();
        let id = Uuid::new_v4();
        write_note(&store.notes, &id, "conteúdo");
        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");

        restore_from_trash(&store.notes, &store.trash, &id).expect("restore");

        let restored = store.notes.join(note_file_name(&id));
        assert!(restored.is_file());
        let parsed =
            NoteDocument::parse(&fs::read_to_string(&restored).expect("read")).expect("parse");
        assert_eq!(parsed.metadata.id, id);
        assert!(!store.trash.join(note_file_name(&id)).exists());
        assert!(!store.trash.join(record_file_name(&id)).exists());
    }

    #[test]
    fn restoring_a_note_preserves_updated_at() {
        let store = store();
        let id = Uuid::new_v4();
        let path = write_note(&store.notes, &id, "conteúdo");
        let before = fs::read(&path).expect("read before");

        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");
        restore_from_trash(&store.notes, &store.trash, &id).expect("restore");

        assert_eq!(fs::read(&path).expect("read after"), before);
        let parsed = NoteDocument::parse(&fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(
            parsed.metadata.updated_at,
            Some(
                DateTime::parse_from_rfc3339("2026-02-02T10:11:12Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        // Appearance stored in the note travels with it.
        assert_eq!(parsed.metadata.paper_type, "lined");
        assert_eq!(parsed.metadata.paper_intensity, "strong");
    }

    #[test]
    fn restore_never_overwrites_an_existing_live_note() {
        let store = store();
        let id = Uuid::new_v4();
        write_note(&store.notes, &id, "a nota original");
        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");

        // Something put a different note back under the same identifier.
        let live = write_note(&store.notes, &id, "outra nota, viva");
        let live_bytes = fs::read(&live).expect("read live");
        let trashed = store.trash.join(note_file_name(&id));
        let trashed_bytes = fs::read(&trashed).expect("read trashed");

        let error =
            restore_from_trash(&store.notes, &store.trash, &id).expect_err("restore must refuse");
        assert_eq!(error, RestoreError::Occupied);

        assert_eq!(fs::read(&live).expect("live after"), live_bytes);
        assert_eq!(fs::read(&trashed).expect("trashed after"), trashed_bytes);
    }

    #[test]
    fn trashing_refuses_when_the_note_is_not_there() {
        let store = store();
        let id = Uuid::new_v4();
        let error = move_to_trash(&store.notes, &store.trash, &id, now())
            .expect_err("a note that is not there cannot be trashed");
        assert!(
            error.contains("no longer in the notes directory"),
            "{error}"
        );
        assert!(!store.trash.join(note_file_name(&id)).exists());
    }

    #[test]
    fn a_move_that_cannot_be_completed_leaves_the_note_live() {
        let store = store();
        let id = Uuid::new_v4();
        let path = write_note(&store.notes, &id, "conteúdo");
        let before = fs::read(&path).expect("read");

        // A directory sitting where the trash entry belongs: the rename fails
        // on path resolution, which fails for every user, root included.
        fs::create_dir(store.trash.join(note_file_name(&id))).expect("occupy the destination");

        move_to_trash(&store.notes, &store.trash, &id, now())
            .expect_err("moving onto an occupied destination must fail");

        assert!(path.is_file(), "the note must still be live");
        assert_eq!(fs::read(&path).expect("read after"), before);
    }

    #[test]
    fn listing_trash_never_writes_anything() {
        let store = store();
        let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
        for (index, id) in ids.iter().enumerate() {
            write_note(&store.notes, id, &format!("nota {index}"));
            move_to_trash(&store.notes, &store.trash, id, now()).expect("move to trash");
        }

        let fingerprint = |dir: &Path| -> Vec<(String, u64, SystemTime)> {
            let mut files: Vec<_> = fs::read_dir(dir)
                .expect("read trash")
                .flatten()
                .map(|entry| {
                    let meta = entry.metadata().expect("metadata");
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        meta.len(),
                        meta.modified().expect("mtime"),
                    )
                })
                .collect();
            files.sort();
            files
        };

        let before = fingerprint(&store.trash);
        let entries = list_trash(&store.trash);
        assert_eq!(entries.len(), 3);
        assert_eq!(fingerprint(&store.trash), before);
    }

    #[test]
    fn unrelated_files_inside_trash_do_not_crash_listing() {
        let store = store();
        let id = Uuid::new_v4();
        write_note(&store.notes, &id, "uma nota de verdade");
        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");

        fs::write(store.trash.join("README"), "não é uma nota").expect("stray file");
        fs::write(store.trash.join("not-a-uuid.md"), "também não").expect("stray markdown");
        fs::write(store.trash.join("outro.txt"), "nem isto").expect("stray text");
        fs::create_dir(store.trash.join("uma-pasta")).expect("stray directory");
        fs::write(store.trash.join(".tmp.debris"), "resto").expect("stray temp");
        // A broken sidecar must cost its entry the exact date, not the listing.
        let orphan = Uuid::new_v4();
        fs::write(
            store.trash.join(note_file_name(&orphan)),
            "sem front matter",
        )
        .expect("orphan note");
        fs::write(
            store.trash.join(record_file_name(&orphan)),
            "{ isto não é json",
        )
        .expect("broken record");

        let entries = list_trash(&store.trash);
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert!(entries.iter().any(|entry| entry.note_id == id));
        let orphan_entry = entries
            .iter()
            .find(|entry| entry.note_id == orphan)
            .expect("the orphan is still listed");
        // No sidecar to read, so the file's own date answers instead.
        assert!(orphan_entry.deleted_at.is_some());
        assert_eq!(orphan_entry.label, "sem front matter");
    }

    #[test]
    fn a_listing_names_notes_the_way_search_does() {
        let store = store();
        let id = Uuid::new_v4();
        write_note(
            &store.notes,
            &id,
            "# Título da nota\n\nCorpo com MARCADOR-8391.",
        );
        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");

        let entries = list_trash(&store.trash);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "Título da nota");
        assert!(entries[0].snippet.contains("MARCADOR-8391"));
        assert_eq!(entries[0].deleted_at, Some(now()));
        // The front matter is bookkeeping, never part of what is shown.
        assert!(!entries[0].snippet.contains("note_it"));
    }

    #[test]
    fn a_trash_listing_shows_the_words_and_never_the_storage_around_them() {
        // 3.9UX.R.1. The trash is a presentation surface like any other and
        // reads notes through the same projection search does.
        let store = store();
        let id = Uuid::new_v4();
        write_note(
            &store.notes,
            &id,
            concat!(
                "# <mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">",
                "<span data-note-it-color=\"#64748B\" style=\"color:#64748B\">teste de verdade</span>",
                "</mark>\n\n**OBSERVAÇÃO:** MARCADOR-8391\n\n",
                "<!-- esse é um comentário de teste -->",
            ),
        );
        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");

        let entries = list_trash(&store.trash);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "teste de verdade");
        assert!(entries[0].snippet.contains("OBSERVAÇÃO: MARCADOR-8391"));
        assert!(entries[0].snippet.contains("esse é um comentário de teste"));

        for spelling in [
            "data-note-it-color",
            "data-note-it-highlight",
            "<span",
            "<mark",
            "<!--",
            "**",
            "note_it",
        ] {
            assert!(
                !entries[0].label.contains(spelling),
                "label leaks {spelling:?}"
            );
            assert!(
                !entries[0].snippet.contains(spelling),
                "snippet leaks {spelling:?}",
            );
        }
    }

    #[test]
    fn the_trash_is_listed_with_the_most_recently_deleted_first() {
        let store = store();
        let mut ids = Vec::new();
        for (index, minute) in [0u32, 5, 10].into_iter().enumerate() {
            let id = Uuid::new_v4();
            write_note(&store.notes, &id, &format!("nota {index}"));
            let instant = now() + chrono::Duration::minutes(i64::from(minute));
            move_to_trash(&store.notes, &store.trash, &id, instant).expect("move to trash");
            ids.push(id);
        }

        let entries = list_trash(&store.trash);
        assert_eq!(entries[0].note_id, ids[2]);
        assert_eq!(entries[2].note_id, ids[0]);
    }

    #[test]
    fn the_deletion_date_is_recorded_beside_the_note_and_never_inside_it() {
        let store = store();
        let id = Uuid::new_v4();
        write_note(&store.notes, &id, "conteúdo");
        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");

        let raw = fs::read_to_string(store.trash.join(note_file_name(&id))).expect("read note");
        assert!(
            !raw.contains("deleted_at"),
            "the Markdown must not be touched"
        );

        let record: TrashRecord = serde_json::from_str(
            &fs::read_to_string(store.trash.join(record_file_name(&id))).expect("read record"),
        )
        .expect("parse record");
        assert_eq!(record.deleted_at, now());
    }

    #[test]
    fn a_note_in_the_trash_is_reported_as_held() {
        let store = store();
        let id = Uuid::new_v4();
        assert!(!holds(&store.trash, &id));
        write_note(&store.notes, &id, "conteúdo");
        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");
        assert!(holds(&store.trash, &id));
        restore_from_trash(&store.notes, &store.trash, &id).expect("restore");
        assert!(!holds(&store.trash, &id));
    }

    #[test]
    fn restoring_an_entry_that_vanished_externally_reports_it_and_changes_nothing() {
        // Reliability audit, case C: something outside Note-it emptied the
        // trash between the listing and the click.
        let store = store();
        let id = Uuid::new_v4();
        write_note(&store.notes, &id, "conteúdo");
        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");

        fs::remove_file(store.trash.join(note_file_name(&id))).expect("external removal");

        assert_eq!(
            restore_from_trash(&store.notes, &store.trash, &id).expect_err("nothing to restore"),
            RestoreError::Missing
        );
        assert!(
            !store.notes.join(note_file_name(&id)).exists(),
            "a failed restore must not leave an empty note behind"
        );
    }

    #[test]
    fn a_note_with_damaged_front_matter_still_goes_to_the_trash_and_comes_back() {
        // Reliability audit, case K. The trash moves a file; it never parses
        // one. A note Note-it cannot open is exactly the note whose recovery
        // matters most, and it must not be the one deletion destroys.
        let store = store();
        let id = Uuid::new_v4();
        let path = store.notes.join(note_file_name(&id));
        let damaged =
            format!("---\nnote_it:\n  id: {id}\n  color: [isto não é yaml\n---\n\ntexto\n");
        fs::write(&path, &damaged).expect("write a damaged note");
        assert!(
            NoteDocument::parse(&damaged).is_err(),
            "the fixture has to be a note the parser rejects"
        );

        move_to_trash(&store.notes, &store.trash, &id, now()).expect("move to trash");
        assert_eq!(
            fs::read_to_string(store.trash.join(note_file_name(&id))).expect("read"),
            damaged
        );

        // It is still listed, named after its first readable line.
        let entries = list_trash(&store.trash);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].note_id, id);

        restore_from_trash(&store.notes, &store.trash, &id).expect("restore");
        assert_eq!(fs::read_to_string(&path).expect("read back"), damaged);
    }
}
