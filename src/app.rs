use crate::cli::CliCommand;
use crate::diagnostics::{self, LayerToggleTrace};
use crate::layer_shell::{
    calculate_cascade_position, find_monitor_by_connector, install_paper_color_styles,
    DEFAULT_MONITOR_HEIGHT, DEFAULT_MONITOR_WIDTH,
};
use crate::model::NoteDocument;
use crate::note_window::{NoteWindow, NoteWindowOptions};
use crate::search;
use crate::settings::{theme_name, AppConfig};
use crate::state::{next_collapse_all, AppState, LayerMode, NoteWindowState};
use crate::storage::StorageManager;
use gio::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use uuid::Uuid;

static NEXT_FLUSH_ID: AtomicU64 = AtomicU64::new(1);
const LAYER_PERSISTENCE_DEBOUNCE: Duration = Duration::from_millis(180);

type LifecycleCallback = Box<dyn FnOnce(Result<(), String>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleOperation {
    Hide,
    Quit,
}

#[derive(Debug, Default)]
struct LifecycleCoordinator {
    active: Option<LifecycleOperation>,
}

impl LifecycleCoordinator {
    fn begin(&mut self, operation: LifecycleOperation) -> Result<(), String> {
        if let Some(active) = self.active {
            return Err(format!(
                "lifecycle operation {active:?} is already in progress"
            ));
        }
        self.active = Some(operation);
        Ok(())
    }

    fn finish(&mut self, operation: LifecycleOperation) {
        if self.active == Some(operation) {
            self.active = None;
        }
    }

    fn is_active(&self) -> bool {
        self.active.is_some()
    }

    fn ensure_structural_action_allowed(&self, action: &str) -> Result<(), String> {
        if let Some(active) = self.active {
            return Err(format!(
                "{action} is unavailable while lifecycle operation {active:?} is in progress"
            ));
        }
        Ok(())
    }
}

/// What a summon should do about the layer the notes sit on.
///
/// A `Bottom` surface is always below ordinary windows on Wayland, so a note
/// left on the desktop cannot be made visible over another application without
/// moving it to the overlay. The elevation is deliberately temporary: the
/// user's own preference is remembered rather than overwritten.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum SummonLayerPlan {
    /// Apply the persisted mode unchanged.
    Persisted(LayerMode),
    /// Show above other windows, remembering the mode to return to.
    Elevate { restore: LayerMode },
}

fn plan_summon_layer(persisted: LayerMode, already_running: bool) -> SummonLayerPlan {
    match persisted {
        // Coming back from hidden is a real state change, as `note-it toggle`
        // already treats it.
        LayerMode::Hidden => SummonLayerPlan::Persisted(LayerMode::Overlay),
        // Only a summon into a running application elevates. Launching the
        // application is not a summon, so it simply honours the preference.
        LayerMode::Desktop if already_running => SummonLayerPlan::Elevate {
            restore: LayerMode::Desktop,
        },
        other => SummonLayerPlan::Persisted(other),
    }
}

fn is_live_layer_noop(
    stored: LayerMode,
    target: LayerMode,
    has_windows: bool,
    summon_restore: Option<LayerMode>,
) -> bool {
    target != LayerMode::Hidden && has_windows && summon_restore.is_none() && stored == target
}

fn preferred_layer_after_new_note(
    stored: LayerMode,
    live_target: LayerMode,
    summon_restore: Option<LayerMode>,
) -> LayerMode {
    if summon_restore.is_some() {
        stored
    } else {
        live_target
    }
}

#[derive(Debug, PartialEq, Eq)]
enum StartupPlan {
    Background,
    CreateNew,
    Restore {
        note_ids: Vec<Uuid>,
        mode: LayerMode,
    },
}

struct FlushBatch {
    remaining: usize,
    first_error: Option<String>,
    on_complete: Option<LifecycleCallback>,
}

#[derive(Debug, Default)]
struct StatePersistenceDebouncer {
    generation: u64,
    pending: Option<u64>,
}

impl StatePersistenceDebouncer {
    fn schedule(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.pending = Some(self.generation);
        self.generation
    }

    fn settle(&mut self, generation: u64) -> bool {
        if self.pending != Some(generation) {
            return false;
        }
        self.pending = None;
        true
    }

    fn cancel(&mut self) {
        self.pending = None;
    }
}

impl FlushBatch {
    fn new(total: usize, on_complete: LifecycleCallback) -> Self {
        Self {
            remaining: total,
            first_error: None,
            on_complete: Some(on_complete),
        }
    }

    fn record(
        &mut self,
        result: Result<(), String>,
    ) -> Option<(LifecycleCallback, Result<(), String>)> {
        if self.remaining == 0 {
            return None;
        }
        if let Err(error) = result {
            if self.first_error.is_none() {
                self.first_error = Some(error);
            }
        }
        self.remaining -= 1;
        if self.remaining != 0 {
            return None;
        }

        let result = self.first_error.take().map_or(Ok(()), Err);
        self.on_complete.take().map(|callback| (callback, result))
    }
}

pub struct AppContext {
    pub storage: StorageManager,
    pub config: AppConfig,
    pub state: AppState,
    pub windows: HashMap<Uuid, NoteWindow>,
    pub ui_dist_path: PathBuf,
    lifecycle: LifecycleCoordinator,
    /// Layer preference to return to after a temporary summon elevation.
    /// Never persisted: it only records that the live layer is currently
    /// ahead of the stored preference.
    summon_restore: Option<LayerMode>,
    /// False until the first command has been handled, so launching the
    /// application is not mistaken for summoning it.
    activated: bool,
    layer_state_persistence: StatePersistenceDebouncer,
}

pub struct NoteItApp {
    app: gtk4::Application,
    context: Rc<RefCell<AppContext>>,
    _hold_guard: gio::ApplicationHoldGuard,
}

