//! A note body while it is being edited, in memory and nowhere else.
//!
//! This is the whole of the editor's model: lines of text, one cursor, an
//! optional selection anchor and a bounded history. It knows nothing about
//! Markdown, the store, revisions or the terminal — which is what lets the
//! application decide *when* a draft becomes a write and lets these rules be
//! tested without either.
//!
//! ## Positions are characters
//!
//! A [`Position`] counts characters, not bytes and not display columns. Bytes
//! would let a cursor land inside a `ç`; display columns would make the model
//! depend on a font. Characters keep every operation total: no index can split
//! a UTF-8 sequence, so no input can panic here.
//!
//! A grapheme cluster spanning several characters — a combining accent, a
//! flag, a family emoji — is therefore edited one scalar at a time. It is
//! never corrupted, cut in half or reordered; it simply takes more than one
//! `Backspace` to remove. That is the v1 limitation, and it is the reason this
//! module needs no segmentation table and no dependency.
//!
//! ## History is bounded on purpose
//!
//! Undo keeps whole snapshots, which is the small correct thing rather than the
//! clever one, and [`UNDO_LIMIT`] is what stops a long session from growing
//! without end. Consecutive insertions collapse into one step, and so do
//! consecutive deletions, so undo walks back the way a person typed rather than
//! one character at a time.

use std::collections::VecDeque;

/// How many undo steps a draft keeps. The oldest is dropped past this.
pub const UNDO_LIMIT: usize = 200;

/// A place in the text: which line, and how many characters into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// A cursor movement the editor can be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    LineEnd,
    PageUp(usize),
    PageDown(usize),
}

/// What kind of edit produced a step, so a run of the same kind is one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grouping {
    Insert,
    Delete,
    /// Always its own step: a paste, a newline, a selection replacement.
    Discrete,
}

#[derive(Debug, Clone)]
struct Snapshot {
    lines: Vec<String>,
    cursor: Position,
    anchor: Option<Position>,
}

/// The note body being edited.
#[derive(Debug, Clone)]
pub struct Draft {
    lines: Vec<String>,
    cursor: Position,
    anchor: Option<Position>,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    grouping: Option<Grouping>,
}

impl Draft {
    /// Opens `content` for editing, cursor at the start.
    ///
    /// Splitting on `\n` — rather than `lines()` — is what makes [`Draft::text`]
    /// give back exactly what came in: a body ending in a newline keeps its
    /// final empty line, and an empty body is one empty line rather than none.
    pub fn new(content: &str) -> Self {
        Self {
            lines: content.split('\n').map(str::to_owned).collect(),
            cursor: Position::default(),
            anchor: None,
            undo: VecDeque::new(),
            redo: Vec::new(),
            grouping: None,
        }
    }

