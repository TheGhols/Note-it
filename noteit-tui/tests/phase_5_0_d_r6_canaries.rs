//! Fase 5.0D.R6 — the canaries, stated on screen.
//!
//! §24 is the rule this file exists to obey: every defect R6 closed is
//! asserted against the rendered buffer, not against a `SourceTransaction`.
//! R5's suite proved the transactions and shipped an editor a person could not
//! type in, so "the plan was right" is no longer accepted as evidence.

mod support;

use crossterm::event::KeyCode;
use noteit_tui::app::EditorMode;
use support::{Screen, CANONICAL_MARKUP};

/// §9's fixture, verbatim.
const CANARY: &str = "# Título\n\nPrimeira linha com **negrito** e *itálico*.\n\nSegunda linha longa o bastante para quebrar quando o terminal tiver 40 colunas.\n\nTerceira linha.\n";

/// §11 and §12's fixture.
const COLOURS: &str = "# Cores\n\ntexto normal\n";

// ---------------------------------------------------------------------------
// §9 — the cursor canary, at every width in the matrix
// ---------------------------------------------------------------------------

#[test]
fn r6_g01_the_cursor_canary_holds_at_every_width() {
    // The exact sequence of §9, at the exact widths of §9, asserting after
    // every single key rather than at the end.
    for width in [140u16, 100, 80, 60, 40, 80, 140] {
        let mut screen = Screen::open_sized(CANARY, width, 24);
        assert_eq!(screen.mode(), EditorMode::Visual, "{width}");
        screen.assert_caret_is_sane(&format!("{width}: open"));

        for index in 0..10 {
            screen.key(KeyCode::Left);
            screen.assert_caret_is_sane(&format!("{width}: left {index}"));
        }
        for index in 0..10 {
            screen.key(KeyCode::Right);
            screen.assert_caret_is_sane(&format!("{width}: right {index}"));
        }
        for (index, code) in [
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Home,
            KeyCode::End,
        ]
        .into_iter()
        .enumerate()
        {
            screen.key(code);
            screen.assert_caret_is_sane(&format!("{width}: {code:?} {index}"));
        }

        screen.assert_types_at_caret('X', &format!("{width}: X"));
        screen.assert_caret_is_sane(&format!("{width}: after X"));
        screen.key(KeyCode::Left);
        screen.assert_caret_is_sane(&format!("{width}: left after X"));
        screen.assert_types_at_caret('Y', &format!("{width}: Y"));

        // Both characters reached the source, in the order they were typed.
        assert!(
            screen.source().contains("YX"),
            "{width}: expected `YX` in {:?}",
            screen.source()
        );
    }
}

#[test]
fn r6_g02_the_same_sequence_is_deterministic_over_many_runs() {
    // "Nenhum 'às vezes'" (§9). The same keys from the same start must reach
    // the same caret and the same bytes, every time.
    let run = || {
        let mut screen = Screen::open_sized(CANARY, 60, 24);
        for _ in 0..6 {
            screen.key(KeyCode::Down);
        }
        for _ in 0..7 {
            screen.key(KeyCode::Right);
        }
        screen.key(KeyCode::Char('Z'));
        let frame = screen.frame();
        (frame.caret(), screen.source(), screen.visual_offset())
    };
    let first = run();
    for index in 1..40 {
        assert_eq!(run(), first, "run {index} diverged from the first");
    }
}

#[test]
fn r6_g03_resizing_does_not_move_the_caret_in_the_text() {
    // The caret is a source offset, so it survives a resize; only where it is
    // *drawn* may change.
    let mut screen = Screen::open_sized(CANARY, 140, 24);
    for _ in 0..5 {
        screen.key(KeyCode::Down);
    }
    for _ in 0..12 {
        screen.key(KeyCode::Right);
    }
    let offset = screen.visual_offset();

    for width in [100u16, 80, 60, 40, 80, 140] {
        screen.resize(width, 24);
        assert_eq!(
            screen.visual_offset(),
            offset,
            "resizing to {width} moved the caret in the text"
        );
        screen.assert_caret_is_sane(&format!("resize {width}"));
    }
}

