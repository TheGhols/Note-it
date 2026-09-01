use noteit_core::model::NoteDocument;
use noteit_core::storage::StorageManager;
use noteit_core::NoteItCore;
use tempfile::tempdir;

fn synthetic_core() -> (tempfile::TempDir, NoteItCore) {
    let root = tempdir().expect("temporary core store");
    let storage = StorageManager::with_custom_paths(
        root.path().join("data/note-it/notes"),
        root.path().join("config/note-it"),
        root.path().join("state/note-it"),
        root.path().join("runtime/note-it"),
    )
    .expect("synthetic storage");
    (root, NoteItCore::from_storage(storage))
}

#[test]
fn shared_api_lists_reads_and_searches_the_same_stored_note() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "# Arquitetura\n\nfronteira reutilizável".to_string();
    core.storage()
        .save_note_atomic(&note)
        .expect("save through the established storage implementation");

    assert_eq!(core.list_notes().expect("list"), vec![note.metadata.id]);
    assert_eq!(
        core.read_note(&note.metadata.id).expect("read").content,
        note.content
    );
    let results = core.search_notes("reutilizavel");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note_id, note.metadata.id);
}

#[test]
fn trash_and_study_queries_use_the_same_synthetic_store() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "# Recuperável\n\nconteúdo".to_string();
    core.storage().save_note_atomic(&note).expect("save note");
    core.storage()
        .move_note_to_trash(&note.metadata.id)
        .expect("trash note");

    let trash = core.list_trash();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].note_id, note.metadata.id);
    assert!(core.list_notes().expect("list live notes").is_empty());
    assert!(core.study_state().expect("study state").cards.is_empty());
}

#[test]
fn workspace_package_version_is_shared() {
    assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0");
}
