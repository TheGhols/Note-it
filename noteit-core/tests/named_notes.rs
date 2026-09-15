use noteit_core::{
    NoteDocument, NoteItCore, NoteNameResolution, StorageManager, Uuid, MAX_ALIASES,
    MAX_NOTE_NAME_CHARS,
};
use std::fs;
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
