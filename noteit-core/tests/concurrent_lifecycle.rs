//! 5.0D.R0: all client writes enter through authority, including held leases.
use noteit_core::authority::{perform_at, WritePath};
use noteit_core::coordination::{WriteCoordinationPaths, WriterLease};
use noteit_core::write::{self, NoteDraft, NoteMutation, WriteError, WriteOperation};
use noteit_core::{NoteItCore, StorageManager};
use std::fs;
use tempfile::{tempdir, TempDir};

fn store() -> (TempDir, NoteItCore) {
    let dir = tempdir().unwrap();
    let storage = StorageManager::with_custom_paths(
        dir.path().join("data/notes"),
        dir.path().join("config"),
        dir.path().join("state"),
        dir.path().join("runtime"),
    )
    .unwrap();
    (dir, NoteItCore::from_storage(storage))
}

fn create(core: &NoteItCore) -> noteit_core::Uuid {
    let result = perform_at(
        core.paths(),
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "original".into(),
                ..Default::default()
            },
        },
    )
    .unwrap();
    assert_eq!(result.path, WritePath::Direct);
    result.outcome.note_id
}

fn revision(core: &NoteItCore, id: &noteit_core::Uuid) -> noteit_core::revision::NoteRevision {
    write::revision_of(&core.read_note(id).unwrap()).unwrap()
}

fn fingerprint(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut result = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                result.push((path.clone(), fs::read(path).unwrap()));
            }
        }
    }
    result.sort();
    result
}

#[test]
fn create_through_authority_has_an_absence_precondition() {
    let (_dir, core) = store();
    let first = create(&core);
    let bytes = fs::read(core.storage().note_path(&first)).unwrap();
    let second = create(&core);
    assert_ne!(first, second);
    assert_eq!(fs::read(core.storage().note_path(&first)).unwrap(), bytes);
    assert_eq!(core.read_note(&second).unwrap().content, "original");
}

#[test]
fn discard_and_restore_with_read_revisions_preserve_exact_bytes() {
    let (_dir, core) = store();
    let id = create(&core);
    let before = fs::read(core.storage().note_path(&id)).unwrap();
    let discarded = perform_at(
        core.paths(),
        &WriteOperation::DiscardNote {
            selector: id.to_string(),
            expected_revision: revision(&core, &id),
        },
    )
    .unwrap();
    assert_eq!(discarded.path, WritePath::Direct);
    assert!(!core.storage().note_path(&id).exists());
    let trash_path = core.storage().trash_dir().join(format!("{id}.md"));
    assert_eq!(fs::read(&trash_path).unwrap(), before);
    let expected = write::revision_of(&core.read_trash_note(&id).unwrap()).unwrap();
    let restored = perform_at(
        core.paths(),
        &WriteOperation::RestoreFromTrashAtRevision {
            selector: id.to_string(),
            expected_revision: expected.clone(),
        },
    )
    .unwrap();
    assert_eq!(restored.outcome.revision, Some(expected));
    assert!(!trash_path.exists());
    assert_eq!(fs::read(core.storage().note_path(&id)).unwrap(), before);
}

#[test]
fn stale_discard_changes_neither_source_nor_destination() {
    let (_dir, core) = store();
    let id = create(&core);
    let old = revision(&core, &id);
    perform_at(
        core.paths(),
        &WriteOperation::MutateNote {
            selector: id.to_string(),
            expected_revision: Some(old.clone()),
            mutation: NoteMutation::Append {
                payload: "new content".into(),
            },
        },
    )
    .unwrap();
    // Include an occupied trash destination in the preservation assertion.
    fs::create_dir_all(core.storage().trash_dir()).unwrap();
    fs::write(
        core.storage().trash_dir().join(format!("{id}.md")),
        b"other trash bytes",
    )
    .unwrap();
    let before = (
        fingerprint(core.storage().notes_dir()),
        fingerprint(core.storage().trash_dir()),
    );
    assert!(matches!(
        perform_at(
            core.paths(),
            &WriteOperation::DiscardNote {
                selector: id.to_string(),
                expected_revision: old,
            }
        ),
        Err(WriteError::RevisionConflict { .. })
    ));
    assert_eq!(
        (
            fingerprint(core.storage().notes_dir()),
            fingerprint(core.storage().trash_dir())
        ),
        before
    );
}

