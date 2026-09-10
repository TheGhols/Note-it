//! Application state and event loop for Note-it TUI.
//!
//! Provides interactive navigation across recent notes, pending tasks, and trash,
//! with quick search (/) using noteit-core in-process.

use crate::{
    document::LoadedDocument,
    draft::{Draft, Motion},
    editor,
    formatting::{self, Kind},
    terminal::TerminalGuard,
    ui,
};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use noteit_core::Uuid;
use noteit_core::{
    authority,
    filter::NoteFilter,
    model::NoteSummary,
    search::SearchResult,
    task::{TaskEntry, TaskStateFilter},
    trash::TrashEntry,
    write::{NoteDraft, NoteMutation, WriteError, WriteOperation},
    NoteItCore, StorePaths,
};
use ratatui::{backend::Backend, backend::CrosstermBackend, Terminal};
use std::cell::Cell;
use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Navigation panels in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePanel {
    #[default]
    RecentNotes,
    PendingTasks,
    Trash,
}

impl ActivePanel {
    pub fn next(self) -> Self {
        match self {
            Self::RecentNotes => Self::PendingTasks,
            Self::PendingTasks => Self::Trash,
            Self::Trash => Self::RecentNotes,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::RecentNotes => Self::Trash,
            Self::PendingTasks => Self::RecentNotes,
            Self::Trash => Self::PendingTasks,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::RecentNotes => "Notas Recentes",
            Self::PendingTasks => "Tarefas Pendentes",
            Self::Trash => "Lixeira",
        }
    }
}

/// Active input/focus mode in the TUI.
///
/// `Reader` and `Editor` are the two halves of the right pane, and they are
/// never ambiguous: the reader renders the note, the editor holds its source
/// and every printable key is text. `Esc` steps outwards one level at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    List,
    Reader,
    Editor,
    Search,
}

/// A question the editor must have answered before it can go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPrompt {
    /// Leaving with text that is not in the store yet.
    Pending,
    /// The store moved under the draft; nothing was overwritten.
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMenu {
    Root,
    TextColor,
    Highlight,
}

#[derive(Debug, Clone, Copy)]
enum PendingMouseAction {
    Panel(ActivePanel),
    Row(usize),
    Search,
    TaskToggle(usize),
}

/// The interactive TUI application state.
pub struct App {
    pub core: NoteItCore,
    pub paths: StorePaths,
    pub panel: ActivePanel,
    pub focus: Focus,

    // Data lists
    pub recent_notes: Vec<NoteSummary>,
    pub recent_selected: usize,

    pub pending_tasks: Vec<TaskEntry>,
    pub tasks_selected: usize,

    pub trash_items: Vec<TrashEntry>,
    pub trash_selected: usize,

    // Search state
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub search_selected: usize,

    // Reader state
    pub current_note: Option<LoadedDocument>,
    /// Compatibility projection for navigation; mutations use the snapshot.
    pub current_note_id: Option<Uuid>,
    pub reader_scroll: usize,
    pub reader_cursor: usize,
    pub notice: String,
    pub discard_confirmation: Option<LoadedDocument>,
    editor_requested: bool,

    // Native editor state
    /// The note body being edited, present exactly while `focus` is `Editor`.
    pub draft: Option<Draft>,
    pub editor_prompt: Option<EditorPrompt>,
    pub format_menu: Option<FormatMenu>,
    pub format_selected: usize,
    pub active_text_color: Option<&'static str>,
    pub active_highlight: Option<&'static str>,
    pending_mouse_action: Option<PendingMouseAction>,
    /// Set when the open question was raised by somebody asking to leave, so
    /// answering it finishes the exit instead of returning to the reader.
    exit_requested: bool,
    /// Viewport bookkeeping the renderer owns: only drawing knows how tall and
    /// wide the pane turned out to be, and the cursor must stay inside it.
    pub(crate) editor_scroll: Cell<usize>,
    pub(crate) editor_column: Cell<usize>,
    pub(crate) editor_viewport: Cell<usize>,

    // Lifecycle
    pub should_quit: bool,
    pub term_flag: Arc<AtomicBool>,
}

impl App {
    /// Creates a new App instance using canonical XDG paths.
    pub fn new(term_flag: Arc<AtomicBool>) -> Self {
        let paths = NoteItCore::resolve_paths();
        Self::new_at(paths, term_flag)
    }

    /// Creates a new App instance using the given StorePaths in strictly read-only mode.
    pub fn new_at(paths: StorePaths, term_flag: Arc<AtomicBool>) -> Self {
        let core = NoteItCore::open_read_only_at(paths.clone());
        let mut app = Self {
            core,
            paths,
            panel: ActivePanel::RecentNotes,
            focus: Focus::List,
            recent_notes: Vec::new(),
            recent_selected: 0,
            pending_tasks: Vec::new(),
            tasks_selected: 0,
            trash_items: Vec::new(),
            trash_selected: 0,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            current_note: None,
            current_note_id: None,
            reader_scroll: 0,
            reader_cursor: 0,
            notice: String::new(),
            discard_confirmation: None,
            editor_requested: false,
            draft: None,
            editor_prompt: None,
            format_menu: None,
            format_selected: 0,
            active_text_color: None,
            active_highlight: None,
            pending_mouse_action: None,
            exit_requested: false,
            editor_scroll: Cell::new(0),
            editor_column: Cell::new(0),
            editor_viewport: Cell::new(1),
            should_quit: false,
            term_flag,
        };

        app.reload_all();

        // Automatically load the first recent note into the reader if available
        if let Some(first) = app.recent_notes.first() {
            app.load_note(first.id);
        }

        app
    }

