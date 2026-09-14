//! Phase 5.0D.R5 — what the manual test found that the suite had not.
//!
//! Three defects came out of a person using the visual editor rather than a
//! test driving it: the caret was drawn on every line above the real one, a
//! styled Enter leaked the canonical `<span>` into the projection, and the
//! result was that Visual felt like a preview you could not type into. Each
//! test here fails on the code that shipped before this phase.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use noteit_tui::app::{App, EditorMode, Focus};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};
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

/// The same, already in the visual editor.
fn visual(root: &std::path::Path, content: &str) -> App {
    let mut app = editing(root, content);
    app.handle_key(alt('v'));
    assert_eq!(app.editor_mode, EditorMode::Visual);
    app
}

fn alt(character: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(character), KeyModifiers::ALT)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::from(code)
}

/// Every cell drawn reversed, as `(column, row, symbol)`.
///
/// The caret is the only thing the editor reverses when nothing is selected,
/// so counting these counts carets — which is the property a reader actually
/// observes, and the one the renderer was getting wrong.
fn reversed_cells(app: &App, width: u16, height: u16) -> Vec<(u16, u16, String)> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    app.draw(&mut terminal).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut found = Vec::new();
    for row in 0..height {
        for column in 0..width {
            let cell = &buffer[(column, row)];
            if cell.modifier.contains(Modifier::REVERSED) {
                found.push((column, row, cell.symbol().to_owned()));
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------
// R5-G05 / R5-G06 / R5-G07 — one caret, and only one
// ---------------------------------------------------------------------------

#[test]
fn r5_g05_the_visual_editor_draws_exactly_one_caret() {
    let root = tempfile::tempdir().unwrap();
    let app = visual(
        root.path(),
        "primeira linha\n\nsegunda linha\n\nterceira linha\n\nquarta linha\n",
    );

    let carets = reversed_cells(&app, 80, 24);
    assert_eq!(
        carets.len(),
        1,
        "one caret is observable, not one per block: {carets:?}"
    );
}

#[test]
fn r5_g06_moving_the_caret_leaves_no_artefact_behind() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(
        root.path(),
        "primeira linha\n\nsegunda linha\n\nterceira linha\n",
    );

    for code in [
        KeyCode::Down,
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Right,
        KeyCode::Up,
        KeyCode::End,
        KeyCode::Home,
        KeyCode::Left,
    ] {
        app.handle_key(key(code));
        let carets = reversed_cells(&app, 80, 24);
        assert_eq!(
            carets.len(),
            1,
            "still one caret after {code:?}: {carets:?}"
        );
    }
}

#[test]
fn r5_g07_resizing_does_not_multiply_the_caret() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(
        root.path(),
        "uma linha bastante longa que certamente ultrapassa quarenta colunas\n\noutra\n\nmais uma\n",
    );
    app.handle_key(key(KeyCode::Down));

    for width in [140u16, 40, 100] {
        let carets = reversed_cells(&app, width, 24);
        assert_eq!(carets.len(), 1, "one caret at {width} columns: {carets:?}");
    }
}

#[test]
fn r5_g05_switching_back_to_markdown_leaves_one_caret_too() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "primeira\n\nsegunda\n\nterceira\n");
    app.handle_key(key(KeyCode::Down));
    assert_eq!(reversed_cells(&app, 80, 24).len(), 1, "one in Visual");

    app.handle_key(alt('v'));
    assert_eq!(app.editor_mode, EditorMode::Markdown);
    assert_eq!(
        reversed_cells(&app, 80, 24).len(),
        1,
        "and one in Markdown: no ghost left by the mode switch"
    );
}

// ---------------------------------------------------------------------------
// R5-G01 / R5-G02 — a blank note is a note you can type into
// ---------------------------------------------------------------------------

/// The draft's text, which `pending_text` reports only when it differs from
/// what the store holds — so `None` here means "nothing was written", which is
/// exactly the assertion a refusal test wants.
fn pending_source(app: &App) -> Option<String> {
    app.pending_text()
}

/// The same, for a test that has just typed something and expects it to stick.
fn draft_source(app: &App) -> String {
    pending_source(app).expect("the draft differs from the store")
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        app.handle_key(key(KeyCode::Char(character)));
    }
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

#[test]
fn r5_g01_an_empty_note_is_editable_in_the_visual_editor() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "");

    assert!(
        app.visual_cursor.is_some(),
        "an empty note has a caret: it is not protected source"
    );
    assert_eq!(
        reversed_cells(&app, 80, 24).len(),
        1,
        "and the caret is drawn, so the reader can see where typing lands"
    );

    type_text(&mut app, "gustavo");
    assert_eq!(
        draft_source(&app),
        "gustavo",
        "typing reaches the draft at the right place"
    );

    let screen = rendered(&app, 80, 24);
    assert!(
        !screen.contains("fonte protegida"),
        "and nothing claims the empty note was protected: {screen}"
    );
}

#[test]
fn r5_g02_enter_in_a_blank_note_opens_a_second_editable_line() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "");

    app.handle_key(key(KeyCode::Enter));
    type_text(&mut app, "depois");
    assert_eq!(
        draft_source(&app),
        "\ndepois",
        "Enter on a blank note is a break, not a refusal"
    );
    assert_eq!(reversed_cells(&app, 80, 24).len(), 1, "still one caret");
}

#[test]
fn r5_g01_a_whitespace_only_note_is_editable_too() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "\n\n");

    assert!(app.visual_cursor.is_some(), "whitespace is not protection");
    type_text(&mut app, "x");
    assert!(
        draft_source(&app).contains('x'),
        "and it takes the character"
    );
}

#[test]
fn r5_g13_protected_source_is_still_refused() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "```\ncodigo\n```\n");

    assert!(
        app.visual_cursor.is_none(),
        "a fence has no visual caret: this is the case `is_blank` must not widen"
    );
    assert_eq!(pending_source(&app), None, "nothing pending to begin with");
    type_text(&mut app, "x");
    assert_eq!(
        pending_source(&app),
        None,
        "and typing into it writes nothing at all"
    );
    let screen = rendered(&app, 80, 24);
    assert!(
        screen.contains("fonte protegida"),
        "the refusal still says why: {screen}"
    );
}
