use noteit_core::{
    NoteDocument, NoteItCore, NoteNameResolution, ReadWarningKind, StorageManager, Uuid,
    MAX_ALIASES, MAX_NOTE_NAME_CHARS,
};
use std::fs;
use std::time::SystemTime;
use tempfile::TempDir;

fn store() -> (TempDir, NoteItCore) {
    let root = tempfile::tempdir().expect("tempdir");
    let storage = StorageManager::with_custom_paths(
        root.path().join("notes"),
        root.path().join("config"),
        root.path().join("state"),
        root.path().join("runtime"),
    )
    .expect("isolated store");
    (root, NoteItCore::from_storage(storage))
}

fn named(core: &NoteItCore, title: Option<&str>, aliases: &[&str]) -> Uuid {
    let mut note = NoteDocument::new_empty();
    note.set_title(title).expect("title");
    note.set_aliases(
        &aliases
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
    .expect("aliases");
    let id = note.metadata.id;
    core.storage().save_note_atomic(&note).expect("save");
    id
}

#[test]
fn old_notes_remain_unnamed_and_do_not_gain_empty_fields() {
    let id = Uuid::new_v4();
    let raw = format!("---\nnote_it:\n  id: {id}\n---\n\n# Nota antiga\n");
    let note = NoteDocument::parse_with_id(&raw, id).expect("old note");
    assert_eq!(note.title(), None);
    assert!(note.aliases().is_empty());
    let saved = note.serialize().expect("serialize");
    assert!(!saved.contains("title:"));
    assert!(!saved.contains("aliases:"));
    assert!(saved.contains("# Nota antiga"));
}

#[test]
fn title_aliases_and_external_yaml_round_trip_without_losing_data() {
    let id = Uuid::new_v4();
    let raw = format!(
        "---\nnote_it:\n  version: 1\n  id: {id}\n  color: yellow\n  created_at: 2026-09-15T10:00:00Z\n  updated_at: 2026-09-15T11:00:00Z\ntitle: 'Pré-operatório'\naliases:\n  - AVC\n  - Derrame\n  - avc\ntags:\n  - Medicina\nproperties:\n  fonte: Harrison\nfuture_tool:\n  unicode: '血圧'\nexternal_number: 42\n---\n\n# Corpo em português\n\nMarkdown **intacto**.\n"
    );
    let note = NoteDocument::parse_with_id(&raw, id).expect("external note");
    assert_eq!(note.title(), Some("Pré-operatório"));
    assert_eq!(note.aliases(), ["AVC", "Derrame"]);

    let saved = note.serialize().expect("serialize");
    let reparsed = NoteDocument::parse_with_id(&saved, id).expect("reparse");
    assert_eq!(reparsed.title(), note.title());
    assert_eq!(reparsed.aliases(), note.aliases());
    assert_eq!(reparsed.user_metadata, note.user_metadata);
    assert_eq!(reparsed.metadata.created_at, note.metadata.created_at);
    assert_eq!(reparsed.metadata.updated_at, note.metadata.updated_at);
    assert_eq!(reparsed.content, note.content);
    assert!(saved.contains("future_tool:"));
    assert!(saved.contains("unicode: 血圧"));
    assert!(saved.contains("external_number: 42"));
    assert_eq!(
        saved.matches("- AVC").count(),
        1,
        "stored duplicates are preserved"
    );
    assert!(saved.contains("- avc"));
}

#[test]
fn malformed_external_names_are_preserved_but_do_not_resolve() {
    let id = Uuid::new_v4();
    let raw = format!(
        "---\nnote_it:\n  id: {id}\ntitle:\n  pt: Nome\naliases: not-a-list\n---\n\nCorpo\n"
    );
    let note = NoteDocument::parse_with_id(&raw, id).expect("permissive read");
    assert_eq!(note.title(), None);
    assert!(note.aliases().is_empty());
    let saved = note.serialize().expect("serialize");
    assert!(saved.contains("title:\n  pt: Nome"));
    assert!(saved.contains("aliases: not-a-list"));
}

#[test]
fn explicit_writes_validate_dedupe_and_leave_timestamps_unchanged() {
    let note = NoteDocument::new_empty();
    let created = note.metadata.created_at;
    let updated = note.metadata.updated_at;

    let mut renamed = note.clone();
    assert!(renamed.set_title(Some("  Hipertensão  ")).expect("write"));
    assert_eq!(renamed.title(), Some("Hipertensão"));
    assert_eq!(renamed.metadata.created_at, created);
    assert_eq!(renamed.metadata.updated_at, updated);

    let mut aliased = renamed.clone();
    assert!(aliased
        .set_aliases(&["AVC".into(), "avc".into(), " Derrame ".into()])
        .expect("write"));
    assert_eq!(aliased.aliases(), ["AVC", "Derrame"]);
    assert_eq!(aliased.metadata.updated_at, updated);
    let serialized = aliased.serialize().expect("serialize");
    assert!(!serialized.contains("- avc"));
}

#[test]
fn write_limits_fail_explicitly_without_truncation() {
    let note = NoteDocument::new_empty();
    let mut candidate = note.clone();
    assert!(candidate
        .set_title(Some(&"x".repeat(MAX_NOTE_NAME_CHARS + 1)))
        .is_err());
    assert!(candidate
        .set_aliases(
            &(0..=MAX_ALIASES)
                .map(|i| format!("alias {i}"))
                .collect::<Vec<_>>()
        )
        .is_err());
    assert!(candidate.set_aliases(&["linha\nnova".into()]).is_err());
    assert_eq!(note.title(), None);
    assert!(note.aliases().is_empty());
}

#[test]
fn resolution_uses_one_namespace_and_deduplicates_each_note_by_uuid() {
    let (_tmp, core) = store();
    let one = named(&core, Some("AVC"), &["avc", "Derrame"]);
    assert_eq!(
        core.resolve_note_name("ávc").expect("resolve"),
        NoteNameResolution::Resolved { note_id: one }
    );
    assert_eq!(
        core.resolve_note_name("DERRAME").expect("resolve"),
        NoteNameResolution::Resolved { note_id: one }
    );

    let two = named(&core, Some("Acidente vascular cerebral"), &["AVC"]);
    let mut expected = vec![one, two];
    expected.sort_unstable();
    assert_eq!(
        core.resolve_note_name("avc").expect("resolve"),
        NoteNameResolution::Ambiguous {
            candidates: expected
        }
    );
}

#[test]
fn equivalent_titles_and_aliases_are_ambiguous_but_missing_names_are_not() {
    let (_tmp, core) = store();
    let title_a = named(&core, Some("Cardiologia"), &[]);
    let title_b = named(&core, Some("CARDIOLOGÍA"), &[]);
    let alias_a = named(&core, None, &["Coração"]);
    let alias_b = named(&core, None, &["coracao"]);
    named(&core, None, &[]);

    let mut titles = vec![title_a, title_b];
    titles.sort_unstable();
    assert_eq!(
        core.resolve_note_name("cardiologia").expect("resolve"),
        NoteNameResolution::Ambiguous { candidates: titles }
    );
    let mut aliases = vec![alias_a, alias_b];
    aliases.sort_unstable();
    assert_eq!(
        core.resolve_note_name("CORAÇÃO").expect("resolve"),
        NoteNameResolution::Ambiguous {
            candidates: aliases
        }
    );
    assert_eq!(
        core.resolve_note_name("inexistente").expect("resolve"),
        NoteNameResolution::Unresolved
    );
    assert_eq!(
        core.resolve_note_name("  ").expect("resolve"),
        NoteNameResolution::Unresolved
    );
}

#[test]
fn trash_is_outside_the_active_namespace_and_restore_can_make_it_ambiguous() {
    let (_tmp, core) = store();
    let first = named(&core, Some("Neurologia"), &[]);
    core.storage().move_note_to_trash(&first).expect("trash");
    assert_eq!(
        core.resolve_note_name("neurologia").unwrap(),
        NoteNameResolution::Unresolved
    );

    let second = named(&core, None, &["Neurología"]);
    assert_eq!(
        core.resolve_note_name("neurologia").unwrap(),
        NoteNameResolution::Resolved { note_id: second }
    );
    core.storage()
        .restore_note_from_trash(&first)
        .expect("restore");
    assert!(matches!(
        core.resolve_note_name("neurologia").unwrap(),
        NoteNameResolution::Ambiguous { candidates } if candidates.len() == 2
    ));
}

#[test]
fn saved_title_does_not_move_the_uuid_or_file() {
    let (_tmp, core) = store();
    let id = named(&core, Some("Antes"), &[]);
    let path = core.storage().note_path(&id);
    let mut note = core.read_note(&id).expect("read");
    note.set_title(Some("Depois")).expect("rename");
    core.storage().save_note_atomic(&note).expect("save");
    assert_eq!(note.metadata.id, id);
    assert!(path.exists());
    assert_eq!(fs::read_dir(core.storage().notes_dir()).unwrap().count(), 1);
    assert_eq!(
        core.resolve_note_name("Antes").unwrap(),
        NoteNameResolution::Unresolved
    );
    assert_eq!(
        core.resolve_note_name("Depois").unwrap(),
        NoteNameResolution::Resolved { note_id: id }
    );
}

/// Every file under the store, with its bytes and its modification time.
///
/// The read-only proof of 3.8 applied to resolution: content answers "was a
/// note rewritten?", `modified` answers "was it touched at all?", and the path
/// list answers "did a file or directory appear?".
fn fingerprint(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>, Option<SystemTime>)> {
    let mut result = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                result.push((path.clone(), Vec::new(), None));
                pending.push(path);
            } else {
                let modified = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                result.push((path.clone(), fs::read(&path).expect("read"), modified));
            }
        }
    }
    result.sort();
    result
}

