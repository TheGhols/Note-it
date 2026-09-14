//! Gate 5.0D.4B.R — adversarial and regression closure.
//!
//! Every earlier gate proved one layer. This one asks whether the contracts
//! that existed *before* the visual editor still hold now that it exists:
//! a revision conflict must still refuse to overwrite, a signal must still
//! preserve the draft, a narrow terminal must still draw something, and the
//! phases 5.0D.1 through 5.0D.3 must be untouched.
//!
//! It also points the hostile inputs at the visual editor specifically. The
//! projector has been fuzzed since B.1, but "the projection survives" and "the
//! application survives" are different claims, and only the second one is what
//! a reader experiences.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use noteit_core::write::{NoteDraft, NoteMutation, WriteOperation};
use noteit_core::{authority::perform_at, NoteItCore, StorePaths, Uuid};
use noteit_tui::app::{App, EditorMode, Focus};
use ratatui::{backend::TestBackend, Terminal};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use support::cleanup_coordination;
use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    paths: StorePaths,
    id: Uuid,
    app: App,
    terminal: Terminal<TestBackend>,
}

impl Fixture {
    fn new(body: &str) -> Self {
        Self::sized(body, 120, 40)
    }

    fn sized(body: &str, columns: u16, rows: u16) -> Self {
        let root = tempfile::tempdir().unwrap();
        let paths = StorePaths::from_custom_paths(
            root.path().join("notes"),
            root.path().join("config"),
            root.path().join("state/note-it"),
            root.path().join("runtime"),
        );
        let created = perform_at(
            &paths,
            &WriteOperation::CreateNote {
                draft: NoteDraft {
                    content: body.into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let app = App::new_at(paths.clone(), Arc::new(AtomicBool::new(false)));
        Self {
            _root: root,
            paths,
            id: created.outcome.note_id,
            app,
            terminal: Terminal::new(TestBackend::new(columns, rows)).unwrap(),
        }
    }

    /// Opens the note and switches to the visual editor.
    fn open_visual(&mut self) {
        self.app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(self.app.focus, Focus::Editor);
        self.app
            .handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT));
        assert_eq!(self.app.editor_mode, EditorMode::Visual);
    }

    fn key(&mut self, code: KeyCode) {
        self.app.handle_key(KeyEvent::from(code));
    }

    fn ctrl(&mut self, character: char) {
        self.app.handle_key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        ));
    }

    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.key(KeyCode::Char(character));
        }
    }

    fn screen(&mut self) -> String {
        self.app.draw(&mut self.terminal).unwrap();
        let buffer = self.terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn stored(&self) -> String {
        NoteItCore::open_read_only_at(self.paths.clone())
            .read_note(&self.id)
            .unwrap()
            .content
    }

    fn external_write(&self, body: &str) {
        let core = NoteItCore::open_read_only_at(self.paths.clone());
        let document = core.read_note(&self.id).unwrap();
        perform_at(
            &self.paths,
            &WriteOperation::MutateNote {
                selector: self.id.to_string(),
                expected_revision: Some(noteit_core::write::revision_of(&document).unwrap()),
                mutation: NoteMutation::ReplaceBody { body: body.into() },
            },
        )
        .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        cleanup_coordination(&self.paths);
    }
}

// ---------------------------------------------------------------------------
// Revision conflict
// ---------------------------------------------------------------------------

#[test]
fn a_conflict_raised_from_the_visual_editor_overwrites_nothing() {
    // The contract that predates all of this: the note is saved with the
    // revision that was read when the pane opened, and somebody else's write
    // in between is a question, never a casualty.
    let mut fixture = Fixture::new("original");
    fixture.open_visual();
    fixture.type_text("X");
    fixture.external_write("outro autor");

    fixture.ctrl('s');

    assert_eq!(
        fixture.stored(),
        "outro autor",
        "the other author's bytes are intact"
    );
    let screen = fixture.screen();
    assert!(
        screen.contains("Conflito"),
        "and the conflict is named: {screen}"
    );
    assert!(
        fixture.app.draft.is_some(),
        "the draft is still here to be rescued"
    );
}

#[test]
fn a_conflict_leaves_the_visual_editor_usable() {
    // A refusal to save is not a reason to lose the mode the reader was in.
    let mut fixture = Fixture::new("original");
    fixture.open_visual();
    fixture.type_text("X");
    fixture.external_write("outro autor");
    fixture.ctrl('s');

    assert_eq!(fixture.app.editor_mode, EditorMode::Visual);
    assert!(fixture.app.pending_text().is_some());
}

