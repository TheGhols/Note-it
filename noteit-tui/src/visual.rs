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

/// Which inline constructions the current sub-phase may edit.
///
/// `docs/tui.md` §26.5 makes every capability absent from the current matrix a
/// denial, and §26.14 requires B.5 to enable Strong, Emphasis, Strike and
/// InlineCode **individually** — "nenhum é liberado em lote". This struct is
/// that rule made executable: a gate constructs exactly the set it has proved,
/// and a construction whose flag is off stays `SourceVisible`, delimiters and
/// all, exactly as it was in B.4.
///
/// Underline, colour and highlight are deliberately absent. They are canonical
/// HTML, and §27.19 moved them to B.6 behind the pre-HTML gate — enabling them
/// here would be editing HTML one gate before the gate that authorises editing
/// HTML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub strong: bool,
    pub emphasis: bool,
    pub strike: bool,
    pub inline_code: bool,
    // Canonical HTML, granted by B.6 behind the pre-HTML gate (§27.19).
    pub underline: bool,
    pub color: bool,
    pub highlight: bool,
    // Structured blocks, granted by B.7 behind the pre-blocks gate.
    pub link: bool,
    pub list: bool,
    pub task: bool,
    pub blockquote: bool,
    pub callout: bool,
}

impl Capabilities {
    /// What B.1 through B.4 grant: nothing inline.
    pub const NONE: Self = Self {
        strong: false,
        emphasis: false,
        strike: false,
        inline_code: false,
        underline: false,
        color: false,
        highlight: false,
        link: false,
        list: false,
        task: false,
        blockquote: false,
        callout: false,
    };

    /// Everything B.5 ends up granting, once each has been proved on its own.
    pub const INLINE: Self = Self {
        strong: true,
        emphasis: true,
        strike: true,
        inline_code: true,
        underline: false,
        color: false,
        highlight: false,
        link: false,
        list: false,
        task: false,
        blockquote: false,
        callout: false,
    };

    /// Everything B.6 ends up granting: the inline marks plus the three
    /// constructions the graphical editor persists as HTML.
    pub const HTML: Self = Self {
        strong: true,
        emphasis: true,
        strike: true,
        inline_code: true,
        underline: true,
        color: true,
        highlight: true,
        link: false,
        list: false,
        task: false,
        blockquote: false,
        callout: false,
    };

    /// Everything B.7 ends up granting: the inline marks, the canonical HTML
    /// and the structured blocks.
    pub const BLOCKS: Self = Self {
        strong: true,
        emphasis: true,
        strike: true,
        inline_code: true,
        underline: true,
        color: true,
        highlight: true,
        link: true,
        list: true,
        task: true,
        blockquote: true,
        callout: true,
    };

    /// Whether this node's delimiters may be hidden and its text edited.
    fn allows(self, kind: &NodeKind) -> bool {
        match kind {
            NodeKind::Strong => self.strong,
            NodeKind::Emphasis => self.emphasis,
            // A composite run is one node with a single ownership (§26.5), so
            // it needs both of the marks it stands for.
            NodeKind::StrongEmphasis => self.strong && self.emphasis,
            NodeKind::Strike => self.strike,
            NodeKind::InlineCode => self.inline_code,
            NodeKind::Underline => self.underline,
            NodeKind::Color(_) => self.color,
            NodeKind::Highlight(_) => self.highlight,
            NodeKind::Link => self.link,
            NodeKind::ListItem => self.list,
            NodeKind::Task => self.task,
            NodeKind::Blockquote => self.blockquote,
            NodeKind::Callout => self.callout,
            _ => false,
        }
    }
}

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
    capabilities: Capabilities,
    /// One entry per node: the span of its content, delimiters excluded.
    ///
    /// Precomputed in a single pass. Deriving it per query made the seam walk
    /// `O(cells x nodes x lexemes)`, which stopped being a theoretical concern
    /// the first time the performance suite ran and hung.
    content_ranges: Vec<Option<(usize, usize)>>,
}

