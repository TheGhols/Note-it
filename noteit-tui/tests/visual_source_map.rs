//! Gate 5.0D.4B.2 — the read-only visual source map.
//!
//! B.2 adds sight, not hands. It turns the byte partition B.1 proved into
//! something a cursor can move through — extended grapheme clusters, caret
//! slots, context paths, a bookmark for raw positions — and it may not mutate
//! anything at all. Every test here either measures that mapping or proves that
//! switching modes changes nothing.
//!
//! The Unicode cases are not decoration. A caret that counts `char`s puts
//! itself between the two code points of `é` written as `e` + U+0301, and
//! between the man and the woman of a family emoji; the whole reason this gate
//! takes a dependency is that neither may happen.

use noteit_tui::draft::Draft;
use noteit_tui::source_map::{Generation, GraphemeIndex, ScalarPosition, SourceOffset};
use noteit_tui::visual::{Direction, VisualDocument};

fn document(source: &str) -> VisualDocument {
    VisualDocument::project(source, Generation::first())
}

// ---------------------------------------------------------------------------
// Extended grapheme clusters (§26.15.7, §26.15.17)
// ---------------------------------------------------------------------------

/// The projected graphemes of the whole document, in order.
fn graphemes(source: &str) -> Vec<String> {
    let document = document(source);
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

#[test]
fn a_grapheme_is_never_split_however_many_scalars_it_has() {
    // Each of these is ONE extended grapheme cluster and several `char`s. A
    // caret counting scalars would land inside every one of them.
    for (source, expected) in [
        ("cafe\u{0301}", 4),                     // e + combining acute
        ("👨\u{200D}👩\u{200D}👧\u{200D}👦", 1), // ZWJ family
        ("🇧🇷", 1),                               // regional indicators
        ("👍🏽", 1),                               // skin tone modifier
        ("日本語", 3),                           // CJK, double width
        ("a\u{0301}\u{0327}b", 2),               // stacked combining marks
    ] {
        let cells = graphemes(source);
        assert_eq!(
            cells.len(),
            expected,
            "{source:?} should be {expected} graphemes, got {cells:?}"
        );
        assert_eq!(cells.concat(), source, "graphemes must rebuild the source");
    }
}

#[test]
fn nfc_and_nfd_are_different_bytes_and_are_never_normalised() {
    let composed = "café";
    let decomposed = "cafe\u{0301}";
    assert_ne!(composed.len(), decomposed.len());

    // Both are four graphemes, and each keeps its own bytes.
    assert_eq!(graphemes(composed).len(), 4);
    assert_eq!(graphemes(decomposed).len(), 4);
    assert_eq!(graphemes(composed).concat(), composed);
    assert_eq!(graphemes(decomposed).concat(), decomposed);
}

#[test]
fn a_tab_is_a_grapheme_and_is_not_dropped() {
    // §27.17: ratatui's own grapheme iterator filters control characters, TAB
    // among them, which would shift the index of everything after it. This is
    // the regression that forbids using it as the source map's segmenter.
    let source = "a\tb";
    let cells = graphemes(source);
    assert_eq!(cells, vec!["a", "\t", "b"]);
    assert_eq!(cells.concat(), source);
}

#[test]
fn display_width_is_separate_from_grapheme_count() {
    // A CJK ideograph is one grapheme and two cells. Conflating them is how a
    // cursor ends up half a character off on a wide glyph.
    let document = document("日a");
    let cells: Vec<_> = document.graphemes_of(document.blocks()[0].id).collect();
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].width, 2, "CJK is two cells wide");
    assert_eq!(cells[1].width, 1);
}

// ---------------------------------------------------------------------------
// Caret slots (§26.15.7 - §26.15.9)
// ---------------------------------------------------------------------------

#[test]
fn every_caret_slot_sits_on_a_grapheme_boundary() {
    for source in [
        "texto simples",
        "# Um título\n\nUm parágrafo com ç e 日.\n",
        "cafe\u{0301} e 👨\u{200D}👩\u{200D}👧\u{200D}👦\n",
        "**negrito** e `código`\n",
        "<x>opaco</x> depois\n",
    ] {
        let document = document(source);
        for slot in document.slots() {
            let block = document.block(slot.block);
            let boundaries: Vec<usize> = document
                .graphemes_of(block.id)
                .map(|cell| cell.source.start())
                .chain(std::iter::once(block.content_end))
                .collect();
            assert!(
                boundaries.contains(&slot.source_offset.get()),
                "slot at {:?} in {source:?} is not on a grapheme boundary (boundaries {boundaries:?})",
                slot.source_offset
            );
        }
    }
}

