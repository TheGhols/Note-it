//! Flashcards, read out of the note rather than kept beside it.
//!
//! A card here is a **projection**. Nothing is stored: no `flashcards.json`,
//! no database, no identifier hidden in a comment, no front matter, and not one
//! byte written into the Markdown by anything in this file. What the reader
//! typed is the card, and this reads the document they typed it into.
//!
//! That is the whole design, and it is what makes editing work: change the
//! words and the card changes with them; delete the delimiter and the card
//! stops existing. There is no second copy to fall out of step, nothing to
//! migrate, and nothing to reconcile after a restore from a backup — a note
//! that comes back out of the trash brings its cards because it brings its
//! text.
//!
//! ## Why this reads the projection and not the source
//!
//! The graphical editor extracts from its own document tree, never from the
//! Markdown, and for a reason worth repeating: a regular expression over the
//! file cannot see that `::` is inside a fenced block, inside inline code,
//! inside a link destination or part of `https://`. Every one of those appears
//! in real notes.
//!
//! The terminal has no such tree, but since 5.0D.4B it has something with the
//! same knowledge: the lossless projection knows what is code, what is text and
//! what is opaque. So this asks the projection, exactly as the other editor
//! asks its document, and the two are held to the same fixture in
//! `tests/fixtures/flashcard-conformance.json`.

use crate::projection::{LexemeKind, NodeKind};
use crate::source_map::Generation;
use crate::visual::{BlockId, Capabilities, VisualDocument};

/// One direction.
pub const BASIC_DELIMITER: &str = "::";
/// Both directions.
pub const REVERSIBLE_DELIMITER: &str = ":::";

/// How many directions a card is studied in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Basic,
    Reversible,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Reversible => "reversible",
        }
    }
}

/// Whether the card was written on one line or across blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Inline,
    Block,
}

impl Form {
    pub fn name(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Block => "block",
        }
    }
}

/// Which way round one review item runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Reverse => "reverse",
        }
    }
}

/// A card as it is written in the note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub front: String,
    pub back: String,
    pub mode: Mode,
    pub form: Form,
    /// Where the card begins in the source. Ordering, and nothing else.
    pub position: usize,
}

/// One thing to answer. A basic card is one of these; a reversible card is two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewItem {
    pub question: String,
    pub answer: String,
    pub direction: Direction,
    /// Index of the card this came from, in document order.
    pub source: usize,
}

/// How many cards are written, and how many questions they come to.
///
/// Both numbers, because five cards can be seven questions and a progress bar
/// that says "1 of 5" while there are seven to get through is lying about the
/// shorter half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub cards: usize,
    pub reviews: usize,
}

/// What stands in for something that is not text while the delimiters are
/// looked for.
///
/// One character per unit, so an index into the scanned string is an offset
/// into the block's content and the two never drift. It is not whitespace on
/// purpose: `[imagem]:: x` is not a card, for the same reason `A::B` is not.
const OPAQUE: char = '\0';

/// Every card in the note, in the order they are written.
pub fn extract(source: &str) -> Vec<Card> {
    let document = VisualDocument::project_with(source, Generation::first(), Capabilities::BLOCKS);
    let blocks: Vec<BlockId> = document.blocks().iter().map(|block| block.id).collect();

    let scanned: Vec<String> = blocks
        .iter()
        .map(|block| scan_text(&document, *block))
        .collect();

    let mut cards = Vec::new();
    let mut consumed = vec![false; blocks.len()];

    // Two forms, and the block form is decided first. A marker paragraph takes
    // the blocks on either side, and those blocks are then not read again as
    // cards of their own: a block cannot be both an answer and a card, and
    // deciding it once is what keeps the result the same however it is read.
    for index in 0..blocks.len() {
        let Some(mode) = marker_mode(&document, blocks[index], &scanned[index]) else {
            continue;
        };
        let (Some(before), Some(after)) = (index.checked_sub(1), index.checked_add(1)) else {
            continue;
        };
        if after >= blocks.len() {
            continue;
        }
        // A marker is never a side: `A / :: / ::: / B` is one reader's typo,
        // not a card whose answer is three colons.
        if marker_mode(&document, blocks[before], &scanned[before]).is_some()
            || marker_mode(&document, blocks[after], &scanned[after]).is_some()
        {
            continue;
        }

        let front = scanned[before].trim().to_owned();
        let back = scanned[after].trim().to_owned();
        if !has_meaning(&front) || !has_meaning(&back) {
            continue;
        }

        cards.push(Card {
            front,
            back,
            mode,
            form: Form::Block,
            position: document.block(blocks[before]).coverage.start(),
        });
        consumed[before] = true;
        consumed[index] = true;
        consumed[after] = true;
    }

    for (index, block) in blocks.iter().enumerate() {
        if consumed[index] {
            continue;
        }
        // A fenced block is code. Everything in it is code, delimiters included.
        if matches!(
            document.projection().node(document.block(*block).node).kind,
            NodeKind::CodeBlock | NodeKind::Opaque | NodeKind::ThematicBreak
        ) {
            continue;
        }

        let hits = delimiters_in(&scanned[index]);
        // Exactly one, or nothing. `A :: B :: C` has two readings and no way to
        // choose between them, and guessing at one is worse than declining.
        if hits.len() != 1 {
            continue;
        }
        let hit = &hits[0];

        let front = scanned[index][..hit.start].trim().to_owned();
        let back = scanned[index][hit.end..].trim().to_owned();
        if !has_meaning(&front) || !has_meaning(&back) {
            continue;
        }

        cards.push(Card {
            front,
            back,
            mode: hit.mode,
            form: Form::Inline,
            position: document.block(*block).coverage.start(),
        });
    }

    cards.sort_by_key(|card| card.position);
    cards
}

