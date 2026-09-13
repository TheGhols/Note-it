//! The lossless projection: source bytes in, a partition and a tree out.
//!
//! This is the foundation the visual editor stands on, and its whole promise
//! is in one property: [`Lexeme`]s tile the source exactly, so concatenating
//! their slices gives back the bytes that went in, to the byte. Everything
//! else — which run is bold, which region is protected, where a caret may go —
//! is derived from a partition that has already been proved not to lose
//! anything.
//!
//! ## Three layers, not one
//!
//! `docs/tui.md` §26.2 separates them and so does this module:
//!
//! - [`Lexeme`] is **physical**. A flat, ordered, gapless partition of every
//!   byte. Text, delimiters, tags, entities, line endings, block prefixes and
//!   opaque source all belong to exactly one lexeme. This is the only layer
//!   the losslessness property is stated over.
//! - [`Node`] is **semantic**. A tree that may nest, whose coverage ranges
//!   overlap only by ancestry, and which owns lexemes without copying a byte
//!   of them.
//! - The visual layer — runs, graphemes, carets — is derived on top, and does
//!   not exist in B.1 at all. There is no cursor here, and no dependency on a
//!   grapheme table, because nothing at this level can move a cursor.
//!
//! ## Why this is not the reader's parser
//!
//! `markdown.rs` and `inline.rs` render. They discard delimiters, decode
//! entities and stop a link destination at the first `)`, all of which are
//! right for showing a note and wrong for editing one — §26.16 rejects them as
//! a foundation explicitly. This module recognises less than they do and
//! throws away nothing, which is the opposite trade and the correct one here.
//!
//! ## Fail closed, always
//!
//! Anything this grammar cannot prove is preserved rather than guessed at. An
//! HTML element whose close tag is never proved protects from its `<` to the
//! end of the document (§26.10); an unterminated fence or comment does the
//! same; a delimiter run that is not unambiguous stays literal text. Refusing
//! to recognise costs a reader the visual editor for that region and nothing
//! else — the raw Markdown editor still opens it, and the bytes are still
//! there.

use crate::source_map::{Generation, SourceRange};

/// How deep the projector will nest before it gives up and protects (§26.10).
pub const NESTING_BUDGET: usize = 32;

/// HTML elements that never need a close tag (§26.10 step 3).
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// The entities the projector decodes. Anything else stays literal (§26.10).
const ENTITIES: &[&str] = &["&amp;", "&lt;", "&gt;", "&quot;", "&apos;", "&nbsp;"];

/// What a run of source bytes is, physically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexemeKind {
    /// Ordinary visible text.
    Text,
    /// `\n` or `\r\n`, kept exactly as written.
    LineEnding,
    /// A block's leading marker: `## `, `- `, `> `, `- [ ] `.
    BlockPrefix,
    /// The opening delimiter of an inline mark: `**`, `*`, `~~`.
    MarkOpen,
    /// The closing delimiter of an inline mark.
    MarkClose,
    /// A complete HTML open tag, attributes included.
    HtmlOpenTag,
    /// A complete HTML close tag.
    HtmlCloseTag,
    /// `<!-- … -->`, or an unterminated one.
    Comment,
    /// One of the six recognised entities, as a single unit.
    Entity,
    /// A backslash and the character it escapes.
    Escape,
    /// A ``` or ~~~ fence line.
    FenceDelimiter,
    /// Literal content of a fenced or inline code span.
    CodeText,
    /// A code span's backtick run.
    CodeDelimiter,
    /// `[`, `](`, `)` of a link.
    LinkSyntax,
    /// A link's destination, which is never editable as text.
    LinkDestination,
    /// Task completion metadata the reader never sees.
    Metadata,
    /// Source inside a region the grammar refuses to interpret.
    Opaque,
}

/// One run of bytes, and what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lexeme {
    pub id: LexemeId,
    pub source: SourceRange,
    pub kind: LexemeKind,
    /// The node that owns these bytes, if any owns them directly.
    pub owner: Option<NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LexemeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

/// What a node means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// The root. Always node 0.
    Document,
    Paragraph,
    Heading(u8),
    Strong,
    Emphasis,
    /// `***abc***`: one node owning one run, not two independent pairs
    /// (§26.5).
    StrongEmphasis,
    Strike,
    Underline,
    Color(String),
    Highlight(String),
    Link,
    InlineCode,
    CodeBlock,
    ListItem,
    Task,
    Blockquote,
    Callout,
    Comment,
    ThematicBreak,
    /// One entity from the allowlist: atomic, one grapheme, N bytes.
    Entity,
    /// Source the grammar would not prove. Dominates every descendant.
    Opaque,
}

/// Whether the grammar gave a region meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Recognition {
    /// The lossless grammar assigned semantics.
    Recognized,
    /// It did not. The bytes are preserved, uninterpreted, and this dominates
    /// every descendant.
    Opaque,
}

/// Whether the reader sees the source spelling or only its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Visibility {
    /// Delimiters and attributes are invisible; only content is projected.
    Projected,
    /// The literal spelling is projected, delimiters included.
    SourceVisible,
}

/// How much of a region may be mutated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProtectionLevel {
    /// The capabilities granted by the current sub-phase's matrix apply.
    Editable,
    /// Carets only at the ends; removed whole or not at all.
    Atomic,
    /// No interior caret and no mutation. Dominates every descendant.
    Protected,
}

/// The three orthogonal axes of §27.6, carried together.
///
/// They were named but never defined before R3, and the matrix wrote them
/// sometimes as pairs and sometimes as triples — which let an implementation
/// pass all 27 properties while making `**abc**` both caret-bearing and
/// unselectable. Three axes, always all three, removes the question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    pub recognition: Recognition,
    pub visibility: Visibility,
    pub protection: ProtectionLevel,
}

impl Classification {
    /// Ordinary editable content.
    pub const EDITABLE: Self = Self {
        recognition: Recognition::Recognized,
        visibility: Visibility::Projected,
        protection: ProtectionLevel::Editable,
    };

    /// Recognised, but shown and protected as source until its gate opens.
    pub const SOURCE_VISIBLE: Self = Self {
        recognition: Recognition::Recognized,
        visibility: Visibility::SourceVisible,
        protection: ProtectionLevel::Protected,
    };

