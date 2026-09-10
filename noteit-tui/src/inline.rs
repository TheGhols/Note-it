//! Inline projection: from what a line of a note *stores* to what it *shows*.
//!
//! A note is Markdown, and Markdown says two things at once — the words, and
//! how they are dressed. `noteit_core::visible_text` answers the first half for
//! a label or a search result by throwing the dressing away. The reader pane
//! needs both halves: the words as text, the dressing as a Ratatui [`Style`].
//! This module is that second answer, and it is deliberately held to the same
//! rules as the first so a colour that disappears from a search snippet is the
//! same colour that appears on screen.
//!
//! ## Shape
//!
//! One scanner, three collaborators:
//!
//! * a **token scan** over the line, which recognises exactly the constructs
//!   Note-it's own serializer writes and nothing else;
//! * a **tag stack**, so `</span>` restores the style that was open when its
//!   `<span>` opened rather than resetting to nothing;
//! * an **emitter**, which coalesces runs of equal style into one [`Span`] and
//!   is the single place text becomes presentation — which is why it is also
//!   the single place terminal control characters are removed.
//!
//! Emphasis recurses (a delimiter is only a delimiter if it has a partner, so
//! its extent is known before its contents are read); tags do not, so a note
//! with two hundred nested spans costs two hundred vector entries and no
//! stack.
//!
//! ## What it is not
//!
//! It is not an HTML parser, and there is no DOM here. Six element spellings
//! are understood — `span`, `mark`, `u` and their closers — and every other tag
//! is dropped whole while its text survives. No attribute is executed, no URL
//! is resolved, no CSS is evaluated: two colour properties are read out of a
//! `style` attribute by name and validated as hexadecimal, and anything that
//! is not six or three hex digits is simply not a colour.
//!
//! Nothing here writes, opens a file, or spawns anything. It takes a `&str`
//! and returns spans.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// The foreground the graphical editor forces onto highlighted text.
///
/// Highlights are kept pale so they work on every paper colour, which means
/// the note's own light text would vanish on them. `ui/src/ui/palettes.ts`
/// solves it by writing this colour on the same inline style as the
/// background; the reader does the same thing for the same reason.
pub const HIGHLIGHT_TEXT: Color = Color::Rgb(0x1E, 0x29, 0x3B);

/// The first entry of the highlight palette, used by a `<mark>` that names no
/// colour — the spelling notes written before the attribute existed still use.
const DEFAULT_HIGHLIGHT: Color = Color::Rgb(0xFD, 0xE6, 0x8A);

/// Inline code is source, and reads as source.
const CODE_FG: Color = Color::Yellow;

/// How deeply emphasis and links may nest before their delimiters are read as
/// the characters they are.
///
/// Real prose does not reach this. A file engineered to recurse would, and a
/// reader is not a place to find out how deep the stack goes.
const MAX_NESTING: usize = 32;

/// The elements this projection understands. Everything else is not an element
/// as far as the reader is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Element {
    Span,
    Mark,
    Underline,
}

/// What an opening tag does to the style in force.
#[derive(Debug, Clone, Copy)]
enum Effect {
    /// The element was recognised but says nothing this terminal can show —
    /// a font size, or an attribute that failed validation. It still opens and
    /// closes, so the nesting stays balanced.
    None,
    Foreground(Color),
    Highlight(Color),
    Underline,
}

impl Effect {
    fn apply(self, style: Style) -> Style {
        match self {
            Self::None => style,
            Self::Foreground(colour) => style.fg(colour),
            Self::Highlight(colour) => style.bg(colour).fg(HIGHLIGHT_TEXT),
            Self::Underline => style.add_modifier(Modifier::UNDERLINED),
        }
    }
}

/// A tag, once it has been recognised.
#[derive(Debug, Clone, Copy)]
enum Tag {
    Open(Element, Effect),
    Close(Element),
    /// A well-formed tag of an element Note-it does not write. It is removed
    /// from the presentation, and whatever it wrapped is kept.
    Foreign,
}

/// One open element and the style to put back when it closes.
struct Frame {
    element: Element,
    restore: Style,
}

/// Collects styled runs, and is the only door text passes through on its way
/// to the screen.
struct Emitter {
    spans: Vec<Span<'static>>,
    pending: String,
    style: Style,
}

