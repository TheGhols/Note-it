//! The Write API's domain behaviour, against a real store on disk.
//!
//! Everything here goes through the same entry points both adapters use, so
//! what these prove about `noteit adicionar` is equally true of the same
//! change made by the desktop instance on a CLI's behalf.

use noteit_core::metadata::NoteProperty;
use noteit_core::model::NoteDocument;
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
                ..NoteDraft::default()
            },
        },
    )
    .expect("create")
    .note_id
}

fn mutate(core: &NoteItCore, id: Uuid, mutation: NoteMutation) -> Result<bool, WriteError> {
    write::execute(
        core,
        &WriteOperation::MutateNote {
            selector: id.to_string(),
            mutation,

            expected_revision: None,
        },
    )
    .map(|outcome| outcome.changed)
}

fn body(core: &NoteItCore, id: Uuid) -> String {
    core.read_note(&id).expect("read").content
}

fn note_path(core: &NoteItCore, id: Uuid) -> PathBuf {
    core.paths().note_path(&id)
}

fn bytes_of(path: &Path) -> Vec<u8> {
    fs::read(path).expect("read the note file")
}

fn first_task_ref(core: &NoteItCore, id: Uuid) -> String {
    task::parse_tasks(id, "nota", &body(core, id))[0]
        .task_ref
        .as_str()
        .to_string()
}

// 1 -------------------------------------------------------------------------
#[test]
fn creating_a_note_stores_it_and_answers_with_its_identifier() {
    let (_tmp, core) = store();
    let outcome = write::execute(
        &core,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "# Choque distributivo".into(),
                ..NoteDraft::default()
            },
        },
    )
    .expect("create");

    assert_eq!(outcome.kind, WriteOutcomeKind::NoteCreated);
    assert!(outcome.changed);
    assert!(note_path(&core, outcome.note_id).is_file());
    assert_eq!(body(&core, outcome.note_id), "# Choque distributivo");
}

// 2 -------------------------------------------------------------------------
#[test]
fn a_note_can_be_created_with_its_tags_and_properties_already_on_it() {
    let (_tmp, core) = store();
    let outcome = write::execute(
        &core,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "corpo".into(),
                tags: vec!["Medicina".into(), "PBL".into()],
                properties: vec![NoteProperty {
                    key: "fonte".into(),
                    value: "Harrison".into(),
                }],
            },
        },
    )
    .expect("create");

    let document = core.read_note(&outcome.note_id).expect("read");
    assert_eq!(document.user_metadata.tags.as_slice(), ["Medicina", "PBL"]);
    assert_eq!(
        document.user_metadata.properties.as_slice()[0].value,
        "Harrison"
    );
}

#[test]
fn a_note_can_be_created_with_nothing_in_it() {
    let (_tmp, core) = store();
    let id = create(&core, "");
    assert_eq!(body(&core, id), "");
}

// 3, 4 ----------------------------------------------------------------------
#[test]
fn appending_puts_the_payload_after_one_line_break() {
    let (_tmp, core) = store();
    let id = create(&core, "ABC");
    assert!(mutate(
        &core,
        id,
        NoteMutation::Append {
            payload: "XYZ".into()
        }
    )
    .expect("append"));
    assert_eq!(body(&core, id), "ABC\nXYZ");
}

#[test]
fn appending_to_an_empty_note_gives_exactly_the_payload() {
    let (_tmp, core) = store();
    let id = create(&core, "");
    mutate(
        &core,
        id,
        NoteMutation::Append {
            payload: "primeiro".into(),
        },
    )
    .expect("append");
    assert_eq!(body(&core, id), "primeiro");
}

#[test]
fn appending_nothing_at_all_is_refused_as_invalid_usage() {
    let (_tmp, core) = store();
    let id = create(&core, "ABC");
    let error = mutate(
        &core,
        id,
        NoteMutation::Append {
            payload: String::new(),
        },
    )
    .expect_err("an empty payload");
    assert!(
        matches!(error, WriteError::InvalidInput { .. }),
        "{error:?}"
    );
    assert_eq!(body(&core, id), "ABC");
}

// 5, 6 ----------------------------------------------------------------------
#[test]
fn editing_replaces_the_whole_body() {
    let (_tmp, core) = store();
    let id = create(&core, "antes\ncom linhas");
    mutate(
        &core,
        id,
        NoteMutation::ReplaceBody {
            body: "depois".into(),
        },
    )
    .expect("replace");
    assert_eq!(body(&core, id), "depois");
}

#[test]
fn emptying_a_note_needs_the_explicit_operation() {
    let (_tmp, core) = store();
    let id = create(&core, "valioso");

    let error = mutate(
        &core,
        id,
        NoteMutation::ReplaceBody {
            body: "\n\n".into(),
        },
    )
    .expect_err("an accidental empty pipe");
    assert!(
        matches!(error, WriteError::InvalidInput { .. }),
        "{error:?}"
    );
    assert_eq!(body(&core, id), "valioso");

    mutate(&core, id, NoteMutation::ClearBody).expect("clear");
    assert_eq!(body(&core, id), "");
}