// ---------------------------------------------------------------------------
// §10 — vertical movement is movement between rendered rows
// ---------------------------------------------------------------------------

/// Walks down until the caret is on a row containing `needle`.
fn down_to(screen: &mut Screen, needle: &str) {
    for _ in 0..40 {
        if screen.frame().caret_row().contains(needle) {
            return;
        }
        screen.key(KeyCode::Down);
    }
    panic!(
        "never reached a row containing {needle:?}:\n{}",
        screen.frame().text()
    );
}

#[test]
fn r6_g04_down_and_up_walk_a_wrapped_paragraph_row_by_row() {
    // At 40 columns the third block wraps onto three rows. §10: `↓` is the
    // row immediately below, including a row the terminal's width invented.
    let mut screen = Screen::open_sized(CANARY, 40, 24);
    down_to(&mut screen, "Segunda");
    let first = screen.frame().caret();

    let second = screen.key(KeyCode::Down).caret();
    assert_eq!(second.1, first.1 + 1, "one row down, not one block down");
    let third = screen.key(KeyCode::Down).caret();
    assert_eq!(third.1, second.1 + 1, "and again");

    // Still inside the same paragraph: `↓` did not leave it.
    let frame = screen.frame();
    assert!(
        !frame.caret_row().contains("Terceira"),
        "three rows of the wrapped paragraph, not the next block:\n{}",
        frame.text()
    );

    let back = screen.key(KeyCode::Up).caret();
    assert_eq!(back, second, "Up returns to the row Down came from");
}

#[test]
fn r6_g05_the_preferred_column_survives_a_short_row() {
    // Walking down through a row shorter than the column being kept, and out
    // the other side, comes back to the original column.
    // Wide enough that none of the three blocks wraps, so the only thing
    // changing the column is the short row in the middle.
    let mut screen = Screen::open_sized(
        "linha bem longa para manter a coluna\n\ncurta\n\noutra linha bem longa aqui para provar\n",
        120,
        24,
    );
    screen.key(KeyCode::Home);
    let home = screen.frame().caret();
    for _ in 0..30 {
        screen.key(KeyCode::Right);
    }
    let start = screen.frame().caret();
    assert_eq!(start.0, home.0 + 30, "thirty columns in");

    let short = screen.key(KeyCode::Down).caret();
    assert!(short.0 < start.0, "the short row cannot hold that column");
    let long = screen.key(KeyCode::Down).caret();
    assert_eq!(
        long.0, start.0,
        "and the column comes back on a row long enough for it"
    );
}

#[test]
fn r6_g06_up_from_the_first_row_and_down_from_the_last_stay_put() {
    let mut screen = Screen::open_sized(CANARY, 80, 24);
    let top = screen.key(KeyCode::Up).caret();
    assert_eq!(
        top,
        screen.key(KeyCode::Up).caret(),
        "Up at the top is a no-op"
    );
    screen.assert_caret_is_sane("Up at the top");

    for _ in 0..40 {
        screen.key(KeyCode::Down);
    }
    let bottom = screen.frame().caret();
    assert_eq!(
        bottom,
        screen.key(KeyCode::Down).caret(),
        "Down at the bottom is a no-op"
    );
    screen.assert_caret_is_sane("Down at the bottom");
}

#[test]
fn r6_g07_home_and_end_are_the_ends_of_the_visual_row() {
    let mut screen = Screen::open_sized(CANARY, 40, 24);
    down_to(&mut screen, "Segunda");
    screen.key(KeyCode::Down); // the middle row of the wrapped paragraph
    let middle_row = screen.frame().caret().1;

    let end = screen.key(KeyCode::End).caret();
    assert_eq!(end.1, middle_row, "End stays on the row it was on");
    let home = screen.key(KeyCode::Home).caret();
    assert_eq!(home.1, middle_row, "and so does Home");
    assert!(end.0 > home.0, "End is past Home on the same row");
    assert_eq!(
        screen.key(KeyCode::Home).caret(),
        home,
        "Home again is the same place"
    );
    // One Left from Home leaves the row upwards, which is what makes it the
    // row's first column rather than merely a column near it.
    assert_eq!(
        screen.key(KeyCode::Left).caret().1,
        middle_row - 1,
        "Home really was the start of the row"
    );
}