/// Writes a file named like a note that no reader can parse.
///
/// Invalid UTF-8 rather than broken YAML: it fails at the one place every
/// reader has to pass, so the test cannot accidentally be measuring a lenient
/// front-matter parser instead of a degraded read.
fn unreadable(core: &NoteItCore) -> Uuid {
    let id = Uuid::new_v4();
    fs::write(core.storage().note_path(&id), [0xF0, 0x28, 0x8C, 0x28]).expect("write garbage");
    id
}

#[test]
fn an_unreadable_note_warns_without_deciding_for_the_notes_that_could_be_read() {
    let (_tmp, core) = store();
    let readable = named(&core, Some("AVC"), &[]);
    let broken = unreadable(&core);

    let (resolution, warnings) = core
        .resolve_note_name_with_warnings("AVC")
        .expect("resolution survives one unreadable note");

    assert_eq!(
        resolution,
        NoteNameResolution::Resolved { note_id: readable },
        "one corrupted file must not hide the note that does answer to the name"
    );
    assert_eq!(warnings.len(), 1, "exactly the unreadable note is reported");
    assert_eq!(warnings[0].note_id, Some(broken));
    assert_eq!(warnings[0].kind, ReadWarningKind::UnreadableNote);
    assert!(!warnings[0].message.is_empty());
}

