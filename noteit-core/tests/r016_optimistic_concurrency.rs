//! R-016: a write built on a base that has moved on must be refused.
//!
//! The writer lease in `coordination` already answers "who may write now", and
//! it answers it correctly — the R3 audit proved 48 concurrent operations all
//! committed and all persisted. What it cannot see is a writer whose *base* is
//! old, and that is the loss this suite exists to make impossible:
//!
//! ```text
//! T0  a client reads                        SHARED-BASE
//! T1  somebody else appends, coordinated    committed
//! T2  the client writes what it built at T0 committed  <- T1 is gone
//! ```
//!
//! Both writes took the lease. Both were told they committed. Nothing failed.

use noteit_core::metadata::NoteProperty;
use noteit_core::model::NoteDocument;
use noteit_core::revision::NoteRevision;
use noteit_core::storage::StorageManager;
use noteit_core::task;
use noteit_core::write::{
    self, NoteDraft, NoteMutation, WriteError, WriteOperation, WriteOutcomeKind,
};
use noteit_core::{NoteItCore, TaskStateFilter, Uuid};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

fn store() -> (TempDir, NoteItCore) {
    let tmp = tempdir().expect("tempdir");
    let storage = StorageManager::with_custom_paths(
        tmp.path().join("data/note-it/notes"),
        tmp.path().join("config/note-it"),
        tmp.path().join("state/note-it"),
        tmp.path().join("runtime/note-it"),
    )
    .expect("storage");
    (tmp, NoteItCore::from_storage(storage))
}

fn create(core: &NoteItCore, content: &str) -> Uuid {
    write::execute(
        core,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: content.to_string(),
                ..Default::default()
            },
        },
    )
    .expect("create")
    .note_id
}

fn mutate(
    core: &NoteItCore,
    id: &Uuid,
    mutation: NoteMutation,
    expected_revision: Option<NoteRevision>,
) -> Result<noteit_core::write::WriteOutcome, WriteError> {
    write::execute(
        core,
        &WriteOperation::MutateNote {
            selector: id.to_string(),
            mutation,
            expected_revision,
        },
    )
}

/// The revision a client would have been handed by a read.
fn revision_now(core: &NoteItCore, id: &Uuid) -> NoteRevision {
    NoteRevision::for_document(&core.read_note(id).expect("read")).expect("revision")
}

fn note_path(tmp: &TempDir, id: &Uuid) -> PathBuf {
    tmp.path().join(format!("data/note-it/notes/{id}.md"))
}

fn body_of(tmp: &TempDir, id: &Uuid) -> String {
    let raw = fs::read_to_string(note_path(tmp, id)).expect("read file");
    NoteDocument::parse(&raw).expect("parse").content
}

fn temp_debris(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with(".tmp"))
                .collect()
        })
        .unwrap_or_default()
}

fn backup_count(tmp: &TempDir) -> usize {
    fs::read_dir(tmp.path().join("data/note-it/backups"))
        .map(|entries| entries.flatten().count())
        .unwrap_or(0)
}

// --------------------------------------------------------------------- R016-A

#[test]
fn r016_a_a_stale_cooperative_writer_is_refused_and_the_earlier_write_survives() {
    // The exact scenario the R3 audit reproduced, with the precondition now
    // supplied. Before this mechanism existed the T1 append disappeared and
    // both writers were told `committed`.
    let (tmp, core) = store();
    let id = create(&core, "SHARED-BASE");

    // T0 — the client reads and keeps the revision it saw.
    let r0 = revision_now(&core, &id);
    let base_body = body_of(&tmp, &id);
    assert_eq!(base_body, "SHARED-BASE");

    // T1 — somebody else writes, coordinated, and really commits.
    let t1 = mutate(
        &core,
        &id,
        NoteMutation::Append {
            payload: "USER-TYPED-THIS-MEANWHILE".to_string(),
        },
        None,
    )
    .expect("the other writer commits");
    assert!(t1.changed);
    let r1 = t1
        .revision
        .clone()
        .expect("a committed write reports where it landed");
    assert_ne!(r0, r1, "a real change has to move the revision");
    assert_eq!(body_of(&tmp, &id), "SHARED-BASE\nUSER-TYPED-THIS-MEANWHILE");

    // T2 — the client writes back what it built from the base it read at T0.
    let before = fs::read(note_path(&tmp, &id)).expect("bytes before");
    let error = mutate(
        &core,
        &id,
        NoteMutation::ReplaceBody {
            body: format!("{base_body}\nAGENT-CONCLUSION"),
        },
        Some(r0.clone()),
    )
    .expect_err("a write built on a base that moved on must be refused");

    match &error {
        WriteError::RevisionConflict {
            note_id,
            expected_revision,
            current_revision,
        } => {
            assert_eq!(*note_id, id);
            assert_eq!(*expected_revision, r0);
            assert_eq!(
                *current_revision, r1,
                "the caller is told where the note is now"
            );
        }
        other => panic!("expected a revision conflict, got {other:?}"),
    }

    // The whole point: T1 is still there, byte for byte.
    assert_eq!(
        fs::read(note_path(&tmp, &id)).expect("bytes after"),
        before,
        "a refused write must not change a single byte"
    );
    assert_eq!(body_of(&tmp, &id), "SHARED-BASE\nUSER-TYPED-THIS-MEANWHILE");
}

