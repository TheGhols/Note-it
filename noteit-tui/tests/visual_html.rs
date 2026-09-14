//! Gate 5.0D.4B.6 — canonical HTML formatting.
//!
//! Colour, highlight and underline are the three constructions the graphical
//! editor persists as HTML, and this is where the visual editor learns to read
//! and write them. Underline is here rather than in B.5 because §27.19 moved
//! it: `<u>` is HTML, and editing it needs the close-matching, the ancestor
//! dominance and the EOF protection that the pre-HTML gate exists to clear.
//!
//! The rule that does most of the work is the rewrite envelope (§26.7): an
//! edit may normalise inside the smallest node it must change and nowhere
//! else. "Maximal wrapper" means maximal *within the envelope*, never global,
//! which is why two identical adjacent spans stay two spans.

use noteit_tui::draft::Draft;
use noteit_tui::source_map::{Generation, SourceOffset};
use noteit_tui::visual::{Capabilities, VisualDocument};
use noteit_tui::visual_edit::{plan, Refusal, VisualCommand};

/// The palette entries this project uses, from `formatting.rs`.
const RED: &str = "#DC2626";
const YELLOW: &str = "#FDE68A";

fn html() -> Capabilities {
    Capabilities::HTML
}

fn with(source: &str, capabilities: Capabilities) -> VisualDocument {
    VisualDocument::project_with(source, Generation::first(), capabilities)
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

#[track_caller]
fn applied(source: &str, command: VisualCommand) -> String {
    let mut draft = Draft::new(source);
    let document = VisualDocument::project_with(source, draft.generation(), html());
    let transaction = plan(&document, command).expect("the command should be planned");
    draft.apply_transaction(&transaction).expect("applies");
    draft.text()
}

#[track_caller]
fn assert_refused(source: &str, command: VisualCommand) -> Refusal {
    let mut draft = Draft::new(source);
    let before = draft.text();
    let history = draft.history_depth();
    let document = VisualDocument::project_with(source, draft.generation(), html());
    let refusal = plan(&document, command).expect_err("this must be refused");
    assert_eq!(draft.text(), before, "a refusal changed the source");
    assert_eq!(draft.history_depth(), history);
    assert!(!draft.undo());
    refusal
}

fn at(source: &str, byte: usize) -> SourceOffset {
    SourceOffset::in_source(source, byte).expect("a boundary")
}

// ---------------------------------------------------------------------------
// Reading: the canonical vocabulary becomes meaning
// ---------------------------------------------------------------------------

#[test]
fn canonical_tags_are_hidden_only_once_their_capability_is_granted() {
    let source = "<span data-note-it-color=\"#DC2626\">vermelho</span>";

    // B.5's view: HTML is still source, because B.5 may not edit it.
    assert_eq!(visible(&with(source, Capabilities::INLINE)), source);

    // B.6's view: the colour is the meaning, and the tag is not drawn.
    assert_eq!(visible(&with(source, html())), "vermelho");
}

#[test]
fn each_html_capability_is_independent_of_the_others() {
    let source = "<u>sub</u> e <mark data-note-it-highlight=\"#FDE68A\">marca</mark>";

    let mut only_underline = Capabilities::INLINE;
    only_underline.underline = true;
    let shown = visible(&with(source, only_underline));
    assert!(shown.starts_with("sub e "), "underline is hidden: {shown}");
    assert!(
        shown.contains("data-note-it-highlight"),
        "highlight is not: {shown}"
    );
}

#[test]
fn a_canonical_tag_inside_an_unknown_element_stays_source() {
    // Ancestor dominance beats every capability (§26.15.10).
    let source = "<custom><span data-note-it-color=\"#DC2626\">x</span></custom>";
    assert_eq!(visible(&with(source, html())), source);
    assert!(with(source, html()).slots().is_empty());
}

#[test]
fn an_unclosed_canonical_tag_still_protects_to_eof() {
    // §26.10's rule does not soften because the element is one we understand.
    let source = "<span data-note-it-color=\"#DC2626\">hello\nparagraph";
    let document = with(source, html());
    assert_eq!(visible(&document), source, "shown verbatim");
    assert!(
        document.slots().is_empty(),
        "and nothing after it is editable"
    );
}

#[test]
fn the_colour_of_a_span_is_read_from_its_canonical_attribute() {
    use noteit_tui::projection::NodeKind;
    let source = "<span data-note-it-color=\"#DC2626\">x</span>";
    let document = with(source, html());
    let colour = document
        .projection()
        .nodes()
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::Color(value) => Some(value.clone()),
            _ => None,
        });
    assert_eq!(colour.as_deref(), Some(RED));
}

