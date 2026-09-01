//! Task item extraction and read projection for Note-it Markdown.
//!
//! Tasks are represented as checklist items in Markdown:
//! - `- [ ] Pending task`
//! - `- [x] Completed task <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->`
//! - `- [X] Completed task`
//!
//! Fenced code blocks and YAML front matter are excluded from task extraction.
//! Completed tasks without a valid completion comment have `completed_at: None`
//! and never invent a timestamp.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Filter for selecting tasks based on their completion state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaskStateFilter {
    #[default]
    Pending,
    Completed,
    All,
}

impl TaskStateFilter {
    pub fn matches(&self, checked: bool) -> bool {
        match self {
            Self::Pending => !checked,
            Self::Completed => checked,
            Self::All => true,
        }
    }
}

/// A single extracted task item projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEntry {
    pub note_id: Uuid,
    pub note_label: String,
    pub text: String,
    pub checked: bool,
    pub completed_at: Option<DateTime<Utc>>,
    pub depth: usize,
    pub line_number: usize,
}

/// ISO 8601 validation matching the TypeScript taskMeta.ts specification.
/// Requires explicit offset or 'Z' timezone. Bare local timestamps are invalid.
fn is_valid_iso_8601_with_zone(candidate: &str) -> bool {
    let bytes = candidate.as_bytes();
    if bytes.len() < 20 {
        return false;
    }
    if !candidate.contains('T') {
        return false;
    }
    let has_valid_zone = candidate.ends_with('Z')
        || (candidate.len() >= 6
            && (candidate.as_bytes()[candidate.len() - 6] == b'+'
                || candidate.as_bytes()[candidate.len() - 6] == b'-')
            && candidate.as_bytes()[candidate.len() - 3] == b':');

    if !has_valid_zone {
        return false;
    }

    DateTime::parse_from_rfc3339(candidate).is_ok()
}

/// Extracts `completed_at` timestamp comment from a task line and returns clean text.
/// Format: `<!-- note-it:completed_at=ISO8601 -->`
pub fn extract_completed_at(raw_text: &str) -> (Option<DateTime<Utc>>, String) {
    let comment_marker = "note-it:completed_at=";
    let Some(start_idx) = raw_text.find("<!--") else {
        return (None, raw_text.trim_end().to_string());
    };

    let rest = &raw_text[start_idx..];
    let Some(end_rel) = rest.find("-->") else {
        return (None, raw_text.trim_end().to_string());
    };

    let full_comment = &rest[..end_rel + 3];
    let inside = full_comment[4..full_comment.len() - 3].trim();

    if let Some(candidate_with_prefix) = inside.strip_prefix("note-it:completed_at=") {
        let candidate = candidate_with_prefix.trim();
        let parsed = if is_valid_iso_8601_with_zone(candidate) {
            DateTime::parse_from_rfc3339(candidate)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        } else {
            None
        };

        let mut cleaned = String::with_capacity(raw_text.len());
        cleaned.push_str(&raw_text[..start_idx]);
        cleaned.push_str(&rest[end_rel + 3..]);
        return (parsed, cleaned.trim_end().to_string());
    } else if inside.contains(comment_marker) {
        let mut cleaned = String::with_capacity(raw_text.len());
        cleaned.push_str(&raw_text[..start_idx]);
        cleaned.push_str(&rest[end_rel + 3..]);
        return (None, cleaned.trim_end().to_string());
    }

    (None, raw_text.trim_end().to_string())
}

