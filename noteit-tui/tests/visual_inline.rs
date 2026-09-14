//! Gate 5.0D.4B.5 — inline Markdown capabilities, one at a time.
//!
//! §26.14 is explicit that Strong, Emphasis, Strike and InlineCode are enabled
//! **individually** — "nenhum é liberado em lote" — so every capability here
//! gets the same four proofs: that it is denied before it is granted, that
//! granting it hides its own delimiters and nobody else's, that its content
//! becomes editable, and that a selection crossing half of it is still refused.
//!
//! Underline is not here. It is canonical HTML, and §27.19 moved it to B.6
//! behind the pre-HTML gate: enabling it in B.5 would be editing HTML one gate
//! before the gate that authorises editing HTML.

use noteit_tui::draft::Draft;
use noteit_tui::source_map::{Generation, SourceOffset};
use noteit_tui::visual::{Capabilities, VisualDocument};
use noteit_tui::visual_edit::{plan, Refusal, VisualCommand};

fn with(source: &str, capabilities: Capabilities) -> VisualDocument {
    VisualDocument::project_with(source, Generation::first(), capabilities)
}

/// Everything the reader sees, as one string.
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

fn only(capability: &str) -> Capabilities {
    let mut capabilities = Capabilities::NONE;
    match capability {
        "strong" => capabilities.strong = true,
        "emphasis" => capabilities.emphasis = true,
        "strike" => capabilities.strike = true,
        "inline_code" => capabilities.inline_code = true,
        other => panic!("unknown capability {other}"),
    }
    capabilities
}

// ---------------------------------------------------------------------------
// Denied before granted (property 27)
// ---------------------------------------------------------------------------

#[test]
fn every_mark_shows_its_delimiters_until_its_own_capability_is_granted() {
    // B.4's view: the source is shown exactly, because nothing inline is
    // editable and hiding syntax would claim otherwise.
    for source in ["**abc**", "*abc*", "~~abc~~", "`abc`"] {
        let document = with(source, Capabilities::NONE);
        assert_eq!(
            visible(&document),
            source,
            "{source:?} must be shown verbatim before its gate"
        );
    }
}

#[test]
fn granting_one_capability_hides_only_its_own_delimiters() {
    // The point of enabling them one at a time: turning Strong on must not
    // quietly turn Emphasis on as well.
    let source = "**forte** e *ênfase*";

    let document = with(source, only("strong"));
    assert_eq!(
        visible(&document),
        "forte e *ênfase*",
        "Strong's asterisks go, Emphasis's stay"
    );

    let document = with(source, only("emphasis"));
    assert_eq!(
        visible(&document),
        "**forte** e ênfase",
        "Emphasis's asterisks go, Strong's stay"
    );

    let document = with(source, Capabilities::INLINE);
    assert_eq!(visible(&document), "forte e ênfase");
}

#[test]
fn strike_and_inline_code_are_independent_too() {
    let source = "~~riscado~~ e `código`";

    assert_eq!(visible(&with(source, only("strike"))), "riscado e `código`");
    assert_eq!(
        visible(&with(source, only("inline_code"))),
        "~~riscado~~ e código"
    );
}

// ---------------------------------------------------------------------------
// Ownership: hiding a delimiter does not lose it
// ---------------------------------------------------------------------------

#[test]
fn hidden_delimiters_are_still_covered_by_the_partition() {
    // The whole reason the projection is lossless: what is invisible is not
    // absent. Every byte still belongs to exactly one lexeme.
    let source = "**abc** e *d*";
    let document = with(source, Capabilities::INLINE);
    let projection = document.projection();

    let mut end = 0usize;
    let mut rebuilt = String::new();
    for lexeme in projection.lexemes() {
        assert_eq!(lexeme.source.start(), end);
        rebuilt.push_str(lexeme.source.slice(source));
        end = lexeme.source.end();
    }
    assert_eq!(end, source.len());
    assert_eq!(rebuilt, source, "hiding a delimiter must not drop a byte");
}

