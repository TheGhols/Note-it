//! The visual editor as the application actually presents it.
//!
//! Everything up to here proved the engine. This proves the part a reader
//! touches: that `Alt+V` opens the visual editor, that what it draws is the
//! meaning rather than the markup, that typing in it changes the same `Draft`
//! the Markdown editor holds, that switching back is not a move, and that a
//! refusal says so instead of doing nothing.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use noteit_tui::app::{App, EditorMode, Focus};
use ratatui::{backend::TestBackend, Terminal};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use support::store;

/// An application with `content` open in the editor.
fn editing(root: &std::path::Path, content: &str) -> App {
    let runtime = root.join("runtime");
    let (paths, _) = store(root, &runtime, content);
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.focus, Focus::Editor, "Enter opens the editor");
    app
}

fn alt(character: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(character), KeyModifiers::ALT)
}

/// Everything on screen, as one string.
fn rendered(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    app.draw(&mut terminal).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

// ---------------------------------------------------------------------------
// Switching modes
// ---------------------------------------------------------------------------

#[test]
fn alt_v_opens_the_visual_editor_and_alt_v_leaves_it() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "texto simples");

    assert_eq!(
        app.editor_mode,
        EditorMode::Markdown,
        "notes open in Markdown"
    );
    app.handle_key(alt('v'));
    assert_eq!(app.editor_mode, EditorMode::Visual);
    app.handle_key(alt('v'));
    assert_eq!(app.editor_mode, EditorMode::Markdown);
}

#[test]
fn the_title_says_which_editor_has_the_keyboard() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "texto");

    // The footer names the mode switch in both modes, so the title is what
    // distinguishes them: Markdown keeps exactly the title it always had.
    let markdown = rendered(&app, 80, 20);
    assert!(
        markdown.contains("Edição:"),
        "the Markdown title is unchanged"
    );
    assert!(
        !markdown.contains("· Visual"),
        "and does not claim to be the visual editor"
    );

    app.handle_key(alt('v'));
    let visual = rendered(&app, 80, 20);
    assert!(
        visual.contains("· Visual"),
        "the visual editor announces itself in the title: {visual}"
    );
}

#[test]
fn switching_modes_without_typing_changes_no_byte_and_no_history() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "# Título\n\nUm **parágrafo** com ç.\n");

    let before = app.draft.as_ref().unwrap().text();
    let history = app.draft.as_ref().unwrap().history_depth();

    for _ in 0..6 {
        app.handle_key(alt('v'));
    }

    assert_eq!(app.draft.as_ref().unwrap().text(), before);
    assert_eq!(app.draft.as_ref().unwrap().history_depth(), history);
    assert!(
        app.pending_text().is_none(),
        "switching created no pending edit"
    );
}

#[test]
fn leaving_the_editor_returns_to_markdown_for_the_next_note() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "texto");
    app.handle_key(alt('v'));
    assert_eq!(app.editor_mode, EditorMode::Visual);

    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.focus, Focus::Reader);
    assert_eq!(
        app.editor_mode,
        EditorMode::Markdown,
        "a note always opens in Markdown"
    );
}

// ---------------------------------------------------------------------------
// What the visual editor draws
// ---------------------------------------------------------------------------

#[test]
fn the_visual_editor_hides_the_markup_it_can_edit() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "um **negrito** e um *itálico*");

    let markdown = rendered(&app, 80, 20);
    assert!(
        markdown.contains("**negrito**"),
        "Markdown shows the asterisks"
    );

    app.handle_key(alt('v'));
    let visual = rendered(&app, 80, 20);
    assert!(visual.contains("negrito"), "the word is still there");
    assert!(
        !visual.contains("**negrito**"),
        "but its asterisks are not: {visual}"
    );
}

#[test]
fn the_visual_editor_still_shows_what_it_cannot_edit() {
    // A link's destination is B.7 work. Until that gate passes the whole link
    // is source-visible, because hiding it would tell the reader they can edit
    // it — and the destination is the part that must never be edited by
    // accident.
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "veja [o site](https://example.com/a_(b))");
    app.handle_key(alt('v'));

    let visual = rendered(&app, 120, 20);
    assert!(
        visual.contains("https://example.com"),
        "unsupported markup stays visible: {visual}"
    );
}

