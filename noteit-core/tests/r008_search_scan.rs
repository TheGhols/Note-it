//! R-008 for the path the desktop search actually takes.
//!
//! The listing and the metadata catalogue were made honest in 4.0R.R2.R1.1.
//! The reading behind search was not: it turned a scan that could not be
//! performed into an empty result, and a note it could not read into a note
//! that was never there. Both are lies a search must not tell, because an
//! empty palette looks exactly like a healthy store with nothing to show.
//!
//! Every scenario here is reachable without special privileges, so the
//! container CI runs the same proof this machine does.

use noteit_core::model::NoteDocument;
use noteit_core::search::{resolve_search_answer, SearchAnswer};
use noteit_core::warning::{ReadBatch, ReadWarningKind};
use noteit_core::{NoteItCore, StorageManager, StorePaths, Uuid};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn open_store(root: &Path) -> NoteItCore {
    let paths = StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        root.join("runtime/note-it"),
    );
    let storage = StorageManager::from_paths(paths).expect("open storage");
    let core = NoteItCore::from_storage(storage);
    core.storage().ensure_directories().expect("ensure dirs");
    core
}

/// Reopens the same store read-only, which is what the desktop does when it is
/// not the writer, and the only way to look at a store whose notes directory
/// can no longer be created.
fn reopen_read_only(root: &Path) -> NoteItCore {
    NoteItCore::open_read_only_at(StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        root.join("runtime/note-it"),
    ))
}

fn write_note(core: &NoteItCore, body: &str) -> Uuid {
    let mut doc = NoteDocument::new_empty();
    doc.content = body.to_string();
    core.storage().save_note_atomic(&doc).expect("save note");
    doc.metadata.id
}

/// Replaces the notes directory with a regular file: `read_dir` then fails
/// with `NotADirectory` for every user, root included.
fn make_store_unscannable(root: &Path) -> PathBuf {
    let notes_dir = root.join("data/note-it/notes");
    fs::remove_dir_all(&notes_dir).expect("remove notes dir");
    fs::write(&notes_dir, b"not a directory").expect("write file in its place");
    notes_dir
}

#[test]
fn r008_a_healthy_store_searches_exactly_as_before() {
    let tmp = tempdir().expect("tempdir");
    let core = open_store(tmp.path());

    let target = write_note(&core, "# Biópsia hepática\n\nencefalopatia");
    write_note(&core, "# Outra nota\n\ncafé da manhã");
    write_note(&core, "# Terceira\n\nnada a ver");

    let found = core
        .search_notes("encefalopatia")
        .expect("search must succeed");
    assert_eq!(found.items.len(), 1, "the matching note is found");
    assert_eq!(found.items[0].note_id, target);
    assert!(
        found.warnings.is_empty(),
        "a healthy store produces no warnings, got {:?}",
        found.warnings
    );

    // The empty query still lists, which is what makes the palette a way to
    // move between notes rather than only a way to search.
    let listed = core.search_notes("").expect("listing must succeed");
    assert_eq!(listed.items.len(), 3, "an empty query lists every note");
    assert!(listed.warnings.is_empty());

    // And a query nothing holds is an honest empty answer, not a failure.
    let nothing = core
        .search_notes("palavra-que-ninguem-escreveu")
        .expect("search");
    assert!(nothing.items.is_empty());
    assert!(nothing.warnings.is_empty());
    assert_eq!(
        resolve_search_answer(Ok(nothing)).notice,
        None,
        "nothing matched is not a notice"
    );
}

#[test]
fn r008_b_global_scan_failure_is_an_error_not_an_empty_result() {
    let tmp = tempdir().expect("tempdir");
    let core = open_store(tmp.path());
    write_note(&core, "# Nota\n\nconteudo procuravel");
    drop(core);

    make_store_unscannable(tmp.path());
    let core = reopen_read_only(tmp.path());

    for query in ["conteudo", ""] {
        let outcome = core.search_notes(query);
        assert!(
            outcome.is_err(),
            "a store that cannot be scanned must not answer {query:?} with an empty list"
        );
    }

    // The bodies the search reads are the same story one level down.
    assert!(
        core.storage().read_note_bodies_by_recency().is_err(),
        "reading every body must fail when the store cannot be listed"
    );
    assert!(
        core.storage().read_recent_note_bodies(10).is_err(),
        "reading recent bodies must fail when the store cannot be listed"
    );
}

