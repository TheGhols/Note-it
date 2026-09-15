//! Recognising the references a note's Markdown makes to other notes.
//!
//! ADR-065 fixed the grammar; this module is its implementation and nothing
//! else. It answers one question — **which bytes of this body are a reference,
//! and what does each one say?** — and it answers it without opening the store.
//!
//! ## Syntax is not identity
//!
//! `[[HAS]]` produces the *text* `HAS`. Whether a note answers to that name is
//! [`crate::NoteItCore::resolve_note_name`]'s question, asked later, and the two
//! stay apart on purpose: a parser that consulted the store would make the
//! meaning of a document depend on which notes happened to exist while it was
//! being read, and a body would parse differently on two machines. Nothing here
//! takes a `&StorageManager`, a `Uuid` or a name catalogue, and nothing here
//! decides existence, picks between candidates or reads a file.
//!
//! ## Lossless
//!
//! The source is never altered, normalised or repaired. Every reference carries
//! `[start_byte, end_byte)` over the original UTF-8 and a `raw` that is exactly
//! that slice, plus the raw slice of each component before decoding. Laying the
//! text between references alongside the references' own `raw` reproduces the
//! input byte for byte — the property `lossless_reconstruction_holds_for_the_corpus`
//! asserts precisely that over every case of the normative fixture.
//!
//! ## Total
//!
//! Every input produces an answer. A candidate that breaks any rule produces
//! **no** reference, stays literal, and the scan resumes after its closing
//! `]]` — never inside it, so a refused candidate can never be re-read as a
//! smaller one. An opener with no closer on its line makes the rest of that
//! line literal. Both failures move the cursor past what they examined, which
//! is what keeps a hostile document linear instead of quadratic.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::ops::Range;

/// The most Unicode scalars a decoded, trimmed `note`, `section` or `display`
/// may carry. The `note` limit is the one the ADR-063/064 names already have.
pub const MAX_COMPONENT_SCALARS: usize = 512;

/// The most Unicode scalars the raw interior between `[[` and `]]` may carry.
///
/// Three components of 512 scalars, each entirely escaped, cost 3 × 1 024; the
/// `#` and the `|` that separate them cost two more. Anything longer cannot be
/// a valid candidate however it is spelled, so the scanner stops looking rather
/// than reading a whole hostile line.
pub const MAX_RAW_INTERIOR_SCALARS: usize = 3074;

/// The narrowest and widest a block identifier may be.
pub const MIN_BLOCK_ID_CHARS: usize = 6;
pub const MAX_BLOCK_ID_CHARS: usize = 64;

/// The seven characters a backslash decodes inside a component.
///
/// Every other backslash stays exactly where it is: `A\qB` names `A\qB`, which
/// is what keeps a Windows path or a regular expression from being quietly
/// rewritten by the act of linking to it.
const ESCAPABLE: [char; 7] = ['\\', '[', ']', '#', '^', '|', '!'];

/// Whether a reference asks to point at something or to show it.
///
/// `Embed` is an intention and nothing more in this phase: it is not permission
/// to transclude, and nothing here reads the note it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Link,
    Embed,
}

/// One reference, as syntax.
///
/// The decoded components are what a resolver is handed; the `*_raw` slices are
/// what the file actually spells, before escapes were decoded and before the
/// ends were trimmed. Both are kept because a surface that re-renders the
/// document needs the spelling, and a resolver needs the meaning.
///
/// A component is `None` when the grammar has no place for it, which is not the
/// same as empty: `[[#Tratamento]]` has `note: None` meaning *this note*, while
/// an empty name is simply not a candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteReference<'a> {
    pub kind: ReferenceKind,
    /// Inclusive start over the source's UTF-8. For an embed it includes the `!`.
    pub start_byte: usize,
    /// Exclusive end over the source's UTF-8, just past the closing `]]`.
    pub end_byte: usize,
    /// Exactly `&source[start_byte..end_byte]`.
    pub raw: &'a str,
    pub note: Option<Cow<'a, str>>,
    pub section: Option<Cow<'a, str>>,
    /// A block identifier is ASCII with no escape, so its decoded form is its
    /// raw form; the type stays a `Cow` so every component reads the same way.
    pub block: Option<Cow<'a, str>>,
    pub display: Option<Cow<'a, str>>,
    pub note_raw: Option<&'a str>,
    pub section_raw: Option<&'a str>,
    pub block_raw: Option<&'a str>,
    pub display_raw: Option<&'a str>,
}

impl NoteReference<'_> {
    /// The reference's span over the source.
    pub fn span(&self) -> Range<usize> {
        self.start_byte..self.end_byte
    }
}

/// Every reference in `source`, in source order.
///
/// `source` is a whole stored note: leading YAML front matter is recognised and
/// skipped here rather than being the caller's problem, so that two callers
/// cannot disagree about where the body starts.
///
/// Never panics, never allocates per byte, and never writes.
pub fn parse_references(source: &str) -> Vec<NoteReference<'_>> {
    let mut protected = protected_ranges(source);
    scan_references(source, &mut protected)
}