impl NoteItApp {
    pub fn new(app: &gtk4::Application) -> Self {
        let hold_guard = app.hold();

        diagnostics::log(format_args!(
            "event=startup layer_shell_protocol_version={}",
            gtk4_layer_shell::protocol_version()
        ));

        if let Some(display) = gtk4::gdk::Display::default() {
            install_paper_color_styles(&display);
        }

        let storage = StorageManager::new().expect("Failed to initialize XDG storage");
        let config = AppConfig::load_from_file(&storage.config_file_path());
        let state = AppState::load_from_file(&storage.state_file_path());
        let ui_dist_path = find_ui_dist_path();

        let context = Rc::new(RefCell::new(AppContext {
            storage,
            config,
            state,
            windows: HashMap::new(),
            ui_dist_path,
            lifecycle: LifecycleCoordinator::default(),
            summon_restore: None,
            activated: false,
            layer_state_persistence: StatePersistenceDebouncer::default(),
        }));

        let action = gio::SimpleAction::new("toggle-layer", None);
        let action_context = Rc::clone(&context);
        let weak_app = app.downgrade();
        action.connect_activate(move |_, _| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            NoteItAppClone {
                app,
                context: Rc::clone(&action_context),
            }
            .toggle_layer_mode_from("gaction");
        });
        app.add_action(&action);

        Self {
            app: app.clone(),
            context,
            _hold_guard: hold_guard,
        }
    }

    pub fn controller(&self) -> NoteItAppClone {
        NoteItAppClone {
            app: self.app.clone(),
            context: Rc::clone(&self.context),
        }
    }

    pub fn handle_command(&self, command: Option<CliCommand>, is_background: bool) {
        let controller = self.controller();
        match command {
            Some(CliCommand::New) => {
                controller.create_new_note();
            }
            Some(CliCommand::Toggle) => {
                controller.toggle_layer_mode_from("command-line");
            }
            Some(CliCommand::Show) => {
                controller.set_layer_mode(LayerMode::Overlay);
            }
            Some(CliCommand::Hide) => {
                controller.set_layer_mode(LayerMode::Hidden);
            }
            Some(CliCommand::ToggleCollapseAll) => {
                controller.toggle_collapse_all();
            }
            Some(CliCommand::Quit) => {
                controller.save_and_quit();
            }
            None => {
                controller.summon(is_background);
            }
        }
        controller.context.borrow_mut().activated = true;
    }
}

#[derive(Clone)]
pub struct NoteItAppClone {
    pub app: gtk4::Application,
    pub context: Rc<RefCell<AppContext>>,
}

impl NoteItAppClone {
    /// Brings Note-it to the user: restores the notes and makes them visible.
    ///
    /// This is what a global keybinding runs. It reaches the already running
    /// instance through the single-instance command line, so no second
    /// application is ever started.
    pub fn summon(&self, is_background: bool) {
        if is_background {
            let mut ctx = self.context.borrow_mut();
            ctx.state.active_layer_mode = LayerMode::Hidden;
            return;
        }

        let (persisted, already_running) = {
            let ctx = self.context.borrow();
            (ctx.state.active_layer_mode, ctx.activated)
        };

        match plan_summon_layer(persisted, already_running) {
            SummonLayerPlan::Persisted(mode) => self.restore_saved_notes_in_mode(mode),
            SummonLayerPlan::Elevate { restore } => {
                self.restore_saved_notes_in_mode(LayerMode::Overlay);
                // The stored preference stays as it was; only the live layer
                // moves, so the note returns to the desktop on the next
                // explicit layer change or restart.
                let mut ctx = self.context.borrow_mut();
                ctx.state.active_layer_mode = restore;
                ctx.summon_restore = Some(restore);
                if let Err(error) = persist_state_now(&mut ctx, "summon") {
                    eprintln!("Failed to persist note state after summon: {error}");
                }
            }
        }
    }

    /// Collapses every open note, or expands them all when they are already
    /// collapsed.
    ///
    /// Reached from the compositor through the same single-instance dispatcher
    /// as every other command, and applied through each window's own collapse
    /// path, so nothing here duplicates the per-note behaviour.
    pub fn toggle_collapse_all(&self) {
        if let Err(error) = self.ensure_structural_action_allowed("collapsing every note") {
            eprintln!("Collapse-all rejected: {error}");
            return;
        }

        let windows: Vec<NoteWindow> = {
            let ctx = self.context.borrow();
            ctx.windows.values().cloned().collect()
        };
        let flags: Vec<bool> = windows.iter().map(|window| window.is_collapsed()).collect();
        let Some(collapsed) = next_collapse_all(&flags) else {
            return;
        };

        let mut changed: Vec<(Uuid, NoteWindowState)> = Vec::new();
        for window in &windows {
            if let Some(snapshot) = window.set_collapsed(collapsed) {
                changed.push((window.id, snapshot));
            }
        }
        if changed.is_empty() {
            return;
        }

        let mut ctx = self.context.borrow_mut();
        for (id, snapshot) in changed {
            ctx.state.notes.insert(id, snapshot);
        }
        if let Err(error) = persist_state_now(&mut ctx, "collapse-all") {
            eprintln!("Failed to persist collapse state for every note: {error}");
        }
    }

    /// The layer the surfaces are actually on, which is ahead of the stored
    /// preference while a summon elevation is in effect.
    fn effective_layer_mode(&self) -> LayerMode {
        let ctx = self.context.borrow();
        effective_layer_mode(ctx.state.active_layer_mode, ctx.summon_restore)
    }

    fn restore_saved_notes_in_mode(&self, mode: LayerMode) {
        self.restore_saved_notes(false);
        let windows: Vec<NoteWindow> = self.context.borrow().windows.values().cloned().collect();
        for window in windows {
            window.set_layer_mode(mode);
        }
    }

    pub fn restore_saved_notes(&self, is_background: bool) {
        if is_background {
            let mut ctx = self.context.borrow_mut();
            ctx.state.active_layer_mode = LayerMode::Hidden;
            return;
        }

        let plan = {
            let ctx = self.context.borrow();
            let ids_by_recency = match ctx.storage.list_notes_by_recency() {
                Ok(ids) => ids,
                Err(error) => {
                    eprintln!("Failed to list saved notes: {error}");
                    Vec::new()
                }
            };
            plan_startup(false, ids_by_recency, &ctx.state)
        };

        match plan {
            StartupPlan::Background => {}
            StartupPlan::CreateNew => self.create_new_note(),
            StartupPlan::Restore { note_ids, mode } => {
                for id in &note_ids {
                    self.instantiate_note_by_id(*id, mode);
                }
                self.mark_notes_open(&note_ids, mode);
            }
        }
    }

