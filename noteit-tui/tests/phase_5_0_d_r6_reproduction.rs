//! Fase 5.0D.R6.A — reproducing the manual failure, on screen.
//!
//! R5's suite was green and the person using the editor said no. Every test in
//! this file is written from what they observed rather than from what the
//! model holds: it presses a key, draws a frame, and reads the frame back.
//!
//! Running it with `--nocapture` prints the trace §7 asks for — the key, the
//! draft cursor, the visual caret, both viewports and the caret's place on
//! screen — for each of the sequences A through I.

mod support;

use crossterm::event::KeyCode;
use noteit_tui::app::EditorMode;
use support::{Screen, CANONICAL_MARKUP};

/// The fixture of §9, verbatim.
const CANARY: &str = "# Título\n\nPrimeira linha com **negrito** e *itálico*.\n\nSegunda linha longa o bastante para quebrar quando o terminal tiver 40 colunas.\n\nTerceira linha.\n";

/// The fixture of §11, verbatim.
const COLOURS: &str = "# Cores\n\ntexto normal\n";

// ---------------------------------------------------------------------------
// R6-000 — the mode a note opens in
// ---------------------------------------------------------------------------

#[test]
fn r6_000_an_editable_note_opens_in_the_visual_editor() {
    let mut screen = Screen::open(CANARY);
    assert_eq!(
        screen.mode(),
        EditorMode::Visual,
        "§4.1: opening an editable note enters Visual"
    );
    let frame = screen.frame();
    assert!(
        frame.text().contains("Visual"),
        "§4.3: the title says Visual:\n{}",
        frame.text()
    );
    assert!(
        !frame.text().contains("**negrito**"),
        "and the markup is hidden:\n{}",
        frame.text()
    );
}

#[test]
fn r6_000_the_source_mode_names_itself() {
    let mut screen = Screen::open(CANARY);
    let frame = screen.alt('v');
    assert_eq!(screen.mode(), EditorMode::Markdown);
    assert!(
        frame.text().contains("Markdown/Fonte"),
        "§4.2: the raw mode is never unlabelled:\n{}",
        frame.text()
    );
    assert!(
        frame.text().contains("**negrito**"),
        "and it does show the source:\n{}",
        frame.text()
    );
}

// ---------------------------------------------------------------------------
// R6-001 — the caret
// ---------------------------------------------------------------------------

#[test]
fn r6_001_the_visual_viewport_is_never_driven_by_the_raw_cursor() {
    // The reader's report: "ao digitar, o cursor pode saltar para uma posição
    // acima" and "a tela volta para cima". Both are one defect — the visual
    // pane's scroll was the Markdown pane's scroll, computed from a cursor the
    // visual editor does not use.
    let mut screen = Screen::open(CANARY);
    // Walk down to the last block in Visual; the raw cursor stays at 0,0.
    for _ in 0..8 {
        screen.key(KeyCode::Down);
    }
    let frame = screen.frame();
    let (_, row) = frame.caret();
    assert!(
        row > 0,
        "the caret is somewhere below the top after eight Downs:\n{}",
        frame.text()
    );
    assert!(
        frame.caret_row().contains("Terceira"),
        "and it is on the last block, not scrolled back to the first:\n{}",
        frame.text()
    );
}

#[test]
fn r6_001_down_moves_one_visual_row_inside_a_wrapped_paragraph() {
    // §10. The second paragraph wraps onto several rows at 40 columns. Down
    // from its first row must reach its second row, not the block below.
    let mut screen = Screen::open_sized(CANARY, 40, 24);
    // Onto the wrapped paragraph: Título, blank, "Primeira…", blank, "Segunda…"
    let mut rows = Vec::new();
    for _ in 0..12 {
        let frame = screen.frame();
        rows.push(frame.caret_row());
        if frame.caret_row().contains("Segunda") {
            break;
        }
        screen.key(KeyCode::Down);
    }
    let frame = screen.frame();
    assert!(
        frame.caret_row().contains("Segunda"),
        "reached the wrapped paragraph, rows seen: {rows:?}\n{}",
        frame.text()
    );
    let (_, first) = frame.caret();
    let after = screen.key(KeyCode::Down);
    let (_, second) = after.caret();
    assert_eq!(
        second,
        first + 1,
        "Down inside a wrapped paragraph goes to the next visual row:\n{}",
        after.text()
    );
}

#[test]
fn r6_001_up_and_down_keep_the_visual_column() {
    let mut screen = Screen::open_sized(CANARY, 40, 24);
    while !screen.frame().caret_row().contains("Segunda") {
        screen.key(KeyCode::Down);
    }
    for _ in 0..10 {
        screen.key(KeyCode::Right);
    }
    let before = screen.frame().caret();
    let down = screen.key(KeyCode::Down).caret();
    let back = screen.key(KeyCode::Up).caret();
    assert_eq!(down.1, before.1 + 1, "one row down");
    assert_eq!(
        down.0, before.0,
        "the preferred visual column is kept going down"
    );
    assert_eq!(back, before, "and Up returns to exactly where Down started");
}

#[test]
fn r6_001_typing_lands_where_the_caret_is() {
    let mut screen = Screen::open_sized(CANARY, 40, 24);
    while !screen.frame().caret_row().contains("Segunda") {
        screen.key(KeyCode::Down);
    }
    screen.key(KeyCode::Down);
    for _ in 0..4 {
        screen.key(KeyCode::Right);
    }
    let before = screen.frame().caret();
    let after = screen.key(KeyCode::Char('X'));
    let (column, row) = after.caret();
    assert_eq!(row, before.1, "typing does not change row");
    assert_eq!(column, before.0 + 1, "and the caret advances by one cell");
    assert!(
        after.rows[row as usize].contains('X'),
        "the character is on the caret's own row:\n{}",
        after.text()
    );
}