// ------------------------------------------------------- protected regions

/// Byte ranges no wikilink may begin, end or sit inside.
///
/// Sorted and merged, then walked left to right: both scans move forward only,
/// so a cursor answers "is this byte protected?" in amortised constant time and
/// the whole pass stays linear in the source.
struct Regions {
    ranges: Vec<Range<usize>>,
    cursor: usize,
}

impl Regions {
    fn new(mut ranges: Vec<Range<usize>>) -> Self {
        ranges.retain(|range| range.start < range.end);
        ranges.sort_by_key(|range| (range.start, range.end));
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for range in ranges {
            match merged.last_mut() {
                Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
                _ => merged.push(range),
            }
        }
        Self {
            ranges: merged,
            cursor: 0,
        }
    }

    fn advance_to(&mut self, index: usize) {
        while self
            .ranges
            .get(self.cursor)
            .is_some_and(|range| range.end <= index)
        {
            self.cursor += 1;
        }
    }

    /// The end of the region covering `index`, when one does.
    fn covering_end(&mut self, index: usize) -> Option<usize> {
        self.advance_to(index);
        self.ranges
            .get(self.cursor)
            .filter(|range| range.start <= index)
            .map(|range| range.end)
    }

    /// The widest unprotected stretch beginning at or after `index`.
    fn gap_from(&mut self, index: usize, limit: usize) -> Option<Range<usize>> {
        let mut start = index;
        loop {
            if start >= limit {
                return None;
            }
            self.advance_to(start);
            match self.ranges.get(self.cursor) {
                Some(range) if range.start <= start => start = range.end,
                Some(range) => return Some(start..range.start.min(limit)),
                None => return Some(start..limit),
            }
        }
    }
}

/// Everything ADR-065's precedence table keeps a wikilink out of.
fn protected_ranges(source: &str) -> Regions {
    let lines = lines_of(source);

    // Whole lines that are not Markdown text at all. They come first because
    // every later question — is this a definition? is this a calculation? — is
    // meaningless inside a fence.
    let opaque = opaque_blocks(&lines, source.len());
    let mut structural = opaque.clone();

    // Reference definitions have to be known before a reference link can be
    // recognised, and the definition's own bytes are protected as a block.
    let (labels, definitions) = link_definitions(&lines, &mut Regions::new(opaque.clone()));
    structural.extend(definitions);

    // Lines the math grammar already owns.
    structural.extend(math_lines(&lines, &mut Regions::new(opaque)));

    let mut structural = Regions::new(structural);
    let inline = inline_ranges(source, &mut structural, &labels);

    let mut all = structural.ranges;
    all.extend(inline);
    Regions::new(all)
}

// ------------------------------------------------------------ line structure

#[derive(Clone, Copy)]
struct Line<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    next: usize,
}

fn lines_of(source: &str) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start <= source.len() {
        let end = source[start..]
            .find('\n')
            .map_or(source.len(), |offset| start + offset);
        let text_end = if end > start && source.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end
        };
        lines.push(Line {
            text: &source[start..text_end],
            start,
            end: text_end,
            next: (end + 1).min(source.len()),
        });
        if end >= source.len() {
            break;
        }
        start = end + 1;
    }
    lines
}

/// Whether `index` sits inside an opaque block.
///
/// Both callers walk the lines in order, so the region cursor only ever moves
/// forward: the whole sweep costs one pass over the blocks, not one pass per
/// line.
fn covered(regions: &mut Regions, index: usize) -> bool {
    regions.covering_end(index).is_some()
}

// ------------------------------------------------------------ opaque blocks

/// Front matter, fenced code and indented code: lines that never hold inlines.
fn opaque_blocks(lines: &[Line<'_>], source_len: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut index = 0;

    // Only at the very start, and only when it closes — the same rule
    // `storage::read_front_matter` applies to the file on disk. An unclosed
    // `---` is not metadata; it is a thematic break with text under it.
    if let Some(close) = front_matter_close(lines) {
        ranges.push(lines[0].start..lines[close].next);
        index = close + 1;
    }

    let mut in_paragraph = false;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.text.trim_start_matches(' ');
        let indent = line.text.len() - trimmed.len();

        if indent <= 3 {
            if let Some((character, width)) = fence_marker(trimmed) {
                let mut end = source_len;
                let mut closed = index;
                for (offset, candidate) in lines.iter().enumerate().skip(index + 1) {
                    let body = candidate.text.trim_start_matches(' ');
                    if candidate.text.len() - body.len() > 3 {
                        continue;
                    }
                    if let Some((other, other_width)) = fence_marker(body) {
                        if other == character && other_width >= width {
                            end = candidate.next;
                            closed = offset;
                            break;
                        }
                    }
                }
                ranges.push(line.start..end);
                index = if closed == index {
                    lines.len()
                } else {
                    closed + 1
                };
                in_paragraph = false;
                continue;
            }
        }

        if line.text.trim().is_empty() {
            in_paragraph = false;
            index += 1;
            continue;
        }

        // Four spaces are code only where a paragraph could not be continuing;
        // inside one they are a lazy continuation, which CommonMark reads as
        // the prose it looks like.
        if !in_paragraph && (line.text.starts_with("    ") || line.text.starts_with('\t')) {
            ranges.push(line.start..line.next);
            index += 1;
            continue;
        }

        in_paragraph = true;
        index += 1;
    }

    ranges
}