#[test]
fn an_unreadable_note_never_becomes_a_candidate_of_its_own() {
    let (_tmp, core) = store();
    let broken = unreadable(&core);

    let (resolution, warnings) = core
        .resolve_note_name_with_warnings("AVC")
        .expect("a store of one corrupted note is still a store");

    assert_eq!(
        resolution,
        NoteNameResolution::Unresolved,
        "a note whose names could not be read has unknown names, not matching ones"
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].note_id, Some(broken));
    assert_eq!(warnings[0].kind, ReadWarningKind::UnreadableNote);
}

#[test]
fn an_unreadable_note_does_not_turn_two_candidates_into_an_answer() {
    let (_tmp, core) = store();
    let first = named(&core, Some("Cardiologia"), &[]);
    let second = named(&core, None, &["cardiología"]);
    unreadable(&core);

    let (resolution, warnings) = core
        .resolve_note_name_with_warnings("cardiologia")
        .expect("resolve");

    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(
        resolution,
        NoteNameResolution::Ambiguous {
            candidates: expected
        },
        "a corrupted third file cannot break a tie it was never part of"
    );
    assert_eq!(warnings.len(), 1);
}

#[test]
fn an_empty_store_resolves_to_nothing_without_warnings() {
    let (_tmp, core) = store();

    let (resolution, warnings) = core
        .resolve_note_name_with_warnings("qualquer nome")
        .expect("an empty store is not a failure");

    assert_eq!(resolution, NoteNameResolution::Unresolved);
    assert!(
        warnings.is_empty(),
        "nothing went wrong, so nothing is reported: {warnings:?}"
    );
    assert_eq!(
        fs::read_dir(core.storage().notes_dir()).unwrap().count(),
        0,
        "asking an empty store a question must not populate it"
    );
}

