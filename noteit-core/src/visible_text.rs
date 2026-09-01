//! Projecting a stored note onto the text a reader actually sees.
//!
//! A note is stored as Markdown, and Markdown says two things at once: the
//! words, and how they are dressed. `# ` names a heading, `**` names emphasis,
//! and `<span data-note-it-color="#64748B">` names a colour Note-it applied.
//! Inside a note that second half is invisible — the editor renders it — but a
//! collapsed title and a search result are not the editor. They were showing
//! the file, and the file spells a coloured phrase
//! `<span data-note-it-color="#64748B" style="color:#64748B">teste de
//! verdade</span>`, which is not something anybody wrote.
//!
//! So this module answers one question: **given what is stored, what does the
//! reader see?** It is the only place that answer is written down, and every
//! presentation surface — the collapsed title, the search palette, the trash —
//! goes through it. Search compares against this projection too, so an
//! attribute name Note-it invented is not something a query can find.
//!
//! Nothing here writes. The Markdown remains the source of truth and is never
//! rewritten to make a label look better: `updated_at` cannot move because no
//! file is opened.
//!
//! ## What it is not
//!
//! It is not a Markdown renderer and it is not a pattern that happens to fit
//! the examples that were reported. It is a scanner over the forms Note-it's
//! own serializer produces — enumerated by round-tripping documents through
//! the real editor — and it reads a foreign `.md` under the same rules,
//! declining anything it cannot recognise rather than guessing. A delimiter
//! with no partner stays the character it is, which is why a note saying
//! `contém .* mesmo assim` still says that.
//!
//! The web page carries the same projection in `ui/src/markdown/visibleText.ts`
//! for the collapsed title, which is decided in the WebView. The two are kept
//! in step by testing both against the same cases; they cannot be one file
//! because a search must read a thousand notes with no WebView at all.

