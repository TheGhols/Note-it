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
