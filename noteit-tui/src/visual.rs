//! The visual source map: what the reader sees, and where a caret may go.
//!
//! B.1 proved the bytes partition. This layer turns that partition into
//! something a cursor can move through, and it is deliberately **read only**:
//! a [`VisualDocument`] is derived, immutable and valid for exactly one
//! [`Generation`]. Nothing here writes to a [`Draft`](crate::draft::Draft), and
//! nothing here can — the type has no method that returns a mutation.
//!
//! ## Three units, kept apart
//!
//! - a **source offset** is a byte, and it is what a patch is written in;
//! - a **grapheme index** is an extended grapheme cluster within one block, and
//!   it is what an arrow key moves by;
//! - a **display column** is a terminal cell, and it is only ever layout.
//!
//! They disagree for almost every character that is not ASCII. `e` + U+0301 is
//! three bytes, one grapheme and one cell; `日` is three bytes, one grapheme
//! and *two* cells; a ZWJ family emoji is twenty-five bytes, one grapheme and
//! two cells. A cursor stored in the wrong one of those is a cursor that lands
//! inside a character.
//!
//! ## Why the segmenter is a dependency
//!
//! Writing one by hand is forbidden by `docs/tui.md` §26.12, and ratatui's own
//! `styled_graphemes` is forbidden as this map's segmenter by §27.17: it drops
//! any grapheme containing a control character, TAB included, which would
//! silently shift the index of everything after it. `unicode-segmentation` is
//! the audited alternative.
//!
//! ## What B.2 does not do
//!
//! No mutation, no command, no transaction. Marks and canonical HTML are still
//! `SourceVisible` here, so their delimiters are projected literally and carry
//! no interior caret; they become invisible only when B.5 and B.6 grant the
//! capabilities that make hiding them honest.

use crate::projection::{project, LexemeKind, NodeId, NodeKind, Projection};
use crate::source_map::{Generation, GraphemeIndex, SourceOffset, SourceRange};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CaretSlotId(pub u32);

/// Which way the caret arrived at a boundary.
///
/// The canonical-slot rule is a total function of `(boundary, direction)`
/// (§27.8, §28.6), and `Absolute` is the case every editor forgets: `End`, a
/// click, `Up`/`Down`, the cursor a transaction leaves behind, undo, and the
/// snap of a bookmark all arrive without a direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    FromLeft,
    FromRight,
    Absolute,
}

/// One extended grapheme cluster, as the reader sees it.
#[derive(Debug, Clone)]
pub struct GraphemeCell<'a> {
    /// The bytes it occupies in the source.
    pub source: SourceRange,
    /// Its text, borrowed from the source.
    pub text: &'a str,
    /// How many terminal cells it takes. Layout only, never a position.
    pub width: usize,
}

/// A block of projected content: a paragraph, a heading, an opaque region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualBlock {
    pub id: BlockId,
    pub node: NodeId,
    /// The source span this block projects.
    pub coverage: SourceRange,
    /// The offset just past the block's last projected byte that is not a line
    /// ending — the end of its *text*, and where a caret at the end lives.
    ///
    /// The trailing line ending is deliberately outside it. It belongs to this
    /// block (§28.2) but it is not content: a caret after it would be a caret
    /// in the gap between blocks, and a Join that started before it would
    /// leave one of the two line endings behind.
    pub content_end: usize,
    /// How many graphemes it projects.
    pub graphemes: usize,
}

/// A place the caret may legally be.
#[derive(Debug, Clone)]
pub struct CaretSlot {
    pub id: CaretSlotId,
    pub generation: Generation,
    pub block: BlockId,
    pub grapheme: GraphemeIndex,
    pub source_offset: SourceOffset,
    /// The semantic nodes containing this position, outermost first.
    pub context_path: Vec<NodeId>,
}

/// A raw cursor position kept across a trip through the visual editor.
///
/// Raw mode can put the cursor anywhere — inside a delimiter, inside a tag,
/// inside an attribute — and the visual editor has no slot for most of those.
/// The bookmark remembers the exact bytes so that going in and straight back
/// out is not a move.
///
/// It is valid only while **intact**: any mutation, and any cursor movement in
/// visual mode, consumes it (§27.9). Without that rule, moving the caret three
/// times and switching back would silently undo the user's own navigation,
/// which is what the earlier draft of this contract required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawBookmark {
    generation: Generation,
    anchor: SourceOffset,
    head: SourceOffset,
    intact: bool,
}