    pub fn create_new_note(&self) {
        if let Err(error) = self.ensure_structural_action_allowed("new note creation") {
            eprintln!("New note creation rejected: {error}");
            return;
        }

        let display = gtk4::gdk::Display::default();
        let (_, conn_name, mon_w, mon_h) = find_monitor_by_connector(display.as_ref(), None);
        let (doc, win_state, target_mode) = {
            let ctx = self.context.borrow();
            prepare_new_note(
                &ctx.config,
                // The layer the other notes are really on: a note created
                // right after a summon must not be filed behind every window
                // while its siblings sit on top.
                effective_layer_mode(ctx.state.active_layer_mode, ctx.summon_restore),
                ctx.windows.len(),
                conn_name,
                mon_w,
                mon_h,
            )
        };
        let note_id = doc.metadata.id;

        {
            let ctx = self.context.borrow();
            if let Err(error) = ctx.storage.save_note_atomic(&doc) {
                eprintln!("Failed to save new note {note_id}: {error}");
                return;
            }
        }

        let note_window = {
            let ctx = self.context.borrow();
            instantiate_note_window(
                &self.app,
                &ctx,
                self.clone(),
                doc,
                win_state.clone(),
                target_mode,
            )
        };

        let mut ctx = self.context.borrow_mut();
        ctx.state.notes.insert(note_id, win_state);
        ctx.windows.insert(note_id, note_window);
        ctx.state.active_layer_mode = preferred_layer_after_new_note(
            ctx.state.active_layer_mode,
            target_mode,
            ctx.summon_restore,
        );
        if let Err(error) = persist_state_now(&mut ctx, "new-note") {
            eprintln!("Failed to persist application state after note creation: {error}");
        }
    }

    pub fn close_note(&self, id: &Uuid) -> Result<(), String> {
        self.ensure_structural_action_allowed("closing a note")?;
        let mut ctx = self.context.borrow_mut();
        if !ctx.windows.contains_key(id) {
            return Err("note window is not instantiated".to_string());
        }

        let previous_state = ctx.state.clone();
        ctx.state.notes.entry(*id).or_default().is_open = false;
        if let Err(error) = persist_state_now(&mut ctx, "close-note") {
            ctx.state = previous_state;
            return Err(format!("failed to persist closed note state: {error}"));
        }

        if let Some(win) = ctx.windows.remove(id) {
            win.close_after_save();
        }
        Ok(())
    }

    pub fn update_geometry(&self, id: Uuid, geom: NoteWindowState) {
        if let Err(error) = self.ensure_structural_action_allowed("changing note geometry") {
            eprintln!("Geometry update rejected: {error}");
            return;
        }
        let mut ctx = self.context.borrow_mut();
        ctx.state.notes.insert(id, geom);
        if let Err(error) = persist_state_now(&mut ctx, "geometry") {
            eprintln!("Failed to persist updated note geometry: {error}");
        }
    }

    pub fn flush_all_windows<F: FnOnce(Result<(), String>) + 'static>(&self, on_complete: F) {
        let windows: Vec<NoteWindow> = {
            let ctx = self.context.borrow();
            ctx.windows.values().cloned().collect()
        };

        if windows.is_empty() {
            on_complete(Ok(()));
            return;
        }

        let batch = Rc::new(RefCell::new(FlushBatch::new(
            windows.len(),
            Box::new(on_complete),
        )));
        let request_id = NEXT_FLUSH_ID.fetch_add(1, Ordering::SeqCst);

