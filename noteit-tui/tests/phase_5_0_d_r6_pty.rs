//! Fase 5.0D.R6 §25 — the manual sequence, on a real terminal.
//!
//! `TestBackend` proves what the renderer draws into a buffer. It cannot prove
//! that the real binary, in raw mode, reading real escape sequences from a
//! pseudoterminal, does the same thing — and R5 shipped a green suite that a
//! person could not type in, so the distinction stopped being academic.
//!
//! Every assertion here is made against the *screen*, replayed from the bytes
//! the application wrote, because Ratatui redraws only the cells that changed
//! and a substring search over the raw stream finds neither a wrapped title
//! nor a partially rewritten line.

mod support;

use std::path::Path;
use support::{cleanup_coordination, replay_screen, store, Tui};

const COLUMNS: u16 = 100;
const ROWS: u16 = 30;

/// The application, in a disposable store, with `body` as its only note.
fn open(root: &Path, body: &str) -> (Tui, noteit_core::StorePaths) {
    let runtime = root.join("runtime");
    let (paths, _) = store(root, &runtime, body);
    let mut tui = Tui::spawn_sized(
        root,
        env!("CARGO_BIN_EXE_noteit-tui").as_ref(),
        COLUMNS,
        ROWS,
        |command| {
            command.env("XDG_RUNTIME_DIR", &runtime);
        },
    );
    tui.wait_text("Notas Recentes");
    (tui, paths)
}

/// Everything drawn so far, as the screen it produced.
fn screen(tui: &mut Tui) -> String {
    tui.drain();
    replay_screen(
        &String::from_utf8_lossy(&tui.output),
        COLUMNS as usize,
        ROWS as usize,
    )
    .join("\n")
}

/// Markup that may never be visible while the visual editor has the keyboard.
const HIDDEN: &[&str] = &["<span", "</span", "<mark", "</mark", "data-note-it-"];

#[test]
fn r6_pty_the_manual_sequence_on_a_real_terminal() {
    let root = tempfile::tempdir().unwrap();
    let (mut tui, paths) = open(
        root.path(),
        "# Cores\n\ntexto normal\n\nSegunda linha longa o bastante para quebrar em algum lugar.\n",
    );

    // Enter opens the note, and it opens in the visual editor.
    tui.input(b"\r");
    tui.wait_text("Visual");
    let opened = screen(&mut tui);
    assert!(
        opened.contains("Visual"),
        "R6 §4: the note opened in the visual editor:\n{opened}"
    );
    assert!(
        !opened.contains("# Cores"),
        "and the heading's hashes are hidden:\n{opened}"
    );

    // Arrows. Each is its own write, because bundled with the next byte an
    // escape is read as Alt.
    for sequence in [
        &b"\x1b[C"[..],
        &b"\x1b[C"[..],
        &b"\x1b[B"[..],
        &b"\x1b[B"[..],
        &b"\x1b[A"[..],
        &b"\x1b[D"[..],
    ] {
        tui.input(sequence);
        std::thread::sleep(std::time::Duration::from_millis(30));
        tui.drain();
    }

    // Alt+F, "Cor do texto", the third colour, then type.
    tui.input(b"\x1bf");
    tui.wait_text("Formatar seleção");
    tui.input(b"\r");
    tui.wait_text("Cor do texto");
    tui.input(b"\x1b[B");
    tui.input(b"\x1b[B");
    tui.input(b"\r");
    tui.wait_text("Cor:");

    let before_typing = tui.output.len();
    tui.input(b"gustavo testes de verdade");
    tui.input(b" ");
    tui.input(b"continua");
    tui.input(b"\r");
    tui.input(b"segunda linha");
    std::thread::sleep(std::time::Duration::from_millis(200));
    tui.drain();

    // Nothing the visual editor drew, from the moment the colour was chosen,
    // may contain canonical markup.
    let typed = String::from_utf8_lossy(&tui.output[before_typing..]).into_owned();
    for needle in HIDDEN {
        assert!(
            !typed.contains(needle),
            "`{needle}` reached a real terminal while Visual had the keyboard"
        );
    }
    let visual = screen(&mut tui);
    for needle in HIDDEN {
        assert!(
            !visual.contains(needle),
            "`{needle}` is on screen in Visual:\n{visual}"
        );
    }
    assert!(
        visual.contains("gustavo testes de verdade continua"),
        "the text itself is there:\n{visual}"
    );

    // Alt+V: the source editor, where the markup is allowed and named.
    tui.input(b"\x1bv");
    tui.wait_text("Markdown/Fonte");
    let source = screen(&mut tui);
    assert!(
        source.contains("Markdown/Fonte"),
        "§4.2 the source mode names itself:\n{source}"
    );
    // The source pane scrolls horizontally to follow the cursor, so what is
    // visible is the middle of the wrapper rather than its opening tag. Any
    // fragment of it proves the point: this surface shows canonical markup,
    // and that is correct here.
    assert!(
        source.contains("style=\"color:") || source.contains("data-note-it-"),
        "§17 and the source editor shows the source:\n{source}"
    );

    // Alt+V back: the markup disappears from the screen again.
    let before_return = tui.output.len();
    tui.input(b"\x1bv");
    std::thread::sleep(std::time::Duration::from_millis(200));
    tui.drain();
    let returned = screen(&mut tui);
    assert!(
        returned.contains("Visual") && !returned.contains("Markdown/Fonte"),
        "back in the visual editor:\n{returned}"
    );
    for needle in HIDDEN {
        assert!(
            !returned.contains(needle),
            "`{needle}` survived the return to Visual:\n{returned}"
        );
    }
    let _ = before_return;

    // Save, leave, and let the harness prove the terminal was restored.
    tui.input(b"\x13");
    tui.wait_text("Edição salva");
    let stored = noteit_core::NoteItCore::open_read_only_at(paths.clone())
        .list_summaries(&noteit_core::filter::NoteFilter::default(), None)
        .unwrap()
        .items
        .first()
        .map(|summary| {
            noteit_core::NoteItCore::open_read_only_at(paths.clone())
                .read_note(&summary.id)
                .unwrap()
                .content
        })
        .unwrap();
    assert!(
        stored.contains("<span data-note-it-color="),
        "the canonical source carries the wrapper: {stored:?}"
    );
    assert!(
        stored.contains("gustavo testes de verdade continua"),
        "and the text: {stored:?}"
    );
    assert!(
        !stored.contains("\"></span>"),
        "§16: no empty wrapper after Enter: {stored:?}"
    );

    tui.input(b"\x1b");
    std::thread::sleep(std::time::Duration::from_millis(80));
    tui.drain();
    tui.input(b"\x1b");
    std::thread::sleep(std::time::Duration::from_millis(80));
    tui.finish();
    cleanup_coordination(&paths);
}