// ---------------------------------------------------------------------------
// §11, §12, §13 — the formatting canaries
// ---------------------------------------------------------------------------

/// Opens the text-colour submenu and picks the colour at `index`.
fn choose_colour(screen: &mut Screen, index: usize) {
    screen.alt('f');
    screen.key(KeyCode::Enter); // Cor do texto
    for _ in 0..index {
        screen.key(KeyCode::Down);
    }
    screen.key(KeyCode::Enter);
}

/// Opens the highlight submenu and picks the colour at `index`.
fn choose_highlight(screen: &mut Screen, index: usize) {
    screen.alt('f');
    screen.key(KeyCode::Down); // Marca-texto
    screen.key(KeyCode::Enter);
    for _ in 0..index {
        screen.key(KeyCode::Down);
    }
    screen.key(KeyCode::Enter);
}

#[test]
fn r6_g08_the_colour_canary_never_shows_a_tag() {
    // §11, key for key.
    let mut screen = Screen::open(COLOURS);
    screen.key(KeyCode::End);
    choose_colour(&mut screen, 2); // Vermelho

    let mut frames = vec![screen.frame()];
    frames.extend(screen.type_text("gustavo testes de verdade"));
    frames.push(screen.key(KeyCode::Char(' ')));
    frames.extend(screen.type_text("continua"));
    frames.push(screen.key(KeyCode::Enter));
    frames.extend(screen.type_text("segunda linha"));

    for (index, frame) in frames.iter().enumerate() {
        frame.assert_hides(CANONICAL_MARKUP, &format!("colour frame {index}"));
    }

    // Alt+V: now the source may show it, and must.
    let source_frame = screen.alt('v');
    assert_eq!(screen.mode(), EditorMode::Markdown);
    assert!(
        source_frame.text().contains("Markdown/Fonte"),
        "the source mode names itself:\n{}",
        source_frame.text()
    );
    assert!(
        CANONICAL_MARKUP
            .iter()
            .any(|needle| source_frame.text().contains(needle)),
        "§17: the source editor shows the source:\n{}",
        source_frame.text()
    );

    // And back: the markup disappears from the screen again.
    let visual_frame = screen.alt('v');
    assert_eq!(screen.mode(), EditorMode::Visual);
    visual_frame.assert_hides(CANONICAL_MARKUP, "back in Visual");
    for expected in ["gustavo testes de verdade continua", "segunda linha"] {
        assert!(
            visual_frame.text().contains(expected),
            "the text itself is still there: {expected:?}\n{}",
            visual_frame.text()
        );
    }
}

#[test]
fn r6_g09_the_highlight_canary_never_shows_a_mark() {
    // §12, including the exact fragment the manual capture photographed.
    let mut screen = Screen::open(COLOURS);
    screen.key(KeyCode::End);
    choose_highlight(&mut screen, 1); // Amarelo

    let mut frames = vec![screen.frame()];
    frames.extend(screen.type_text("teste marca texto"));
    frames.push(screen.key(KeyCode::Char(' ')));
    frames.extend(screen.type_text("continua"));
    frames.push(screen.key(KeyCode::Enter));
    frames.extend(screen.type_text("nova linha"));

    for (index, frame) in frames.iter().enumerate() {
        frame.assert_hides(CANONICAL_MARKUP, &format!("highlight frame {index}"));
        // The literal shape from the manual test.
        assert!(
            !frame.text().contains("<mark data-note-it-highlight"),
            "frame {index} shows the photographed fragment:\n{}",
            frame.text()
        );
    }

    let source = screen.source();
    assert!(
        source.contains("<mark data-note-it-highlight=\"#FDE68A\""),
        "the canonical source carries the wrapper: {source:?}"
    );
}