    /// Not recognised at all. Implies SourceVisible and Protected, in itself
    /// and in everything below it.
    pub const OPAQUE: Self = Self {
        recognition: Recognition::Opaque,
        visibility: Visibility::SourceVisible,
        protection: ProtectionLevel::Protected,
    };

    /// A unit with carets only at its ends.
    pub const ATOMIC: Self = Self {
        recognition: Recognition::Recognized,
        visibility: Visibility::Projected,
        protection: ProtectionLevel::Atomic,
    };

    /// This classification with an ancestor's applied over it.
    ///
    /// Each axis takes the stricter value, which is what ancestor dominance
    /// means: an unknown wrapper makes everything below it opaque, and a
    /// protected one makes everything below it unmutable, however editable the
    /// descendant would be on its own.
    fn under(self, ancestor: Self) -> Self {
        let recognition = self.recognition.max(ancestor.recognition);
        let mut combined = Self {
            recognition,
            visibility: self.visibility.max(ancestor.visibility),
            protection: self.protection.max(ancestor.protection),
        };
        if recognition == Recognition::Opaque {
            // §27.6: Opaque implies SourceVisible and Protected.
            combined.visibility = Visibility::SourceVisible;
            combined.protection = ProtectionLevel::Protected;
        }
        combined
    }

    /// Whether a caret may ever exist inside this region.
    ///
    /// Property 9, with `SourceVisible` included as §27.20 requires.
    pub fn admits_interior_caret(self) -> bool {
        self.protection == ProtectionLevel::Editable
            && self.visibility == Visibility::Projected
            && self.recognition == Recognition::Recognized
    }
}

/// One semantic node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub kind: NodeKind,
    pub coverage: SourceRange,
    pub children: Vec<NodeId>,
    /// This node's own classification, before its ancestors are applied.
    pub classification: Classification,
}

/// A source text, partitioned and understood.
#[derive(Debug, Clone)]
pub struct Projection {
    generation: Generation,
    source_len: usize,
    lexemes: Vec<Lexeme>,
    nodes: Vec<Node>,
}

impl Projection {
    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub fn lexemes(&self) -> &[Lexeme] {
        &self.lexemes
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    /// The classification of `id` with every ancestor applied.
    ///
    /// This is the value that decides anything: a canonical colour span inside
    /// an unknown element is Opaque, however editable a colour span is on its
    /// own (§26.15.10).
    pub fn effective_classification(&self, id: NodeId) -> Classification {
        let mut classification = Classification::EDITABLE;
        let mut current = Some(id);
        while let Some(node) = current {
            let node = self.node(node);
            classification = classification.under(node.classification);
            current = node.parent;
        }
        classification
    }

    /// The innermost node whose coverage contains `byte`.
    pub fn node_at(&self, byte: usize) -> NodeId {
        let mut best = NodeId(0);
        let mut best_len = usize::MAX;
        for node in &self.nodes {
            if node.coverage.start() <= byte
                && byte < node.coverage.end()
                && node.coverage.len() <= best_len
            {
                best = node.id;
                best_len = node.coverage.len();
            }
        }
        best
    }
}

/// The byte ranges that are literal for the HTML scanner: step 0 of §26.10.
///
/// Fenced code, balanced same-line code spans and backslash escapes are
/// resolved **before** any HTML candidate, and a `<` inside one of them never
/// starts a tag. Without this rule a note holding a ```` ```html ```` block, or
/// a sentence saying `` Use `<div>` aqui ``, would make every byte from that
/// `<` to the end of the note opaque and uneditable — which is the opposite of
/// what a fence means.
///
/// Computed in one linear pass over the raw bytes, deliberately blind to HTML:
/// the precedence only means something if the thing with precedence is decided
/// first. The result is sorted and non-overlapping.
fn literal_regions(source: &str) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let bytes = source.as_bytes();
    let mut pos = 0usize;

    while pos < source.len() {
        let line_start = pos;
        let newline = source[pos..].find('\n').map(|index| pos + index);
        let line_end = newline.unwrap_or(source.len());
        let line = &source[line_start..line_end];
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = &trimmed[..3];
            // The fence owns everything to its closing line, or to EOF.
            let mut cursor = newline.map_or(source.len(), |index| index + 1);
            let mut end = source.len();
            while cursor < source.len() {
                let next_newline = source[cursor..].find('\n').map(|index| cursor + index);
                let next_end = next_newline.unwrap_or(source.len());
                if source[cursor..next_end].trim_start().starts_with(marker) {
                    end = next_end;
                    break;
                }
                match next_newline {
                    Some(index) => cursor = index + 1,
                    None => break,
                }
            }
            regions.push((line_start, end));
            pos = end;
            continue;
        }

        // Inside one line: escapes, then balanced backtick runs.
        let mut index = line_start;
        while index < line_end {
            match bytes[index] {
                b'\\' if index + 1 < line_end => {
                    let width = escape_width(&source[index..]);
                    if width > 0 {
                        regions.push((index, index + width));
                        index += width;
                        continue;
                    }
                    index += 1;
                }
                b'`' => {
                    let width = source[index..line_end]
                        .bytes()
                        .take_while(|byte| *byte == b'`')
                        .count();
                    match closing_run(&source[index + width..line_end], width) {
                        Some(offset) => {
                            let end = index + width + offset + width;
                            regions.push((index, end));
                            index = end;
                        }
                        // An unbalanced run is not a code span, and §27.3 says
                        // its `<` is a candidate again.
                        None => index += width,
                    }
                }
                _ => index += 1,
            }
        }

        pos = newline.map_or(source.len(), |index| index + 1);
    }

    regions
}

/// Where a run of exactly `width` `marker` bytes appears in `body`, counted
/// from its start, skipping runs of any other width.
///
/// This is how §28.5 decides whether a differing-width run between two
/// delimiters is a balanced pair of its own or an unterminated one.
fn closing_delimiter_run(body: &str, marker: char, width: usize) -> Option<usize> {
    let mut index = 0usize;
    while index < body.len() {
        let next = index + body[index..].find(marker)?;
        let run = body[next..].chars().take_while(|c| *c == marker).count();
        if run == width {
            return Some(next);
        }
        index = next + run;
    }
    None
}

