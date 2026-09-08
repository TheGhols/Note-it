use crossterm::event::{KeyCode, KeyEvent};
use noteit_core::{
    authority::perform_at,
    coordination::WriteCoordinationPaths,
    write::{self, NoteDraft, NoteMutation, WriteOperation},
    StorePaths,
};
use noteit_tui::{
    app::{App, Focus},
    editor::{editor_program, mutation_for, recovery_directory, EditorSession},
};
use std::{
    fs,
    process::Command,
    sync::{atomic::AtomicBool, Arc},
};

struct Fixture {
    root: tempfile::TempDir,
    app: App,
}

impl Fixture {
    fn new(body: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let paths = StorePaths::from_custom_paths(
            root.path().join("notes"),
            root.path().join("config"),
            root.path().join("state"),
            root.path().join("runtime"),
        );
        perform_at(
            &paths,
            &WriteOperation::CreateNote {
                draft: NoteDraft {
                    content: body.into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
        app.focus = Focus::Reader;
        Self { root, app }
    }
    fn key(&mut self, key: char) {
        self.app.handle_key(KeyEvent::from(KeyCode::Char(key)));
    }
    fn external(&self, body: &str) {
        let note = self.app.current_note.as_ref().unwrap();
        perform_at(
            &self.app.paths,
            &WriteOperation::MutateNote {
                selector: note.id.to_string(),
                expected_revision: Some(note.revision.clone()),
                mutation: NoteMutation::ReplaceBody { body: body.into() },
            },
        )
        .unwrap();
    }
    fn session(&self, edited: &[u8]) -> EditorSession {
        let session =
            EditorSession::prepare(self.app.current_note.clone().unwrap(), self.root.path())
                .unwrap();
        fs::write(&session.temporary, edited).unwrap();
        session
    }
    fn finish(&self, session: &EditorSession) -> noteit_tui::editor::EditorResult {
        session.finish(
            &self.app.paths,
            Command::new("true").status(),
            Ok(self.root.path().join("recovery")),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let paths = WriteCoordinationPaths::for_store(&self.app.paths).unwrap();
        if fs::read_to_string(paths.store_marker_path())
            .ok()
            .as_deref()
            .map(str::trim_end)
            == paths.notes_dir().to_str()
        {
            let _ = fs::remove_dir_all(paths.store_dir());
        }
    }
}

#[test]
fn toggle_success_refreshes_revision_and_reopens_using_core_task_reference() {
    let mut f = Fixture::new("- [ ] tarefa");
    let before = f.app.current_note.clone().unwrap();
    f.key(' ');
    let changed = f.app.current_note.as_ref().unwrap();
    assert!(changed.content.starts_with("- [x]"));
    assert_ne!(changed.revision, before.revision);
    f.key(' ');
    assert_eq!(f.app.current_note.as_ref().unwrap().content, "- [ ] tarefa");
}

#[test]
fn toggle_conflict_reloads_without_retry_or_optimistic_local_change() {
    let mut f = Fixture::new("- [ ] tarefa");
    f.external("- [ ] tarefa\nR2 desktop");
    let id = f.app.current_note_id.unwrap();
    let path = f.app.paths.notes_dir.join(format!("{id}.md"));
    let r2 = fs::read(&path).unwrap();
    f.key(' ');
    assert!(f.app.notice.contains("Conflito"));
    assert_eq!(fs::read(path).unwrap(), r2);
    assert_eq!(
        f.app.current_note.as_ref().unwrap().content,
        "- [ ] tarefa\nR2 desktop"
    );
    assert_eq!(
        f.app.current_note.as_ref().unwrap().revision,
        write::revision_of(&f.app.core.read_note(&id).unwrap()).unwrap()
    );
}

#[test]
fn task_cursor_uses_source_lines_and_ignores_fenced_task_text() {
    let mut f = Fixture::new("```\n- [ ] not a task\n```\n- [ ] real");
    f.key('j');
    f.key(' ');
    assert!(f.app.notice.contains("Posicione"));
    f.key('j');
    f.key('j');
    f.key(' ');
    assert!(f
        .app
        .current_note
        .as_ref()
        .unwrap()
        .content
        .contains("- [x] real"));
    assert!(f
        .app
        .current_note
        .as_ref()
        .unwrap()
        .content
        .contains("- [ ] not a task"));
}

#[test]
fn create_empty_note_opens_reader_with_canonical_revision() {
    let mut f = Fixture::new("original");
    let old = f.app.current_note_id;
    f.app.focus = Focus::List;
    f.key('n');
    assert_eq!(f.app.focus, Focus::Reader);
    assert_ne!(f.app.current_note_id, old);
    let note = f.app.current_note.as_ref().unwrap();
    assert_eq!(note.content, "");
    assert_eq!(
        note.revision,
        write::revision_of(&f.app.core.read_note(&note.id).unwrap()).unwrap()
    );
}

#[test]
fn discard_confirmation_y_moves_and_n_cancels() {
    let mut f = Fixture::new("keep");
    let id = f.app.current_note_id.unwrap();
    f.key('d');
    assert!(f.app.discard_confirmation.is_some());
    assert!(f.app.core.read_note(&id).is_ok());
    f.key('n');
    assert!(f.app.core.read_note(&id).is_ok());
    f.key('d');
    f.key('y');
    assert!(f.app.core.read_note(&id).is_err());
    assert_eq!(f.app.core.read_trash_note(&id).unwrap().content, "keep");
}

#[test]
fn discard_confirmation_freezes_revision_and_conflict_preserves_both_locations() {
    let mut f = Fixture::new("R1");
    let id = f.app.current_note_id.unwrap();
    f.key('d');
    f.external("R2");
    let path = f.app.paths.notes_dir.join(format!("{id}.md"));
    let bytes = fs::read(&path).unwrap();
    f.key('y');
    assert!(f.app.notice.contains("Conflito"));
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert!(f.app.core.list_trash().is_empty());
    assert_eq!(f.app.current_note.as_ref().unwrap().content, "R2");
}

#[test]
fn restore_uses_previously_previewed_trash_revision() {
    let mut f = Fixture::new("restore");
    let id = f.app.current_note_id.unwrap();
    f.key('d');
    f.key('y');
    f.key('3');
    assert!(f.app.current_note.as_ref().unwrap().in_trash);
    f.key('r');
    assert_eq!(f.app.core.read_note(&id).unwrap().content, "restore");
    assert!(f.app.core.list_trash().is_empty());
}

#[test]
fn restore_conflict_preserves_bytes_and_reloads_trash_without_retry() {
    let mut f = Fixture::new("R1");
    let id = f.app.current_note_id.unwrap();
    f.key('d');
    f.key('y');
    f.key('3');
    let path = f.app.paths.trash_dir.join(format!("{id}.md"));
    let mut doc = f.app.core.read_trash_note(&id).unwrap();
    doc.content = "R2 trash".into();
    // Simulate an outside writer changing the trash after the preview.
    fs::write(&path, doc.serialize().unwrap()).unwrap();
    let bytes = fs::read(&path).unwrap();
    f.key('r');
    assert!(f.app.notice.contains("Conflito"));
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert!(f.app.core.read_note(&id).is_err());
    assert_eq!(f.app.current_note.as_ref().unwrap().content, "R2 trash");
}

#[test]
fn editor_literal_and_canonical_noops_do_not_write_advance_revision_or_recover() {
    for (original, edited) in [
        ("abc", "abc"),
        ("abc", "abc\n\n"),
        ("", "\n\n"),
        ("abc", "abc\r\n"),
    ] {
        let f = Fixture::new(original);
        let note = f.app.current_note.as_ref().unwrap();
        let path = f.app.paths.notes_dir.join(format!("{}.md", note.id));
        let bytes = fs::read(&path).unwrap();
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();
        let session = f.session(edited.as_bytes());
        assert_eq!(mutation_for(original, edited), None);
        let result = f.finish(&session);
        assert!(!result.reload);
        assert!(result.recovery.is_none());
        assert!(!session.temporary.exists());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), mtime);
        assert_eq!(
            write::revision_of(&f.app.core.read_note(&note.id).unwrap()).unwrap(),
            note.revision
        );
        assert!(!f.root.path().join("recovery").exists());
    }
}

#[test]
fn editor_real_content_and_trailing_spaces_replace_body_via_authority() {
    for edited in ["abc\ndef", "abc  ", "edited\n\n"] {
        let f = Fixture::new("abc");
        let original = f.app.current_note.as_ref().unwrap();
        assert_eq!(
            mutation_for("abc", edited),
            Some(NoteMutation::ReplaceBody {
                body: edited.into()
            })
        );
        let session = f.session(edited.as_bytes());
        assert!(f.finish(&session).reload);
        assert!(!session.temporary.exists());
        let updated = f.app.core.read_note(&original.id).unwrap();
        assert_eq!(
            updated.content,
            noteit_core::model::NoteDocument::canonical_content(edited)
        );
        assert_ne!(write::revision_of(&updated).unwrap(), original.revision);
    }
}

#[test]
fn editor_clear_body_selects_explicit_operation_and_advances_revision() {
    for edited in ["", "\n", "\n\n"] {
        let f = Fixture::new("abc");
        let original = f.app.current_note.as_ref().unwrap();
        assert_eq!(mutation_for("abc", edited), Some(NoteMutation::ClearBody));
        let session = f.session(edited.as_bytes());
        assert!(f.finish(&session).reload);
        let updated = f.app.core.read_note(&original.id).unwrap();
        assert_eq!(updated.content, "");
        assert_ne!(write::revision_of(&updated).unwrap(), original.revision);
    }
}

#[test]
fn editor_conflict_including_clear_preserves_r2_without_retry_and_recovers_literal_bytes() {
    for edited in ["edited\n\n", "", "\n\n"] {
        let f = Fixture::new("abc");
        let session = f.session(edited.as_bytes());
        f.external("R2 survives");
        let path = f
            .app
            .paths
            .notes_dir
            .join(format!("{}.md", session.original.id));
        let r2 = fs::read(&path).unwrap();
        let result = f.finish(&session);
        assert!(result.conflict);
        assert!(result.reload);
        let recovery = result.recovery.unwrap();
        assert_eq!(fs::read(&recovery).unwrap(), edited.as_bytes());
        assert_eq!(fs::metadata(recovery).unwrap().len(), edited.len() as u64);
        assert_eq!(fs::read(path).unwrap(), r2);
        assert!(!session.temporary.exists());
    }
}

#[test]
fn recovery_failure_keeps_the_last_copy_and_reports_its_path() {
    let f = Fixture::new("abc");
    let session = f.session(b"valuable\n\n");
    f.external("R2");
    let blocked = f.root.path().join("not-a-directory");
    fs::write(&blocked, "block").unwrap();
    let result = session.finish(&f.app.paths, Command::new("true").status(), Ok(blocked));
    assert!(result.conflict);
    assert!(result.message.contains("falha no recovery"));
    assert!(result.message.contains(session.temporary.to_str().unwrap()));
    assert_eq!(fs::read(session.temporary).unwrap(), b"valuable\n\n");
}

#[test]
fn editor_nonzero_exit_and_spawn_failure_keep_significant_edits_without_writing() {
    let f = Fixture::new("abc");
    for status in [
        Command::new("false").status(),
        Command::new("/no/such/noteit-editor").status(),
    ] {
        let session = f.session(b"unsaved");
        let result = session.finish(&f.app.paths, status, Ok(f.root.path().join("recovery")));
        assert!(result.message.contains(session.temporary.to_str().unwrap()));
        assert_eq!(fs::read(&session.temporary).unwrap(), b"unsaved");
        assert_eq!(
            f.app.core.read_note(&session.original.id).unwrap().content,
            "abc"
        );
    }
}

#[test]
fn editor_invalid_utf8_keeps_exact_bytes_without_lossy_conversion() {
    let f = Fixture::new("abc");
    let session = f.session(&[0xff, 0x0a]);
    let result = f.finish(&session);
    assert!(!result.reload);
    assert_eq!(fs::read(session.temporary).unwrap(), [0xff, 0x0a]);
}

#[test]
fn editor_fallback_and_recovery_paths_are_explicit() {
    assert_eq!(editor_program(None), "vi");
    assert_eq!(editor_program(Some("".into())), "vi");
    assert_eq!(
        editor_program(Some("/tmp/editor with spaces".into())),
        "/tmp/editor with spaces"
    );
    assert_eq!(
        recovery_directory(Some("/state".into()), None).unwrap(),
        std::path::Path::new("/state/note-it/tui-recovery")
    );
    assert_eq!(
        recovery_directory(None, Some("/home/test".into())).unwrap(),
        std::path::Path::new("/home/test/.local/state/note-it/tui-recovery")
    );
    assert!(recovery_directory(None, None).is_err());
}

#[test]
fn unavailable_writer_is_not_bypassed_and_edited_copy_is_retained() {
    use noteit_core::coordination::WriterLease;
    let f = Fixture::new("original");
    let coordination = WriteCoordinationPaths::for_store(&f.app.paths).unwrap();
    let lease = WriterLease::try_acquire(&coordination).unwrap().unwrap();
    let session = f.session(b"valuable\n\n");
    let result = f.finish(&session);
    assert!(!result.reload);
    assert!(result.message.contains("não confirmada"));
    assert_eq!(fs::read(&session.temporary).unwrap(), b"valuable\n\n");
    assert_eq!(
        f.app.core.read_note(&session.original.id).unwrap().content,
        "original"
    );
    drop(lease);
}

#[test]
fn confirmation_and_notice_fit_minimum_viewport_without_panicking() {
    use ratatui::{backend::TestBackend, Terminal};
    let mut f = Fixture::new("original");
    f.key('d');
    for (width, height) in [(35, 6), (60, 6), (80, 10)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        f.app.draw(&mut terminal).unwrap();
    }
}