#[test]
fn r6_g10_colour_and_highlight_together_stay_clean_and_canonical() {
    // §13.
    let mut screen = Screen::open(COLOURS);
    screen.key(KeyCode::End);
    choose_colour(&mut screen, 2);
    choose_highlight(&mut screen, 1);

    let mut frames = screen.type_text("combinado");
    frames.push(screen.key(KeyCode::Char(' ')));
    frames.push(screen.key(KeyCode::Enter));
    frames.extend(screen.type_text("continuar"));
    for (index, frame) in frames.iter().enumerate() {
        frame.assert_hides(CANONICAL_MARKUP, &format!("combined frame {index}"));
    }

    let source = screen.source();
    assert!(
        source.contains("data-note-it-color=\"#DC2626\""),
        "{source:?}"
    );
    assert!(
        source.contains("data-note-it-highlight=\"#FDE68A\""),
        "{source:?}"
    );
    // Well formed and closed in the order it was opened.
    assert_eq!(
        source.matches("<span").count(),
        source.matches("</span>").count()
    );
    assert_eq!(
        source.matches("<mark").count(),
        source.matches("</mark>").count()
    );
    assert!(
        !source.contains("<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\"></span>"),
        "no empty wrapper: {source:?}"
    );
    assert!(
        !source.contains(
            "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\"></mark>"
        ),
        "no empty wrapper: {source:?}"
    );
}

// ---------------------------------------------------------------------------
// §15 — the formatting menu acts on the visual selection
// ---------------------------------------------------------------------------

#[test]
fn r6_g11_alt_f_colours_the_visual_selection() {
    let mut screen = Screen::open("palavra colorida\n");
    screen.key(KeyCode::Home);
    for _ in 0..7 {
        screen.shift(KeyCode::Right);
    }
    // The selection is visible before the menu opens.
    assert!(
        screen
            .app
            .visual_cursor
            .and_then(|cursor| cursor.anchor)
            .is_some(),
        "Shift+Right made a visual selection"
    );

    choose_colour(&mut screen, 2);
    let source = screen.source();
    assert!(
        source.contains(
            "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">palavra</span>"
        ),
        "the selection itself was coloured: {source:?}"
    );
    assert!(
        source.contains("</span> colorida"),
        "and nothing beyond it was: {source:?}"
    );
    screen
        .frame()
        .assert_hides(CANONICAL_MARKUP, "after colouring a selection");
}

#[test]
fn r6_g12_alt_f_highlights_the_visual_selection() {
    let mut screen = Screen::open("realcar isto\n");
    screen.key(KeyCode::Home);
    for _ in 0..7 {
        screen.shift(KeyCode::Right);
    }
    choose_highlight(&mut screen, 1);
    assert!(
        screen.source().contains(
            "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">realcar</mark>"
        ),
        "{:?}",
        screen.source()
    );
    screen
        .frame()
        .assert_hides(CANONICAL_MARKUP, "after highlighting a selection");
}

#[test]
fn r6_g13_alt_f_without_a_selection_arms_the_next_character() {
    let mut screen = Screen::open("texto\n");
    let before = screen.source();
    screen.key(KeyCode::End);
    choose_colour(&mut screen, 2);
    assert_eq!(screen.app.active_text_color, Some("#DC2626"));
    assert_eq!(screen.source(), before, "arming a style writes nothing");
    screen.key(KeyCode::Char('Z'));
    assert!(
        screen.source().contains(">Z</span>"),
        "the next character carries it: {:?}",
        screen.source()
    );
}

