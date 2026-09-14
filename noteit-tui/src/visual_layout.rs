//! Where the visual editor puts things, and therefore what "the row above"
//! means.
//!
//! ## Why this module exists
//!
//! Up to R5 the visual editor had two different ideas of its own shape. The
//! renderer wrapped a block onto as many rows as the pane was narrow enough to
//! need, and navigation moved between *blocks* — so `↓` inside a paragraph
//! that took three rows skipped all three, and the "column" it tried to keep
//! was a grapheme index inside a block rather than a column on the screen. A
//! reader pressing `↓` watched the caret leave the paragraph they were in.
//!
//! So layout is computed once, here, and both callers use it: the renderer
//! draws these rows and navigation walks them. They cannot disagree, because
//! there is only one of them.
//!
//! ## Why it is per block and not per document
//!
//! Laying out the whole note on every frame would rebuild every block's cell
//! and caret map, which is exactly the cost [`VisualDocument`] is built lazily
//! to avoid — `slots()` is documented as "the expensive way to ask". A row is
//! identified by `(block index, row within block)` instead, which is a stable
//! coordinate that needs only its own block to be built. Drawing touches the
//! blocks on screen; moving the caret touches one block, or two when the move
//! crosses a boundary. Neither grows with the note.

use crate::source_map::{SourceOffset, SourceRange};
use crate::visual::{BlockId, VisualDocument};

/// One grapheme, placed.
#[derive(Debug, Clone)]
pub struct LayoutCell {
    /// The bytes it occupies in the source.
    pub source: SourceRange,
    /// What is drawn for it.
    pub text: String,
    /// How many terminal cells it takes.
    pub width: usize,
    /// The visual column it starts at.
    pub column: usize,
}

/// A legal caret position, placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutCaret {
    pub offset: SourceOffset,
    /// The visual column the caret is drawn at.
    pub column: usize,
}

/// One row of the visual editor: what a reader calls a line.
#[derive(Debug, Clone, Default)]
pub struct BlockRow {
    pub cells: Vec<LayoutCell>,
    /// Every caret that lands on this row, by ascending column.
    pub carets: Vec<LayoutCaret>,
    /// Columns already spent when the row begins — the block's glyph, on the
    /// first row of a list item, a task or a quote.
    pub indent: usize,
    /// Columns spent in total.
    pub width: usize,
}

impl BlockRow {
    /// The caret on this row nearest to `column`, preferring the one at or
    /// before it so that `↓` into a shorter row lands at its end.
    pub fn caret_near(&self, column: usize) -> Option<LayoutCaret> {
        let mut best: Option<LayoutCaret> = None;
        for caret in &self.carets {
            let better = match best {
                None => true,
                Some(current) => {
                    let (a, b) = (
                        caret.column.abs_diff(column),
                        current.column.abs_diff(column),
                    );
                    a < b || (a == b && caret.column <= column)
                }
            };
            if better {
                best = Some(*caret);
            }
        }
        best
    }
}

/// A block, as the rows it occupies at one width.
#[derive(Debug, Clone, Default)]
pub struct BlockLayout {
    pub rows: Vec<BlockRow>,
    /// The marker drawn before the first row when the block's own is hidden.
    pub glyph: Option<String>,
}

impl BlockLayout {
    /// Whether any caret may be placed in this block at all.
    ///
    /// A fenced code block, an unterminated comment and any other region still
    /// shown as source have rows on screen and no caret anywhere in them.
    /// Navigation must pass over them rather than land in them.
    pub fn has_caret(&self) -> bool {
        self.rows.iter().any(|row| !row.carets.is_empty())
    }

    /// The row `offset` is drawn on, and the column it is drawn at.
    pub fn locate(&self, offset: usize) -> Option<(usize, usize)> {
        self.rows.iter().enumerate().find_map(|(index, row)| {
            row.carets
                .iter()
                .find(|caret| caret.offset.get() == offset)
                .map(|caret| (index, caret.column))
        })
    }
}