impl Emitter {
    fn new(base: Style) -> Self {
        Self {
            spans: Vec::new(),
            pending: String::new(),
            style: base,
        }
    }

    fn set_style(&mut self, style: Style) {
        if style != self.style {
            self.flush();
            self.style = style;
        }
    }

    /// Appends one character of the note, made inert.
    ///
    /// A note is data. A stored `ESC [ 3 1 m` is five characters somebody's
    /// file contains, never an instruction to this terminal, so the C0 and C1
    /// ranges do not survive the trip to a cell. A tab is the one control that
    /// means something on a page, and it becomes the spaces it stands for
    /// rather than a cursor movement.
    fn push(&mut self, character: char) {
        match character {
            '\t' => self.pending.push_str("    "),
            '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}' => {}
            other => self.pending.push(other),
        }
    }

    fn push_str(&mut self, text: &str) {
        for character in text.chars() {
            self.push(character);
        }
    }

    fn flush(&mut self) {
        if !self.pending.is_empty() {
            let content = std::mem::take(&mut self.pending);
            self.spans.push(Span::styled(content, self.style));
        }
    }

    fn finish(mut self) -> Vec<Span<'static>> {
        self.flush();
        self.spans
    }
}

/// The styled spans a line of a note shows, starting from `base`.
pub fn spans(source: &str, base: Style) -> Vec<Span<'static>> {
    let mut emitter = Emitter::new(base);
    let mut stack: Vec<Frame> = Vec::new();
    scan(source, &mut emitter, &mut stack, 0);
    emitter.finish()
}

/// Text with nothing interpreted, only made inert: what a fenced code block
/// shows, since inside one every character is the character somebody typed.
pub fn inert(source: &str) -> String {
    let mut emitter = Emitter::new(Style::default());
    emitter.push_str(source);
    emitter.pending
}

fn scan(source: &str, out: &mut Emitter, stack: &mut Vec<Frame>, depth: usize) {
    let mut index = 0;

    while index < source.len() {
        let rest = &source[index..];
        let Some(character) = rest.chars().next() else {
            break;
        };

        match character {
            // A backslash escape is how the serializer stores a character that
            // would otherwise be a mark; what the reader sees is the character.
            '\\' => match rest[1..].chars().next() {
                Some(next) if next.is_ascii_punctuation() => {
                    out.push(next);
                    index += 1 + next.len_utf8();
                }
                _ => {
                    out.push('\\');
                    index += 1;
                }
            },
            '`' => {
                let width = rest.chars().take_while(|c| *c == '`').count();
                match code_span_close(&rest[width..], width) {
                    Some(close) => {
                        let saved = out.style;
                        out.set_style(saved.fg(CODE_FG));
                        out.push_str(&rest[width..width + close]);
                        out.set_style(saved);
                        index += width + close + width;
                    }
                    // A backtick with no partner is a backtick.
                    None => {
                        out.push_str(&rest[..width]);
                        index += width;
                    }
                }
            }
            '<' => {
                if let Some(width) = comment_width(rest) {
                    index += width;
                    continue;
                }
                match tag_width(rest).map(|width| (read_tag(&rest[..width]), width)) {
                    Some((tag, width)) => {
                        apply_tag(tag, out, stack);
                        index += width;
                    }
                    // Not a tag: `um < dois` is a comparison somebody wrote.
                    None => {
                        out.push('<');
                        index += 1;
                    }
                }
            }
            '&' => match entity(rest) {
                Some((decoded, width)) => {
                    out.push(decoded);
                    index += width;
                }
                None => {
                    out.push('&');
                    index += 1;
                }
            },
            // An image shows its alternative text; a link shows its words. The
            // destination is not on screen, and the reader never follows one.
            '!' if rest[1..].starts_with('[') => match link(&rest[1..]) {
                Some((text, width)) if depth < MAX_NESTING => {
                    nested(text, out, stack, depth, out.style);
                    index += 1 + width;
                }
                _ => {
                    out.push('!');
                    index += 1;
                }
            },
            '[' => match link(rest) {
                Some((text, width)) if depth < MAX_NESTING => {
                    let style = out.style.add_modifier(Modifier::UNDERLINED);
                    nested(text, out, stack, depth, style);
                    index += width;
                }
                _ => {
                    out.push('[');
                    index += 1;
                }
            },
            '*' | '_' | '~' => {
                let width = rest.chars().take_while(|c| *c == character).count();
                match emphasis_close(source, index, character, width)
                    .filter(|_| depth < MAX_NESTING)
                {
                    Some(close) => {
                        let style = out.style.add_modifier(emphasis_modifier(character, width));
                        nested(&rest[width..width + close], out, stack, depth, style);
                        index += width + close + width;
                    }
                    // A delimiter with no partner stays the character it is,
                    // which is what leaves `= 100 * 2.5` alone.
                    None => {
                        out.push_str(&rest[..width]);
                        index += width;
                    }
                }
            }
            other => {
                out.push(other);
                index += other.len_utf8();
            }
        }
    }
}

