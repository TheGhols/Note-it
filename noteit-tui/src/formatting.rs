//! Canonical inline colour markup shared semantically with the graphical editor.

pub const TEXT_COLORS: &[(&str, &str)] = &[
    ("Cinza", "#64748B"),
    ("Vermelho", "#DC2626"),
    ("Laranja", "#C2410C"),
    ("Amarelo", "#A16207"),
    ("Verde", "#15803D"),
    ("Azul", "#2563EB"),
    ("Roxo", "#7C3AED"),
    ("Rosa", "#DB2777"),
];

pub const HIGHLIGHT_COLORS: &[(&str, &str)] = &[
    ("Amarelo", "#FDE68A"),
    ("Verde", "#BBF7D0"),
    ("Azul", "#BFDBFE"),
    ("Rosa", "#FBCFE8"),
    ("Roxo", "#DDD6FE"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    TextColor,
    Highlight,
}

pub fn combined_wrapper(color: Option<&str>, highlight: Option<&str>) -> (String, String) {
    match (color, highlight) {
        (Some(color), Some(highlight)) => (
            format!("<mark data-note-it-highlight=\"{highlight}\" style=\"background-color:{highlight}\"><span data-note-it-color=\"{color}\" style=\"color:{color}\">"),
            "</span></mark>".into(),
        ),
        (Some(color), None) => { let (open, close) = wrapper(Kind::TextColor, color); (open, close.into()) }
        (None, Some(highlight)) => { let (open, close) = wrapper(Kind::Highlight, highlight); (open, close.into()) }
        (None, None) => (String::new(), String::new()),
    }
}

pub fn escaped_typed(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            other => other.to_string(),
        })
        .collect()
}

pub fn wrapper(kind: Kind, color: &str) -> (String, &'static str) {
    match kind {
        Kind::TextColor => (
            format!("<span data-note-it-color=\"{color}\" style=\"color:{color}\">"),
            "</span>",
        ),
        Kind::Highlight => (
            format!("<mark data-note-it-highlight=\"{color}\" style=\"background-color:{color}\">"),
            "</mark>",
        ),
    }
}

/// Removes only complete canonical wrappers present in the selected source.
/// Unknown HTML and incomplete tags are deliberately left untouched.
pub fn clear_selected(source: &str, kind: Kind) -> String {
    let (needle, close) = match kind {
        Kind::TextColor => ("<span", "</span>"),
        Kind::Highlight => ("<mark", "</mark>"),
    };
    let attribute = match kind {
        Kind::TextColor => "data-note-it-color=",
        Kind::Highlight => "data-note-it-highlight=",
    };
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find(needle) {
        out.push_str(&rest[..start]);
        let tag = &rest[start..];
        let Some(end) = tag.find('>') else {
            out.push_str(tag);
            return out;
        };
        let opening = &tag[..=end];
        if !opening.contains(attribute) {
            out.push_str(opening);
            rest = &tag[end + 1..];
            continue;
        }
        let after = &tag[end + 1..];
        let Some(close_at) = after.find(close) else {
            out.push_str(tag);
            return out;
        };
        out.push_str(&after[..close_at]);
        rest = &after[close_at + close.len()..];
    }
    out.push_str(rest);
    out
}

/// Removes the innermost canonical wrapper that encloses the selected byte range.
pub fn clear_enclosing(
    source: &str,
    start: usize,
    end: usize,
    kind: Kind,
) -> Option<(String, usize, usize)> {
    let (needle, close, attribute) = match kind {
        Kind::TextColor => ("<span", "</span>", "data-note-it-color="),
        Kind::Highlight => ("<mark", "</mark>", "data-note-it-highlight="),
    };
    let open_at = source[..start].rfind(needle)?;
    let open_end = open_at + source[open_at..].find('>')? + 1;
    if open_end > start || !source[open_at..open_end].contains(attribute) {
        return None;
    }
    let close_at = end + source[end..].find(close)?;
    // A same-kind closer before the selection means the candidate does not enclose it.
    if source[open_end..start].contains(close) {
        return None;
    }
    let mut result = source.to_owned();
    result.replace_range(close_at..close_at + close.len(), "");
    result.replace_range(open_at..open_end, "");
    let removed = open_end - open_at;
    Some((result, start - removed, end - removed))
}
