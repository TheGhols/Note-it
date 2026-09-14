//! Gate 5.0D.4B.4 — headings and block boundaries.
//!
//! B.4 is where `Enter`, `Backspace` and `Delete` stop being insertions and
//! start being structure. The matrix in `docs/tui.md` §26.9, as completed by
//! §28.2, says what each key does at each position of each construct, and this
//! file is that matrix executed: every cell that grants an operation has a test
//! that performs it, and every cell that refuses has a test that proves nothing
//! moved.
//!
//! The heading rules are the delicate ones. A heading's `# ` is Protected and
//! invisible, so joining a heading backwards would have to consume bytes no
//! caret can reach — and the matrix answers `Refusal` rather than inventing a
//! demotion nobody asked for.

use noteit_tui::draft::Draft;
use noteit_tui::source_map::{Generation, GraphemeIndex, SourceOffset};
use noteit_tui::visual::VisualDocument;
use noteit_tui::visual_edit::{plan, Refusal, VisualCommand};

/// Applies one command and returns the resulting source.
#[track_caller]
fn applied(source: &str, command: VisualCommand) -> String {
    let mut draft = Draft::new(source);
    let document = VisualDocument::project(source, draft.generation());
    let transaction = plan(&document, command).expect("the command should be planned");
    draft.apply_transaction(&transaction).expect("applies");
    draft.text()
}

/// Asserts a command is refused and that nothing observable moved.
#[track_caller]
fn assert_refused(source: &str, command: VisualCommand, expected: Refusal) {
    let mut draft = Draft::new(source);
    let before = draft.text();
    let history = draft.history_depth();
    let cursor = draft.cursor();

    let document = VisualDocument::project(source, draft.generation());
    assert_eq!(plan(&document, command).err(), Some(expected));

    assert_eq!(draft.text(), before);
    assert_eq!(draft.history_depth(), history);
    assert_eq!(draft.cursor(), cursor);
    assert!(!draft.undo());
}

fn offset(source: &str, byte: usize) -> SourceOffset {
    SourceOffset::in_source(source, byte).expect("a boundary")
}

fn split(source: &str, byte: usize) -> VisualCommand {
    VisualCommand::SplitBlock {
        at: offset(source, byte),
    }
}

// ---------------------------------------------------------------------------
// Enter: the paragraph row of the matrix
// ---------------------------------------------------------------------------

#[test]
fn enter_in_the_middle_of_a_paragraph_makes_two_paragraphs() {
    assert_eq!(applied("abcd", split("abcd", 2)), "ab\n\ncd");
}

#[test]
fn enter_at_the_start_of_a_paragraph_puts_an_empty_one_before_it() {
    assert_eq!(applied("abcd", split("abcd", 0)), "\n\nabcd");
}

#[test]
fn enter_at_the_end_of_a_paragraph_puts_an_empty_one_after_it() {
    assert_eq!(applied("abcd", split("abcd", 4)), "abcd\n\n");
}

// ---------------------------------------------------------------------------
// Enter: the heading row
// ---------------------------------------------------------------------------

#[test]
fn enter_in_the_middle_of_a_heading_makes_two_headings_of_the_same_level() {
    // §12 and §26.9: the level is an attribute of the block, and splitting may
    // not quietly lose it.
    assert_eq!(applied("## Meu", split("## Meu", 5)), "## Me\n\n## u");
    assert_eq!(
        applied("###### H", split("###### H", 8)),
        "###### H\n\n",
        "a split at the end is the end rule, not the middle one"
    );
}

#[test]
fn enter_at_the_start_of_a_heading_keeps_the_heading_and_adds_a_paragraph_above() {
    assert_eq!(applied("# T", split("# T", 2)), "\n\n# T");
}

#[test]
fn enter_at_the_end_of_a_heading_adds_a_paragraph_not_another_heading() {
    // The expectation a title sets: what follows a title is text.
    assert_eq!(applied("# T", split("# T", 3)), "# T\n\n");
}

#[test]
fn a_split_never_lands_inside_a_protected_prefix() {
    // Byte 1 is inside `# `, where no caret exists.
    assert_refused("# T", split("# T", 1), Refusal::ProtectedRegion);
}

// ---------------------------------------------------------------------------
// Line endings are not normalised (§28.3, property 18)
// ---------------------------------------------------------------------------

#[test]
fn a_split_uses_the_local_line_ending_convention() {
    // §27.13/§28.3: a new break copies the convention of the current block,
    // then of the block before it, then the document's first, then LF.
    let crlf = "ab\r\n\r\ncd";
    let result = applied(crlf, split(crlf, 7));
    assert!(
        result.contains("\r\n\r\nd"),
        "a CRLF document must get a CRLF break, got {result:?}"
    );
    assert!(!result.contains("\n\n\n"), "no stray LF was introduced");

    // A document with no line ending at all falls through to LF.
    assert_eq!(applied("abcd", split("abcd", 2)), "ab\n\ncd");
}