// --------------------------------------------------------------------- R016-B

#[test]
fn r016_b_rereading_yields_a_revision_that_lets_a_conscious_write_through() {
    let (tmp, core) = store();
    let id = create(&core, "SHARED-BASE");
    let r0 = revision_now(&core, &id);

    let r1 = mutate(
        &core,
        &id,
        NoteMutation::Append {
            payload: "USER-TYPED-THIS-MEANWHILE".to_string(),
        },
        None,
    )
    .expect("commit")
    .revision
    .expect("revision");

    mutate(
        &core,
        &id,
        NoteMutation::ReplaceBody {
            body: "SHARED-BASE\nAGENT-CONCLUSION".to_string(),
        },
        Some(r0),
    )
    .expect_err("stale");

    // The client re-reads, sees what actually happened, and reconciles itself.
    let reread = core.read_note(&id).expect("re-read");
    let r1_again = NoteRevision::for_document(&reread).expect("revision");
    assert_eq!(
        r1_again, r1,
        "reading twice without a change is the same version"
    );

    let outcome = mutate(
        &core,
        &id,
        NoteMutation::ReplaceBody {
            body: format!("{}\nAGENT-CONCLUSION", reread.content),
        },
        Some(r1.clone()),
    )
    .expect("a write built on the current base is accepted");
    let r2 = outcome.revision.expect("revision");
    assert_ne!(r2, r1);
    assert_eq!(
        body_of(&tmp, &id),
        "SHARED-BASE\nUSER-TYPED-THIS-MEANWHILE\nAGENT-CONCLUSION",
        "nothing was lost: the reconciliation kept both"
    );
}

// --------------------------------------------------------------------- R016-C

#[test]
fn r016_c_a_matching_revision_commits_and_reports_the_new_one() {
    let (tmp, core) = store();
    let id = create(&core, "BASE");
    let r0 = revision_now(&core, &id);

    let outcome = mutate(
        &core,
        &id,
        NoteMutation::Append {
            payload: "ADDED".to_string(),
        },
        Some(r0.clone()),
    )
    .expect("committed");

    assert!(outcome.changed);
    let r1 = outcome.revision.expect("revision");
    assert_ne!(r1, r0);
    assert_eq!(
        r1,
        revision_now(&core, &id),
        "the reported revision is the one on disk"
    );
    assert_eq!(body_of(&tmp, &id), "BASE\nADDED");
}

// --------------------------------------------------------------------- R016-D

#[test]
fn r016_d_a_conditional_no_op_reports_the_current_revision_and_rewrites_nothing() {
    let (tmp, core) = store();
    let id = create(&core, "BASE");
    mutate(
        &core,
        &id,
        NoteMutation::AddTag {
            tag: "medicina".to_string(),
        },
        None,
    )
    .expect("tag");

    let r0 = revision_now(&core, &id);
    let before = fs::metadata(note_path(&tmp, &id)).expect("meta");
    let bytes_before = fs::read(note_path(&tmp, &id)).expect("bytes");

    // The tag is already there: nothing to do.
    let outcome = mutate(
        &core,
        &id,
        NoteMutation::AddTag {
            tag: "medicina".to_string(),
        },
        Some(r0.clone()),
    )
    .expect("a satisfied condition is not a failure");

    assert!(!outcome.changed, "nothing changed");
    assert_eq!(
        outcome.revision.expect("revision"),
        r0,
        "a no-op leaves the note at the revision it already had"
    );
    assert_eq!(fs::read(note_path(&tmp, &id)).expect("bytes"), bytes_before);
    assert_eq!(
        fs::metadata(note_path(&tmp, &id))
            .expect("meta")
            .modified()
            .ok(),
        before.modified().ok(),
        "a no-op must not rewrite the file just to produce a revision"
    );
}

