//! Gate 5.0D.4B.3 — minimal visual editing.
//!
//! B.3 is the first gate where a key changes a byte, and almost every test
//! here is about what must *not* happen when it does. A visual command becomes
//! a [`SourceTransaction`]: ordered, disjoint patches against one generation,
//! applied by the `Draft` and nobody else, as one undo step. Anything the
//! planner cannot prove is a `Refusal`, and a `Refusal` leaves every observable
//! piece of state exactly as it was.
//!
//! Scope is plain text and paragraphs. Headings get their boundaries in B.4,
//! marks get their capabilities in B.5 and B.6; asking for either here must be
//! refused, because an absent capability is a denial (§26.5, property 27).

use noteit_tui::draft::Draft;
use noteit_tui::source_map::{Generation, SourceOffset};
use noteit_tui::visual::{Direction, VisualDocument};
use noteit_tui::visual_edit::{plan, Refusal, VisualCommand};

/// Plans `command` against a freshly projected `source`.
fn planned(
    source: &str,
    command: VisualCommand,
) -> Result<noteit_tui::visual_edit::SourceTransaction, Refusal> {
    let document = VisualDocument::project(source, Generation::first());
    plan(&document, command)
}

/// The source after applying a planned command, for the happy path.
///
/// The projection is built from the draft's own generation, which is how the
/// application does it: a `VisualDocument` is only ever valid for the version
/// of the text it was projected from.
fn applied(source: &str, command: VisualCommand) -> String {
    let mut draft = Draft::new(source);
    let document = VisualDocument::project(source, draft.generation());
    let transaction = plan(&document, command).expect("the command should be planned");
    draft
        .apply_transaction(&transaction)
        .expect("a planned transaction applies");
    draft.text()
}

/// A draft and a projection of it, agreeing about the generation.
fn draft_and_document(source: &str) -> (Draft, VisualDocument) {
    let draft = Draft::new(source);
    let document = VisualDocument::project(source, draft.generation());
    (draft, document)
}

/// The offset of the first occurrence of `needle`, as a caret position.
fn at(source: &str, needle: &str) -> SourceOffset {
    SourceOffset::in_source(source, source.find(needle).expect("the needle")).expect("a boundary")
}

// ---------------------------------------------------------------------------
// Patches are generation-correct, ordered, disjoint and valid (§26.15.11)
// ---------------------------------------------------------------------------

#[test]
fn a_transaction_carries_ordered_disjoint_patches_of_its_own_generation() {
    let source = "abc def";
    let document = VisualDocument::project(source, Generation::first());
    let transaction = plan(
        &document,
        VisualCommand::Insert {
            at: at(source, "def"),
            text: "X".into(),
        },
    )
    .expect("a plain insertion");

    assert_eq!(transaction.generation, Generation::first());
    assert!(!transaction.patches.is_empty());

    // §27.11 made this strict: for i < j, patches[i].end < patches[j].start.
    // Two empty ranges at one offset are "disjoint" by set theory and still
    // ambiguous to apply, so they must be fused into one patch instead.
    for pair in transaction.patches.windows(2) {
        assert!(
            pair[0].range.end() < pair[1].range.start(),
            "patches must be strictly ordered and never share an offset: {:?}",
            transaction.patches
        );
    }

    for patch in &transaction.patches {
        assert!(source.is_char_boundary(patch.range.start()));
        assert!(source.is_char_boundary(patch.range.end()));
        assert!(std::str::from_utf8(patch.replacement.as_bytes()).is_ok());
    }
}

#[test]
fn every_patch_lies_inside_the_declared_envelope() {
    // §27.12: the envelope is declared and compared, so a whole-document
    // rewrite can no longer pass the property vacuously.
    let source = "primeiro parágrafo\n\nsegundo parágrafo";
    let transaction = planned(
        source,
        VisualCommand::Insert {
            at: at(source, "segundo"),
            text: "X".into(),
        },
    )
    .expect("an insertion");

    for patch in &transaction.patches {
        assert!(
            transaction.envelope.covers(patch.range),
            "patch {:?} escapes envelope {:?}",
            patch.range,
            transaction.envelope
        );
    }

    // And the envelope is local: it may not reach the untouched first block.
    assert!(
        transaction.envelope.start() >= source.find("segundo").unwrap(),
        "the envelope reached into a block the edit never touched"
    );
}