#[test]
fn splitting_never_normalises_anything_else_in_the_document() {
    let source = "a\r\nb\n\nc   \n\nd";
    let result = applied(source, split(source, 12));
    assert!(result.starts_with("a\r\nb\n\nc   \n\n"), "got {result:?}");
}

// ---------------------------------------------------------------------------
// Join: Backspace at a block start, Delete at a block end
// ---------------------------------------------------------------------------

#[test]
fn backspace_at_the_start_of_a_paragraph_joins_it_to_the_one_above() {
    assert_eq!(
        applied(
            "ab\n\ncd",
            VisualCommand::JoinBackward {
                at: offset("ab\n\ncd", 4)
            }
        ),
        "abcd"
    );
}

#[test]
fn delete_at_the_end_of_a_paragraph_joins_the_one_below() {
    assert_eq!(
        applied(
            "ab\n\ncd",
            VisualCommand::JoinForward {
                at: offset("ab\n\ncd", 2)
            }
        ),
        "abcd"
    );
}

#[test]
fn joining_is_one_transaction_and_one_undo() {
    let source = "ab\n\ncd";
    let mut draft = Draft::new(source);
    let before = draft.history_depth();
    let document = VisualDocument::project(source, draft.generation());
    let transaction = plan(
        &document,
        VisualCommand::JoinBackward {
            at: offset(source, 4),
        },
    )
    .expect("a join");

    draft.apply_transaction(&transaction).expect("applies");
    assert_eq!(draft.text(), "abcd");
    assert_eq!(draft.history_depth(), before + 1);
    assert!(draft.undo());
    assert_eq!(draft.text(), source, "one undo restores the whole join");
}

#[test]
fn backspace_at_the_start_of_a_heading_is_refused() {
    // §28.2: joining backwards would have to consume the Protected `## `, and
    // the matrix refuses rather than demoting the heading to literal text.
    for source in ["para\n\n## H", "# A\n\n## H"] {
        let byte = source.find("H").expect("the heading text");
        assert_refused(
            source,
            VisualCommand::JoinBackward {
                at: offset(source, byte),
            },
            Refusal::BlockBoundary,
        );
    }
}

#[test]
fn delete_at_the_end_of_a_heading_pulls_the_paragraph_into_it() {
    // The one direction §28.2 grants: the level is preserved and it is one
    // transaction.
    assert_eq!(
        applied(
            "# T\n\npara",
            VisualCommand::JoinForward {
                at: offset("# T\n\npara", 3)
            }
        ),
        "# Tpara"
    );
}

#[test]
fn delete_at_the_end_of_a_heading_before_another_heading_is_refused() {
    let source = "# A\n\n## B";
    assert_refused(
        source,
        VisualCommand::JoinForward {
            at: offset(source, 3),
        },
        Refusal::BlockBoundary,
    );
}

#[test]
fn a_boundary_next_to_an_opaque_block_is_refused_in_both_directions() {
    let source = "para\n\n<custom>x</custom>";
    assert_refused(
        source,
        VisualCommand::JoinForward {
            at: offset(source, 4),
        },
        Refusal::BlockBoundary,
    );

    let source = "<custom>x</custom>\n\npara";
    let byte = source.find("para").expect("the paragraph");
    assert_refused(
        source,
        VisualCommand::JoinBackward {
            at: offset(source, byte),
        },
        Refusal::BlockBoundary,
    );
}

#[test]
fn a_boundary_next_to_a_fenced_block_is_refused() {
    let source = "```\nx\n```\n\npara";
    let byte = source.find("para").expect("the paragraph");
    assert_refused(
        source,
        VisualCommand::JoinBackward {
            at: offset(source, byte),
        },
        Refusal::BlockBoundary,
    );
}

// ---------------------------------------------------------------------------
// Document edges are a no-op, not a refusal (§28.2)
// ---------------------------------------------------------------------------

#[test]
fn backspace_at_the_start_of_the_document_does_nothing_at_all() {
    // "no-op: nenhum patch, nenhuma entrada de history, nenhum aviso de
    // recusa". It is distinguishable from a refusal precisely so the interface
    // can stay silent about it.
    let source = "abc";
    let mut draft = Draft::new(source);
    let document = VisualDocument::project(source, draft.generation());
    assert_eq!(
        plan(
            &document,
            VisualCommand::JoinBackward {
                at: offset(source, 0)
            }
        )
        .err(),
        Some(Refusal::NothingToDo)
    );
    assert_eq!(draft.text(), source);
    assert_eq!(draft.history_depth(), 0);
    assert!(!draft.undo());
}