/// Lays one block out at `width` columns.
///
/// This mirrors the renderer exactly, because the renderer draws what it
/// returns. Two rules carry the whole correspondence:
///
/// * a line ending is where a row *stops*, not something drawn — a caret owed
///   to it belongs at the end of the row it closes;
/// * a caret offset is taken by the first cell that starts at or after it,
///   because a caret before a hidden `<span>` tag is several bytes earlier
///   than the character it precedes.
pub fn layout_block(document: &VisualDocument, block: BlockId, width: usize) -> BlockLayout {
    let width = width.max(1);
    let glyph = block_glyph(document, block);
    let indent = glyph.as_deref().map_or(0, |text| text.chars().count());

    let slots = document.slots_of_block(block);
    let mut pending = slots.iter().peekable();
    let mut rows: Vec<BlockRow> = Vec::new();
    let mut row = BlockRow {
        indent,
        width: indent,
        ..BlockRow::default()
    };

    for cell in document.graphemes_of(block) {
        let start = cell.source.start();
        let mut due: Vec<SourceOffset> = Vec::new();
        while pending
            .peek()
            .is_some_and(|slot| slot.source_offset.get() <= start)
        {
            due.push(pending.next().expect("peeked").source_offset);
        }

        if matches!(cell.text, "\n" | "\r\n" | "\r") {
            // A caret owed to a line ending belongs at the end of the row it
            // closes — unless that row is already exactly full, in which case
            // the place the next character will be drawn is the row *after*
            // it. Putting it at column `width` instead drew it one cell past
            // the pane, where the terminal clipped it: the caret vanished and
            // typing looked as though it had stopped working.
            if !due.is_empty() && row.width >= width {
                rows.push(std::mem::take(&mut row));
            }
            for offset in due {
                row.carets.push(LayoutCaret {
                    offset,
                    column: row.width,
                });
            }
            rows.push(std::mem::take(&mut row));
            continue;
        }

        let span = cell.width.max(1);
        if row.width + span > width && row.width > 0 {
            rows.push(std::mem::take(&mut row));
        }
        for offset in due {
            row.carets.push(LayoutCaret {
                offset,
                column: row.width,
            });
        }
        row.cells.push(LayoutCell {
            source: cell.source,
            text: cell.text.to_owned(),
            width: span,
            column: row.width,
        });
        row.width += span;
    }

    // Whatever is left is the caret past the last grapheme: the end of a block
    // that does not end in a line ending, and the only caret an empty block
    // has at all.
    let remaining: Vec<SourceOffset> = pending.map(|slot| slot.source_offset).collect();
    if !remaining.is_empty() {
        if row.width >= width {
            rows.push(std::mem::take(&mut row));
        }
        for offset in remaining {
            row.carets.push(LayoutCaret {
                offset,
                column: row.width,
            });
        }
        rows.push(std::mem::take(&mut row));
    } else if !row.cells.is_empty() || rows.is_empty() {
        rows.push(std::mem::take(&mut row));
    }

    BlockLayout { rows, glyph }
}

/// The marker a structured block is drawn with, when its own is hidden.
///
/// Presentation derived from the node kind, which is the honest way round: the
/// source keeps its `- `, and the screen shows a bullet. It lives here rather
/// than in the renderer because it costs columns, and a column is layout.
pub fn block_glyph(document: &VisualDocument, block: BlockId) -> Option<String> {
    use crate::projection::NodeKind;

    let node = document.block(block).node;
    // Only when the marker is actually hidden: a block still shown as source
    // has its own marker on screen and must not get a second one.
    if !document.block_marker_is_hidden(block) {
        return None;
    }
    match document.projection().node(node).kind {
        NodeKind::ListItem => Some("• ".to_owned()),
        NodeKind::Task => Some(
            if document.task_is_done(block) {
                "☑ "
            } else {
                "☐ "
            }
            .to_owned(),
        ),
        NodeKind::Blockquote | NodeKind::Callout => Some("│ ".to_owned()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Rows as a coordinate
// ---------------------------------------------------------------------------

/// A row of the visual editor, named in a way that survives a reprojection of
/// everything except its own block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct RowCoord {
    pub block: usize,
    pub row: usize,
}

/// Where a caret is, on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaretPlace {
    pub coord: RowCoord,
    pub column: usize,
}

/// The row and column `offset` is drawn at, or `None` when no block claims it.
///
/// The open tail — the empty paragraph Enter leaves at the end of a note — is
/// deliberately not found here. It belongs to no block, and [`open_tail_coord`]
/// is where it is answered.
pub fn place(document: &VisualDocument, width: usize, offset: usize) -> Option<CaretPlace> {
    let block = document.block_containing(offset)?;
    let index = block.0 as usize;
    let (row, column) = layout_block(document, block, width).locate(offset)?;
    Some(CaretPlace {
        coord: RowCoord { block: index, row },
        column,
    })
}

/// The row the open tail is drawn on.
///
/// One past the last row of the last block, which is where the renderer draws
/// the caret of an empty paragraph — and, for a note that projects nothing at
/// all, the only row there is.
pub fn open_tail_coord(document: &VisualDocument) -> RowCoord {
    RowCoord {
        block: document.blocks().len(),
        row: 0,
    }
}

/// The number of rows a block takes, without keeping the layout.
pub fn row_count(document: &VisualDocument, block: usize, width: usize) -> usize {
    document
        .blocks()
        .get(block)
        .map(|block| layout_block(document, block.id, width).rows.len())
        .unwrap_or(1)
}