impl RawBookmark {
    pub fn capture(generation: Generation, anchor: SourceOffset, head: SourceOffset) -> Self {
        Self {
            generation,
            anchor,
            head,
            intact: true,
        }
    }

    /// The exact raw offsets, if this bookmark still speaks for the cursor.
    pub fn restore(&self, generation: Generation) -> Option<(SourceOffset, SourceOffset)> {
        (self.intact && self.generation == generation).then_some((self.anchor, self.head))
    }

    pub fn is_intact(&self) -> bool {
        self.intact
    }

    /// Marks the bookmark spent. Called by any visual movement or mutation.
    pub fn consume(&mut self) {
        self.intact = false;
    }
}

/// The document as the visual editor sees it: derived, immutable, one
/// generation.
#[derive(Debug, Clone)]
pub struct VisualDocument {
    generation: Generation,
    source: String,
    projection: Projection,
    blocks: Vec<VisualBlock>,
    /// Grapheme ranges per block, as `(source range, width)`.
    cells: Vec<Vec<(SourceRange, usize)>>,
    slots: Vec<CaretSlot>,
}

impl VisualDocument {
    /// Projects `source` for `generation`.
    pub fn project(source: &str, generation: Generation) -> Self {
        let projection = project(source, generation);
        let mut document = Self {
            generation,
            source: source.to_owned(),
            projection,
            blocks: Vec::new(),
            cells: Vec::new(),
            slots: Vec::new(),
        };
        document.build();
        document
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn source_len(&self) -> usize {
        self.source.len()
    }

    pub fn projection(&self) -> &Projection {
        &self.projection
    }

    pub fn blocks(&self) -> &[VisualBlock] {
        &self.blocks
    }

    pub fn block(&self, id: BlockId) -> VisualBlock {
        self.blocks[id.0 as usize]
    }

    pub fn slots(&self) -> &[CaretSlot] {
        &self.slots
    }

    /// Whether an object measured in `generation` may still be used.
    pub fn accepts(&self, generation: Generation) -> bool {
        self.generation == generation
    }

    /// The graphemes of one block, in order.
    pub fn graphemes_of(&self, block: BlockId) -> impl Iterator<Item = GraphemeCell<'_>> + '_ {
        self.cells[block.0 as usize]
            .iter()
            .map(move |(range, width)| GraphemeCell {
                source: *range,
                text: range.slice(&self.source),
                width: *width,
            })
    }

    /// The slots at one boundary, ordered outermost-first.
    pub fn slots_at(&self, block: BlockId, grapheme: GraphemeIndex) -> Vec<&CaretSlot> {
        self.slots
            .iter()
            .filter(|slot| slot.block == block && slot.grapheme == grapheme)
            .collect()
    }

    /// Every boundary's slots, for checking the ordering invariant.
    pub fn grouped_slots(&self) -> Vec<Vec<&CaretSlot>> {
        let mut groups: Vec<Vec<&CaretSlot>> = Vec::new();
        for slot in &self.slots {
            match groups.last_mut() {
                Some(group)
                    if group[0].block == slot.block && group[0].grapheme == slot.grapheme =>
                {
                    group.push(slot)
                }
                _ => groups.push(vec![slot]),
            }
        }
        groups
    }

    /// The canonical text slot of a boundary, by §27.8 as amended by §28.6.
    ///
    /// Total: every boundary that exists has one, for every direction. The
    /// rule keeps the style when typing at the start or end of a run, and makes
    /// `End` followed by typing behave like arriving there with `Right`.
    pub fn canonical_slot(
        &self,
        block: BlockId,
        grapheme: GraphemeIndex,
        direction: Direction,
    ) -> Option<&CaretSlot> {
        let candidates = self.slots_at(block, grapheme);
        if candidates.is_empty() {
            return None;
        }

        let cells = &self.cells[block.0 as usize];
        let has_left = grapheme.0 > 0;
        let has_right = grapheme.0 < cells.len();

        // Rule 1: a directional arrival with a run on the side moved towards
        // takes the innermost slot containing that run. Rules 2 and 3 fall back
        // to the opposite side, then to the outermost slot.
        let prefer_inner = match direction {
            Direction::FromLeft => has_right,
            Direction::FromRight => has_left,
            // §28.6: with runs on both sides, `Absolute` ties to the left.
            Direction::Absolute => has_left || has_right,
        };

        if prefer_inner {
            candidates
                .into_iter()
                .max_by_key(|slot| slot.context_path.len())
        } else {
            candidates
                .into_iter()
                .min_by_key(|slot| slot.context_path.len())
        }
    }

    /// The slot for a source offset, for coming back from raw mode.
    pub fn slot_for_offset(
        &self,
        offset: SourceOffset,
        direction: Direction,
    ) -> Option<&CaretSlot> {
        if let Some(exact) = self.slots.iter().find(|slot| slot.source_offset == offset) {
            return Some(exact);
        }

        // §27.21/m10: nearest is measured in bytes, ties going to the side of
        // the direction and then to the earlier slot.
        let target = offset.get();
        self.slots.iter().min_by_key(|slot| {
            let distance = slot.source_offset.get().abs_diff(target);
            let tie = match direction {
                Direction::FromRight => usize::from(slot.source_offset.get() > target),
                _ => usize::from(slot.source_offset.get() < target),
            };
            (distance, tie, slot.source_offset.get())
        })
    }

    /// Whether a slot may be used for editing. Every slot B.2 publishes may.
    ///
    /// The method exists so that the gate can assert it rather than assume it:
    /// a slot inside a protected region is a defect, not a state to handle.
    pub fn slot_is_editable(&self, id: CaretSlotId) -> bool {
        self.slots
            .iter()
            .find(|slot| slot.id == id)
            .is_some_and(|slot| !self.offset_is_protected(slot.source_offset))
    }

    /// Whether a grapheme's bytes are a line ending.
    fn is_line_ending(&self, range: SourceRange) -> bool {
        matches!(range.slice(&self.source), "\n" | "\r\n" | "\r")
    }

    /// The source this document was projected from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The heading level of a block, when it is one.
    pub fn heading_level(&self, block: BlockId) -> Option<u8> {
        match self.projection.node(self.block(block).node).kind {
            NodeKind::Heading(level) => Some(level),
            _ => None,
        }
    }

    /// The offset of the block's first caret, which is where its content
    /// begins — after any invisible prefix.
    pub fn first_caret_offset(&self, block: BlockId) -> Option<usize> {
        self.slots
            .iter()
            .find(|slot| slot.block == block)
            .map(|slot| slot.source_offset.get())
    }

    /// Whether a block may take part in a Join at all.
    ///
    /// B.4 grants Join to paragraphs and, in the forward direction only, to
    /// headings. Everything else — fenced code, comments, opaque regions,
    /// lists, tasks, quotes and callouts — is `SourceVisible` until B.7, and an
    /// absent capability is a denial (property 27).
    pub fn block_is_joinable(&self, block: BlockId) -> bool {
        let node = self.block(block).node;
        if !self
            .projection
            .effective_classification(node)
            .admits_interior_caret()
        {
            return false;
        }
        if !matches!(
            self.projection.node(node).kind,
            NodeKind::Paragraph | NodeKind::Heading(_)
        ) {
            return false;
        }
        // A paragraph whose entire content is an opaque region is a paragraph
        // by node kind and uneditable in fact: it publishes no caret. Joining
        // into it would move text next to bytes the editor refuses to touch,
        // so having somewhere for a caret to be is the real test.
        self.slots.iter().any(|slot| slot.block == block)
    }

    /// Whether `start..end` touches a lexeme that may never be partly rewritten.
    ///
    /// Clause (iii) of §28.2's block-scope rule, broadened by the review from
    /// "partially contained" to "intersected, wholly or partly": a heading's
    /// `# `, a task's completion metadata and any protected syntax may not end
    /// up inside a patch at all. Line endings are deliberately exempt — §28.4
    /// says so explicitly, because forbidding them would make every multi-block
    /// selection impossible and leave the rule that follows it dead code.
    pub fn range_touches_protected_lexeme(&self, block: BlockId, start: usize, end: usize) -> bool {
        let coverage = self.projection.node(self.block(block).node).coverage;
        self.projection.lexemes().iter().any(|lexeme| {
            if !coverage.covers(lexeme.source) {
                return false;
            }
            let protected = matches!(lexeme.kind, LexemeKind::BlockPrefix | LexemeKind::Metadata);
            let intersects = lexeme.source.start() < end && start < lexeme.source.end();
            protected && intersects
        })
    }

    /// Whether an offset falls inside a region no caret may enter.
    pub fn offset_is_protected(&self, offset: SourceOffset) -> bool {
        let node = self.projection.node_at(offset.get());
        !self
            .projection
            .effective_classification(node)
            .admits_interior_caret()
    }

    // -- construction -------------------------------------------------------

    fn build(&mut self) {
        // Top-level nodes are the blocks. The document node is the parent of
        // all of them and is never a block itself.
        let roots: Vec<NodeId> = self.projection.node(NodeId(0)).children.clone();

        for node in roots {
            let coverage = self.projection.node(node).coverage;
            let id = BlockId(self.blocks.len() as u32);
            let cells = self.cells_of(node);
            let content_end = cells
                .iter()
                .rev()
                .find(|(range, _)| !self.is_line_ending(*range))
                .map_or(coverage.start(), |(range, _)| range.end());

            self.blocks.push(VisualBlock {
                id,
                node,
                coverage,
                content_end,
                graphemes: cells.len(),
            });
            self.cells.push(cells);
        }

        self.build_slots();
    }

    /// The graphemes a block projects.
    ///
    /// A lexeme contributes its characters only when the reader is meant to see
    /// them. An invisible lexeme — a heading's `# `, a task's checkbox, a
    /// completion metadata suffix — contributes none and has no grapheme,
    /// which is exactly what makes it impossible to put a caret inside one.
    fn cells_of(&self, node: NodeId) -> Vec<(SourceRange, usize)> {
        let coverage = self.projection.node(node).coverage;
        let mut cells = Vec::new();

        for lexeme in self.projection.lexemes() {
            if !coverage.covers(lexeme.source) {
                continue;
            }
            if !projects_graphemes(lexeme.kind) {
                continue;
            }
            let text = lexeme.source.slice(&self.source);
            let base = lexeme.source.start();
            for (offset, grapheme) in text.grapheme_indices(true) {
                let start = base + offset;
                cells.push((
                    SourceRange::trusted(start, start + grapheme.len()),
                    UnicodeWidthStr::width(grapheme),
                ));
            }
        }

        cells
    }

    fn build_slots(&mut self) {
        let mut slots = Vec::new();

        for block in &self.blocks {
            let classification = self.projection.effective_classification(block.node);
            // A block the reader may not edit publishes no caret at all. Its
            // bytes are still projected — hiding them would imply they could be
            // edited — but there is nowhere in it for a cursor to be.
            if !classification.admits_interior_caret() {
                continue;
            }

            let cells = &self.cells[block.id.0 as usize];
            // A caret sits before each grapheme of content, plus once at the
            // end. A line ending is a break rather than content, so it carries
            // no caret of its own.
            let positions: Vec<usize> = cells
                .iter()
                .enumerate()
                .filter(|(_, (range, _))| !self.is_line_ending(*range))
                .map(|(index, _)| index)
                .collect();

            for (ordinal, index) in positions
                .iter()
                .copied()
                .map(Some)
                .chain(std::iter::once(None))
                .enumerate()
            {
                let offset = match index {
                    Some(index) => cells[index].0.start(),
                    None => block.content_end,
                };
                let index = index.unwrap_or(cells.len());
                let _ = ordinal;
                let offset = SourceOffset::trusted(offset);
                // The end-of-block caret can coincide with the last content
                // caret when the block ends in a line ending; one boundary is
                // one caret.
                if self.offset_is_protected(offset) {
                    continue;
                }
                // A boundary is a caret only when a grapheme the reader may
                // edit touches it. The end of a block whose whole content is
                // opaque has an offset outside the opaque range — half-open
                // ranges make it so — and would otherwise become a caret
                // floating at the end of something uneditable. Requiring an
                // editable neighbour is the fail-closed reading, and it keeps
                // `<x>abc</x>` with no caret at all.
                // Line endings are skipped when looking for a neighbour: they
                // are breaks, not content, and an editable line ending at the
                // end of a wholly opaque block would otherwise smuggle a caret
                // back in through the side door.
                let previous = cells[..index]
                    .iter()
                    .rev()
                    .find(|(range, _)| !self.is_line_ending(*range));
                let next = cells[index..]
                    .iter()
                    .find(|(range, _)| !self.is_line_ending(*range));
                let editable_neighbour =
                    [previous, next].into_iter().flatten().any(|(range, _)| {
                        !self.offset_is_protected(SourceOffset::trusted(range.start()))
                    });
                if !editable_neighbour {
                    continue;
                }
                if slots.last().is_some_and(|last: &CaretSlot| {
                    last.source_offset == offset && last.block == block.id
                }) {
                    continue;
                }
                slots.push(CaretSlot {
                    id: CaretSlotId(slots.len() as u32),
                    generation: self.generation,
                    block: block.id,
                    grapheme: GraphemeIndex(index),
                    source_offset: offset,
                    context_path: self.path_of(offset.get()),
                });
            }
        }

        self.slots = slots;
    }

    /// The nodes containing `byte`, outermost first, excluding the document.
    fn path_of(&self, byte: usize) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut current = Some(self.projection.node_at(byte));
        while let Some(id) = current {
            if id != NodeId(0) {
                path.push(id);
            }
            current = self.projection.node(id).parent;
        }
        path.reverse();
        path
    }
}

