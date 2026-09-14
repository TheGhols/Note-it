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
// ---------------------------------------------------------------------------
// R5-G08 / R5-G09 / R5-G10 — styled typing across Enter
// ---------------------------------------------------------------------------

/// Picks a text colour through the format menu, the way a reader does.
fn choose_text_colour(app: &mut App) {
    app.handle_key(alt('f'));
    app.handle_key(key(KeyCode::Enter)); // "Cor do texto"
    app.handle_key(key(KeyCode::Down)); // the first colour
    app.handle_key(key(KeyCode::Enter));
    assert!(app.active_text_color.is_some(), "a colour is active");
}

/// The same for a highlight.
fn choose_highlight(app: &mut App) {
    app.handle_key(alt('f'));
    app.handle_key(key(KeyCode::Down)); // "Realce"
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Down)); // the first colour
    app.handle_key(key(KeyCode::Enter));
    assert!(app.active_highlight.is_some(), "a highlight is active");
}

#[test]
fn r5_g08_colour_then_enter_does_not_leak_a_span_into_the_projection() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "");

    choose_text_colour(&mut app);
    type_text(&mut app, "gustavo testes de verdade");
    app.handle_key(key(KeyCode::Enter));
    type_text(&mut app, "segunda linha");

    let screen = rendered(&app, 100, 24);
    assert!(
        !screen.contains("<span") && !screen.contains("</span"),
        "no canonical tag reaches the visual editor: {screen}"
    );
    assert!(
        !screen.contains("data-note-it-color"),
        "and no canonical attribute either: {screen}"
    );
    assert!(
        screen.contains("segunda linha"),
        "the second line is typable: {screen}"
    );

    let source = draft_source(&app);
    assert!(
        source.contains("gustavo testes de verdade"),
        "the first line survives: {source:?}"
    );
    assert!(
        source.contains("segunda linha"),
        "and so does the second: {source:?}"
    );
    assert!(
        !source.contains("</span></span>") && !source.contains("<span></span>"),
        "the canonical source has no broken or empty wrapper: {source:?}"
    );
}

#[test]
fn r5_g09_highlight_then_enter_does_not_leak_a_mark() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "");

    choose_highlight(&mut app);
    type_text(&mut app, "marcado");
    app.handle_key(key(KeyCode::Enter));
    type_text(&mut app, "depois");

    let screen = rendered(&app, 100, 24);
    assert!(
        !screen.contains("<mark") && !screen.contains("</mark"),
        "no canonical mark tag reaches the visual editor: {screen}"
    );
    assert!(screen.contains("depois"), "the second line is typable");
}

#[test]
fn r5_g08_the_reported_sequence_produces_canonical_source() {
    // Exactly what the manual test did: colour, "gustavo", a space, a phrase,
    // Enter, a second line.
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "");
    choose_text_colour(&mut app);
    let colour = app.active_text_color.expect("a colour is armed");

    type_text(&mut app, "gustavo");
    type_text(&mut app, " ");
    type_text(&mut app, "testes de verdade");
    app.handle_key(key(KeyCode::Enter));
    type_text(&mut app, "segunda linha");

    let source = draft_source(&app);
    let open = format!("<span data-note-it-color=\"{colour}\" style=\"color:{colour}\">");
    assert_eq!(
        source,
        format!("{open}gustavo testes de verdade</span>\n\n{open}segunda linha</span>"),
        "one wrapper per line, closed before the break and reopened after it"
    );
    assert_eq!(
        source.matches("<span").count(),
        2,
        "one wrapper per line, not one per character: {source}"
    );
    assert_eq!(
        source.matches("<span").count(),
        source.matches("</span>").count(),
        "every wrapper is closed: {source}"
    );

    // And the projection of that source shows the reader two coloured lines,
    // with no markup in sight.
    let screen = rendered(&app, 100, 24);
    assert!(
        !screen.contains("span") && !screen.contains("data-note-it"),
        "no markup reaches the visual editor: {screen}"
    );
    assert!(screen.contains("gustavo testes de verdade"));
    assert!(screen.contains("segunda linha"));
    assert_eq!(reversed_cells(&app, 100, 24).len(), 1, "and one caret");
}

#[test]
fn r5_g10_colour_and_highlight_together_stay_canonical() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "");
    choose_text_colour(&mut app);
    choose_highlight(&mut app);

    type_text(&mut app, "ab");
    app.handle_key(key(KeyCode::Enter));
    type_text(&mut app, "cd");

    let source = draft_source(&app);
    for (open, close) in [("<span", "</span>"), ("<mark", "</mark>")] {
        assert_eq!(
            source.matches(open).count(),
            source.matches(close).count(),
            "{open} is balanced: {source}"
        );
        assert_eq!(
            source.matches(open).count(),
            2,
            "one {open} per line: {source}"
        );
    }
    // The highlight is the outer wrapper on both lines, which is the nesting
    // the graphical editor persists.
    assert!(
        !source.contains("<span data-note-it-color=\"#64748B\" style=\"color:#64748B\"><mark"),
        "the mark stays outside the span: {source}"
    );
    let screen = rendered(&app, 100, 24);
    assert!(
        !screen.contains("mark") && !screen.contains("span"),
        "and neither reaches the screen: {screen}"
    );
}