#[test]
fn no_editable_slot_exists_inside_a_protected_or_opaque_region() {
    // §26.15.9, amended by §28.4 to be about nodes rather than lexemes.
    for source in [
        "<custom>hello</custom> depois",
        "<x>a\n# h",
        "```rust\nfn main() {}\n```\n",
        "<!-- comentário -->\n",
        "# Título\n",
    ] {
        let document = document(source);
        for slot in document.slots() {
            assert!(
                document.slot_is_editable(slot.id),
                "every slot published in B.2 must be editable; {:?} in {source:?} is not",
                slot.id
            );
            assert!(
                !document.offset_is_protected(slot.source_offset),
                "slot at {:?} in {source:?} falls inside a protected region",
                slot.source_offset
            );
        }
    }
}

#[test]
fn a_heading_prefix_holds_no_caret_and_its_text_does() {
    // `# ` is Protected and invisible (§28.4). The first caret of the block is
    // after it, so typing at the start of a heading cannot break the heading.
    let source = "# T";
    let document = document(source);
    let block = document.blocks()[0];

    assert_eq!(
        document.graphemes_of(block.id).count(),
        1,
        "only `T` is projected"
    );

    let offsets: Vec<usize> = document
        .slots()
        .iter()
        .map(|slot| slot.source_offset.get())
        .collect();
    assert_eq!(
        offsets,
        vec![2, 3],
        "carets only around `T`, never inside `# `"
    );
}

#[test]
fn a_boundary_may_carry_zero_one_or_several_ordered_slots() {
    // §26.15.8. Zero: inside an opaque region. One: ordinary paragraph text.
    // Several requires two nested *projected* marks, which only exist from B.6
    // — §27.21/m8 records that, so what B.2 proves is the ordering rule and the
    // zero and one cases.
    assert_eq!(
        document("<x>abc</x>").slots().len(),
        0,
        "zero inside Opaque"
    );

    let document = document("ab");
    assert_eq!(document.slots().len(), 3, "one per boundary of `ab`");

    // Wherever a boundary does carry more than one, they are ordered by
    // context-path depth: outermost first on the way in.
    for slot_group in document.grouped_slots() {
        let depths: Vec<usize> = slot_group
            .iter()
            .map(|slot| slot.context_path.len())
            .collect();
        let mut sorted = depths.clone();
        sorted.sort_unstable();
        assert_eq!(depths, sorted, "slots must be ordered outermost-first");
    }
}