// 7 – 10 --------------------------------------------------------------------
#[test]
fn a_tag_is_added_once_and_a_repeat_rewrites_nothing() {
    let (_tmp, core) = store();
    let id = create(&core, "corpo");

    assert!(mutate(
        &core,
        id,
        NoteMutation::AddTag {
            tag: "Medicina".into()
        }
    )
    .expect("add"));

    let before = bytes_of(&note_path(&core, id));
    assert!(
        !mutate(
            &core,
            id,
            NoteMutation::AddTag {
                tag: "medicina".into()
            }
        )
        .expect("add again"),
        "a tag the note already carries is not a change"
    );
    assert_eq!(
        bytes_of(&note_path(&core, id)),
        before,
        "a no-op tag rewrote the file"
    );
}

#[test]
fn a_tag_is_removed_and_removing_an_absent_one_rewrites_nothing() {
    let (_tmp, core) = store();
    let id = create(&core, "corpo");
    mutate(
        &core,
        id,
        NoteMutation::AddTag {
            tag: "Urgência".into(),
        },
    )
    .expect("add");

    // Identity is case- and accent-insensitive on the way out too.
    assert!(mutate(
        &core,
        id,
        NoteMutation::RemoveTag {
            tag: "urgencia".into()
        }
    )
    .expect("remove"));
    assert!(core
        .read_note(&id)
        .expect("read")
        .user_metadata
        .tags
        .is_empty());

    let before = bytes_of(&note_path(&core, id));
    assert!(!mutate(
        &core,
        id,
        NoteMutation::RemoveTag {
            tag: "Ausente".into()
        }
    )
    .expect("remove absent"));
    assert_eq!(bytes_of(&note_path(&core, id)), before);
}

// 11 – 14 -------------------------------------------------------------------
#[test]
fn a_property_is_set_updated_and_removed_with_no_op_repeats_writing_nothing() {
    let (_tmp, core) = store();
    let id = create(&core, "corpo");

    mutate(
        &core,
        id,
        NoteMutation::SetProperty {
            key: "status".into(),
            value: "revisando".into(),
        },
    )
    .expect("set");

    let before = bytes_of(&note_path(&core, id));
    assert!(
        !mutate(
            &core,
            id,
            NoteMutation::SetProperty {
                key: "STATUS".into(),
                value: "revisando".into(),
            },
        )
        .expect("set the same value"),
        "setting the value a property already has is not a change"
    );
    assert_eq!(bytes_of(&note_path(&core, id)), before);

    assert!(mutate(
        &core,
        id,
        NoteMutation::SetProperty {
            key: "status".into(),
            value: "concluído".into(),
        },
    )
    .expect("set a new value"));
    assert_eq!(
        core.read_note(&id)
            .expect("read")
            .user_metadata
            .properties
            .as_slice()[0]
            .value,
        "concluído"
    );

    assert!(mutate(
        &core,
        id,
        NoteMutation::RemoveProperty {
            key: "status".into()
        },
    )
    .expect("remove"));

    let before = bytes_of(&note_path(&core, id));
    assert!(!mutate(
        &core,
        id,
        NoteMutation::RemoveProperty {
            key: "ausente".into()
        },
    )
    .expect("remove absent"));
    assert_eq!(bytes_of(&note_path(&core, id)), before);
}

// 15 – 18 -------------------------------------------------------------------
#[test]
fn metadata_changes_never_move_a_timestamp_and_content_changes_move_only_updated_at() {
    let (_tmp, core) = store();
    let id = create(&core, "corpo");
    let original = core.read_note(&id).expect("read");
    let created_at = original.metadata.created_at;
    let updated_at = original.metadata.updated_at;

    // Tags and properties are what a note is *about*, not an edit of it. The
    // file is reserialized and neither date moves.
    for mutation in [
        NoteMutation::AddTag {
            tag: "Medicina".into(),
        },
        NoteMutation::SetProperty {
            key: "fonte".into(),
            value: "Harrison".into(),
        },
        NoteMutation::RemoveTag {
            tag: "Medicina".into(),
        },
        NoteMutation::RemoveProperty {
            key: "fonte".into(),
        },
    ] {
        mutate(&core, id, mutation).expect("metadata mutation");
        let document = core.read_note(&id).expect("read");
        assert_eq!(document.metadata.created_at, created_at);
        assert_eq!(
            document.metadata.updated_at, updated_at,
            "semantic metadata moved the modification date"
        );
    }

    std::thread::sleep(std::time::Duration::from_millis(5));
    mutate(
        &core,
        id,
        NoteMutation::Append {
            payload: "mais".into(),
        },
    )
    .expect("append");
    let appended = core.read_note(&id).expect("read");
    assert_eq!(appended.metadata.created_at, created_at);
    assert!(appended.metadata.updated_at > updated_at);

    std::thread::sleep(std::time::Duration::from_millis(5));
    mutate(
        &core,
        id,
        NoteMutation::ReplaceBody {
            body: "outro corpo".into(),
        },
    )
    .expect("replace");
    let replaced = core.read_note(&id).expect("read");
    assert_eq!(replaced.metadata.created_at, created_at, "created_at moved");
    assert!(replaced.metadata.updated_at > appended.metadata.updated_at);
}