/// Where a run of exactly `width` backticks closes `body`, counted from its
/// start. A run of a different length is content, not a terminator.
fn closing_run(body: &str, width: usize) -> Option<usize> {
    let mut index = 0usize;
    while index < body.len() {
        let next = body[index..].find('`')? + index;
        let run = body[next..]
            .bytes()
            .take_while(|byte| *byte == b'`')
            .count();
        if run == width {
            return Some(next);
        }
        index = next + run;
    }
    None
}

/// Projects `source` losslessly.
///
/// Total: every string projects, including an empty one, one that is not
/// Markdown at all and one that is actively hostile. There is no error return
/// because there is no input this refuses — only input it declines to
/// interpret, which comes back as `Opaque`.
pub fn project(source: &str, generation: Generation) -> Projection {
    let mut projector = Projector::new(source, generation);
    projector.run();
    projector.finish()
}

// ---------------------------------------------------------------------------

struct Projector<'a> {
    source: &'a str,
    generation: Generation,
    lexemes: Vec<Lexeme>,
    nodes: Vec<Node>,
    pos: usize,
    /// Fenced code, balanced code spans and escapes, from step 0.
    literals: Vec<(usize, usize)>,
}

impl<'a> Projector<'a> {
    fn new(source: &'a str, generation: Generation) -> Self {
        let mut projector = Self {
            source,
            generation,
            lexemes: Vec::new(),
            nodes: Vec::new(),
            pos: 0,
            literals: literal_regions(source),
        };
        // The document node always exists and always covers everything, so
        // `effective_protection` has a root to walk up to.
        projector.nodes.push(Node {
            id: NodeId(0),
            parent: None,
            kind: NodeKind::Document,
            coverage: SourceRange::trusted(0, source.len()),
            children: Vec::new(),
            classification: Classification::EDITABLE,
        });
        projector
    }

    fn finish(self) -> Projection {
        Projection {
            generation: self.generation,
            source_len: self.source.len(),
            lexemes: self.lexemes,
            nodes: self.nodes,
        }
    }

    fn len(&self) -> usize {
        self.source.len()
    }

