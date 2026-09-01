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

use crate::hashing::fnv1a_64_of_parts;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
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
    /// The optimistic reference a write command names this task by.
    ///
    /// See [`TaskRef`]: it identifies the task *in the note as it is right
    /// now*, and stops matching as soon as the task itself changes.
    pub task_ref: TaskRef,
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
///
/// Complies with the TypeScript `taskMeta.ts` contract:
/// - Locates `<!--\s*note-it:completed_at=([^\s]+?)\s*-->` anywhere in the line.
/// - Requires exactly one non-whitespace timestamp candidate token within the comment.
/// - Comments with extra garbage (e.g. `<!-- note-it:completed_at=2026-08-27T11:32:00Z lixo -->`)
///   do not match the Note-it metadata comment regex and are left in the text unmodified.
/// - Removes ONLY the matched Note-it metadata comment and trailing whitespace.
/// - Leaves external/other HTML comments intact.
/// - Validates ISO 8601 with explicit offset or Z. Returns `None` if absent or invalid.
pub fn extract_completed_at(raw_text: &str) -> (Option<DateTime<Utc>>, String) {
    match find_completion_comment(raw_text) {
        Some(found) => {
            let mut cleaned = String::with_capacity(raw_text.len());
            cleaned.push_str(&raw_text[..found.start]);
            cleaned.push_str(&raw_text[found.end..]);
            (found.completed_at, cleaned.trim_end().to_string())
        }
        None => (None, raw_text.trim_end().to_string()),
    }
}

/// Removes Note-it's own completion comment, and nothing else.
///
/// Answers `None` when there is no such comment, so reopening a task that has
/// none rewrites nothing. Comments belonging to other tools, indentation, the
/// bullet, the nesting and the task's own text are all left exactly as they
/// were — the only thing this is allowed to take out of a line is the marker
/// Note-it itself put there.
pub fn remove_completion_comment(raw_text: &str) -> Option<String> {
    let found = find_completion_comment(raw_text)?;
    let mut cleaned = String::with_capacity(raw_text.len());
    cleaned.push_str(&raw_text[..found.start]);
    cleaned.push_str(&raw_text[found.end..]);
    Some(cleaned.trim_end().to_string())
}

/// Where Note-it's completion comment sits in a line, and what it says.
struct CompletionComment {
    start: usize,
    end: usize,
    completed_at: Option<DateTime<Utc>>,
}

/// The one scanner for the completion comment.
///
/// Reading a task and rewriting one have to agree exactly on what counts as
/// Note-it's marker, so there is a single implementation and both go through
/// it. A comment with anything other than one timestamp token in it is not
/// Note-it's, is not found here, and therefore is never removed.
fn find_completion_comment(raw_text: &str) -> Option<CompletionComment> {
    let mut search_from = 0;

    while let Some(rel_start) = raw_text[search_from..].find("<!--") {
        let comment_start = search_from + rel_start;
        let rest = &raw_text[comment_start..];
        let end_rel = rest.find("-->")?;
        let comment_end = comment_start + end_rel + 3;
        let inside = &raw_text[comment_start + 4..comment_end - 3];
        let trimmed_inside = inside.trim_start();

        if let Some(after_prefix) = trimmed_inside.strip_prefix("note-it:completed_at=") {
            let mut tokens = after_prefix.split_whitespace();
            let first = tokens.next();
            let second = tokens.next();

            if let (Some(candidate), None) = (first, second) {
                let completed_at = if is_valid_iso_8601_with_zone(candidate) {
                    DateTime::parse_from_rfc3339(candidate)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                } else {
                    None
                };
                return Some(CompletionComment {
                    start: comment_start,
                    end: comment_end,
                    completed_at,
                });
            }
        }

        search_from = comment_start + 4;
    }

    None
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

/// One task line, exactly as the shared scanner found it.
///
/// Everything that has an opinion about tasks reads them through this: the
/// listing projection, the reference that names one, and the rewrite that
/// completes or reopens one. There is deliberately one scanner, so a fenced
/// code block that is invisible to the listing is equally invisible to a
/// write — a fake task inside a fence can never be mutated because nothing
/// ever sees it as a task.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScannedTask<'a> {
    /// Zero-based index into `content.lines()`.
    line_index: usize,
    depth: usize,
    checked: bool,
    /// Everything after the `- [ ] ` marker, with trailing whitespace kept.
    raw_text: &'a str,
}