// ---------------------------------------------------------------------------
// Signals and recovery
// ---------------------------------------------------------------------------

#[test]
fn a_signal_with_a_pending_visual_edit_preserves_the_draft() {
    // A signal has nobody to ask, so it writes the text somewhere recoverable
    // rather than discarding it — and the visual editor changes nothing about
    // that. Proved through a real pseudoterminal, because the signal path lives
    // in the terminal loop and a unit-level hook would be proving a hook.
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, id) = support::store(root.path(), &runtime, "original");
    let recovery = root.path().join("state/note-it/tui-recovery");

    let mut tui = support::Tui::spawn(root.path(), |command| {
        command.env("XDG_RUNTIME_DIR", &runtime);
    });
    tui.wait_text("Notas Recentes");
    tui.input(b"\r");
    tui.wait_text("Edição:");

    // Alt+V, then type. The draft is now dirty in the visual editor.
    tui.input(b"\x1bv");
    tui.wait_text("Visual");
    tui.input("pendente".as_bytes());

    // The application itself confirms there is something to lose.
    tui.input(b"\x1b");
    tui.wait_text("Alterações não salvas");

    tui.signal(libc::SIGTERM);
    tui.wait_exit();

    let saved: Vec<_> = std::fs::read_dir(&recovery)
        .expect("the recovery directory")
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(saved.len(), 1, "one preserved draft");
    let text = std::fs::read_to_string(&saved[0]).unwrap();
    assert!(
        text.contains("pendente"),
        "the visual edit was preserved: {text:?}"
    );

    let stored = NoteItCore::open_read_only_at(paths)
        .read_note(&id)
        .unwrap()
        .content;
    assert_eq!(stored, "original", "and the note was not written");

    tui.assert_restored();
    assert!(tui.cooked(), "the terminal was left in raw mode");
}

// ---------------------------------------------------------------------------
// Narrow terminals and resizing
// ---------------------------------------------------------------------------

#[test]
fn the_visual_editor_draws_something_at_every_width_in_the_matrix() {
    for (columns, rows) in [(40, 12), (50, 20), (80, 24), (140, 34)] {
        let mut fixture = Fixture::sized("um **negrito** e texto", columns, rows);
        fixture.open_visual();
        let screen = fixture.screen();
        assert!(
            !screen.trim().is_empty(),
            "{columns}x{rows} drew nothing at all"
        );
        assert_eq!(
            screen.lines().count(),
            rows as usize,
            "{columns}x{rows} drew the wrong number of rows"
        );
    }
}

#[test]
fn an_extremely_small_terminal_still_draws_a_determinate_state() {
    // Below 35x6 the interface shows a minimal state rather than a broken one.
    let mut fixture = Fixture::sized("texto", 20, 4);
    fixture.open_visual();
    let screen = fixture.screen();
    assert_eq!(screen.lines().count(), 4);
    assert!(!screen.trim().is_empty());
}

#[test]
fn resizing_while_in_the_visual_editor_keeps_the_source_and_the_mode() {
    let mut fixture = Fixture::new("um parágrafo com **negrito**");
    fixture.open_visual();
    let before = fixture.app.draft.as_ref().unwrap().text();

    for (columns, rows) in [(40u16, 10u16), (200, 50), (60, 20)] {
        fixture.terminal = Terminal::new(TestBackend::new(columns, rows)).unwrap();
        let screen = fixture.screen();
        assert!(!screen.trim().is_empty());
    }

    assert_eq!(fixture.app.draft.as_ref().unwrap().text(), before);
    assert_eq!(fixture.app.editor_mode, EditorMode::Visual);
}

// ---------------------------------------------------------------------------
// Hostile content, pointed at the application
// ---------------------------------------------------------------------------

