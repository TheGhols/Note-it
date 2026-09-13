//! Turning a visual intention into a source patch, or refusing to.
//!
//! This module is the only thing in the TUI that may convert "the user pressed
//! a key in the visual editor" into bytes. The renderer never mutates; the
//! [`VisualDocument`] is immutable; the [`Draft`](crate::draft::Draft) applies
//! what arrives here and checks it again on the way in.
//!
//! ## Plan, then apply
//!
//! [`plan`] is pure. It reads an immutable projection and returns either a
//! [`SourceTransaction`] or a [`Refusal`], and it cannot change anything in
//! either case. That split is what makes "a refusal preserves every observable
//! state" a property rather than a promise: there is nothing for a refusal to
//! roll back, because nothing was done.
//!
//! ## Refusing is the normal case
//!
//! Most of this file says no. `docs/tui.md` §26.5 makes every capability
//! absent from the current sub-phase's matrix a denial, so B.3 grants exactly
//! three operations on plain paragraph text — insert, delete, replace — and
//! everything else is a [`Refusal`] with a reason the interface can name. A
//! refusal is not a failure: it is the editor declining to guess, with the raw
//! Markdown mode still there for anyone who meant it.
//!
//! ## Why patches are strict about order
//!
//! §27.11 requires `patches[i].end < patches[j].start` for `i < j`, not merely
//! disjointness. Two empty ranges at the same offset are disjoint by set
//! theory and still ambiguous to apply — which is precisely what Enter inside
//! a mark emits — so inserting several things at one offset must produce one
//! patch whose replacement is already in final byte order.

use crate::draft::Draft;
use crate::source_map::{Generation, GraphemeIndex, SourceOffset, SourceRange};
use crate::visual::{BlockId, VisualDocument};

/// Why a command was declined.
///
/// Typed rather than a bare `None` (§27.21/m4) so the interface can tell the
/// reader what happened and offer the raw editor, instead of a key that
/// silently does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The command was measured against a different version of the text.
    StaleGeneration,
    /// It reaches bytes no caret may enter.
    ProtectedRegion,
    /// It crosses part of an inline mark (§26.15.26).
    PartialMarkBoundary,
    /// The current sub-phase grants no such operation (property 27).
    MissingCapability,
    /// It crosses a block boundary that grants neither Join nor DeleteBoundary.
    BlockBoundary,
    /// The offsets do not name a position in this document.
    InvalidPosition,
    /// There was nothing to do — Backspace at the start of the document, or
    /// Delete at its end.
    ///
    /// Not an error and **not a refusal to announce**: §28.2 makes this case a
    /// no-op with no patch, no history entry and no notice. It is a distinct
    /// value precisely so the interface can stay silent about it while still
    /// saying something useful about a real refusal.
    NothingToDo,
}

/// One replacement of a byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePatch {
    pub range: SourceRange,
    pub replacement: String,
}

/// Everything one visual command does, as one undoable unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTransaction {
    pub generation: Generation,
    /// Strictly ordered and non-touching (§27.11).
    pub patches: Vec<SourcePatch>,
    /// The smallest span the command is allowed to rewrite (§27.12).
    ///
    /// Declared rather than inferred, so that "bytes outside the envelope are
    /// identical" stops being vacuously true of any patch set.
    pub envelope: SourceRange,
    /// Where the caret ends up, in the source after the patches.
    pub resulting_cursor: usize,
}

/// What the visual editor was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualCommand {
    Insert {
        at: SourceOffset,
        text: String,
    },
    /// Insert, stated against an explicit generation, for testing staleness.
    InsertAtGeneration {
        generation: Generation,
        at: SourceOffset,
        text: String,
    },
    /// Backspace: remove the grapheme before this boundary.
    DeleteBackward {
        block: BlockId,
        grapheme: GraphemeIndex,
    },
    /// Delete: remove the grapheme after this boundary.
    DeleteForward {
        block: BlockId,
        grapheme: GraphemeIndex,
    },
    ReplaceSelection {
        anchor: SourceOffset,
        head: SourceOffset,
        text: String,
    },
    /// Enter: split the block at this caret (B.4).
    SplitBlock {
        at: SourceOffset,
    },
    /// Backspace at the start of a block: join it to the one above (B.4).
    JoinBackward {
        at: SourceOffset,
    },
    /// Delete at the end of a block: pull the one below into it (B.4).
    JoinForward {
        at: SourceOffset,
    },
    /// Not granted in B.3 or B.4. Present so the denial is explicit and
    /// testable rather than an operation nobody thought to forbid.
    ToggleStrong {
        anchor: SourceOffset,
        head: SourceOffset,
    },
}