    /// Reloads all data categories from noteit-core in strictly read-only mode.
    pub fn reload_all(&mut self) {
        // 1. Recent notes (ordered by updated_at / canonical recency)
        self.recent_notes = self
            .core
            .list_summaries(&NoteFilter::default(), None)
            .map(|batch| batch.items)
            .unwrap_or_default();
        if self.recent_selected >= self.recent_notes.len() {
            self.recent_selected = self.recent_notes.len().saturating_sub(1);
        }

        // 2. Pending tasks
        self.pending_tasks = self
            .core
            .list_tasks(TaskStateFilter::Pending, &NoteFilter::default(), None)
            .map(|batch| batch.items)
            .unwrap_or_default();
        if self.tasks_selected >= self.pending_tasks.len() {
            self.tasks_selected = self.pending_tasks.len().saturating_sub(1);
        }

        // 3. Trash items
        self.trash_items = self.core.list_trash();
        if self.trash_selected >= self.trash_items.len() {
            self.trash_selected = self.trash_items.len().saturating_sub(1);
        }
    }

    /// Loads a specific note by UUID into the reader pane.
    pub fn load_note(&mut self, id: Uuid) {
        if self
            .current_note
            .as_ref()
            .is_some_and(|note| note.id == id && !note.in_trash)
        {
            return;
        }
        match self.core.read_note(&id) {
            Ok(doc) => {
                self.current_note = LoadedDocument::new(doc, false).ok();
                self.current_note_id = Some(id);
                self.reader_scroll = 0;
                self.reader_cursor = 0;
            }
            Err(_) => {
                self.current_note = None;
                self.current_note_id = None;
            }
        }
    }

    /// Performs search using noteit-core's search engine.
    pub fn update_search(&mut self) {
        let trimmed = self.search_query.trim();
        if trimmed.is_empty() {
            self.search_results.clear();
            self.search_selected = 0;
            return;
        }

        match self.core.search_notes(trimmed) {
            Ok(batch) => {
                self.search_results = batch.items;
                self.search_selected = 0;
            }
            Err(_) => {
                self.search_results.clear();
                self.search_selected = 0;
            }
        }
    }