// 19 ------------------------------------------------------------------------
#[test]
fn yaml_written_by_another_tool_survives_every_mutation() {
    let (_tmp, core) = store();
    let id = Uuid::new_v4();
    let raw = format!(
        concat!(
            "---\n",
            "note_it:\n",
            "  version: 1\n",
            "  id: {}\n",
            "future_tool:\n",
            "  enabled: true\n",
            "  nested:\n",
            "    - um\n",
            "    - dois\n",
            "---\n\n",
            "texto\n",
        ),
        id
    );
    fs::write(note_path(&core, id), raw).expect("place the note");

    for mutation in [
        NoteMutation::Append {
            payload: "mais".into(),
        },
        NoteMutation::AddTag {
            tag: "Projeto".into(),
        },
        NoteMutation::SetProperty {
            key: "tipo".into(),
            value: "estudo".into(),
        },
    ] {
        mutate(&core, id, mutation).expect("mutation");
        let stored = fs::read_to_string(note_path(&core, id)).expect("read");
        assert!(stored.contains("future_tool"), "unknown YAML was dropped");
        assert!(stored.contains("- dois"), "unknown YAML lost its nesting");
    }
}

// 20, 21 --------------------------------------------------------------------
#[test]
fn a_write_that_fails_before_the_commit_point_leaves_the_note_alone_and_the_retry_works() {
    let (_tmp, core) = store();
    let id = create(&core, "original");
    let path = note_path(&core, id);
    let before = bytes_of(&path);

    // The atomic writer builds `.tmp.<name>.<pid>` beside the target before it
    // renames. A directory already sitting on that exact name makes creating
    // the temp file fail — for every user, root included, because it is path
    // resolution and not a permission bit — so the failure lands strictly
    // before the commit point.
    let blocker = core
        .paths()
        .notes_dir
        .join(format!(".tmp.{id}.md.{}", std::process::id()));
    fs::create_dir(&blocker).expect("block the temp file");

    let error = mutate(
        &core,
        id,
        NoteMutation::Append {
            payload: "acréscimo".into(),
        },
    )
    .expect_err("the write must fail");
    assert!(matches!(error, WriteError::Persistence { .. }), "{error:?}");
    assert_eq!(bytes_of(&path), before, "a failed write changed the file");

    fs::remove_dir(&blocker).expect("unblock");
    assert!(mutate(
        &core,
        id,
        NoteMutation::Append {
            payload: "acréscimo".into(),
        },
    )
    .expect("the retry must succeed"));
    assert_eq!(body(&core, id), "original\nacréscimo");
}

// 22 – 24 -------------------------------------------------------------------
#[test]
fn a_note_is_restored_from_the_trash_by_a_prefix_and_comes_back_byte_for_byte() {
    let (_tmp, core) = store();
    let id = create(&core, "# Para a lixeira");
    let before = bytes_of(&note_path(&core, id));
    core.storage().move_note_to_trash(&id).expect("trash");
    assert!(!note_path(&core, id).exists());

    let prefix = &id.as_simple().to_string()[..8];
    let outcome = write::execute(
        &core,
        &WriteOperation::RestoreFromTrash {
            selector: prefix.to_string(),
        },
    )
    .expect("restore by prefix");
    assert_eq!(outcome.kind, WriteOutcomeKind::NoteRestored);
    assert_eq!(outcome.note_id, id);
    assert_eq!(
        bytes_of(&note_path(&core, id)),
        before,
        "restoring is not editing"
    );
}

#[test]
fn restoring_over_a_live_note_is_refused_and_changes_neither_file() {
    let (_tmp, core) = store();
    let id = create(&core, "# Original");
    core.storage().move_note_to_trash(&id).expect("trash");

    // Put a different note back under the same identifier, the way an external
    // copy or a sync tool might.
    let mut impostor = NoteDocument::new_empty();
    impostor.metadata.id = id;
    impostor.content = "# Nota viva diferente".into();
    core.storage().save_note_atomic(&impostor).expect("place");
    let live_before = bytes_of(&note_path(&core, id));

    let error = write::execute(
        &core,
        &WriteOperation::RestoreFromTrash {
            selector: id.to_string(),
        },
    )
    .expect_err("an occupied identifier must be refused");
    assert!(
        matches!(error, WriteError::TrashTargetOccupied { .. }),
        "{error:?}"
    );
    assert_eq!(bytes_of(&note_path(&core, id)), live_before);
    assert_eq!(core.list_trash().len(), 1, "the trash entry was consumed");
}

#[test]
fn the_trash_selector_refuses_paths_short_prefixes_and_live_notes() {
    let (_tmp, core) = store();
    let live = create(&core, "# Viva");
    let deleted = create(&core, "# Apagada");
    core.storage().move_note_to_trash(&deleted).expect("trash");

    // A live note is not in the trash, whatever its identifier says.
    assert!(core.resolve_trash_id(&live.to_string()).is_err());
    assert!(core.resolve_trash_id("../../etc/passwd").is_err());
    assert!(core.resolve_trash_id("notes/foo.md").is_err());
    assert!(core.resolve_trash_id("abc").is_err());
    assert_eq!(
        core.resolve_trash_id(&deleted.as_simple().to_string()[..8])
            .expect("prefix"),
        deleted
    );
}

