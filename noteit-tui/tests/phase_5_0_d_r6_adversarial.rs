//! Fase 5.0D.R6 §30 — four independent attempts to break what R6 fixed.
//!
//! Each section is written to *fail*: it looks for the shape of the defect
//! rather than for the shape of the fix. A review that only re-runs the
//! canaries proves the canaries.

mod support;

use crossterm::event::KeyCode;
use noteit_core::{
    authority::perform_at,
    write::{NoteMutation, WriteOperation},
};
use noteit_tui::app::EditorMode;
use support::{Screen, CANONICAL_MARKUP};

/// Notes chosen to be awkward rather than representative.
const HOSTILE: &[&str] = &[
    "",
    "\n",
    "   \n\n  \n",
    "uma linha só",
    "# Título\n",
    "```\nsem fechar\n",
    "```rust\nfn main() {}\n```\n",
    "<!-- comentário sem fim\n",
    "<custom attr='x'>opaco</custom>\n",
    "- [ ] tarefa\n- [x] feita\n",
    "> citação\n>\n> segunda\n",
    "**negrito** *itálico* ~~riscado~~ <u>sub</u>\n",
    "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">já colorido</span>\n",
    "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">já realçado</mark>\n",
    "família 👨‍👩‍👧‍👦 bandeira 🇧🇷 polegar 👍🏽 日本語 cafe\u{301}\n",
    "a&amp;b &lt;c&gt; d\n",
    "[rótulo](https://exemplo.test)\n",
    "linha\nsoft break\n\noutro bloco\n",
    "parágrafo muito longo sem quebra nenhuma que vai ultrapassar qualquer largura razoável de terminal e continuar indo\n",
];

/// The smallest pane the interface agrees to draw at all.
///
/// Below 35x6 `ui::render` draws "Note-it (muito pequeno)" and nothing else,
/// which is a deliberate guard and not a caret defect — so the sizes exercised
/// here start at it.
/// The smallest pane the editor actually draws a row into.
///
/// The guard above passes at 35x6, but the header, the footer and the pane's
/// own border take every row of it, so the editor gets none. That is the
/// guard's edge rather than a caret defect, and it has its own test.
const SMALLEST: (u16, u16) = (35, 10);

/// Every key a person can press in the editor, including the ones that edit.
const EVERY_KEY: &[KeyCode] = &[
    KeyCode::Left,
    KeyCode::Right,
    KeyCode::Up,
    KeyCode::Down,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::Char('a'),
    KeyCode::Char(' '),
    KeyCode::Enter,
    KeyCode::Backspace,
    KeyCode::Delete,
    KeyCode::Tab,
];

// ---------------------------------------------------------------------------
// REV 1 — the cursor
// ---------------------------------------------------------------------------

#[test]
fn rev1_no_key_on_any_hostile_note_at_any_width_loses_the_caret() {
    // Stick, vanish, jump, leave the viewport, or stop agreeing with where the
    // next character lands — all five are "the caret is not drawn exactly once
    // somewhere legal", which is what this asserts after every key.
    for source in HOSTILE {
        for (width, height) in [(140u16, 30u16), (80, 24), (40, 12), SMALLEST, (36, 11)] {
            let mut screen = Screen::open_sized(source, width, height);
            if screen.mode() != EditorMode::Visual {
                // A note the visual editor cannot put a caret in stays in the
                // source editor, and that is the contract, not a failure.
                continue;
            }
            screen.assert_caret_is_sane(&format!("{source:?} {width}x{height} open"));
            for (index, key) in EVERY_KEY.iter().enumerate() {
                screen.key(*key);
                screen.assert_caret_is_sane(&format!(
                    "{source:?} {width}x{height} key {index} {key:?}"
                ));
            }
        }
    }
}

#[test]
fn rev1_resizing_between_every_keystroke_never_loses_the_caret() {
    // The caret is a source offset, so a resize may move where it is drawn and
    // may never move it in the text.
    let widths = [140u16, 35, 80, 37, 60, 140, 43];
    for source in HOSTILE {
        let mut screen = Screen::open_sized(source, 80, 20);
        if screen.mode() != EditorMode::Visual {
            continue;
        }
        for (index, key) in EVERY_KEY.iter().enumerate() {
            screen.key(*key);
            let offset = screen.visual_offset();
            for width in widths {
                screen.resize(width, 20);
                assert_eq!(
                    screen.visual_offset(),
                    offset,
                    "{source:?}: resizing to {width} after key {index} moved the caret"
                );
                screen.assert_caret_is_sane(&format!("{source:?} key {index} at {width}"));
            }
            screen.resize(80, 20);
        }
    }
}