fn front_matter_close(lines: &[Line<'_>]) -> Option<usize> {
    if lines.first()?.text != "---" {
        return None;
    }
    lines
        .iter()
        .skip(1)
        .position(|line| line.text == "---")
        .map(|offset| offset + 1)
}

/// The fence character and how many of it open or close a fenced block.
///
/// A backtick fence's info string may not itself contain a backtick, which is
/// what keeps `` ```a`b `` from opening a block nobody meant.
fn fence_marker(trimmed: &str) -> Option<(char, usize)> {
    let character = trimmed.chars().next()?;
    if character != '`' && character != '~' {
        return None;
    }
    let width = trimmed.chars().take_while(|c| *c == character).count();
    if width < 3 {
        return None;
    }
    if character == '`' && trimmed[width..].contains('`') {
        return None;
    }
    Some((character, width))
}

// -------------------------------------------------------------- math lines

/// Whether a line is one the math grammar already claims.
///
/// The two rules are `noteit-tui`'s, in `src/math/document.rs`: a calculation
/// begins with `=` (never `==`, so a line of emphasis is never one), and a
/// declaration is one bare token before `:=`. They are restated rather than
/// called because the Core cannot depend on a surface built on top of it; the
/// fixture's `math-calculation` and `math-declaration` are what hold the two
/// statements of the rule together.
fn math_lines(lines: &[Line<'_>], opaque: &mut Regions) -> Vec<Range<usize>> {
    lines
        .iter()
        .filter(|line| !covered(opaque, line.start))
        .filter(|line| is_calculation(line.text) || is_declaration(line.text))
        .map(|line| line.start..line.end)
        .collect()
}

fn is_calculation(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    matches!(trimmed.strip_prefix('='), Some(rest) if !rest.starts_with('='))
}

fn is_declaration(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let Some(operator) = trimmed.find(":=") else {
        return false;
    };
    let name = trimmed[..operator].trim_end_matches([' ', '\t']);
    !name.is_empty() && !name.contains([' ', '\t', ':', '='])
}

// ------------------------------------------------- link reference definitions

/// The most characters a CommonMark link label may hold.
const MAX_LABEL_CHARS: usize = 999;

/// The labels this document defines, and the bytes those definitions occupy.
///
/// A definition is a block, so it is only looked for where a block could begin:
/// at the start, after a blank line, or directly after another definition. Its
/// label, destination and title are all protected — a definition is not prose,
/// and `[ref]: /[[A]]` points at a file rather than naming a note.
fn link_definitions(lines: &[Line<'_>], opaque: &mut Regions) -> (Vec<String>, Vec<Range<usize>>) {
    let mut labels = Vec::new();
    let mut ranges = Vec::new();
    let mut at_block_start = true;

    for line in lines {
        if covered(opaque, line.start) || line.text.trim().is_empty() {
            at_block_start = true;
            continue;
        }
        if at_block_start {
            if let Some(label) = definition_label(line.text) {
                labels.push(label);
                ranges.push(line.start..line.end);
                continue;
            }
        }
        at_block_start = false;
    }

    labels.sort();
    labels.dedup();
    (labels, ranges)
}

/// The label a one-line definition defines, when the line really is one.
///
/// Only single-line definitions are recognised. A definition whose destination
/// or title has been wrapped onto the next line is read as the paragraph it
/// resembles, which costs a reference link its protection rather than
/// protecting a paragraph that was never a definition.
fn definition_label(text: &str) -> Option<String> {
    let trimmed = text.trim_start_matches(' ');
    if text.len() - trimmed.len() > 3 {
        return None;
    }
    let (label, after_label) = label_at(trimmed, 0)?;
    let rest = trimmed
        .get(after_label..)?
        .strip_prefix(':')?
        .trim_start_matches([' ', '\t']);
    let (_, after_destination) = link_destination(rest)?;
    let tail = rest
        .get(after_destination..)?
        .trim_start_matches([' ', '\t']);
    if tail.is_empty() {
        return Some(label);
    }
    let after_title = link_title(tail)?;
    tail.get(after_title..)?.trim().is_empty().then_some(label)
}

/// A link label at `at`, normalised for matching, and where it ends.
///
/// CommonMark ends a label at the first `]` that is not backslash-escaped and
/// forbids an unescaped `[` or `]` inside it. That is exactly what makes
/// `[veja [[A]]]: /destino` not a definition — and so leaves the `[[A]]` in it
/// an ordinary wikilink.
fn label_at(source: &str, at: usize) -> Option<(String, usize)> {
    let body = source.get(at..)?.strip_prefix('[')?;
    let mut index = 0;
    while index < body.len() {
        let character = body[index..].chars().next()?;
        match character {
            '\\' => {
                let next = body[index + 1..].chars().next()?;
                index += 1 + next.len_utf8();
            }
            '[' => return None,
            ']' => {
                let raw = &body[..index];
                let label = normalized_label(raw)?;
                return Some((label, at + 1 + index + 1));
            }
            _ => index += character.len_utf8(),
        }
    }
    None
}

/// A label folded the way CommonMark matches them: case-insensitive, with runs
/// of whitespace counting as one space.
fn normalized_label(raw: &str) -> Option<String> {
    if raw.trim().is_empty() || raw.chars().count() > MAX_LABEL_CHARS {
        return None;
    }
    let mut normalized = String::with_capacity(raw.len());
    let mut spaced = false;
    for character in raw.trim().chars() {
        if character.is_whitespace() {
            spaced = true;
            continue;
        }
        if spaced && !normalized.is_empty() {
            normalized.push(' ');
        }
        spaced = false;
        for lowered in character.to_lowercase() {
            normalized.push(lowered);
        }
    }
    Some(normalized)
}

/// A label used where it is written rather than where it is referenced: the
/// text between an opener's brackets, which a collapsed or shortcut reference
/// reuses as its own label.
fn label_of_text(text: &str) -> Option<String> {
    let mut index = 0;
    while index < text.len() {
        let character = text[index..].chars().next()?;
        match character {
            '\\' => {
                let next = text[index + 1..].chars().next()?;
                index += 1 + next.len_utf8();
            }
            '[' | ']' => return None,
            _ => index += character.len_utf8(),
        }
    }
    normalized_label(text)
}

// -------------------------------------------------------------- inline scan

/// An unclosed `[` or `![`, waiting to find out whether it opens a link.
#[derive(Clone, Copy)]
struct Opener {
    start: usize,
    image: bool,
    active: bool,
}

/// Every inline construct that owns its own brackets.
///
/// One left-to-right pass with a bracket stack, which is how CommonMark itself
/// resolves links: each `]` does work bounded by the destination or label that
/// follows it, and an opener that fails is dropped rather than retried, so a
/// line of nothing but `[` costs one pass and not one pass per bracket.
fn inline_ranges(source: &str, structural: &mut Regions, labels: &[String]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut stack: Vec<Opener> = Vec::new();
    let mut index = 0;
    let mut paragraph_limit = paragraph_end(source, 0);

    // Backtick widths already known to have no partner in this paragraph. A
    // search that failed from one position fails from every later one, because
    // the range it looks in only shrinks — so remembering the failure turns a
    // line of mismatched runs from quadratic into one pass.
    let mut unclosable: Vec<bool> = Vec::new();

    while index < source.len() {
        if let Some(end) = structural.covering_end(index) {
            index = end;
            stack.clear();
            continue;
        }
        if index >= paragraph_limit {
            paragraph_limit = paragraph_end(source, index).max(index + 1);
            unclosable.clear();
        }

        let rest = &source[index..];
        let character = rest.chars().next().expect("index is on a char boundary");
        match character {
            // The CommonMark punctuation escape is consumed before anything
            // else can look at what it protects.
            '\\' => {
                index += match rest[1..].chars().next() {
                    Some(next) if next.is_ascii_punctuation() => 1 + next.len_utf8(),
                    _ => 1,
                };
            }
            '\n' => {
                if index + 1 >= paragraph_limit {
                    stack.clear();
                }
                index += 1;
            }
            '`' => {
                let width = rest.chars().take_while(|c| *c == '`').count();
                let opened = (index + width).min(paragraph_limit);
                let hopeless = unclosable.get(width).copied().unwrap_or(false);
                let closed = if hopeless {
                    None
                } else {
                    code_span_close(&source[opened..paragraph_limit], width)
                };
                match closed {
                    Some(inner) => {
                        let end = index + width + inner + width;
                        ranges.push(index..end);
                        index = end;
                    }
                    None => {
                        if unclosable.len() <= width {
                            unclosable.resize(width + 1, false);
                        }
                        unclosable[width] = true;
                        index += width;
                    }
                }
            }
            '<' => {
                if let Some(width) = html_comment_width(rest) {
                    ranges.push(index..index + width);
                    index += width;
                } else if let Some(width) = autolink_width(rest) {
                    ranges.push(index..index + width);
                    index += width;
                } else if let Some(tag) = html_tag(rest) {
                    let end = html_region_end(source, index, &tag);
                    ranges.push(index..end);
                    index = end;
                } else {
                    index += 1;
                }
            }
            '!' if rest[1..].starts_with('[') => {
                stack.push(Opener {
                    start: index,
                    image: true,
                    active: true,
                });
                index += 2;
            }
            '[' => {
                stack.push(Opener {
                    start: index,
                    image: false,
                    active: true,
                });
                index += 1;
            }
            ']' => match close_bracket(source, &mut stack, index, paragraph_limit, labels) {
                Some(span) => {
                    index = span.end;
                    ranges.push(span);
                }
                None => index += 1,
            },
            _ => index += character.len_utf8(),
        }
    }

    ranges
}

/// The start of the next blank line at or after `from`, or the end of the
/// source. Inline constructs never cross a paragraph, which is what bounds
/// every lookahead below to one paragraph instead of one document.
fn paragraph_end(source: &str, from: usize) -> usize {
    let mut index = from;
    while let Some(offset) = source[index..].find('\n') {
        let line_start = index + offset + 1;
        if line_start >= source.len() {
            return source.len();
        }
        let line_end = source[line_start..]
            .find('\n')
            .map_or(source.len(), |end| line_start + end);
        if source[line_start..line_end].trim().is_empty() {
            return line_start;
        }
        index = line_start;
    }
    source.len()
}

/// Where a code span opened by `width` backticks closes, measured from just
/// after the opening run. A run of a different length is content, not a close.
fn code_span_close(haystack: &str, width: usize) -> Option<usize> {
    let mut index = 0;
    while index < haystack.len() {
        let rest = &haystack[index..];
        if rest.starts_with('`') {
            let run = rest.chars().take_while(|c| *c == '`').count();
            if run == width {
                return Some(index);
            }
            index += run;
        } else {
            index += rest.chars().next()?.len_utf8();
        }
    }
    None
}

/// An HTML comment is opaque, closed or not: an unfinished `<!--` swallows the
/// rest of the file rather than letting half a comment become text.
fn html_comment_width(rest: &str) -> Option<usize> {
    if !rest.starts_with("<!--") {
        return None;
    }
    Some(rest.find("-->").map_or(rest.len(), |end| end + 3))
}

/// `<scheme:...>` — an absolute URI in angle brackets, which owns any bracket
/// inside it.
fn autolink_width(rest: &str) -> Option<usize> {
    let body = rest.strip_prefix('<')?;

    // Walk to the `>` rather than searching for one. An autolink holds no
    // whitespace and no second `<`, so this stops within a few characters of a
    // `<` that opens nothing — where `find('>')` would read to the end of the
    // file, once per `<`.
    let mut end = 0;
    loop {
        let character = body.get(end..)?.chars().next()?;
        match character {
            '>' => break,
            c if c.is_whitespace() || c == '<' || is_control_category(c) => return None,
            c => end += c.len_utf8(),
        }
    }

    let inner = &body[..end];
    let scheme = &inner[..inner.find(':')?];
    let mut characters = scheme.chars();
    if !characters.next()?.is_ascii_alphabetic() || !(2..=32).contains(&scheme.len()) {
        return None;
    }
    characters
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-')
        .then_some(1 + end + 1)
}

struct HtmlTag<'a> {
    width: usize,
    name: &'a str,
    closing: bool,
    self_closing: bool,
}

/// The tag at the start of `rest`, or `None` when this `<` opens none.
fn html_tag(rest: &str) -> Option<HtmlTag<'_>> {
    let mut index = 1;
    let closing = rest[index..].starts_with('/');
    if closing {
        index += 1;
    }
    if !rest[index..].starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let name_width = rest[index..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .count();
    let name = &rest[index..index + name_width];
    index += name_width;

    match rest[index..].chars().next() {
        Some('>') => {
            return Some(HtmlTag {
                width: index + 1,
                name,
                closing,
                self_closing: false,
            })
        }
        Some('/') if rest[index..].starts_with("/>") => {
            return Some(HtmlTag {
                width: index + 2,
                name,
                closing,
                self_closing: true,
            })
        }
        Some(c) if c.is_whitespace() => {}
        _ => return None,
    }

    let mut quote: Option<char> = None;
    for (offset, character) in rest[index..].char_indices() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '>' => {
                let self_closing = rest[..index + offset].ends_with('/');
                return Some(HtmlTag {
                    width: index + offset + 1,
                    name,
                    closing,
                    self_closing,
                });
            }
            None => {}
        }
    }
    None
}