#[test]
fn bytes_outside_the_envelope_are_identical_after_the_edit() {
    let source = "um\n\ndois\n\ntrês";
    let (mut draft, document) = draft_and_document(source);
    let transaction = plan(
        &document,
        VisualCommand::Insert {
            at: at(source, "dois"),
            text: "Z".into(),
        },
    )
    .expect("an insertion");

    draft.apply_transaction(&transaction).expect("applies");
    let after = draft.text();

    let envelope = transaction.envelope;
    assert_eq!(&after[..envelope.start()], &source[..envelope.start()]);
    let tail_before = &source[envelope.end()..];
    let tail_after = &after[after.len() - tail_before.len()..];
    assert_eq!(tail_after, tail_before, "bytes after the envelope changed");
}

// ---------------------------------------------------------------------------
// The three commands B.3 grants
// ---------------------------------------------------------------------------

#[test]
fn insert_puts_text_exactly_where_the_caret_is() {
    assert_eq!(
        applied(
            "abc",
            VisualCommand::Insert {
                at: SourceOffset::in_source("abc", 1).unwrap(),
                text: "X".into()
            }
        ),
        "aXbc"
    );
}

#[test]
fn insert_preserves_unicode_and_never_splits_a_grapheme() {
    let source = "cafe\u{0301}";
    let end = SourceOffset::in_source(source, source.len()).unwrap();
    let result = applied(
        source,
        VisualCommand::Insert {
            at: end,
            text: " ☕".into(),
        },
    );
    assert_eq!(result, "cafe\u{0301} ☕");
}

#[test]
fn delete_removes_one_whole_grapheme_however_many_bytes_it_has() {
    // §26.15.17: a family emoji is 25 bytes and one grapheme. Backspace takes
    // all of it or none of it.
    let source = "a👨\u{200D}👩\u{200D}👧\u{200D}👦b";
    let (mut draft, document) = draft_and_document(source);
    let block = document.blocks()[0].id;

    // The caret after the emoji is grapheme 2.
    let transaction = plan(
        &document,
        VisualCommand::DeleteBackward {
            block,
            grapheme: noteit_tui::source_map::GraphemeIndex(2),
        },
    )
    .expect("a backspace");

    draft.apply_transaction(&transaction).expect("applies");
    assert_eq!(draft.text(), "ab", "the whole cluster went, and only it");
}

#[test]
fn delete_forward_removes_the_grapheme_after_the_caret() {
    let source = "a日b";
    let (mut draft, document) = draft_and_document(source);
    let block = document.blocks()[0].id;
    let transaction = plan(
        &document,
        VisualCommand::DeleteForward {
            block,
            grapheme: noteit_tui::source_map::GraphemeIndex(1),
        },
    )
    .expect("a delete");

    draft.apply_transaction(&transaction).expect("applies");
    assert_eq!(draft.text(), "ab");
}

#[test]
fn replacing_a_selection_is_one_transaction() {
    let source = "abcdef";
    let result = applied(
        source,
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 1).unwrap(),
            head: SourceOffset::in_source(source, 4).unwrap(),
            text: "X".into(),
        },
    );
    assert_eq!(result, "aXef");
}

#[test]
fn deleting_a_whole_paragraph_is_permitted() {
    // §28.3/N1: plain text has the empty mark path, and the whole-leaf rule
    // must not capture it — a paragraph has no delimiters to clean up, so
    // selecting all of it and deleting is an ordinary edit.
    let source = "parágrafo inteiro";
    let result = applied(
        source,
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 0).unwrap(),
            head: SourceOffset::in_source(source, source.len()).unwrap(),
            text: String::new(),
        },
    );
    assert_eq!(result, "");
}