/// Callout kinds Note-it writes as a `[!KIND]` marker on a line of its own.
///
/// The list is the whitelist deliberately: a marker this version does not know
/// is not a marker, it is the text somebody typed, and it stays visible — the
/// same failure mode the editor itself has.
const CALLOUT_KINDS: &[&str] = &["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];

/// Note-it's own task metadata, which is machine bookkeeping rather than text.
///
/// It travels in an HTML comment on the task's line so the file stays ordinary
/// Markdown. The reader never sees it, so neither does a label, a snippet or a
/// query.
const TASK_METADATA_PREFIX: &str = "note-it:";

/// The stored note as the reader sees it: one visible line per stored line.
///
/// Line structure is kept because that is what a label and a snippet are made
/// of — the first line that says something, and a window around a match.
pub fn visible_text(stored: &str) -> String {
    let lines: Vec<&str> = stored
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();

    let mut out = String::with_capacity(stored.len());
    let mut index = 0;
    let mut fence: Option<(char, usize)> = None;

    while index < lines.len() {
        let line = lines[index];
        index += 1;

        // Inside a fence every line is the code somebody typed. It is shown
        // exactly as written, so it is projected exactly as written.
        if let Some((character, width)) = fence {
            if fence_marker(line.trim()).is_some_and(|(c, w)| c == character && w >= width) {
                fence = None;
            } else {
                push_line(&mut out, line.trim_end());
            }
            continue;
        }

        let trimmed = line.trim();

        if let Some(marker) = fence_marker(trimmed) {
            // The fence and its info string are how a code block is written
            // down, not part of it.
            fence = Some(marker);
            continue;
        }
        if trimmed.is_empty() || is_thematic_break(trimmed) {
            push_line(&mut out, "");
            continue;
        }
        if trimmed.starts_with("<!--") {
            let (body, resumed) = read_block_comment(&lines, index - 1);
            index = resumed;
            push_line(&mut out, body.as_deref().unwrap_or(""));
            continue;
        }

        let mut rendered = String::new();
        render_inline(strip_block_markers(trimmed), &mut rendered);
        push_line(&mut out, rendered.trim());
    }

    while out.ends_with('\n') {
        out.pop();
    }
    out
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

/// The opening or closing delimiter of a fenced code block, as (char, width).
fn fence_marker(trimmed: &str) -> Option<(char, usize)> {
    for character in ['`', '~'] {
        let width = trimmed.chars().take_while(|c| *c == character).count();
        if width >= 3 {
            return Some((character, width));
        }
    }
    None
}

/// A horizontal rule: a line that draws something and says nothing.
fn is_thematic_break(trimmed: &str) -> bool {
    for character in ['-', '*', '_'] {
        let stripped: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
        if stripped.len() >= 3 && stripped.chars().all(|c| c == character) {
            return true;
        }
    }
    false
}

/// Reads a whole-block Note-it comment starting at `start`, returning its text
/// and the line to carry on from.
///
/// A comment is stored but it is not hidden: the editor shows it as a small
/// labelled block, so its words are words the reader sees and they stay
/// searchable. What goes is the `<!--` and `-->` around them — and the task
/// metadata comment in its entirety, because that one is never shown at all.
fn read_block_comment(lines: &[&str], start: usize) -> (Option<String>, usize) {
    let mut collected = String::new();
    let mut index = start;

    while index < lines.len() {
        let line = lines[index];
        index += 1;
        if !collected.is_empty() {
            collected.push('\n');
        }
        if let Some(end) = line.find("-->") {
            collected.push_str(&line[..end]);
            let body = collected
                .trim_start()
                .trim_start_matches("<!--")
                .trim()
                .replace("--&gt;", "-->");
            if body.starts_with(TASK_METADATA_PREFIX) {
                return (None, index);
            }
            return (Some(body), index);
        }
        collected.push_str(line);
    }

    // A comment nobody closed. The file is still the file; what it holds is
    // shown rather than swallowed, minus the opener.
    let body = collected
        .trim_start()
        .trim_start_matches("<!--")
        .trim()
        .replace("--&gt;", "-->");
    (Some(body), index)
}

/// Removes the markers that name a line's *kind* rather than saying anything:
/// quote and callout, heading, list and task box.
fn strip_block_markers(line: &str) -> &str {
    let mut text = line;

    loop {
        let stripped = text.trim_start();
        let Some(rest) = stripped.strip_prefix('>') else {
            text = stripped;
            break;
        };
        text = rest;
    }

    if let Some(rest) = strip_callout_marker(text) {
        text = rest;
    }
    if let Some(rest) = strip_heading_marker(text) {
        return rest.trim();
    }
    if let Some(rest) = strip_list_marker(text) {
        text = strip_task_marker(rest).unwrap_or(rest);
    }

    text.trim()
}

/// `[!WARNING]` alone on its line. The kind is decoration Note-it draws as a
/// coloured label; putting the identifier in a title would be showing the
/// reader a word they never typed.
fn strip_callout_marker(text: &str) -> Option<&str> {
    let rest = text.trim_start().strip_prefix("[!")?;
    let end = rest.find(']')?;
    let kind = rest[..end].to_ascii_uppercase();
    if !CALLOUT_KINDS.contains(&kind.as_str()) {
        return None;
    }
    let after = &rest[end + 1..];
    after.trim().is_empty().then_some("")
}

/// `#` through `######`, and only when a space follows: `#tag` is a word.
fn strip_heading_marker(text: &str) -> Option<&str> {
    let hashes = text.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &text[hashes..];
    if rest.is_empty() {
        return Some(rest);
    }
    rest.strip_prefix([' ', '\t'])
}

/// A bullet or an ordered marker, each of which must be followed by a space to
/// be a marker at all — which is what keeps `*grifado*` from being a list.
fn strip_list_marker(text: &str) -> Option<&str> {
    for bullet in ['-', '*', '+'] {
        if let Some(rest) = text.strip_prefix(bullet) {
            if rest.is_empty() {
                return Some(rest);
            }
            if let Some(rest) = rest.strip_prefix([' ', '\t']) {
                return Some(rest.trim_start());
            }
        }
    }

    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let rest = &text[digits..];
        if let Some(rest) = rest.strip_prefix(['.', ')']) {
            if rest.is_empty() {
                return Some(rest);
            }
            if let Some(rest) = rest.strip_prefix([' ', '\t']) {
                return Some(rest.trim_start());
            }
        }
    }
    None
}

/// `[ ]` or `[x]`: the box a task is drawn with, never words.
fn strip_task_marker(text: &str) -> Option<&str> {
    for box_marker in ["[ ]", "[x]", "[X]"] {
        if let Some(rest) = text.strip_prefix(box_marker) {
            if rest.is_empty() {
                return Some(rest);
            }
            if let Some(rest) = rest.strip_prefix([' ', '\t']) {
                return Some(rest.trim_start());
            }
        }
    }
    None
}

/// Everything inside a line: emphasis, code, links, HTML and entities.
fn render_inline(src: &str, out: &mut String) {
    let mut index = 0;

    while index < src.len() {
        let rest = &src[index..];
        let character = rest.chars().next().expect("index is on a char boundary");

        match character {
            // A backslash escape is how the serializer stores a character that
            // would otherwise be a mark. What the reader sees is the character.
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
                match find_code_span_close(&rest[width..], width) {
                    // Code is source: it is shown exactly as typed, so nothing
                    // inside it is unwrapped, decoded or matched.
                    Some(close) => {
                        out.push_str(&rest[width..width + close]);
                        index += width + close + width;
                    }
                    None => {
                        out.push_str(&rest[..width]);
                        index += width;
                    }
                }
            }
            '<' => {
                if let Some(width) = inline_comment_width(rest) {
                    index += width;
                } else if let Some(width) = html_tag_width(rest) {
                    // A raw tag in a stored note is always Note-it's own
                    // serialization: a `<` the reader typed is stored as
                    // `&lt;` and arrives here as text.
                    index += width;
                } else {
                    out.push('<');
                    index += 1;
                }
            }
            '&' => match parse_entity(rest) {
                Some((decoded, width)) => {
                    out.push(decoded);
                    index += width;
                }
                None => {
                    out.push('&');
                    index += 1;
                }
            },
            '!' if rest[1..].starts_with('[') => match parse_link(&rest[1..]) {
                // An image shows its alternative text and nothing else.
                Some((text, width)) => {
                    render_inline(text, out);
                    index += 1 + width;
                }
                None => {
                    out.push('!');
                    index += 1;
                }
            },
            '[' => match parse_link(rest) {
                // A link shows its words. The destination is not on screen —
                // the editor's own find cannot reach it either.
                Some((text, width)) => {
                    render_inline(text, out);
                    index += width;
                }
                None => {
                    out.push('[');
                    index += 1;
                }
            },
            '*' | '_' | '~' => {
                let width = rest.chars().take_while(|c| *c == character).count();
                match emphasis_span(src, index, character, width) {
                    Some(close) => {
                        render_inline(&rest[width..width + close], out);
                        index += width + close + width;
                    }
                    None => {
                        out.push_str(&rest[..width]);
                        index += width;
                    }
                }
            }
            _ => {
                out.push(character);
                index += character.len_utf8();
            }
        }
    }
}

