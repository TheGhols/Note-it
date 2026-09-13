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
    /// Not granted in B.3. Present so the denial is explicit and testable
    /// rather than an operation nobody thought to forbid.
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
        // §26.5 and property 27: B.3's matrix has no Format row, and an absent
        // capability is a denial. It is spelled out here so that granting it in
        // B.5 is a deliberate edit to this file and not an accident.
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