#[test]
fn a_caret_inside_an_enabled_mark_exists_and_maps_to_the_right_byte() {
    let source = "**abc**";
    let document = with(source, only("strong"));

    // §26.11 enumerates exactly these: before `a` there is an outer slot at 0
    // and an inner one at 2; after `c`, an inner one at 5 and an outer at 7.
    // Six positions, not four, and the pairs are what let a caret be *inside*
    // or *outside* the mark at the same visual place.
    let offsets: Vec<usize> = document
        .slots()
        .iter()
        .map(|slot| slot.source_offset.get())
        .collect();
    assert_eq!(offsets, vec![0, 2, 3, 4, 5, 7]);

    // And the outer/inner pair really differs in depth.
    let slots = document.slots();
    let outer = slots.iter().find(|s| s.source_offset.get() == 0);
    let inner = slots.iter().find(|s| s.source_offset.get() == 2);
    assert!(
        outer.unwrap().context_path.len() < inner.unwrap().context_path.len(),
        "the slot before the delimiters is outside the mark; the one after is inside"
    );
}

#[test]
fn a_nested_mark_gives_a_boundary_more_than_one_slot() {
    // §26.15.8's "N slots" case, reachable now that marks project.
    let source = "**a *b* c**";
    let document = with(source, Capabilities::INLINE);

    let depths: Vec<usize> = document
        .slots()
        .iter()
        .map(|slot| slot.context_path.len())
        .collect();
    assert!(
        depths.iter().any(|depth| *depth >= 3),
        "a caret inside the nested emphasis must have a deeper path: {depths:?}"
    );

    // §26.4's ordering: outermost first on the way *in*, innermost first on
    // the way *out*. So a boundary's depths are monotonic — ascending at an
    // opening seam, descending at a closing one — and its offsets always
    // ascend, because they are positions in the source.
    for group in document.grouped_slots() {
        let depths: Vec<usize> = group.iter().map(|s| s.context_path.len()).collect();
        let ascending = depths.windows(2).all(|pair| pair[0] <= pair[1]);
        let descending = depths.windows(2).all(|pair| pair[0] >= pair[1]);
        assert!(
            ascending || descending,
            "slots at one boundary must be ordered by depth, got {depths:?}"
        );

        let offsets: Vec<usize> = group.iter().map(|s| s.source_offset.get()).collect();
        assert!(
            offsets.windows(2).all(|pair| pair[0] < pair[1]),
            "offsets at one boundary must ascend, got {offsets:?}"
        );
    }
}

#[test]
fn adjacent_marks_are_siblings_and_keep_their_own_delimiters() {
    let source = "**a****b**";
    let document = with(source, only("strong"));
    // `****` in the middle is not an empty Strong (§26.11) — the run rules make
    // this literal, and what matters is that nothing is lost either way.
    assert_eq!(
        visible(&document).len() + count_hidden(&document),
        source.chars().count(),
        "every character is either shown or accounted for as hidden syntax"
    );
}

/// How many characters of the source are hidden syntax in this projection.
fn count_hidden(document: &VisualDocument) -> usize {
    let shown: usize = document
        .blocks()
        .iter()
        .map(|block| {
            document
                .graphemes_of(block.id)
                .map(|cell| cell.text.chars().count())
                .sum::<usize>()
        })
        .sum();
    document.source().chars().count() - shown
}

// ---------------------------------------------------------------------------
// Editing inside an enabled mark
// ---------------------------------------------------------------------------

#[track_caller]
fn applied(source: &str, capabilities: Capabilities, command: VisualCommand) -> String {
    let mut draft = Draft::new(source);
    let document = VisualDocument::project_with(source, draft.generation(), capabilities);
    let transaction = plan(&document, command).expect("the command should be planned");
    draft.apply_transaction(&transaction).expect("applies");
    draft.text()
}