#[test]
fn composed_and_decomposed_spellings_name_the_same_note() {
    let (_tmp, core) = store();
    let composed = "Pr\u{e9}-operat\u{f3}rio";
    let decomposed = "Pre\u{301}-operato\u{301}rio";
    assert_ne!(
        composed, decomposed,
        "the two spellings must really differ byte for byte"
    );

    let id = named(&core, Some(composed), &[]);

    assert_eq!(
        core.resolve_note_name(decomposed).expect("resolve"),
        NoteNameResolution::Resolved { note_id: id },
        "a decomposed query reaches a composed title"
    );

    let other = named(&core, Some(decomposed), &[]);
    let mut expected = vec![id, other];
    expected.sort_unstable();
    assert_eq!(
        core.resolve_note_name(composed).expect("resolve"),
        NoteNameResolution::Ambiguous {
            candidates: expected
        },
        "two notes spelling one name differently collide instead of being chosen between"
    );
}

#[test]
fn resolving_never_writes_to_the_store() {
    let (tmp, core) = store();
    named(&core, Some("AVC"), &["Derrame"]);
    named(&core, Some("Cardiologia"), &[]);
    named(&core, Some("cardiología"), &[]);
    let trashed = named(&core, Some("Neurologia"), &[]);
    core.storage().move_note_to_trash(&trashed).expect("trash");
    unreadable(&core);

    // Modification times have a filesystem-dependent resolution; let the clock
    // move past it so a write during resolution could not hide inside one tick.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let before = fingerprint(tmp.path());

    for query in [
        "AVC",
        "Derrame",
        "cardiologia",
        "Neurologia",
        "inexistente",
        "   ",
        "",
        "Pr\u{e9}-operat\u{f3}rio",
    ] {
        let _ = core
            .resolve_note_name_with_warnings(query)
            .expect("resolve without writing");
    }

    assert_eq!(
        fingerprint(tmp.path()),
        before,
        "resolution touched the store: content, timestamps or files changed"
    );
}

#[test]
fn repeating_a_resolution_repeats_its_answer() {
    let (_tmp, core) = store();
    let resolved = named(&core, Some("AVC"), &[]);
    let first = named(&core, Some("Cardiologia"), &[]);
    let second = named(&core, None, &["cardiología"]);
    unreadable(&core);

    let mut expected = vec![first, second];
    expected.sort_unstable();

    for _ in 0..16 {
        let (one, warnings_one) = core
            .resolve_note_name_with_warnings("avc")
            .expect("resolve");
        assert_eq!(one, NoteNameResolution::Resolved { note_id: resolved });

        let (two, warnings_two) = core
            .resolve_note_name_with_warnings("cardiologia")
            .expect("resolve");
        assert_eq!(
            two,
            NoteNameResolution::Ambiguous {
                candidates: expected.clone()
            },
            "the candidate order is the answer, not the order the files were read in"
        );

        let (three, warnings_three) = core
            .resolve_note_name_with_warnings("inexistente")
            .expect("resolve");
        assert_eq!(three, NoteNameResolution::Unresolved);

        for warnings in [warnings_one, warnings_two, warnings_three] {
            assert_eq!(warnings.len(), 1, "the same file is reported every time");
        }
    }
}