impl VisualDocument {
    /// Projects `source` for `generation`, granting nothing inline.
    ///
    /// This is what B.1 through B.4 see, and it stays the default so that a
    /// caller who has not thought about capabilities gets the conservative
    /// answer rather than the permissive one.
    pub fn project(source: &str, generation: Generation) -> Self {
        Self::project_with(source, generation, Capabilities::NONE)
    }

    /// Projects `source` granting exactly `capabilities`.
    pub fn project_with(source: &str, generation: Generation, capabilities: Capabilities) -> Self {
        let projection = project(source, generation);
        let mut document = Self {
            generation,
            source: source.to_owned(),
            projection,
            blocks: Vec::new(),
            cells: Vec::new(),
            slots: Vec::new(),
            capabilities,
            content_ranges: Vec::new(),
        };
        document.build();
        document
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// Whether `node` is a mark this document may edit.
    pub fn mark_is_editable(&self, node: NodeId) -> bool {
        self.capabilities.allows(&self.projection.node(node).kind)
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
        if let Some(exact) = self.slot_at_offset(offset) {
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

    /// Whether a caret may sit exactly at `offset`.
    ///
    /// Slots are built block by block and, within a block, in ascending
    /// grapheme order, so the list is sorted by source offset and this is a
    /// bisection. It is consulted once per keystroke, and a linear scan here
    /// made planning cost 16.5x for 10x the bytes — growth that outpaces the
    /// document is the shape P1 and P2 exist to keep out.
    pub fn slot_at_offset(&self, offset: SourceOffset) -> Option<&CaretSlot> {
        let index = self
            .slots
            .binary_search_by(|slot| slot.source_offset.cmp(&offset))
            .ok()?;
        self.slots.get(index)
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

    /// Whether this lexeme is syntax belonging to a mark the reader may edit.
    ///
    /// Only the delimiters of an enabled mark are hidden. The delimiters of a
    /// disabled one are still shown, and text is never hidden by anything.
    fn lexeme_is_hidden_syntax(&self, kind: LexemeKind, owner: Option<NodeId>) -> bool {
        if !matches!(
            kind,
            LexemeKind::MarkOpen
                | LexemeKind::MarkClose
                | LexemeKind::CodeDelimiter
                | LexemeKind::HtmlOpenTag
                | LexemeKind::HtmlCloseTag
                | LexemeKind::LinkSyntax
                | LexemeKind::LinkDestination
                | LexemeKind::BlockPrefix
                | LexemeKind::Metadata
        ) {
            return false;
        }
        let Some(owner) = owner else {
            return false;
        };
        // An ancestor that is not editable dominates: a Strong inside an opaque
        // region keeps its asterisks visible however enabled Strong is.
        if !self.ancestors_admit(owner) {
            return false;
        }
        let node = &self.projection.node(owner).kind;
        // Paragraphs and headings have been editable since B.3 and B.4, so
        // their structural prefixes are hidden without a capability flag of
        // their own. Everything else has to be granted.
        matches!(node, NodeKind::Paragraph | NodeKind::Heading(_)) || self.capabilities.allows(node)
    }

    /// Whether every ancestor of `node` admits an interior caret.
    fn ancestors_admit(&self, node: NodeId) -> bool {
        let mut current = self.projection.node(node).parent;
        while let Some(id) = current {
            if matches!(self.projection.node(id).kind, NodeKind::Opaque) {
                return false;
            }
            current = self.projection.node(id).parent;
        }
        true
    }

    /// The inline marks containing `byte`, outermost first.
    ///
    /// This is the "inline-mark path" the selection algebra of §26.6 is stated
    /// over. Block nodes are deliberately not in it: block scope is rule 0's
    /// business, and mixing the two is what let a selection across a heading
    /// look like plain text to every rule.
    pub fn mark_path(&self, byte: usize) -> Vec<NodeId> {
        let mut path: Vec<NodeId> = self
            .projection
            .nodes()
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    NodeKind::Strong
                        | NodeKind::Emphasis
                        | NodeKind::StrongEmphasis
                        | NodeKind::Strike
                        | NodeKind::InlineCode
                        | NodeKind::Underline
                        | NodeKind::Color(_)
                        | NodeKind::Highlight(_)
                ) && node.coverage.start() <= byte
                    && byte < node.coverage.end()
            })
            .map(|node| node.id)
            .collect();
        path.sort_by_key(|id| std::cmp::Reverse(self.projection.node(*id).coverage.len()));
        path
    }

    /// The visual content range of a mark: its bytes minus its delimiters.
    pub fn mark_content_range(&self, node: NodeId) -> Option<(usize, usize)> {
        self.content_ranges.get(node.0 as usize).copied().flatten()
    }

    /// Computes every node's content range in one pass over the lexemes.
    fn compute_content_ranges(&mut self) {
        let mut ranges: Vec<Option<(usize, usize)>> = vec![None; self.projection.nodes().len()];

        for lexeme in self.projection.lexemes() {
            if matches!(
                lexeme.kind,
                LexemeKind::MarkOpen
                    | LexemeKind::MarkClose
                    | LexemeKind::CodeDelimiter
                    | LexemeKind::HtmlOpenTag
                    | LexemeKind::HtmlCloseTag
                    | LexemeKind::BlockPrefix
                    | LexemeKind::Metadata
            ) {
                continue;
            }
            // A lexeme's content belongs to its owner and to every ancestor of
            // its owner, so the range widens on the way up the tree.
            let mut current = lexeme.owner;
            while let Some(node) = current {
                let slot = &mut ranges[node.0 as usize];
                *slot = Some(match *slot {
                    Some((start, end)) => (
                        start.min(lexeme.source.start()),
                        end.max(lexeme.source.end()),
                    ),
                    None => (lexeme.source.start(), lexeme.source.end()),
                });
                current = self.projection.node(node).parent;
            }
        }

        self.content_ranges = ranges;
    }

    /// The graphemes fully inside `start..end`, across every block.
    pub fn selected_cells(&self, start: usize, end: usize) -> Vec<SourceRange> {
        self.blocks
            .iter()
            // Only the blocks the range actually touches: a selection is small
            // and a document is not.
            .filter(|block| block.coverage.start() < end && start <= block.content_end)
            .flat_map(|block| {
                self.graphemes_of(block.id)
                    .filter(|cell| cell.source.start() >= start && cell.source.end() <= end)
                    .map(|cell| cell.source)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The projected text between two source offsets.
    ///
    /// `CopyVisualSelection` (§26.6) is the one command that never needs
    /// ownership: it does not mutate, so it succeeds for every valid selection
    /// — including one that crosses half a mark, which Delete, Replace and
    /// Format all refuse. Copying gives back exactly what the reader saw:
    /// hidden delimiters contribute nothing, and a `SourceVisible` region
    /// contributes its literal spelling.
    pub fn copy_visual(&self, anchor: SourceOffset, head: SourceOffset) -> Option<String> {
        let (start, end) = if anchor <= head {
            (anchor.get(), head.get())
        } else {
            (head.get(), anchor.get())
        };
        if end > self.source.len() {
            return None;
        }
        let mut copied = String::new();
        for block in &self.blocks {
            for cell in self.graphemes_of(block.id) {
                if cell.source.start() >= start && cell.source.end() <= end {
                    copied.push_str(cell.text);
                }
            }
        }
        Some(copied)
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

    /// Whether a block-level construction's capability has been granted.
    ///
    /// An opaque ancestor still dominates: a list inside an unknown element is
    /// not made editable by granting lists.
    fn block_is_granted(&self, node: NodeId) -> bool {
        if self.projection.node(node).parent.is_some_and(|parent| {
            !self
                .projection
                .effective_classification(parent)
                .admits_interior_caret()
        }) {
            return false;
        }
        self.capabilities.allows(&self.projection.node(node).kind)
    }

    /// Whether this block's own structural marker is hidden from the reader.
    pub fn block_marker_is_hidden(&self, block: BlockId) -> bool {
        let node = self.block(block).node;
        self.lexeme_is_hidden_syntax(LexemeKind::BlockPrefix, Some(node))
    }

    /// Whether a task's checkbox is ticked.
    pub fn task_is_done(&self, block: BlockId) -> bool {
        let coverage = self.projection.node(self.block(block).node).coverage;
        self.projection
            .lexemes()
            .iter()
            .find(|lexeme| lexeme.kind == LexemeKind::BlockPrefix && coverage.covers(lexeme.source))
            .is_some_and(|lexeme| {
                let text = lexeme.source.slice(&self.source);
                text.contains("[x]") || text.contains("[X]")
            })
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
    ///
    /// A mark the current sub-phase has enabled counts as editable even though
    /// the projector marks every mark `SourceVisible`: B.1 classifies what the
    /// grammar proved, and the capability matrix decides what may be done with
    /// it. Keeping those separate is why enabling Strong is one flag here and
    /// not a change to the parser.
    pub fn offset_is_protected(&self, offset: SourceOffset) -> bool {
        let node = self.projection.node_at(offset.get());
        if self
            .projection
            .effective_classification(node)
            .admits_interior_caret()
        {
            return false;
        }
        // Walk out to the nearest ancestor that decides the question. An
        // enabled mark makes its own content editable; anything above it that
        // is opaque or protected still dominates.
        let mut current = Some(node);
        while let Some(id) = current {
            let kind = &self.projection.node(id).kind;
            if matches!(kind, NodeKind::Opaque) {
                return true;
            }
            if self.capabilities.allows(kind) {
                // Its ancestors must still admit it.
                return self
                    .projection
                    .node(id)
                    .parent
                    .is_some_and(|parent| self.node_is_protected(parent));
            }
            if !matches!(
                kind,
                NodeKind::Strong
                    | NodeKind::Emphasis
                    | NodeKind::StrongEmphasis
                    | NodeKind::Strike
                    | NodeKind::InlineCode
                    | NodeKind::Underline
                    | NodeKind::Color(_)
                    | NodeKind::Highlight(_)
                    | NodeKind::Link
            ) {
                return true;
            }
            current = self.projection.node(id).parent;
        }
        true
    }

    fn node_is_protected(&self, node: NodeId) -> bool {
        if self
            .projection
            .effective_classification(node)
            .admits_interior_caret()
        {
            return false;
        }
        let kind = &self.projection.node(node).kind;
        if self.capabilities.allows(kind) {
            return self
                .projection
                .node(node)
                .parent
                .is_some_and(|parent| self.node_is_protected(parent));
        }
        true
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
            // The end of the block's text, including any trailing syntax that
            // is hidden but still owned — the outer caret after `**abc**` is
            // at byte 7, past the closing asterisks, not at 5 (§26.11).
            let lexemes = self.projection.lexemes();
            let from = lexemes.partition_point(|lexeme| lexeme.source.start() < coverage.start());
            let content_end = lexemes[from..]
                .iter()
                .take_while(|lexeme| lexeme.source.start() < coverage.end())
                .filter(|lexeme| {
                    coverage.covers(lexeme.source)
                        // A line ending is a break, not content, and the Core's
                        // completion metadata is a suffix the reader never sees.
                        // A caret past either would be a caret outside the text
                        // it is editing — after the `-->` of a task's marker,
                        // typing would land beyond the task altogether.
                        && !matches!(
                            lexeme.kind,
                            LexemeKind::LineEnding | LexemeKind::Metadata
                        )
                })
                .map(|lexeme| lexeme.source.end())
                .max()
                .unwrap_or(coverage.start());

            self.blocks.push(VisualBlock {
                id,
                node,
                coverage,
                content_end,
                graphemes: cells.len(),
            });
            self.cells.push(cells);
        }

        self.compute_content_ranges();
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

        // Bisect to the block's first lexeme and stop at its last. Scanning
        // every lexeme for every block is `O(blocks x lexemes)`, which on a
        // 200 KB note was most of the half-second the whole document took to
        // build — the fourth quadratic a performance gate has caught here, and
        // none of the four was visible by reading the code.
        let lexemes = self.projection.lexemes();
        let from = lexemes.partition_point(|lexeme| lexeme.source.start() < coverage.start());

        for lexeme in &lexemes[from..] {
            if lexeme.source.start() >= coverage.end() {
                break;
            }
            if !coverage.covers(lexeme.source) {
                continue;
            }
            if !projects_graphemes(lexeme.kind) {
                continue;
            }
            // A mark whose capability is granted becomes *projected*: its
            // delimiters stop reaching the screen and only its content does.
            // Until then it is SourceVisible, delimiters and all — hiding
            // syntax the editor cannot yet edit would tell the reader a lie
            // about what they can do with it.
            if self.lexeme_is_hidden_syntax(lexeme.kind, lexeme.owner) {
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
        let mut slots: Vec<CaretSlot> = Vec::new();

        for block in &self.blocks {
            // A structured block is `SourceVisible` in the projection until its
            // capability is granted; the capability is what turns its content
            // into somewhere a caret may be, while its marker stays protected.
            if !self
                .projection
                .effective_classification(block.node)
                .admits_interior_caret()
                && !self.block_is_granted(block.node)
            {
                continue;
            }

            let cells: Vec<(SourceRange, usize)> = self.cells[block.id.0 as usize]
                .iter()
                .copied()
                .filter(|(range, _)| !self.is_line_ending(*range))
                .collect();

            for boundary in 0..=cells.len() {
                // The seam region: everything between the visible grapheme
                // before this boundary and the visible grapheme after it. It
                // holds the hidden syntax, and every lexeme edge inside it is a
                // caret position of its own — the outer one before a `**` and
                // the inner one after it (§26.11, §26.4).
                let lo = match boundary.checked_sub(1).and_then(|index| cells.get(index)) {
                    Some((range, _)) => range.end(),
                    None => block.coverage.start(),
                };
                let hi = match cells.get(boundary) {
                    Some((range, _)) => range.start(),
                    None => block.content_end,
                };

                for offset in self.seam_offsets(lo, hi) {
                    let offset = SourceOffset::trusted(offset);
                    if self.offset_is_protected(offset) {
                        continue;
                    }
                    if !self.seam_is_legal(offset) {
                        continue;
                    }
                    if !self.boundary_has_editable_neighbour(&cells, boundary) {
                        continue;
                    }
                    if slots
                        .last()
                        .is_some_and(|last| last.source_offset == offset)
                    {
                        continue;
                    }
                    slots.push(CaretSlot {
                        id: CaretSlotId(slots.len() as u32),
                        generation: self.generation,
                        block: block.id,
                        grapheme: GraphemeIndex(boundary),
                        source_offset: offset,
                        context_path: self.path_of_seam(offset.get()),
                    });
                }
            }
        }

        self.slots = slots;
    }

    /// Every lexeme edge in `lo..=hi`, in order.
    fn seam_offsets(&self, lo: usize, hi: usize) -> Vec<usize> {
        if hi < lo {
            return Vec::new();
        }
        // Lexemes tile the source, so they are sorted by start: bisect to the
        // seam and walk the handful that fall inside it — usually none,
        // sometimes one delimiter. Scanning all of them was `O(cells x
        // lexemes)` and hung the performance suite outright.
        let lexemes = self.projection.lexemes();
        let from = lexemes.partition_point(|lexeme| lexeme.source.start() < lo);

        let mut offsets = vec![lo, hi];
        for lexeme in &lexemes[from..] {
            if lexeme.source.start() > hi {
                break;
            }
            if lexeme.source.end() <= hi {
                offsets.push(lexeme.source.end());
            }
        }
        offsets.sort_unstable();
        offsets.dedup();
        offsets
    }

    /// Whether a seam offset is a place a caret may be.
    ///
    /// Inline syntax gives two carets, one on each side: before `**` and after
    /// it. A *block* prefix does not — `# ` is structural, and a caret before
    /// it would be a caret that types outside the heading it is editing. So a
    /// seam at the start of a `BlockPrefix` or `Metadata` lexeme is refused
    /// while a seam at its end is allowed.
    fn seam_is_legal(&self, offset: SourceOffset) -> bool {
        let lexemes = self.projection.lexemes();
        let index = lexemes.partition_point(|lexeme| lexeme.source.start() <= offset.get());
        match index.checked_sub(1).and_then(|index| lexemes.get(index)) {
            // A caret may sit *after* protected structural bytes but never at
            // their start or inside them. `# ` is the reason for the rule and a
            // link's destination is the reason it has to be enforced: the
            // destination is invisible, so without this a caret would land in
            // the middle of a URL the reader cannot even see.
            Some(lexeme) => {
                !(matches!(
                    lexeme.kind,
                    LexemeKind::BlockPrefix | LexemeKind::Metadata | LexemeKind::LinkDestination
                ) && offset.get() < lexeme.source.end())
            }
            None => true,
        }
    }

    /// Whether an editable grapheme touches this boundary.
    fn boundary_has_editable_neighbour(
        &self,
        cells: &[(SourceRange, usize)],
        boundary: usize,
    ) -> bool {
        let previous = boundary.checked_sub(1).and_then(|index| cells.get(index));
        let next = cells.get(boundary);
        [previous, next]
            .into_iter()
            .flatten()
            .any(|(range, _)| !self.offset_is_protected(SourceOffset::trusted(range.start())))
    }

    /// The node path of a seam, counting a node as containing its own edges.
    ///
    /// A plain `node_at` would put the seam after `**abc**`'s closing
    /// asterisks outside the Strong, which is right, and the seam before them
    /// inside it, which is also right — this simply makes both answerable from
    /// one place.
    fn path_of_seam(&self, byte: usize) -> Vec<NodeId> {
        // Walk up from the innermost node covering the byte. Nesting is bounded
        // at 32, so this is bounded work; scanning every node was linear in the
        // document once per seam.
        let mut candidates = Vec::new();
        let mut current = Some(self.projection.node_at(byte));
        while let Some(id) = current {
            candidates.push(id);
            current = self.projection.node(id).parent;
        }
        // A node that *begins* exactly here contains the seam but not the byte,
        // so `node_at` misses it. It is the one extra candidate worth testing.
        let nodes = self.projection.nodes();
        let from = nodes.partition_point(|node| node.coverage.start() < byte);
        if let Some(node) = nodes.get(from).filter(|node| node.coverage.start() == byte) {
            candidates.push(node.id);
            let mut current = node.parent;
            while let Some(id) = current {
                candidates.push(id);
                current = self.projection.node(id).parent;
            }
        }

        let mut path = Vec::new();
        for node in candidates.iter().map(|id| self.projection.node(*id)) {
            if node.id == NodeId(0) {
                continue;
            }
            // For a mark, "inside" means inside its *content*, not its
            // coverage: the seam before `**` is at the coverage start and is
            // outside the mark, while the seam after `**` is at the content
            // start and is inside it. Using coverage here would make the two
            // slots indistinguishable and destroy the whole point of having
            // both.
            let inside = match self.mark_content_range(node.id) {
                Some((start, end)) if self.is_mark(node.id) => start <= byte && byte <= end,
                _ => node.coverage.start() <= byte && byte <= node.coverage.end(),
            };
            if inside && !path.contains(&node.id) {
                path.push(node.id);
            }
        }
        path.sort_by_key(|id| self.projection.node(*id).coverage.len());
        path.reverse();
        path
    }

    fn is_mark(&self, node: NodeId) -> bool {
        matches!(
            self.projection.node(node).kind,
            NodeKind::Strong
                | NodeKind::Emphasis
                | NodeKind::StrongEmphasis
                | NodeKind::Strike
                | NodeKind::InlineCode
                | NodeKind::Underline
                | NodeKind::Color(_)
                | NodeKind::Highlight(_)
        )
    }
}

/// Whether a lexeme's bytes reach the screen as graphemes.
///
/// The invisible ones have a non-empty *source* range and an empty *visual*
/// one, which is the distinction §28.4 turns on: they exist, they are covered,
/// they carry no grapheme and therefore no caret can be inside them.
fn projects_graphemes(kind: LexemeKind) -> bool {
    match kind {
        // Structural markers. Whether they are drawn depends on whether their
        // block's capability has been granted, which `lexeme_is_hidden_syntax`
        // decides; a construction the editor cannot yet edit shows its marker,
        // because hiding it would claim otherwise.
        LexemeKind::BlockPrefix | LexemeKind::Metadata => true,
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