// ---------------------------------------------------------------------------
// Refusals (§26.15.13, §26.15.27, §28.2)
// ---------------------------------------------------------------------------

/// Asserts a command is refused and that nothing observable moved.
#[track_caller]
fn assert_refused(source: &str, command: VisualCommand, expected: Refusal) {
    let mut draft = Draft::new(source);
    let before_text = draft.text();
    let before_history = draft.history_depth();
    let before_cursor = draft.cursor();

    let document = VisualDocument::project(source, Generation::first());
    let result = plan(&document, command);

    assert_eq!(result.err(), Some(expected), "expected a refusal");
    assert_eq!(draft.text(), before_text, "a refusal changed the source");
    assert_eq!(
        draft.history_depth(),
        before_history,
        "a refusal created history"
    );
    assert_eq!(draft.cursor(), before_cursor, "a refusal moved the cursor");
    assert!(!draft.undo(), "a refusal left something to undo");
}

#[test]
fn a_command_against_a_protected_region_is_refused() {
    let source = "<custom>hello</custom>";
    assert_refused(
        source,
        VisualCommand::Insert {
            at: SourceOffset::in_source(source, 9).unwrap(),
            text: "X".into(),
        },
        Refusal::ProtectedRegion,
    );
}

#[test]
fn a_command_of_an_older_generation_is_refused() {
    let source = "abc";
    let document = VisualDocument::project(source, Generation::first());
    // The document is generation 0; ask it to plan against generation 1.
    let result = plan(
        &document,
        VisualCommand::InsertAtGeneration {
            generation: Generation::first().next(),
            at: SourceOffset::in_source(source, 1).unwrap(),
            text: "X".into(),
        },
    );
    assert_eq!(result.err(), Some(Refusal::StaleGeneration));
}

#[test]
fn a_selection_crossing_a_block_boundary_is_refused() {
    // §28.2/B3: the counterexample the review chain kept open the longest.
    // Every selected grapheme has the empty mark path, so nothing but a block
    // rule can catch it.
    let source = "# T\n\npara";
    assert_refused(
        source,
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 3).unwrap(),
            head: SourceOffset::in_source(source, 7).unwrap(),
            text: String::new(),
        },
        Refusal::BlockBoundary,
    );
}

#[test]
fn a_selection_crossing_two_paragraphs_is_refused_in_b3() {
    // §28.2 again: `Join` and `DeleteBoundary` arrive in B.4, and an absent
    // capability is a denial.
    let source = "ab\n\ncd";
    assert_refused(
        source,
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 1).unwrap(),
            head: SourceOffset::in_source(source, 5).unwrap(),
            text: String::new(),
        },
        Refusal::BlockBoundary,
    );
}

#[test]
fn a_selection_partially_crossing_a_mark_is_refused() {
    // §26.15.26. Marks are SourceVisible until B.5, so this is refused for
    // protection now and for the boundary rule later; either way it is one
    // determinate answer and it never mutates.
    let source = "x **ab**";
    let result = planned(
        source,
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 0).unwrap(),
            head: SourceOffset::in_source(source, 5).unwrap(),
            text: String::new(),
        },
    );
    assert!(result.is_err(), "a partially crossed mark must refuse");
}

#[test]
fn formatting_is_refused_because_b3_grants_no_such_capability() {
    // Property 27: absence from the matrix is a denial, not an omission.
    let source = "abc";
    assert_refused(
        source,
        VisualCommand::ToggleStrong {
            anchor: SourceOffset::in_source(source, 0).unwrap(),
            head: SourceOffset::in_source(source, 3).unwrap(),
        },
        Refusal::MissingCapability,
    );
}

// ---------------------------------------------------------------------------
// History, Draft authority and the raw editor (§26.15.14, §26.15.22)
// ---------------------------------------------------------------------------