/// Where the emphasis opened at `start` closes, or `None` when it never does.
///
/// A run only opens if something follows it immediately — `a ** b` is two
/// asterisks somebody typed — and only closes if something precedes it. `_`
/// additionally may not open or close inside a word, which is what leaves
/// `note_it_config` alone.
fn emphasis_span(src: &str, start: usize, delimiter: char, width: usize) -> Option<usize> {
    let allowed = if delimiter == '~' {
        width == 2
    } else {
        (1..=3).contains(&width)
    };
    if !allowed {
        return None;
    }

    let rest = &src[start..];
    let inner = &rest[width..];
    if inner.chars().next().is_none_or(char::is_whitespace) {
        return None;
    }
    if delimiter == '_'
        && src[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric())
    {
        return None;
    }

    find_emphasis_close(inner, delimiter, width)
}

fn find_emphasis_close(haystack: &str, delimiter: char, width: usize) -> Option<usize> {
    let mut index = 0;

    while index < haystack.len() {
        let rest = &haystack[index..];
        let character = rest.chars().next()?;

        if character == '\\' {
            index += 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
            continue;
        }
        if character == '`' {
            let ticks = rest.chars().take_while(|c| *c == '`').count();
            index += match find_code_span_close(&rest[ticks..], ticks) {
                Some(close) => ticks + close + ticks,
                None => ticks,
            };
            continue;
        }
        if character == delimiter {
            let run = rest.chars().take_while(|c| *c == delimiter).count();
            let closes = run == width
                && index > 0
                && haystack[..index]
                    .chars()
                    .next_back()
                    .is_some_and(|c| !c.is_whitespace())
                && (delimiter != '_'
                    || haystack[index + run..]
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

/// Where a code span opened with `width` backticks closes, counted from just
/// after the opener. A run of a different length is part of the code.
fn find_code_span_close(haystack: &str, width: usize) -> Option<usize> {
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

/// The width of `<!-- ... -->` appearing inside a line, which in a stored note
/// is Note-it's task metadata and is never on screen.
fn inline_comment_width(rest: &str) -> Option<usize> {
    rest.starts_with("<!--")
        .then(|| rest.find("-->").map(|end| end + 3))
        .flatten()
}

/// The width of an HTML tag at the start of `rest`, or `None` if this `<` opens
/// no tag — `<https://exemplo.com>` and a bare `<` among words included.
fn html_tag_width(rest: &str) -> Option<usize> {
    let mut chars = rest.char_indices();
    chars.next()?;

    let mut index = 1;
    if rest[index..].starts_with('/') {
        index += 1;
    }
    let name = rest[index..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .count();
    if name == 0 || !rest[index..].starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    index += name;

    // A name has to be followed by the end of the tag or by attributes; a `:`
    // or a `/` right after it means this was never a tag.
    match rest[index..].chars().next() {
        Some('>') => return Some(index + 1),
        Some(c) if c.is_whitespace() => {}
        Some('/') if rest[index..].starts_with("/>") => return Some(index + 2),
        _ => return None,
    }

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

/// The character an HTML entity stands for, and how much of the source it took.
///
/// Only the entities Note-it's serializer writes, plus numeric references. An
/// `&` that begins nothing recognisable is an ampersand somebody typed.
fn parse_entity(rest: &str) -> Option<(char, usize)> {
    for (entity, decoded) in [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&apos;", '\''),
        ("&nbsp;", '\u{00A0}'),
    ] {
        if rest.starts_with(entity) {
            return Some((decoded, entity.len()));
        }
    }

    let numeric = rest.strip_prefix("&#")?;
    let (digits, radix) = match numeric.strip_prefix(['x', 'X']) {
        Some(hex) => (hex, 16),
        None => (numeric, 10),
    };
    let end = digits.find(';')?;
    if end == 0 || end > 8 {
        return None;
    }
    let code = u32::from_str_radix(&digits[..end], radix).ok()?;
    let decoded = char::from_u32(code)?;
    Some((decoded, rest.len() - digits.len() + end + 1))
}

/// A `[text](destination)` or `[text][reference]` starting at `rest`, as its
/// visible text and the width it occupies.
fn parse_link(rest: &str) -> Option<(&str, usize)> {
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
    let text = &rest[1..label_end];
    let after = &rest[label_end + 1..];

    let closing = match after.chars().next() {
        Some('(') => ')',
        Some('[') => ']',
        // A pair of brackets with nothing addressed is a pair of brackets.
        _ => return None,
    };
    let end = after.find(closing)?;
    Some((text, label_end + 1 + end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus both projections are held to.
    ///
    /// The same file is read by the page's tests, because the host and the
    /// WebView each carry an implementation and two implementations that are
    /// only *described* as equivalent drift. Every case is a stored note and
    /// the text a reader sees in it.
    const CASES: &str = include_str!("../../tests/visible_text_cases.json");

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        name: String,
        stored: String,
        visible: String,
        /// Set on the cases where Note-it's own spelling *is* the text: a
        /// reader who typed `<span` had it stored escaped, and a reader who
        /// quoted an attribute in code meant to quote it. Those must survive
        /// the projection, so they are exempt from the leak sweep.
        #[serde(default)]
        reader_typed: bool,
    }

    fn cases() -> Vec<Case> {
        serde_json::from_str(CASES).expect("the shared corpus must parse")
    }

    /// Every spelling of Note-it's storage that could reach a reader's eye.
    const STORAGE_SPELLINGS: &[&str] = &[
        "data-note-it-color",
        "data-note-it-highlight",
        "data-note-it-font-size",
        "note-it:completed_at",
        "<span",
        "</span>",
        "<mark",
        "</mark>",
        "<u>",
        "</u>",
        "background-color",
        "-->",
    ];

    #[test]
    fn nothing_the_corpus_projects_carries_storage_syntax() {
        for case in cases() {
            if case.reader_typed {
                continue;
            }
            for spelling in STORAGE_SPELLINGS {
                assert!(
                    !case.visible.contains(spelling),
                    "case {} leaks {spelling:?}",
                    case.name,
                );
            }
        }
    }

    #[test]
    fn an_unpaired_delimiter_is_never_read_as_a_mark() {
        // The reason this is a scanner and not a pattern: a partner has to
        // exist for a delimiter to be one.
        assert_eq!(
            visible_text("isto contém .* mesmo assim"),
            "isto contém .* mesmo assim"
        );
        assert_eq!(visible_text("a ** b ** c"), "a ** b ** c");
        assert_eq!(
            visible_text("caminho ~/Downloads e ~5 min"),
            "caminho ~/Downloads e ~5 min"
        );
        assert_eq!(
            visible_text("note_it_config e snake_case"),
            "note_it_config e snake_case"
        );
        assert_eq!(visible_text("3 * 4 * 5"), "3 * 4 * 5");
        assert_eq!(visible_text("veja [isto] sozinho"), "veja [isto] sozinho");
        assert_eq!(visible_text("um < dois"), "um < dois");
    }

    #[test]
    fn a_note_of_only_storage_projects_to_nothing_at_all() {
        assert!(visible_text("<!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->").is_empty());
        assert!(visible_text("---").is_empty());
        assert!(visible_text("").is_empty());
    }

    #[test]
    fn projecting_reads_and_only_reads() {
        // Presentation cannot move `updated_at` because presentation cannot
        // reach a file: this takes a string and returns a string.
        let stored = "# <span data-note-it-color=\"#64748B\" style=\"color:#64748B\">Título</span>";
        let before = stored.to_string();
        assert_eq!(visible_text(stored), "Título");
        assert_eq!(stored, before);
    }

    #[test]
    fn the_shared_corpus_is_projected_exactly() {
        for case in cases() {
            assert_eq!(
                visible_text(&case.stored),
                case.visible,
                "case: {}\nstored: {:?}",
                case.name,
                case.stored,
            );
        }
    }
}