        for window in windows {
            let batch_clone = Rc::clone(&batch);

            window.request_flush(request_id, move |result| {
                let completion = batch_clone.borrow_mut().record(result);
                if let Some((callback, result)) = completion {
                    callback(result);
                }
            });
        }
    }

    /// The shared interface theme, chosen from any note's menu.
    ///
    /// It dresses the application's chrome and nothing else: every note keeps
    /// the paper colour, pattern and intensity it was given. The preference is
    /// global, so it is stored once in `config.toml` and broadcast to every
    /// open note rather than written into any of them.
    pub fn set_theme(&self, theme: &str) {
        let resolved = theme_name(theme);
        let windows: Vec<NoteWindow> = {
            let mut ctx = self.context.borrow_mut();
            if ctx.config.theme == resolved {
                return;
            }
            ctx.config.theme = resolved.to_string();
            let config_path = ctx.storage.config_file_path();
            if let Err(error) = ctx.config.save_to_file(&config_path) {
                eprintln!("Failed to persist the interface theme: {error}");
            }
            ctx.windows.values().cloned().collect()
        };

        for window in windows {
            window.set_theme(resolved);
        }
    }

    /// The shared Desktop/Overlay switch, reached from the note menu, the
    /// keyboard shortcut and `note-it toggle` alike.
    pub fn toggle_layer_mode(&self) {
        self.toggle_layer_mode_from("webview");
    }

    fn toggle_layer_mode_from(&self, source: &str) {
        let trace = LayerToggleTrace::begin(source);
        // Toggling from a summoned note starts at the layer it is really on,
        // so the first press sends it back to the desktop as expected.
        let current = self.effective_layer_mode();
        let next = current.toggled();
        trace.phase(
            "T1",
            format_args!("current={} target={}", current.as_str(), next.as_str()),
        );
        self.set_layer_mode_traced(next, Some(trace));
    }

    pub fn set_layer_mode(&self, mode: LayerMode) {
        self.set_layer_mode_traced(mode, None);
    }

    fn set_layer_mode_traced(&self, mode: LayerMode, trace: Option<LayerToggleTrace>) {
        if mode == LayerMode::Hidden {
            if let Err(error) = self.begin_lifecycle(LifecycleOperation::Hide) {
                eprintln!("Hide operation rejected: {error}");
                return;
            }
            let self_clone = self.clone();
            self.flush_all_windows(move |result| match result {
                Ok(()) => {
                    let mut ctx = self_clone.context.borrow_mut();
                    let AppContext {
                        storage,
                        state,
                        windows,
                        lifecycle,
                        summon_restore,
                        layer_state_persistence,
                        ..
                    } = &mut *ctx;
                    let state_path = storage.state_file_path();
                    *summon_restore = None;
                    layer_state_persistence.cancel();
                    let commit_result = commit_hidden_transition(
                        state,
                        |next_state| next_state.save_to_file(&state_path),
                        || {
                            for window in windows.values() {
                                window.close_after_save();
                            }
                            windows.clear();
                        },
                    );
                    if let Err(error) = commit_result {
                        eprintln!("Hide operation aborted before closing windows: {error}");
                    }
                    lifecycle.finish(LifecycleOperation::Hide);
                }
                Err(error) => {
                    eprintln!(
                        "Hide operation aborted because one or more notes failed to save: {error}"
                    );
                    self_clone.finish_lifecycle(LifecycleOperation::Hide);
                }
            });
            return;
        }

        if self.context.borrow().lifecycle.is_active() {
            eprintln!("Layer mode change rejected while a lifecycle operation is in progress");
            return;
        }

        {
            let ctx = self.context.borrow();
            if is_live_layer_noop(
                ctx.state.active_layer_mode,
                mode,
                !ctx.windows.is_empty(),
                ctx.summon_restore,
            ) {
                diagnostics::log(format_args!(
                    "event=shared-layer-noop target={} windows={}",
                    mode.as_str(),
                    ctx.windows.len()
                ));
                return;
            }
        }

        let needs_instantiation = {
            if let Some(trace) = trace {
                trace.phase("T2", format_args!("target={}", mode.as_str()));
            }
            let mut ctx = self.context.borrow_mut();
            // An explicit layer choice replaces any temporary elevation.
            ctx.summon_restore = None;
            ctx.state.active_layer_mode = mode;
            if ctx.windows.is_empty() {
                true
            } else {
                let changed = apply_shared_layer_transition(ctx.windows.values(), |window| {
                    window.set_layer_mode(mode)
                });
                diagnostics::log(format_args!(
                    "event=shared-layer-transition target={} windows={} changed={}",
                    mode.as_str(),
                    ctx.windows.len(),
                    changed
                ));
                false
            }
        };

        if needs_instantiation {
            let plan = {
                let ctx = self.context.borrow();
                let ids_by_recency = ctx.storage.list_notes_by_recency().unwrap_or_default();
                plan_startup(false, ids_by_recency, &ctx.state)
            };
            match plan {
                StartupPlan::Background => {}
                StartupPlan::CreateNew => self.create_new_note(),
                StartupPlan::Restore { note_ids, .. } => {
                    for id in &note_ids {
                        self.instantiate_note_by_id(*id, mode);
                    }
                    self.mark_notes_open(&note_ids, mode);
                }
            }
        }

        if let Some(trace) = trace {
            trace.phase("T3", format_args!("target={}", mode.as_str()));
        }

        self.schedule_layer_state_persistence(trace);
    }

    pub fn save_and_quit(&self) {
        if let Err(error) = self.begin_lifecycle(LifecycleOperation::Quit) {
            eprintln!("Quit operation rejected: {error}");
            return;
        }
        let self_clone = self.clone();
        self.flush_all_windows(move |result| match result {
            Ok(()) => {
                if let Err(error) = commit_quit(
                    || {
                        let mut ctx = self_clone.context.borrow_mut();
                        persist_state_now(&mut ctx, "quit")
                    },
                    || self_clone.app.quit(),
                ) {
                    eprintln!(
                        "Quit cancelled because application state could not be saved: {error}"
                    );
                    self_clone.finish_lifecycle(LifecycleOperation::Quit);
                }
            }
            Err(error) => {
                eprintln!("Quit cancelled because one or more notes could not be saved: {error}");
                self_clone.finish_lifecycle(LifecycleOperation::Quit);
            }
        });
    }

    fn begin_lifecycle(&self, operation: LifecycleOperation) -> Result<(), String> {
        self.context.borrow_mut().lifecycle.begin(operation)
    }

    fn finish_lifecycle(&self, operation: LifecycleOperation) {
        self.context.borrow_mut().lifecycle.finish(operation);
    }

    fn ensure_structural_action_allowed(&self, action: &str) -> Result<(), String> {
        self.context
            .borrow()
            .lifecycle
            .ensure_structural_action_allowed(action)
    }

    fn schedule_layer_state_persistence(&self, trace: Option<LayerToggleTrace>) {
        let generation = self.context.borrow_mut().layer_state_persistence.schedule();
        let context = Rc::clone(&self.context);
        glib::timeout_add_local_once(LAYER_PERSISTENCE_DEBOUNCE, move || {
            let mut ctx = context.borrow_mut();
            if !ctx.layer_state_persistence.settle(generation) {
                diagnostics::log(format_args!(
                    "event=state-write-coalesced generation={generation}"
                ));
                return;
            }

            if let Some(trace) = trace {
                trace.phase(
                    "T4",
                    format_args!(
                        "generation={} target={}",
                        generation,
                        ctx.state.active_layer_mode.as_str()
                    ),
                );
            }
            let result = ctx.state.save_to_file(&ctx.storage.state_file_path());
            if let Some(trace) = trace {
                trace.phase(
                    "T5",
                    format_args!(
                        "generation={} target={} ok={}",
                        generation,
                        ctx.state.active_layer_mode.as_str(),
                        result.is_ok()
                    ),
                );
            }
            if let Err(error) = result {
                eprintln!("Failed to persist layer mode: {error}");
            }
        });
    }

    /// Records restored notes as open and persists the result, so a note
    /// brought back after being closed does not stay marked as closed.
    fn mark_notes_open(&self, note_ids: &[Uuid], mode: LayerMode) {
        let mut ctx = self.context.borrow_mut();
        ctx.state.active_layer_mode = mode;
        for id in note_ids {
            ctx.state.notes.entry(*id).or_default().is_open = true;
        }
        if let Err(error) = persist_state_now(&mut ctx, "restore-notes") {
            eprintln!("Failed to persist restored notes: {error}");
        }
    }

    /// Answers a search, and writes nothing at all.
    ///
    /// The note bodies come off disk in the store's own recency order and go
    /// straight into [`crate::search`]. No window is created, no timestamp
    /// moves and no file is opened for writing — a thousand notes are searched
    /// with zero additional WebViews, because a WebView is how a note is
    /// *edited* and nobody is editing.
    ///
    /// A query is asked of **every** note. The two paths differ only in how
    /// much has to be read: a listing shows at most
    /// [`search::MAX_RESULTS`](crate::search::MAX_RESULTS) notes, so reading
    /// past that would answer no question, while a search cannot know which
    /// note holds the word until it has looked.
    ///
    /// An empty query is not an empty answer: it lists the most recent notes,
    /// which is what makes the same control a way to move between them.
    pub fn answer_search(&self, requester: Uuid, request_id: u64, query: &str) {
        let ctx = self.context.borrow();
        let listing = query.trim().is_empty();

        let bodies = if listing {
            ctx.storage.read_recent_note_bodies(search::MAX_RESULTS)
        } else {
            ctx.storage.read_note_bodies_by_recency()
        };
        let notes = bodies.iter().map(|(id, body)| (*id, body.as_str()));
        let results = if listing {
            search::recent_notes(notes)
        } else {
            search::search_notes(query, notes)
        };

        if let Some(window) = ctx.windows.get(&requester) {
            window.send_search_results(request_id, results);
        }
    }

    /// Brings the note a reader chose in the search palette to the front.
    ///
    /// Three cases, and the difference between them is only how much has to
    /// happen first: an open note is presented, a collapsed one is expanded
    /// before that, and a closed one is instantiated through exactly the path
    /// a restore uses. In all three the note is then told what to look for so
    /// the editor can reveal it.
    ///
    /// The shared layer is not touched. Opening a note is not a reason to
    /// restack every other one, and Phase 3.5R.1 established where layer
    /// changes are decided; this is not that place.
    ///
    /// Nothing here writes to the note. A note that is opened becomes open in
    /// `state.json`, because it *is* open — that is window state, not content,
    /// and `updated_at` is untouched either way.
    pub fn open_search_result(&self, requester: Uuid, target: Uuid, query: String) {
        // The file may have been removed between the search and the choice.
        let exists = {
            let ctx = self.context.borrow();
            ctx.storage.note_path(&target).is_file()
        };
        if !exists {
            let ctx = self.context.borrow();
            if let Some(window) = ctx.windows.get(&requester) {
                window.report_missing_note(target);
            }
            return;
        }

        let mode = self.effective_layer_mode();
        let already_open = self.context.borrow().windows.contains_key(&target);
        if !already_open {
            self.instantiate_note_by_id(target, mode);
            if self.context.borrow().windows.contains_key(&target) {
                self.mark_notes_open(&[target], mode);
            } else {
                let ctx = self.context.borrow();
                if let Some(window) = ctx.windows.get(&requester) {
                    window.report_missing_note(target);
                }
                return;
            }
        }

        // Expanding is a geometry change like any other, so it goes through
        // the window's own collapse path and is persisted the same way.
        let expanded = {
            let ctx = self.context.borrow();
            ctx.windows
                .get(&target)
                .filter(|window| window.is_collapsed())
                .and_then(|window| window.set_collapsed(false))
        };
        if let Some(geometry) = expanded {
            self.update_geometry(target, geometry);
        }

        let ctx = self.context.borrow();
        if let Some(window) = ctx.windows.get(&target) {
            window.reveal();
            if !query.trim().is_empty() {
                window.reveal_match(query);
            }
        }
    }

    fn instantiate_note_by_id(&self, id: Uuid, mode: LayerMode) {
        if self.context.borrow().windows.contains_key(&id) {
            return;
        }

        let (doc_res, win_state) = {
            let ctx = self.context.borrow();
            let doc = ctx.storage.load_note(&id);
            let win_state = ctx
                .state
                .notes
                .get(&id)
                .cloned()
                .unwrap_or_else(NoteWindowState::default);
            (doc, win_state)
        };

        match doc_res {
            Ok(doc) => {
                let note_window = {
                    let ctx = self.context.borrow();
                    instantiate_note_window(&self.app, &ctx, self.clone(), doc, win_state, mode)
                };

                let mut ctx = self.context.borrow_mut();
                ctx.windows.insert(id, note_window);
            }
            Err(error) => eprintln!("Failed to load note {id}: {error}"),
        }
    }
}

