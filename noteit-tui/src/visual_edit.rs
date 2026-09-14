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
    /// The character would have to be written as an HTML entity, and the
    /// projection cannot yet put a caret after one that sits immediately before
    /// a closing tag — so typing it into a styled run would scramble the text
    /// that follows. Refusing says so instead; the Markdown editor takes it.
    EntityInStyledRun,
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
    /// Insert text that carries the colour and highlight the reader has armed.
    ///
    /// The style is part of the same transaction as the character, so one
    /// keystroke stays one undo step and the projection is never asked to
    /// describe a half-styled document.
    InsertStyled {
        at: SourceOffset,
        text: String,
        color: Option<String>,
        highlight: Option<String>,
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
    /// B.6: set or clear the text colour of a selection. `None` clears.
    SetColor {
        anchor: SourceOffset,
        head: SourceOffset,
        color: Option<String>,
    },
    /// B.6: set or clear the highlight of a selection. `None` clears.
    SetHighlight {
        anchor: SourceOffset,
        head: SourceOffset,
        color: Option<String>,
    },
    /// B.6: add or remove `<u>` around a selection.
    ToggleUnderline {
        anchor: SourceOffset,
        head: SourceOffset,
    },
    /// B.7: tick or untick the task whose text contains this caret.
    ToggleTask {
        at: SourceOffset,
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
        VisualCommand::InsertStyled {
            at,
            text,
            color,
            highlight,
        } => insert_styled(document, at, &text, color.as_deref(), highlight.as_deref()),
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
        VisualCommand::SetColor {
            anchor,
            head,
            color,
        } => format(document, anchor, head, Formatting::Color(color)),
        VisualCommand::SetHighlight {
            anchor,
            head,
            color,
        } => format(document, anchor, head, Formatting::Highlight(color)),
        VisualCommand::ToggleUnderline { anchor, head } => {
            format(document, anchor, head, Formatting::Underline)
        }
        VisualCommand::ToggleTask { at } => toggle_task(document, at),
    }
}

enum Side {
    Before,
    After,
}