    /// The body as it would be stored right now.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> Position {
        self.cursor
    }

    /// The selection as an ordered pair, or `None` when nothing is selected.
    pub fn selection(&self) -> Option<(Position, Position)> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some(if anchor < self.cursor {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        })
    }

    /// How many undo steps are currently held. Never above [`UNDO_LIMIT`].
    pub fn history_depth(&self) -> usize {
        self.undo.len()
    }

    // -- editing ------------------------------------------------------------

    pub fn insert_char(&mut self, character: char) {
        if self.selection().is_some() {
            self.checkpoint(Grouping::Discrete);
            self.remove_selection();
        } else {
            self.checkpoint(Grouping::Insert);
        }
        let byte = self.byte_of(self.cursor);
        self.lines[self.cursor.line].insert(byte, character);
        self.cursor.column += 1;
        self.anchor = None;
    }

    /// Inserts text that may contain line breaks — a paste is one undo step.
    pub fn insert_str(&mut self, text: &str) {
        self.checkpoint(Grouping::Discrete);
        self.remove_selection();
        for (index, piece) in text.split('\n').enumerate() {
            if index > 0 {
                self.split_line();
            }
            let byte = self.byte_of(self.cursor);
            self.lines[self.cursor.line].insert_str(byte, piece);
            self.cursor.column += piece.chars().count();
        }
        self.anchor = None;
    }

    pub fn newline(&mut self) {
        self.checkpoint(Grouping::Discrete);
        self.remove_selection();
        self.split_line();
        self.anchor = None;
    }

    /// Removes the selection, or the character before the cursor, or joins
    /// this line onto the previous one.
    pub fn backspace(&mut self) {
        if self.selection().is_some() {
            self.checkpoint(Grouping::Discrete);
            self.remove_selection();
            self.anchor = None;
            return;
        }
        if self.cursor.column == 0 && self.cursor.line == 0 {
            return;
        }
        self.checkpoint(Grouping::Delete);
        if self.cursor.column > 0 {
            let byte = self.byte_of(Position {
                line: self.cursor.line,
                column: self.cursor.column - 1,
            });
            self.lines[self.cursor.line].remove(byte);
            self.cursor.column -= 1;
        } else {
            let removed = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            self.cursor.column = self.line_length(self.cursor.line);
            self.lines[self.cursor.line].push_str(&removed);
        }
        self.anchor = None;
    }

    /// Removes the selection, or the character under the cursor, or joins the
    /// next line onto this one.
    pub fn delete(&mut self) {
        if self.selection().is_some() {
            self.checkpoint(Grouping::Discrete);
            self.remove_selection();
            self.anchor = None;
            return;
        }
        let length = self.line_length(self.cursor.line);
        if self.cursor.column >= length && self.cursor.line + 1 >= self.lines.len() {
            return;
        }
        self.checkpoint(Grouping::Delete);
        if self.cursor.column < length {
            let byte = self.byte_of(self.cursor);
            self.lines[self.cursor.line].remove(byte);
        } else {
            let next = self.lines.remove(self.cursor.line + 1);
            self.lines[self.cursor.line].push_str(&next);
        }
        self.anchor = None;
    }

    // -- navigation and selection -------------------------------------------

    pub fn move_cursor(&mut self, motion: Motion, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        // A movement always ends the run of edits before it, so what comes
        // next is a step of its own.
        self.grouping = None;

        match motion {
            Motion::Left => {
                if self.cursor.column > 0 {
                    self.cursor.column -= 1;
                } else if self.cursor.line > 0 {
                    self.cursor.line -= 1;
                    self.cursor.column = self.line_length(self.cursor.line);
                }
            }
            Motion::Right => {
                if self.cursor.column < self.line_length(self.cursor.line) {
                    self.cursor.column += 1;
                } else if self.cursor.line + 1 < self.lines.len() {
                    self.cursor.line += 1;
                    self.cursor.column = 0;
                }
            }
            Motion::Up => self.step_lines(-1),
            Motion::Down => self.step_lines(1),
            Motion::LineStart => self.cursor.column = 0,
            Motion::LineEnd => self.cursor.column = self.line_length(self.cursor.line),
            Motion::PageUp(page) => self.step_lines(-(page.max(1) as isize)),
            Motion::PageDown(page) => self.step_lines(page.max(1) as isize),
        }
    }

    /// Puts the cursor back where it was, clamped to the text that is there
    /// now. Used when a draft is reseated on a note the store just returned.
    pub fn restore_cursor(&mut self, cursor: Position) {
        self.cursor = cursor;
        self.anchor = None;
        self.clamp();
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(Position::default());
        let line = self.lines.len() - 1;
        self.cursor = Position {
            line,
            column: self.line_length(line),
        };
        self.grouping = None;
    }

    // -- history ------------------------------------------------------------

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push_back(self.snapshot());
        self.restore(next);
        true
    }

    // -- internals ----------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.lines = snapshot.lines;
        self.cursor = snapshot.cursor;
        self.anchor = snapshot.anchor;
        self.grouping = None;
        self.clamp();
    }

    /// Records the state an undo would return to, unless this edit continues
    /// the run the last one started.
    fn checkpoint(&mut self, grouping: Grouping) {
        self.redo.clear();
        if grouping != Grouping::Discrete && self.grouping == Some(grouping) {
            return;
        }
        self.undo.push_back(self.snapshot());
        while self.undo.len() > UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.grouping = Some(grouping);
    }

    fn split_line(&mut self) {
        let byte = self.byte_of(self.cursor);
        let tail = self.lines[self.cursor.line].split_off(byte);
        self.lines.insert(self.cursor.line + 1, tail);
        self.cursor.line += 1;
        self.cursor.column = 0;
    }

    fn remove_selection(&mut self) {
        let Some((start, end)) = self.selection() else {
            return;
        };
        let start_byte = self.byte_of(start);
        let end_byte = self.byte_of(end);
        if start.line == end.line {
            self.lines[start.line].replace_range(start_byte..end_byte, "");
        } else {
            let tail = self.lines[end.line][end_byte..].to_owned();
            self.lines[start.line].truncate(start_byte);
            self.lines[start.line].push_str(&tail);
            self.lines.drain(start.line + 1..=end.line);
        }
        self.cursor = start;
        self.anchor = None;
    }

    fn step_lines(&mut self, delta: isize) {
        let target = self
            .cursor
            .line
            .saturating_add_signed(delta)
            .min(self.lines.len() - 1);
        self.cursor.line = target;
        self.cursor.column = self.cursor.column.min(self.line_length(target));
    }

    fn line_length(&self, line: usize) -> usize {
        self.lines[line].chars().count()
    }

    fn byte_of(&self, position: Position) -> usize {
        let line = &self.lines[position.line];
        line.char_indices()
            .nth(position.column)
            .map_or(line.len(), |(byte, _)| byte)
    }

    fn clamp(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor.line = self.cursor.line.min(self.lines.len() - 1);
        self.cursor.column = self.cursor.column.min(self.line_length(self.cursor.line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Named so a local `draft` variable never shadows the constructor.
    fn open(content: &str) -> Draft {
        Draft::new(content)
    }

    #[test]
    fn a_body_survives_the_round_trip_exactly() {
        for content in ["", "uma", "uma\nduas", "acaba em nova linha\n", "\n\n"] {
            assert_eq!(open(content).text(), content, "content {content:?}");
        }
    }

    #[test]
    fn typing_lands_where_the_cursor_is_and_never_splits_a_character() {
        let mut draft = open("ação");
        draft.move_cursor(Motion::Right, false);
        draft.insert_char('X');
        assert_eq!(draft.text(), "aXção");
        assert_eq!(draft.cursor(), Position { line: 0, column: 2 });

        // Position 1 is *after* `a`, not inside the two bytes of `ç`.
        let mut draft = open("çé");
        draft.move_cursor(Motion::Right, false);
        draft.insert_char('!');
        assert_eq!(draft.text(), "ç!é");
    }

    #[test]
    fn backspace_joins_lines_and_stops_at_the_start_of_the_document() {
        let mut draft = open("uma\nduas");
        draft.move_cursor(Motion::Down, false);
        draft.move_cursor(Motion::LineStart, false);
        draft.backspace();
        assert_eq!(draft.text(), "umaduas");

        draft.move_cursor(Motion::LineStart, false);
        draft.backspace();
        assert_eq!(
            draft.text(),
            "umaduas",
            "não há o que apagar antes do início"
        );
    }

    #[test]
    fn delete_removes_forwards_and_stops_at_the_end_of_the_document() {
        let mut draft = open("uma\nduas");
        draft.move_cursor(Motion::LineEnd, false);
        draft.delete();
        assert_eq!(draft.text(), "umaduas");

        draft.move_cursor(Motion::LineEnd, false);
        draft.delete();
        assert_eq!(draft.text(), "umaduas");
    }

    #[test]
    fn a_paste_is_one_step_and_keeps_its_line_breaks() {
        let mut draft = open("fim");
        draft.insert_str("uma\nduas\n");
        assert_eq!(draft.text(), "uma\nduas\nfim");
        assert_eq!(draft.cursor(), Position { line: 2, column: 0 });
        draft.undo();
        assert_eq!(draft.text(), "fim");
    }

    #[test]
    fn a_selection_is_ordered_however_it_was_made() {
        let mut draft = open("abcdef");
        draft.move_cursor(Motion::LineEnd, false);
        draft.move_cursor(Motion::Left, true);
        draft.move_cursor(Motion::Left, true);
        assert_eq!(
            draft.selection(),
            Some((
                Position { line: 0, column: 4 },
                Position { line: 0, column: 6 }
            ))
        );
        draft.backspace();
        assert_eq!(draft.text(), "abcd");
        assert_eq!(draft.selection(), None);
    }

    #[test]
    fn selecting_across_lines_and_typing_replaces_the_whole_range() {
        let mut draft = open("uma\nduas\ntres");
        draft.select_all();
        draft.insert_char('x');
        assert_eq!(draft.text(), "x");
    }

    #[test]
    fn a_run_of_typing_undoes_as_one_step_and_redoes_back() {
        let mut draft = open("base");
        draft.move_cursor(Motion::LineEnd, false);
        for character in " novo".chars() {
            draft.insert_char(character);
        }
        assert_eq!(draft.text(), "base novo");
        assert_eq!(draft.history_depth(), 1, "digitar corrido é um passo");

        assert!(draft.undo());
        assert_eq!(draft.text(), "base");
        assert!(draft.redo());
        assert_eq!(draft.text(), "base novo");

        assert!(!draft.undo() || draft.text() == "base");
    }

    #[test]
    fn an_edit_after_an_undo_drops_what_was_redoable() {
        let mut draft = open("base");
        draft.move_cursor(Motion::LineEnd, false);
        draft.insert_char('!');
        draft.undo();
        draft.insert_char('?');
        assert!(!draft.redo(), "o futuro descartado não volta");
        assert_eq!(draft.text(), "base?");
    }

    #[test]
    fn history_never_grows_past_its_explicit_limit() {
        let mut draft = open("");
        for index in 0..(UNDO_LIMIT * 2) {
            // Alternating kinds so nothing coalesces into one step.
            draft.insert_char('a');
            draft.move_cursor(Motion::Left, false);
            draft.delete();
            draft.move_cursor(Motion::LineEnd, false);
            assert!(
                draft.history_depth() <= UNDO_LIMIT,
                "passo {index} ultrapassou o limite"
            );
        }
        assert_eq!(draft.history_depth(), UNDO_LIMIT);
    }

    #[test]
    fn movement_is_total_on_an_empty_document() {
        let mut draft = open("");
        for motion in [
            Motion::Left,
            Motion::Right,
            Motion::Up,
            Motion::Down,
            Motion::LineStart,
            Motion::LineEnd,
            Motion::PageUp(10),
            Motion::PageDown(10),
        ] {
            draft.move_cursor(motion, false);
            assert_eq!(draft.cursor(), Position::default());
        }
        draft.backspace();
        draft.delete();
        assert_eq!(draft.text(), "");
    }

    #[test]
    fn a_cursor_past_a_shorter_line_is_clamped_not_lost() {
        let mut draft = open("uma linha longa\ncurta");
        draft.move_cursor(Motion::LineEnd, false);
        draft.move_cursor(Motion::Down, false);
        assert_eq!(draft.cursor(), Position { line: 1, column: 5 });
    }

    #[test]
    fn a_combined_grapheme_is_never_corrupted_only_edited_by_scalar() {
        let mut draft = open("e\u{0301}");
        draft.move_cursor(Motion::LineEnd, false);
        draft.backspace();
        assert_eq!(
            draft.text(),
            "e",
            "o acento sai antes da letra, sem quebrar bytes"
        );

        let mut draft = open("👨‍👩‍👧‍👦");
        draft.move_cursor(Motion::LineEnd, false);
        let scalars = "👨‍👩‍👧‍👦".chars().count();
        for _ in 0..scalars {
            draft.backspace();
        }
        assert_eq!(draft.text(), "");
    }

    #[test]
    fn a_very_long_line_and_a_very_tall_document_stay_addressable() {
        let mut draft = open(&"á".repeat(50_000));
        draft.move_cursor(Motion::LineEnd, false);
        draft.insert_char('!');
        assert!(draft.text().ends_with("á!"));

        let tall: String = (0..5_000).map(|n| format!("linha {n}\n")).collect();
        let mut draft = open(&tall);
        draft.move_cursor(Motion::PageDown(10_000), false);
        assert_eq!(draft.cursor().line, draft.lines().len() - 1);
    }
}