// 25 – 27 -------------------------------------------------------------------
#[test]
fn every_task_gets_a_reference_and_identical_tasks_get_different_ones() {
    let (_tmp, core) = store();
    let id = create(
        &core,
        "- [ ] Revisar noradrenalina\n- [ ] Revisar noradrenalina\n- [ ] Outra",
    );
    let tasks = task::parse_tasks(id, "nota", &body(&core, id));

    assert_eq!(tasks.len(), 3);
    for entry in &tasks {
        assert_eq!(entry.task_ref.as_str().len(), 8);
        assert!(entry
            .task_ref
            .as_str()
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }
    assert_ne!(
        tasks[0].task_ref, tasks[1].task_ref,
        "two identical tasks share one reference"
    );
    assert_ne!(tasks[0].task_ref, tasks[2].task_ref);
}

#[test]
fn each_of_two_identical_tasks_is_completed_on_its_own() {
    let (_tmp, core) = store();
    let id = create(&core, "- [ ] Revisar\n- [ ] Revisar");
    let second = task::parse_tasks(id, "nota", &body(&core, id))[1]
        .task_ref
        .as_str()
        .to_string();

    mutate(&core, id, NoteMutation::CompleteTask { task_ref: second }).expect("complete");
    let after = body(&core, id);
    let lines: Vec<&str> = after.lines().collect();
    assert!(lines[0].starts_with("- [ ] Revisar"), "{after}");
    assert!(lines[1].starts_with("- [x] Revisar"), "{after}");
}

#[test]
fn a_reference_from_before_the_note_changed_is_refused() {
    let (_tmp, core) = store();
    let id = create(&core, "- [ ] Revisar noradrenalina");
    let stale = first_task_ref(&core, id);

    // Somebody edits the task itself between the listing and the write.
    mutate(
        &core,
        id,
        NoteMutation::ReplaceBody {
            body: "- [ ] Revisar volume".into(),
        },
    )
    .expect("edit");

    let error = mutate(&core, id, NoteMutation::CompleteTask { task_ref: stale })
        .expect_err("a stale reference must be refused");
    assert!(
        matches!(error, WriteError::StaleTaskRef { .. }),
        "{error:?}"
    );
    assert_eq!(
        body(&core, id),
        "- [ ] Revisar volume",
        "a stale reference completed the wrong task"
    );
}

#[test]
fn a_reference_survives_an_unrelated_line_being_inserted_above_it() {
    // The reference deliberately does not carry a line number: a paragraph
    // typed somewhere else in the note would otherwise invalidate every
    // reference below it, for tasks that did not change at all.
    let (_tmp, core) = store();
    let id = create(&core, "intro\n\n- [ ] Revisar noradrenalina");
    let reference = first_task_ref(&core, id);

    mutate(
        &core,
        id,
        NoteMutation::ReplaceBody {
            body: "intro\n\numa linha nova\n\n- [ ] Revisar noradrenalina".into(),
        },
    )
    .expect("insert a line above");

    mutate(
        &core,
        id,
        NoteMutation::CompleteTask {
            task_ref: reference,
        },
    )
    .expect("the reference must still name the same task");
    assert!(body(&core, id).contains("- [x] Revisar noradrenalina"));
}

#[test]
fn a_reference_that_is_not_one_is_refused_as_invalid_usage() {
    let (_tmp, core) = store();
    let id = create(&core, "- [ ] Tarefa");
    for bad in ["", "abc", "zzzzzzzz", "a71bc9200"] {
        let error = mutate(
            &core,
            id,
            NoteMutation::CompleteTask {
                task_ref: bad.to_string(),
            },
        )
        .expect_err("not a reference");
        assert!(
            matches!(error, WriteError::InvalidInput { .. }),
            "{bad}: {error:?}"
        );
    }
}

// 28 – 31 -------------------------------------------------------------------
#[test]
fn completing_a_task_ticks_it_and_records_a_real_instant() {
    let (_tmp, core) = store();
    let id = create(&core, "- [ ] Revisar noradrenalina\n- [ ] Outra");
    let reference = first_task_ref(&core, id);

    assert!(mutate(
        &core,
        id,
        NoteMutation::CompleteTask {
            task_ref: reference
        }
    )
    .expect("complete"));

    let after = body(&core, id);
    assert!(after.starts_with("- [x] Revisar noradrenalina <!-- note-it:completed_at="));
    assert!(after.contains("- [ ] Outra"), "the other task moved");

    // The instant has to be one the reader — and the page's own parser —
    // accepts: an explicit zone, never a bare local time.
    let tasks = task::parse_tasks(id, "nota", &after);
    assert!(tasks[0].checked);
    assert!(
        tasks[0].completed_at.is_some(),
        "the completion instant did not round-trip: {after}"
    );
    assert_eq!(tasks[0].text, "Revisar noradrenalina");

    // Completing it again is a no-op success and rewrites nothing.
    let refreshed = tasks[0].task_ref.as_str().to_string();
    let before = bytes_of(&note_path(&core, id));
    assert!(!mutate(
        &core,
        id,
        NoteMutation::CompleteTask {
            task_ref: refreshed
        }
    )
    .expect("complete again"));
    assert_eq!(bytes_of(&note_path(&core, id)), before);
}

#[test]
fn reopening_a_task_removes_only_note_its_own_comment() {
    let (_tmp, core) = store();
    let id = create(
        &core,
        "  - [x] Revisar <!-- observação externa --> <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->",
    );
    let reference = first_task_ref(&core, id);

    assert!(mutate(
        &core,
        id,
        NoteMutation::ReopenTask {
            task_ref: reference
        }
    )
    .expect("reopen"));

    let after = body(&core, id);
    assert!(
        after.starts_with("  - [ ] Revisar"),
        "the indentation or the checkbox changed: {after}"
    );
    assert!(
        after.contains("<!-- observação externa -->"),
        "someone else's comment was removed: {after}"
    );
    assert!(!after.contains("note-it:completed_at"));

    // Reopening an already open task is a no-op success.
    let refreshed = first_task_ref(&core, id);
    let before = bytes_of(&note_path(&core, id));
    assert!(!mutate(
        &core,
        id,
        NoteMutation::ReopenTask {
            task_ref: refreshed
        }
    )
    .expect("reopen again"));
    assert_eq!(bytes_of(&note_path(&core, id)), before);
}

#[test]
fn reopening_preserves_the_bullet_and_the_nesting_the_note_actually_uses() {
    let (_tmp, core) = store();
    let id = create(&core, "    * [X] Tarefa aninhada com asterisco");
    let reference = first_task_ref(&core, id);
    mutate(
        &core,
        id,
        NoteMutation::ReopenTask {
            task_ref: reference,
        },
    )
    .expect("reopen");
    assert_eq!(body(&core, id), "    * [ ] Tarefa aninhada com asterisco");
}

// 32 ------------------------------------------------------------------------
#[test]
fn a_task_written_inside_a_code_fence_can_never_be_mutated() {
    let (_tmp, core) = store();
    let markdown = "\
- [ ] Tarefa real

```markdown
- [ ] Tarefa de exemplo
```
";
    let id = create(&core, markdown);
    let tasks = task::parse_tasks(id, "nota", &body(&core, id));

    // The fenced line is not a task at all, so there is no reference that can
    // name it. One scanner decides this, and both reading and writing use it.
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].text, "Tarefa real");

    mutate(
        &core,
        id,
        NoteMutation::CompleteTask {
            task_ref: tasks[0].task_ref.as_str().to_string(),
        },
    )
    .expect("complete the real one");

    let after = body(&core, id);
    assert!(
        after.contains("- [ ] Tarefa de exemplo"),
        "the example inside the fence was ticked: {after}"
    );
    assert!(after.contains("- [x] Tarefa real"));

    // And the listing agrees with the writer, because it is the same scanner.
    let listed = core
        .list_tasks(
            TaskStateFilter::All,
            &noteit_core::NoteFilter::default(),
            None,
        )
        .expect("list");
    assert_eq!(listed.items.len(), 1);
}