#[test]
fn rev1_the_caret_and_the_next_character_never_disagree() {
    // The strongest statement of "the cursor is where typing happens": walk to
    // an arbitrary place, note the caret, type, and find the character there.
    for source in HOSTILE {
        for width in [140u16, 60, 44, 35] {
            for steps in [0usize, 1, 3, 7, 13] {
                let mut screen = Screen::open_sized(source, width, 16);
                if screen.mode() != EditorMode::Visual {
                    continue;
                }
                for _ in 0..steps {
                    screen.key(KeyCode::Right);
                }
                for _ in 0..(steps % 4) {
                    screen.key(KeyCode::Down);
                }
                screen.assert_types_at_caret('Ω', &format!("{source:?} {width} {steps}"));
            }
        }
    }
}

#[test]
fn rev1_holding_a_direction_down_stops_at_the_edge_instead_of_wandering() {
    for source in HOSTILE {
        let mut screen = Screen::open_sized(source, 50, 14);
        if screen.mode() != EditorMode::Visual {
            continue;
        }
        for _ in 0..200 {
            screen.key(KeyCode::Down);
        }
        let bottom = screen.visual_offset();
        for _ in 0..20 {
            screen.key(KeyCode::Down);
        }
        assert_eq!(
            screen.visual_offset(),
            bottom,
            "{source:?}: the bottom held"
        );
        screen.assert_caret_is_sane(&format!("{source:?} bottom"));

        for _ in 0..200 {
            screen.key(KeyCode::Up);
        }
        let top = screen.visual_offset();
        for _ in 0..20 {
            screen.key(KeyCode::Up);
        }
        assert_eq!(screen.visual_offset(), top, "{source:?}: the top held");
        screen.assert_caret_is_sane(&format!("{source:?} top"));
    }
}

// ---------------------------------------------------------------------------
// REV 2 — the markup
// ---------------------------------------------------------------------------

fn arm_colour(screen: &mut Screen) {
    screen.alt('f');
    screen.key(KeyCode::Enter);
    screen.key(KeyCode::Down);
    screen.key(KeyCode::Down);
    screen.key(KeyCode::Enter);
}

fn arm_highlight(screen: &mut Screen) {
    screen.alt('f');
    screen.key(KeyCode::Down);
    screen.key(KeyCode::Enter);
    screen.key(KeyCode::Down);
    screen.key(KeyCode::Enter);
}

#[test]
fn rev2_no_sequence_of_styled_keys_puts_a_tag_on_the_visual_screen() {
    // Space, Enter, selection, undo, redo, resize and Alt+V, in every order
    // this can reach, after every one of which the visual screen is checked.
    let sequences: &[&[KeyCode]] = &[
        &[KeyCode::Char(' '), KeyCode::Enter, KeyCode::Char('a')],
        &[KeyCode::Enter, KeyCode::Enter, KeyCode::Char('a')],
        &[KeyCode::Char('a'), KeyCode::Backspace, KeyCode::Char('b')],
        &[KeyCode::Char('a'), KeyCode::Enter, KeyCode::Backspace],
        &[KeyCode::Enter, KeyCode::Char(' '), KeyCode::Delete],
        &[KeyCode::Char('a'), KeyCode::Home, KeyCode::Char('b')],
        &[KeyCode::Char('a'), KeyCode::Up, KeyCode::Char('b')],
    ];
    for arm in [0usize, 1, 2] {
        for sequence in sequences {
            for width in [120u16, 44, 36] {
                let mut screen = Screen::open_sized("base\n\nsegunda\n", width, 16);
                screen.key(KeyCode::End);
                if arm != 1 {
                    arm_colour(&mut screen);
                }
                if arm != 0 {
                    arm_highlight(&mut screen);
                }
                for (index, key) in sequence.iter().enumerate() {
                    let frame = screen.key(*key);
                    frame.assert_hides(
                        CANONICAL_MARKUP,
                        &format!("arm {arm} seq {sequence:?} key {index} at {width}"),
                    );
                }
                // Undo and redo the whole run, checking every frame.
                for step in 0..8 {
                    screen
                        .ctrl('z')
                        .assert_hides(CANONICAL_MARKUP, &format!("undo {step}"));
                }
                for step in 0..8 {
                    screen
                        .ctrl('y')
                        .assert_hides(CANONICAL_MARKUP, &format!("redo {step}"));
                }
                // And a resize, which reprojects nothing but redraws
                // everything.
                for other in [140u16, 20, 70] {
                    screen.resize(other, 16);
                    screen
                        .frame()
                        .assert_hides(CANONICAL_MARKUP, &format!("resize {other}"));
                }
            }
        }
    }
}