#[test]
fn a_colour_is_drawn_as_a_colour_and_its_tag_is_not_drawn() {
    // B.6: the canonical wrapper the graphical editor writes becomes meaning.
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(
        root.path(),
        "<span data-note-it-color=\"#DC2626\">vermelho</span>",
    );
    app.handle_key(alt('v'));

    let visual = rendered(&app, 120, 20);
    assert!(visual.contains("vermelho"), "the word is there");
    assert!(
        !visual.contains("data-note-it-color"),
        "and its tag is not: {visual}"
    );

    // Drawn in the colour the note asked for, not merely undrawn.
    let mut terminal = ratatui::Terminal::new(TestBackend::new(120, 20)).unwrap();
    app.draw(&mut terminal).unwrap();
    let painted = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .any(|cell| cell.fg == ratatui::style::Color::Rgb(0xDC, 0x26, 0x26));
    assert!(painted, "the red the note asked for reached a cell");
}

#[test]
fn a_heading_shows_its_text_without_its_hashes() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "## Meu título");
    app.handle_key(alt('v'));

    let visual = rendered(&app, 80, 20);
    assert!(visual.contains("Meu título"));
    assert!(
        !visual.contains("## Meu"),
        "the prefix is not drawn: {visual}"
    );
}

// ---------------------------------------------------------------------------
// Typing in the visual editor
// ---------------------------------------------------------------------------

#[test]
fn typing_in_the_visual_editor_changes_the_same_draft() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "abc");
    app.handle_key(alt('v'));

    app.handle_key(KeyEvent::from(KeyCode::Char('X')));
    assert_eq!(app.draft.as_ref().unwrap().text(), "Xabc");

    // And the Markdown editor sees it, because there is only one source.
    app.handle_key(alt('v'));
    assert_eq!(app.draft.as_ref().unwrap().text(), "Xabc");
    assert!(app.pending_text().is_some(), "the note is now dirty");
}

#[test]
fn typing_inside_a_hidden_mark_stays_inside_it() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "**abc**");
    app.handle_key(alt('v'));

    // The caret snapped to the first legal slot, which is inside the mark.
    app.handle_key(KeyEvent::from(KeyCode::Char('X')));
    let text = app.draft.as_ref().unwrap().text();
    assert!(
        text == "**Xabc**" || text == "X**abc**",
        "unexpected result {text:?}"
    );
    assert!(text.contains("abc"), "the content survived");
    assert_eq!(text.matches('*').count(), 4, "the delimiters are intact");
}

#[test]
fn enter_in_the_visual_editor_splits_the_block() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "abcd");
    app.handle_key(alt('v'));
    app.handle_key(KeyEvent::from(KeyCode::Right));
    app.handle_key(KeyEvent::from(KeyCode::Right));
    app.handle_key(KeyEvent::from(KeyCode::Enter));

    assert_eq!(app.draft.as_ref().unwrap().text(), "ab\n\ncd");
}

#[test]
fn backspace_removes_one_whole_grapheme() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "a👨\u{200D}👩\u{200D}👧\u{200D}👦b");
    app.handle_key(alt('v'));

    // Move past the emoji, then remove it in one press.
    app.handle_key(KeyEvent::from(KeyCode::Right));
    app.handle_key(KeyEvent::from(KeyCode::Right));
    app.handle_key(KeyEvent::from(KeyCode::Backspace));

    assert_eq!(app.draft.as_ref().unwrap().text(), "ab");
}

#[test]
fn undo_after_a_visual_edit_restores_the_whole_command() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "abc");
    app.handle_key(alt('v'));
    app.handle_key(KeyEvent::from(KeyCode::Char('X')));
    assert_eq!(app.draft.as_ref().unwrap().text(), "Xabc");

    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    assert_eq!(app.draft.as_ref().unwrap().text(), "abc");
}