#[test]
fn r6_g14_a_selection_crossing_blocks_is_refused_by_name() {
    // §20: a refusal explains itself instead of doing nothing.
    let mut screen = Screen::open("primeiro\n\nsegundo\n");
    screen.key(KeyCode::End);
    for _ in 0..4 {
        screen.shift(KeyCode::Right);
    }
    let before = screen.source();
    choose_colour(&mut screen, 2);
    assert_eq!(screen.source(), before, "nothing was written");
    assert!(
        screen.app.notice.contains("Recusado"),
        "the refusal is named: {:?}",
        screen.app.notice
    );
}

// ---------------------------------------------------------------------------
// §16 — Enter with a style armed
// ---------------------------------------------------------------------------

#[test]
fn r6_g15_enter_with_a_style_writes_no_empty_wrapper() {
    for (colour, highlight) in [(true, false), (false, true), (true, true)] {
        let mut screen = Screen::open("base\n");
        screen.key(KeyCode::End);
        if colour {
            choose_colour(&mut screen, 2);
        }
        if highlight {
            choose_highlight(&mut screen, 1);
        }
        screen.type_text("abc");
        let frame = screen.key(KeyCode::Enter);
        frame.assert_hides(CANONICAL_MARKUP, "after Enter");

        let source = screen.source();
        for empty in ["\"></span>", "\"></mark>", "</span><span", "</mark><mark"] {
            assert!(
                !source.contains(empty),
                "({colour},{highlight}) Enter left `{empty}`: {source:?}"
            );
        }

        // The style is still armed, and the next character materialises it.
        let frame = screen.key(KeyCode::Char('d'));
        frame.assert_hides(CANONICAL_MARKUP, "after the character following Enter");
        let source = screen.source();
        if colour {
            assert!(
                source.contains("data-note-it-color"),
                "({colour},{highlight}) the colour came back: {source:?}"
            );
        }
        if highlight {
            assert!(
                source.contains("data-note-it-highlight"),
                "({colour},{highlight}) the highlight came back: {source:?}"
            );
        }
        assert!(source.contains('d'), "{source:?}");
    }
}

#[test]
fn r6_g16_enter_enter_and_space_around_a_style_stay_clean() {
    // §16's remaining sequences: Enter → Enter → letter, and
    // colour → Space → Enter → Space → letter.
    let mut screen = Screen::open("base\n");
    screen.key(KeyCode::End);
    choose_colour(&mut screen, 2);
    for code in [KeyCode::Enter, KeyCode::Enter] {
        screen
            .key(code)
            .assert_hides(CANONICAL_MARKUP, "Enter Enter");
    }
    screen
        .key(KeyCode::Char('a'))
        .assert_hides(CANONICAL_MARKUP, "letter after Enter Enter");

    let mut screen = Screen::open("base\n");
    screen.key(KeyCode::End);
    choose_colour(&mut screen, 2);
    for code in [
        KeyCode::Char(' '),
        KeyCode::Enter,
        KeyCode::Char(' '),
        KeyCode::Char('a'),
    ] {
        screen
            .key(code)
            .assert_hides(CANONICAL_MARKUP, "space/enter run");
    }
    let source = screen.source();
    assert!(!source.contains("\"></span>"), "{source:?}");
}

// ---------------------------------------------------------------------------
// §18 and §19 — boundaries and Unicode
// ---------------------------------------------------------------------------

#[test]
fn r6_g17_every_boundary_keeps_the_caret_coherent() {
    // §18: the caret before, inside and after each construction, under every
    // movement and edit key, stays legal and stays drawn exactly once.
    let fixtures = [
        "um **negrito** aqui\n",
        "um *itálico* aqui\n",
        "um ~~riscado~~ aqui\n",
        "um <u>sublinhado</u> aqui\n",
        "um <span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">colorido</span> aqui\n",
        "um <mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">realce</mark> aqui\n",
        "um [rótulo](https://exemplo.test) aqui\n",
        "um &amp; aqui\n",
        "- [ ] uma tarefa\n",
        "- um item\n",
        "# Um título\n",
    ];
    for source in fixtures {
        for step in 0..10usize {
            let mut screen = Screen::open_sized(source, 60, 16);
            for _ in 0..step {
                screen.key(KeyCode::Right);
            }
            screen.assert_caret_is_sane(&format!("{source:?} at {step}"));
            for code in [
                KeyCode::Left,
                KeyCode::Right,
                KeyCode::Up,
                KeyCode::Down,
                KeyCode::Char('Z'),
                KeyCode::Backspace,
                KeyCode::Delete,
                KeyCode::Enter,
            ] {
                screen.key(code);
                screen.assert_caret_is_sane(&format!("{source:?} at {step} after {code:?}"));
            }
        }
    }
}