/// Reads `source` under `style`, then puts back exactly what was in force.
///
/// A tag opened inside and never closed ends here rather than leaking into the
/// rest of the line: the enclosing construct is its outermost possible extent.
fn nested(source: &str, out: &mut Emitter, stack: &mut Vec<Frame>, depth: usize, style: Style) {
    let saved = out.style;
    let opened = stack.len();
    out.set_style(style);
    scan(source, out, stack, depth + 1);
    stack.truncate(opened);
    out.set_style(saved);
}

fn emphasis_modifier(delimiter: char, width: usize) -> Modifier {
    match (delimiter, width) {
        ('~', _) => Modifier::CROSSED_OUT,
        (_, 2) => Modifier::BOLD,
        (_, 3) => Modifier::BOLD | Modifier::ITALIC,
        _ => Modifier::ITALIC,
    }
}

fn apply_tag(tag: Tag, out: &mut Emitter, stack: &mut Vec<Frame>) {
    match tag {
        Tag::Open(element, effect) => {
            stack.push(Frame {
                element,
                restore: out.style,
            });
            let style = effect.apply(out.style);
            out.set_style(style);
        }
        Tag::Close(element) => {
            // Closing out of order closes what it can: the innermost matching
            // element, and everything opened inside it. A closer with nothing
            // to close is dropped, never printed.
            if let Some(position) = stack.iter().rposition(|frame| frame.element == element) {
                let restore = stack[position].restore;
                stack.truncate(position);
                out.set_style(restore);
            }
        }
        Tag::Foreign => {}
    }
}

// ---------------------------------------------------------------------------
// Recognising the small HTML subset
// ---------------------------------------------------------------------------

/// The width of `<!-- … -->` at the start of `rest`.
///
/// A comment is stored but it is not what the note says, so the reader shows
/// none of it — the task metadata comment included. An opener nobody closed is
/// not a comment: only the four characters go, so the words after them survive
/// instead of being swallowed to the end of the file.
pub fn comment_width(rest: &str) -> Option<usize> {
    if !rest.starts_with("<!--") {
        return None;
    }
    Some(rest.find("-->").map_or(4, |end| end + 3))
}

/// The width of an HTML tag at the start of `rest`, or `None` when this `<`
/// opens no tag — `<https://exemplo.com>` and a bare `<` among words included.
fn tag_width(rest: &str) -> Option<usize> {
    let mut index = 1;
    if rest[index..].starts_with('/') {
        index += 1;
    }
    if !rest[index..].starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    index += rest[index..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .count();

    match rest[index..].chars().next() {
        Some('>') => return Some(index + 1),
        Some('/') if rest[index..].starts_with("/>") => return Some(index + 2),
        Some(c) if c.is_whitespace() => {}
        _ => return None,
    }

    // Attributes may quote a `>`, so the scan for the end tracks quoting.
    let mut quote: Option<char> = None;
    for (offset, character) in rest[index..].char_indices() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '>' => return Some(index + offset + 1),
            None => {}
        }
    }
    None
}