/// Every real task in a body, in document order.
///
/// Front matter never reaches here — callers pass the note's body — and lines
/// inside a fenced code block are skipped, so `- [ ] ` written as an example
/// is text and stays text.
fn scan_tasks(content: &str) -> Vec<ScannedTask<'_>> {
    let mut found = Vec::new();
    let mut code_fence: Option<(char, usize)> = None;

    for (line_index, line) in content.lines().enumerate() {
        if let Some(new_fence) = check_code_fence(line, code_fence) {
            code_fence = new_fence;
            continue;
        }
        if code_fence.is_some() {
            continue;
        }

        let depth = calculate_indent_depth(line);
        let trimmed = line.trim_start();

        let (checked, raw_text) = if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
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

        found.push(ScannedTask {
            line_index,
            depth,
            checked,
            raw_text,
        });
    }

    found
}

/// An optimistic reference to one task in one note.
///
/// **This is a snapshot token, not an identity.** Nothing about it is stored,
/// nothing carries it inside the Markdown, and it is not meant to survive the
/// note changing. It exists to answer one question safely: *is the task I am
/// about to write still the task I was shown?*
///
/// Phase 4.0D deliberately gave tasks no persistent identifier, and this does
/// not smuggle one in. A reference is recomputed from the note as it is at the
/// moment of the write and compared with the one the caller quoted. If the
/// task moved in the tree, was reworded, or was completed by someone else in
/// between, the reference stops matching and the write is refused. Listing the
/// tasks again is then the correct next step — and it is a far better outcome
/// than the alternative, which is quietly ticking off a different task.
///
/// It is derived from, in this order and with each part length-prefixed so two
/// different tasks can never be spelled the same way:
///
/// 1. the note's identifier;
/// 2. how deeply the task is nested;
/// 3. whether it is done;
/// 4. the exact text of the line after the checkbox, completion comment
///    included;
/// 5. how many earlier tasks in the same note are identical in all of the
///    above — which is what tells two literally identical tasks apart.
///
/// The line number is deliberately *not* part of it: inserting a paragraph
/// somewhere else in the note would otherwise invalidate every reference below
/// it, for a note whose tasks did not change at all.
///
/// The digest is [`crate::hashing`] — deterministic, documented, and never
/// seeded — shown as its first eight hexadecimal characters. Eight characters
/// can collide, so a reference matching more than one task in a note is
/// reported as ambiguous and refused; it is never resolved by choosing one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskRef(String);

/// The number of characters a task reference is written with.
pub const TASK_REF_LENGTH: usize = 8;

/// Why a string is not a task reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRefError(String);

impl fmt::Display for TaskRefError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TaskRefError {}