fn persist_state_now(ctx: &mut AppContext, reason: &str) -> Result<(), String> {
    ctx.layer_state_persistence.cancel();
    diagnostics::log(format_args!("event=state-persist-now reason={reason}"));
    ctx.state.save_to_file(&ctx.storage.state_file_path())
}

fn apply_shared_layer_transition<T>(
    surfaces: impl IntoIterator<Item = T>,
    mut apply: impl FnMut(T) -> bool,
) -> usize {
    surfaces
        .into_iter()
        .map(|surface| usize::from(apply(surface)))
        .sum()
}

fn commit_hidden_transition<P, C>(
    current_state: &mut AppState,
    persist: P,
    close_windows: C,
) -> Result<(), String>
where
    P: FnOnce(&AppState) -> Result<(), String>,
    C: FnOnce(),
{
    let mut next_state = current_state.clone();
    next_state.active_layer_mode = LayerMode::Hidden;
    persist(&next_state)?;
    close_windows();
    *current_state = next_state;
    Ok(())
}

fn commit_quit<P, Q>(persist: P, quit: Q) -> Result<(), String>
where
    P: FnOnce() -> Result<(), String>,
    Q: FnOnce(),
{
    persist()?;
    quit();
    Ok(())
}

fn note_ids_to_restore(all_ids: &[Uuid], state: &AppState) -> Vec<Uuid> {
    all_ids
        .iter()
        .copied()
        .filter(|id| state.notes.get(id).map(|note| note.is_open).unwrap_or(true))
        .collect()
}

/// Decides what a summon should put on screen.
///
/// `ids_by_recency` must be ordered most recently saved first. Closing the last
/// note leaves it on disk with `is_open = false`; without the fallback below
/// there would be no way back to it, and a summon would answer with a blank
/// note instead of the note that was just closed.
fn plan_startup(is_background: bool, ids_by_recency: Vec<Uuid>, state: &AppState) -> StartupPlan {
    if is_background {
        return StartupPlan::Background;
    }

    let mode = if state.active_layer_mode == LayerMode::Hidden {
        LayerMode::Overlay
    } else {
        state.active_layer_mode
    };

    let open_ids = note_ids_to_restore(&ids_by_recency, state);
    if !open_ids.is_empty() {
        return StartupPlan::Restore {
            note_ids: open_ids,
            mode,
        };
    }

    // Everything is closed: bring back the note that was used last.
    match ids_by_recency.first() {
        Some(most_recent) => StartupPlan::Restore {
            note_ids: vec![*most_recent],
            mode,
        },
        None => StartupPlan::CreateNew,
    }
}