#[test]
fn r008_c_an_unreadable_note_warns_and_the_healthy_ones_survive() {
    let tmp = tempdir().expect("tempdir");
    let core = open_store(tmp.path());

    let good = write_note(&core, "# Boa\n\nagulha no palheiro");
    let other = write_note(&core, "# Outra\n\nagulha tambem aqui");

    // A regular .md the listing accepts and `read_to_string` cannot decode.
    // Invalid UTF-8 fails for every user, so this is not a DAC scenario.
    let bad_id = Uuid::new_v4();
    let bad_path = tmp.path().join(format!("data/note-it/notes/{bad_id}.md"));
    fs::write(&bad_path, [0xff, 0xfe, 0x00, 0x9f]).expect("write undecodable note");

    let found = core.search_notes("agulha").expect("search still succeeds");
    let ids: Vec<Uuid> = found.items.iter().map(|r| r.note_id).collect();
    assert!(
        ids.contains(&good) && ids.contains(&other),
        "healthy notes survive"
    );
    assert_eq!(ids.len(), 2);

    let named: Vec<_> = found
        .warnings
        .iter()
        .filter(|w| w.note_id == Some(bad_id))
        .collect();
    assert_eq!(
        named.len(),
        1,
        "the unreadable note is named exactly once, got {:?}",
        found.warnings
    );
    assert_eq!(named[0].kind, ReadWarningKind::UnreadableNote);

    // And the answer does not present itself as wholly healthy.
    let answer = resolve_search_answer(Ok(found));
    assert_eq!(answer.results.len(), 2);
    assert!(
        answer.notice.is_some(),
        "a partial scan must carry a notice beside its results"
    );
}

#[test]
fn r008_d_an_entry_that_cannot_be_treated_as_a_note_is_reported() {
    let tmp = tempdir().expect("tempdir");
    let core = open_store(tmp.path());
    let good = write_note(&core, "# Boa\n\nalvo");

    // Named like a note, but there is no metadata of a note to be had here.
    // Previously this entry was dropped without a word.
    let odd_id = Uuid::new_v4();
    let odd_path = tmp.path().join(format!("data/note-it/notes/{odd_id}.md"));
    fs::create_dir(&odd_path).expect("create a directory named like a note");

    let (ids, warnings) = core
        .storage()
        .list_notes_by_recency_with_warnings()
        .expect("listing succeeds");
    assert_eq!(ids, vec![good], "only the real note is listed");
    let named: Vec<_> = warnings
        .iter()
        .filter(|w| w.note_id == Some(odd_id))
        .collect();
    assert_eq!(
        named.len(),
        1,
        "the entry that could not be treated as a note is reported, got {warnings:?}"
    );
    assert_eq!(named[0].kind, ReadWarningKind::IoError);

    // No note is dated to the Unix epoch to paper over a metadata failure:
    // the healthy note keeps its own order and the odd entry never entered.
    let found = core.search_notes("alvo").expect("search");
    assert_eq!(found.items.len(), 1);
    assert!(found.warnings.iter().any(|w| w.note_id == Some(odd_id)));
}

#[test]
fn r008_e_a_body_that_cannot_be_read_is_a_warning_not_a_silent_absence() {
    let tmp = tempdir().expect("tempdir");
    let core = open_store(tmp.path());
    let good = write_note(&core, "# Boa\n\ntexto legivel");

    let bad_id = Uuid::new_v4();
    let bad_path = tmp.path().join(format!("data/note-it/notes/{bad_id}.md"));
    fs::write(&bad_path, [0xf0, 0x28, 0x8c, 0x28]).expect("write undecodable note");

    let batch = core
        .storage()
        .read_note_bodies_by_recency()
        .expect("the scan itself succeeds");
    let read_ids: Vec<Uuid> = batch.items.iter().map(|(id, _)| *id).collect();
    assert_eq!(read_ids, vec![good], "the readable body is returned");
    assert!(
        batch
            .warnings
            .iter()
            .any(|w| w.note_id == Some(bad_id) && w.kind == ReadWarningKind::UnreadableNote),
        "the body that could not be read is named, got {:?}",
        batch.warnings
    );
}

#[test]
fn r008_f_the_answer_the_desktop_shows_never_calls_a_failure_no_results() {
    // `NoteItApp::answer_search` is exactly `resolve_search_answer(core.search_notes(q))`,
    // so this is the decision the desktop makes, not a restatement of it.
    let tmp = tempdir().expect("tempdir");
    let core = open_store(tmp.path());
    write_note(&core, "# Nota\n\nalvo");
    drop(core);
    make_store_unscannable(tmp.path());
    let core = reopen_read_only(tmp.path());

    let answer = resolve_search_answer(core.search_notes("alvo"));
    assert!(answer.results.is_empty());
    let notice = answer
        .notice
        .expect("a failed scan must reach the palette as a notice");
    assert!(
        notice.contains("falhou"),
        "the notice must say the search failed, got: {notice}"
    );

    // The three answers are distinguishable, which is the whole point.
    let healthy: SearchAnswer = resolve_search_answer(Ok(ReadBatch::new(Vec::new(), Vec::new())));
    assert_eq!(
        healthy.notice, None,
        "an honest empty answer carries no notice"
    );
}