    fn rest(&self) -> &'a str {
        &self.source[self.pos..]
    }

    /// Records `self.pos..end` as one lexeme and advances.
    ///
    /// An `end` at or behind the cursor is a no-op rather than a panic. It can
    /// only happen when a nested construct has already consumed those bytes —
    /// an HTML region that escaped its enclosing span — and in that case the
    /// bytes are already covered exactly once, which is the property that
    /// matters. The guards in `inline_until` are what stop it arising at all.
    fn emit(&mut self, end: usize, kind: LexemeKind, owner: Option<NodeId>) {
        if end <= self.pos {
            return;
        }
        let id = LexemeId(self.lexemes.len() as u32);
        self.lexemes.push(Lexeme {
            id,
            source: SourceRange::trusted(self.pos, end),
            kind,
            owner,
        });
        self.pos = end;
    }

    fn open_node(
        &mut self,
        parent: NodeId,
        kind: NodeKind,
        classification: Classification,
    ) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node {
            id,
            parent: Some(parent),
            kind,
            coverage: SourceRange::trusted(self.pos, self.pos),
            children: Vec::new(),
            classification,
        });
        self.nodes[parent.0 as usize].children.push(id);
        id
    }

    fn close_node(&mut self, id: NodeId) {
        let start = self.nodes[id.0 as usize].coverage.start();
        self.nodes[id.0 as usize].coverage = SourceRange::trusted(start, self.pos);
    }

    // -- block level --------------------------------------------------------

    fn run(&mut self) {
        while self.pos < self.len() {
            self.block();
        }
    }

    /// The end of the line starting at `self.pos`, not counting its ending.
    ///
    /// A `\r` immediately before the `\n` belongs to the ending, not to the
    /// line: CRLF is one line ending of two bytes (§26.3), and leaving the
    /// `\r` on the text would both mis-measure the line and split the ending
    /// across two lexemes.
    fn line_end(&self) -> usize {
        let end = match self.rest().find('\n') {
            Some(index) => self.pos + index,
            None => self.len(),
        };
        if end > self.pos && self.source.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end
        }
    }

    /// Consumes the line ending at `self.pos`, if that is where we are.
    fn line_ending(&mut self) {
        if self.rest().starts_with("\r\n") {
            self.emit(self.pos + 2, LexemeKind::LineEnding, None);
        } else if self.rest().starts_with('\n') {
            self.emit(self.pos + 1, LexemeKind::LineEnding, None);
        }
    }

    fn block(&mut self) {
        let line_end = self.line_end();
        let line = &self.source[self.pos..line_end];
        let trimmed = line.trim_start();
        let root = NodeId(0);

        // A `\r` alone before the ending is part of the line, not a blank.
        if trimmed.trim_end().is_empty() {
            self.emit(line_end, LexemeKind::Text, None);
            self.line_ending();
            return;
        }

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            self.fenced_code(line_end);
            return;
        }

        if line.starts_with("<!--") {
            self.comment_block();
            return;
        }

        if let Some(level) = heading_level(line) {
            let node = self.open_node(root, NodeKind::Heading(level), Classification::EDITABLE);
            // `### ` — the hashes and the single space that separates them
            // from the text. It is a prefix the reader never sees a caret in.
            self.emit(
                self.pos + level as usize + 1,
                LexemeKind::BlockPrefix,
                Some(node),
            );
            self.inline_until(line_end, node);
            self.close_node(node);
            self.line_ending();
            return;
        }

        if is_thematic_break(trimmed) {
            let node = self.open_node(
                root,
                NodeKind::ThematicBreak,
                Classification::SOURCE_VISIBLE,
            );
            self.emit(line_end, LexemeKind::BlockPrefix, Some(node));
            self.close_node(node);
            self.line_ending();
            return;
        }

        if let Some(prefix) = task_prefix(line) {
            let node = self.open_node(root, NodeKind::Task, Classification::SOURCE_VISIBLE);
            self.emit(self.pos + prefix, LexemeKind::BlockPrefix, Some(node));
            self.task_content(line_end, node);
            self.close_node(node);
            self.line_ending();
            return;
        }

        if let Some(prefix) = list_prefix(line) {
            let node = self.open_node(root, NodeKind::ListItem, Classification::SOURCE_VISIBLE);
            self.emit(self.pos + prefix, LexemeKind::BlockPrefix, Some(node));
            self.inline_until(line_end, node);
            self.close_node(node);
            self.line_ending();
            return;
        }

        if let Some(prefix) = quote_prefix(line) {
            let kind = if callout_marker(&line[prefix..]) {
                NodeKind::Callout
            } else {
                NodeKind::Blockquote
            };
            let node = self.open_node(root, kind, Classification::SOURCE_VISIBLE);
            self.emit(self.pos + prefix, LexemeKind::BlockPrefix, Some(node));
            self.inline_until(line_end, node);
            self.close_node(node);
            self.line_ending();
            return;
        }

        self.paragraph();
    }

    /// A run of consecutive non-blank lines that no other rule claimed.
    ///
    /// The loop re-checks every line because an HTML region opened on the
    /// first one may have swallowed several: after `inline_until` returns past
    /// `line_end`, the paragraph simply continues from wherever it now is.
    fn paragraph(&mut self) {
        let node = self.open_node(NodeId(0), NodeKind::Paragraph, Classification::EDITABLE);
        loop {
            let line_end = self.line_end();
            self.inline_until(line_end, node);
            self.line_ending();

            if self.pos >= self.len() {
                break;
            }
            let next_end = self.line_end();
            let next = &self.source[self.pos..next_end];
            if next.trim().is_empty() || starts_a_different_block(next) {
                break;
            }
        }
        self.close_node(node);
    }

    fn fenced_code(&mut self, first_line_end: usize) {
        let line = &self.source[self.pos..first_line_end];
        let trimmed = line.trim_start();
        let marker = &trimmed[..3];
        let node = self.open_node(
            NodeId(0),
            NodeKind::CodeBlock,
            Classification::SOURCE_VISIBLE,
        );

        self.emit(first_line_end, LexemeKind::FenceDelimiter, Some(node));
        self.line_ending();

        // Document-bounded: no close means the fence owns the rest of the
        // note (§26.10). A fence is never terminated by a blank line.
        while self.pos < self.len() {
            let line_end = self.line_end();
            let line = &self.source[self.pos..line_end];
            if line.trim_start().starts_with(marker) {
                self.emit(line_end, LexemeKind::FenceDelimiter, Some(node));
                self.line_ending();
                self.close_node(node);
                return;
            }
            self.emit(line_end, LexemeKind::CodeText, Some(node));
            self.line_ending();
        }
        self.close_node(node);
    }

    fn comment_block(&mut self) {
        let node = self.open_node(NodeId(0), NodeKind::Comment, Classification::SOURCE_VISIBLE);
        let end = match self.rest().find("-->") {
            Some(index) => self.pos + index + 3,
            None => self.len(),
        };
        self.emit(end, LexemeKind::Comment, Some(node));
        self.close_node(node);
        // Only when the comment really ended on its line is there an ending
        // left to take; an unterminated one has already consumed it.
        self.line_ending();
    }

    /// A task's text, with the Core's completion metadata kept protected.
    fn task_content(&mut self, line_end: usize, node: NodeId) {
        let text = &self.source[self.pos..line_end];
        let (completed, cleaned) = noteit_core::task::extract_completed_at(text);
        if completed.is_some() && cleaned.len() < text.len() {
            // The metadata is a suffix the reader never sees. Everything up to
            // it is ordinary inline content; the rest is protected source.
            let boundary = self.pos + cleaned.trim_end().len();
            if self.html_escapes(self.pos, boundary) {
                self.emit(boundary, LexemeKind::Opaque, Some(node));
            } else {
                self.inline_until(boundary, node);
            }
            self.emit(line_end, LexemeKind::Metadata, Some(node));
            return;
        }
        self.inline_until(line_end, node);
    }

    // -- inline level -------------------------------------------------------

    /// Scans inline content up to `end`.
    ///
    /// May finish *past* `end`: an HTML element that is not closed on this
    /// line owns everything to its proved close or to EOF, and that decision
    /// belongs to the grammar rather than to the line the scan started on.
    fn inline_until(&mut self, end: usize, parent: NodeId) {
        let mut text_from = self.pos;

        while self.pos < end {
            let rest = &self.source[self.pos..];
            let flush = |scanner: &mut Self, from: usize| {
                if scanner.pos > from {
                    let to = scanner.pos;
                    scanner.pos = from;
                    scanner.emit(to, LexemeKind::Text, Some(parent));
                }
            };

            if rest.starts_with('\\') {
                let width = escape_width(rest);
                if width > 0 {
                    flush(self, text_from);
                    self.emit(self.pos + width, LexemeKind::Escape, Some(parent));
                    text_from = self.pos;
                    continue;
                }
            }

            if rest.starts_with('&') {
                if let Some(entity) = ENTITIES.iter().find(|entity| rest.starts_with(**entity)) {
                    flush(self, text_from);
                    // §27.7: one grapheme, N bytes, carets only at its ends.
                    // The 5.0D.3 editor already writes these into notes, so an
                    // entity is a thing a reader meets and deletes.
                    let node = self.open_node(parent, NodeKind::Entity, Classification::ATOMIC);
                    self.emit(self.pos + entity.len(), LexemeKind::Entity, Some(node));
                    self.close_node(node);
                    text_from = self.pos;
                    continue;
                }
            }

            if rest.starts_with('<') {
                if let Some(region) = self.html_region(self.pos) {
                    flush(self, text_from);
                    self.html(region, parent);
                    text_from = self.pos;
                    continue;
                }
            }

            if rest.starts_with('`') {
                // The span was decided in step 0, so the scanner and the HTML
                // precedence rule cannot disagree about where code is.
                if let Some((_, region_end)) = self.literal_at(self.pos) {
                    let width = rest.bytes().take_while(|byte| *byte == b'`').count();
                    flush(self, text_from);
                    self.inline_code(width, region_end, parent);
                    text_from = self.pos;
                    continue;
                }
            }

            if rest.starts_with('[') {
                if let Some(link) = link_span(rest, end - self.pos)
                    .filter(|link| !self.html_escapes(self.pos, self.pos + link.total))
                {
                    flush(self, text_from);
                    self.link(link, parent);
                    text_from = self.pos;
                    continue;
                }
            }

            if rest.starts_with('*') || rest.starts_with('~') {
                let before = self.source[..self.pos]
                    .chars()
                    .next_back()
                    .filter(|c| *c != '\n');
                if let Some(mark) = mark_span(rest, end - self.pos, before)
                    .filter(|mark| !self.html_escapes(self.pos, self.pos + mark.total))
                {
                    flush(self, text_from);
                    self.mark(mark, parent);
                    text_from = self.pos;
                    continue;
                }
                // A run is *maximal* (§27.15). Stepping one byte on failure
                // would let the scanner start a new run in the middle of one —
                // reading the second asterisk of `**` as an opener — which is
                // exactly the ambiguity the procedure exists to remove. Skip
                // the whole run as literal text instead.
                let marker = rest.chars().next().unwrap_or('*');
                let run = rest.chars().take_while(|c| *c == marker).count();
                self.pos = (self.pos + run).min(end);
                continue;
            }

            // Ordinary text: step one whole character, never one byte, so a
            // lexeme can never begin inside a multi-byte sequence.
            let step = rest.chars().next().map_or(1, char::len_utf8);
            self.pos += step;
        }

        if self.pos > text_from {
            let to = self.pos;
            self.pos = text_from;
            self.emit(to, LexemeKind::Text, Some(parent));
        }
    }

    /// The literal region at or before `pos`, found by bisection.
    ///
    /// `literal_regions` produces them sorted and disjoint, so this is a binary
    /// search rather than a scan. That matters: these are consulted once per
    /// candidate byte, and a linear scan here turns the whole projection
    /// superlinear on exactly the documents §26.13 measures — it cost 31x for
    /// 10x the bytes before this was a bisection, and 10x after.
    fn literal_before(&self, pos: usize) -> Option<(usize, usize)> {
        let index = self.literals.partition_point(|(start, _)| *start <= pos);
        (index > 0).then(|| self.literals[index - 1])
    }

    /// The literal region that begins exactly at `pos`, if one does.
    fn literal_at(&self, pos: usize) -> Option<(usize, usize)> {
        self.literal_before(pos).filter(|(start, _)| *start == pos)
    }

    /// The literal region containing `pos`, if any.
    fn literal_covering(&self, pos: usize) -> Option<(usize, usize)> {
        self.literal_before(pos).filter(|(_, end)| pos < *end)
    }

    /// Whether an HTML region starting inside `from..to` reaches past `to`.
    ///
    /// An inline construct — a mark, a code span, a link — is bounded by its
    /// own delimiters. An unclosed tag inside one is not: §26.10 gives it
    /// everything to EOF. The two cannot both be true, so the construct is the
    /// one that loses: it stays literal text and the tag owns the region. That
    /// is fail-closed, and it is also what keeps the partition contiguous.
    fn html_escapes(&self, from: usize, to: usize) -> bool {
        let mut cursor = from;
        while cursor < to {
            let Some(index) = self.source[cursor..to].find('<') else {
                return false;
            };
            let at = cursor + index;
            match self.html_region(at) {
                Some(region) if region.end > to => return true,
                Some(region) => cursor = region.end.max(at + 1),
                None => cursor = at + 1,
            }
        }
        false
    }

    fn html(&mut self, region: HtmlRegion, parent: NodeId) {
        match region.shape {
            HtmlShape::Comment => {
                let node =
                    self.open_node(parent, NodeKind::Comment, Classification::SOURCE_VISIBLE);
                self.emit(region.end, LexemeKind::Comment, Some(node));
                self.close_node(node);
            }
            HtmlShape::Opaque => {
                let node = self.open_node(parent, NodeKind::Opaque, Classification::OPAQUE);
                self.emit(region.end, LexemeKind::Opaque, Some(node));
                self.close_node(node);
            }
            HtmlShape::Canonical {
                kind,
                open_end,
                close_start,
            } => {
                // A canonical element keeps its tags as their own lexemes and
                // its content as ordinary inline content, so B.6 has the
                // ownership it needs. It is still SourceVisible until then.
                let node = self.open_node(parent, kind, Classification::SOURCE_VISIBLE);
                self.emit(open_end, LexemeKind::HtmlOpenTag, Some(node));
                if self.html_escapes(open_end, close_start) {
                    // Cannot happen for a proved element — the matcher already
                    // balanced everything inside it — but if it ever did, the
                    // bytes stay covered as opaque source rather than half read.
                    self.emit(close_start, LexemeKind::Opaque, Some(node));
                } else {
                    self.inline_until(close_start, node);
                }
                self.emit(region.end, LexemeKind::HtmlCloseTag, Some(node));
                self.close_node(node);
            }
        }
    }

    fn inline_code(&mut self, width: usize, region_end: usize, parent: NodeId) {
        let node = self.open_node(parent, NodeKind::InlineCode, Classification::SOURCE_VISIBLE);
        self.emit(self.pos + width, LexemeKind::CodeDelimiter, Some(node));
        self.emit(region_end - width, LexemeKind::CodeText, Some(node));
        self.emit(region_end, LexemeKind::CodeDelimiter, Some(node));
        self.close_node(node);
    }

    fn link(&mut self, span: LinkSpan, parent: NodeId) {
        let node = self.open_node(parent, NodeKind::Link, Classification::SOURCE_VISIBLE);
        let start = self.pos;
        self.emit(start + 1, LexemeKind::LinkSyntax, Some(node));
        self.inline_until(start + span.label_end, node);
        self.emit(
            start + span.destination_start,
            LexemeKind::LinkSyntax,
            Some(node),
        );
        self.emit(
            start + span.destination_end,
            LexemeKind::LinkDestination,
            Some(node),
        );
        self.emit(start + span.total, LexemeKind::LinkSyntax, Some(node));
        self.close_node(node);
    }

    fn mark(&mut self, span: MarkSpan, parent: NodeId) {
        let node = self.open_node(parent, span.kind.clone(), Classification::SOURCE_VISIBLE);
        let start = self.pos;
        self.emit(start + span.width, LexemeKind::MarkOpen, Some(node));
        self.inline_until(start + span.total - span.width, node);
        self.emit(start + span.total, LexemeKind::MarkClose, Some(node));
        self.close_node(node);
    }

    // -- the normative HTML boundary algorithm (§26.10) ---------------------

    /// The region an HTML candidate at `start` owns, or `None` when the `<`
    /// there is not a lexical tag at all and is therefore literal text.
    ///
    /// This is §26.10 step by step. It consults nothing but the bytes: no
    /// renderer, no viewport, no line. Given the same source it gives the same
    /// answer, and it is total — every candidate resolves to a span, with EOF
    /// as the safe limit whenever a close cannot be proved.
    fn html_region(&self, start: usize) -> Option<HtmlRegion> {
        let source = self.source;

        // Step 0: a `<` inside fenced code, a balanced code span or an escape
        // is literal text and starts nothing.
        if self.literal_covering(start).is_some() {
            return None;
        }

        let rest = &source[start..];

        // Step 1: a comment is its own shape.
        if rest.starts_with("<!--") {
            let end = match source[start + 4..].find("-->") {
                Some(index) => start + 4 + index + 3,
                None => source.len(),
            };
            return Some(HtmlRegion {
                end,
                shape: HtmlShape::Comment,
            });
        }

        // Step 1 and step 6: a close tag where a candidate begins is an orphan,
        // and an orphan is malformed from where it starts to EOF.
        if let Some(after) = rest.strip_prefix("</") {
            if after.starts_with(|c: char| c.is_ascii_alphabetic()) {
                return Some(HtmlRegion {
                    end: source.len(),
                    shape: HtmlShape::Opaque,
                });
            }
            return None;
        }

        // Step 1: anything but `<` + ASCII letter is literal text. This is the
        // rule that keeps `2 < 3 and 4 > 1` a sentence.
        if !rest[1..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            return None;
        }

        // Step 2: find the `>` that ends the tag, quote-aware.
        let Some(tag) = scan_tag(source, start) else {
            return Some(HtmlRegion {
                end: source.len(),
                shape: HtmlShape::Opaque,
            });
        };

        // Step 3: void and self-closing elements are complete as they stand.
        if tag.self_contained || VOID_ELEMENTS.contains(&tag.name.as_str()) {
            return Some(HtmlRegion {
                end: tag.end,
                shape: HtmlShape::Opaque,
            });
        }

        // Steps 4 and 5: prove the close with a document-bounded, quote-aware
        // stack. Anything unprovable falls to step 6 and protects to EOF.
        match self.matching_close(&tag) {
            Some(close_start) => {
                let end = scan_tag(source, close_start)
                    .map(|close| close.end)
                    .unwrap_or(source.len());
                let shape = match canonical_kind(source, &tag) {
                    Some(kind) => HtmlShape::Canonical {
                        kind,
                        open_end: tag.end,
                        close_start,
                    },
                    None => HtmlShape::Opaque,
                };
                Some(HtmlRegion { end, shape })
            }
            None => Some(HtmlRegion {
                end: source.len(),
                shape: HtmlShape::Opaque,
            }),
        }
    }

    /// Where the close tag that balances `tag` begins, if one is proved.
    fn matching_close(&self, tag: &Tag) -> Option<usize> {
        let source = self.source;
        let mut stack: Vec<String> = vec![tag.name.clone()];
        let mut cursor = tag.end;

        while cursor < source.len() {
            let at = cursor + source[cursor..].find('<')?;

            // §26.10 step 4: "fences … não têm significado para matching HTML".
            // With step 0 that is one rule, not two: bytes the lexer already
            // resolved as literal cannot open or close anything.
            if let Some((_, end)) = self.literal_covering(at) {
                cursor = end.max(at + 1);
                continue;
            }

            let rest = &source[at..];

            // A comment is one token and its contents never reach the stack.
            if rest.starts_with("<!--") {
                // An unterminated comment means the candidate is never closed.
                cursor = at + 4 + source[at + 4..].find("-->")? + 3;
                continue;
            }

            if let Some(after) = rest.strip_prefix("</") {
                if !after.starts_with(|c: char| c.is_ascii_alphabetic()) {
                    cursor = at + 1;
                    continue;
                }
                let close = scan_tag(source, at)?;
                // A lexically malformed close tail, or a name that is not the
                // one on top, fails the whole candidate (step 6).
                if !close.well_formed_close || stack.last() != Some(&close.name) {
                    return None;
                }
                stack.pop();
                if stack.is_empty() {
                    return Some(at);
                }
                cursor = close.end;
                continue;
            }

            if !rest[1..].starts_with(|c: char| c.is_ascii_alphabetic()) {
                // Not a lexical tag: literal text, and meaningless to matching.
                cursor = at + 1;
                continue;
            }

            let open = scan_tag(source, at)?;
            if !open.self_contained && !VOID_ELEMENTS.contains(&open.name.as_str()) {
                if stack.len() >= NESTING_BUDGET {
                    // Past the budget the projector stops proving and the
                    // region that began unresolved protects to EOF.
                    return None;
                }
                stack.push(open.name.clone());
            }
            cursor = open.end;
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Pure helpers: no state, so each is testable on its own
// ---------------------------------------------------------------------------

/// Where an HTML candidate ends, and what it turned out to be.
///
/// Not `Copy`: a canonical shape carries the colour it was written with, and
/// a colour is a `String`. Cloning one per tag is not a cost worth a lifetime.
#[derive(Debug, Clone)]
struct HtmlRegion {
    end: usize,
    shape: HtmlShape,
}

#[derive(Debug, Clone)]
enum HtmlShape {
    Comment,
    Opaque,
    Canonical {
        kind: NodeKind,
        open_end: usize,
        close_start: usize,
    },
}

#[derive(Debug, Clone)]
struct Tag {
    start: usize,
    end: usize,
    name: String,
    self_contained: bool,
    well_formed_close: bool,
}

/// Reads one complete tag starting at `start`, quote-aware (§26.10 step 2).
///
/// `None` when EOF arrives before the `>` that would end it — including a
/// quoted attribute that is never closed, which is the case that makes
/// `<x a=">">ok` protect to the end rather than stopping at the quoted `>`.
fn scan_tag(source: &str, start: usize) -> Option<Tag> {
    let bytes = source.as_bytes();
    let is_close = source[start..].starts_with("</");
    let mut index = start + if is_close { 2 } else { 1 };

    let name_start = index;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':' | b'_' | b'.') {
            index += 1;
        } else {
            break;
        }
    }
    let name = source[name_start..index].to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }

    #[derive(PartialEq)]
    enum State {
        Unquoted,
        Single,
        Double,
    }
    let mut state = State::Unquoted;
    let mut last_solidus = false;
    let mut only_whitespace_since_name = true;
    let name_end = index;
    // What stands immediately before the byte being read, for the solidus test.
    let mut after_quote_close = false;
    let mut previous_was_whitespace = false;

    while index < bytes.len() {
        let byte = bytes[index];
        match state {
            State::Unquoted => match byte {
                b'"' => {
                    state = State::Double;
                    last_solidus = false;
                    only_whitespace_since_name = false;
                    after_quote_close = false;
                    previous_was_whitespace = false;
                }
                b'\'' => {
                    state = State::Single;
                    last_solidus = false;
                    only_whitespace_since_name = false;
                    after_quote_close = false;
                    previous_was_whitespace = false;
                }
                b'>' => {
                    return Some(Tag {
                        start,
                        end: index + 1,
                        name,
                        self_contained: last_solidus,
                        // A close tag may carry only whitespace before its `>`.
                        well_formed_close: is_close && only_whitespace_since_name,
                    });
                }
                b'/' => {
                    // §27.5/M2: a `/` only makes the tag self-closing when it
                    // stands on its own. One that merely ends an unquoted
                    // attribute value — `data-src=a/` — does not, and reading it
                    // as one turns the whole fail-closed model fail-open: the
                    // element would never be opened and its content would
                    // become editable text inside an unknown wrapper.
                    last_solidus =
                        previous_was_whitespace || index == name_end || after_quote_close;
                    only_whitespace_since_name = false;
                    after_quote_close = false;
                    previous_was_whitespace = false;
                }
                other => {
                    if other.is_ascii_whitespace() {
                        previous_was_whitespace = true;
                    } else {
                        last_solidus = false;
                        only_whitespace_since_name = false;
                        previous_was_whitespace = false;
                    }
                    after_quote_close = false;
                }
            },
            State::Double => {
                if byte == b'"' {
                    state = State::Unquoted;
                    after_quote_close = true;
                    previous_was_whitespace = false;
                }
            }
            State::Single => {
                if byte == b'\'' {
                    state = State::Unquoted;
                    after_quote_close = true;
                    previous_was_whitespace = false;
                }
            }
        }
        index += 1;
    }

    None
}