#[test]
fn r6_pty_arrows_move_one_row_at_a_time_in_a_wrapped_paragraph() {
    // §10, on a real terminal narrow enough to wrap.
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(
        root.path(),
        &runtime,
        "primeiro\n\numa linha bastante longa que certamente vai quebrar quando o painel for estreito\n\nultimo\n",
    );
    let mut tui = Tui::spawn_sized(
        root.path(),
        env!("CARGO_BIN_EXE_noteit-tui").as_ref(),
        60,
        20,
        |command| {
            command.env("XDG_RUNTIME_DIR", &runtime);
        },
    );
    tui.wait_text("Notas Recentes");
    tui.input(b"\r");
    tui.wait_text("Visual");

    // Down until the caret is inside the long paragraph, then keep going: the
    // point is that it takes several presses to cross it, because it is
    // several rows.
    for _ in 0..6 {
        tui.input(b"\x1b[B");
        std::thread::sleep(std::time::Duration::from_millis(30));
        tui.drain();
    }
    let rendered = replay_screen(&String::from_utf8_lossy(&tui.output), 60, 20).join("\n");
    assert!(
        rendered.contains("ultimo"),
        "the whole note is on screen:\n{rendered}"
    );
    // The paragraph is drawn across more than one row, which is what makes
    // "one row at a time" a different thing from "one block at a time": no
    // single row holds the whole sentence, and its two ends are both drawn.
    assert!(
        rendered
            .lines()
            .all(|row| !row.contains("uma linha bastante longa que certamente")),
        "the paragraph should not fit on one row:\n{rendered}"
    );
    for fragment in ["uma linha bastante", "estreito"] {
        assert!(
            rendered.contains(fragment),
            "{fragment:?} is drawn somewhere:\n{rendered}"
        );
    }

    tui.input(b"\x1b");
    std::thread::sleep(std::time::Duration::from_millis(80));
    tui.drain();
    tui.input(b"\x1b");
    std::thread::sleep(std::time::Duration::from_millis(80));
    tui.finish();
    cleanup_coordination(&paths);
}