/// Plans `command` against `document`, or refuses.
///
/// Pure: it reads and decides. Nothing here writes.
pub fn plan(
    document: &VisualDocument,
    command: VisualCommand,
) -> Result<SourceTransaction, Refusal> {
    match command {
        VisualCommand::InsertAtGeneration {
            generation,
            at,
            text,
        } => {
            if !document.accepts(generation) {
                return Err(Refusal::StaleGeneration);
            }
            insert(document, at, &text)
        }
        VisualCommand::Insert { at, text } => insert(document, at, &text),
        VisualCommand::DeleteBackward { block, grapheme } => {
            delete_grapheme(document, block, grapheme, Side::Before)
        }
        VisualCommand::DeleteForward { block, grapheme } => {
            delete_grapheme(document, block, grapheme, Side::After)
        }
        VisualCommand::ReplaceSelection { anchor, head, text } => {
            replace_selection(document, anchor, head, &text)
        }
        VisualCommand::SplitBlock { at } => split_block(document, at),
        VisualCommand::JoinBackward { at } => join(document, at, Side::Before),
        VisualCommand::JoinForward { at } => join(document, at, Side::After),
        // §26.5 and property 27: neither B.3's nor B.4's matrix has a Format
        // row, and an absent capability is a denial. It is spelled out here so
        // that granting it in B.5 is a deliberate edit to this file and not an
        // accident.
        VisualCommand::ToggleStrong { .. } => Err(Refusal::MissingCapability),
    }
}

enum Side {
    Before,
    After,
}

/// Whether a caret may sit at `offset` at all.
fn caret_is_legal(document: &VisualDocument, offset: SourceOffset) -> bool {
    document
        .slots()
        .iter()
        .any(|slot| slot.source_offset == offset)
}

fn insert(
    document: &VisualDocument,
    at: SourceOffset,
    text: &str,
) -> Result<SourceTransaction, Refusal> {
    if at.get() > document.source_len() {
        return Err(Refusal::InvalidPosition);
    }
    if document.offset_is_protected(at) {
        return Err(Refusal::ProtectedRegion);
    }
    if !caret_is_legal(document, at) {
        return Err(Refusal::ProtectedRegion);
    }

    let range = SourceRange::trusted(at.get(), at.get());
    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![SourcePatch {
            range,
            replacement: text.to_owned(),
        }],
        envelope: range,
        resulting_cursor: at.get() + text.len(),
    })
}

fn delete_grapheme(
    document: &VisualDocument,
    block: BlockId,
    grapheme: GraphemeIndex,
    side: Side,
) -> Result<SourceTransaction, Refusal> {
    let cells: Vec<_> = document
        .graphemes_of(block)
        .map(|cell| cell.source)
        .collect();

    // The whole cluster or none of it (§26.15.17): the range removed is the
    // grapheme's own source range, whatever its byte length.
    let target = match side {
        Side::Before => grapheme.0.checked_sub(1).and_then(|index| cells.get(index)),
        Side::After => cells.get(grapheme.0),
    };
    // At the edges of the document there is nothing to remove, and §28.2's
    // matrix makes that a no-op rather than a refusal — so it is reported as a
    // missing capability only when a boundary key would have to join blocks.
    let Some(range) = target.copied() else {
        return Err(Refusal::MissingCapability);
    };

    if document.offset_is_protected(SourceOffset::trusted(range.start())) {
        return Err(Refusal::ProtectedRegion);
    }

    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![SourcePatch {
            range,
            replacement: String::new(),
        }],
        envelope: range,
        resulting_cursor: range.start(),
    })
}