#[test]
fn editing_inside_a_coloured_span_keeps_its_tags() {
    let source = "<span data-note-it-color=\"#DC2626\">abc</span>";
    let content = source.find("abc").expect("the content");
    assert_eq!(
        applied(
            source,
            VisualCommand::Insert {
                at: at(source, content),
                text: "X".into()
            }
        ),
        "<span data-note-it-color=\"#DC2626\">Xabc</span>"
    );
}

// ---------------------------------------------------------------------------
// Writing: applying a colour or a highlight
// ---------------------------------------------------------------------------

#[test]
fn applying_a_colour_to_plain_text_writes_the_canonical_wrapper() {
    let source = "abc";
    assert_eq!(
        applied(
            source,
            VisualCommand::SetColor {
                anchor: at(source, 0),
                head: at(source, 3),
                color: Some(RED.into()),
            }
        ),
        "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">abc</span>",
        "exactly the spelling the graphical editor writes"
    );
}

#[test]
fn applying_a_highlight_to_plain_text_writes_the_canonical_wrapper() {
    let source = "abc";
    assert_eq!(
        applied(
            source,
            VisualCommand::SetHighlight {
                anchor: at(source, 0),
                head: at(source, 3),
                color: Some(YELLOW.into()),
            }
        ),
        "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">abc</mark>"
    );
}

#[test]
fn applying_underline_writes_the_canonical_tag() {
    let source = "abc";
    assert_eq!(
        applied(
            source,
            VisualCommand::ToggleUnderline {
                anchor: at(source, 0),
                head: at(source, 3),
            }
        ),
        "<u>abc</u>"
    );
}

#[test]
fn applying_to_part_of_a_paragraph_touches_only_that_part() {
    let source = "um abc dois";
    let result = applied(
        source,
        VisualCommand::SetColor {
            anchor: at(source, 3),
            head: at(source, 6),
            color: Some(RED.into()),
        },
    );
    assert!(result.starts_with("um "), "the text before is untouched");
    assert!(result.ends_with(" dois"), "and so is the text after");
    assert!(result.contains(">abc</span>"));
}

// ---------------------------------------------------------------------------
// Clearing
// ---------------------------------------------------------------------------

#[test]
fn clearing_a_colour_removes_exactly_its_own_wrapper() {
    let source = "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">abc</span>";
    let content = source.find("abc").expect("the content");
    assert_eq!(
        applied(
            source,
            VisualCommand::SetColor {
                anchor: at(source, content),
                head: at(source, content + 3),
                color: None,
            }
        ),
        "abc"
    );
}

#[test]
fn clearing_an_underline_removes_exactly_its_own_tag() {
    let source = "<u>abc</u>";
    assert_eq!(
        applied(
            source,
            VisualCommand::ToggleUnderline {
                anchor: at(source, 3),
                head: at(source, 6),
            }
        ),
        "abc"
    );
}

// ---------------------------------------------------------------------------
// The rewrite envelope (§26.7, property 12)
// ---------------------------------------------------------------------------

#[test]
fn an_identical_adjacent_wrapper_is_not_coalesced() {
    // §26.7's own example. "Maximal wrapper" means maximal within the
    // envelope; coalescing across it would rewrite bytes the edit never
    // touched, which is the normalisation risk §22 puts first.
    let source = "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">a</span>\
<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">b</span>";
    let first = source.find(">a<").expect("the first content") + 1;

    let result = applied(
        source,
        VisualCommand::Insert {
            at: at(source, first),
            text: "X".into(),
        },
    );
    assert_eq!(
        result.matches("<span").count(),
        2,
        "the two spans stayed two: {result}"
    );
    assert!(result.contains(">Xa</span>"));
    assert!(result.ends_with(">b</span>"));
}