#[test]
fn r6_g18_unicode_is_not_broken_by_movement_or_backspace() {
    // §19.
    let source = "família 👨‍👩‍👧‍👦 bandeira 🇧🇷 polegar 👍🏽 café cafe\u{301} 日本語\n";
    let mut screen = Screen::open_sized(source, 80, 16);
    let source = screen.source();
    for index in 0..40 {
        screen.key(KeyCode::Right);
        screen.assert_caret_is_sane(&format!("right {index}"));
    }
    for index in 0..40 {
        screen.key(KeyCode::Left);
        screen.assert_caret_is_sane(&format!("left {index}"));
    }
    assert_eq!(screen.source(), source, "walking wrote nothing");

    // A family is one Backspace, not five.
    let mut screen = Screen::open_sized("a👨‍👩‍👧‍👦b\n", 80, 16);
    screen.key(KeyCode::End);
    screen.key(KeyCode::Left);
    screen.key(KeyCode::Backspace);
    assert_eq!(screen.source().trim_end(), "ab", "one grapheme, whole");

    // And so is a flag.
    let mut screen = Screen::open_sized("a🇧🇷b\n", 80, 16);
    screen.key(KeyCode::End);
    screen.key(KeyCode::Left);
    screen.key(KeyCode::Backspace);
    assert_eq!(screen.source().trim_end(), "ab");
}

#[test]
fn r6_g19_a_wide_grapheme_is_two_columns_and_the_caret_knows_it() {
    let mut screen = Screen::open_sized("日本語 fim\n", 40, 12);
    screen.key(KeyCode::Home);
    let start = screen.frame().caret();
    let after = screen.key(KeyCode::Right).caret();
    assert_eq!(
        after.0,
        start.0 + 2,
        "a full-width grapheme takes two terminal cells"
    );
}

// ---------------------------------------------------------------------------
// §20 and §21 — what stays refused
// ---------------------------------------------------------------------------

#[test]
fn r6_g20_a_fenced_block_is_shown_honestly_and_refuses_every_edit() {
    let source = "antes\n\n```rust\nfn main() {}\n```\n\ndepois\n";
    let mut screen = Screen::open_sized(source, 60, 20);
    let source = screen.source();
    let frame = screen.frame();
    assert!(
        frame.text().contains("fn main()"),
        "the fence is shown as what it is:\n{}",
        frame.text()
    );

    // Walk down through it: the caret may never land inside.
    for index in 0..12 {
        screen.key(KeyCode::Down);
        screen.assert_caret_is_sane(&format!("down {index} past the fence"));
        let offset = screen.visual_offset().expect("a caret");
        let fence = screen.source().find("```").expect("a fence");
        let close = screen.source().rfind("```").expect("a fence") + 3;
        assert!(
            offset <= fence || offset >= close,
            "down {index}: the caret landed inside the fence at {offset}"
        );
    }
    assert_eq!(screen.source(), source, "walking wrote nothing");
}

#[test]
fn r6_g21_angle_brackets_in_a_styled_run_are_still_refused_safely() {
    // §21: the known R5 limitation, preserved deliberately — a refusal that
    // changes no byte and leaves the caret where it was.
    let mut screen = Screen::open("texto\n");
    screen.key(KeyCode::End);
    choose_colour(&mut screen, 2);
    screen.key(KeyCode::Char('a'));
    let source = screen.source();
    let caret = screen.visual_offset();

    for character in ['<', '>', '&'] {
        let frame = screen.key(KeyCode::Char(character));
        assert_eq!(
            screen.source(),
            source,
            "`{character}` changed a byte instead of refusing"
        );
        assert_eq!(screen.visual_offset(), caret, "and the caret stayed put");
        assert!(
            screen.app.notice.contains("Recusado"),
            "the refusal says so: {:?}",
            screen.app.notice
        );
        frame.assert_hides(CANONICAL_MARKUP, "after a refusal");
    }
    screen.assert_caret_is_sane("after three refusals");
}