// 33 ------------------------------------------------------------------------
#[test]
fn a_mutation_that_changes_nothing_does_not_touch_the_file_at_all() {
    let (_tmp, core) = store();
    let id = create(&core, "corpo");
    mutate(
        &core,
        id,
        NoteMutation::AddTag {
            tag: "Medicina".into(),
        },
    )
    .expect("add");

    let path = note_path(&core, id);
    let before = bytes_of(&path);
    let modified_before = fs::metadata(&path)
        .expect("metadata")
        .modified()
        .expect("mtime");
    std::thread::sleep(std::time::Duration::from_millis(20));

    for mutation in [
        NoteMutation::AddTag {
            tag: "MEDICINA".into(),
        },
        NoteMutation::RemoveTag {
            tag: "inexistente".into(),
        },
        NoteMutation::RemoveProperty {
            key: "inexistente".into(),
        },
        NoteMutation::ReplaceBody {
            body: "corpo".into(),
        },
    ] {
        assert!(!mutate(&core, id, mutation).expect("no-op"));
    }

    assert_eq!(bytes_of(&path), before);
    assert_eq!(
        fs::metadata(&path)
            .expect("metadata")
            .modified()
            .expect("mtime"),
        modified_before,
        "a no-op rewrote the file"
    );
}

// Selectors ------------------------------------------------------------------
#[test]
fn a_mutation_selector_is_never_a_path() {
    let (_tmp, core) = store();
    create(&core, "corpo");
    for selector in ["../../etc/passwd", "notes/foo.md", "/etc/passwd", "abc"] {
        let error = write::execute(
            &core,
            &WriteOperation::MutateNote {
                selector: selector.to_string(),
                mutation: NoteMutation::Append {
                    payload: "x".into(),
                },

                expected_revision: None,
            },
        )
        .expect_err("a path must never resolve");
        assert!(
            matches!(error, WriteError::InvalidInput { .. }),
            "{selector}: {error:?}"
        );
    }
}

#[test]
fn an_ambiguous_prefix_is_reported_rather_than_resolved() {
    let (_tmp, core) = store();
    // Two notes deliberately sharing the first eight hexadecimal characters.
    for suffix in ["1111", "2222"] {
        let id =
            Uuid::parse_str(&format!("aabbccdd-0000-4000-8000-00000000{suffix}")).expect("uuid");
        let mut document = NoteDocument::new_empty();
        document.metadata.id = id;
        document.content = "corpo".into();
        core.storage().save_note_atomic(&document).expect("place");
    }

    let error = write::execute(
        &core,
        &WriteOperation::MutateNote {
            selector: "aabbccdd".to_string(),
            mutation: NoteMutation::Append {
                payload: "x".into(),
            },

            expected_revision: None,
        },
    )
    .expect_err("an ambiguous prefix");
    assert!(
        matches!(error, WriteError::AmbiguousSelector { matches: 2, .. }),
        "{error:?}"
    );
}