// --------------------------------------------------------------------- R016-F

#[test]
fn r016_f_a_conflict_leaves_the_store_exactly_as_it_was() {
    let (tmp, core) = store();
    let id = create(&core, "BASE");
    let stale = revision_now(&core, &id);
    mutate(
        &core,
        &id,
        NoteMutation::Append {
            payload: "MOVED-ON".to_string(),
        },
        None,
    )
    .expect("move the note on");

    let notes_dir = tmp.path().join("data/note-it/notes");
    let path = note_path(&tmp, &id);
    let bytes_before = fs::read(&path).expect("bytes");
    let mtime_before = fs::metadata(&path).expect("meta").modified().ok();
    let backups_before = backup_count(&tmp);
    let debris_before = temp_debris(&notes_dir);
    let revision_before = revision_now(&core, &id);

    let error = mutate(
        &core,
        &id,
        NoteMutation::ReplaceBody {
            body: "OVERWRITTEN".to_string(),
        },
        Some(stale),
    )
    .expect_err("stale");
    assert!(matches!(error, WriteError::RevisionConflict { .. }));

    assert_eq!(fs::read(&path).expect("bytes"), bytes_before, "bytes");
    assert_eq!(
        fs::metadata(&path).expect("meta").modified().ok(),
        mtime_before,
        "mtime"
    );
    assert_eq!(
        backup_count(&tmp),
        backups_before,
        "a refused write is not worth a backup"
    );
    assert_eq!(
        temp_debris(&notes_dir),
        debris_before,
        "no temp file survived"
    );
    assert_eq!(revision_now(&core, &id), revision_before, "revision");
}

// --------------------------------------------------------------------- R016-J/K

#[test]
fn r016_jk_every_mutation_of_an_existing_note_honours_the_precondition() {
    // Consistency of the protocol over a matrix of "this one looks safe".
    // An append could in principle be replayed onto the new base — and must not
    // be, because the client asked for an operation on a specific version.
    let mutations: Vec<(&str, NoteMutation)> = vec![
        (
            "append",
            NoteMutation::Append {
                payload: "X".to_string(),
            },
        ),
        (
            "replace",
            NoteMutation::ReplaceBody {
                body: "X".to_string(),
            },
        ),
        ("clear", NoteMutation::ClearBody),
        (
            "add tag",
            NoteMutation::AddTag {
                tag: "nova".to_string(),
            },
        ),
        (
            "remove tag",
            NoteMutation::RemoveTag {
                tag: "existente".to_string(),
            },
        ),
        (
            "set property",
            NoteMutation::SetProperty {
                key: "chave".to_string(),
                value: "valor".to_string(),
            },
        ),
        (
            "remove property",
            NoteMutation::RemoveProperty {
                key: "presente".to_string(),
            },
        ),
    ];

    for (name, mutation) in mutations {
        let (tmp, core) = store();
        let id = create(&core, "BASE");
        // Give the note the tag and property the removals name, so every case
        // would really have changed something if it had been allowed through.
        mutate(
            &core,
            &id,
            NoteMutation::AddTag {
                tag: "existente".to_string(),
            },
            None,
        )
        .expect("seed tag");
        mutate(
            &core,
            &id,
            NoteMutation::SetProperty {
                key: "presente".to_string(),
                value: "sim".to_string(),
            },
            None,
        )
        .expect("seed property");

        let stale = revision_now(&core, &id);
        mutate(
            &core,
            &id,
            NoteMutation::Append {
                payload: "MOVED-ON".to_string(),
            },
            None,
        )
        .expect("move on");
        let bytes_before = fs::read(note_path(&tmp, &id)).expect("bytes");

        // Stale: refused.
        let error = mutate(&core, &id, mutation.clone(), Some(stale)).expect_err(name);
        assert!(
            matches!(error, WriteError::RevisionConflict { .. }),
            "{name} must refuse a stale base, got {error:?}"
        );
        assert_eq!(
            fs::read(note_path(&tmp, &id)).expect("bytes"),
            bytes_before,
            "{name} changed the file while refusing"
        );

        // Matching: accepted.
        let fresh = revision_now(&core, &id);
        mutate(&core, &id, mutation, Some(fresh))
            .unwrap_or_else(|error| panic!("{name} must accept a current base, got {error:?}"));
    }
}