#[test]
fn the_declared_envelope_contains_every_patch_and_nothing_spare() {
    let source = "antes abc depois";
    let mut draft = Draft::new(source);
    let document = VisualDocument::project_with(source, draft.generation(), html());
    let transaction = plan(
        &document,
        VisualCommand::SetColor {
            anchor: at(source, 6),
            head: at(source, 9),
            color: Some(RED.into()),
        },
    )
    .expect("a colour");

    for patch in &transaction.patches {
        assert!(transaction.envelope.covers(patch.range));
    }
    assert!(
        transaction.envelope.start() >= 6 && transaction.envelope.end() <= 9,
        "the envelope is the selection, not the block: {:?}",
        transaction.envelope
    );

    draft.apply_transaction(&transaction).expect("applies");
    let after = draft.text();
    assert!(after.starts_with("antes "), "bytes before are identical");
    assert!(after.ends_with(" depois"), "bytes after are identical");
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

#[test]
fn formatting_a_selection_that_crosses_a_mark_boundary_is_refused() {
    let source = "x **ab**";
    assert_refused(
        source,
        VisualCommand::SetColor {
            anchor: at(source, 0),
            head: at(source, 5),
            color: Some(RED.into()),
        },
    );
}

#[test]
fn formatting_inside_a_protected_region_is_refused() {
    let source = "<custom>abc</custom>";
    assert_eq!(
        assert_refused(
            source,
            VisualCommand::SetColor {
                anchor: at(source, 8),
                head: at(source, 11),
                color: Some(RED.into()),
            },
        ),
        Refusal::ProtectedRegion
    );
}

#[test]
fn formatting_across_a_block_boundary_is_refused() {
    let source = "ab\n\ncd";
    assert_eq!(
        assert_refused(
            source,
            VisualCommand::SetColor {
                anchor: at(source, 0),
                head: at(source, 6),
                color: Some(RED.into()),
            },
        ),
        Refusal::BlockBoundary
    );
}

#[test]
fn an_empty_selection_formats_nothing() {
    let source = "abc";
    assert_eq!(
        assert_refused(
            source,
            VisualCommand::SetColor {
                anchor: at(source, 1),
                head: at(source, 1),
                color: Some(RED.into()),
            },
        ),
        Refusal::NothingToDo
    );
}

#[test]
fn a_colour_that_is_not_in_the_palette_is_refused() {
    // The spellings that can reach a note are the ones this project writes.
    // Accepting anything else would let a value travel from a keystroke into
    // an HTML attribute unchecked.
    let source = "abc";
    assert_eq!(
        assert_refused(
            source,
            VisualCommand::SetColor {
                anchor: at(source, 0),
                head: at(source, 3),
                color: Some("javascript:alert(1)".into()),
            },
        ),
        Refusal::MissingCapability
    );
    assert_eq!(
        assert_refused(
            source,
            VisualCommand::SetColor {
                anchor: at(source, 0),
                head: at(source, 3),
                color: Some("\"><script>".into()),
            },
        ),
        Refusal::MissingCapability
    );
}

// ---------------------------------------------------------------------------
// Unicode and losslessness still hold
// ---------------------------------------------------------------------------

#[test]
fn applying_a_colour_across_unicode_keeps_every_byte() {
    let source = "ç日👍🏽";
    let result = applied(
        source,
        VisualCommand::SetColor {
            anchor: at(source, 0),
            head: at(source, source.len()),
            color: Some(RED.into()),
        },
    );
    assert!(result.contains("ç日👍🏽"), "the text survived: {result}");
    assert!(result.starts_with("<span"));
    assert!(result.ends_with("</span>"));
}

#[test]
fn the_projection_of_canonical_html_is_still_lossless() {
    for source in [
        "<span data-note-it-color=\"#DC2626\">a</span>",
        "<mark data-note-it-highlight=\"#FDE68A\">b</mark>",
        "<u>c</u>",
        "<mark data-note-it-highlight=\"#FDE68A\"><span data-note-it-color=\"#DC2626\">d</span></mark>",
        "<span data-note-it-color=\"#DC2626\">e</span> e <u>f</u>",
    ] {
        let document = with(source, html());
        let mut end = 0usize;
        let mut rebuilt = String::new();
        for lexeme in document.projection().lexemes() {
            assert_eq!(lexeme.source.start(), end, "{source}");
            rebuilt.push_str(lexeme.source.slice(source));
            end = lexeme.source.end();
        }
        assert_eq!(end, source.len());
        assert_eq!(rebuilt, source, "hiding a tag must not lose a byte");
    }
}

#[test]
fn a_nested_highlight_and_colour_both_project() {
    let source = "<mark data-note-it-highlight=\"#FDE68A\">\
<span data-note-it-color=\"#DC2626\">abc</span></mark>";
    let document = with(source, html());
    assert_eq!(visible(&document), "abc");

    // And the caret inside carries both in its path, innermost last.
    let content = source.find("abc").expect("the content");
    let path = document.mark_path(content);
    assert_eq!(path.len(), 2, "a highlight containing a colour");
}