fn replace_selection(
    document: &VisualDocument,
    anchor: SourceOffset,
    head: SourceOffset,
    text: &str,
) -> Result<SourceTransaction, Refusal> {
    let (start, end) = if anchor <= head {
        (anchor, head)
    } else {
        (head, anchor)
    };

    if end.get() > document.source_len() {
        return Err(Refusal::InvalidPosition);
    }

    // Rule 0 (§27.4, §28.2): block scope comes first. A selection that crosses
    // a block boundary needs Join or DeleteBoundary for that exact pair, and
    // B.3 grants neither — so every multi-block selection is refused here,
    // including the `# T\n\npara` case where every selected grapheme has the
    // empty mark path and nothing else would catch it.
    let start_block = block_of(document, start);
    let end_block = block_of(document, end);
    if start_block != end_block {
        return Err(Refusal::BlockBoundary);
    }
    let Some(block) = start_block else {
        return Err(Refusal::InvalidPosition);
    };

    // Rule 1: anything Protected, Opaque or SourceVisible in the range refuses
    // the whole operation. Marks are SourceVisible until B.5, so a selection
    // that reaches into one is caught here.
    if !range_is_editable(document, start.get(), end.get()) {
        return Err(Refusal::ProtectedRegion);
    }

    // Both endpoints must be real carets: a selection may not begin or end
    // inside a grapheme or inside invisible syntax.
    if !caret_is_legal(document, start) || !caret_is_legal(document, end) {
        return Err(Refusal::ProtectedRegion);
    }

    // Clause (iii) of rule 0: no BlockPrefix, Metadata or Protected lexeme may
    // be intersected at all, wholly or partly (§28.2).
    if document.range_touches_protected_lexeme(block, start.get(), end.get()) {
        return Err(Refusal::ProtectedRegion);
    }

    let range = SourceRange::trusted(start.get(), end.get());
    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![SourcePatch {
            range,
            replacement: text.to_owned(),
        }],
        envelope: range,
        resulting_cursor: start.get() + text.len(),
    })
}

// -- B.4: block boundaries ---------------------------------------------------

/// The line ending a new break should use, by §27.13's ordered fallback.
///
/// The current block's own ending first, then the one before it, then the
/// document's first, then `LF`. The R2 rule had no answer for the last block
/// of a CRLF document, which is exactly the case that would silently write an
/// LF into a file that had never contained one.
fn local_line_ending(source: &str, block_end: usize) -> &'static str {
    let after = &source[block_end.min(source.len())..];
    if after.starts_with("\r\n") {
        return "\r\n";
    }
    if after.starts_with('\n') {
        return "\n";
    }
    let before = &source[..block_end.min(source.len())];
    if before.contains("\r\n") {
        return "\r\n";
    }
    if source.contains("\r\n") {
        return "\r\n";
    }
    "\n"
}

/// `Enter`: the split rules of the §26.9 matrix.
fn split_block(document: &VisualDocument, at: SourceOffset) -> Result<SourceTransaction, Refusal> {
    if at.get() > document.source_len() {
        return Err(Refusal::InvalidPosition);
    }
    if document.offset_is_protected(at) || !caret_is_legal(document, at) {
        return Err(Refusal::ProtectedRegion);
    }
    let Some(block) = block_of(document, at) else {
        return Err(Refusal::InvalidPosition);
    };

    let source = document.source();
    let visual = document.block(block);
    let ending = local_line_ending(source, visual.content_end);
    let separator = format!("{ending}{ending}");

    // The heading rules differ from the paragraph ones in one place only: a
    // split in the *middle* of a heading produces two headings of the same
    // level, because the level is an attribute of the block and losing it would
    // be a silent demotion. At either end the matrix asks for a paragraph.
    let content_start = document
        .first_caret_offset(block)
        .unwrap_or(visual.coverage.start());
    let at_start = at.get() == content_start;
    let at_end = at.get() == visual.content_end;

    let replacement = match document.heading_level(block) {
        Some(level) if !at_start && !at_end => {
            format!("{separator}{} ", "#".repeat(level as usize))
        }
        _ => separator,
    };

    // At the start of a block with an invisible prefix, the new paragraph goes
    // *before* the prefix — a heading keeps its `# ` and moves down whole.
    // Splitting at the caret would put the break between the marker and its
    // text and leave `# ` orphaned on a line of its own.
    let insert_at = if at_start {
        visual.coverage.start()
    } else {
        at.get()
    };

    let range = SourceRange::trusted(insert_at, insert_at);
    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![SourcePatch {
            range,
            replacement: replacement.clone(),
        }],
        envelope: range,
        // §12: Enter at the start of a block leaves the caret in the new empty
        // paragraph above; everywhere else it follows the text it just moved.
        resulting_cursor: if at_start {
            insert_at
        } else {
            at.get() + replacement.len()
        },
    })
}