/// Classifies a complete tag, `<` and `>` included.
fn read_tag(tag: &str) -> Tag {
    let body = tag
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or_default();
    let body = body.strip_suffix('/').unwrap_or(body);

    if let Some(closing) = body.strip_prefix('/') {
        return match element_of(closing.trim()) {
            Some(element) => Tag::Close(element),
            None => Tag::Foreign,
        };
    }

    let name_width = body
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .count();
    let (name, attributes) = body.split_at(name_width);

    match element_of(name) {
        Some(Element::Underline) => Tag::Open(Element::Underline, Effect::Underline),
        Some(Element::Span) => Tag::Open(Element::Span, span_effect(attributes)),
        Some(Element::Mark) => Tag::Open(Element::Mark, mark_effect(attributes)),
        None => Tag::Foreign,
    }
}

fn element_of(name: &str) -> Option<Element> {
    match name.to_ascii_lowercase().as_str() {
        "span" => Some(Element::Span),
        "mark" => Some(Element::Mark),
        "u" => Some(Element::Underline),
        _ => None,
    }
}

/// A `<span>` is Note-it's text colour. `data-note-it-color` is the canonical
/// spelling and is preferred; the `style` the same serializer writes beside it
/// is accepted as the fallback, read by property name and validated as a
/// colour. A span carrying only a font size is an element this terminal cannot
/// honour, and it opens with no effect rather than being mistaken for one.
fn span_effect(attributes: &str) -> Effect {
    let attributes = read_attributes(attributes);
    let canonical = value_of(&attributes, "data-note-it-color").and_then(hex_colour);
    let styled = value_of(&attributes, "style")
        .and_then(|style| css_property(style, "color"))
        .and_then(hex_colour);

    match canonical.or(styled) {
        Some(colour) => Effect::Foreground(colour),
        None => Effect::None,
    }
}

/// A `<mark>` is a highlight, whatever it says about its colour: an unreadable
/// value is refused, but the element still means what it means, so it falls
/// back to the palette's first entry — which is exactly what a note written
/// before the attribute existed stores.
fn mark_effect(attributes: &str) -> Effect {
    let attributes = read_attributes(attributes);
    let canonical = value_of(&attributes, "data-note-it-highlight").and_then(hex_colour);
    let styled = value_of(&attributes, "style")
        .and_then(|style| css_property(style, "background-color"))
        .and_then(hex_colour);

    Effect::Highlight(canonical.or(styled).unwrap_or(DEFAULT_HIGHLIGHT))
}

/// The attributes of a tag as (lowercased name, raw value) pairs.
fn read_attributes(region: &str) -> Vec<(String, String)> {
    let characters: Vec<char> = region.chars().collect();
    let mut attributes = Vec::new();
    let mut index = 0;

    while index < characters.len() {
        while index < characters.len()
            && (characters[index].is_whitespace() || characters[index] == '/')
        {
            index += 1;
        }
        let start = index;
        while index < characters.len()
            && !characters[index].is_whitespace()
            && characters[index] != '='
            && characters[index] != '/'
        {
            index += 1;
        }
        if index == start {
            break;
        }
        let name: String = characters[start..index]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();

        while index < characters.len() && characters[index].is_whitespace() {
            index += 1;
        }
        let mut value = String::new();
        if index < characters.len() && characters[index] == '=' {
            index += 1;
            while index < characters.len() && characters[index].is_whitespace() {
                index += 1;
            }
            match characters.get(index) {
                Some(&quote @ ('"' | '\'')) => {
                    index += 1;
                    while index < characters.len() && characters[index] != quote {
                        value.push(characters[index]);
                        index += 1;
                    }
                    index += usize::from(index < characters.len());
                }
                _ => {
                    while index < characters.len() && !characters[index].is_whitespace() {
                        value.push(characters[index]);
                        index += 1;
                    }
                }
            }
        }
        attributes.push((name, value));
    }

    attributes
}

fn value_of<'a>(attributes: &'a [(String, String)], wanted: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(name, _)| name == wanted)
        .map(|(_, value)| value.as_str())
}

/// One declaration out of a `style` attribute, by exact property name.
///
/// Everything else in the attribute is ignored rather than interpreted: this
/// is a lookup of two known properties, not a CSS engine, so a `position`, a
/// `z-index` or an `expression(…)` beside them changes nothing.
fn css_property<'a>(style: &'a str, property: &str) -> Option<&'a str> {
    style.split(';').find_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        (name.trim().eq_ignore_ascii_case(property)).then(|| value.trim())
    })
}