/// HTML elements that hold nothing, so there is no content to scan.
const VOID_ELEMENTS: [&str; 13] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// The wrappers Note-it writes itself, whose content stays ordinary Markdown.
const CANONICAL_WRAPPERS: [&str; 3] = ["u", "span", "mark"];

/// How far an HTML construct protects.
///
/// A canonical wrapper protects its own tag and nothing more — `<u>[[A]]</u>`
/// is one wikilink wearing a wrapper. Anything else is opaque: the element and
/// everything in it, through its closing tag or to the end of the file.
fn html_region_end(source: &str, start: usize, tag: &HtmlTag<'_>) -> usize {
    let tag_end = start + tag.width;
    let name = tag.name.to_ascii_lowercase();
    if tag.closing
        || tag.self_closing
        || CANONICAL_WRAPPERS.contains(&name.as_str())
        || VOID_ELEMENTS.contains(&name.as_str())
    {
        return tag_end;
    }
    closing_tag_end(source, tag_end, &name).unwrap_or(source.len())
}

fn closing_tag_end(source: &str, from: usize, name: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let name = name.as_bytes();
    let mut index = from;
    while index + 2 + name.len() <= bytes.len() {
        if bytes[index] == b'<'
            && bytes[index + 1] == b'/'
            && bytes[index + 2..index + 2 + name.len()].eq_ignore_ascii_case(name)
        {
            let mut after = index + 2 + name.len();
            while bytes.get(after).is_some_and(u8::is_ascii_whitespace) {
                after += 1;
            }
            if bytes.get(after) == Some(&b'>') {
                return Some(after + 1);
            }
        }
        index += 1;
    }
    None
}