// --------------------------------------------------------------------- R016-L

#[test]
fn r016_l_the_note_revision_is_checked_before_the_task_reference() {
    // Both preconditions can fail at once. The order is fixed and documented:
    // the revision comes first, because a task reference resolved against a
    // base that has moved on is meaningless either way, and the caller's real
    // problem is that it is looking at an old note.
    let (tmp, core) = store();
    let id = create(&core, "- [ ] Comprar pão");

    let tasks = core
        .list_tasks(TaskStateFilter::Pending, &Default::default(), None)
        .expect("tasks");
    let task_ref = tasks.items[0].task_ref.as_str().to_string();
    let stale = revision_now(&core, &id);

    // The note moves on in a way that also invalidates the task reference.
    mutate(
        &core,
        &id,
        NoteMutation::ReplaceBody {
            body: "- [ ] Outra coisa".to_string(),
        },
        None,
    )
    .expect("move on");
    let bytes_before = fs::read(note_path(&tmp, &id)).expect("bytes");

    let error = mutate(
        &core,
        &id,
        NoteMutation::CompleteTask {
            task_ref: task_ref.clone(),
        },
        Some(stale),
    )
    .expect_err("both preconditions are broken");
    assert!(
        matches!(error, WriteError::RevisionConflict { .. }),
        "the revision is the first question asked, got {error:?}"
    );
    assert_eq!(fs::read(note_path(&tmp, &id)).expect("bytes"), bytes_before);

    // With a current revision, the stale task reference is still caught: the
    // existing protection is not replaced by this one.
    let fresh = revision_now(&core, &id);
    let error = mutate(
        &core,
        &id,
        NoteMutation::CompleteTask { task_ref },
        Some(fresh),
    )
    .expect_err("the task reference is still stale");
    assert!(
        matches!(error, WriteError::StaleTaskRef { .. }),
        "got {error:?}"
    );
    assert_eq!(fs::read(note_path(&tmp, &id)).expect("bytes"), bytes_before);

    // And a conditional task write with both preconditions right works.
    let tasks = core
        .list_tasks(TaskStateFilter::Pending, &Default::default(), None)
        .expect("tasks");
    let current_ref = tasks.items[0].task_ref.as_str().to_string();
    let fresh = revision_now(&core, &id);
    let outcome = mutate(
        &core,
        &id,
        NoteMutation::CompleteTask {
            task_ref: current_ref,
        },
        Some(fresh),
    )
    .expect("both current");
    assert_eq!(outcome.kind, WriteOutcomeKind::TaskCompleted);
    assert!(task::parse_tasks(id, "", &body_of(&tmp, &id))[0].checked);
}

// --------------------------------------------------------------------- R016-M

#[test]
fn r016_m_a_note_without_front_matter_still_has_a_stable_revision() {
    let (tmp, core) = store();
    let id = Uuid::new_v4();
    let path = tmp.path().join(format!("data/note-it/notes/{id}.md"));
    fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    fs::write(&path, "BODY-WITHOUT-FRONT-MATTER\n").expect("write");

    let first = revision_now(&core, &id);
    let second = revision_now(&core, &id);
    assert_eq!(
        first, second,
        "reading twice without a change is one version"
    );

    let before = fs::read_dir(tmp.path().join("data/note-it/notes"))
        .expect("dir")
        .count();

    let outcome = mutate(
        &core,
        &id,
        NoteMutation::Append {
            payload: "APPENDED".to_string(),
        },
        Some(first.clone()),
    )
    .expect("a conditional write on an anchored note works");

    let after_revision = outcome.revision.expect("revision");
    assert_ne!(after_revision, first, "the write moved the revision");
    assert_eq!(after_revision, revision_now(&core, &id));
    assert_eq!(
        fs::read_dir(tmp.path().join("data/note-it/notes"))
            .expect("dir")
            .count(),
        before,
        "no ghost note was created"
    );
    // The file name stayed the authority all the way through.
    assert_eq!(core.read_note(&id).expect("read").metadata.id, id);
    assert!(body_of(&tmp, &id).contains("APPENDED"));

    // A stale revision is refused here too.
    let error = mutate(
        &core,
        &id,
        NoteMutation::Append {
            payload: "NOPE".to_string(),
        },
        Some(first),
    )
    .expect_err("stale");
    assert!(matches!(error, WriteError::RevisionConflict { .. }));
}

