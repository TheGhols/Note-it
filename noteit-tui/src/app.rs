//! Application state and event loop for Note-it TUI.
//!
//! Provides interactive navigation across recent notes, pending tasks, and trash,
//! with quick search (/) using noteit-core in-process.

use crate::{document::LoadedDocument, editor, terminal::TerminalGuard, ui};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    List,
    Reader,
    Search,
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
        // Ctrl+C always terminates the application immediately
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
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
            Focus::List => self.handle_key_list(key),
        }

        // Selection changes load a canonical trash snapshot for preview and
        // subsequent restore. Restore itself never refreshes its precondition.
        if self.panel == ActivePanel::Trash && self.focus != Focus::Search {
            self.load_selected_trash(false);
        }

        self.should_quit
    }

    fn handle_key_list(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('n') if self.panel == ActivePanel::RecentNotes => self.create_note(),
            KeyCode::Char('r') if self.panel == ActivePanel::Trash => self.restore_selected(),
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

            // Open / switch focus to Reader
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => match self.panel {
                ActivePanel::RecentNotes => {
                    if let Some(note) = self.recent_notes.get(self.recent_selected) {
                        self.load_note(note.id);
                        self.focus = Focus::Reader;
                    }
                }
                ActivePanel::PendingTasks => {
                    if let Some(task) = self.pending_tasks.get(self.tasks_selected) {
                        self.load_note(task.note_id);
                        self.focus = Focus::Reader;
                    }
                }
                ActivePanel::Trash => {
                    // Trash note preview
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
                    self.focus = Focus::Reader;
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
        if self.panel == ActivePanel::RecentNotes {
            if let Some(note) = self.recent_notes.get(self.recent_selected) {
                self.load_note(note.id);
            }
        }
        if self.panel == ActivePanel::Trash {
            self.load_selected_trash(false);
        }
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
                self.focus = Focus::Reader;
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

    /// Main interactive event loop.
    pub fn run(
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