// ------------------------------------------------------- markdown link tails

/// Closes the bracket at `close` against the opener on top of the stack.
///
/// Returns the span the whole construct occupies when it really is a link or
/// an image. A failed opener is dropped rather than reconsidered, which is what
/// keeps a run of `[` linear.
fn close_bracket(
    source: &str,
    stack: &mut Vec<Opener>,
    close: usize,
    paragraph_limit: usize,
    labels: &[String],
) -> Option<Range<usize>> {
    let opener = stack.pop()?;
    if !opener.active {
        return None;
    }
    let text_start = opener.start + if opener.image { 2 } else { 1 };
    let text = source.get(text_start..close)?;
    let after = close + 1;
    let tail = source.get(after..)?;

    let defined = |label: Option<String>| label.is_some_and(|l| labels.binary_search(&l).is_ok());

    // Without a definition there is no collapsed or shortcut reference to find,
    // so no label is folded and nothing is allocated — which is every document
    // that never wrote one.
    if labels.is_empty() && !tail.starts_with('(') {
        return None;
    }

    let inline = tail
        .starts_with('(')
        .then(|| inline_tail_end(source, after, paragraph_limit))
        .flatten();

    let end = inline.or_else(|| {
        if tail.starts_with("[]") {
            return defined(label_of_text(text)).then_some(after + 2);
        }
        if tail.starts_with('[') {
            // A label followed. If it names a definition this is a full
            // reference; if it is a label that names nothing, the construct
            // fails outright — CommonMark does not let a shortcut take over.
            if let Some((label, label_end)) = label_at(source, after) {
                return labels.binary_search(&label).is_ok().then_some(label_end);
            }
        }
        defined(label_of_text(text)).then_some(after)
    })?;

    // Links may not contain links: everything still open below a link that
    // just closed can no longer become one.
    if !opener.image {
        for other in stack.iter_mut().filter(|other| !other.image) {
            other.active = false;
        }
    }
    Some(opener.start..end)
}