/// The cards expanded into the things actually answered.
///
/// A reversible card puts both of its directions where it is written rather
/// than collecting them at the end, so studying follows the note.
pub fn review_items(cards: &[Card]) -> Vec<ReviewItem> {
    let mut items = Vec::new();
    for (index, card) in cards.iter().enumerate() {
        items.push(ReviewItem {
            question: card.front.clone(),
            answer: card.back.clone(),
            direction: Direction::Forward,
            source: index,
        });
        if card.mode == Mode::Reversible {
            items.push(ReviewItem {
                question: card.back.clone(),
                answer: card.front.clone(),
                direction: Direction::Reverse,
                source: index,
            });
        }
    }
    items
}

pub fn count(cards: &[Card]) -> Counts {
    Counts {
        cards: cards.len(),
        reviews: cards
            .iter()
            .map(|card| if card.mode == Mode::Reversible { 2 } else { 1 })
            .sum(),
    }
}

/// One block as a string the delimiters can be looked for in.
///
/// Text carrying code is masked rather than included, which is how
/// `` `A :: B` `` stays a sentence about a delimiter instead of becoming a
/// card. Structural syntax — a heading's hashes, a list's bullet, a mark's
/// asterisks, a link's destination — contributes nothing, exactly as it
/// contributes nothing to the other editor's text content.
fn scan_text(document: &VisualDocument, block: BlockId) -> String {
    let coverage = document
        .projection()
        .node(document.block(block).node)
        .coverage;
    let source = document.source();
    let mut text = String::new();

    for lexeme in document.projection().lexemes() {
        if !coverage.covers(lexeme.source) {
            continue;
        }
        let slice = lexeme.source.slice(source);
        match lexeme.kind {
            LexemeKind::Text | LexemeKind::Entity | LexemeKind::Escape => text.push_str(slice),
            // A line ending is whitespace, and the reader who put `::` on its
            // own line inside one paragraph meant the same as the one who used
            // two.
            LexemeKind::LineEnding => text.push('\n'),
            // Code, and anything the grammar would not interpret, stands in as
            // opaque: present, sized, and never a delimiter.
            LexemeKind::CodeText | LexemeKind::Opaque | LexemeKind::Comment => {
                text.extend(std::iter::repeat_n(OPAQUE, slice.chars().count()));
            }
            // Structural syntax contributes nothing at all.
            LexemeKind::MarkOpen
            | LexemeKind::MarkClose
            | LexemeKind::CodeDelimiter
            | LexemeKind::BlockPrefix
            | LexemeKind::Metadata
            | LexemeKind::HtmlOpenTag
            | LexemeKind::HtmlCloseTag
            | LexemeKind::FenceDelimiter
            | LexemeKind::LinkSyntax
            | LexemeKind::LinkDestination => {}
        }
    }

    text
}

/// A paragraph that is nothing but a delimiter, and which delimiter it is.
///
/// `None` for everything else, including a paragraph holding a delimiter and
/// anything at all besides. Only a top-level paragraph is ever asked, so a `::`
/// inside a quote or a list item is not a marker.
fn marker_mode(document: &VisualDocument, block: BlockId, scanned: &str) -> Option<Mode> {
    if document.projection().node(document.block(block).node).kind != NodeKind::Paragraph {
        return None;
    }
    match scanned.trim() {
        REVERSIBLE_DELIMITER => Some(Mode::Reversible),
        BASIC_DELIMITER => Some(Mode::Basic),
        _ => None,
    }
}

struct Hit {
    start: usize,
    end: usize,
    mode: Mode,
}

/// Every delimiter in one block, as byte offsets into its scanned text.
///
/// Runs of colons are matched **whole**, which is the longest-match rule stated
/// once rather than as an ordering between two patterns: `:::` is a run of
/// three and is never read as `::` followed by `:`, and `::::` is a run of four
/// and is nothing at all. A single colon is left alone, so `12:30` and
/// `https://example.com` are the times and addresses they are.
///
/// Whitespace on both sides is required. `namespace::method` and `A::B` are
/// ordinary technical writing, and a note is full of it; demanding the spaces
/// costs the reader nothing and stops the detector reaching into code they
/// happened to paste. The edges of the block count as whitespace.
fn delimiters_in(text: &str) -> Vec<Hit> {
    let bytes = text.as_bytes();
    let mut hits = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b':' {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index] == b':' {
            index += 1;
        }
        let length = index - start;
        if length != 2 && length != 3 {
            continue;
        }

        let before = text[..start].chars().next_back().unwrap_or(' ');
        let after = text[index..].chars().next().unwrap_or(' ');
        if before.is_whitespace() && after.is_whitespace() {
            hits.push(Hit {
                start,
                end: index,
                mode: if length == 3 {
                    Mode::Reversible
                } else {
                    Mode::Basic
                },
            });
        }
    }

    hits
}

/// Whether a side has anything on it.
///
/// Text is content when it is not only whitespace, and the opaque stand-in is
/// not content: a side holding only masked code draws nothing.
fn has_meaning(side: &str) -> bool {
    side.chars().any(|c| !c.is_whitespace() && c != OPAQUE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_of_four_colons_is_nothing_at_all() {
        assert!(extract("Pergunta :::: Resposta").is_empty());
    }

    #[test]
    fn a_single_colon_is_a_time_or_an_address() {
        assert!(extract("12:30 é a hora").is_empty());
        assert!(extract("https://example.com/x").is_empty());
    }

    #[test]
    fn nothing_is_written_into_the_note() {
        // The whole design in one assertion: extraction is a read.
        let source = "Pergunta :: Resposta";
        let before = source.to_owned();
        let cards = extract(source);
        assert_eq!(cards.len(), 1);
        assert_eq!(source, before);
    }
}