// ---------------------------------------------------------------------------
// R6-002 — markup in front of the reader
// ---------------------------------------------------------------------------

#[test]
fn r6_002_colour_then_typing_never_shows_a_span() {
    // §11, key for key, asserting every frame and not just the last.
    let mut screen = Screen::open(COLOURS);
    assert_eq!(screen.mode(), EditorMode::Visual);
    screen.key(KeyCode::End);

    screen.alt('f');
    screen.key(KeyCode::Enter); // Cor do texto
    screen.key(KeyCode::Down); // the first colour
    let frame = screen.key(KeyCode::Enter);
    frame.assert_hides(CANONICAL_MARKUP, "after choosing a colour");

    let mut frames = screen.type_text("gustavo testes de verdade");
    frames.push(screen.key(KeyCode::Char(' ')));
    frames.extend(screen.type_text("continua"));
    frames.push(screen.key(KeyCode::Enter));
    frames.extend(screen.type_text("segunda linha"));
    for (index, frame) in frames.iter().enumerate() {
        frame.assert_hides(CANONICAL_MARKUP, &format!("frame {index}"));
    }

    // And the source really is canonical underneath.
    let source = screen.source();
    assert!(
        source.contains("<span data-note-it-color="),
        "the canonical source carries the wrapper: {source:?}"
    );
}

#[test]
fn r6_002_highlight_then_typing_never_shows_a_mark() {
    let mut screen = Screen::open(COLOURS);
    screen.key(KeyCode::End);

    screen.alt('f');
    screen.key(KeyCode::Down); // Marca-texto
    screen.key(KeyCode::Enter);
    screen.key(KeyCode::Down); // Amarelo
    let frame = screen.key(KeyCode::Enter);
    frame.assert_hides(CANONICAL_MARKUP, "after choosing a highlight");

    let mut frames = screen.type_text("teste marca texto");
    frames.push(screen.key(KeyCode::Char(' ')));
    frames.extend(screen.type_text("continua"));
    frames.push(screen.key(KeyCode::Enter));
    frames.extend(screen.type_text("nova linha"));
    for (index, frame) in frames.iter().enumerate() {
        frame.assert_hides(CANONICAL_MARKUP, &format!("frame {index}"));
    }
    assert!(
        screen.source().contains("<mark data-note-it-highlight="),
        "the canonical source carries the wrapper: {:?}",
        screen.source()
    );
}

#[test]
fn r6_002_enter_with_a_style_armed_leaves_no_empty_wrapper() {
    // §16. Enter is structural. The style stays armed as a state of the
    // interface; only the next character materialises it.
    let mut screen = Screen::open(COLOURS);
    screen.key(KeyCode::End);
    screen.alt('f');
    screen.key(KeyCode::Enter);
    screen.key(KeyCode::Down);
    screen.key(KeyCode::Enter);

    screen.type_text("abc");
    screen.key(KeyCode::Enter);
    let source_after_enter = screen.source();
    assert!(
        !source_after_enter.contains("</span><span"),
        "Enter must not close and reopen an empty wrapper: {source_after_enter:?}"
    );
    assert!(
        !source_after_enter.contains("\"></span>"),
        "and must not leave an empty one: {source_after_enter:?}"
    );

    let frame = screen.key(KeyCode::Char('d'));
    frame.assert_hides(CANONICAL_MARKUP, "after Enter then a character");
    assert!(
        screen.source().contains(">d</span>"),
        "the character after Enter carries the style: {:?}",
        screen.source()
    );
}

// ---------------------------------------------------------------------------
// The trace of §7
// ---------------------------------------------------------------------------

#[test]
fn r6_a_trace_of_the_manual_sequences() {
    let mut lines = Vec::new();
    for (name, width) in [("A-I @80", 80u16), ("A-I @40", 40)] {
        let mut screen = Screen::open_sized(CANARY, width, 24);
        if screen.mode() != EditorMode::Visual {
            screen.alt('v');
        }
        lines.push(format!("--- {name} ---"));
        lines.push(screen.trace("open"));
        let steps: Vec<(&str, KeyCode)> = vec![
            ("A left", KeyCode::Left),
            ("A right", KeyCode::Right),
            ("B down", KeyCode::Down),
            ("B down", KeyCode::Down),
            ("B up", KeyCode::Up),
            ("C type", KeyCode::Char('X')),
            ("D space", KeyCode::Char(' ')),
            ("E enter", KeyCode::Enter),
            ("F type", KeyCode::Char('Y')),
        ];
        for (label, code) in steps {
            let frame = screen.key(code);
            lines.push(format!(
                "{}  caret={:?}",
                screen.trace(label),
                frame.carets.first().map(|(c, r, _)| (*c, *r))
            ));
        }
        let frame = screen.alt('v');
        lines.push(format!(
            "{}  caret={:?}",
            screen.trace("I alt+v"),
            frame.carets.first().map(|(c, r, _)| (*c, *r))
        ));
        let frame = screen.alt('v');
        lines.push(format!(
            "{}  caret={:?}",
            screen.trace("I alt+v back"),
            frame.carets.first().map(|(c, r, _)| (*c, *r))
        ));
    }
    println!("{}", lines.join("\n"));
}