// --------------------------------------------------------------------- R016-N

#[test]
fn r016_n_an_identity_conflict_is_still_refused_and_no_revision_resolves_it() {
    // R-002/R-004 stay above this mechanism: a note whose front matter declares
    // another identifier is unreadable, and a revision cannot be used to talk
    // the store into writing it.
    let (tmp, core) = store();
    let legitimate = create(&core, "NOTA-B-LEGITIMA");
    let b_bytes_before = fs::read(note_path(&tmp, &legitimate)).expect("bytes");

    let impostor = Uuid::new_v4();
    let path = tmp.path().join(format!("data/note-it/notes/{impostor}.md"));
    let mut document = NoteDocument::new_empty();
    document.metadata.id = legitimate;
    document.content = "CORPO-DE-A".to_string();
    fs::write(&path, document.serialize().expect("serialize")).expect("write");

    // No revision can be obtained for it, because it cannot be read at all.
    assert!(core.read_note(&impostor).is_err());

    for expected in [None, Some(revision_now(&core, &legitimate))] {
        let error = mutate(
            &core,
            &impostor,
            NoteMutation::Append {
                payload: "NAO-DEVE-CHEGAR".to_string(),
            },
            expected,
        )
        .expect_err("an identity conflict is refused whatever the precondition says");
        assert!(
            matches!(error, WriteError::StoreUnavailable { .. }),
            "got {error:?}"
        );
    }

    assert_eq!(
        fs::read(note_path(&tmp, &legitimate)).expect("bytes"),
        b_bytes_before,
        "the legitimate note carrying that identifier was never touched"
    );
}

// --------------------------------------------------------------------- R016-O

#[test]
fn r016_o_a_revision_belongs_to_the_note_and_not_to_the_path_it_was_reached_by() {
    // The same store opened through a different spelling is the same store, and
    // the same note has to carry the same version — otherwise a client reading
    // through one alias could never write through another.
    let tmp = tempdir().expect("tempdir");
    let notes = tmp.path().join("data/note-it/notes");
    fs::create_dir_all(&notes).expect("dirs");

    let canonical = StorageManager::with_custom_paths(
        notes.clone(),
        tmp.path().join("config/note-it"),
        tmp.path().join("state/note-it"),
        tmp.path().join("runtime/note-it"),
    )
    .expect("storage");
    let core = NoteItCore::from_storage(canonical);
    let id = create(&core, "MESMO-CONTEUDO");
    let through_canonical = revision_now(&core, &id);

    // The same directory named with `.` and `..` segments.
    let aliased = StorageManager::with_custom_paths(
        tmp.path().join("data/./note-it/sub/../notes"),
        tmp.path().join("config/note-it"),
        tmp.path().join("state/note-it"),
        tmp.path().join("runtime/note-it"),
    )
    .expect("storage");
    let alias_core = NoteItCore::from_storage(aliased);
    let through_alias = revision_now(&alias_core, &id);

    assert_eq!(
        through_canonical, through_alias,
        "the revision must not depend on how the store was addressed"
    );

    // And a conditional write with the revision read through one alias is
    // accepted through the other.
    mutate(
        &alias_core,
        &id,
        NoteMutation::Append {
            payload: "VIA-ALIAS".to_string(),
        },
        Some(through_canonical),
    )
    .expect("one note, one version, however it was reached");
}

// ------------------------------------------------------- unconditional stays so

#[test]
fn r016_an_unconditional_write_is_still_last_writer_wins() {
    // The human contract does not change: `noteit editar <id> "texto"` with no
    // precondition means "replace the body", and it still does exactly that.
    // This is the behaviour R3 documented, kept on purpose for the caller who
    // did not ask for a condition.
    let (tmp, core) = store();
    let id = create(&core, "SHARED-BASE");
    let base = body_of(&tmp, &id);

    mutate(
        &core,
        &id,
        NoteMutation::Append {
            payload: "USER-TYPED-THIS-MEANWHILE".to_string(),
        },
        None,
    )
    .expect("commit");

    mutate(
        &core,
        &id,
        NoteMutation::ReplaceBody {
            body: format!("{base}\nAGENT-CONCLUSION"),
        },
        None,
    )
    .expect("an unconditional replace is not refused");

    assert_eq!(body_of(&tmp, &id), "SHARED-BASE\nAGENT-CONCLUSION");
}