#[test]
fn typing_inside_an_enabled_mark_stays_inside_it() {
    // §26.11: inserting at the inner caret of `**abc**` gives `**Xabc**`,
    // never `X**abc**`.
    assert_eq!(
        applied(
            "**abc**",
            only("strong"),
            VisualCommand::Insert {
                at: SourceOffset::in_source("**abc**", 2).unwrap(),
                text: "X".into()
            }
        ),
        "**Xabc**"
    );
}

#[test]
fn deleting_inside_an_enabled_mark_leaves_the_delimiters_alone() {
    let source = "**abc**";
    let mut draft = Draft::new(source);
    let document = VisualDocument::project_with(source, draft.generation(), only("strong"));
    let transaction = plan(
        &document,
        VisualCommand::DeleteForward {
            block: document.blocks()[0].id,
            grapheme: noteit_tui::source_map::GraphemeIndex(0),
        },
    )
    .expect("a delete");

    draft.apply_transaction(&transaction).expect("applies");
    assert_eq!(draft.text(), "**bc**", "only `a` went");
}

#[test]
fn a_selection_inside_one_mark_may_be_replaced() {
    assert_eq!(
        applied(
            "**abc**",
            only("strong"),
            VisualCommand::ReplaceSelection {
                anchor: SourceOffset::in_source("**abc**", 3).unwrap(),
                head: SourceOffset::in_source("**abc**", 4).unwrap(),
                text: "X".into(),
            }
        ),
        "**aXc**"
    );
}

// ---------------------------------------------------------------------------
// Partial crossing is always a refusal (§26.15.26)
// ---------------------------------------------------------------------------

#[track_caller]
fn assert_refused(source: &str, capabilities: Capabilities, command: VisualCommand) -> Refusal {
    let mut draft = Draft::new(source);
    let before = draft.text();
    let history = draft.history_depth();

    let document = VisualDocument::project_with(source, draft.generation(), capabilities);
    let refusal = plan(&document, command).expect_err("this must be refused");

    assert_eq!(draft.text(), before, "a refusal changed the source");
    assert_eq!(draft.history_depth(), history);
    assert!(!draft.undo());
    refusal
}

#[test]
fn a_selection_starting_outside_and_ending_inside_a_mark_is_refused() {
    // §26.15.26's own example: in `x **ab**`, selecting `x a`.
    let source = "x **ab**";
    assert_refused(
        source,
        only("strong"),
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 0).unwrap(),
            head: SourceOffset::in_source(source, 6).unwrap(),
            text: String::new(),
        },
    );
}

#[test]
fn a_selection_starting_inside_and_ending_outside_a_mark_is_refused() {
    // The mirror: in `**ab** x`, selecting `b x`.
    let source = "**ab** x";
    assert_refused(
        source,
        only("strong"),
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 3).unwrap(),
            head: SourceOffset::in_source(source, 8).unwrap(),
            text: String::new(),
        },
    );
}

#[test]
fn a_selection_entering_only_part_of_a_nested_stack_is_refused() {
    // `**a *bc* d**`: content that is only Strong, running into the Emphasis.
    let source = "**a *bc* d**";
    assert_refused(
        source,
        Capabilities::INLINE,
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 2).unwrap(),
            head: SourceOffset::in_source(source, 6).unwrap(),
            text: String::new(),
        },
    );
}

#[test]
fn copying_a_partially_crossed_selection_still_succeeds() {
    // §26.6: `CopyVisualSelection` never mutates, so it is allowed for every
    // valid EGC selection — that asymmetry is the point.
    let source = "x **ab**";
    let document = with(source, only("strong"));
    let copied = document.copy_visual(
        SourceOffset::in_source(source, 0).unwrap(),
        SourceOffset::in_source(source, 5).unwrap(),
    );
    assert_eq!(copied.as_deref(), Some("x a"), "copy sees the projection");
}

// ---------------------------------------------------------------------------
// Spelling outside the envelope is untouched
// ---------------------------------------------------------------------------