// Input is not terminal output ------------------------------------------------
#[test]
fn markdown_carrying_terminal_escapes_is_stored_exactly_as_written() {
    // Sanitisation is about *showing* text and is applied on the way out. A
    // note that legitimately contains an escape sequence — a shell transcript,
    // say — must survive being written and read back byte for byte.
    let (_tmp, core) = store();
    let raw = "Saída do terminal:\n\n```\n\u{1b}[31mvermelho\u{1b}[0m\n```";
    let id = create(&core, raw);
    assert_eq!(body(&core, id), raw);

    mutate(
        &core,
        id,
        NoteMutation::Append {
            payload: "\u{1b}]0;título\u{7}".into(),
        },
    )
    .expect("append");
    assert!(body(&core, id).ends_with("\u{1b}]0;título\u{7}"));
}

// R-002 and R-004 Identity and Integrity Verification -------------------------

#[test]
fn r002_case_1_deterministic_parse_anchored_to_filename_uuid() {
    let id_a = Uuid::new_v4();
    let raw = "Corpo de texto sem nenhum front matter";

    // Parse multiple times with the expected ID
    let doc1 = NoteDocument::parse_with_id(raw, id_a).expect("parse 1");
    let doc2 = NoteDocument::parse_with_id(raw, id_a).expect("parse 2");
    let doc3 = NoteDocument::parse_with_id(raw, id_a).expect("parse 3");

    assert_eq!(doc1.metadata.id, id_a);
    assert_eq!(doc2.metadata.id, id_a);
    assert_eq!(doc3.metadata.id, id_a);
    assert_eq!(doc1.content, raw);
}