// ------------------------------------------------------------- created revision

#[test]
fn r016_creating_a_note_reports_the_revision_it_was_created_at() {
    let (_tmp, core) = store();
    let outcome = write::execute(
        &core,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "NOVA".to_string(),
                ..Default::default()
            },
        },
    )
    .expect("create");
    let revision = outcome
        .revision
        .clone()
        .expect("a creation reports its revision");
    assert_eq!(revision, revision_now(&core, &outcome.note_id));

    // Which means the very next conditional write needs no read at all.
    mutate(
        &core,
        &outcome.note_id,
        NoteMutation::AddTag {
            tag: "nova".to_string(),
        },
        Some(revision),
    )
    .expect("chained without re-reading");
}

// ------------------------------------------------------------- property removal

#[test]
fn r016_a_property_write_carries_the_same_guarantee_as_a_body_write() {
    let (tmp, core) = store();
    let id = create(&core, "BASE");
    let stale = revision_now(&core, &id);

    // Somebody else sets a property.
    mutate(
        &core,
        &id,
        NoteMutation::SetProperty {
            key: "estado".to_string(),
            value: "revisado".to_string(),
        },
        None,
    )
    .expect("commit");
    let bytes_before = fs::read(note_path(&tmp, &id)).expect("bytes");

    // A client that read before that tries to set it to something else.
    let error = mutate(
        &core,
        &id,
        NoteMutation::SetProperty {
            key: "estado".to_string(),
            value: "rascunho".to_string(),
        },
        Some(stale),
    )
    .expect_err("stale");
    assert!(matches!(error, WriteError::RevisionConflict { .. }));
    assert_eq!(fs::read(note_path(&tmp, &id)).expect("bytes"), bytes_before);
    assert_eq!(
        core.read_note(&id)
            .expect("read")
            .user_metadata
            .properties
            .as_slice()[0]
            .value,
        "revisado",
        "the other writer's value stands"
    );
    let _ = NoteProperty {
        key: String::new(),
        value: String::new(),
    };
}

// --------------------------------------------------------------------- R016-G

#[test]
fn r016_g_a_stale_write_cannot_erase_text_the_editor_has_not_saved_yet() {
    // The case the file on disk cannot answer, and the reason the check lives
    // on the folded base rather than on the stored bytes.
    //
    // A note is open. The person has typed a paragraph the autosave has not
    // persisted. A client read the note before that and now writes a whole new
    // body. If the precondition were compared against the file, it would match
    // — the file has not changed — and the unsaved paragraph would be gone.
    let (tmp, core) = store();
    let id = create(&core, "BASE");

    // What a client reads: the persisted note.
    let r0 = revision_now(&core, &id);
    let committed = core.read_note(&id).expect("read");

    // What the window is really holding.
    let live_body = "BASE\nUSER-UNSAVED";

    let error = write::apply_over_live_body(
        &committed,
        live_body,
        &NoteMutation::ReplaceBody {
            body: "BASE\nAGENT".to_string(),
        },
        &Some(r0.clone()),
    )
    .expect_err("the live note is not the note that was read");

    match &error {
        WriteError::RevisionConflict {
            expected_revision,
            current_revision,
            ..
        } => {
            assert_eq!(*expected_revision, r0);
            assert_ne!(
                *current_revision, r0,
                "the current revision is the live note's, not the file's"
            );
        }
        other => panic!("expected a revision conflict, got {other:?}"),
    }

    // Nothing was written, so the user's unsaved text is still only in the
    // editor and the file still holds what it held.
    assert_eq!(body_of(&tmp, &id), "BASE");

    // The other half of the contract: while the editor holds unsaved text, the
    // live note is by definition not the note anybody read, so *every*
    // persisted revision is stale against it. That is deliberate and it is
    // safe in both directions — a client cannot obtain the live revision from
    // any read, and retrying with the `current_revision` it was told would fold
    // again and conflict again rather than slipping through.
    let second = write::apply_over_live_body(
        &committed,
        live_body,
        &NoteMutation::ReplaceBody {
            body: "BASE\nAGENT".to_string(),
        },
        &Some(r0),
    )
    .expect_err("still stale on a second attempt");
    assert!(matches!(second, WriteError::RevisionConflict { .. }));

    // Once the editor has nothing unsaved, the persisted revision is the live
    // one again and an ordinary conditional write goes straight through — the
    // mechanism must not turn Note-it's normal operation into a false conflict.
    let settled = revision_now(&core, &id);
    let live = write::apply_over_live_body(
        &committed,
        "BASE",
        &NoteMutation::Append {
            payload: "AGENT".to_string(),
        },
        &Some(settled),
    )
    .expect("nothing unsaved: the persisted revision describes the live note");
    let candidate = live.candidate.expect("candidate");
    assert!(candidate.content.contains("AGENT"), "{}", candidate.content);
}