// ---------------------------------------------------------------------------
// Refusals are announced
// ---------------------------------------------------------------------------

#[test]
fn a_refused_edit_says_so_and_changes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "<custom>protegido</custom>");
    app.handle_key(alt('v'));

    let before = app.draft.as_ref().unwrap().text();
    app.handle_key(KeyEvent::from(KeyCode::Char('X')));

    assert_eq!(
        app.draft.as_ref().unwrap().text(),
        before,
        "a refusal changed the source"
    );
    assert!(
        !app.notice.is_empty(),
        "a refusal must not be silent — the reader needs to know why"
    );
    assert!(
        app.notice.contains("Markdown"),
        "and it must offer the way out: {}",
        app.notice
    );
}

#[test]
fn a_note_that_is_entirely_protected_says_so_on_entry() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "<custom>tudo protegido</custom>");
    app.handle_key(alt('v'));

    assert!(
        !app.notice.is_empty(),
        "entering a note with nowhere to type must explain itself"
    );
    // And Esc still leaves, so it is never a trap.
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.focus, Focus::Reader);
}

// ---------------------------------------------------------------------------
// Saving is unchanged
// ---------------------------------------------------------------------------

#[test]
fn saving_from_the_visual_editor_goes_through_the_same_authority() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, id) = store(root.path(), &runtime, "antes");
    let mut app = App::new_at(paths.clone(), Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    app.handle_key(alt('v'));
    app.handle_key(KeyEvent::from(KeyCode::Char('X')));
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    let stored = noteit_core::NoteItCore::open_read_only_at(paths)
        .read_note(&id)
        .unwrap()
        .content;
    assert!(
        stored.contains('X'),
        "the edit reached the store: {stored:?}"
    );
    assert!(stored.contains("antes"), "and kept what was there");
}

#[test]
fn the_footer_keeps_every_command_it_promised_and_adds_the_mode_switch() {
    // The 5.0D.3 contract: Salvar, Formatar and Sair stay discoverable at
    // every practical width. Adding a mode switch may not cost one of them.
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "texto");

    for mode in [EditorMode::Markdown, EditorMode::Visual] {
        if app.editor_mode != mode {
            app.handle_key(alt('v'));
        }
        for width in [50, 60, 70, 80, 100, 120, 140] {
            let screen = rendered(&app, width, 20);
            for promise in ["Salvar", "Formatar", "Sair", "Visual"] {
                assert!(
                    screen.contains(promise),
                    "{width} columns in {mode:?} lost {promise}: {screen}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// A real terminal
// ---------------------------------------------------------------------------

#[test]
fn a_real_terminal_edits_in_the_visual_editor_saves_and_is_restored() {
    // Everything above runs against `TestBackend`, which knows nothing about
    // raw mode, escape sequences or a tty. This one uses a pseudoterminal, so
    // the visual editor is proved where it will actually live — and proved to
    // give the terminal back exactly as it found it.
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, id) = store(root.path(), &runtime, "um **negrito** aqui");

    let mut tui = support::Tui::spawn(root.path(), |command| {
        command.env("XDG_RUNTIME_DIR", &runtime);
    });
    tui.wait_text("Notas Recentes");
    tui.input(b"\r");
    tui.wait_text("Edição:");

    // Alt+V is ESC then the character, which is how a terminal sends it.
    tui.input(b"\x1bv");
    tui.wait_text("Visual");

    // Type at the caret, then save.
    tui.input("Z".as_bytes());
    tui.input(b"\x13");
    tui.wait_text("Salv");

    tui.input(b"\x1b");
    tui.input(b"\x1b");
    tui.input(b"q");
    tui.wait_exit();

    let stored = noteit_core::NoteItCore::open_read_only_at(paths)
        .read_note(&id)
        .unwrap()
        .content;
    assert!(
        stored.contains('Z'),
        "the visual edit reached the store: {stored:?}"
    );
    assert!(
        stored.contains("**negrito**"),
        "and the markup it hid is still in the file, byte for byte: {stored:?}"
    );

    tui.assert_restored();
    assert!(tui.cooked(), "the terminal was left in raw mode");
}