#[test]
fn hostile_notes_open_in_the_visual_editor_without_panicking_or_losing_a_byte() {
    // The projector has been fuzzed since B.1. This asks the harder question:
    // does the *application* survive opening one, drawing it, typing into it
    // and switching back?
    let hostile = [
        "<x a=\"",
        "</orphan>",
        "<!-- sem fecho",
        "```sem fecho\nresto",
        "2 < 3 and 4 > 1",
        "<a>".repeat(40).as_str().to_owned().leak(),
        "\u{0}\u{1}\u{7F}controle",
        "\u{202E}rtl\u{202D}",
        "*a**b*",
        "[link](https://example.com/a_(b))",
        "- [x] feita <!-- note-it:completed_at=2026-01-01T00:00:00Z -->",
        "👨\u{200D}👩\u{200D}👧\u{200D}👦\u{0301}",
        "a\r\nb\r\n",
    ];

    for source in hostile {
        let mut fixture = Fixture::new(source);
        fixture.open_visual();

        // Compared against what the store actually holds, not against the
        // literal above: the Core canonicalises a note's final terminator on
        // write, so `"a\r\nb\r\n"` is stored as `"a\r\nb"`. That is a
        // pre-existing Core contract (§26.3 keeps it outside the projection),
        // and the invariant that belongs to this gate is that *opening* a note
        // in the visual editor changes nothing.
        let stored = fixture.stored();
        let before = fixture.app.draft.as_ref().unwrap().text();
        assert_eq!(before, stored, "opening changed the source of {source:?}");

        let screen = fixture.screen();
        assert!(!screen.trim().is_empty(), "{source:?} drew nothing");

        // Type, move, and switch back. Any of these may be refused; none may
        // panic, and none may lose a byte it did not mean to change.
        fixture.type_text("Z");
        fixture.key(KeyCode::Right);
        fixture.key(KeyCode::Backspace);
        fixture
            .app
            .handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT));
        assert_eq!(fixture.app.editor_mode, EditorMode::Markdown);

        // Whatever happened, the note in the store is untouched until a save.
        assert_eq!(fixture.stored(), stored, "{source:?} reached the store");
    }
}

#[test]
fn switching_modes_on_hostile_content_never_changes_a_byte() {
    for source in [
        "<x a=\"",
        "<!-- sem fecho",
        "```sem fecho\nresto",
        "**a *b* c**",
        "a\r\nb",
        "",
    ] {
        let mut fixture = Fixture::new(source);
        fixture.app.handle_key(KeyEvent::from(KeyCode::Enter));
        let before = fixture.app.draft.as_ref().unwrap().text();
        let history = fixture.app.draft.as_ref().unwrap().history_depth();

        for _ in 0..8 {
            fixture
                .app
                .handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT));
        }

        assert_eq!(
            fixture.app.draft.as_ref().unwrap().text(),
            before,
            "{source:?} changed while switching modes"
        );
        assert_eq!(fixture.app.draft.as_ref().unwrap().history_depth(), history);
        assert!(
            fixture.app.pending_text().is_none(),
            "{source:?} became dirty"
        );
    }
}

// ---------------------------------------------------------------------------
// Undo, redo and save from the visual editor
// ---------------------------------------------------------------------------

#[test]
fn undo_and_redo_survive_a_trip_through_both_editors() {
    let mut fixture = Fixture::new("abc");
    fixture.open_visual();
    fixture.type_text("X");
    assert_eq!(fixture.app.draft.as_ref().unwrap().text(), "Xabc");

    // Back to Markdown, undo there, forward again.
    fixture
        .app
        .handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT));
    fixture.ctrl('z');
    assert_eq!(fixture.app.draft.as_ref().unwrap().text(), "abc");

    fixture.ctrl('y');
    assert_eq!(fixture.app.draft.as_ref().unwrap().text(), "Xabc");
}

#[test]
fn a_save_from_the_visual_editor_is_a_no_op_when_nothing_changed() {
    // The canonical no-op the Core already guarantees: opening, switching
    // modes and saving writes nothing, so no timestamp moves.
    let mut fixture = Fixture::new("inalterado");
    fixture.open_visual();
    fixture.ctrl('s');
    assert_eq!(fixture.stored(), "inalterado");
    assert!(fixture.app.pending_text().is_none());
}

// ---------------------------------------------------------------------------
// The phases before this one
// ---------------------------------------------------------------------------

#[test]
fn the_markdown_editor_is_exactly_what_it_was() {
    // 5.0D.2's editor is reached the same way, shows the same bytes and is not
    // affected by the existence of a second mode.
    let mut fixture = Fixture::new("um **negrito** com ç");
    fixture.app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(fixture.app.editor_mode, EditorMode::Markdown);

    let screen = fixture.screen();
    assert!(
        screen.contains("**negrito**"),
        "the Markdown editor shows the source: {screen}"
    );
    assert!(screen.contains("Edição:"), "with the title it always had");
}

#[test]
fn the_reader_is_untouched_by_the_visual_editor() {
    // 5.0D.1's reader renders, it does not edit, and it never showed markup.
    let mut fixture = Fixture::new("um **negrito**");
    // Arrow keys preview in the reader without opening the editor.
    fixture.key(KeyCode::Down);
    let screen = fixture.screen();
    assert!(
        !screen.contains("**negrito**"),
        "the reader still hides markup: {screen}"
    );
}