#[test]
fn r016_g_an_ordinary_desktop_write_with_no_precondition_is_never_a_false_conflict() {
    // Regression guard for §59: the mechanism must not make Note-it's own
    // normal operation fail. No precondition means no conflict, ever.
    let (_tmp, core) = store();
    let id = create(&core, "BASE");
    let committed = core.read_note(&id).expect("read");

    let live = write::apply_over_live_body(
        &committed,
        "BASE\nUSER-UNSAVED",
        &NoteMutation::AddTag {
            tag: "etiqueta".to_string(),
        },
        &None,
    )
    .expect("an unconditional live write still works");
    let candidate = live.candidate.expect("candidate");
    assert!(candidate.content.contains("USER-UNSAVED"));
    assert!(live.adopted_unsaved_text);
}

// --------------------------------------------------------------------- R016-H

#[test]
fn r016_h_a_write_that_the_window_already_persisted_makes_an_older_client_stale() {
    let (tmp, core) = store();
    let id = create(&core, "BASE");
    let r0 = revision_now(&core, &id);

    // The window saves what the person typed. This is an ordinary persisted
    // write and it moves the note on.
    let committed = core.read_note(&id).expect("read");
    let live = write::apply_over_live_body(
        &committed,
        "BASE\nUSER-TYPED",
        &NoteMutation::AddTag {
            tag: "etiqueta".to_string(),
        },
        &None,
    )
    .expect("window write");
    let saved = live.candidate.expect("candidate");
    write::commit_addressed(&core, &id, &saved).expect("persist");
    let r1 = revision_now(&core, &id);
    assert_ne!(r0, r1);
    let bytes_before = fs::read(note_path(&tmp, &id)).expect("bytes");

    // The client that read before it is now stale, through the ordinary path.
    let error = mutate(
        &core,
        &id,
        NoteMutation::ReplaceBody {
            body: "BASE\nAGENT".to_string(),
        },
        Some(r0),
    )
    .expect_err("stale");
    assert!(matches!(error, WriteError::RevisionConflict { .. }));
    assert_eq!(fs::read(note_path(&tmp, &id)).expect("bytes"), bytes_before);
    assert!(body_of(&tmp, &id).contains("USER-TYPED"));
}

// --------------------------------------------------------------------- R016-K'

#[test]
fn r016_k_reopening_a_task_honours_the_precondition_like_every_other_mutation() {
    // `ReopenTask` was the one mutation the R4 matrix exercised only through
    // its sibling. Stated on its own here, because "consistency of the
    // protocol" is worth more than an argument that it must behave the same.
    let (tmp, core) = store();
    let id = create(&core, "- [ ] Comprar pão");

    // Complete it first, so there is something to reopen.
    let tasks = core
        .list_tasks(TaskStateFilter::Pending, &Default::default(), None)
        .expect("tasks");
    let pending_ref = tasks.items[0].task_ref.as_str().to_string();
    mutate(
        &core,
        &id,
        NoteMutation::CompleteTask {
            task_ref: pending_ref,
        },
        None,
    )
    .expect("complete");
    assert!(task::parse_tasks(id, "", &body_of(&tmp, &id))[0].checked);

    // The reference for the completed task, and the revision that describes it.
    let tasks = core
        .list_tasks(TaskStateFilter::Completed, &Default::default(), None)
        .expect("tasks");
    let completed_ref = tasks.items[0].task_ref.as_str().to_string();
    let stale = revision_now(&core, &id);

    // Somebody else moves the note on, without touching the task itself, so the
    // task reference stays valid and the revision is the only thing that is not.
    mutate(
        &core,
        &id,
        NoteMutation::AddTag {
            tag: "compras".to_string(),
        },
        None,
    )
    .expect("move the note on");
    let bytes_before = fs::read(note_path(&tmp, &id)).expect("bytes");

    // Stale revision: refused, and the file does not move.
    let error = mutate(
        &core,
        &id,
        NoteMutation::ReopenTask {
            task_ref: completed_ref.clone(),
        },
        Some(stale),
    )
    .expect_err("a reopen built on a base that moved on must be refused");
    assert!(
        matches!(error, WriteError::RevisionConflict { .. }),
        "got {error:?}"
    );
    assert_eq!(
        fs::read(note_path(&tmp, &id)).expect("bytes"),
        bytes_before,
        "a refused reopen must not change a single byte"
    );
    assert!(
        task::parse_tasks(id, "", &body_of(&tmp, &id))[0].checked,
        "the task is still completed: nothing was applied"
    );

    // Current revision: accepted, and the task really reopens.
    let fresh = revision_now(&core, &id);
    let outcome = mutate(
        &core,
        &id,
        NoteMutation::ReopenTask {
            task_ref: completed_ref,
        },
        Some(fresh),
    )
    .expect("a reopen on the current base is accepted");
    assert_eq!(outcome.kind, WriteOutcomeKind::TaskReopened);
    assert!(
        outcome.revision.is_some(),
        "a committed reopen reports its revision"
    );
    assert!(!task::parse_tasks(id, "", &body_of(&tmp, &id))[0].checked);
}