/// Whether a caret may sit at `offset` at all.
fn caret_is_legal(document: &VisualDocument, offset: SourceOffset) -> bool {
    document.slot_at_offset(offset).is_some()
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
    // A blank document, and the empty paragraph at the end of any note, have
    // no block and so no slot either — but they have nothing to protect, and
    // refusing here is what made a new note impossible to type into and put
    // the line after Enter back on the line before it. Every other position
    // still needs a real caret slot.
    if !caret_is_legal(document, at) && !document.is_open_tail(at.get()) {
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

/// `insert`, with the reader's armed colour and highlight applied.
///
/// The wrapper is written only when the caret is not already inside one that
/// says the same thing — which is what makes typing a word cost one `<span>`
/// rather than one per character — and the caret is left *inside* it, so the
/// next keystroke continues in the same run.
fn insert_styled(
    document: &VisualDocument,
    at: SourceOffset,
    text: &str,
    color: Option<&str>,
    highlight: Option<&str>,
) -> Result<SourceTransaction, Refusal> {
    let (have_color, have_highlight) = document.inline_colours_at(at.get());
    let needed_color = color.filter(|wanted| have_color.as_deref() != Some(*wanted));
    let needed_highlight = highlight.filter(|wanted| have_highlight.as_deref() != Some(*wanted));

    // `<`, `>` and `&` are text when a reader types them, and the canonical
    // source spells them as entities — which is what the Markdown editor does
    // while a style is active. The visual editor cannot follow it there yet:
    // the projection gives an entity that sits immediately before a closing tag
    // no caret after it, so the next character lands at the start of the run and
    // the text comes out scrambled. Both alternatives were worse than saying so
    // — a raw `<` inside a `<span>` makes the projection read the rest of the
    // run as a tag and silently drop every further keystroke.
    //
    // The refusal is only for a styled run. Unstyled typing is not escaped here
    // and never was: it is the Markdown editor's plain path, character for
    // character.
    let styled = color.is_some() || highlight.is_some();
    if styled && crate::formatting::escaped_typed(text) != text {
        return Err(Refusal::EntityInStyledRun);
    }

    if needed_color.is_none() && needed_highlight.is_none() {
        // Already inside everything the reader asked for: the text inherits it,
        // and adding a second identical wrapper would only make the source
        // larger and the undo history longer.
        return insert(document, at, text);
    }

    if at.get() > document.source_len() {
        return Err(Refusal::InvalidPosition);
    }
    if document.offset_is_protected(at) {
        return Err(Refusal::ProtectedRegion);
    }
    if !caret_is_legal(document, at) && !document.is_open_tail(at.get()) {
        return Err(Refusal::ProtectedRegion);
    }

    let (prefix, suffix) = crate::formatting::combined_wrapper(needed_color, needed_highlight);
    let replacement = format!("{prefix}{text}{suffix}");

    let range = SourceRange::trusted(at.get(), at.get());
    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![SourcePatch { range, replacement }],
        envelope: range,
        resulting_cursor: at.get() + prefix.len() + text.len(),
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

    // Rules 3 and 4 (§27.18, §28.3): the inline-mark algebra.
    check_mark_boundaries(document, start.get(), end.get())?;

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

// -- B.7: structured blocks --------------------------------------------------

/// Ticks or unticks a task's checkbox, and touches nothing else.
///
/// The patch is the single character inside the brackets. The marker, the
/// text and the Core's completion metadata are all outside it, so a toggle
/// cannot reword the task or forge a completion date — and the transaction
/// goes through the same `Draft` and the same revision as every other edit
/// (§26.15.22).
fn toggle_task(document: &VisualDocument, at: SourceOffset) -> Result<SourceTransaction, Refusal> {
    use crate::projection::{LexemeKind, NodeKind};

    let Some(block) = block_of(document, at) else {
        return Err(Refusal::InvalidPosition);
    };
    let node = document.block(block).node;
    if document.projection().node(node).kind != NodeKind::Task {
        return Err(Refusal::MissingCapability);
    }
    if !document.mark_is_editable(node) && !document.block_is_joinable(block) {
        return Err(Refusal::ProtectedRegion);
    }

    // The checkbox lives in the block's prefix lexeme: `- [ ] ` or `- [x] `.
    let coverage = document.projection().node(node).coverage;
    let prefix = document
        .projection()
        .lexemes()
        .iter()
        .find(|lexeme| lexeme.kind == LexemeKind::BlockPrefix && coverage.covers(lexeme.source))
        .ok_or(Refusal::MissingCapability)?;

    let text = prefix.source.slice(document.source());
    let open = text.find('[').ok_or(Refusal::MissingCapability)?;
    let state = prefix.source.start() + open + 1;
    let current = document.source()[state..]
        .chars()
        .next()
        .ok_or(Refusal::InvalidPosition)?;

    let replacement = match current {
        ' ' => "x",
        'x' | 'X' => " ",
        _ => return Err(Refusal::MissingCapability),
    };

    let range = SourceRange::trusted(state, state + current.len_utf8());
    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![SourcePatch {
            range,
            replacement: replacement.to_owned(),
        }],
        envelope: range,
        resulting_cursor: at.get(),
    })
}

// -- B.6: canonical HTML formatting ------------------------------------------

/// What a formatting command is asking for.
enum Formatting {
    Color(Option<String>),
    Highlight(Option<String>),
    Underline,
}

/// Applies or clears one canonical HTML wrapper over a selection.
///
/// The whole of §26.7's precedence is here, in order: preserve every byte
/// outside the approved envelope; choose the smallest node that must change;
/// normalise only inside it; never widen it to tidy equivalent syntax
/// elsewhere. The envelope is the selection itself when a wrapper is being
/// added, and the wrapper's own span when one is being removed — in neither
/// case does it reach a sibling.
fn format(
    document: &VisualDocument,
    anchor: SourceOffset,
    head: SourceOffset,
    formatting: Formatting,
) -> Result<SourceTransaction, Refusal> {
    let (start, end) = if anchor <= head {
        (anchor, head)
    } else {
        (head, anchor)
    };
    if start == end {
        // §26.8: an empty selection is not a FormatSelection. Changing only the
        // active typing style creates no patch, no history and no pending edit.
        return Err(Refusal::NothingToDo);
    }

    // A colour reaches an HTML attribute, so it may only ever be one of the
    // spellings this project writes. Anything else is refused rather than
    // escaped: the palette is a closed list, and a value that is not in it is
    // not a colour somebody chose.
    let colour = match &formatting {
        Formatting::Color(Some(value)) => Some((value.as_str(), crate::formatting::TEXT_COLORS)),
        Formatting::Highlight(Some(value)) => {
            Some((value.as_str(), crate::formatting::HIGHLIGHT_COLORS))
        }
        _ => None,
    };
    if let Some((value, palette)) = colour {
        if !palette.iter().any(|(_, hex)| *hex == value) {
            return Err(Refusal::MissingCapability);
        }
    }

    // The same checks every mutating selection passes: one block, nothing
    // protected, whole marks only.
    let start_block = block_of(document, start);
    if start_block != block_of(document, end) {
        return Err(Refusal::BlockBoundary);
    }
    let Some(block) = start_block else {
        return Err(Refusal::InvalidPosition);
    };
    if !range_is_editable(document, start.get(), end.get()) {
        return Err(Refusal::ProtectedRegion);
    }
    if !caret_is_legal(document, start) || !caret_is_legal(document, end) {
        return Err(Refusal::ProtectedRegion);
    }
    if document.range_touches_protected_lexeme(block, start.get(), end.get()) {
        return Err(Refusal::ProtectedRegion);
    }
    let kind = match &formatting {
        Formatting::Color(_) => WrapperKind::Color,
        Formatting::Highlight(_) => WrapperKind::Highlight,
        Formatting::Underline => WrapperKind::Underline,
    };

    // Is the selection already exactly the content of a wrapper of this kind?
    // If so the command removes it; otherwise it adds one.
    let existing = enclosing_wrapper(document, kind, start.get(), end.get());

    // The whole-leaf rule of §27.18 refuses a selection covering all of a
    // mark's content *unless the requested capability has an explicit cleanup
    // contract for that mark's own delimiters*. Clearing a wrapper is that
    // contract: it is the operation the rule was reserving the case for. So the
    // check runs for everything except the exact wrapper being removed, which
    // would otherwise be the one command that can never be issued.
    if existing.is_none() {
        check_mark_boundaries(document, start.get(), end.get())?;
    }

    match (&formatting, existing) {
        // Clearing, or toggling off: the envelope is the wrapper's own span and
        // the patches are its two tags. Its content is not rewritten at all.
        (Formatting::Color(None) | Formatting::Highlight(None), Some(node))
        | (Formatting::Underline, Some(node)) => unwrap_node(document, node),

        // Nothing of this kind here, and nothing asked for: a no-op.
        (Formatting::Color(None) | Formatting::Highlight(None), None) => Err(Refusal::NothingToDo),

        // Setting a value where one already exists: replace the wrapper rather
        // than nesting a second one, so the source stays something the
        // graphical editor would have written.
        (Formatting::Color(Some(value)) | Formatting::Highlight(Some(value)), Some(node)) => {
            let (open, close) = wrapper_text(kind, Some(value));
            let coverage = document.projection().node(node).coverage;
            let (content_start, content_end) = document
                .mark_content_range(node)
                .ok_or(Refusal::InvalidPosition)?;
            Ok(SourceTransaction {
                generation: document.generation(),
                patches: vec![
                    SourcePatch {
                        range: SourceRange::trusted(coverage.start(), content_start),
                        replacement: open,
                    },
                    SourcePatch {
                        range: SourceRange::trusted(content_end, coverage.end()),
                        replacement: close,
                    },
                ],
                envelope: coverage,
                resulting_cursor: content_start,
            })
        }

        // Adding one around the selection.
        (Formatting::Color(Some(value)) | Formatting::Highlight(Some(value)), None) => {
            wrap_selection(document, start, end, kind, Some(value.as_str()))
        }
        (Formatting::Underline, None) => wrap_selection(document, start, end, kind, None),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WrapperKind {
    Color,
    Highlight,
    Underline,
}

/// The canonical spelling of a wrapper, byte for byte what the graphical
/// editor writes. Sharing the spelling is what keeps a note editable in both.
fn wrapper_text(kind: WrapperKind, value: Option<&str>) -> (String, String) {
    match (kind, value) {
        (WrapperKind::Color, Some(value)) => {
            let (open, close) =
                crate::formatting::wrapper(crate::formatting::Kind::TextColor, value);
            (open, close.to_owned())
        }
        (WrapperKind::Highlight, Some(value)) => {
            let (open, close) =
                crate::formatting::wrapper(crate::formatting::Kind::Highlight, value);
            (open, close.to_owned())
        }
        _ => ("<u>".to_owned(), "</u>".to_owned()),
    }
}

/// The innermost wrapper of `kind` whose content is exactly `start..end`.
fn enclosing_wrapper(
    document: &VisualDocument,
    kind: WrapperKind,
    start: usize,
    end: usize,
) -> Option<crate::projection::NodeId> {
    use crate::projection::NodeKind;
    document.mark_path(start).into_iter().rev().find(|node| {
        let matches_kind = matches!(
            (&document.projection().node(*node).kind, kind),
            (NodeKind::Color(_), WrapperKind::Color)
                | (NodeKind::Highlight(_), WrapperKind::Highlight)
                | (NodeKind::Underline, WrapperKind::Underline)
        );
        matches_kind
            && document.mark_content_range(*node) == Some((start, end))
            && document.mark_is_editable(*node)
    })
}

/// Removes a wrapper's two tags and nothing else.
fn unwrap_node(
    document: &VisualDocument,
    node: crate::projection::NodeId,
) -> Result<SourceTransaction, Refusal> {
    let coverage = document.projection().node(node).coverage;
    let (content_start, content_end) = document
        .mark_content_range(node)
        .ok_or(Refusal::InvalidPosition)?;

    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![
            SourcePatch {
                range: SourceRange::trusted(coverage.start(), content_start),
                replacement: String::new(),
            },
            SourcePatch {
                range: SourceRange::trusted(content_end, coverage.end()),
                replacement: String::new(),
            },
        ],
        envelope: coverage,
        resulting_cursor: coverage.start(),
    })
}

/// Wraps a selection, leaving its content byte-identical.
fn wrap_selection(
    document: &VisualDocument,
    start: SourceOffset,
    end: SourceOffset,
    kind: WrapperKind,
    value: Option<&str>,
) -> Result<SourceTransaction, Refusal> {
    let (open, close) = wrapper_text(kind, value);
    let open_len = open.len();

    // Two patches at the two ends, strictly ordered and never touching, which
    // §27.11 requires: the content between them is not in any patch and so is
    // not rewritten.
    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![
            SourcePatch {
                range: SourceRange::trusted(start.get(), start.get()),
                replacement: open,
            },
            SourcePatch {
                range: SourceRange::trusted(end.get(), end.get()),
                replacement: close,
            },
        ],
        envelope: SourceRange::trusted(start.get(), end.get()),
        resulting_cursor: start.get() + open_len,
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
    if document.offset_is_protected(at) {
        return Err(Refusal::ProtectedRegion);
    }
    // Enter in a blank note, or in the empty paragraph at the end of one:
    // there is no block to split, so the break is the whole edit. `\n` rather
    // than a paragraph separator, because there is no paragraph here for the
    // second one to be separate from.
    if document.is_open_tail(at.get()) {
        let range = SourceRange::trusted(at.get(), at.get());
        return Ok(SourceTransaction {
            generation: document.generation(),
            patches: vec![SourcePatch {
                range,
                replacement: "\n".to_owned(),
            }],
            envelope: range,
            resulting_cursor: at.get() + 1,
        });
    }
    if !caret_is_legal(document, at) {
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

    // A break dropped in the middle of an inline mark leaves its delimiters on
    // opposite sides of a blank line: `**negr` and `ito aqui**` are not bold,
    // they are two paragraphs with stray asterisks, and `<span …>gustavo` and
    // `</span>` are a wrapper straddling a block boundary. The point is first
    // moved out of any mark it merely touches, and the marks it genuinely
    // divides are then closed before the break and reopened after it.
    let (insert_at, closers, openers) = split_through_marks(document, insert_at)?;
    let replacement = format!("{closers}{replacement}{openers}");

    let range = SourceRange::trusted(insert_at, insert_at);
    Ok(SourceTransaction {
        generation: document.generation(),
        patches: vec![SourcePatch {
            range,
            replacement: replacement.clone(),
        }],
        envelope: range,
        // §12: Enter at the start of a block leaves the caret in the new empty
        // paragraph above; everywhere else it follows the text it just moved —
        // and lands inside the reopened marks, not before them, so typing on
        // carries the colour it had.
        resulting_cursor: if at_start {
            insert_at
        } else {
            insert_at + replacement.len()
        },
    })
}

/// Where a block break really goes, and the delimiters that have to travel
/// with it.
///
/// Returns the adjusted offset, the text to emit before the break and the text
/// to emit after it. Every delimiter is the one the source itself spells, so a
/// colour is reopened with the exact attributes the note carries rather than
/// with the ones this build would have written.
fn split_through_marks(
    document: &VisualDocument,
    at: usize,
) -> Result<(usize, String, String), Refusal> {
    // Step out of every mark the point only touches, innermost first: leaving
    // one can put the point on the edge of the next one out.
    let mut point = at;
    while let Some((node, start)) = document
        .marks_containing(point, false)
        .into_iter()
        .filter_map(|node| {
            document
                .mark_content_range(node)
                .map(|(start, end)| (node, start, end))
        })
        .filter(|(_, start, end)| point == *start || point == *end)
        .min_by_key(|(_, start, end)| end - start)
        .map(|(node, start, _)| (node, start))
    {
        let coverage = document.projection().node(node).coverage;
        // Strictly outward each time, and a mark left behind cannot be chosen
        // again, so this terminates.
        point = if point == start {
            coverage.start()
        } else {
            coverage.end()
        };
    }

    // The point moved, so what it is now has to be asked again: stepping out of
    // a mark lands beside whatever follows it, and nothing may be inserted into
    // a protected region by arriving there sideways.
    if document.offset_is_protected(SourceOffset::trusted(point)) {
        return Err(Refusal::ProtectedRegion);
    }

    // Outermost first, which is the order the reopened marks must nest in.
    let dividing = document.marks_containing(point, true);
    let mut closers = String::new();
    let mut openers = String::new();
    for node in dividing.iter().rev() {
        // A mark whose delimiters cannot be read is one this cannot put back
        // together. Refusing keeps the source correct; splitting anyway would
        // not.
        let (_, close) = document
            .mark_delimiters(*node)
            .ok_or(Refusal::PartialMarkBoundary)?;
        closers.push_str(close);
    }
    for node in &dividing {
        let (open, _) = document
            .mark_delimiters(*node)
            .ok_or(Refusal::PartialMarkBoundary)?;
        openers.push_str(open);
    }

    Ok((point, closers, openers))
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

/// The inline-mark rules of §26.6, as corrected by §27.18 and §28.3.
///
/// A mutating selection that crosses *part* of a mark is always a `Refusal`
/// (property 26). The rules are evaluated in order and have no discretion:
///
/// - every selected grapheme must share exactly one inline-mark path;
/// - an empty path — plain text in one block — is permitted at any extent,
///   including the whole block, because a block has no delimiters to clean up;
/// - a non-empty path is permitted only for a *proper subset* of the mark's
///   visual content. Selecting all of it is the whole-leaf case, which needs an
///   explicit cleanup contract for the mark's delimiters, and no sub-phase up
///   to B.5 has one — so it refuses rather than leaving `****` behind, which
///   §26.11 says is not an empty Strong but literal, protected text.
fn check_mark_boundaries(
    document: &VisualDocument,
    start: usize,
    end: usize,
) -> Result<(), Refusal> {
    let cells = document.selected_cells(start, end);
    if cells.is_empty() {
        return Ok(());
    }

    let first = document.mark_path(cells[0].start());
    for cell in &cells[1..] {
        if document.mark_path(cell.start()) != first {
            return Err(Refusal::PartialMarkBoundary);
        }
    }

    let Some(innermost) = first.last().copied() else {
        // Plain text: §28.3/N1 permits any extent within one block.
        return Ok(());
    };

    if !document.mark_is_editable(innermost) {
        return Err(Refusal::ProtectedRegion);
    }

    let Some((content_start, content_end)) = document.mark_content_range(innermost) else {
        return Err(Refusal::PartialMarkBoundary);
    };
    if start <= content_start && end >= content_end {
        // The whole leaf. Permitted only with a cleanup contract, which does
        // not exist yet.
        return Err(Refusal::MissingCapability);
    }

    Ok(())
}

fn block_of(document: &VisualDocument, offset: SourceOffset) -> Option<BlockId> {
    document
        .blocks()
        .iter()
        .find(|block| block.coverage.start() <= offset.get() && offset.get() <= block.content_end)
        .map(|block| block.id)
}

/// Whether every grapheme in `start..end` may be edited.
///
/// Asks only about the graphemes in the selection. Walking every block to find
/// them made formatting cost 1.8 ms in a large note — work proportional to the
/// document for a decision about a handful of characters.
fn range_is_editable(document: &VisualDocument, start: usize, end: usize) -> bool {
    document
        .selected_cells(start, end)
        .into_iter()
        .all(|cell| !document.offset_is_protected(SourceOffset::trusted(cell.start())))
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