impl TaskRef {
    /// Reads a reference a person or an agent typed.
    ///
    /// Case-insensitive on input and lowercase from here on, so `A71BC920`
    /// and `a71bc920` are the same reference. Anything that is not exactly
    /// eight hexadecimal characters is refused as invalid usage rather than
    /// tried against the note.
    pub fn parse(raw: &str) -> Result<Self, TaskRefError> {
        let trimmed = raw.trim();
        if trimmed.chars().count() != TASK_REF_LENGTH
            || !trimmed.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(TaskRefError(format!(
                "`{trimmed}` is not a task reference; it is {TASK_REF_LENGTH} hexadecimal characters"
            )));
        }
        Ok(Self(trimmed.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn task_ref_for(note_id: Uuid, task: &ScannedTask<'_>, occurrence: usize) -> TaskRef {
    let note = note_id.as_simple().to_string();
    let depth = task.depth.to_string();
    let checked = if task.checked { "1" } else { "0" };
    let occurrence = occurrence.to_string();
    let digest = fnv1a_64_of_parts(&[
        note.as_bytes(),
        depth.as_bytes(),
        checked.as_bytes(),
        task.raw_text.as_bytes(),
        occurrence.as_bytes(),
    ]);
    TaskRef(format!("{:08x}", (digest >> 32) as u32))
}

/// The reference for every task in a body, in document order.
fn task_refs(note_id: Uuid, content: &str) -> Vec<(usize, TaskRef)> {
    let scanned = scan_tasks(content);
    let mut seen: Vec<(usize, bool, &str)> = Vec::new();
    let mut refs = Vec::with_capacity(scanned.len());

    for task in &scanned {
        let key = (task.depth, task.checked, task.raw_text);
        let occurrence = seen.iter().filter(|entry| **entry == key).count();
        seen.push(key);
        refs.push((task.line_index, task_ref_for(note_id, task, occurrence)));
    }

    refs
}

/// Why a task reference did not name exactly one task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskResolution {
    /// The note changed: nothing in it answers to this reference any more.
    Stale,
    /// More than one task answers to it. Never resolved by guessing.
    Ambiguous,
}

/// Finds the one task a reference names, or refuses.
///
/// Recomputed against the content handed in, which is the content that is
/// about to be written — not the content the reference was produced from. That
/// is the whole guarantee: the reference is checked against reality at the
/// moment of the write.
pub fn resolve_task_ref(
    note_id: Uuid,
    content: &str,
    wanted: &TaskRef,
) -> Result<usize, TaskResolution> {
    let matches: Vec<usize> = task_refs(note_id, content)
        .into_iter()
        .filter(|(_, candidate)| candidate == wanted)
        .map(|(line_index, _)| line_index)
        .collect();

    match matches.len() {
        0 => Err(TaskResolution::Stale),
        1 => Ok(matches[0]),
        _ => Err(TaskResolution::Ambiguous),
    }
}

/// Rewrites one task line as done or not done.
///
/// Answers `None` when the line already says exactly that, so completing an
/// already completed task writes nothing and moves no timestamp.
///
/// Everything that is not the checkbox and Note-it's own completion comment
/// survives untouched: the indentation, the bullet character, the nesting, the
/// text, and any HTML comment belonging to another tool.
pub fn rewrite_task_line(
    content: &str,
    line_index: usize,
    complete: bool,
    completed_at: &str,
) -> Option<String> {
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let line = lines.get(line_index)?;

    let indent_len = line.len() - line.trim_start().len();
    let (indent, trimmed) = line.split_at(indent_len);
    let bullet = trimmed.chars().next()?;
    if bullet != '-' && bullet != '*' {
        return None;
    }
    let after_marker = trimmed
        .strip_prefix(&format!("{bullet} [ ] "))
        .map(|rest| (false, rest))
        .or_else(|| {
            trimmed
                .strip_prefix(&format!("{bullet} [x] "))
                .map(|rest| (true, rest))
        })
        .or_else(|| {
            trimmed
                .strip_prefix(&format!("{bullet} [X] "))
                .map(|rest| (true, rest))
        });
    let (checked, text) = after_marker?;

    if checked == complete {
        return None;
    }

    let rewritten = if complete {
        // A completion Note-it made records when it made it. This is a real
        // act of completing something, so the instant is legitimately created
        // here rather than invented for a note that was already ticked.
        let body = text.trim_end();
        format!("{indent}{bullet} [x] {body} <!-- note-it:completed_at={completed_at} -->")
    } else {
        let body = remove_completion_comment(text).unwrap_or_else(|| text.trim_end().to_string());
        format!("{indent}{bullet} [ ] {body}")
    };

    lines[line_index] = rewritten;
    Some(lines.join("\n"))
}

/// Parses task lines from Markdown body content.
pub fn parse_tasks(note_id: Uuid, note_label: &str, content: &str) -> Vec<TaskEntry> {
    let scanned = scan_tasks(content);
    let refs = task_refs(note_id, content);

    scanned
        .into_iter()
        .zip(refs)
        .map(|(task, (_, task_ref))| {
            let (completed_at, clean_text) = extract_completed_at(task.raw_text);
            TaskEntry {
                note_id,
                note_label: note_label.to_string(),
                text: clean_text,
                checked: task.checked,
                completed_at: if task.checked { completed_at } else { None },
                depth: task.depth,
                line_number: task.line_index + 1,
                task_ref,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_01_completion_comment_alone() {
        let raw = "Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->";
        let (dt, text) = extract_completed_at(raw);
        assert!(dt.is_some());
        assert_eq!(text, "Comprar material");
    }

    #[test]
    fn test_case_02_other_html_comment_before() {
        let raw = "Comprar material <!-- observação externa --> <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->";
        let (dt, text) = extract_completed_at(raw);
        assert!(dt.is_some());
        assert_eq!(text, "Comprar material <!-- observação externa -->");
    }

    #[test]
    fn test_case_03_other_html_comment_after() {
        let raw = "Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 --> <!-- observação externa -->";
        let (dt, text) = extract_completed_at(raw);
        assert!(dt.is_some());
        assert_eq!(text, "Comprar material  <!-- observação externa -->");
    }

    #[test]
    fn test_case_04_invalid_comment_yields_none_and_strips_comment() {
        let raw = "Comprar material <!-- note-it:completed_at=data-invalida -->";
        let (dt, text) = extract_completed_at(raw);
        assert_eq!(dt, None);
        assert_eq!(text, "Comprar material");
    }

    #[test]
    fn test_case_05_absent_comment_yields_none_and_preserves_text() {
        let raw = "Comprar material simples";
        let (dt, text) = extract_completed_at(raw);
        assert_eq!(dt, None);
        assert_eq!(text, "Comprar material simples");
    }

    #[test]
    fn test_case_06_unchecked_task_with_completion_metadata_drops_timestamp() {
        let id = Uuid::new_v4();
        let markdown =
            "- [ ] Tarefa não concluída <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->";
        let tasks = parse_tasks(id, "Nota", markdown);
        assert_eq!(tasks.len(), 1);
        assert!(!tasks[0].checked);
        assert_eq!(tasks[0].completed_at, None);
        assert_eq!(tasks[0].text, "Tarefa não concluída");
    }

    #[test]
    fn test_case_07_similar_non_note_it_comment_preserved() {
        let raw = "Comprar material <!-- other-app:completed_at=2026-08-27T11:32:00-03:00 -->";
        let (dt, text) = extract_completed_at(raw);
        assert_eq!(dt, None);
        assert_eq!(
            text,
            "Comprar material <!-- other-app:completed_at=2026-08-27T11:32:00-03:00 -->"
        );
    }

    #[test]
    fn test_case_08_external_comments_are_not_removed() {
        let raw = "Comprar material <!-- importante --> e mais texto <!-- id=123 -->";
        let (dt, text) = extract_completed_at(raw);
        assert_eq!(dt, None);
        assert_eq!(
            text,
            "Comprar material <!-- importante --> e mais texto <!-- id=123 -->"
        );
    }

    #[test]
    fn test_case_09_valid_comment_with_extra_whitespace_parsed() {
        let raw = "Comprar material <!--   note-it:completed_at=2026-08-27T11:32:00Z   -->";
        let (dt, text) = extract_completed_at(raw);
        assert!(dt.is_some());
        assert_eq!(text, "Comprar material");
    }

    #[test]
    fn test_case_10_timestamp_with_extra_garbage_rejected_and_not_stripped() {
        let raw = "Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00Z lixo -->";
        let (dt, text) = extract_completed_at(raw);
        assert_eq!(dt, None);
        assert_eq!(
            text,
            "Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00Z lixo -->"
        );
    }

    #[test]
    fn test_case_11_empty_timestamp_comment_rejected_and_not_stripped() {
        let raw = "Comprar material <!-- note-it:completed_at= -->";
        let (dt, text) = extract_completed_at(raw);
        assert_eq!(dt, None);
        assert_eq!(text, "Comprar material <!-- note-it:completed_at= -->");
    }

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
}