/// Checks if a line opens or closes a fenced code block (``` or ~~~).
fn check_code_fence(
    line: &str,
    current_fence: Option<(char, usize)>,
) -> Option<Option<(char, usize)>> {
    let trimmed = line.trim_start();
    let fence_char = trimmed.chars().next()?;
    if fence_char != '`' && fence_char != '~' {
        return None;
    }

    let count = trimmed.chars().take_while(|&c| c == fence_char).count();
    if count < 3 {
        return None;
    }

    match current_fence {
        None => Some(Some((fence_char, count))),
        Some((open_char, open_count)) => {
            if fence_char == open_char && count >= open_count {
                let after_fence = trimmed[count..].trim();
                if after_fence.is_empty() {
                    Some(None)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

/// Calculates indentation depth from leading whitespace.
/// 2 spaces or 1 tab counts as 1 level of nesting.
fn calculate_indent_depth(line: &str) -> usize {
    let mut spaces = 0;
    let mut tabs = 0;
    for ch in line.chars() {
        match ch {
            ' ' => spaces += 1,
            '\t' => tabs += 1,
            _ => break,
        }
    }
    tabs + (spaces / 2)
}

/// Parses task lines from Markdown body content.
pub fn parse_tasks(note_id: Uuid, note_label: &str, content: &str) -> Vec<TaskEntry> {
    let mut tasks = Vec::new();
    let mut code_fence: Option<(char, usize)> = None;

    for (line_idx, line) in content.lines().enumerate() {
        let line_number = line_idx + 1;

        if let Some(new_fence) = check_code_fence(line, code_fence) {
            code_fence = new_fence;
            continue;
        }

        if code_fence.is_some() {
            continue;
        }

        let depth = calculate_indent_depth(line);
        let trimmed = line.trim_start();

        let (checked, task_text) = if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
            (false, rest)
        } else if let Some(rest) = trimmed.strip_prefix("- [x] ") {
            (true, rest)
        } else if let Some(rest) = trimmed.strip_prefix("- [X] ") {
            (true, rest)
        } else if let Some(rest) = trimmed.strip_prefix("* [ ] ") {
            (false, rest)
        } else if let Some(rest) = trimmed.strip_prefix("* [x] ") {
            (true, rest)
        } else if let Some(rest) = trimmed.strip_prefix("* [X] ") {
            (true, rest)
        } else {
            continue;
        };

        let (completed_at, clean_text) = extract_completed_at(task_text);
        let final_completed_at = if checked { completed_at } else { None };

        tasks.push(TaskEntry {
            note_id,
            note_label: note_label.to_string(),
            text: clean_text,
            checked,
            completed_at: final_completed_at,
            depth,
            line_number,
        });
    }

    tasks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pending_and_completed_tasks() {
        let id = Uuid::new_v4();
        let markdown = "\
# Note Title

- [ ] Pending task 1
- [x] Completed task 1 <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->
- [X] Completed task uppercase X
- [ ] Another pending task
";
        let tasks = parse_tasks(id, "Note Title", markdown);
        assert_eq!(tasks.len(), 4);

        assert_eq!(tasks[0].text, "Pending task 1");
        assert!(!tasks[0].checked);
        assert_eq!(tasks[0].completed_at, None);
        assert_eq!(tasks[0].depth, 0);

        assert_eq!(tasks[1].text, "Completed task 1");
        assert!(tasks[1].checked);
        assert!(tasks[1].completed_at.is_some());

        assert_eq!(tasks[2].text, "Completed task uppercase X");
        assert!(tasks[2].checked);
        assert_eq!(tasks[2].completed_at, None);

        assert_eq!(tasks[3].text, "Another pending task");
        assert!(!tasks[3].checked);
    }

    #[test]
    fn tasks_inside_fenced_code_blocks_are_ignored() {
        let id = Uuid::new_v4();
        let markdown = "\
- [ ] Real task before code

```markdown
- [ ] Fake task in 3-backtick code block
- [x] Fake completed in code block
```

- [ ] Real task between code blocks

~~~rust
- [ ] Fake task in tilde code block
~~~

````
```markdown
- [ ] Fake task inside nested 4-backtick code fence
```
````

- [x] Real task after code
";
        let tasks = parse_tasks(id, "Code note", markdown);
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].text, "Real task before code");
        assert_eq!(tasks[1].text, "Real task between code blocks");
        assert_eq!(tasks[2].text, "Real task after code");
    }

    #[test]
    fn nested_tasks_preserve_depth() {
        let id = Uuid::new_v4();
        let markdown = "\
- [ ] Top level task
  - [ ] Level 1 nested task
    - [x] Level 2 nested task
\t- [ ] Level 1 nested with tab
";
        let tasks = parse_tasks(id, "Nested note", markdown);
        assert_eq!(tasks.len(), 4);
        assert_eq!(tasks[0].depth, 0);
        assert_eq!(tasks[1].depth, 1);
        assert_eq!(tasks[2].depth, 2);
        assert_eq!(tasks[3].depth, 1);
    }

    #[test]
    fn invalid_completion_timestamp_falls_back_to_none_without_inventing_date() {
        let id = Uuid::new_v4();
        let markdown = "\
- [x] Task with invalid date <!-- note-it:completed_at=not-a-date -->
- [x] Task with bare local date <!-- note-it:completed_at=2026-08-27T11:32:00 -->
- [x] Task without date comment
";
        let tasks = parse_tasks(id, "Invalid date note", markdown);
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].completed_at, None);
        assert_eq!(tasks[0].text, "Task with invalid date");

        assert_eq!(tasks[1].completed_at, None);
        assert_eq!(tasks[1].text, "Task with bare local date");

        assert_eq!(tasks[2].completed_at, None);
        assert_eq!(tasks[2].text, "Task without date comment");
    }

    #[test]
    fn unchecked_task_with_comment_drops_timestamp() {
        let id = Uuid::new_v4();
        let markdown =
            "- [ ] Unchecked task <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->";
        let tasks = parse_tasks(id, "Unchecked note", markdown);
        assert_eq!(tasks.len(), 1);
        assert!(!tasks[0].checked);
        assert_eq!(tasks[0].completed_at, None);
        assert_eq!(tasks[0].text, "Unchecked task");
    }
}