/// The Note-it element a proved open tag stands for, if it is one of them.
fn canonical_kind(source: &str, tag: &Tag) -> Option<NodeKind> {
    let body = &source[tag.start..tag.end];
    match tag.name.as_str() {
        "u" => Some(NodeKind::Underline),
        "span" => attribute(body, "data-note-it-color").map(NodeKind::Color),
        "mark" => attribute(body, "data-note-it-highlight").map(NodeKind::Highlight),
        _ => None,
    }
}

/// The value of `wanted` in a tag body, read with the same quote awareness the
/// tag scanner uses. Never a dynamic lookup: the caller names the attribute.
fn attribute(tag: &str, wanted: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(index) = rest.find(wanted) {
        let after = &rest[index + wanted.len()..];
        let Some(after) = after.strip_prefix('=') else {
            rest = &rest[index + wanted.len()..];
            continue;
        };
        let quote = after.chars().next()?;
        if quote != '"' && quote != '\'' {
            rest = after;
            continue;
        }
        let value = &after[1..];
        let end = value.find(quote)?;
        return Some(value[..end].to_owned());
    }
    None
}

fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        Some(hashes as u8)
    } else {
        None
    }
}

fn is_thematic_break(trimmed: &str) -> bool {
    let text = trimmed.trim_end();
    for marker in ['-', '*', '_'] {
        if text.len() >= 3 && text.chars().all(|c| c == marker) {
            return true;
        }
    }
    false
}