fn inline_tail_end(source: &str, at: usize, limit: usize) -> Option<usize> {
    let mut index = skip_inline_space(source, at + 1, limit)?;
    if source.get(index..)?.starts_with(')') {
        return Some(index + 1);
    }
    let (_, width) = link_destination(source.get(index..limit)?)?;
    index += width;

    let spaced = skip_inline_space(source, index, limit)?;
    if spaced > index {
        match link_title(source.get(spaced..limit)?) {
            Some(width) => index = skip_inline_space(source, spaced + width, limit)?,
            None => index = spaced,
        }
    }
    source.get(index..)?.starts_with(')').then_some(index + 1)
}

/// Spaces, tabs and at most one line ending — the whitespace CommonMark allows
/// between the parts of an inline link.
fn skip_inline_space(source: &str, from: usize, limit: usize) -> Option<usize> {
    let mut index = from;
    let mut endings = 0;
    while index < limit {
        match source.get(index..)?.chars().next()? {
            '\n' => {
                endings += 1;
                if endings > 1 {
                    return None;
                }
                index += 1;
            }
            '\r' => index += 1,
            ' ' | '\t' => index += 1,
            _ => break,
        }
    }
    Some(index)
}

/// How deep a destination's parentheses may nest before it stops being one.
///
/// CommonMark leaves the ceiling to the implementation and cmark picks this
/// number. Without one, `[a](` repeated across a megabyte makes every `]` scan
/// to the end of the document looking for a close that never comes, which is
/// the quadratic the fixture's hostile shapes are there to catch.
const MAX_DESTINATION_DEPTH: usize = 32;

/// A link destination: `<...>` with no line ending, or a run with no ASCII
/// space and balanced parentheses.
fn link_destination(rest: &str) -> Option<(&str, usize)> {
    if let Some(body) = rest.strip_prefix('<') {
        let mut index = 0;
        while index < body.len() {
            match body[index..].chars().next()? {
                '\n' | '\r' | '<' => return None,
                '\\' => index += 1 + body[index + 1..].chars().next()?.len_utf8(),
                '>' => return Some((&body[..index], index + 2)),
                character => index += character.len_utf8(),
            }
        }
        return None;
    }

    let mut index = 0;
    let mut depth = 0usize;
    while index < rest.len() {
        let character = rest[index..].chars().next()?;
        match character {
            c if c.is_whitespace() => break,
            c if is_control_category(c) => return None,
            '\\' => match rest[index + 1..].chars().next() {
                Some(next) => index += 1 + next.len_utf8(),
                None => break,
            },
            '(' => {
                depth += 1;
                if depth > MAX_DESTINATION_DEPTH {
                    return None;
                }
                index += 1;
            }
            ')' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                index += 1;
            }
            character => index += character.len_utf8(),
        }
    }
    (depth == 0 && index > 0).then(|| (&rest[..index], index))
}