#[test]
fn rev2_a_note_that_already_has_markup_never_shows_it_in_visual() {
    for source in [
        "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">vermelho</span> normal\n",
        "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">amarelo</mark> normal\n",
        "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\"><span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">ambos</span></mark>\n",
    ] {
        for width in [120u16, 52, 40, 35] {
            let mut screen = Screen::open_sized(source, width, 16);
            assert_eq!(screen.mode(), EditorMode::Visual, "{source:?}");
            screen
                .frame()
                .assert_hides(CANONICAL_MARKUP, &format!("{source:?} at {width}"));
            for key in EVERY_KEY {
                screen
                    .key(*key)
                    .assert_hides(CANONICAL_MARKUP, &format!("{source:?} {key:?} at {width}"));
            }
        }
    }
}

#[test]
fn rev2_a_visual_selection_coloured_and_then_undone_shows_no_tag() {
    let mut screen = Screen::open("uma frase inteira aqui\n");
    screen.key(KeyCode::Home);
    for _ in 0..9 {
        screen.shift(KeyCode::Right);
    }
    arm_colour(&mut screen);
    screen.frame().assert_hides(CANONICAL_MARKUP, "coloured");
    assert!(
        screen.source().contains("data-note-it-color"),
        "the selection was coloured: {:?}",
        screen.source()
    );
    for step in 0..4 {
        screen
            .ctrl('z')
            .assert_hides(CANONICAL_MARKUP, &format!("undo {step}"));
    }
    for step in 0..4 {
        screen
            .ctrl('y')
            .assert_hides(CANONICAL_MARKUP, &format!("redo {step}"));
    }
}

// ---------------------------------------------------------------------------
// REV 3 — lossless
// ---------------------------------------------------------------------------

#[test]
fn rev3_a_round_trip_without_an_edit_changes_not_one_byte() {
    for source in HOSTILE {
        for width in [140u16, 80, 40, 35] {
            let mut screen = Screen::open_sized(source, width, 16);
            let before = screen.source();
            let start = screen.mode();
            for round in 0..8 {
                screen.alt('v');
                assert_eq!(
                    screen.source(),
                    before,
                    "{source:?} at {width}: round {round} changed a byte"
                );
                assert!(
                    screen.app.pending_text().is_none(),
                    "{source:?} at {width}: round {round} created a pending edit"
                );
            }
            // Eight is even, so it ends where it began.
            assert_eq!(screen.mode(), start, "{source:?}: the mode came back");
        }
    }
}