    /// Handles a keyboard event. Returns true if the event caused the app to quit.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return self.should_quit;
        }
        if self.editor_prompt.is_none() && self.discard_confirmation.is_none() {
            self.notice.clear();
        }
        // Ctrl+C is somebody asking to leave, and in raw mode it is a key, not
        // a signal — there is a screen to ask on and somebody there to answer.
        // So it goes through the same question every other user-initiated exit
        // goes through, and only leaves at once when there is nothing to lose.
        // An externally delivered SIGINT is a different thing entirely and
        // keeps its own path: nobody can be asked, so the draft is preserved.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.focus == Focus::Editor && self.pending_text().is_some() {
                self.exit_requested = true;
                self.editor_prompt = Some(EditorPrompt::Pending);
                return self.should_quit;
            }
            self.should_quit = true;
            return true;
        }

        if let Some(snapshot) = self.discard_confirmation.take() {
            match key.code {
                KeyCode::Char('y' | 'Y') => self.discard(snapshot),
                KeyCode::Char('n' | 'N') | KeyCode::Esc | KeyCode::Enter => {
                    self.notice = "Descarte cancelado".into();
                }
                _ => self.discard_confirmation = Some(snapshot),
            }
            return self.should_quit;
        }

        // Mode-specific handling
        match self.focus {
            Focus::Search => self.handle_key_search(key),
            Focus::Reader => self.handle_key_reader(key),
            Focus::Editor => self.handle_key_editor(key),
            Focus::List => self.handle_key_list(key),
        }

        // Selection changes load a canonical trash snapshot for preview and
        // subsequent restore. Restore itself never refreshes its precondition.
        // Editing is never a selection change, and a draft must never be
        // replaced by a preview under it.
        if self.panel == ActivePanel::Trash && !matches!(self.focus, Focus::Search | Focus::Editor)
        {
            self.load_selected_trash(false);
        }

        self.should_quit
    }

    fn handle_key_list(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('n') if self.panel == ActivePanel::RecentNotes => self.create_note(),
            KeyCode::Char('r') if self.panel == ActivePanel::Trash => self.restore_selected(),
            KeyCode::Char(' ') if self.panel == ActivePanel::PendingTasks => {
                self.toggle_selected_task()
            }
            // Exit shortcuts
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }

            // Quick search activation
            KeyCode::Char('/') => {
                self.focus = Focus::Search;
                self.search_query.clear();
                self.search_results.clear();
                self.search_selected = 0;
            }

            // Panel navigation shortcuts
            KeyCode::Tab => {
                self.panel = self.panel.next();
                self.on_panel_switched();
            }
            KeyCode::BackTab => {
                self.panel = self.panel.prev();
                self.on_panel_switched();
            }
            KeyCode::Char('1') => {
                self.panel = ActivePanel::RecentNotes;
                self.on_panel_switched();
            }
            KeyCode::Char('2') => {
                self.panel = ActivePanel::PendingTasks;
                self.on_panel_switched();
            }
            KeyCode::Char('3') => {
                self.panel = ActivePanel::Trash;
                self.on_panel_switched();
            }

            // List selection navigation
            KeyCode::Up | KeyCode::Char('k') => match self.panel {
                ActivePanel::RecentNotes => {
                    if self.recent_selected > 0 {
                        self.recent_selected -= 1;
                        if let Some(note) = self.recent_notes.get(self.recent_selected) {
                            self.load_note(note.id);
                        }
                    }
                }
                ActivePanel::PendingTasks => {
                    if self.tasks_selected > 0 {
                        self.tasks_selected -= 1;
                        self.load_selected_task_note();
                    }
                }
                ActivePanel::Trash => {
                    if self.trash_selected > 0 {
                        self.trash_selected -= 1;
                    }
                }
            },
            KeyCode::Down | KeyCode::Char('j') => match self.panel {
                ActivePanel::RecentNotes => {
                    if !self.recent_notes.is_empty()
                        && self.recent_selected + 1 < self.recent_notes.len()
                    {
                        self.recent_selected += 1;
                        if let Some(note) = self.recent_notes.get(self.recent_selected) {
                            self.load_note(note.id);
                        }
                    }
                }
                ActivePanel::PendingTasks => {
                    if !self.pending_tasks.is_empty()
                        && self.tasks_selected + 1 < self.pending_tasks.len()
                    {
                        self.tasks_selected += 1;
                        self.load_selected_task_note();
                    }
                }
                ActivePanel::Trash => {
                    if !self.trash_items.is_empty()
                        && self.trash_selected + 1 < self.trash_items.len()
                    {
                        self.trash_selected += 1;
                    }
                }
            },

            // Jump to top/bottom
            KeyCode::Home | KeyCode::Char('g') => match self.panel {
                ActivePanel::RecentNotes => {
                    self.recent_selected = 0;
                    if let Some(note) = self.recent_notes.first() {
                        self.load_note(note.id);
                    }
                }
                ActivePanel::PendingTasks => {
                    self.tasks_selected = 0;
                }
                ActivePanel::Trash => {
                    self.trash_selected = 0;
                }
            },
            KeyCode::End | KeyCode::Char('G') => match self.panel {
                ActivePanel::RecentNotes => {
                    if !self.recent_notes.is_empty() {
                        self.recent_selected = self.recent_notes.len() - 1;
                        if let Some(note) = self.recent_notes.get(self.recent_selected) {
                            self.load_note(note.id);
                        }
                    }
                }
                ActivePanel::PendingTasks => {
                    if !self.pending_tasks.is_empty() {
                        self.tasks_selected = self.pending_tasks.len() - 1;
                    }
                }
                ActivePanel::Trash => {
                    if !self.trash_items.is_empty() {
                        self.trash_selected = self.trash_items.len() - 1;
                    }
                }
            },

            // Opening a note is opening it to be worked on: the right pane
            // starts editing, with no second key. Selecting one with the
            // arrows keeps showing the rendered reader.
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => match self.panel {
                ActivePanel::RecentNotes => {
                    if let Some(note) = self.recent_notes.get(self.recent_selected) {
                        self.load_note(note.id);
                        self.start_editing();
                    }
                }
                ActivePanel::PendingTasks => {
                    if let Some(task) = self.pending_tasks.get(self.tasks_selected) {
                        self.load_note(task.note_id);
                        self.start_editing();
                    }
                }
                ActivePanel::Trash => {
                    // A note in the trash is read, never edited.
                    self.focus = Focus::Reader;
                }
            },

            _ => {}
        }
    }

    fn handle_key_reader(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(' ') if self.panel != ActivePanel::Trash => self.toggle_task(),
            KeyCode::Char('d') if self.panel != ActivePanel::Trash => {
                self.discard_confirmation = self.current_note.clone().filter(|note| !note.in_trash);
            }
            KeyCode::Char('e') if self.panel != ActivePanel::Trash => self.editor_requested = true,
            // Back into the editor without a detour through the list.
            KeyCode::Enter | KeyCode::Char('i') if self.panel != ActivePanel::Trash => {
                self.start_editing()
            }
            KeyCode::Char('r') if self.panel == ActivePanel::Trash => self.restore_selected(),
            // Return to list focus
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => {
                self.focus = Focus::List;
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }

            // Quick search activation
            KeyCode::Char('/') => {
                self.focus = Focus::Search;
                self.search_query.clear();
                self.search_results.clear();
                self.search_selected = 0;
            }

            // Scroll reader pane
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_cursor(-1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_cursor(1);
            }
            KeyCode::PageUp => {
                self.move_cursor(-10);
            }
            KeyCode::PageDown => {
                self.move_cursor(10);
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.reader_scroll = 0;
                self.reader_cursor = 0;
            }

            // Switch panels even while in reader
            KeyCode::Tab => {
                self.panel = self.panel.next();
                self.focus = Focus::List;
                self.on_panel_switched();
            }
            KeyCode::BackTab => {
                self.panel = self.panel.prev();
                self.focus = Focus::List;
                self.on_panel_switched();
            }
            KeyCode::Char('1') => {
                self.panel = ActivePanel::RecentNotes;
                self.focus = Focus::List;
                self.on_panel_switched();
            }
            KeyCode::Char('2') => {
                self.panel = ActivePanel::PendingTasks;
                self.focus = Focus::List;
                self.on_panel_switched();
            }
            KeyCode::Char('3') => {
                self.panel = ActivePanel::Trash;
                self.focus = Focus::List;
                self.on_panel_switched();
            }

            _ => {}
        }
    }

    /// Every printable key is text here.
    ///
    /// That is not a detail: `Tab`, `/`, `d` and `q` navigate elsewhere in the
    /// application, and inside the editor they are characters. It leaves the
    /// editor exactly one door — `Esc` — which is the door the pending-changes
    /// question is asked at, so "switching note, panel, search or discarding
    /// with unsaved text" cannot happen behind the question's back.
    fn handle_key_editor(&mut self, key: KeyEvent) {
        if self.format_menu.is_some() {
            self.handle_format_key(key);
            return;
        }
        if let Some(prompt) = self.editor_prompt.take() {
            self.answer_prompt(prompt, key);
            return;
        }

        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let extend = key.modifiers.contains(KeyModifiers::SHIFT);
        let page = self.editor_viewport.get().max(1);

        if alt && key.code == KeyCode::Char('f') {
            self.open_format_menu();
            return;
        }
        if control || alt {
            match key.code {
                KeyCode::Char('s') => self.save_draft(),
                KeyCode::Char('z') if extend => self.edit(|draft| {
                    draft.redo();
                }),
                KeyCode::Char('z' | 'Z') => self.edit(|draft| {
                    draft.undo();
                }),
                KeyCode::Char('y') => self.edit(|draft| {
                    draft.redo();
                }),
                KeyCode::Char('a') => self.edit(Draft::select_all),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc => self.leave_editor(),
            KeyCode::Char(character) => {
                let mut text = [0; 4];
                self.insert_with_active_style(character.encode_utf8(&mut text));
            }
            // A note body is Markdown, where indentation is spaces. Storing a
            // tab whose width nothing agrees on would be storing a surprise.
            KeyCode::Tab => self.edit(|draft| draft.insert_str("    ")),
            KeyCode::Enter => self.insert_with_active_style("\n"),
            KeyCode::Backspace => {
                let color = self.active_text_color;
                let highlight = self.active_highlight;
                self.edit(|draft| {
                    if !draft.backspace_styled(color, highlight) {
                        draft.backspace();
                    }
                });
            }
            KeyCode::Delete => {
                let styled = self.active_text_color.is_some() || self.active_highlight.is_some();
                self.edit(|draft| {
                    if !styled || !draft.styled_delete_is_unsafe() {
                        draft.delete();
                    }
                });
            }
            KeyCode::Left => self.edit(|draft| draft.move_cursor(Motion::Left, extend)),
            KeyCode::Right => self.edit(|draft| draft.move_cursor(Motion::Right, extend)),
            KeyCode::Up => self.edit(|draft| draft.move_cursor(Motion::Up, extend)),
            KeyCode::Down => self.edit(|draft| draft.move_cursor(Motion::Down, extend)),
            KeyCode::Home => self.edit(|draft| draft.move_cursor(Motion::LineStart, extend)),
            KeyCode::End => self.edit(|draft| draft.move_cursor(Motion::LineEnd, extend)),
            KeyCode::PageUp => self.edit(|draft| draft.move_cursor(Motion::PageUp(page), extend)),
            KeyCode::PageDown => {
                self.edit(|draft| draft.move_cursor(Motion::PageDown(page), extend))
            }
            _ => {}
        }
    }

    fn insert_with_active_style(&mut self, text: &str) {
        let color = self.active_text_color;
        let highlight = self.active_highlight;
        let styled = color.is_some() || highlight.is_some();
        self.edit(|draft| {
            if !draft.insert_styled(text, color, highlight) && !styled {
                draft.insert_str(text);
            }
        });
    }

    fn open_format_menu(&mut self) {
        self.format_menu = Some(FormatMenu::Root);
        self.format_selected = 0;
    }

    fn handle_format_key(&mut self, key: KeyEvent) {
        let menu = self.format_menu.unwrap();
        let count = match menu {
            FormatMenu::Root => 4,
            FormatMenu::TextColor => formatting::TEXT_COLORS.len() + 1,
            FormatMenu::Highlight => formatting::HIGHLIGHT_COLORS.len() + 1,
        };
        match key.code {
            KeyCode::Esc => {
                self.format_menu = None;
                self.notice.clear();
            }
            KeyCode::Up => self.format_selected = self.format_selected.saturating_sub(1),
            KeyCode::Down => self.format_selected = (self.format_selected + 1).min(count - 1),
            KeyCode::Enter => self.activate_format_item(self.format_selected),
            _ => {}
        }
    }

    fn activate_format_item(&mut self, index: usize) {
        match self.format_menu {
            Some(FormatMenu::Root) => match index {
                0 => {
                    self.format_menu = Some(FormatMenu::TextColor);
                    self.format_selected = 0;
                }
                1 => {
                    self.format_menu = Some(FormatMenu::Highlight);
                    self.format_selected = 0;
                }
                2 => self.apply_format(Kind::TextColor, None),
                3 => self.apply_format(Kind::Highlight, None),
                _ => {}
            },
            Some(FormatMenu::TextColor) => {
                let color = index
                    .checked_sub(1)
                    .and_then(|i| formatting::TEXT_COLORS.get(i))
                    .map(|(_, value)| *value);
                self.apply_format(Kind::TextColor, color);
            }
            Some(FormatMenu::Highlight) => {
                let color = index
                    .checked_sub(1)
                    .and_then(|i| formatting::HIGHLIGHT_COLORS.get(i))
                    .map(|(_, value)| *value);
                self.apply_format(Kind::Highlight, color);
            }
            None => {}
        }
    }

    fn apply_format(&mut self, kind: Kind, color: Option<&'static str>) {
        let Some(selected) = self.draft.as_ref().and_then(Draft::selected_text) else {
            self.edit(Draft::finish_edit_group);
            match kind {
                Kind::TextColor => self.active_text_color = color,
                Kind::Highlight => self.active_highlight = color,
            }
            self.format_menu = None;
            self.notice.clear();
            return;
        };
        if color.is_none()
            && self
                .draft
                .as_mut()
                .is_some_and(|draft| draft.clear_enclosing_format(kind))
        {
            self.format_menu = None;
            self.notice.clear();
            return;
        }
        let replacement = if let Some(color) = color {
            let (open, close) = formatting::wrapper(kind, color);
            format!("{open}{selected}{close}")
        } else {
            formatting::clear_selected(&selected, kind)
        };
        self.edit(|draft| {
            draft.replace_selection_with(&replacement);
        });
        self.format_menu = None;
        self.notice.clear();
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, area: ratatui::layout::Rect) {
        if self.editor_prompt.is_none() && self.discard_confirmation.is_none() {
            self.notice.clear();
        }
        let target = ui::hit_test(area, self, mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.activate_mouse_target(target),
            MouseEventKind::ScrollUp => self.scroll_mouse_target(target, -3),
            MouseEventKind::ScrollDown => self.scroll_mouse_target(target, 3),
            _ => {}
        }
    }

    fn activate_mouse_target(&mut self, target: ui::HitTarget) {
        match target {
            ui::HitTarget::Panel(panel) => {
                self.request_mouse_action(PendingMouseAction::Panel(panel))
            }
            ui::HitTarget::Search => self.request_mouse_action(PendingMouseAction::Search),
            ui::HitTarget::ListRow(index) => {
                self.request_mouse_action(PendingMouseAction::Row(index))
            }
            ui::HitTarget::TaskCheckbox(index) => {
                self.request_mouse_action(PendingMouseAction::TaskToggle(index))
            }
            ui::HitTarget::FormatControl if self.focus == Focus::Editor => self.open_format_menu(),
            ui::HitTarget::FormatItem(index) => self.activate_format_item(index),
            _ => {}
        }
    }

    fn request_mouse_action(&mut self, action: PendingMouseAction) {
        if self.focus == Focus::Editor && self.pending_text().is_some() {
            self.pending_mouse_action = Some(action);
            self.editor_prompt = Some(EditorPrompt::Pending);
            return;
        }
        if self.focus == Focus::Editor {
            self.close_editor();
        }
        self.perform_mouse_action(action);
    }

    fn perform_mouse_action(&mut self, action: PendingMouseAction) {
        self.focus = Focus::List;
        match action {
            PendingMouseAction::Panel(panel) => {
                self.panel = panel;
                self.on_panel_switched();
            }
            PendingMouseAction::Row(index) => match self.panel {
                ActivePanel::RecentNotes if index < self.recent_notes.len() => {
                    self.recent_selected = index;
                    let id = self.recent_notes[index].id;
                    self.load_note(id);
                    self.start_editing();
                }
                ActivePanel::PendingTasks if index < self.pending_tasks.len() => {
                    self.tasks_selected = index;
                    let id = self.pending_tasks[index].note_id;
                    self.load_note(id);
                    self.start_editing();
                }
                ActivePanel::Trash if index < self.trash_items.len() => {
                    self.trash_selected = index;
                    self.load_selected_trash(false);
                    self.focus = Focus::Reader;
                }
                _ => {}
            },
            PendingMouseAction::Search => {
                self.focus = Focus::Search;
                self.search_query.clear();
                self.search_results.clear();
                self.search_selected = 0;
            }
            PendingMouseAction::TaskToggle(index) => {
                self.tasks_selected = index;
                self.load_selected_task_note();
                self.toggle_task();
                self.focus = Focus::List;
            }
        }
    }

    fn scroll_mouse_target(&mut self, target: ui::HitTarget, delta: isize) {
        match target {
            ui::HitTarget::Reader => {
                self.focus = Focus::Reader;
                self.move_cursor(delta);
            }
            ui::HitTarget::Editor => {
                let motion = if delta < 0 {
                    Motion::PageUp(delta.unsigned_abs())
                } else {
                    Motion::PageDown(delta.unsigned_abs())
                };
                self.edit(|draft| draft.move_cursor(motion, false));
            }
            ui::HitTarget::List | ui::HitTarget::ListRow(_) => {
                if self.focus == Focus::Editor && self.pending_text().is_some() {
                    self.editor_prompt = Some(EditorPrompt::Pending);
                    return;
                }
                if self.focus == Focus::Editor {
                    self.close_editor();
                }
                let code = if delta < 0 {
                    KeyCode::Up
                } else {
                    KeyCode::Down
                };
                self.focus = Focus::List;
                for _ in 0..delta.unsigned_abs() {
                    self.handle_key_list(KeyEvent::from(code));
                }
            }
            _ => {}
        }
    }

    fn edit(&mut self, change: impl FnOnce(&mut Draft)) {
        if let Some(draft) = self.draft.as_mut() {
            change(draft);
        }
    }

    /// Answers the question the editor is currently holding.
    fn answer_prompt(&mut self, prompt: EditorPrompt, key: KeyEvent) {
        match (prompt, key.code) {
            (EditorPrompt::Pending, KeyCode::Char('s' | 'S')) => {
                self.save_draft();
                // A conflict asks its own question; only a settled save leaves.
                if self.editor_prompt.is_none() {
                    self.leave_after_answer();
                    if let Some(action) = self.pending_mouse_action.take() {
                        self.perform_mouse_action(action);
                    }
                } else {
                    // A request to leave does not survive a conflict: that is a
                    // new situation to decide, and asking again costs one key.
                    self.exit_requested = false;
                }
            }
            (EditorPrompt::Pending, KeyCode::Char('d' | 'D')) => {
                self.leave_after_answer();
                self.notice = "Alterações descartadas".into();
                if let Some(action) = self.pending_mouse_action.take() {
                    self.perform_mouse_action(action);
                }
            }
            (EditorPrompt::Conflict, KeyCode::Char('p' | 'P')) => self.preserve_and_reread(),
            (EditorPrompt::Conflict, KeyCode::Char('r' | 'R')) => {
                self.reread_into_editor();
                self.notice = "Nota relida; o rascunho anterior foi descartado".into();
            }
            (_, KeyCode::Esc) => {
                // Continuing to edit also cancels the exit that asked.
                self.exit_requested = false;
                self.pending_mouse_action = None;
                self.notice = match prompt {
                    EditorPrompt::Pending => "Edição retomada".into(),
                    EditorPrompt::Conflict => "Rascunho mantido no editor; nada foi escrito".into(),
                };
            }
            // Anything else is not an answer: keep asking.
            _ => self.editor_prompt = Some(prompt),
        }
    }

    /// Opens the loaded note in the right pane for editing.
    fn start_editing(&mut self) {
        let Some(note) = self.current_note.as_ref().filter(|note| !note.in_trash) else {
            self.focus = Focus::Reader;
            return;
        };
        self.draft = Some(Draft::new(&note.content));
        self.editor_prompt = None;
        self.active_text_color = None;
        self.active_highlight = None;
        self.editor_scroll.set(0);
        self.editor_column.set(0);
        self.focus = Focus::Editor;
    }

    /// Leaves editing for reading, asking first when text would be lost.
    fn leave_editor(&mut self) {
        if self.pending_text().is_some() {
            self.editor_prompt = Some(EditorPrompt::Pending);
            return;
        }
        self.close_editor();
    }

    fn close_editor(&mut self) {
        self.draft = None;
        self.editor_prompt = None;
        self.focus = Focus::Reader;
        self.active_text_color = None;
        self.active_highlight = None;
        self.notice.clear();
    }

    /// Closes the editor and, when the question was raised by a request to
    /// leave, finishes leaving.
    fn leave_after_answer(&mut self) {
        let leaving = std::mem::take(&mut self.exit_requested);
        self.close_editor();
        self.should_quit |= leaving;
    }

    /// The draft, when it says something the store does not already say.
    ///
    /// The question is asked through `editor::mutation_for`, the same function
    /// the external editor decides with, so "pending" means exactly one thing
    /// in this application: a change the canonical model would persist. A body
    /// that differs only by trailing newlines is not pending, and is not
    /// written, in either editor.
    pub fn pending_text(&self) -> Option<String> {
        let note = self.current_note.as_ref()?;
        let text = self.draft.as_ref()?.text();
        editor::mutation_for(&note.content, &text).map(|_| text)
    }

    /// Writes the draft through the Core with the revision that was read.
    fn save_draft(&mut self) {
        let Some(note) = self.current_note.as_ref().filter(|note| !note.in_trash) else {
            return;
        };
        let Some(text) = self.pending_text() else {
            self.notice = "Sem alteração canônica; nenhuma escrita".into();
            return;
        };
        let Some(mutation) = editor::mutation_for(&note.content, &text) else {
            return;
        };
        let id = note.id;
        let operation = WriteOperation::MutateNote {
            selector: id.to_string(),
            mutation,
            // No reread, no retry: this is the revision the pane was opened on.
            expected_revision: Some(note.revision.clone()),
        };
        match authority::perform_at(&self.paths, &operation) {
            Ok(_) => {
                let cursor = self.draft.as_ref().map(Draft::cursor);
                self.reload_all();
                self.reload_note(id);
                // The saved text is now the note; the draft continues from it
                // with the new revision underneath and the cursor where it was.
                self.reseat_draft(cursor);
                self.notice = "Edição salva".into();
            }
            Err(WriteError::RevisionConflict { .. }) => {
                // Nothing was overwritten and nothing is thrown away: the draft
                // stays on screen until its owner says what to do with it.
                self.editor_prompt = Some(EditorPrompt::Conflict);
            }
            Err(error) => self.notice = format!("Edição não confirmada: {error}"),
        }
    }

    /// Rebuilds the draft from the note as it is now, keeping `cursor`.
    fn reseat_draft(&mut self, cursor: Option<crate::draft::Position>) {
        let Some(note) = self.current_note.as_ref() else {
            self.draft = None;
            return;
        };
        let mut draft = Draft::new(&note.content);
        if let Some(cursor) = cursor {
            draft.restore_cursor(cursor);
        }
        self.draft = Some(draft);
    }

    fn reread_into_editor(&mut self) {
        let Some(id) = self.current_note.as_ref().map(|note| note.id) else {
            return;
        };
        let cursor = self.draft.as_ref().map(Draft::cursor);
        self.reload_all();
        self.reload_note(id);
        self.reseat_draft(cursor);
    }

    /// Puts the refused draft where it can be found again, then rereads.
    fn preserve_and_reread(&mut self) {
        match self.preserve_draft() {
            Some(Ok(path)) => {
                self.reread_into_editor();
                self.notice = format!("Rascunho preservado em {}; nota relida", path.display());
            }
            Some(Err(error)) => {
                // Preservation failed, so the draft is the only copy there is
                // and it is not touched.
                self.editor_prompt = Some(EditorPrompt::Conflict);
                self.notice = format!("Falha ao preservar o rascunho: {error}");
            }
            None => self.notice = "Nada pendente para preservar".into(),
        }
    }

    /// Writes the pending draft to the recovery directory, if there is one.
    ///
    /// The directory comes from the store's own `state_dir`, which is
    /// `$XDG_STATE_HOME/note-it` — the same place the external editor writes.
    fn preserve_draft(&mut self) -> Option<io::Result<std::path::PathBuf>> {
        let text = self.pending_text()?;
        let id = self.current_note.as_ref()?.id;
        let directory = self.paths.state_dir.join(editor::RECOVERY_DIRECTORY);
        Some(editor::preserve_draft(&directory, id, text.as_bytes()))
    }

    fn handle_key_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                // Cancel search mode
                self.focus = Focus::List;
                self.search_query.clear();
                self.search_results.clear();
            }
            KeyCode::Enter => {
                // Open highlighted search result
                if let Some(res) = self.search_results.get(self.search_selected) {
                    self.load_note(res.note_id);
                    self.start_editing();
                } else {
                    self.focus = Focus::List;
                }
            }
            KeyCode::Up => {
                if self.search_selected > 0 {
                    self.search_selected -= 1;
                }
            }
            KeyCode::Down => {
                if !self.search_results.is_empty()
                    && self.search_selected + 1 < self.search_results.len()
                {
                    self.search_selected += 1;
                }
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.update_search();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.update_search();
            }
            _ => {}
        }
    }

    fn on_panel_switched(&mut self) {
        self.notice.clear();
        if self.panel == ActivePanel::RecentNotes {
            if let Some(note) = self.recent_notes.get(self.recent_selected) {
                self.load_note(note.id);
            }
        }
        if self.panel == ActivePanel::Trash {
            self.load_selected_trash(false);
        }
        if self.panel == ActivePanel::PendingTasks {
            self.load_selected_task_note();
        }
    }

    fn load_selected_task_note(&mut self) {
        if let Some(task) = self.pending_tasks.get(self.tasks_selected) {
            let id = task.note_id;
            let line = task.line_number.saturating_sub(1);
            self.load_note(id);
            self.reader_cursor = line;
        }
    }

    fn toggle_selected_task(&mut self) {
        self.load_selected_task_note();
        self.toggle_task();
        self.focus = Focus::List;
    }

    fn move_cursor(&mut self, delta: isize) {
        let last = self
            .current_note
            .as_ref()
            .map_or(0, |note| note.content.lines().count().saturating_sub(1));
        self.reader_cursor = self.reader_cursor.saturating_add_signed(delta).min(last);
    }

    fn load_selected_trash(&mut self, force: bool) {
        let Some(id) = self
            .trash_items
            .get(self.trash_selected)
            .map(|item| item.note_id)
        else {
            self.current_note = None;
            self.current_note_id = None;
            return;
        };
        if !force
            && self
                .current_note
                .as_ref()
                .is_some_and(|note| note.id == id && note.in_trash)
        {
            return;
        }
        self.current_note = self
            .core
            .read_trash_note(&id)
            .ok()
            .and_then(|doc| LoadedDocument::new(doc, true).ok());
        self.current_note_id = self.current_note.as_ref().map(|note| note.id);
        self.reader_cursor = 0;
    }

    fn reload_note(&mut self, id: Uuid) {
        let cursor = self.reader_cursor;
        self.current_note = None;
        self.load_note(id);
        self.reader_cursor = cursor;
        self.move_cursor(0);
    }

    fn mutation_error(&mut self, error: WriteError, id: Uuid, in_trash: bool) {
        self.notice = if matches!(error, WriteError::RevisionConflict { .. }) {
            self.reload_all();
            if in_trash {
                self.load_selected_trash(true);
            } else {
                self.reload_note(id);
            }
            "Conflito de revision; nenhuma sobrescrita. Nota recarregada; ação não repetida.".into()
        } else {
            format!("Operação não confirmada: {error}")
        };
    }

    pub fn toggle_task(&mut self) {
        let Some(note) = self.current_note.as_ref().filter(|note| !note.in_trash) else {
            return;
        };
        let tasks = noteit_core::task::parse_tasks(
            note.id,
            &noteit_core::search::label_for(&note.content),
            &note.content,
        );
        let Some(task) = tasks
            .into_iter()
            .find(|task| task.line_number == self.reader_cursor + 1)
        else {
            self.notice = "Posicione o cursor sobre uma tarefa".into();
            return;
        };
        let id = note.id;
        let task_ref = task.task_ref.to_string();
        let mutation = if task.checked {
            NoteMutation::ReopenTask { task_ref }
        } else {
            NoteMutation::CompleteTask { task_ref }
        };
        let operation = WriteOperation::MutateNote {
            selector: id.to_string(),
            mutation,
            expected_revision: Some(note.revision.clone()),
        };
        match authority::perform_at(&self.paths, &operation) {
            Ok(_) => {
                self.reload_all();
                self.reload_note(id);
                self.notice = "Tarefa alternada".into();
            }
            Err(error) => self.mutation_error(error, id, false),
        }
    }

    fn create_note(&mut self) {
        let operation = WriteOperation::CreateNote {
            draft: NoteDraft {
                content: String::new(),
                tags: Vec::new(),
                properties: Vec::new(),
            },
        };
        match authority::perform_at(&self.paths, &operation) {
            Ok(write) => {
                self.reload_all();
                self.load_note(write.outcome.note_id);
                self.start_editing();
                self.notice = "Nota criada".into();
            }
            Err(error) => self.notice = format!("Não foi possível criar a nota: {error}"),
        }
    }

    fn discard(&mut self, note: LoadedDocument) {
        let operation = WriteOperation::DiscardNote {
            selector: note.id.to_string(),
            expected_revision: note.revision,
        };
        match authority::perform_at(&self.paths, &operation) {
            Ok(_) => {
                self.current_note = None;
                self.current_note_id = None;
                self.reload_all();
                self.panel = ActivePanel::RecentNotes;
                self.focus = Focus::List;
                self.on_panel_switched();
                self.notice = "Nota movida para a lixeira".into();
            }
            Err(error) => self.mutation_error(error, note.id, false),
        }
    }

    fn restore_selected(&mut self) {
        let Some(note) = self.current_note.as_ref().filter(|note| note.in_trash) else {
            return;
        };
        let id = note.id;
        let operation = WriteOperation::RestoreFromTrashAtRevision {
            selector: id.to_string(),
            expected_revision: note.revision.clone(),
        };
        match authority::perform_at(&self.paths, &operation) {
            Ok(_) => {
                self.reload_all();
                self.load_selected_trash(true);
                self.notice = "Nota restaurada".into();
            }
            Err(error) => self.mutation_error(error, id, true),
        }
    }

    fn open_editor(
        &mut self,
        guard: &mut TerminalGuard,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> io::Result<()> {
        let Some(note) = self.current_note.clone().filter(|note| !note.in_trash) else {
            return Ok(());
        };
        let session = match editor::EditorSession::prepare(note, &std::env::temp_dir()) {
            Ok(session) => session,
            Err(error) => {
                self.notice = format!("Não foi possível preparar editor: {error}");
                return Ok(());
            }
        };
        if let Err(error) = guard.suspend() {
            // A partial terminal transition must not launch a child or return
            // to the input loop. Drop/normal exit retry the shared cleanup.
            return Err(io::Error::other(format!(
                "Falha ao suspender terminal: {error}. Temporário preservado em {}",
                session.temporary.display()
            )));
        }
        let run = session.run(
            &editor::editor_program(std::env::var_os("EDITOR")),
            &self.term_flag,
        );
        if let Err(error) = guard
            .resume()
            // Fullscreen resize clears the display and invalidates Ratatui's
            // previous buffer, even if the editor did not change dimensions.
            .and_then(|()| {
                terminal
                    .size()
                    .and_then(|size| terminal.resize(size.into()))
            })
        {
            return Err(io::Error::other(format!(
                "Falha ao retomar terminal: {error}. Temporário preservado em {}",
                session.temporary.display()
            )));
        }
        let run = if self.term_flag.load(Ordering::Relaxed) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Editor interrompido",
            ))
        } else {
            run
        };
        let result = session.finish(
            &self.paths,
            run,
            editor::recovery_directory(
                std::env::var_os("XDG_STATE_HOME"),
                std::env::var_os("HOME"),
            ),
        );
        if result.reload {
            self.reload_all();
            self.reload_note(session.original.id);
        }
        self.notice = result.message;
        // A signal exits without another frame: do not hide the recovery path.
        if self.term_flag.load(Ordering::Relaxed) {
            guard.suspend()?;
            eprintln!("{}", self.notice);
        }
        Ok(())
    }

    /// Draws the current application frame to any backend (useful for tests and headless validation).
    pub fn draw<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), B::Error> {
        terminal.draw(|frame| ui::render(frame, self))?;
        Ok(())
    }

    /// Runs the interactive session and then settles anything it left pending.
    ///
    /// The shutdown half is deliberately outside the loop *and* outside its
    /// `?`: quitting, `Ctrl+C`, a signal and a frame that failed to reach a
    /// terminal that is no longer there all end up here. The last of those is
    /// not hypothetical — the usual companion of a `SIGHUP` is the very write
    /// that discovers the terminal is gone — and whether the process noticed
    /// the signal or the broken write first must not decide whether somebody's
    /// unsaved text survives.
    pub fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        guard: &mut TerminalGuard,
    ) -> io::Result<()> {
        let outcome = self.event_loop(terminal, guard);

        // A signal cannot be asked a question, so text that never reached the
        // store is written where it can be found again and its path is printed
        // on the restored terminal. The file is the guarantee; the message is a
        // courtesy, and a terminal that can no longer be written to must not
        // suppress either.
        if let Some(preserved) = self.preserve_draft() {
            let message = match preserved {
                Ok(path) => format!("Rascunho não salvo preservado em {}", path.display()),
                Err(error) => format!("Rascunho não salvo NÃO pôde ser preservado: {error}"),
            };
            let suspended = guard.suspend();
            eprintln!("{message}");
            // The loop's own failure, if there was one, is the one reported.
            return outcome.and(suspended);
        }

        outcome
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        guard: &mut TerminalGuard,
    ) -> io::Result<()> {
        terminal.draw(|frame| ui::render(frame, self))?;
        let mut needs_redraw = false;

        while !self.should_quit {
            if self.term_flag.load(Ordering::Relaxed) {
                break;
            }

            if needs_redraw {
                terminal.draw(|frame| ui::render(frame, self))?;
                needs_redraw = false;
            }

            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => {
                        self.handle_key(key);
                        if std::mem::take(&mut self.editor_requested) {
                            self.open_editor(guard, terminal)?;
                        }
                        needs_redraw = true;
                    }
                    Event::Resize(_cols, _rows) => {
                        terminal.autoresize()?;
                        needs_redraw = true;
                    }
                    Event::Mouse(mouse) => {
                        let size = terminal.size()?;
                        self.handle_mouse(mouse, size.into());
                        needs_redraw = true;
                    }
                    _ => {}
                }
            }

            if self.term_flag.load(Ordering::Relaxed) {
                break;
            }
        }

        Ok(())
    }
}
