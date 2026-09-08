//! Read-only Markdown presentation for Note-it TUI.
//!
//! Formats Markdown documents into styled Ratatui lines for the reader pane.
//! Operates in-process without modifying any stored data or evaluating mathematical expressions.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// The five GitHub Flavored Markdown (GFM) callout kinds supported by Note-it (Phase 3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl CalloutKind {
    pub fn parse(tag: &str) -> Option<Self> {
        match tag.to_ascii_uppercase().as_str() {
            "NOTE" => Some(Self::Note),
            "TIP" => Some(Self::Tip),
            "IMPORTANT" => Some(Self::Important),
            "WARNING" => Some(Self::Warning),
            "CAUTION" => Some(Self::Caution),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Note => "[NOTA]",
            Self::Tip => "[DICA]",
            Self::Important => "[IMPORTANTE]",
            Self::Warning => "[AVISO]",
            Self::Caution => "[CUIDADO]",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Note => Color::Cyan,
            Self::Tip => Color::Green,
            Self::Important => Color::Magenta,
            Self::Warning => Color::Yellow,
            Self::Caution => Color::Red,
        }
    }
}

/// Renders raw Markdown content into styled Ratatui lines.
pub fn render_markdown(content: &str) -> Vec<Line<'static>> {
    render_with_sources(content).lines
}

/// Presentation is unchanged; attach source positions for the reading cursor.
#[derive(Default)]
pub struct RenderedMarkdown {
    pub lines: Vec<Line<'static>>,
    pub sources: Vec<usize>,
    source: usize,
}

impl RenderedMarkdown {
    fn push(&mut self, line: Line<'static>) {
        self.lines.push(line);
        self.sources.push(self.source);
    }
}