#[test]
fn r002_case_2_multiple_appends_on_note_without_frontmatter_mutates_only_addressed_file() {
    let (_tmp, core) = store();
    let id_a = Uuid::new_v4();
    let file_path = core.storage().note_path(&id_a);
    // Write a note without front matter directly to disk
    fs::write(&file_path, "Texto inicial sem frontmatter").expect("write raw note");

    // Perform multiple appends via write API
    for i in 1..=3 {
        let outcome = write::execute(
            &core,
            &WriteOperation::MutateNote {
                selector: id_a.to_string(),
                mutation: NoteMutation::Append {
                    payload: format!("Linha {i}"),
                },

                expected_revision: None,
            },
        )
        .expect("append");

        // Verify machine outcome reports addressed ID
        assert_eq!(outcome.note_id, id_a);
        assert!(outcome.changed);
    }

    // Verify disk state: exactly 1 file exists, which is id_a.md
    let notes_dir = core.storage().paths().notes_dir.clone();
    let entries: Vec<_> = fs::read_dir(&notes_dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();

    assert_eq!(
        entries.len(),
        1,
        "Must be exactly 1 file on disk, found {entries:?}"
    );
    assert_eq!(entries[0], file_path);

    // Verify content
    let doc = core.read_note(&id_a).expect("read note");
    assert_eq!(doc.metadata.id, id_a);
    assert!(doc.content.contains("Texto inicial sem frontmatter"));
    assert!(doc.content.contains("Linha 1"));
    assert!(doc.content.contains("Linha 2"));
    assert!(doc.content.contains("Linha 3"));
}

#[test]
fn r002_case_3_divergence_between_filename_and_frontmatter_fails_explicitly() {
    let (_tmp, core) = store();
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    assert_ne!(id_a, id_b);

    let mut doc_b = NoteDocument::new_with_id(id_b);
    doc_b.content = "Corpo com ID divergente".into();
    let hostile_content = doc_b.serialize().expect("serialize doc_b");
    let path_a = core.storage().note_path(&id_a);
    let path_b = core.storage().note_path(&id_b);
    fs::write(&path_a, &hostile_content).expect("write hostile note");

    // Reading note A must fail with explicit identity conflict error
    let read_err = core
        .read_note(&id_a)
        .expect_err("reading note with divergent ID must fail");
    assert!(
        read_err.contains("conflito de identidade"),
        "error must explicitly report identity conflict: {read_err}"
    );

    // Attempting a mutation on selector id_a must fail before touching anything
    let mutate_err = write::execute(
        &core,
        &WriteOperation::MutateNote {
            selector: id_a.to_string(),
            mutation: NoteMutation::Append {
                payload: "tentativa de ataque".into(),
            },

            expected_revision: None,
        },
    )
    .expect_err("mutation on note with divergent ID must fail");

    assert!(
        matches!(mutate_err, WriteError::StoreUnavailable { .. }),
        "mutation must fail closed: {mutate_err:?}"
    );

    // Verify store state: file B was NEVER created, file A was NOT modified
    assert!(!path_b.exists(), "Target file B must NOT be created");
    let content_after = fs::read_to_string(&path_a).expect("read path a");
    assert_eq!(
        content_after, hostile_content,
        "Path A must remain completely unmodified"
    );
}

#[test]
fn r002_case_4_defense_in_depth_storage_layer_rejects_identity_mismatch() {
    let (_tmp, core) = store();
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    assert_ne!(id_a, id_b);

    let doc_b = NoteDocument::new_with_id(id_b);
    let path_a = core.storage().note_path(&id_a);
    let path_b = core.storage().note_path(&id_b);

    // Call save_note_atomic_with_id with mismatched expected_id
    let err = core
        .storage()
        .save_note_atomic_with_id(&id_a, &doc_b)
        .expect_err("save_note_atomic_with_id must reject mismatched ID");

    assert!(
        err.contains("conflito de identidade na persistência"),
        "error must report identity conflict: {err}"
    );

    // Verify neither file exists on disk
    assert!(!path_a.exists(), "Path A must not exist");
    assert!(!path_b.exists(), "Path B must not exist");
}

#[test]
fn r002_case_5_defense_in_depth_write_layer_rejects_identity_mismatch() {
    let (_tmp, core) = store();
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    assert_ne!(id_a, id_b);

    let doc_b = NoteDocument::new_with_id(id_b);
    let path_a = core.storage().note_path(&id_a);
    let path_b = core.storage().note_path(&id_b);

    let err = write::commit_addressed(&core, &id_a, &doc_b)
        .expect_err("commit_addressed must reject mismatched ID");

    assert!(
        matches!(err, WriteError::Persistence { .. }),
        "error must be WriteError::Persistence: {err:?}"
    );

    assert!(!path_a.exists(), "Path A must not exist");
    assert!(!path_b.exists(), "Path B must not exist");
}

#[test]
fn r002_case_7_sequential_appends_on_note_without_frontmatter_preserves_single_file_and_uuid() {
    let (_tmp, core) = store();
    let id = Uuid::new_v4();
    let file_path = core.storage().note_path(&id);
    fs::write(&file_path, "Início").expect("initial write");

    for i in 1..=5 {
        let outcome = write::execute(
            &core,
            &WriteOperation::MutateNote {
                selector: id.to_string(),
                mutation: NoteMutation::Append {
                    payload: format!("Parágrafo {i}"),
                },

                expected_revision: None,
            },
        )
        .expect("append");

        assert_eq!(outcome.note_id, id);
        assert!(outcome.changed);
    }

    let notes_dir = core.storage().paths().notes_dir.clone();
    let count = fs::read_dir(&notes_dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .count();
    assert_eq!(count, 1, "Must be exactly 1 file on disk after 5 appends");

    let final_doc = core.read_note(&id).expect("read final doc");
    assert_eq!(final_doc.metadata.id, id);
    for i in 1..=5 {
        assert!(final_doc.content.contains(&format!("Parágrafo {i}")));
    }
}

#[test]
fn r002_case_3_b_exists_cross_corruption_prevented() {
    let (_tmp, core) = store();
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    assert_ne!(id_a, id_b);

    // 1. Create legitimate B.md with valid front matter declaring id = id_b
    let mut doc_b = NoteDocument::new_with_id(id_b);
    doc_b.content = "CONTEUDO_ORIGINAL_DE_B_LEGITIMO".into();
    let content_b = doc_b.serialize().expect("serialize doc_b");
    let path_b = core.storage().note_path(&id_b);
    fs::write(&path_b, &content_b).expect("write legitimate B");

    let bytes_b_before = fs::read(&path_b).expect("read b before");

    // 2. Create hostile A.md: filename is A.md, but front matter claims id = id_b
    let mut hostile_doc = NoteDocument::new_with_id(id_b);
    hostile_doc.content = "CONTEUDO_HOSTIL_DE_A_TENTANDO_CORROMPER_B".into();
    let content_a = hostile_doc.serialize().expect("serialize hostile_doc");
    let path_a = core.storage().note_path(&id_a);
    fs::write(&path_a, &content_a).expect("write hostile A");

    let bytes_a_before = fs::read(&path_a).expect("read a before");

    // Pre-condition: exactly 2 files exist
    let files_before = fs::read_dir(core.storage().notes_dir())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .count();
    assert_eq!(
        files_before, 2,
        "Must be exactly 2 note files before attack"
    );

    // 3. Attack: attempt mutation directed at note A
    let mutate_err = write::execute(
        &core,
        &WriteOperation::MutateNote {
            selector: id_a.to_string(),
            mutation: NoteMutation::Append {
                payload: "ATAQUE_APPEND_EM_A".into(),
            },

            expected_revision: None,
        },
    )
    .expect_err("mutation on hostile note A must fail fail-closed");

    assert!(
        matches!(mutate_err, WriteError::StoreUnavailable { .. }),
        "mutation must fail with StoreUnavailable: {mutate_err:?}"
    );

    // 4. Invariant: B.md MUST remain byte-for-byte identical to snapshot
    let bytes_b_after = fs::read(&path_b).expect("read b after");
    assert_eq!(
        bytes_b_after, bytes_b_before,
        "CORRUPÇÃO CRUZADA DETECTADA: B.md foi alterado após ataque em A!"
    );

    // Invariant: A.md MUST remain byte-for-byte identical to snapshot
    let bytes_a_after = fs::read(&path_a).expect("read a after");
    assert_eq!(
        bytes_a_after, bytes_a_before,
        "A.md foi alterado apesar do erro de validação!"
    );

    // Invariant: Exactly 2 files on disk (zero third files, zero ghost files, zero temps)
    let files_after: Vec<_> = fs::read_dir(core.storage().notes_dir())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .map(|e| e.path())
        .collect();
    assert_eq!(
        files_after.len(),
        2,
        "Must remain exactly 2 files: {files_after:?}"
    );

    // Reading legitimate B succeeds with authentic content
    let read_b = core.read_note(&id_b).expect("read B must succeed");
    assert_eq!(read_b.metadata.id, id_b);
    assert_eq!(read_b.content, "CONTEUDO_ORIGINAL_DE_B_LEGITIMO");

    // Reading hostile A fails with explicit identity conflict
    let read_a_err = core.read_note(&id_a).expect_err("reading A must fail");
    assert!(
        read_a_err.contains("conflito de identidade"),
        "error must mention identity conflict: {read_a_err}"
    );
}

#[test]
fn r002_gap2_list_read_mutate_preserves_identity_standard_note() {
    let (_tmp, core) = store();
    let initial_content = "Nota criada para fluxo list-read-mutate";
    let created_id = create(&core, initial_content);

    // 1. LIST: list notes using canonical API
    let listed_ids = core.list_notes().expect("list notes");
    assert!(
        listed_ids.contains(&created_id),
        "List must include created note"
    );
    let listed_id = *listed_ids.iter().find(|&&id| id == created_id).unwrap();

    // 2. READ: use exactly the ID returned by list
    let read_doc = core
        .read_note(&listed_id)
        .expect("read note using listed id");
    assert_eq!(
        read_doc.metadata.id, created_id,
        "Read ID must match created ID"
    );
    assert_eq!(read_doc.content, initial_content);

    // 3. MUTATE: use exactly the ID for mutation
    let append_payload = "Texto adicionado no passo 3";
    let outcome = write::execute(
        &core,
        &WriteOperation::MutateNote {
            selector: listed_id.to_string(),
            mutation: NoteMutation::Append {
                payload: append_payload.into(),
            },

            expected_revision: None,
        },
    )
    .expect("mutation must succeed");

    assert_eq!(
        outcome.note_id, created_id,
        "Outcome must report the same UUID"
    );
    assert!(outcome.changed);

    // 4. READ AGAIN: verify updated note
    let final_doc = core
        .read_note(&listed_id)
        .expect("read note after mutation");
    assert_eq!(final_doc.metadata.id, created_id);
    assert!(final_doc.content.contains(initial_content));
    assert!(final_doc.content.contains(append_payload));

    // Invariant: exactly 1 note file exists on disk
    let files: Vec<_> = fs::read_dir(core.storage().notes_dir())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .map(|e| e.path())
        .collect();
    assert_eq!(files.len(), 1, "Exactly 1 note file on disk: {files:?}");
    assert_eq!(files[0], core.storage().note_path(&created_id));
}

#[test]
fn r002_gap2_list_read_mutate_preserves_identity_note_without_frontmatter() {
    let (_tmp, core) = store();
    let uuid = Uuid::new_v4();
    let file_path = core.storage().note_path(&uuid);

    // Prepare note without YAML front matter directly on disk
    let raw_content = "Texto puro sem qualquer bloco de frontmatter YAML";
    fs::write(&file_path, raw_content).expect("write plain note");

    // 1. LIST: list notes using canonical API
    let listed_ids = core.list_notes().expect("list notes");
    assert_eq!(listed_ids.len(), 1);
    let listed_id = listed_ids[0];
    assert_eq!(
        listed_id, uuid,
        "Listed note without front matter must be identified by filename UUID"
    );

    // 2. READ: read note using listed id
    let read_doc = core.read_note(&listed_id).expect("read plain note");
    assert_eq!(
        read_doc.metadata.id, uuid,
        "Read document must deterministically anchor to filename UUID"
    );
    assert_eq!(read_doc.content, raw_content);

    // 3. MUTATE: mutate note using listed id
    let append_payload = "Adição em nota sem frontmatter";
    let outcome = write::execute(
        &core,
        &WriteOperation::MutateNote {
            selector: listed_id.to_string(),
            mutation: NoteMutation::Append {
                payload: append_payload.into(),
            },

            expected_revision: None,
        },
    )
    .expect("mutation must succeed");

    assert_eq!(outcome.note_id, uuid, "Outcome must report filename UUID");
    assert!(outcome.changed);

    // 4. READ AGAIN: read note after mutation
    let final_doc = core
        .read_note(&listed_id)
        .expect("read note after mutation");
    assert_eq!(final_doc.metadata.id, uuid);
    assert!(final_doc.content.contains(raw_content));
    assert!(final_doc.content.contains(append_payload));

    // Invariant: exactly 1 file remains on disk, unchanged UUID
    let files: Vec<_> = fs::read_dir(core.storage().notes_dir())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .map(|e| e.path())
        .collect();
    assert_eq!(files.len(), 1, "Exactly 1 note file on disk: {files:?}");
    assert_eq!(files[0], file_path);
}