#[test]
fn stale_restore_preserves_trash_sidecar_and_occupied_live_destination() {
    let (_dir, core) = store();
    let id = create(&core);
    let old = revision(&core, &id);
    perform_at(
        core.paths(),
        &WriteOperation::DiscardNote {
            selector: id.to_string(),
            expected_revision: old.clone(),
        },
    )
    .unwrap();
    let mut changed = core.read_trash_note(&id).unwrap();
    changed.content = "external trash edit".into();
    fs::write(
        core.storage().trash_dir().join(format!("{id}.md")),
        changed.serialize().unwrap(),
    )
    .unwrap();
    fs::write(core.storage().note_path(&id), b"occupied live destination").unwrap();
    let before = (
        fingerprint(core.storage().notes_dir()),
        fingerprint(core.storage().trash_dir()),
    );
    assert!(matches!(
        perform_at(
            core.paths(),
            &WriteOperation::RestoreFromTrashAtRevision {
                selector: id.to_string(),
                expected_revision: old,
            }
        ),
        Err(WriteError::RevisionConflict { .. })
    ));
    assert_eq!(
        (
            fingerprint(core.storage().notes_dir()),
            fingerprint(core.storage().trash_dir())
        ),
        before
    );
}

#[test]
fn conditioned_restore_never_overwrites_a_live_note_even_with_correct_revision() {
    let (_dir, core) = store();
    let id = create(&core);
    let expected = revision(&core, &id);
    perform_at(
        core.paths(),
        &WriteOperation::DiscardNote {
            selector: id.to_string(),
            expected_revision: expected.clone(),
        },
    )
    .unwrap();
    fs::write(core.storage().note_path(&id), b"live note").unwrap();
    let before = (
        fingerprint(core.storage().notes_dir()),
        fingerprint(core.storage().trash_dir()),
    );
    assert!(matches!(
        perform_at(
            core.paths(),
            &WriteOperation::RestoreFromTrashAtRevision {
                selector: id.to_string(),
                expected_revision: expected,
            }
        ),
        Err(WriteError::TrashTargetOccupied { .. })
    ));
    assert_eq!(
        (
            fingerprint(core.storage().notes_dir()),
            fingerprint(core.storage().trash_dir())
        ),
        before
    );
}

#[test]
fn stale_restore_does_not_publish_an_absent_destination() {
    let (_dir, core) = store();
    let id = create(&core);
    let old = revision(&core, &id);
    perform_at(
        core.paths(),
        &WriteOperation::DiscardNote {
            selector: id.to_string(),
            expected_revision: old.clone(),
        },
    )
    .unwrap();
    let mut document = core.read_trash_note(&id).unwrap();
    document.content = "changed in trash".into();
    fs::write(
        core.paths().trash_dir.join(format!("{id}.md")),
        document.serialize().unwrap(),
    )
    .unwrap();
    let before = fingerprint(core.storage().trash_dir());
    assert!(matches!(
        perform_at(
            core.paths(),
            &WriteOperation::RestoreFromTrashAtRevision {
                selector: id.to_string(),
                expected_revision: old,
            }
        ),
        Err(WriteError::RevisionConflict { .. })
    ));
    assert!(!core.storage().note_path(&id).exists());
    assert_eq!(fingerprint(core.storage().trash_dir()), before);
}

#[test]
fn new_operations_cannot_bypass_an_unreachable_lease_holder() {
    let (_dir, core) = store();
    let id = create(&core);
    let expected = revision(&core, &id);
    let coordination = WriteCoordinationPaths::for_store(core.paths()).unwrap();
    coordination.prepare().unwrap();
    let _lease = WriterLease::try_acquire_prepared(&coordination)
        .unwrap()
        .unwrap();
    let before = (
        fingerprint(core.storage().notes_dir()),
        fingerprint(core.storage().trash_dir()),
    );
    for operation in [
        WriteOperation::CreateNote {
            draft: NoteDraft::default(),
        },
        WriteOperation::DiscardNote {
            selector: id.to_string(),
            expected_revision: expected.clone(),
        },
        WriteOperation::RestoreFromTrashAtRevision {
            selector: id.to_string(),
            expected_revision: expected,
        },
    ] {
        assert!(matches!(
            perform_at(core.paths(), &operation),
            Err(WriteError::AuthorityUnavailable { .. })
        ));
        assert_eq!(
            (
                fingerprint(core.storage().notes_dir()),
                fingerprint(core.storage().trash_dir())
            ),
            before
        );
    }
}

#[test]
fn conditional_wire_variants_require_a_non_null_revision() {
    for operation in ["discard_note", "restore_from_trash_at_revision"] {
        for suffix in [
            "",
            ",\"expected_revision\":null",
            ",\"expected_revision\":\"\"",
        ] {
            let raw = format!("{{\"operation\":\"{operation}\",\"selector\":\"abc\"{suffix}}}");
            assert!(serde_json::from_str::<WriteOperation>(&raw).is_err());
        }
    }
    let legacy: WriteOperation =
        serde_json::from_str(r#"{"operation":"restore_from_trash","selector":"abc"}"#).unwrap();
    assert!(matches!(legacy, WriteOperation::RestoreFromTrash { .. }));
}