pub fn render_with_sources(content: &str) -> RenderedMarkdown {
    let mut lines = RenderedMarkdown::default();
    let raw_lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < raw_lines.len() {
        lines.source = i;
        let line = raw_lines[i];
        let trimmed = line.trim();

        // 1. Fenced Code Blocks (``` or ~~~)
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let fence_char = trimmed.chars().next().unwrap();
            let fence_prefix = if fence_char == '`' { "```" } else { "~~~" };
            let lang = trimmed.trim_start_matches(fence_char).trim();
            let lang_display = if lang.is_empty() {
                "código".to_string()
            } else {
                lang.to_string()
            };

            // Top boundary
            lines.push(Line::from(vec![
                Span::styled("┌── [", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    lang_display,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "] ──────────────────────────────────────",
                    Style::default().fg(Color::DarkGray),
                ),
            ]));

            i += 1;
            while i < raw_lines.len() {
                lines.source = i;
                let code_line = raw_lines[i];
                if code_line.trim().starts_with(fence_prefix) {
                    i += 1;
                    break;
                }
                // Plain code content without syntax highlighting
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(code_line.to_string(), Style::default().fg(Color::White)),
                ]));
                i += 1;
            }

            // Bottom boundary
            lines.push(Line::from(Span::styled(
                "└─────────────────────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        // 2. Blockquotes & GFM Callouts / Alerts (> ...)
        if trimmed.starts_with('>') {
            let quote_content = trimmed.strip_prefix('>').unwrap().trim_start();

            // Check if this is the start of a GFM callout: > [!KIND]
            if let Some(callout) = check_callout_marker(quote_content) {
                let callout_color = callout.color();
                let callout_style = Style::default()
                    .fg(callout_color)
                    .add_modifier(Modifier::BOLD);

                // Alert header line: ▍ [RÓTULO]
                lines.push(Line::from(vec![
                    Span::styled("▍ ", Style::default().fg(callout_color)),
                    Span::styled(callout.label(), callout_style),
                ]));

                i += 1;
                while i < raw_lines.len() {
                    lines.source = i;
                    let next_line = raw_lines[i].trim();
                    if !next_line.starts_with('>') {
                        break;
                    }
                    let inner = next_line.strip_prefix('>').unwrap().trim_start();
                    let mut spans = vec![Span::styled("▍ ", Style::default().fg(callout_color))];
                    spans.extend(parse_inlines(inner, Style::default().fg(Color::White)));
                    lines.push(Line::from(spans));
                    i += 1;
                }
                continue;
            } else {
                // Standard blockquote (not a callout)
                let mut spans = vec![Span::styled("│ ", Style::default().fg(Color::Cyan))];
                spans.extend(parse_inlines(
                    quote_content,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::ITALIC),
                ));
                lines.push(Line::from(spans));
                i += 1;
                continue;
            }
        }

        // 3. Headings (# to ######)
        if trimmed.starts_with('#') {
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if hash_count <= 6 && trimmed[hash_count..].starts_with(' ') {
                let heading_text = trimmed[hash_count..].trim();
                let (prefix, style) = match hash_count {
                    1 => (
                        "# ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    2 => (
                        "## ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    3 => (
                        "### ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    4 => (
                        "#### ",
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    5 => (
                        "##### ",
                        Style::default()
                            .fg(Color::Gray)
                            .add_modifier(Modifier::BOLD),
                    ),
                    _ => (
                        "###### ",
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    ),
                };

                let mut spans = vec![Span::styled(prefix, style)];
                spans.extend(parse_inlines(heading_text, style));
                lines.push(Line::from(spans));
                i += 1;
                continue;
            }
        }

        // 4. Thematic breaks / Horizontal rules (---, ***, ___)
        if (trimmed.starts_with("---") || trimmed.starts_with("***") || trimmed.starts_with("___"))
            && trimmed
                .chars()
                .all(|c| c == '-' || c == '*' || c == '_' || c.is_whitespace())
            && trimmed.len() >= 3
        {
            lines.push(Line::from(Span::styled(
                "─────────────────────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
            i += 1;
            continue;
        }

        // 5. Task Checkboxes (- [ ] or - [x] / - [X])
        if let Some((checked, task_text, indent)) = parse_task_line(line) {
            let (cleaned_text, is_checked) = if checked {
                (task_text, true)
            } else {
                (task_text, false)
            };

            let indent_spaces = " ".repeat(indent);
            let mut spans = Vec::new();
            if !indent_spaces.is_empty() {
                spans.push(Span::raw(indent_spaces));
            }

            if is_checked {
                spans.push(Span::styled(
                    "☑ ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.extend(parse_inlines(
                    &cleaned_text,
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::CROSSED_OUT),
                ));
            } else {
                spans.push(Span::styled(
                    "☐ ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.extend(parse_inlines(
                    &cleaned_text,
                    Style::default().fg(Color::White),
                ));
            }

            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        // 6. Bullet Lists (- , * , + )
        if let Some((bullet_char, item_text, indent)) = parse_bullet_list(line) {
            let indent_spaces = " ".repeat(indent);
            let mut spans = Vec::new();
            if !indent_spaces.is_empty() {
                spans.push(Span::raw(indent_spaces));
            }
            let _ = bullet_char;
            spans.push(Span::styled("• ", Style::default().fg(Color::Cyan)));
            spans.extend(parse_inlines(&item_text, Style::default().fg(Color::White)));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        // 7. Numbered Lists (1. , 2. , etc.)
        if let Some((num_str, item_text, indent)) = parse_numbered_list(line) {
            let indent_spaces = " ".repeat(indent);
            let mut spans = Vec::new();
            if !indent_spaces.is_empty() {
                spans.push(Span::raw(indent_spaces));
            }
            spans.push(Span::styled(
                format!("{num_str} "),
                Style::default().fg(Color::Cyan),
            ));
            spans.extend(parse_inlines(&item_text, Style::default().fg(Color::White)));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        // 8. Normal Paragraph / Plain Text (including math lines like = 2 + 2 or a := 5, rendered as-is)
        if trimmed.is_empty() {
            lines.push(Line::from(""));
        } else {
            let spans = parse_inlines(line, Style::default().fg(Color::White));
            lines.push(Line::from(spans));
        }

        i += 1;
    }

    lines
}

/// Helper to identify GFM callouts: matches `[!KIND]`
fn check_callout_marker(text: &str) -> Option<CalloutKind> {
    let trimmed = text.trim();
    if !trimmed.starts_with("[!") || !trimmed.ends_with(']') {
        return None;
    }
    let kind_str = &trimmed[2..trimmed.len() - 1];
    CalloutKind::parse(kind_str)
}

/// Helper to parse task checklist items: `- [ ] ...` or `- [x] ...`
fn parse_task_line(line: &str) -> Option<(bool, String, usize)> {
    let indent = line.chars().take_while(|c| c.is_whitespace()).count();
    let trimmed = line.trim();

    let (checked, rest) = if let Some(r) = trimmed
        .strip_prefix("- [ ] ")
        .or_else(|| trimmed.strip_prefix("* [ ] "))
    {
        (false, r)
    } else {
        let r = trimmed
            .strip_prefix("- [x] ")
            .or_else(|| trimmed.strip_prefix("- [X] "))
            .or_else(|| trimmed.strip_prefix("* [x] "))
            .or_else(|| trimmed.strip_prefix("* [X] "))?;
        (true, r)
    };

    // Reusing noteit_core::task::extract_completed_at to strip completion metadata
    let (_, cleaned) = noteit_core::task::extract_completed_at(rest);
    Some((checked, cleaned, indent))
}

/// Helper to parse bullet list items: `- `, `* `, `+ `
fn parse_bullet_list(line: &str) -> Option<(char, String, usize)> {
    let indent = line.chars().take_while(|c| c.is_whitespace()).count();
    let trimmed = line.trim();

    if trimmed.starts_with("- [") || trimmed.starts_with("* [") {
        return None;
    }

    let first_char = trimmed.chars().next()?;
    if (first_char == '-' || first_char == '*' || first_char == '+') && trimmed.len() >= 2 {
        let second = trimmed.chars().nth(1)?;
        if second.is_whitespace() {
            let content = trimmed[2..].trim_start().to_string();
            return Some((first_char, content, indent));
        }
    }

    None
}

/// Helper to parse numbered list items: `1. `, `2. `, etc.
fn parse_numbered_list(line: &str) -> Option<(String, String, usize)> {
    let indent = line.chars().take_while(|c| c.is_whitespace()).count();
    let trimmed = line.trim();

    let dot_pos = trimmed.find('.')?;
    if dot_pos > 0 && dot_pos <= 6 {
        let prefix = &trimmed[..dot_pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) {
            let after_dot = &trimmed[dot_pos + 1..];
            if after_dot.starts_with(' ') {
                let num_str = format!("{prefix}.");
                let content = after_dot.trim_start().to_string();
                return Some((num_str, content, indent));
            }
        }
    }

    None
}

/// Parses inline Markdown markers (**bold**, *italic*, `code`) into styled Spans.
fn parse_inlines(text: &str, base_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut current = String::new();

    while i < chars.len() {
        // Inline code `...`
        if chars[i] == '`' {
            if let Some(end_offset) = chars[i + 1..].iter().position(|&c| c == '`') {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), base_style));
                    current.clear();
                }
                let code_content: String = chars[i + 1..i + 1 + end_offset].iter().collect();
                spans.push(Span::styled(
                    code_content,
                    Style::default().fg(Color::Yellow),
                ));
                i = i + 1 + end_offset + 1;
                continue;
            }
        }

        // Bold **...**
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            let remaining = &chars[i + 2..];
            if let Some(pos) = remaining
                .windows(2)
                .position(|w| w[0] == '*' && w[1] == '*')
            {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), base_style));
                    current.clear();
                }
                let bold_text: String = chars[i + 2..i + 2 + pos].iter().collect();
                spans.push(Span::styled(
                    bold_text,
                    base_style.add_modifier(Modifier::BOLD),
                ));
                i = i + 2 + pos + 2;
                continue;
            }
        }

        // Italic *...*
        if chars[i] == '*' && (i + 1 < chars.len() && chars[i + 1] != '*') {
            if let Some(end_offset) = chars[i + 1..].iter().position(|&c| c == '*') {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), base_style));
                    current.clear();
                }
                let italic_text: String = chars[i + 1..i + 1 + end_offset].iter().collect();
                spans.push(Span::styled(
                    italic_text,
                    base_style.add_modifier(Modifier::ITALIC),
                ));
                i = i + 1 + end_offset + 1;
                continue;
            }
        }

        current.push(chars[i]);
        i += 1;
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, base_style));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_headings() {
        let md = "# Heading 1\n## Heading 2\n### Heading 3";
        let lines = render_markdown(md);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].to_string().contains("# Heading 1"));
        assert!(lines[1].to_string().contains("## Heading 2"));
        assert!(lines[2].to_string().contains("### Heading 3"));
    }

    #[test]
    fn test_render_lists_and_tasks() {
        let md = "- Item um\n- Item dois\n- [ ] Tarefa pendente\n- [x] Tarefa concluída <!-- note-it:completed_at=2026-09-06T10:00:00Z -->";
        let lines = render_markdown(md);
        assert_eq!(lines.len(), 4);
        assert!(lines[0].to_string().contains("• Item um"));
        assert!(lines[1].to_string().contains("• Item dois"));
        assert!(lines[2].to_string().contains("☐ Tarefa pendente"));
        assert!(lines[3].to_string().contains("☑ Tarefa concluída"));
        // Ensure note-it comment was stripped
        assert!(!lines[3].to_string().contains("note-it:completed_at"));
    }

    #[test]
    fn test_render_callouts_and_blockquotes() {
        let md = "> [!NOTE]\n> Corpo da nota\n\n> [!TIP]\n> Corpo da dica\n\n> [!WARNING]\n> Corpo do aviso\n\n> Citação comum";
        let lines = render_markdown(md);
        let rendered: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        let full = rendered.join("\n");
        assert!(full.contains("[NOTA]"));
        assert!(full.contains("Corpo da nota"));
        assert!(full.contains("[DICA]"));
        assert!(full.contains("Corpo da dica"));
        assert!(full.contains("[AVISO]"));
        assert!(full.contains("Corpo do aviso"));
        assert!(full.contains("│ Citação comum"));
    }

    #[test]
    fn test_render_code_block() {
        let md = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
        let lines = render_markdown(md);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        let full = text.join("\n");
        assert!(full.contains("┌── [rust]"));
        assert!(full.contains("│ fn main() {"));
        assert!(full.contains("│     println!(\"hello\");"));
        assert!(full.contains("└─────────"));
    }

    #[test]
    fn test_math_remains_raw() {
        let md = "= 10 + 20\nx := 100";
        let lines = render_markdown(md);
        assert_eq!(lines[0].to_string(), "= 10 + 20");
        assert_eq!(lines[1].to_string(), "x := 100");
    }
}