/// The layer the surfaces are actually on.
///
/// A summon elevates the notes to the overlay without overwriting the stored
/// preference, so while `summon_restore` is set the preference is behind the
/// live layer. Anything that has to agree with what is on screen — toggling,
/// and opening a new note beside the others — must ask this rather than read
/// the preference directly.
fn effective_layer_mode(active: LayerMode, summon_restore: Option<LayerMode>) -> LayerMode {
    if summon_restore.is_some() {
        LayerMode::Overlay
    } else {
        active
    }
}

fn prepare_new_note(
    config: &AppConfig,
    active_mode: LayerMode,
    window_count: usize,
    monitor_name: Option<String>,
    monitor_width: i32,
    monitor_height: i32,
) -> (NoteDocument, NoteWindowState, LayerMode) {
    let mut document = NoteDocument::new_empty();
    document.metadata.color = config.default_color.clone();
    document.metadata.font_size = config.default_font_size;

    let (x, y) = calculate_cascade_position(
        window_count,
        monitor_width,
        monitor_height,
        config.default_width,
        config.default_height,
    );
    let state = NoteWindowState {
        x,
        y,
        width: config.default_width,
        height: config.default_height,
        is_open: true,
        monitor: monitor_name,
        ..NoteWindowState::default()
    };
    let mode = if active_mode == LayerMode::Hidden {
        LayerMode::Overlay
    } else {
        active_mode
    };

    (document, state, mode)
}

fn instantiate_note_window(
    app: &gtk4::Application,
    ctx: &AppContext,
    app_controller: NoteItAppClone,
    doc: NoteDocument,
    win_state: NoteWindowState,
    layer_mode: LayerMode,
) -> NoteWindow {
    let display = gtk4::gdk::Display::default();
    let (monitor, monitor_name, mon_w, mon_h) =
        find_monitor_by_connector(display.as_ref(), win_state.monitor.as_deref());

    let app_clone1 = app_controller.clone();
    let on_new_note = Rc::new(move || {
        app_clone1.create_new_note();
    });

    let app_clone2 = app_controller.clone();
    let on_close = Rc::new(move |id| app_clone2.close_note(&id));

    let app_clone3 = app_controller.clone();
    let on_geometry_changed = Rc::new(move |id, geom| {
        app_clone3.update_geometry(id, geom);
    });

    let app_clone4 = app_controller.clone();
    let on_toggle_layer_mode = Rc::new(move || {
        app_clone4.toggle_layer_mode();
    });

    let app_clone5 = app_controller.clone();
    let on_theme_changed = Rc::new(move |theme: String| {
        app_clone5.set_theme(&theme);
    });

    let app_clone6 = app_controller.clone();
    let on_search = Rc::new(move |requester, request_id, query: String| {
        app_clone6.answer_search(requester, request_id, &query);
    });

    let app_clone7 = app_controller.clone();
    let on_open_search_result = Rc::new(move |requester, target, query: String| {
        app_clone7.open_search_result(requester, target, query);
    });

    NoteWindow::new(NoteWindowOptions {
        app,
        document: doc,
        state: win_state,
        layer_mode,
        storage: ctx.storage.clone(),
        ui_dist_path: &ctx.ui_dist_path,
        monitor,
        monitor_name,
        monitor_width: if mon_w > 0 {
            mon_w
        } else {
            DEFAULT_MONITOR_WIDTH
        },
        monitor_height: if mon_h > 0 {
            mon_h
        } else {
            DEFAULT_MONITOR_HEIGHT
        },
        on_new_note,
        on_close,
        on_geometry_changed,
        on_toggle_layer_mode,
        theme: ctx.config.theme.clone(),
        on_theme_changed,
        on_search,
        on_open_search_result,
    })
}

fn find_ui_dist_path() -> PathBuf {
    // 1. Current directory ./ui/dist
    let local = PathBuf::from("ui/dist");
    if local.exists() {
        return local;
    }

    // 2. Relative to current exe
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let rel = parent.join("ui/dist");
            if rel.exists() {
                return rel;
            }
            let rel_share = parent.join("../share/note-it/ui/dist");
            if rel_share.exists() {
                return rel_share;
            }
        }
    }

    // 3. System share directory
    let system = PathBuf::from("/usr/share/note-it/ui/dist");
    if system.exists() {
        return system;
    }

    local
}

#[cfg(test)]
mod tests {
    use super::{
        apply_shared_layer_transition, commit_hidden_transition, commit_quit, effective_layer_mode,
        is_live_layer_noop, plan_startup, plan_summon_layer, preferred_layer_after_new_note,
        prepare_new_note, FlushBatch, LifecycleCoordinator, LifecycleOperation, StartupPlan,
        StatePersistenceDebouncer, SummonLayerPlan,
    };
    use crate::settings::AppConfig;
    use crate::state::{AppState, LayerMode};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use uuid::Uuid;

    #[test]
    fn multiple_windows_require_all_confirmations() {
        let completion = Rc::new(RefCell::new(None));
        let completion_clone = Rc::clone(&completion);
        let mut batch = FlushBatch::new(
            3,
            Box::new(move |result| *completion_clone.borrow_mut() = Some(result)),
        );

        assert!(batch.record(Ok(())).is_none());
        assert!(completion.borrow().is_none());
        assert!(batch.record(Ok(())).is_none());
        assert!(completion.borrow().is_none());

        let (callback, result) = batch.record(Ok(())).expect("third confirmation completes");
        callback(result);
        assert!(completion.borrow().as_ref().expect("completion").is_ok());
    }

    #[test]
    fn one_global_layer_decision_reaches_each_surface_exactly_once() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let calls_clone = Rc::clone(&calls);
        let changed = apply_shared_layer_transition([1, 2, 3, 4], move |surface| {
            calls_clone.borrow_mut().push(surface);
            true
        });