/// `#RGB` or `#RRGGBB`, and nothing else.
///
/// Named colours, `rgb(…)`, `url(…)` and `var(…)` are not colours Note-it
/// stores, so they are not colours this reads. A refusal is silent and total:
/// the element keeps its text and gains no style.
fn hex_colour(value: &str) -> Option<Color> {
    let digits = value.trim().strip_prefix('#')?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |slice: &str| u8::from_str_radix(slice, 16).ok();
    match digits.len() {
        3 => {
            let mut doubled = String::with_capacity(6);
            for digit in digits.chars() {
                doubled.push(digit);
                doubled.push(digit);
            }
            Some(Color::Rgb(
                channel(&doubled[0..2])?,
                channel(&doubled[2..4])?,
                channel(&doubled[4..6])?,
            ))
        }
        6 => Some(Color::Rgb(
            channel(&digits[0..2])?,
            channel(&digits[2..4])?,
            channel(&digits[4..6])?,
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Entities, code spans, emphasis and links
// ---------------------------------------------------------------------------

/// The character an HTML entity stands for, and how much of the source it
/// took. Decoding happens once: `&amp;lt;` is the four characters `&lt;`.
fn entity(rest: &str) -> Option<(char, usize)> {
    for (spelling, decoded) in [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&apos;", '\''),
        ("&nbsp;", '\u{00A0}'),
    ] {
        if rest.starts_with(spelling) {
            return Some((decoded, spelling.len()));
        }
    }

    let numeric = rest.strip_prefix("&#")?;
    let (digits, radix) = match numeric.strip_prefix(['x', 'X']) {
        Some(hexadecimal) => (hexadecimal, 16),
        None => (numeric, 10),
    };
    let end = digits.find(';')?;
    if end == 0 || end > 8 {
        return None;
    }
    let code = u32::from_str_radix(&digits[..end], radix).ok()?;
    Some((char::from_u32(code)?, rest.len() - digits.len() + end + 1))
}

/// Where a code span opened with `width` backticks closes, counted from just
/// after the opener. A run of a different length is part of the code.
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
            continue;
        }
        index += rest.chars().next()?.len_utf8();
    }

    None
}

/// Where the emphasis opened at `start` closes, or `None` when it never does.
///
/// A run only opens if something follows it immediately, and only closes if
/// something precedes it; `_` may additionally not open or close inside a
/// word. These are `noteit_core::visible_text`'s rules, deliberately, so the
/// reader and a search snippet never disagree about what a delimiter was.
fn emphasis_close(source: &str, start: usize, delimiter: char, width: usize) -> Option<usize> {
    let allowed = if delimiter == '~' {
        width == 2
    } else {
        (1..=3).contains(&width)
    };
    if !allowed {
        return None;
    }

    let rest = &source[start..];
    let inner = &rest[width..];
    if inner.chars().next().is_none_or(char::is_whitespace) {
        return None;
    }
    if delimiter == '_'
        && source[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric())
    {
        return None;
    }

    let mut index = 0;
    while index < inner.len() {
        let rest = &inner[index..];
        let character = rest.chars().next()?;

        if character == '\\' {
            index += 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
            continue;
        }
        if character == '`' {
            let ticks = rest.chars().take_while(|c| *c == '`').count();
            index += match code_span_close(&rest[ticks..], ticks) {
                Some(close) => ticks + close + ticks,
                None => ticks,
            };
            continue;
        }
        if character == delimiter {
            let run = rest.chars().take_while(|c| *c == delimiter).count();
            let closes = run == width
                && index > 0
                && inner[..index]
                    .chars()
                    .next_back()
                    .is_some_and(|c| !c.is_whitespace())
                && (delimiter != '_'
                    || inner[index + run..]
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_alphanumeric()));
            if closes {
                return Some(index);
            }
            index += run;
            continue;
        }
        index += character.len_utf8();
    }

    None
}