#[test]
fn delete_at_the_end_of_the_document_does_nothing_at_all() {
    let source = "abc";
    let draft = Draft::new(source);
    let document = VisualDocument::project(source, draft.generation());
    assert_eq!(
        plan(
            &document,
            VisualCommand::JoinForward {
                at: offset(source, 3)
            }
        )
        .err(),
        Some(Refusal::NothingToDo)
    );
    assert_eq!(draft.text(), source);
    assert_eq!(draft.history_depth(), 0);
}

// ---------------------------------------------------------------------------
// Empty blocks
// ---------------------------------------------------------------------------

#[test]
fn a_run_of_blank_lines_is_a_separator_and_not_a_block() {
    // The §26.9 matrix has an "empty block" row, and this model does not have
    // an empty block to put in it: a run of blank lines is the separator
    // *between* blocks, so no node covers it and no caret can be placed there.
    // That is recorded as a limitation rather than papered over, and what the
    // model does instead is tested here.
    let source = "ab\n\n\n\ncd";
    let document = VisualDocument::project(source, Generation::first());
    assert_eq!(document.blocks().len(), 2, "two blocks, not three");
    assert!(
        document
            .slots()
            .iter()
            .all(|slot| slot.source_offset.get() != 4),
        "the blank gap holds no caret"
    );
}

#[test]
fn joining_across_a_wide_gap_removes_the_whole_gap() {
    // The join is defined by the two blocks it unites, so however many blank
    // lines separate them, all of them go and none is left stranded.
    let source = "ab\n\n\n\ncd";
    let byte = source.find("cd").expect("the second block");
    assert_eq!(
        applied(
            source,
            VisualCommand::JoinBackward {
                at: offset(source, byte)
            }
        ),
        "abcd"
    );
}

// ---------------------------------------------------------------------------
// Capabilities absent from B.4 stay denied (property 27)
// ---------------------------------------------------------------------------

#[test]
fn b4_grants_no_formatting_of_any_kind() {
    let source = "# T";
    assert_refused(
        source,
        VisualCommand::ToggleStrong {
            anchor: offset(source, 2),
            head: offset(source, 3),
        },
        Refusal::MissingCapability,
    );
}

#[test]
fn a_split_inside_an_opaque_region_is_refused() {
    let source = "<custom>hello</custom>";
    assert_refused(source, split(source, 10), Refusal::ProtectedRegion);
}

#[test]
fn a_stale_split_is_refused_by_the_draft() {
    let source = "abcd";
    let mut draft = Draft::new(source);
    let document = VisualDocument::project(source, draft.generation());
    let transaction = plan(&document, split(source, 2)).expect("a split");

    draft.insert_char('!');
    let text = draft.text();
    assert_eq!(
        draft.apply_transaction(&transaction),
        Err(Refusal::StaleGeneration)
    );
    assert_eq!(draft.text(), text);
}

// ---------------------------------------------------------------------------
// Unicode at boundaries
// ---------------------------------------------------------------------------

#[test]
fn a_split_never_cuts_a_grapheme() {
    let source = "a👨\u{200D}👩\u{200D}👧\u{200D}👦b";
    let document = VisualDocument::project(source, Generation::first());
    let block = document.blocks()[0].id;

    // Splitting at each grapheme boundary keeps the clusters whole.
    for index in 0..=document.grapheme_count(block) {
        let cells: Vec<_> = document.graphemes_of(block).collect();
        let byte = cells
            .get(index)
            .map_or(source.len(), |cell| cell.source.start());
        let result = applied(source, split(source, byte));
        assert_eq!(
            result.replace("\n", ""),
            source,
            "splitting at grapheme {index} changed the text"
        );
    }
}

#[test]
fn a_join_across_unicode_keeps_every_byte() {
    let source = "ç日\n\n👍🏽fim";
    let byte = source.find("👍").expect("the emoji");
    let result = applied(
        source,
        VisualCommand::JoinBackward {
            at: offset(source, byte),
        },
    );
    assert_eq!(result, "ç日👍🏽fim");
}

#[test]
fn a_grapheme_index_at_a_boundary_is_still_a_grapheme_index() {
    // A regression guard for the unit confusion this whole layer exists to
    // prevent: the command carries a grapheme index, never a byte.
    let source = "日本";
    let document = VisualDocument::project(source, Generation::first());
    let block = document.blocks()[0].id;
    let transaction = plan(
        &document,
        VisualCommand::DeleteForward {
            block,
            grapheme: GraphemeIndex(1),
        },
    )
    .expect("a delete");
    assert_eq!(
        transaction.patches[0].range.start(),
        3,
        "byte 3, grapheme 1"
    );
}