        assert_eq!(changed, 4);
        assert_eq!(&*calls.borrow(), &[1, 2, 3, 4]);
    }

    #[test]
    fn twenty_rapid_layer_changes_coalesce_to_the_final_state() {
        let mut debouncer = StatePersistenceDebouncer::default();
        let mut mode = LayerMode::Overlay;
        let mut generations = Vec::new();

        for _ in 0..20 {
            mode = mode.toggled();
            generations.push(debouncer.schedule());
        }

        let mut written = Vec::new();
        for generation in generations {
            if debouncer.settle(generation) {
                written.push(mode);
            }
        }

        assert_eq!(
            mode,
            LayerMode::Overlay,
            "an even count returns to the start"
        );
        assert_eq!(written, vec![LayerMode::Overlay]);
    }

    #[test]
    fn odd_and_even_toggle_counts_have_deterministic_parity() {
        let initial = LayerMode::Desktop;
        let after_nineteen = (0..19).fold(initial, |mode, _| mode.toggled());
        let after_twenty = (0..20).fold(initial, |mode, _| mode.toggled());

        assert_eq!(after_nineteen, LayerMode::Overlay);
        assert_eq!(after_twenty, initial);
    }

    #[test]
    fn an_explicit_unchanged_layer_skips_the_shared_transition() {
        assert!(is_live_layer_noop(
            LayerMode::Desktop,
            LayerMode::Desktop,
            true,
            None
        ));
        assert!(!is_live_layer_noop(
            LayerMode::Desktop,
            LayerMode::Overlay,
            true,
            None
        ));
        assert!(!is_live_layer_noop(
            LayerMode::Desktop,
            LayerMode::Desktop,
            false,
            None
        ));
        assert!(!is_live_layer_noop(
            LayerMode::Desktop,
            LayerMode::Desktop,
            true,
            Some(LayerMode::Desktop)
        ));
    }

    #[test]
    fn an_immediate_lifecycle_save_supersedes_a_pending_layer_write() {
        let mut debouncer = StatePersistenceDebouncer::default();
        let pending = debouncer.schedule();
        debouncer.cancel();

        assert!(!debouncer.settle(pending));
    }

    #[test]
    fn one_window_failure_aborts_global_hide() {
        let windows_destroyed = Rc::new(Cell::new(false));
        let destroyed_clone = Rc::clone(&windows_destroyed);
        let mut batch = FlushBatch::new(
            3,
            Box::new(move |result| {
                if result.is_ok() {
                    destroyed_clone.set(true);
                }
            }),
        );

        assert!(batch.record(Ok(())).is_none());
        assert!(batch
            .record(Err("one note failed to save".to_string()))
            .is_none());
        let (callback, result) = batch.record(Ok(())).expect("all replies received");
        assert!(result.is_err());
        callback(result);
        assert!(!windows_destroyed.get());
    }

    #[test]
    fn quit_failure_keeps_application_running() {
        let quit_called = Cell::new(false);
        let result = commit_quit(
            || Err("state persistence failed".to_string()),
            || quit_called.set(true),
        );

        assert!(result.is_err());
        assert!(!quit_called.get());
    }

    #[test]
    fn state_persistence_failure_aborts_hide_before_window_destruction() {
        let mut state = AppState {
            active_layer_mode: LayerMode::Overlay,
            ..AppState::default()
        };
        let windows_destroyed = Cell::new(false);

        let result = commit_hidden_transition(
            &mut state,
            |_| Err("state persistence failed".to_string()),
            || windows_destroyed.set(true),
        );

        assert!(result.is_err());
        assert_eq!(state.active_layer_mode, LayerMode::Overlay);
        assert!(!windows_destroyed.get());
    }

    #[test]
    fn concurrent_lifecycle_operation_is_handled_safely() {
        let mut coordinator = LifecycleCoordinator::default();
        assert!(coordinator.begin(LifecycleOperation::Hide).is_ok());
        assert!(coordinator.begin(LifecycleOperation::Hide).is_err());
        assert!(coordinator.begin(LifecycleOperation::Quit).is_err());
        assert_eq!(coordinator.active, Some(LifecycleOperation::Hide));

        coordinator.finish(LifecycleOperation::Hide);
        assert!(coordinator.begin(LifecycleOperation::Quit).is_ok());
    }

    #[test]
    fn background_continues_creating_zero_webviews() {
        let state = AppState::default();
        assert_eq!(
            plan_startup(true, vec![Uuid::new_v4()], &state),
            StartupPlan::Background
        );
    }

    #[test]
    fn normal_startup_restores_only_open_notes() {
        let open_id = Uuid::new_v4();
        let closed_id = Uuid::new_v4();
        let mut state = AppState {
            active_layer_mode: LayerMode::Hidden,
            ..AppState::default()
        };
        state.notes.entry(open_id).or_default().is_open = true;
        state.notes.entry(closed_id).or_default().is_open = false;

        assert_eq!(
            plan_startup(false, vec![open_id, closed_id], &state),
            StartupPlan::Restore {
                note_ids: vec![open_id],
                mode: LayerMode::Overlay,
            }
        );
    }

    #[test]
    fn a_summon_reopens_the_last_note_instead_of_creating_a_blank_one() {
        // The note was closed with the X: still on disk, marked closed.
        let closed_id = Uuid::new_v4();
        let mut state = AppState::default();
        state.notes.entry(closed_id).or_default().is_open = false;

        assert_eq!(
            plan_startup(false, vec![closed_id], &state),
            StartupPlan::Restore {
                note_ids: vec![closed_id],
                mode: LayerMode::Overlay,
            }
        );
    }

    #[test]
    fn a_summon_brings_back_the_most_recently_saved_of_several_closed_notes() {
        let newest = Uuid::new_v4();
        let middle = Uuid::new_v4();
        let oldest = Uuid::new_v4();
        let mut state = AppState::default();
        for id in [newest, middle, oldest] {
            state.notes.entry(id).or_default().is_open = false;
        }

        // Only the most recent one comes back; the others stay closed.
        assert_eq!(
            plan_startup(false, vec![newest, middle, oldest], &state),
            StartupPlan::Restore {
                note_ids: vec![newest],
                mode: LayerMode::Overlay,
            }
        );
    }

    #[test]
    fn a_summon_creates_a_note_only_when_none_exist_at_all() {
        let state = AppState::default();
        assert_eq!(
            plan_startup(false, Vec::new(), &state),
            StartupPlan::CreateNew
        );
    }

    #[test]
    fn a_summon_leaves_already_open_notes_alone() {
        let open_id = Uuid::new_v4();
        let closed_id = Uuid::new_v4();
        let mut state = AppState::default();
        state.notes.entry(open_id).or_default().is_open = true;
        state.notes.entry(closed_id).or_default().is_open = false;

        // A closed note is not resurrected while something is still open.
        assert_eq!(
            plan_startup(false, vec![closed_id, open_id], &state),
            StartupPlan::Restore {
                note_ids: vec![open_id],
                mode: LayerMode::Overlay,
            }
        );
    }

    #[test]
    fn a_summon_keeps_the_desktop_layer_preference() {
        let closed_id = Uuid::new_v4();
        let mut state = AppState {
            active_layer_mode: LayerMode::Desktop,
            ..AppState::default()
        };
        state.notes.entry(closed_id).or_default().is_open = false;

        // Restoring must not silently promote the note to the overlay.
        assert_eq!(
            plan_startup(false, vec![closed_id], &state),
            StartupPlan::Restore {
                note_ids: vec![closed_id],
                mode: LayerMode::Desktop,
            }
        );
    }

    #[test]
    fn background_startup_still_creates_nothing_even_with_closed_notes() {
        let closed_id = Uuid::new_v4();
        let mut state = AppState::default();
        state.notes.entry(closed_id).or_default().is_open = false;

        assert_eq!(
            plan_startup(true, vec![closed_id], &state),
            StartupPlan::Background
        );
    }

    #[test]
    fn a_note_created_after_a_summon_opens_beside_the_others() {
        // 3.5R. A summon lifts the notes to the overlay and deliberately keeps
        // the stored preference as it was, so while the elevation is in effect
        // the preference reads "desktop" while every surface is on the
        // overlay. Creating a note from the preference filed it on the bottom
        // layer, behind every window — invisible, moments after the user asked
        // for Note-it. It has to join the layer its siblings are on.
        assert_eq!(
            effective_layer_mode(LayerMode::Desktop, Some(LayerMode::Desktop)),
            LayerMode::Overlay
        );

        let config = AppConfig::default();
        let (_, _, mode) = prepare_new_note(
            &config,
            effective_layer_mode(LayerMode::Desktop, Some(LayerMode::Desktop)),
            0,
            None,
            1920,
            1080,
        );
        assert_eq!(mode, LayerMode::Overlay);
        assert_eq!(
            preferred_layer_after_new_note(
                LayerMode::Desktop,
                LayerMode::Overlay,
                Some(LayerMode::Desktop)
            ),
            LayerMode::Desktop,
            "a new overlay surface must not replace the summoned preference"
        );

        // With no elevation in effect the stored preference is the truth.
        for stored in [LayerMode::Desktop, LayerMode::Overlay] {
            assert_eq!(effective_layer_mode(stored, None), stored);
            assert_eq!(preferred_layer_after_new_note(stored, stored, None), stored);
        }
        // Hidden is never a layer to open a note on.
        let (_, _, from_hidden) = prepare_new_note(
            &config,
            effective_layer_mode(LayerMode::Hidden, None),
            0,
            None,
            1920,
            1080,
        );
        assert_eq!(from_hidden, LayerMode::Overlay);
    }

    #[test]
    fn normal_note_creation_uses_defaults_and_unique_ids() {
        let config = AppConfig {
            default_color: "blue".to_string(),
            default_font_size: 18,
            default_width: 380,
            default_height: 280,
            ..AppConfig::default()
        };
        let (first, first_state, first_mode) = prepare_new_note(
            &config,
            LayerMode::Hidden,
            0,
            Some("eDP-1".to_string()),
            1920,
            1080,
        );
        let (second, second_state, _) = prepare_new_note(
            &config,
            LayerMode::Overlay,
            1,
            Some("eDP-1".to_string()),
            1920,
            1080,
        );

        assert_ne!(first.metadata.id, second.metadata.id);
        assert_eq!(first.metadata.color, "blue");
        assert_eq!(first.metadata.font_size, 18);
        assert_eq!(first_state.width, 380);
        assert_eq!(first_state.height, 280);
        assert!(first_state.is_open);
        assert_eq!(first_state.monitor.as_deref(), Some("eDP-1"));
        assert_ne!(
            (first_state.x, first_state.y),
            (second_state.x, second_state.y)
        );
        assert_eq!(first_mode, LayerMode::Overlay);
    }

    #[test]
    fn note_creation_during_lifecycle_is_rejected() {
        let mut coordinator = LifecycleCoordinator::default();
        coordinator
            .begin(LifecycleOperation::Hide)
            .expect("start hide");

        let error = coordinator
            .ensure_structural_action_allowed("new note creation")
            .expect_err("creation must be rejected");
        assert!(error.contains("Hide"));
        assert_eq!(coordinator.active, Some(LifecycleOperation::Hide));
    }

    #[test]
    fn summoning_a_desktop_note_elevates_it_without_losing_the_preference() {
        // A Bottom surface is always under ordinary windows, so a summon has
        // to elevate; the preference is remembered rather than replaced.
        assert_eq!(
            plan_summon_layer(LayerMode::Desktop, true),
            SummonLayerPlan::Elevate {
                restore: LayerMode::Desktop
            }
        );
    }

    #[test]
    fn launching_the_application_is_not_a_summon() {
        // Starting Note-it honours the stored preference instead of pulling
        // the note to the front.
        assert_eq!(
            plan_summon_layer(LayerMode::Desktop, false),
            SummonLayerPlan::Persisted(LayerMode::Desktop)
        );
    }

    #[test]
    fn summoning_an_overlay_note_changes_no_layer_state() {
        for already_running in [false, true] {
            assert_eq!(
                plan_summon_layer(LayerMode::Overlay, already_running),
                SummonLayerPlan::Persisted(LayerMode::Overlay)
            );
        }
    }

    #[test]
    fn summoning_a_hidden_application_brings_it_back_as_an_overlay() {
        for already_running in [false, true] {
            assert_eq!(
                plan_summon_layer(LayerMode::Hidden, already_running),
                SummonLayerPlan::Persisted(LayerMode::Overlay)
            );
        }
    }
}