#[test]
fn one_command_is_exactly_one_undo_step() {
    let source = "abc";
    let (mut draft, document) = draft_and_document(source);
    let before = draft.history_depth();
    let transaction = plan(
        &document,
        VisualCommand::ReplaceSelection {
            anchor: SourceOffset::in_source(source, 0).unwrap(),
            head: SourceOffset::in_source(source, 3).unwrap(),
            text: "XYZ".into(),
        },
    )
    .expect("a replacement");

    draft.apply_transaction(&transaction).expect("applies");
    assert_eq!(draft.text(), "XYZ");
    assert_eq!(
        draft.history_depth(),
        before + 1,
        "one command, one history entry"
    );

    assert!(draft.undo());
    assert_eq!(draft.text(), source, "one undo restores the whole command");
}

#[test]
fn applying_a_transaction_advances_the_generation() {
    // §27.2/B1: every mutation advances it, so a document projected before the
    // edit can be recognised as stale afterwards.
    let mut draft = Draft::new("abc");
    let before = draft.generation();

    let document = VisualDocument::project(&draft.text(), before);
    let transaction = plan(
        &document,
        VisualCommand::Insert {
            at: SourceOffset::in_source("abc", 1).unwrap(),
            text: "X".into(),
        },
    )
    .expect("an insertion");

    draft.apply_transaction(&transaction).expect("applies");
    assert!(
        draft.generation() > before,
        "the generation must advance on every mutation"
    );
    assert!(
        !document.accepts(draft.generation()),
        "the old projection must now be stale"
    );
}

#[test]
fn undo_and_redo_also_advance_the_generation() {
    // §27.2 is explicit that undo never restores an older generation: it moves
    // forward like any other mutation, or a bookmark from the undone state
    // would be accepted against the redone one.
    let mut draft = Draft::new("abc");
    draft.insert_char('X');
    let after_edit = draft.generation();

    assert!(draft.undo());
    assert!(draft.generation() > after_edit, "undo advances");

    let after_undo = draft.generation();
    assert!(draft.redo());
    assert!(draft.generation() > after_undo, "redo advances");
}

#[test]
fn a_raw_keystroke_advances_the_generation_too() {
    let mut draft = Draft::new("abc");
    let mut previous = draft.generation();
    for character in "xyz".chars() {
        draft.insert_char(character);
        assert!(draft.generation() > previous, "a raw key must advance it");
        previous = draft.generation();
    }
}

#[test]
fn a_stale_transaction_is_refused_by_the_draft_itself() {
    // §26.15.22: no visual mutation bypasses the Draft, and the Draft is the
    // last line of defence — it checks the generation it is handed.
    let mut draft = Draft::new("abc");
    let document = VisualDocument::project(&draft.text(), draft.generation());
    let transaction = plan(
        &document,
        VisualCommand::Insert {
            at: SourceOffset::in_source("abc", 1).unwrap(),
            text: "X".into(),
        },
    )
    .expect("an insertion");

    // Something else edits first, so the transaction is now from the past.
    draft.insert_char('!');
    let text_before = draft.text();
    let history_before = draft.history_depth();

    assert_eq!(
        draft.apply_transaction(&transaction),
        Err(Refusal::StaleGeneration)
    );
    assert_eq!(
        draft.text(),
        text_before,
        "a stale transaction changed text"
    );
    assert_eq!(draft.history_depth(), history_before);
}

#[test]
fn the_canonical_slot_decides_where_typed_text_lands() {
    // §27.8: arriving at the end of a run by Right and typing puts the text
    // inside the run, which is what keeps the style when typing at its edge.
    let source = "# T";
    let mut draft = Draft::new(source);
    let document = VisualDocument::project(source, draft.generation());
    let block = document.blocks()[0].id;
    let slot = document
        .canonical_slot(
            block,
            noteit_tui::source_map::GraphemeIndex(1),
            Direction::FromLeft,
        )
        .expect("a canonical slot");

    let transaction = plan(
        &document,
        VisualCommand::Insert {
            at: slot.source_offset,
            text: "X".into(),
        },
    )
    .expect("an insertion");

    draft.apply_transaction(&transaction).expect("applies");
    assert_eq!(
        draft.text(),
        "# TX",
        "typed inside the heading, after its text"
    );
}
