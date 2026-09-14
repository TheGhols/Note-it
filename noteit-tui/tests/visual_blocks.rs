//! Gate 5.0D.4B.7 — structured blocks.
//!
//! Links, lists, tasks, blockquotes and callouts, each behind its own
//! capability. The rule that governs all of them is that the *structure* is
//! not text: a list's marker, a task's checkbox and a link's destination are
//! attributes of the block or the node, and typing may not reach any of them.
//!
//! The link destination is the sharpest case. `inline.rs` stops it at the
//! first `)`, which §26.16 names as the concrete evidence that the reader's
//! parser cannot be the editor's — so the destination is matched with balanced
//! parentheses, kept whole, and never given a caret.

use noteit_tui::draft::Draft;
use noteit_tui::projection::NodeKind;
use noteit_tui::source_map::{Generation, SourceOffset};
use noteit_tui::visual::{Capabilities, VisualDocument};
use noteit_tui::visual_edit::{plan, Refusal, VisualCommand};

fn with(source: &str, capabilities: Capabilities) -> VisualDocument {
    VisualDocument::project_with(source, Generation::first(), capabilities)
}

fn blocks() -> Capabilities {
    Capabilities::BLOCKS
}

fn visible(document: &VisualDocument) -> String {
    document
        .blocks()
        .iter()
        .flat_map(|block| {
            document
                .graphemes_of(block.id)
                .map(|cell| cell.text.to_owned())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn at(source: &str, needle: &str) -> SourceOffset {
    SourceOffset::in_source(source, source.find(needle).expect("the needle")).expect("a boundary")
}

#[track_caller]
fn applied(source: &str, command: VisualCommand) -> String {
    let mut draft = Draft::new(source);
    let document = VisualDocument::project_with(source, draft.generation(), blocks());
    let transaction = plan(&document, command).expect("the command should be planned");
    draft.apply_transaction(&transaction).expect("applies");
    draft.text()
}

#[track_caller]
fn assert_refused(source: &str, command: VisualCommand) -> Refusal {
    let mut draft = Draft::new(source);
    let before = draft.text();
    let history = draft.history_depth();
    let document = VisualDocument::project_with(source, draft.generation(), blocks());
    let refusal = plan(&document, command).expect_err("this must be refused");
    assert_eq!(draft.text(), before);
    assert_eq!(draft.history_depth(), history);
    assert!(!draft.undo());
    refusal
}

// ---------------------------------------------------------------------------
// Denied before granted
// ---------------------------------------------------------------------------

#[test]
fn structured_blocks_stay_source_until_their_capability_is_granted() {
    for source in [
        "- item de lista",
        "- [ ] uma tarefa",
        "> uma citação",
        "veja [o site](https://example.com)",
    ] {
        assert_eq!(
            visible(&with(source, Capabilities::HTML)),
            source,
            "{source:?} must be shown verbatim before B.7"
        );
    }
}

#[test]
fn each_block_capability_is_independent() {
    let source = "- item\n\n> citação";

    let mut only_list = Capabilities::HTML;
    only_list.list = true;
    let shown = visible(&with(source, only_list));
    assert!(
        !shown.contains("- item"),
        "the list marker is gone: {shown}"
    );
    assert!(
        shown.contains("> citação"),
        "the quote marker stays: {shown}"
    );
}

// ---------------------------------------------------------------------------
// Links
// ---------------------------------------------------------------------------

#[test]
fn a_links_label_is_projected_and_its_destination_is_not() {
    let source = "veja [o site](https://example.com/a_(b)) agora";
    let document = with(source, blocks());

    let shown = visible(&document);
    assert_eq!(
        shown, "veja o site agora",
        "only the label is drawn: {shown}"
    );
    assert!(
        !shown.contains("https://"),
        "the destination never reaches the screen"
    );
}

#[test]
fn a_link_destination_keeps_its_balanced_parentheses() {
    // The case §26.16 names: `inline.rs` stops at the first `)` and would read
    // this destination as `https://example.com/a_(b`.
    let source = "[a](https://example.com/a_(b))";
    let document = with(source, blocks());
    let destination = document
        .projection()
        .lexemes()
        .iter()
        .find(|lexeme| lexeme.kind == noteit_tui::projection::LexemeKind::LinkDestination)
        .expect("a destination");
    assert_eq!(
        destination.source.slice(source),
        "https://example.com/a_(b)",
        "the whole destination, closing parenthesis included"
    );
}

#[test]
fn no_caret_can_be_placed_in_a_link_destination() {
    let source = "[rotulo](https://example.com)";
    let document = with(source, blocks());
    let start = source.find("https").expect("the destination");
    let end = source.len() - 1;

    for byte in start..end {
        if !source.is_char_boundary(byte) {
            continue;
        }
        assert!(
            document
                .slots()
                .iter()
                .all(|slot| slot.source_offset.get() != byte),
            "a caret landed at byte {byte} of the destination"
        );
    }
}

#[test]
fn editing_a_link_label_leaves_its_destination_byte_identical() {
    let source = "[rotulo](https://example.com/a_(b))";
    let result = applied(
        source,
        VisualCommand::Insert {
            at: at(source, "rotulo"),
            text: "X".into(),
        },
    );
    assert_eq!(result, "[Xrotulo](https://example.com/a_(b))");
}

#[test]
fn a_command_aimed_at_a_link_destination_is_refused() {
    let source = "[a](https://example.com)";
    let destination = source.find("https").expect("the destination");
    assert_eq!(
        assert_refused(
            source,
            VisualCommand::Insert {
                at: SourceOffset::in_source(source, destination).unwrap(),
                text: "X".into(),
            },
        ),
        Refusal::ProtectedRegion
    );
}

#[test]
fn a_link_destination_is_never_executed_or_resolved() {
    // Nothing in the projector opens, fetches or normalises a URL: it is a
    // range of bytes with a protection flag. The proof is that a destination
    // which would be alarming anywhere else is treated exactly like any other.
    for source in [
        "[a](javascript:alert(1))",
        "[a](file:///etc/passwd)",
        "[a](https://example.com/../../secret)",
    ] {
        let document = with(source, blocks());
        assert_eq!(
            visible(&document),
            "a",
            "only the label is ever drawn for {source:?}"
        );
        let mut end = 0usize;
        for lexeme in document.projection().lexemes() {
            assert_eq!(lexeme.source.start(), end);
            end = lexeme.source.end();
        }
        assert_eq!(end, source.len(), "and the bytes are preserved exactly");
    }
}

// ---------------------------------------------------------------------------
// Lists and tasks
// ---------------------------------------------------------------------------

#[test]
fn a_list_items_text_is_editable_and_its_marker_is_not() {
    let source = "- item";
    let document = with(source, blocks());
    assert_eq!(visible(&document), "item");

    let offsets: Vec<usize> = document
        .slots()
        .iter()
        .map(|slot| slot.source_offset.get())
        .collect();
    assert!(
        offsets.iter().all(|offset| *offset >= 2),
        "no caret inside `- `: {offsets:?}"
    );

    assert_eq!(
        applied(
            source,
            VisualCommand::Insert {
                at: at(source, "item"),
                text: "X".into()
            }
        ),
        "- Xitem",
        "the marker survived"
    );
}

#[test]
fn a_task_keeps_its_checkbox_and_its_completion_metadata() {
    let source = "- [x] feita <!-- note-it:completed_at=2026-01-01T00:00:00Z -->";
    let document = with(source, blocks());

    // The metadata the Core owns is never drawn and never carries a caret.
    let shown = visible(&document);
    assert!(
        !shown.contains("note-it:"),
        "the Core's own completion marker is not drawn: {shown}"
    );
    assert!(
        !shown.contains("[x]"),
        "nor is the checkbox markup: {shown}"
    );

    let metadata = source.find("<!--").expect("the metadata");
    assert!(
        document
            .slots()
            .iter()
            .all(|slot| slot.source_offset.get() <= metadata),
        "no caret in the completion metadata"
    );
}

#[test]
fn toggling_a_task_is_one_transaction_over_its_checkbox_alone() {
    let source = "- [ ] pendente";
    let result = applied(
        source,
        VisualCommand::ToggleTask {
            at: at(source, "pendente"),
        },
    );
    assert_eq!(result, "- [x] pendente");

    // And back again.
    let source = "- [x] feita";
    assert_eq!(
        applied(
            source,
            VisualCommand::ToggleTask {
                at: at(source, "feita")
            }
        ),
        "- [ ] feita"
    );
}

#[test]
fn toggling_a_task_is_one_undo_step() {
    let source = "- [ ] pendente";
    let mut draft = Draft::new(source);
    let before = draft.history_depth();
    let document = VisualDocument::project_with(source, draft.generation(), blocks());
    let transaction = plan(
        &document,
        VisualCommand::ToggleTask {
            at: at(source, "pendente"),
        },
    )
    .expect("a toggle");

    draft.apply_transaction(&transaction).expect("applies");
    assert_eq!(draft.history_depth(), before + 1);
    assert!(draft.undo());
    assert_eq!(draft.text(), source);
}

#[test]
fn toggling_something_that_is_not_a_task_is_refused() {
    let source = "- apenas uma lista";
    assert_eq!(
        assert_refused(
            source,
            VisualCommand::ToggleTask {
                at: at(source, "apenas")
            },
        ),
        Refusal::MissingCapability
    );
}

// ---------------------------------------------------------------------------
// Blockquotes and callouts
// ---------------------------------------------------------------------------

#[test]
fn a_blockquote_and_a_callout_have_editable_text_and_protected_markers() {
    for (source, expected) in [("> citação", "citação"), ("> [!NOTE]\n> corpo", "[!NOTE]")] {
        let document = with(source, blocks());
        let shown = visible(&document);
        assert!(
            shown.contains(expected),
            "{source:?} should show {expected:?}, got {shown:?}"
        );
        assert!(!shown.starts_with("> "), "the marker is not drawn: {shown}");
    }
}

#[test]
fn a_callout_is_recognised_as_a_callout_and_not_a_quote() {
    let document = with("> [!NOTE]\n> corpo", blocks());
    assert!(document
        .projection()
        .nodes()
        .iter()
        .any(|node| node.kind == NodeKind::Callout));
}

// ---------------------------------------------------------------------------
// What stays refused
// ---------------------------------------------------------------------------

#[test]
fn a_fenced_block_is_still_source_and_still_refuses_everything() {
    let source = "```rust\nfn main() {}\n```\n";
    let document = with(source, blocks());
    assert!(
        visible(&document).contains("fn main()"),
        "the code is shown as code"
    );
    assert!(
        document.slots().is_empty(),
        "and carries no caret, at any gate"
    );
}

#[test]
fn an_unknown_block_stays_opaque_whatever_is_granted() {
    let source = "<custom-widget>x</custom-widget>";
    let document = with(source, blocks());
    assert_eq!(visible(&document), source);
    assert!(document.slots().is_empty());
}

#[test]
fn a_selection_across_a_list_item_boundary_is_refused() {
    let source = "- um\n- dois";
    assert_eq!(
        assert_refused(
            source,
            VisualCommand::ReplaceSelection {
                anchor: at(source, "um"),
                head: SourceOffset::in_source(source, source.len()).unwrap(),
                text: String::new(),
            },
        ),
        Refusal::BlockBoundary
    );
}

// ---------------------------------------------------------------------------
// Losslessness, still
// ---------------------------------------------------------------------------

#[test]
fn every_structured_block_still_projects_losslessly() {
    for source in [
        "- item\n- outro\n",
        "1. um\n2. dois\n",
        "- [ ] a\n- [x] b\n",
        "> citação\n> continua\n",
        "> [!NOTE]\n> corpo\n",
        "[a](https://example.com/a_(b))",
        "- [x] feita <!-- note-it:completed_at=2026-01-01T00:00:00Z -->\n",
        "misto: - não é lista\n\n- é lista\n",
    ] {
        let document = with(source, blocks());
        let mut end = 0usize;
        let mut rebuilt = String::new();
        for lexeme in document.projection().lexemes() {
            assert_eq!(lexeme.source.start(), end, "{source:?}");
            rebuilt.push_str(lexeme.source.slice(source));
            end = lexeme.source.end();
        }
        assert_eq!(end, source.len(), "{source:?}");
        assert_eq!(rebuilt, source, "{source:?}");
    }
}