/// Whether a lexeme's bytes reach the screen as graphemes.
///
/// The invisible ones have a non-empty *source* range and an empty *visual*
/// one, which is the distinction §28.4 turns on: they exist, they are covered,
/// they carry no grapheme and therefore no caret can be inside them.
fn projects_graphemes(kind: LexemeKind) -> bool {
    match kind {
        // Structural markers the reader never sees.
        LexemeKind::BlockPrefix | LexemeKind::Metadata => false,
        // Everything else is either visible text or source shown literally
        // because its construct is still SourceVisible at this gate.
        LexemeKind::Text
        | LexemeKind::LineEnding
        | LexemeKind::MarkOpen
        | LexemeKind::MarkClose
        | LexemeKind::HtmlOpenTag
        | LexemeKind::HtmlCloseTag
        | LexemeKind::Comment
        | LexemeKind::Entity
        | LexemeKind::Escape
        | LexemeKind::FenceDelimiter
        | LexemeKind::CodeText
        | LexemeKind::CodeDelimiter
        | LexemeKind::LinkSyntax
        | LexemeKind::LinkDestination
        | LexemeKind::Opaque => true,
    }
}

/// Re-exported so callers need only one import for the classification lattice.
pub use crate::projection::Classification as VisualClassification;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_family_emoji_is_one_grapheme_and_many_scalars() {
        let source = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        assert!(source.chars().count() > 1, "several scalars");
        let document = VisualDocument::project(source, Generation::first());
        assert_eq!(document.graphemes_of(BlockId(0)).count(), 1);
    }

    #[test]
    fn an_opaque_block_shows_its_source_and_holds_no_caret() {
        let document = VisualDocument::project("<x>a</x>", Generation::first());
        let shown: String = document
            .graphemes_of(BlockId(0))
            .map(|cell| cell.text.to_owned())
            .collect();
        assert_eq!(shown, "<x>a</x>");
        assert!(document.slots().is_empty());
    }

    #[test]
    fn a_bookmark_is_refused_across_generations() {
        let generation = Generation::first();
        let offset = SourceOffset::trusted(0);
        let bookmark = RawBookmark::capture(generation, offset, offset);
        assert!(bookmark.restore(generation).is_some());
        assert!(bookmark.restore(generation.next()).is_none());
    }
}