/// `Backspace` at a block start, or `Delete` at a block end.
fn join(
    document: &VisualDocument,
    at: SourceOffset,
    side: Side,
) -> Result<SourceTransaction, Refusal> {
    if at.get() > document.source_len() {
        return Err(Refusal::InvalidPosition);
    }
    let Some(block) = block_of(document, at) else {
        return Err(Refusal::InvalidPosition);
    };

    let blocks = document.blocks();
    let index = block.0 as usize;
    let (first, second) = match side {
        Side::Before => (index.checked_sub(1), Some(index)),
        Side::After => (
            Some(index),
            index.checked_add(1).filter(|i| *i < blocks.len()),
        ),
    };

    // §28.2: at the edges of the document there is no pair, and that is a
    // no-op rather than a refusal — nothing to do, and nothing to say.
    let (Some(first), Some(second)) = (first, second) else {
        return Err(Refusal::NothingToDo);
    };
    let (first, second) = (blocks[first], blocks[second]);

    // The key only acts from the edge it belongs to.
    let expected = match side {
        Side::Before => document.first_caret_offset(second.id),
        Side::After => Some(first.content_end),
    };
    if expected != Some(at.get()) {
        return Err(Refusal::NothingToDo);
    }

    // Both blocks must grant Join, and neither may be a construction whose
    // prefix is Protected. A heading may be joined *forward into* — its level
    // survives and the paragraph's text moves in — but never backward, because
    // that would consume the `# ` no caret can reach (§28.2).
    if !document.block_is_joinable(first.id) || !document.block_is_joinable(second.id) {
        return Err(Refusal::BlockBoundary);
    }
    if document.heading_level(second.id).is_some() {
        return Err(Refusal::BlockBoundary);
    }

    // The bytes between the two blocks: the line endings that separate them,
    // which §28.2 assigns to the block before.
    let range = SourceRange::trusted(first.content_end, second.coverage.start());
    if range.start() > range.end() {
        return Err(Refusal::InvalidPosition);
    }

    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![SourcePatch {
            range,
            replacement: String::new(),
        }],
        envelope: range,
        resulting_cursor: first.content_end,
    })
}

fn block_of(document: &VisualDocument, offset: SourceOffset) -> Option<BlockId> {
    document
        .blocks()
        .iter()
        .find(|block| block.coverage.start() <= offset.get() && offset.get() <= block.content_end)
        .map(|block| block.id)
}

/// Whether every grapheme in `start..end` may be edited.
fn range_is_editable(document: &VisualDocument, start: usize, end: usize) -> bool {
    document.blocks().iter().all(|block| {
        document.graphemes_of(block.id).all(|cell| {
            let inside = cell.source.start() >= start && cell.source.end() <= end;
            !inside || !document.offset_is_protected(SourceOffset::trusted(cell.source.start()))
        })
    })
}

impl Draft {
    /// Applies a planned transaction as one undoable step.
    ///
    /// The last check, not the first: the planner already proved the patches
    /// are ordered, disjoint and in range, and this proves the generation is
    /// still the one they were measured in. A stale transaction changes
    /// nothing — §26.15.22 makes the `Draft` the single place a visual
    /// mutation can land, so it is also the single place one can be stopped.
    pub fn apply_transaction(&mut self, transaction: &SourceTransaction) -> Result<(), Refusal> {
        if transaction.generation != self.generation() {
            return Err(Refusal::StaleGeneration);
        }

        let source = self.text();
        for patch in &transaction.patches {
            if patch.range.end() > source.len()
                || !source.is_char_boundary(patch.range.start())
                || !source.is_char_boundary(patch.range.end())
            {
                return Err(Refusal::InvalidPosition);
            }
        }
        for pair in transaction.patches.windows(2) {
            if pair[0].range.end() >= pair[1].range.start() {
                return Err(Refusal::InvalidPosition);
            }
        }

        // Highest offset first, so an earlier patch never moves a later one.
        let mut text = source;
        for patch in transaction.patches.iter().rev() {
            text.replace_range(patch.range.start()..patch.range.end(), &patch.replacement);
        }

        self.replace_all_as_one_step(&text, transaction.resulting_cursor);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual::VisualDocument;

    #[test]
    fn a_refusal_names_its_reason() {
        let document = VisualDocument::project("<x>a</x>", Generation::first());
        let result = plan(
            &document,
            VisualCommand::Insert {
                at: SourceOffset::trusted(4),
                text: "X".into(),
            },
        );
        assert_eq!(result.err(), Some(Refusal::ProtectedRegion));
    }

    #[test]
    fn an_insertion_declares_an_empty_envelope_at_its_offset() {
        let document = VisualDocument::project("abc", Generation::first());
        let transaction = plan(
            &document,
            VisualCommand::Insert {
                at: SourceOffset::trusted(1),
                text: "X".into(),
            },
        )
        .expect("an insertion");
        assert_eq!(transaction.envelope.start(), 1);
        assert_eq!(transaction.envelope.end(), 1);
    }
}