/// A `[text](destination)` or `[text][reference]`, as its visible text and the
/// width it occupies.
fn link(rest: &str) -> Option<(&str, usize)> {
    if !rest.starts_with('[') {
        return None;
    }

    let mut depth = 0usize;
    let mut index = 0;
    let mut label_end = None;
    while index < rest.len() {
        let character = rest[index..].chars().next()?;
        match character {
            '\\' => {
                index += 1 + rest[index + 1..].chars().next().map_or(0, char::len_utf8);
                continue;
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    label_end = Some(index);
                    break;
                }
            }
            _ => {}
        }
        index += character.len_utf8();
    }

    let label_end = label_end?;
    let after = &rest[label_end + 1..];
    // A pair of brackets with nothing addressed is a pair of brackets.
    let closing = match after.chars().next() {
        Some('(') => ')',
        Some('[') => ']',
        _ => return None,
    };
    let end = after.find(closing)?;
    Some((&rest[1..label_end], label_end + 1 + end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(source: &str) -> String {
        spans(source, Style::default())
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn a_colour_is_only_a_colour_when_it_is_written_as_one() {
        assert_eq!(hex_colour("#DC2626"), Some(Color::Rgb(0xDC, 0x26, 0x26)));
        assert_eq!(hex_colour("#abc"), Some(Color::Rgb(0xAA, 0xBB, 0xCC)));
        assert_eq!(hex_colour(" #DC2626 "), Some(Color::Rgb(0xDC, 0x26, 0x26)));
        assert_eq!(hex_colour("red"), None);
        assert_eq!(hex_colour("#GG0000"), None);
        assert_eq!(hex_colour("#DC26"), None);
        assert_eq!(hex_colour("rgb(1,2,3)"), None);
        assert_eq!(hex_colour("url(javascript:alert(1))"), None);
        assert_eq!(hex_colour(""), None);
    }

    #[test]
    fn only_the_two_authorised_properties_are_read_from_a_style() {
        let style = "color:#DC2626;background-color:#FDE68A;font-size:32px;position:fixed";
        assert_eq!(css_property(style, "color"), Some("#DC2626"));
        assert_eq!(css_property(style, "background-color"), Some("#FDE68A"));
        assert_eq!(css_property("font-size:32px", "color"), None);
        assert_eq!(css_property("nonsense", "color"), None);
    }

    #[test]
    fn attributes_survive_quoting_and_spacing() {
        let attributes = read_attributes(" data-note-it-color='#DC2626'  style=\"color:#DC2626\" ");
        assert_eq!(value_of(&attributes, "data-note-it-color"), Some("#DC2626"));
        assert_eq!(value_of(&attributes, "style"), Some("color:#DC2626"));
        assert_eq!(value_of(&attributes, "onclick"), None);
    }

    #[test]
    fn a_tag_is_measured_even_when_an_attribute_quotes_a_bracket() {
        let tag = "<span title=\"a > b\">";
        assert_eq!(tag_width(tag), Some(tag.len()));
        assert_eq!(tag_width("<u>"), Some(3));
        assert_eq!(tag_width("<https://exemplo.com>"), None);
        assert_eq!(tag_width("um < dois"), None);
        assert_eq!(tag_width("<span sem fecho"), None);
    }

    #[test]
    fn a_comment_ends_where_it_ends_and_costs_nothing_when_it_does_not() {
        assert_eq!(comment_width("<!-- x -->resto"), Some(10));
        assert_eq!(comment_width("<!-- sem fecho"), Some(4));
        assert_eq!(comment_width("<span>"), None);
    }

    #[test]
    fn control_characters_never_become_cells() {
        let text = rendered("a\u{1b}[31mb\u{7}c\u{9b}d\te");
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{7}'));
        assert!(!text.contains('\u{9b}'));
        assert!(
            text.contains("    "),
            "a tab becomes the spaces it stands for"
        );
        assert!(text.starts_with('a'));
    }

    #[test]
    fn nesting_beyond_the_limit_is_text_and_not_a_deeper_stack() {
        let deep = format!("{}fundo{}", "*".repeat(200), "*".repeat(200));
        assert!(rendered(&deep).contains("fundo"));

        let tags = format!(
            "{}fundo{}",
            "<span data-note-it-color=\"#DC2626\">".repeat(500),
            "</span>".repeat(500)
        );
        assert_eq!(rendered(&tags), "fundo");
    }
}