/// The width of a task's `- [ ] ` prefix, checkbox included.
fn task_prefix(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = &line[indent..];
    for marker in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] "] {
        if trimmed.starts_with(marker) {
            return Some(indent + marker.len());
        }
    }
    None
}

/// The width of a list item's marker, including the space after it.
fn list_prefix(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = &line[indent..];

    let first = trimmed.chars().next()?;
    if matches!(first, '-' | '*' | '+') && trimmed[1..].starts_with(' ') {
        return Some(indent + 2);
    }

    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=6).contains(&digits) && trimmed[digits..].starts_with(". ") {
        return Some(indent + digits + 2);
    }

    None
}

fn quote_prefix(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = &line[indent..];
    if !trimmed.starts_with('>') {
        return None;
    }
    let after = if trimmed[1..].starts_with(' ') { 2 } else { 1 };
    Some(indent + after)
}

fn callout_marker(rest: &str) -> bool {
    let trimmed = rest.trim_start();
    trimmed.starts_with("[!") && trimmed.contains(']')
}

/// Whether `line` would start a block of its own, ending the paragraph above.
fn starts_a_different_block(line: &str) -> bool {
    let trimmed = line.trim_start();
    heading_level(line).is_some()
        || trimmed.starts_with("```")
        || trimmed.starts_with("~~~")
        || line.starts_with("<!--")
        || is_thematic_break(trimmed)
        || task_prefix(line).is_some()
        || list_prefix(line).is_some()
        || quote_prefix(line).is_some()
}