#[test]
fn the_canonical_slot_is_a_total_function_of_boundary_and_direction() {
    // §27.8 as amended by §28.6: every direction has an answer at every
    // boundary, including the ends of a block and `Absolute`.
    let document = document("# T");
    let block = document.blocks()[0].id;

    for grapheme in 0..=1 {
        for direction in [
            Direction::FromLeft,
            Direction::FromRight,
            Direction::Absolute,
        ] {
            assert!(
                document
                    .canonical_slot(block, GraphemeIndex(grapheme), direction)
                    .is_some(),
                "no canonical slot at grapheme {grapheme} for {direction:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Source <-> visual round trip (§26.15.4)
// ---------------------------------------------------------------------------

#[test]
fn every_slot_round_trips_through_its_source_offset() {
    for source in [
        "texto simples",
        "# Título\n\nparágrafo\n",
        "cafe\u{0301} 日本 👨\u{200D}👩\u{200D}👧\u{200D}👦\n",
        "linha um\nlinha dois\n",
    ] {
        let document = document(source);
        for slot in document.slots() {
            let found = document
                .slot_for_offset(slot.source_offset, Direction::Absolute)
                .expect("an offset that came from a slot must resolve to one");
            assert_eq!(
                found.source_offset, slot.source_offset,
                "round trip changed the offset in {source:?}"
            );
        }
    }
}

#[test]
fn a_scalar_position_and_a_source_offset_agree_across_unicode() {
    // The bridge to the raw editor: the visual layer converts at the boundary
    // and never stores the other layer's unit.
    let source = "a\nçé日\nfim";
    let draft = Draft::new(source);
    assert_eq!(draft.text(), source);

    for (index, _) in source.char_indices().chain([(source.len(), ' ')]) {
        let offset = SourceOffset::in_source(source, index).expect("a boundary");
        let scalar = noteit_tui::source_map::scalar_of_offset(source, offset).expect("a position");
        assert_eq!(
            noteit_tui::source_map::offset_of_scalar(source, scalar),
            Some(offset)
        );
    }

    // And the raw editor agrees about where line 1 column 0 is.
    let position = ScalarPosition { line: 1, column: 0 };
    assert_eq!(
        noteit_tui::source_map::offset_of_scalar(source, position),
        SourceOffset::in_source(source, 2)
    );
}

// ---------------------------------------------------------------------------
// Stale rejection (§26.15.20) and the bookmark (§26.15.16, §27.9)
// ---------------------------------------------------------------------------

#[test]
fn a_document_from_another_generation_is_refused() {
    let first = VisualDocument::project("abc", Generation::first());
    let second = VisualDocument::project("abc", Generation::first().next());

    assert!(first.accepts(Generation::first()));
    assert!(!first.accepts(Generation::first().next()));
    assert!(second.accepts(Generation::first().next()));
    assert!(!second.accepts(Generation::first()));
}

#[test]
fn an_intact_bookmark_restores_the_exact_raw_offsets() {
    // §27.9: raw -> visual -> raw with no movement and no mutation gives back
    // the byte the cursor was on, even when it was inside a delimiter.
    let source = "**abc**";
    let generation = Generation::first();
    let anchor = SourceOffset::in_source(source, 1).expect("inside the delimiter");
    let head = SourceOffset::in_source(source, 1).expect("inside the delimiter");

    let bookmark = noteit_tui::visual::RawBookmark::capture(generation, anchor, head);
    assert_eq!(bookmark.restore(generation), Some((anchor, head)));

    // A different generation is refused outright.
    assert_eq!(bookmark.restore(generation.next()), None);
}

#[test]
fn visual_movement_consumes_the_bookmark_instead_of_undoing_the_user() {
    // §27.9 again: the R2 rule would have restored byte 1 after the user moved
    // the caret three times, jumping the cursor backwards over their own
    // navigation. Movement consumes the bookmark.
    let generation = Generation::first();
    let offset = SourceOffset::in_source("**abc**", 1).expect("a boundary");
    let mut bookmark = noteit_tui::visual::RawBookmark::capture(generation, offset, offset);

    assert!(bookmark.is_intact());
    bookmark.consume();
    assert!(!bookmark.is_intact());
    assert_eq!(
        bookmark.restore(generation),
        None,
        "a consumed bookmark must not reposition the cursor"
    );
}

// ---------------------------------------------------------------------------
// Mode switching mutates nothing (§26.15.15)
// ---------------------------------------------------------------------------

#[test]
fn switching_modes_repeatedly_changes_no_byte_and_no_history() {
    for source in [
        "# Título\n\nparágrafo **com** marca\n",
        "<x>opaco</x>\n",
        "cafe\u{0301} 👨\u{200D}👩\u{200D}👧\u{200D}👦\n",
        "a\r\nb\r\n",
        "",
    ] {
        let draft = Draft::new(source);
        let before = draft.text();
        let history_before = draft.history_depth();

        for _ in 0..5 {
            // Entering Visual mode is a projection and nothing else.
            let document = VisualDocument::project(&draft.text(), Generation::first());
            assert_eq!(document.source_len(), draft.text().len());
            // Leaving it restores raw positions from the bookmark.
            let offset = SourceOffset::in_source(&draft.text(), 0).expect("a boundary");
            let bookmark =
                noteit_tui::visual::RawBookmark::capture(Generation::first(), offset, offset);
            assert!(bookmark.restore(Generation::first()).is_some());
        }

        assert_eq!(draft.text(), before, "mode switching changed the source");
        assert_eq!(
            draft.history_depth(),
            history_before,
            "mode switching created history"
        );
    }
}

#[test]
fn a_region_with_no_slots_still_projects_its_source_for_the_reader() {
    // The source-visible fallback: an opaque region has no caret, but the
    // reader must still see the bytes — hiding them would suggest they could
    // be edited.
    let source = "<custom foo=\"bar\">hello</custom>";
    let document = document(source);

    let visible: String = document
        .blocks()
        .iter()
        .flat_map(|block| {
            document
                .graphemes_of(block.id)
                .map(|cell| cell.text.to_owned())
        })
        .collect();
    assert_eq!(visible, source, "an opaque region is shown verbatim");
    assert_eq!(document.slots().len(), 0, "and carries no caret");
}

#[test]
fn the_projection_of_a_large_document_is_produced_once_and_is_stable() {
    let source = "parágrafo com acentuação e 日本語\n\n".repeat(500);
    let first = document(&source);
    let second = document(&source);

    assert_eq!(first.slots().len(), second.slots().len());
    assert_eq!(first.blocks().len(), second.blocks().len());
    assert_eq!(first.source_len(), source.len());
}