#[test]
fn editing_one_mark_leaves_an_identical_neighbour_byte_for_byte() {
    // §26.7: "wrapper máximo" means maximal *within the envelope*, never
    // global. Two identical adjacent marks must not be coalesced.
    let source = "**a** **b**";
    let result = applied(
        source,
        only("strong"),
        VisualCommand::Insert {
            at: SourceOffset::in_source(source, 3).unwrap(),
            text: "X".into(),
        },
    );
    assert_eq!(result, "**aX** **b**", "the second mark kept its bytes");
}

#[test]
fn an_edit_never_normalises_an_equivalent_spelling_elsewhere() {
    let source = "**a**   e   ~~b~~";
    let result = applied(
        source,
        Capabilities::INLINE,
        VisualCommand::Insert {
            at: SourceOffset::in_source(source, 3).unwrap(),
            text: "X".into(),
        },
    );
    assert!(
        result.contains("   e   "),
        "whitespace elsewhere was normalised: {result:?}"
    );
    assert!(result.ends_with("~~b~~"));
}

// ---------------------------------------------------------------------------
// Undo is still one step
// ---------------------------------------------------------------------------

#[test]
fn an_edit_inside_a_mark_is_one_undo_step() {
    let source = "**abc**";
    let mut draft = Draft::new(source);
    let before = draft.history_depth();
    let document = VisualDocument::project_with(source, draft.generation(), only("strong"));
    let transaction = plan(
        &document,
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 2).unwrap(),
            head: SourceOffset::in_source(source, 4).unwrap(),
            text: "XYZ".into(),
        },
    )
    .expect("a replacement");

    draft.apply_transaction(&transaction).expect("applies");
    assert_eq!(draft.text(), "**XYZc**");
    assert_eq!(draft.history_depth(), before + 1);
    assert!(draft.undo());
    assert_eq!(draft.text(), source);
}

// ---------------------------------------------------------------------------
// A disabled capability keeps its old behaviour exactly
// ---------------------------------------------------------------------------

#[test]
fn a_disabled_mark_still_refuses_every_edit_inside_it() {
    let source = "*abc*";
    let refusal = assert_refused(
        source,
        only("strong"),
        VisualCommand::Insert {
            at: SourceOffset::in_source(source, 2).unwrap(),
            text: "X".into(),
        },
    );
    assert_eq!(refusal, Refusal::ProtectedRegion);
}

#[test]
fn an_enabled_mark_inside_an_opaque_region_stays_protected() {
    // Ancestor dominance beats any capability (§26.15.10).
    let source = "<custom>**abc**</custom>";
    let document = with(source, Capabilities::INLINE);
    assert_eq!(
        visible(&document),
        source,
        "inside an unknown element nothing is projected away"
    );
    assert!(document.slots().is_empty(), "and there is no caret at all");
}

#[test]
fn selecting_the_whole_content_of_a_mark_is_refused_without_a_cleanup_contract() {
    // §27.18/M5. Rule 4 permits a *proper subset* of a mark's content; taking
    // all of it is the whole-leaf case and needs an explicit contract for the
    // mark's own delimiters. Without one, deleting `abc` from `**abc**` would
    // leave `****`, which §26.11 says is not an empty Strong at all but
    // literal, protected text — the user would have converted an editable
    // construct into an uneditable one by pressing Delete.
    let source = "**abc**";
    let refusal = assert_refused(
        source,
        only("strong"),
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 2).unwrap(),
            head: SourceOffset::in_source(source, 5).unwrap(),
            text: String::new(),
        },
    );
    assert_eq!(refusal, Refusal::MissingCapability);
}

#[test]
fn a_proper_subset_of_a_marks_content_is_permitted() {
    assert_eq!(
        applied(
            "**abc**",
            only("strong"),
            VisualCommand::ReplaceSelection {
                anchor: SourceOffset::in_source("**abc**", 2).unwrap(),
                head: SourceOffset::in_source("**abc**", 4).unwrap(),
                text: String::new(),
            }
        ),
        "**c**"
    );
}