/// How many bytes a backslash escape takes, or 0 when the backslash is text.
fn escape_width(rest: &str) -> usize {
    let mut chars = rest.chars();
    chars.next();
    match chars.next() {
        Some(next) if next.is_ascii_punctuation() => 1 + next.len_utf8(),
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy)]
struct LinkSpan {
    label_end: usize,
    destination_start: usize,
    destination_end: usize,
    total: usize,
}

/// `[label](destination)` with balanced brackets and balanced parentheses.
///
/// Balance is the whole point: `inline.rs` stops at the first `)` and so reads
/// `https://example.com/a_(b)` as `https://example.com/a_(b`, which §26.16
/// names as the concrete evidence that the reader's parser cannot be the
/// editor's. This one counts.
fn link_span(rest: &str, limit: usize) -> Option<LinkSpan> {
    let text = &rest[..limit];
    let bytes = text.as_bytes();

    let mut depth = 0usize;
    let mut index = 0usize;
    let mut label_end = None;

    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    label_end = Some(index);
                    break;
                }
            }
            _ => {}
        }
        index += 1;
    }

    let label_end = label_end?;
    if !text[label_end + 1..].starts_with('(') {
        return None;
    }

    let destination_start = label_end + 2;
    let mut depth = 1usize;
    let mut index = destination_start;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(LinkSpan {
                        label_end,
                        destination_start,
                        destination_end: index,
                        total: index + 1,
                    });
                }
            }
            _ => {}
        }
        index += 1;
    }

    None
}