// ---------------------------------------------------------------------------
// §22 and §23 — undo, redo, save and reopen
// ---------------------------------------------------------------------------

#[test]
fn r6_g22_undo_and_redo_keep_the_caret_and_the_text_coherent() {
    let mut screen = Screen::open(COLOURS);
    let original = screen.source();
    screen.key(KeyCode::End);
    choose_colour(&mut screen, 2);
    screen.type_text("palavra");
    screen.key(KeyCode::Char(' '));
    screen.key(KeyCode::Enter);
    screen.type_text("nova linha");
    let full = screen.source();

    for index in 0..30 {
        screen.ctrl('z');
        screen.assert_caret_is_sane(&format!("undo {index}"));
        screen
            .frame()
            .assert_hides(CANONICAL_MARKUP, &format!("undo {index}"));
    }
    assert_eq!(screen.source(), original, "undo reached the original text");

    for index in 0..30 {
        screen.ctrl('y');
        screen.assert_caret_is_sane(&format!("redo {index}"));
        screen
            .frame()
            .assert_hides(CANONICAL_MARKUP, &format!("redo {index}"));
    }
    assert_eq!(screen.source(), full, "redo reached the edited text again");
}

#[test]
fn r6_g23_saving_and_reopening_shows_the_same_thing() {
    // §23.
    let mut screen = Screen::open(COLOURS);
    screen.key(KeyCode::End);
    choose_colour(&mut screen, 2);
    screen.type_text("guardado");
    screen.key(KeyCode::Enter);
    screen.type_text("segunda");
    let before = screen.frame().text();
    let source = screen.source();

    screen.ctrl('s');
    assert!(
        screen.app.pending_text().is_none(),
        "the save settled: {:?}",
        screen.app.notice
    );
    screen.key(KeyCode::Esc); // to the reader
    screen.key(KeyCode::Esc); // to the list
    let reopened = screen.key(KeyCode::Enter);

    assert_eq!(screen.mode(), EditorMode::Visual, "and reopens in Visual");
    assert_eq!(screen.source(), source, "the same canonical bytes");
    reopened.assert_hides(CANONICAL_MARKUP, "after reopening");
    for word in ["guardado", "segunda"] {
        assert!(
            reopened.text().contains(word),
            "{word:?} came back:\n{}",
            reopened.text()
        );
    }
    assert!(
        before.contains("guardado") && before.contains("segunda"),
        "and it was there before the save too"
    );
}

// ---------------------------------------------------------------------------
// §30 REV 3 — a round trip is not an edit
// ---------------------------------------------------------------------------

#[test]
fn r6_g24_a_mode_round_trip_changes_no_byte_on_hostile_content() {
    let fixtures = [
        CANARY,
        COLOURS,
        "```\nnão tocar\n```\n",
        "<!-- comentário aberto\n",
        "<custom attr='x'>opaco</custom>\n",
        "- [ ] tarefa\n- [x] feita\n",
        "> citação\n>\n> segunda\n",
        "texto com &amp; entidade e 👨‍👩‍👧‍👦\n",
        "",
    ];
    for source in fixtures {
        let mut screen = Screen::open_sized(source, 70, 20);
        let before = screen.source();
        for _ in 0..6 {
            screen.alt('v');
            assert_eq!(
                screen.source(),
                before,
                "switching modes changed {source:?}"
            );
        }
        assert!(
            screen.app.pending_text().is_none(),
            "a round trip created a pending edit on {source:?}"
        );
    }
}