/// A link title in double quotes, single quotes or parentheses.
fn link_title(rest: &str) -> Option<usize> {
    let open = rest.chars().next()?;
    let close = match open {
        '"' => '"',
        '\'' => '\'',
        '(' => ')',
        _ => return None,
    };
    let mut index = open.len_utf8();
    let mut endings = 0;
    while index < rest.len() {
        let character = rest[index..].chars().next()?;
        match character {
            '\\' => index += 1 + rest[index + 1..].chars().next()?.len_utf8(),
            '\n' => {
                endings += 1;
                if endings > 1 {
                    return None;
                }
                index += 1;
            }
            c if c == close => return Some(index + c.len_utf8()),
            c if c == open => return None,
            c => {
                if !c.is_whitespace() {
                    endings = 0;
                }
                index += c.len_utf8();
            }
        }
    }
    None
}

// ----------------------------------------------------------- the wikilinks

fn scan_references<'a>(source: &'a str, protected: &mut Regions) -> Vec<NoteReference<'a>> {
    let mut references = Vec::new();
    let mut cursor = 0;
    while let Some(gap) = protected.gap_from(cursor, source.len()) {
        scan_gap(source, &gap, &mut references);
        cursor = gap.end;
    }
    references
}

fn scan_gap<'a>(source: &'a str, gap: &Range<usize>, out: &mut Vec<NoteReference<'a>>) {
    let mut index = gap.start;
    while index < gap.end {
        let rest = &source[index..gap.end];
        let character = rest.chars().next().expect("index is on a char boundary");

        // A backslash escape is consumed before anything can open: `\[[a]]`
        // spends its `[` on the escape and never reaches an opener.
        if character == '\\' {
            index += match rest[1..].chars().next() {
                Some(next) if next.is_ascii_punctuation() => 1 + next.len_utf8(),
                _ => 1,
            };
            continue;
        }

        let (kind, interior_start) = if character == '!' && rest[1..].starts_with("[[") {
            (ReferenceKind::Embed, index + 3)
        } else if character == '[' && rest[1..].starts_with('[') {
            (ReferenceKind::Link, index + 2)
        } else {
            index += character.len_utf8();
            continue;
        };

        match find_closer(source, interior_start, gap.end) {
            Some(close) => {
                let end = close + 2;
                if let Some(parts) = decompose(&source[interior_start..close], kind) {
                    out.push(parts.into_reference(source, kind, index, end));
                }
                // Resume after the closing `]]` whether the candidate was
                // accepted or refused. A refused span holds no smaller reading
                // — that is what makes `[[[a]]` zero references and not one.
                index = end;
            }
            None => {
                // Nothing closes it on this line, so the opener and the rest of
                // the line are text, and the scan does not read them again.
                index = line_end(source, index, gap.end);
            }
        }
    }
}

/// The first unescaped `]]` at or after `from`, within one line and within the
/// raw interior ceiling.
fn find_closer(source: &str, from: usize, limit: usize) -> Option<usize> {
    if from > limit {
        return None;
    }
    let mut index = from;
    let mut scalars = 0usize;
    while index < limit {
        let rest = &source[index..limit];
        let character = rest.chars().next()?;

        // Only CR and LF end a candidate. U+2028 and U+2029 are scalars a name
        // may carry, not Markdown line breaks.
        if character == '\r' || character == '\n' {
            return None;
        }
        if character == ']' && rest[1..].starts_with(']') {
            return Some(index);
        }
        if scalars == MAX_RAW_INTERIOR_SCALARS {
            return None;
        }
        if character == '\\' {
            scalars += 1;
            index += 1;
            let next = source.get(index..limit)?.chars().next()?;
            if next == '\r' || next == '\n' || scalars == MAX_RAW_INTERIOR_SCALARS {
                return None;
            }
            scalars += 1;
            index += next.len_utf8();
        } else {
            scalars += 1;
            index += character.len_utf8();
        }
    }
    None
}

fn line_end(source: &str, from: usize, limit: usize) -> usize {
    source[from..limit]
        .find(['\r', '\n'])
        .map_or(limit, |offset| from + offset)
}

// ------------------------------------------------------------ decomposition

struct Parts<'a> {
    note_raw: Option<&'a str>,
    section_raw: Option<&'a str>,
    block_raw: Option<&'a str>,
    display_raw: Option<&'a str>,
    note: Option<Cow<'a, str>>,
    section: Option<Cow<'a, str>>,
    block: Option<Cow<'a, str>>,
    display: Option<Cow<'a, str>>,
}

impl<'a> Parts<'a> {
    fn into_reference(
        self,
        source: &'a str,
        kind: ReferenceKind,
        start: usize,
        end: usize,
    ) -> NoteReference<'a> {
        NoteReference {
            kind,
            start_byte: start,
            end_byte: end,
            raw: &source[start..end],
            note: self.note,
            section: self.section,
            block: self.block,
            display: self.display,
            note_raw: self.note_raw,
            section_raw: self.section_raw,
            block_raw: self.block_raw,
            display_raw: self.display_raw,
        }
    }
}