#[derive(Debug, Clone)]
struct MarkSpan {
    kind: NodeKind,
    width: usize,
    total: usize,
}

/// A balanced delimiter run on this line, by the §27.15 procedure.
///
/// The R2 text described properties of a result rather than a procedure, which
/// left `*a**b*` and `**a*b**` decided by one implementation and undecided by
/// another. This is the procedure: a single left-to-right scan, an explicit
/// opener/closer test, and one literal fallback that catches everything else.
///
/// Recognising a run is not permission to edit it. B.1 marks every one of these
/// `SourceVisible`, and only B.5 grants an operation on one; the structure
/// exists first so that the gate granting the capability has something already
/// proved to grant it over.
fn mark_span(rest: &str, limit: usize, before: Option<char>) -> Option<MarkSpan> {
    let text = &rest[..limit];
    let marker = text.chars().next()?;
    let width = text.chars().take_while(|c| *c == marker).count();

    // Width 4 and above is literal, and so is every marker/width pair the
    // §26.5 vocabulary does not name.
    let kind = match (marker, width) {
        ('*', 1) => NodeKind::Emphasis,
        ('*', 2) => NodeKind::Strong,
        ('*', 3) => NodeKind::StrongEmphasis,
        ('~', 2) => NodeKind::Strike,
        _ => return None,
    };

    // Opener: preceded by line start, whitespace or punctuation; followed by
    // something that is not whitespace.
    let opens = before.is_none_or(|c| c.is_whitespace() || c.is_ascii_punctuation());
    let body = &text[width..];
    let first = body.chars().next();
    if !opens || first.is_none_or(char::is_whitespace) {
        return None;
    }

    let mut index = 0usize;
    while index < body.len() {
        let next = index + body[index..].find(marker)?;
        let run = body[next..].chars().take_while(|c| *c == marker).count();

        if run != width {
            // §28.5: a run of a different width is allowed between the ends
            // only when it is itself a balanced pair contained in the interval
            // — that is what makes `**a *b* c**` a Strong holding an Emphasis
            // rather than a literal. One that never closes makes the outer pair
            // literal, which is the `*a**b*` case.
            let inner_marker = body[next..].chars().next()?;
            let after_inner = next + run;
            let closes_at = closing_delimiter_run(&body[after_inner..], inner_marker, run)?;
            index = after_inner + closes_at + run;
            continue;
        }

        // Closer: preceded by non-whitespace, followed by line end, whitespace
        // or punctuation.
        let preceded = body[..next].chars().next_back();
        let followed = body[next + run..].chars().next();
        let closes = preceded.is_some_and(|c| !c.is_whitespace())
            && followed.is_none_or(|c| c.is_whitespace() || c.is_ascii_punctuation());

        if closes {
            return Some(MarkSpan {
                kind,
                width,
                total: width + next + run,
            });
        }

        index = next + run;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(source: &str) -> Vec<(usize, usize, LexemeKind)> {
        project(source, Generation::first())
            .lexemes
            .iter()
            .map(|lexeme| (lexeme.source.start(), lexeme.source.end(), lexeme.kind))
            .collect()
    }

    #[test]
    fn a_quoted_bracket_does_not_end_a_tag() {
        let tag = scan_tag("<x a=\">\">ok", 0).expect("a tag");
        assert_eq!(tag.end, 9);
        assert_eq!(tag.name, "x");
    }

    #[test]
    fn an_unterminated_quote_never_ends_the_tag() {
        assert!(scan_tag("<x a=\">ok", 0).is_none());
    }

    #[test]
    fn a_self_closing_tag_is_complete() {
        let tag = scan_tag("<x/>after", 0).expect("a tag");
        assert!(tag.self_contained);
    }

    #[test]
    fn a_close_tag_with_a_tail_is_not_well_formed() {
        assert!(!scan_tag("</x junk>", 0).expect("a tag").well_formed_close);
        assert!(scan_tag("</x >", 0).expect("a tag").well_formed_close);
    }

    #[test]
    fn a_link_destination_keeps_its_balanced_parentheses() {
        let source = "[a](https://example.com/a_(b))";
        let link = link_span(source, source.len()).expect("a link");
        assert_eq!(
            &source[link.destination_start..link.destination_end],
            "https://example.com/a_(b)"
        );
    }

    #[test]
    fn an_empty_or_overwide_delimiter_run_is_literal() {
        assert!(mark_span("****", 4, None).is_none());
        assert!(mark_span("**** a", 6, None).is_none());
        assert!(mark_span("** a**", 6, None).is_none());
        assert!(mark_span("*a*", 3, None).is_some());
    }

    #[test]
    fn a_mismatched_inner_run_keeps_the_whole_construct_literal() {
        // §27.15: `*a**b*` and `**a*b**` are the two typos the R2 procedure
        // could not decide. Both are literal now, deterministically.
        assert!(mark_span("*a**b*", 6, None).is_none());
        assert!(mark_span("**a*b**", 7, None).is_none());
    }

    #[test]
    fn a_run_needs_a_flanking_context_to_open() {
        // Preceded by a letter, `*` is not an opener.
        assert!(mark_span("*b*", 3, Some('a')).is_none());
        assert!(mark_span("*b*", 3, Some(' ')).is_some());
        assert!(mark_span("*b*", 3, Some('(')).is_some());
    }

    #[test]
    fn an_entity_is_one_lexeme_and_an_unknown_one_is_text() {
        let known = spans("&amp;");
        assert_eq!(known, vec![(0, 5, LexemeKind::Entity)]);

        let unknown = spans("&nope;");
        assert!(unknown.iter().all(|(_, _, kind)| *kind == LexemeKind::Text));
    }

    #[test]
    fn a_heading_prefix_is_its_own_lexeme() {
        let lexemes = spans("## t");
        assert_eq!(lexemes[0], (0, 3, LexemeKind::BlockPrefix));
        assert_eq!(lexemes[1], (3, 4, LexemeKind::Text));
    }
}