#[test]
fn r5_g11_undo_and_redo_across_a_styled_enter_are_lossless() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "");
    choose_text_colour(&mut app);
    type_text(&mut app, "ab");
    app.handle_key(key(KeyCode::Enter));
    type_text(&mut app, "cd");
    let after = draft_source(&app);

    let undo = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
    let redo = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL);
    for _ in 0..20 {
        app.handle_key(undo);
    }
    for _ in 0..20 {
        app.handle_key(redo);
    }
    assert_eq!(
        pending_source(&app).unwrap_or_default(),
        after,
        "redoing everything undone gets the same bytes back"
    );
}

#[test]
fn r5_g12_visual_to_markdown_and_back_is_not_an_edit() {
    let root = tempfile::tempdir().unwrap();
    let mut app = editing(root.path(), "um **dois** tres\n\n- item\n");
    let before = app.pending_text();
    assert_eq!(before, None, "nothing pending to begin with");

    app.handle_key(alt('v'));
    app.handle_key(alt('v'));
    assert_eq!(app.editor_mode, EditorMode::Markdown);
    assert_eq!(
        app.pending_text(),
        None,
        "a round trip with no keystroke in between writes nothing"
    );
}

#[test]
fn r5_g03_backspace_removes_a_whole_grapheme_cluster() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "familia 👨‍👩‍👧‍👦 fim\n");

    // To the end of the family cluster, then remove it in one Backspace.
    app.handle_key(key(KeyCode::End));
    for _ in 0..4 {
        app.handle_key(key(KeyCode::Left));
    }
    app.handle_key(key(KeyCode::Backspace));

    let source = draft_source(&app);
    assert!(
        !source.contains('\u{1F468}')
            && !source.contains('\u{1F469}')
            && !source.contains('\u{200D}'),
        "the family goes as one grapheme, leaving no half of itself: {source:?}"
    );
    assert!(source.starts_with("familia "), "and nothing else moved");
}

// ---------------------------------------------------------------------------
// R5-G04 / R5-G14 / R5-G15 / R5-G16 — selection, protection and Unicode
// ---------------------------------------------------------------------------

#[test]
fn r5_g04_a_keyboard_selection_is_replaced_in_place() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "alfa bravo charlie\n");

    // Select "alfa" with Shift+Right, then type over it.
    for _ in 0..4 {
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    }
    type_text(&mut app, "X");
    assert_eq!(
        draft_source(&app),
        "X bravo charlie",
        "the selected region is what was replaced"
    );
    assert_eq!(reversed_cells(&app, 80, 24).len(), 1, "one caret after it");
}

#[test]
fn r5_g14_unknown_html_still_refuses_and_still_shows_itself() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(
        root.path(),
        "<custom-thing a=\"1\">conteudo</custom-thing>\n",
    );

    let screen = rendered(&app, 100, 24);
    assert!(
        screen.contains("custom-thing"),
        "unknown HTML stays visible as source rather than being hidden: {screen}"
    );

    // Put the caret where the tag is and try to type through it.
    app.handle_key(key(KeyCode::Home));
    type_text(&mut app, "X");
    let source = pending_source(&app).unwrap_or_default();
    assert!(
        !source.contains("X<custom-thing") && !source.contains("<Xcustom"),
        "nothing was inserted into the tag: {source:?}"
    );
}

#[test]
fn r5_g15_and_g16_unicode_survives_styled_typing_and_the_renderer() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "");
    choose_text_colour(&mut app);

    // Combining acute (NFD), a CJK double-width pair, a flag and a skin tone.
    let sample = "cafe\u{301} \u{4E2D}\u{6587} \u{1F1E7}\u{1F1F7} \u{1F44D}\u{1F3FD}";
    type_text(&mut app, sample);
    app.handle_key(key(KeyCode::Enter));
    type_text(&mut app, "depois");

    let source = draft_source(&app);
    assert!(
        source.contains(sample),
        "every scalar reaches the source unchanged: {source:?}"
    );
    assert_eq!(
        source.matches("<span").count(),
        2,
        "one wrapper per line even across a flag and a skin tone: {source}"
    );
    assert_eq!(
        source.matches("<span").count(),
        source.matches("</span>").count(),
        "and they are balanced: {source}"
    );

    let screen = rendered(&app, 100, 24);
    assert!(!screen.contains("span"), "no markup on screen: {screen}");
    assert_eq!(reversed_cells(&app, 100, 24).len(), 1, "and one caret");
}

#[test]
fn r5_g16_a_flag_is_one_grapheme_to_backspace() {
    let root = tempfile::tempdir().unwrap();
    let mut app = visual(root.path(), "ab \u{1F1E7}\u{1F1F7}\n");

    app.handle_key(key(KeyCode::End));
    app.handle_key(key(KeyCode::Backspace));
    assert_eq!(
        draft_source(&app),
        "ab ",
        "the flag goes whole, leaving no lone regional indicator"
    );
}