#[test]
fn rev3_a_round_trip_after_an_edit_still_changes_nothing_of_its_own() {
    for source in HOSTILE {
        let mut screen = Screen::open_sized(source, 70, 16);
        if screen.mode() != EditorMode::Visual {
            continue;
        }
        screen.key(KeyCode::End);
        screen.key(KeyCode::Char('Z'));
        let after_edit = screen.source();
        for round in 0..6 {
            screen.alt('v');
            assert_eq!(
                screen.source(),
                after_edit,
                "{source:?}: round {round} after an edit changed a byte"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// REV 4 — concurrency and safety
// ---------------------------------------------------------------------------

#[test]
fn rev4_an_external_write_still_fails_closed_from_the_visual_editor() {
    let mut screen = Screen::open("original\n");
    assert_eq!(screen.mode(), EditorMode::Visual);
    screen.key(KeyCode::End);
    screen.type_text(" da TUI");

    // Somebody else writes, exactly as the desktop or the CLI would.
    let paths = screen.app.paths.clone();
    let id = screen.app.current_note.as_ref().unwrap().id;
    let core = noteit_core::NoteItCore::open_read_only_at(paths.clone());
    let document = core.read_note(&id).unwrap();
    perform_at(
        &paths,
        &WriteOperation::MutateNote {
            selector: id.to_string(),
            expected_revision: Some(noteit_core::write::revision_of(&document).unwrap()),
            mutation: NoteMutation::ReplaceBody {
                body: "outro autor".into(),
            },
        },
    )
    .unwrap();

    screen.ctrl('s');
    let stored = noteit_core::NoteItCore::open_read_only_at(paths.clone())
        .read_note(&id)
        .unwrap()
        .content;
    assert!(
        stored.contains("outro autor") && !stored.contains("da TUI"),
        "fail-closed: the other author's bytes are intact: {stored:?}"
    );
    assert!(
        screen.app.editor_prompt.is_some(),
        "and the editor asked a question instead of overwriting"
    );

    // Rereading leaves an editor somebody can still type in.
    screen.key(KeyCode::Char('r'));
    screen.assert_caret_is_sane("after rereading");
    screen.key(KeyCode::End);
    screen.key(KeyCode::Char('!'));
    screen.ctrl('s');
    let stored = noteit_core::NoteItCore::open_read_only_at(paths)
        .read_note(&id)
        .unwrap()
        .content;
    assert!(stored.contains("outro autor!"), "{stored:?}");
}

#[test]
fn rev4_a_note_that_is_entirely_protected_does_not_open_in_the_visual_editor() {
    // §20 and §44.1: a note the visual editor has nowhere to put a caret in
    // stays in Markdown/Fonte, where editing the source is what the surface is
    // for. Opening it into a mode that refuses every key would be worse.
    for source in [
        "```rust\nfn main() {}\n```\n",
        "<!-- comentário sem fim\n",
        "<custom attr='x'>opaco</custom>\n",
    ] {
        let mut screen = Screen::open_sized(source, 60, 16);
        assert_eq!(
            screen.mode(),
            EditorMode::Markdown,
            "{source:?} has no visual caret, so it opens in the source editor"
        );
        // "Fonte" rather than "Markdown/Fonte": at this width the title has
        // to shorten, and shortening is what §44.1 says it does.
        assert!(
            screen.frame().text().contains("Fonte"),
            "and says so: {source:?}\n{}",
            screen.frame().text()
        );

        // Forced into Visual with Alt+V, every key is refused and says why.
        screen.alt('v');
        assert_eq!(screen.mode(), EditorMode::Visual);
        let before = screen.source();
        for key in EVERY_KEY {
            screen.key(*key);
        }
        assert_eq!(
            screen.source(),
            before,
            "{source:?}: the visual editor changed a protected note"
        );
        assert!(
            screen.app.notice.contains("Nada editável"),
            "{source:?}: and it explained itself: {:?}",
            screen.app.notice
        );
    }
}

#[test]
fn rev4_typing_beside_a_fence_never_breaks_the_fence() {
    // The dangerous shape: a protected block surrounded by text somebody *can*
    // edit. A character landing immediately after the closing fence would turn
    // ``` into ```a, which stops being a closing fence and swallows the rest of
    // the note.
    let source = "antes\n\n```rust\nfn main() {}\n```\n\ndepois\n";
    for steps in 0..24usize {
        let mut screen = Screen::open_sized(source, 60, 20);
        assert_eq!(screen.mode(), EditorMode::Visual, "this note has carets");
        for _ in 0..steps {
            screen.key(KeyCode::Right);
        }
        screen.key(KeyCode::Char('Z'));
        let after = screen.source();
        assert!(
            after.contains("```rust\nfn main() {}\n```"),
            "step {steps}: the fence was broken: {after:?}"
        );
        // Every fence line still holds nothing but its own backticks.
        for line in after.lines().filter(|line| line.starts_with("```")) {
            assert!(
                line == "```" || line == "```rust",
                "step {steps}: `{line}` is no longer a fence line: {after:?}"
            );
        }
    }
}

#[test]
fn rev4_a_pane_too_small_to_draw_is_a_message_and_not_a_panic() {
    let mut screen = Screen::open_sized("uma nota\n", 34, 5);
    let frame = screen.frame();
    assert!(
        frame.text().contains("muito pequeno"),
        "the guard drew its message:\n{}",
        frame.text()
    );
    // And growing back to a real size brings the editor and its caret back.
    screen.resize(80, 24);
    screen.assert_caret_is_sane("after growing back");
}

#[test]
fn rev4_the_store_under_test_is_the_only_one_touched() {
    // The harness builds its own store in a temporary directory, and the
    // application resolves everything from the paths it was given. This asserts
    // the property rather than trusting it.
    let screen = Screen::open("nota\n");
    let root = screen.store_root().to_path_buf();
    for path in [
        &screen.app.paths.data_dir,
        &screen.app.paths.notes_dir,
        &screen.app.paths.trash_dir,
        &screen.app.paths.backups_dir,
        &screen.app.paths.assets_dir,
        &screen.app.paths.config_dir,
        &screen.app.paths.state_dir,
        &screen.app.paths.runtime_dir,
    ] {
        assert!(
            path.starts_with(&root),
            "{} escaped the test root {}",
            path.display(),
            root.display()
        );
    }
}