/// Splits a candidate's interior into components, or refuses it entirely.
///
/// There is no partial reading: a candidate that breaks one rule produces
/// nothing, so no half-understood reference can reach a resolver.
fn decompose(interior: &str, kind: ReferenceKind) -> Option<Parts<'_>> {
    let mut selector: Option<(usize, char)> = None;
    let mut bar: Option<usize> = None;
    let mut index = 0;

    while index < interior.len() {
        let character = interior[index..].chars().next()?;
        match character {
            '\\' => {
                index += 1 + interior[index + 1..].chars().next()?.len_utf8();
                continue;
            }
            // There is no nesting, so a bracket inside a candidate is either
            // escaped or a mistake.
            '[' | ']' => return None,
            '#' | '^' => {
                if selector.is_some() {
                    return None;
                }
                selector = Some((index, character));
            }
            '|' => {
                if bar.is_some() {
                    return None;
                }
                bar = Some(index);
            }
            _ => {}
        }
        index += character.len_utf8();
    }

    // Display comes last. A selector on its far side is a second separator in
    // a place the grammar has none.
    if let (Some((at, _)), Some(pipe)) = (selector, bar) {
        if at > pipe {
            return None;
        }
    }

    let (locator, display_raw) = match bar {
        Some(pipe) => (&interior[..pipe], Some(&interior[pipe + 1..])),
        None => (interior, None),
    };

    let display = match (display_raw, kind) {
        // A transclusion has nothing to substitute, so `![[a|b]]` is refused
        // rather than accepted with a field nobody would read.
        (Some(_), ReferenceKind::Embed) => return None,
        (Some(raw), ReferenceKind::Link) => Some(decode_component(raw)?),
        (None, _) => None,
    };

    let mut parts = Parts {
        note_raw: None,
        section_raw: None,
        block_raw: None,
        display_raw,
        note: None,
        section: None,
        block: None,
        display,
    };

    match selector {
        Some((at, character)) => {
            // Nothing at all before the selector means *this note*. A name that
            // is written but empty is a component that failed, not an absence.
            if at > 0 {
                parts.note_raw = Some(&locator[..at]);
                parts.note = Some(decode_component(&locator[..at])?);
            }
            let tail = &locator[at + 1..];
            if character == '#' {
                parts.section_raw = Some(tail);
                parts.section = Some(decode_component(tail)?);
            } else {
                if !valid_block_id(tail) {
                    return None;
                }
                parts.block_raw = Some(tail);
                parts.block = Some(Cow::Borrowed(tail));
            }
        }
        None => {
            parts.note_raw = Some(locator);
            parts.note = Some(decode_component(locator)?);
        }
    }

    Some(parts)
}

/// Decodes a component's escapes, trims its ends, and refuses what is left if
/// it is empty, carries a control character, or is too long.
///
/// The order is the contract's: decode, then trim `White_Space`, then reject a
/// remaining `General_Category=Cc`. That is why a tab at the end of a name is
/// trimmed while a tab inside one invalidates.
fn decode_component(raw: &str) -> Option<Cow<'_, str>> {
    let decoded: Cow<'_, str> = if raw.contains('\\') {
        let mut out = String::with_capacity(raw.len());
        let mut index = 0;
        while index < raw.len() {
            let character = raw[index..].chars().next()?;
            if character == '\\' {
                if let Some(next) = raw[index + 1..].chars().next() {
                    if ESCAPABLE.contains(&next) {
                        out.push(next);
                        index += 1 + next.len_utf8();
                        continue;
                    }
                }
            }
            out.push(character);
            index += character.len_utf8();
        }
        Cow::Owned(out)
    } else {
        Cow::Borrowed(raw)
    };

    let value = match decoded {
        Cow::Borrowed(text) => Cow::Borrowed(text.trim()),
        Cow::Owned(text) => {
            let trimmed = text.trim();
            if trimmed.len() == text.len() {
                Cow::Owned(text)
            } else {
                Cow::Owned(trimmed.to_owned())
            }
        }
    };

    (!value.is_empty()
        && !value.chars().any(is_control_category)
        && value.chars().count() <= MAX_COMPONENT_SCALARS)
        .then_some(value)
}

/// `General_Category=Cc`: the C0 and C1 control ranges and nothing else. A
/// format character such as ZWJ is text a name may legitimately carry.
fn is_control_category(character: char) -> bool {
    matches!(character, '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}')
}

/// ASCII, lowercase, six to sixty-four characters, alphanumeric at both ends
/// and hyphens only in the middle. Case-sensitive: `Abc123` is not `abc123`.
fn valid_block_id(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if !raw.is_ascii() || !(MIN_BLOCK_ID_CHARS..=MAX_BLOCK_ID_CHARS).contains(&bytes.len()) {
        return false;
    }
    let alnum = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    alnum(bytes[0])
        && alnum(bytes[bytes.len() - 1])
        && bytes.iter().all(|byte| alnum(*byte) || *byte == b'-')
}