#[test]
fn r016_every_mutation_variant_passes_the_same_guard() {
    // Exhaustive over `NoteMutation`: if a variant is ever added, this stops
    // compiling until somebody decides whether it takes a precondition.
    // Every variant listed here is proven against a stale base below.
    fn variants() -> Vec<NoteMutation> {
        let sample = NoteMutation::Append {
            payload: "x".to_string(),
        };
        match &sample {
            // The match exists to make the compiler enumerate the type; the
            // list returned is what the test actually exercises.
            NoteMutation::Append { .. }
            | NoteMutation::ReplaceBody { .. }
            | NoteMutation::ClearBody
            | NoteMutation::AddTag { .. }
            | NoteMutation::RemoveTag { .. }
            | NoteMutation::SetProperty { .. }
            | NoteMutation::RemoveProperty { .. }
            | NoteMutation::CompleteTask { .. }
            | NoteMutation::ReopenTask { .. } => {}
        }
        vec![
            NoteMutation::Append {
                payload: "x".to_string(),
            },
            NoteMutation::ReplaceBody {
                body: "x".to_string(),
            },
            NoteMutation::ClearBody,
            NoteMutation::AddTag {
                tag: "nova".to_string(),
            },
            NoteMutation::RemoveTag {
                tag: "existente".to_string(),
            },
            NoteMutation::SetProperty {
                key: "k".to_string(),
                value: "v".to_string(),
            },
            NoteMutation::RemoveProperty {
                key: "presente".to_string(),
            },
            NoteMutation::CompleteTask {
                task_ref: "deadbeef".to_string(),
            },
            NoteMutation::ReopenTask {
                task_ref: "deadbeef".to_string(),
            },
        ]
    }

    for mutation in variants() {
        let (tmp, core) = store();
        let id = create(&core, "- [ ] tarefa");
        mutate(
            &core,
            &id,
            NoteMutation::AddTag {
                tag: "existente".to_string(),
            },
            None,
        )
        .expect("seed tag");
        mutate(
            &core,
            &id,
            NoteMutation::SetProperty {
                key: "presente".to_string(),
                value: "s".to_string(),
            },
            None,
        )
        .expect("seed property");

        let stale = revision_now(&core, &id);
        mutate(
            &core,
            &id,
            NoteMutation::Append {
                payload: "MOVED-ON".to_string(),
            },
            None,
        )
        .expect("move on");
        let before = fs::read(note_path(&tmp, &id)).expect("bytes");

        // Whatever else may be wrong with the request, the revision is checked
        // first and the file is untouched.
        let error = mutate(&core, &id, mutation.clone(), Some(stale)).unwrap_err();
        assert!(
            matches!(error, WriteError::RevisionConflict { .. }),
            "{mutation:?} must refuse a stale base before anything else, got {error:?}"
        );
        assert_eq!(
            fs::read(note_path(&tmp, &id)).expect("bytes"),
            before,
            "{mutation:?} changed the file while refusing"
        );
    }
}
