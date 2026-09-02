use crate::cli::CliCommand;
use crate::layer_shell::{
    calculate_cascade_position, find_monitor_by_connector, install_paper_color_styles,
    DEFAULT_MONITOR_HEIGHT, DEFAULT_MONITOR_WIDTH,
};
use crate::note_window::{NoteWindow, NoteWindowOptions};
use crate::webview_bridge::StudyCatalogNote;
use crate::write_authority::StartupRefusal;
use gio::prelude::*;
use gtk4::gdk;
use gtk4::prelude::*;
use noteit_core::assets::{
    decode_base64, import_image, parse_asset_request, ImportError, ASSET_SCHEME,
};
use noteit_core::autopaste::{
    delimiter_from_name, delimiter_name, is_capturable, AutoPaste, CaptureDelimiter,
    CaptureSession, ChangeDecision, IgnoreReason,
};
use noteit_core::diagnostics::{self, LayerToggleTrace};
use noteit_core::model::NoteDocument;
use noteit_core::search::resolve_search_answer;
use noteit_core::settings::{
    clamp_ui_scale_percent, resolve_startup_config, theme_name, AppConfig,
};
use noteit_core::state::{
    next_collapse_all, resolve_startup_state, AppState, LayerMode, NoteWindowState,
};
use noteit_core::study::Rating;
use noteit_core::timer::TimerFinishKind;
use noteit_core::NoteItCore;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use uuid::Uuid;

static NEXT_FLUSH_ID: AtomicU64 = AtomicU64::new(1);

/// What the reader is told when a note could not be moved to the trash.
///
/// One sentence for every reason, because every reason has the same
/// consequence and the reader needs to know that one: the note is still here.
/// The detail goes to `stderr`, where a diagnosis belongs.
const TRASH_FAILED_MESSAGE: &str =
    "Não foi possível mover a nota para a lixeira. A nota continua aberta e salva.";
const LAYER_PERSISTENCE_DEBOUNCE: Duration = Duration::from_millis(180);

type LifecycleCallback = Box<dyn FnOnce(Result<(), String>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleOperation {
    Hide,
    Quit,
}

/// What may happen at once, and what may not.
///
/// Hiding, quitting, deleting a note and an external write all reach into the
/// same windows and the same files, and two of them overlapping is how a note
/// gets written by one and destroyed by another. There is one coordinator and
/// everything structural asks it first.
///
/// The rule is symmetric and deliberately small:
///
/// - a lifecycle operation refuses to start while an external write is in
///   flight, so the window holding the editor still is not destroyed
///   underneath it;
/// - an external write refuses to start while a lifecycle operation is in
///   flight, and the writer that asked is told the store is busy — which is
///   true, and which it can act on.
#[derive(Debug, Default)]
struct LifecycleCoordinator {
    active: Option<LifecycleOperation>,
    /// External writes currently holding a note's editor still.
    ///
    /// A count rather than a flag so nothing can clear someone else's claim,
    /// though the control server serialises requests and it is never above
    /// one in practice.
    external_writes: usize,
}

impl LifecycleCoordinator {
    fn begin(&mut self, operation: LifecycleOperation) -> Result<(), String> {
        if let Some(active) = self.active {
            return Err(format!(
                "lifecycle operation {active:?} is already in progress"
            ));
        }
        if self.external_writes > 0 {
            return Err("an external write is in progress".to_string());
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
        self.active.is_some() || self.external_writes > 0
    }

    fn begin_external_write(&mut self) -> Result<(), String> {
        if let Some(active) = self.active {
            return Err(format!(
                "o Note-it está ocupado ({active:?}) e nada foi alterado"
            ));
        }
        self.external_writes += 1;
        Ok(())
    }

    fn finish_external_write(&mut self) {
        self.external_writes = self.external_writes.saturating_sub(1);
    }

    fn ensure_structural_action_allowed(&self, action: &str) -> Result<(), String> {
        if let Some(active) = self.active {
            return Err(format!(
                "{action} is unavailable while lifecycle operation {active:?} is in progress"
            ));
        }
        if self.external_writes > 0 {
            return Err(format!(
                "{action} is unavailable while an external write is in progress"
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
    pub core: NoteItCore,
    pub config: AppConfig,
    pub state: AppState,
    pub can_persist_config: bool,
    pub can_persist_state: bool,
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
    /// Which note, if any, is capturing the clipboard, and under which
    /// generation. Never written to disk: see ADR-031.
    autopaste: AutoPaste,
    /// The live `changed` connection, present only while capturing.
    ///
    /// Held so it can be *disconnected* rather than merely ignored. "Off"
    /// having no listener at all is the difference between a promise and a
    /// property: while AutoPaste is off there is nothing subscribed to the
    /// clipboard, so nothing can observe it however the rest of the code
    /// behaves.
    clipboard_watch: Option<gdk::glib::SignalHandlerId>,
    /// The writer lease and the control socket, held for the whole session.
    ///
    /// Held **by value**, and that is the point. A Note-it instance that can
    /// edit and save while something else owns the store would be a second
    /// writer, which is precisely what this phase exists to make impossible —
    /// so it is not a state this program can describe. `AppContext` cannot be
    /// built without an authority, the only way to get one is a complete
    /// [`crate::write_authority::claim`], and startup refuses rather than
    /// carrying on without it.
    ///
    /// Never read, like `_hold_guard`: what it does, it does by existing. It
    /// holds the lease for exactly as long as this context does, and releases
    /// it when the process ends.
    _write_authority: crate::write_authority::WriteAuthority,
}

pub struct NoteItApp {
    app: gtk4::Application,
    context: Rc<RefCell<AppContext>>,
    _hold_guard: gio::ApplicationHoldGuard,
}

impl NoteItApp {
    /// Builds the application, or refuses to.
    ///
    /// The store is claimed *before* anything that could write to it exists.
    /// There is no window, no document, no autosave and no restored note until
    /// this has returned successfully, so a refusal cannot leave a half-started
    /// instance editing a store it does not own.
    pub fn new(app: &gtk4::Application) -> Result<Self, StartupRefusal> {
        let hold_guard = app.hold();

        diagnostics::log(format_args!(
            "event=startup layer_shell_protocol_version={}",
            gtk4_layer_shell::protocol_version()
        ));

        if let Some(display) = gtk4::gdk::Display::default() {
            install_paper_color_styles(&display);
        }

        let core = NoteItCore::new().expect("Failed to initialize XDG storage");

        // Before anything else that could write. Everything below — the config,
        // the state, the windows, the autosave — only comes into being once this
        // process is the store's one writer.
        let (write_authority, pending_requests) =
            crate::write_authority::claim(core.paths())?.split();

        let storage = core.storage();
        register_asset_scheme(storage.assets_dir().to_path_buf());
        let config_outcome = AppConfig::load_detailed(&storage.config_file_path());
        let startup_config = resolve_startup_config(config_outcome);
        if let Some(msg) = &startup_config.log_message {
            eprintln!("{msg}");
        }
        let config = startup_config.config;

        let state_outcome = AppState::load_detailed(&storage.state_file_path());
        let startup_state = resolve_startup_state(state_outcome);
        if let Some(msg) = &startup_state.log_message {
            eprintln!("{msg}");
        }
        let state = startup_state.state;
        let ui_dist_path = find_ui_dist_path();

        let context = Rc::new(RefCell::new(AppContext {
            core,
            config,
            state,
            can_persist_config: startup_config.can_persist,
            can_persist_state: startup_state.can_persist,
            windows: HashMap::new(),
            ui_dist_path,
            lifecycle: LifecycleCoordinator::default(),
            summon_restore: None,
            activated: false,
            layer_state_persistence: StatePersistenceDebouncer::default(),
            autopaste: AutoPaste::new(),
            clipboard_watch: None,
            _write_authority: write_authority,
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

        // The store is already claimed; this is only the point at which there
        // is an application to answer with.
        crate::write_authority::serve(
            NoteItAppClone {
                app: app.clone(),
                context: Rc::clone(&context),
            },
            pending_requests,
        );

        Ok(Self {
            app: app.clone(),
            context,
            _hold_guard: hold_guard,
        })
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
            let ids_by_recency = match ctx.core.list_notes() {
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
            if let Err(error) = ctx.core.storage().save_note_atomic(&doc) {
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
        // Before the window goes: a closed note is not a place a capture can
        // land, and nothing may keep watching the clipboard on its behalf.
        self.disarm_autopaste_for(*id, "close");
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
            let config_path = ctx.core.storage().config_file_path();
            if !ctx.can_persist_config {
                eprintln!("Aviso: gravação de tema ignorada para proteger arquivo de configuração original.");
            } else if let Err(error) = ctx.config.save_to_file(&config_path) {
                eprintln!("Failed to persist the interface theme: {error}");
            }
            ctx.windows.values().cloned().collect()
        };

        for window in windows {
            window.set_theme(resolved);
        }
    }

    /// Commits and broadcasts the one application-wide chrome scale.
    ///
    /// The config write is the commit point: no WebView or collapsed geometry
    /// moves until the complete next `config.toml` has replaced the old one.
    pub fn set_ui_scale(&self, requested: u16) {
        let resolved = clamp_ui_scale_percent(requested);
        let windows: Vec<NoteWindow> = {
            let mut ctx = self.context.borrow_mut();
            if ctx.config.ui_scale_percent == resolved {
                return;
            }
            let mut next = ctx.config.clone();
            next.ui_scale_percent = resolved;
            let config_path = ctx.core.storage().config_file_path();
            if !ctx.can_persist_config {
                eprintln!("Aviso: gravação de escala ignorada para proteger arquivo de configuração original.");
                return;
            }
            if let Err(error) = next.save_to_file(&config_path) {
                eprintln!("Failed to persist the interface scale: {error}");
                return;
            }
            ctx.config = next;
            ctx.windows.values().cloned().collect()
        };

        let mut collapsed_updates = Vec::new();
        for window in &windows {
            if let Some(snapshot) = window.set_ui_scale(resolved) {
                collapsed_updates.push((window.id, snapshot));
            }
        }
        if collapsed_updates.is_empty() {
            return;
        }
        let mut ctx = self.context.borrow_mut();
        for (id, snapshot) in collapsed_updates {
            ctx.state.notes.insert(id, snapshot);
        }
        if let Err(error) = persist_state_now(&mut ctx, "interface-scale") {
            eprintln!("Failed to persist scaled collapsed geometry: {error}");
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
            // Before the flush, not after it. Hiding destroys every WebView,
            // and a read still in the air must not reach a document that is
            // about to be written out and torn down.
            self.disarm_autopaste("hide");
            let self_clone = self.clone();
            self.flush_all_windows(move |result| match result {
                Ok(()) => {
                    let mut ctx = self_clone.context.borrow_mut();
                    let AppContext {
                        core,
                        state,
                        windows,
                        lifecycle,
                        summon_restore,
                        layer_state_persistence,
                        can_persist_state,
                        ..
                    } = &mut *ctx;
                    let state_path = core.storage().state_file_path();
                    let can_persist = *can_persist_state;
                    *summon_restore = None;
                    layer_state_persistence.cancel();
                    let commit_result = commit_hidden_transition(
                        state,
                        |next_state| {
                            if can_persist {
                                next_state.save_to_file(&state_path)
                            } else {
                                Ok(())
                            }
                        },
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
                let ids_by_recency = ctx.core.list_notes().unwrap_or_default();
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
        // Same order as hiding: stop capturing, then flush, then go.
        self.disarm_autopaste("quit");
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

    /// Claims the right to change a note from outside the interface.
    ///
    /// Refused while the application is hiding or quitting: those destroy
    /// WebViews, and a WebView destroyed in the middle of a barrier takes an
    /// unsaved paragraph with it. The writer that asked is told the store is
    /// busy and nothing is changed.
    pub fn begin_external_write(&self) -> Result<(), String> {
        self.context.borrow_mut().lifecycle.begin_external_write()
    }

    /// Releases that claim. Called on every path out of an external write —
    /// committed, refused, timed out — so a failure can never leave the
    /// application unable to hide or quit.
    pub fn finish_external_write(&self) {
        self.context.borrow_mut().lifecycle.finish_external_write();
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
            let result = ctx
                .state
                .save_to_file(&ctx.core.storage().state_file_path());
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
    /// straight into [`noteit_core::search`]. No window is created, no timestamp
    /// moves and no file is opened for writing — a thousand notes are searched
    /// with zero additional WebViews, because a WebView is how a note is
    /// *edited* and nobody is editing.
    ///
    /// A query is asked of **every** note. The two paths differ only in how
    /// much has to be read: a listing shows at most
    /// [`search::MAX_RESULTS`](noteit_core::search::MAX_RESULTS) notes, so reading
    /// past that would answer no question, while a search cannot know which
    /// note holds the word until it has looked.
    ///
    /// An empty query is not an empty answer: it lists the most recent notes,
    /// which is what makes the same control a way to move between them.
    pub fn answer_search(&self, requester: Uuid, request_id: u64, query: &str) {
        let ctx = self.context.borrow();
        // A scan that failed and a store with nothing matching both arrive as
        // an empty list; the notice is what tells the palette which happened.
        let answer = resolve_search_answer(ctx.core.search_notes(query));
        if let Some(notice) = &answer.notice {
            eprintln!("Busca: {notice}");
        }

        if let Some(window) = ctx.windows.get(&requester) {
            window.send_search_results(request_id, answer.results, answer.notice);
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
            ctx.core.storage().note_path(&target).is_file()
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

    /// Moves a note to the trash, in the one order that cannot lose text.
    ///
    /// Flush, move, state, surface — and every failure before the move leaves
    /// the note open, live and editable. A note whose latest text could not be
    /// written must never disappear from the screen as though it had been
    /// deleted: the reader would have been shown a deletion and charged an
    /// edit for it.
    ///
    /// The move is the commit point. Past it the note **is** in the trash, so
    /// nothing that fails afterwards is allowed to report otherwise: the
    /// window state is best-effort and the surface goes either way, because a
    /// window still showing a note whose file has moved is a window showing
    /// something that is not there.
    pub fn trash_note(&self, id: Uuid) {
        if let Err(error) = self.ensure_structural_action_allowed("moving a note to the trash") {
            eprintln!("Move to trash rejected: {error}");
            self.report_data_result(id, "trash", false, TRASH_FAILED_MESSAGE);
            return;
        }

        let window = self.context.borrow().windows.get(&id).cloned();
        let Some(window) = window else {
            eprintln!("Move to trash rejected: note {id} has no window");
            return;
        };

        // Before the flush that precedes the move. A note on its way to the
        // trash is not somewhere a capture may still land.
        self.disarm_autopaste_for(id, "trash");

        let request_id = NEXT_FLUSH_ID.fetch_add(1, Ordering::SeqCst);
        let app = self.clone();
        window.request_flush(request_id, move |flush| {
            let outcome = commit_trash(
                flush,
                || app.context.borrow().core.storage().move_note_to_trash(&id),
                || {
                    let mut ctx = app.context.borrow_mut();
                    // Closed rather than forgotten: the geometry stays, so a
                    // note that comes back out of the trash comes back the
                    // size and place it was.
                    ctx.state.notes.entry(id).or_default().is_open = false;
                    persist_state_now(&mut ctx, "trash-note")
                },
                || {
                    let window = app.context.borrow_mut().windows.remove(&id);
                    if let Some(window) = window {
                        window.close_after_save();
                    }
                },
            );

            if let Err(error) = outcome {
                eprintln!("Move to trash failed for note {id}: {error}");
                app.report_data_result(id, "trash", false, TRASH_FAILED_MESSAGE);
            }
        });
    }

    /// Answers a request for the contents of the trash. Reading only: no file
    /// is opened for writing, no timestamp moves and no window is created.
    pub fn answer_trash_list(&self, requester: Uuid, request_id: u64) {
        let ctx = self.context.borrow();
        let entries = ctx.core.list_trash();
        if let Some(window) = ctx.windows.get(&requester) {
            window.send_trash_entries(request_id, entries);
        }
    }

    /// Brings a note back out of the trash.
    ///
    /// Nothing is opened and nothing is parsed: the file moves back and
    /// becomes a note again, with the same identifier, the same bytes and the
    /// same `updated_at` — restoring is not editing. An identifier already
    /// taken by a live note is refused rather than overwritten, and the reader
    /// is told which of the two it was.
    pub fn restore_note(&self, requester: Uuid, target: Uuid) {
        let result = self
            .context
            .borrow()
            .core
            .storage()
            .restore_note_from_trash(&target);
        match result {
            Ok(()) => {
                diagnostics::log(format_args!("event=note-restored note={target}"));
                self.report_data_result(requester, "restore", true, "Nota restaurada.");
            }
            Err(error) => {
                eprintln!("Restore failed for note {target}: {error}");
                let message = match error {
                    noteit_core::trash::RestoreError::Occupied => {
                        "Já existe uma nota ativa com esse identificador. Nada foi alterado."
                    }
                    noteit_core::trash::RestoreError::Missing => {
                        "Essa nota não está mais na lixeira."
                    }
                    noteit_core::trash::RestoreError::Failed(_) => {
                        "Não foi possível restaurar a nota. Nada foi alterado."
                    }
                };
                self.report_data_result(requester, "restore", false, message);
            }
        }
    }

    /// Takes a snapshot because the reader asked for one, and says what
    /// happened. Unlike the automatic backup this is never silent: someone is
    /// waiting to know whether they have a safety point.
    pub fn create_backup(&self, requester: Uuid) {
        let result = self.context.borrow().core.storage().create_backup_now();
        match result {
            Ok(path) => {
                diagnostics::log(format_args!(
                    "event=backup-created kind=manual path={}",
                    path.display()
                ));
                self.report_data_result(requester, "backup", true, "Backup concluído.");
            }
            Err(error) => {
                eprintln!("Manual backup failed: {error}");
                self.report_data_result(
                    requester,
                    "backup",
                    false,
                    "Não foi possível criar o backup. Nada foi alterado.",
                );
            }
        }
    }

    /// Collects documents only. Flashcard syntax is deliberately absent from
    /// Rust; the requesting WebView parses every item with the one Tiptap
    /// schema and extractor the current note uses.
    pub fn answer_study_catalog(&self, requester: Uuid, request_id: u64) {
        let ctx = self.context.borrow();
        let scan = ctx.core.storage().read_note_bodies_by_recency();
        let batch = match scan {
            Ok(batch) => batch,
            Err(error) => {
                // The catalogue is every note, so a scan that could not be
                // performed cannot be answered with an empty one.
                eprintln!("Study catalog unavailable: {error}");
                if let Some(window) = ctx.windows.get(&requester) {
                    window.send_study_catalog(
                        request_id,
                        Vec::new(),
                        None,
                        Some(
                            "As notas não puderam ser lidas, então o catálogo de estudos está indisponível."
                                .to_string(),
                        ),
                    );
                }
                return;
            }
        };
        for warning in &batch.warnings {
            eprintln!("Study catalog warning: {}", warning.message);
        }
        let mut notes: Vec<StudyCatalogNote> = batch
            .items
            .into_iter()
            .map(|(id, content)| StudyCatalogNote { id, content })
            .collect();

        for note in &mut notes {
            if let Some(window) = ctx.windows.get(&note.id) {
                note.content = window.document.borrow().content.clone();
            }
        }

        let (study_state, error) = match ctx.core.study_state() {
            Ok(state) => (Some(state), None),
            Err(error) => {
                eprintln!("Study catalog unavailable: {error}");
                (
                    None,
                    Some(
                        "O histórico de estudos não pôde ser lido. As notas continuam disponíveis."
                            .to_string(),
                    ),
                )
            }
        };
        if let Some(window) = ctx.windows.get(&requester) {
            window.send_study_catalog(request_id, notes, study_state, error);
        }
    }

    pub fn rate_study(&self, requester: Uuid, request_id: u64, review_key: String, rating: Rating) {
        let result = self
            .context
            .borrow()
            .core
            .storage()
            .rate_study(&review_key, rating);
        let ctx = self.context.borrow();
        if let Some(window) = ctx.windows.get(&requester) {
            window.send_study_rating(request_id, review_key, result);
        }
    }

    /// Says that a timer ran out, once, to the desktop.
    ///
    /// The words come from [`TimerFinishKind::notification`] and from nowhere
    /// else: the page reports which kind of run ended and has no way to supply
    /// text, so no part of a note can reach the shell. Nothing here is
    /// required for the feature to work — the note itself shows the finished
    /// state whether or not a notification daemon is listening — so a desktop
    /// with none simply gets no notification rather than an error.
    ///
    /// One identifier for every note, so a second completion replaces the
    /// first in the shell instead of piling up beside it.
    pub fn announce_timer_finished(&self, kind: TimerFinishKind) {
        let (title, body) = kind.notification();
        let notification = gio::Notification::new(title);
        if let Some(body) = body {
            notification.set_body(Some(body));
        }
        notification.set_priority(gio::NotificationPriority::Normal);
        self.app
            .send_notification(Some(kind.notification_id()), &notification);
        diagnostics::log(format_args!("event=timer-finished kind={title}"));
    }

    /// Turns AutoPaste on for one note, or off.
    ///
    /// The system clipboard is one thing, so there is one target: arming a
    /// note releases whatever held it, and both notes are told, because a note
    /// that has lost the target is still showing that it has it otherwise.
    ///
    /// Switching on is also the only place the clipboard handler is ever
    /// connected, and switching off is the only place it is disconnected.
    /// Nothing observes the clipboard in between.
    pub fn set_autopaste(&self, note_id: Uuid, active: bool) {
        if active && !self.context.borrow().windows.contains_key(&note_id) {
            eprintln!("AutoPaste request rejected for a note that is not open");
            return;
        }

        let (released, target) = {
            let mut ctx = self.context.borrow_mut();
            if active {
                let outcome = ctx.autopaste.arm(note_id);
                (outcome.released, Some(outcome.session.note_id))
            } else {
                (ctx.autopaste.disarm_note(note_id), None)
            }
        };

        if active {
            self.connect_clipboard_watch();
        } else if released.is_some() {
            self.disconnect_clipboard_watch();
        }

        diagnostics::log(format_args!(
            "event=autopaste-target active={} released={}",
            target.is_some(),
            released.is_some()
        ));
        // The note that asked is told either way, so a request that changed
        // nothing cannot leave its own switch showing something else.
        let mut told: Vec<Uuid> = released.into_iter().chain(target).collect();
        if !told.contains(&note_id) {
            told.push(note_id);
        }
        self.publish_autopaste(told);
    }

    /// Stops capturing, whatever asked for it, and takes the listener down.
    ///
    /// Called before every teardown — closing a note, hiding, quitting, moving
    /// a note to the trash — and always *before* the flush, so a read still in
    /// flight can no longer reach a document that is about to be written and
    /// destroyed.
    fn disarm_autopaste(&self, reason: &str) {
        let released = {
            let mut ctx = self.context.borrow_mut();
            ctx.autopaste.disarm()
        };
        if released.is_none() {
            return;
        }
        self.disconnect_clipboard_watch();
        diagnostics::log(format_args!("event=autopaste-disarmed reason={reason}"));
        self.publish_autopaste(released.into_iter().collect());
    }

    /// Stops capturing if `note_id` is the note doing it. A note that never
    /// held the target must not switch it off for the note that does.
    fn disarm_autopaste_for(&self, note_id: Uuid, reason: &str) {
        if self.context.borrow().autopaste.is_target(note_id) {
            self.disarm_autopaste(reason);
        }
    }

    /// The capture delimiter, chosen from any note's menu.
    ///
    /// Application-wide like the theme, so it is stored once and broadcast.
    /// It changes how the *next* capture is laid out and nothing else: no note
    /// is opened, rewritten or dated by choosing one.
    pub fn set_capture_delimiter(&self, delimiter: CaptureDelimiter) {
        let windows: Vec<NoteWindow> = {
            let mut ctx = self.context.borrow_mut();
            let resolved = delimiter.as_str();
            if delimiter_name(&ctx.config.capture_delimiter) == resolved {
                return;
            }
            ctx.config.capture_delimiter = resolved.to_string();
            let config_path = ctx.core.storage().config_file_path();
            if !ctx.can_persist_config {
                eprintln!("Aviso: gravação de delimitador ignorada para proteger arquivo de configuração original.");
            } else if let Err(error) = ctx.config.save_to_file(&config_path) {
                eprintln!("Failed to persist the capture delimiter: {error}");
            }
            ctx.windows.values().cloned().collect()
        };

        let (delimiter, target) = {
            let ctx = self.context.borrow();
            (
                delimiter_from_name(&ctx.config.capture_delimiter),
                ctx.autopaste.target(),
            )
        };
        for window in windows {
            window.set_autopaste(target == Some(window.id), delimiter);
        }
    }

    /// Tells each named note whether it is the capture target now.
    fn publish_autopaste(&self, notes: Vec<Uuid>) {
        let (delimiter, target, windows) = {
            let ctx = self.context.borrow();
            (
                delimiter_from_name(&ctx.config.capture_delimiter),
                ctx.autopaste.target(),
                notes
                    .into_iter()
                    .filter_map(|id| ctx.windows.get(&id).cloned())
                    .collect::<Vec<_>>(),
            )
        };
        for window in windows {
            window.set_autopaste(target == Some(window.id), delimiter);
        }
    }

    fn clipboard(&self) -> Option<gdk::Clipboard> {
        gdk::Display::default().map(|display| display.clipboard())
    }

    fn connect_clipboard_watch(&self) {
        {
            let ctx = self.context.borrow();
            // The listener exists if and only if a note is capturing. Stated
            // here rather than assumed, because "off observes nothing" is the
            // whole privacy contract and it should not rest on call order.
            if ctx.clipboard_watch.is_some() || !ctx.autopaste.is_armed() {
                return;
            }
        }
        let Some(clipboard) = self.clipboard() else {
            eprintln!("AutoPaste cannot observe the clipboard: no display");
            return;
        };

        // Connected here and nowhere else, and only once AutoPaste has been
        // switched on deliberately. Connecting does not read anything: GDK
        // emits `changed` for changes from here on, so whatever was on the
        // clipboard before this moment is never seen.
        let controller = self.clone();
        let handler = clipboard.connect_changed(move |clipboard| {
            controller.handle_clipboard_change(clipboard);
        });
        self.context.borrow_mut().clipboard_watch = Some(handler);
    }

    fn disconnect_clipboard_watch(&self) {
        let handler = self.context.borrow_mut().clipboard_watch.take();
        let Some(handler) = handler else {
            return;
        };
        if let Some(clipboard) = self.clipboard() {
            clipboard.disconnect(handler);
        }
    }

    /// One clipboard change, decided by [`noteit_core::autopaste`].
    ///
    /// Two gates before anything is read. `is_local` is GDK's own answer to
    /// "did this application put that there", and it is the loop protection: a
    /// `Ctrl+C` or `Ctrl+X` inside a note is Note-it claiming the clipboard, so
    /// the capture that would feed a note its own words back never starts. The
    /// formats gate refuses an image or a file list without transferring a
    /// byte of it.
    fn handle_clipboard_change(&self, clipboard: &gdk::Clipboard) {
        let own = clipboard.is_local();
        let has_text = clipboard_offers_text(clipboard);
        let decision = {
            let mut ctx = self.context.borrow_mut();
            ctx.autopaste.observe(own, has_text)
        };
        // The shape of the decision, never a byte of the content. Off by
        // default like every other diagnostic here, and even when it is on
        // there is nothing in it that could say what was copied.
        diagnostics::log(format_args!(
            "event=clipboard-change decision={} own={own} text={has_text}",
            match decision {
                ChangeDecision::Read(_) => "read",
                ChangeDecision::Queue => "queued",
                ChangeDecision::Ignore(IgnoreReason::NotArmed) => "ignored-not-armed",
                ChangeDecision::Ignore(IgnoreReason::OwnClipboard) => "ignored-own",
                ChangeDecision::Ignore(IgnoreReason::NotText) => "ignored-not-text",
            }
        ));
        match decision {
            ChangeDecision::Read(session) => self.read_clipboard(clipboard, session),
            ChangeDecision::Ignore(_) | ChangeDecision::Queue => {}
        }
    }

    fn read_clipboard(&self, clipboard: &gdk::Clipboard, session: CaptureSession) {
        let controller = self.clone();
        let clipboard = clipboard.clone();
        clipboard.read_text_async(gio::Cancellable::NONE, move |result| {
            controller.deliver_capture(result, session);
        });
    }

    /// A finished read, revalidated against the state as it is *now*.
    ///
    /// Everything may have changed while the read was in the air: AutoPaste
    /// switched off, the target moved to another note, the note closed, the
    /// application hiding. The session carries the generation it started under
    /// and the check is exact, so a stale read delivers nothing rather than
    /// arriving late in a note that stopped asking for it.
    fn deliver_capture(
        &self,
        result: Result<Option<glib::GString>, glib::Error>,
        session: CaptureSession,
    ) {
        // The read is over whatever came of it, so the queue may move on.
        let next = {
            let mut ctx = self.context.borrow_mut();
            ctx.autopaste.finish_read()
        };

        match result {
            Ok(Some(text)) if is_capturable(&text) => {
                let window = {
                    let ctx = self.context.borrow();
                    ctx.autopaste
                        .accept(session)
                        .and_then(|note_id| ctx.windows.get(&note_id).cloned())
                };
                if let Some(window) = window {
                    diagnostics::log(format_args!("event=clipboard-capture delivered=true"));
                    // The text goes to the note's editor and to nothing else.
                    // It is not stored here, not logged, and not written to
                    // disk by this path: the page inserts it and the ordinary
                    // autosave carries it to the file.
                    window.send_autopaste_capture(&text);
                }
            }
            Ok(_) => {}
            Err(error) => {
                // The reason, never the content — and the mode stays armed, so
                // one unreadable clipboard does not end the capture session.
                eprintln!("AutoPaste could not read the clipboard: {error}");
            }
        }

        if let Some(session) = next {
            if let Some(clipboard) = self.clipboard() {
                self.read_clipboard(&clipboard, session);
            }
        }
    }

    /// Shows a file chooser and puts the chosen image in the note.
    ///
    /// The host opens the chooser, so the path is one the *reader* picked in a
    /// native dialog rather than one the page named — the page asks for the
    /// gesture and is told the result. A cancelled chooser is not a failure and
    /// not a change: nothing is imported, nothing is said and the note is not
    /// touched, so its modification date does not move for a dialog somebody
    /// looked at and closed.
    pub fn choose_image(&self, note_id: Uuid) {
        if !self.context.borrow().windows.contains_key(&note_id) {
            eprintln!("Image request rejected for a note that is not open");
            return;
        }

        let filter = gtk4::FileFilter::new();
        filter.set_name(Some("Imagens"));
        for mime in ["image/png", "image/jpeg", "image/webp", "image/gif"] {
            filter.add_mime_type(mime);
        }
        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&filter);

        let dialog = gtk4::FileDialog::new();
        dialog.set_title("Inserir imagem");
        dialog.set_filters(Some(&filters));
        dialog.set_default_filter(Some(&filter));

        let controller = self.clone();
        dialog.open(
            None::<&gtk4::Window>,
            gio::Cancellable::NONE,
            move |result| {
                let Ok(file) = result else {
                    // Cancelled, or dismissed. Neither is worth reporting and
                    // neither changes anything.
                    return;
                };
                let Some(path) = file.path() else {
                    controller.report_image_failure(note_id, ImportError::Empty.message());
                    return;
                };
                match std::fs::read(&path) {
                    Ok(bytes) => controller.import_image_for(note_id, &bytes),
                    Err(_) => {
                        controller.report_image_failure(note_id, ImportError::Empty.message())
                    }
                }
            },
        );
    }

    /// Takes in an image the reader pasted or dropped.
    ///
    /// The page hands over the bytes the gesture gave it, never a path, so
    /// there is nothing here the page could point at a file it should not
    /// read. What the bytes are is decided from the bytes.
    pub fn import_image_bytes(&self, note_id: Uuid, encoded: &str) {
        let Some(bytes) = decode_base64(encoded) else {
            self.report_image_failure(note_id, ImportError::Empty.message());
            return;
        };
        self.import_image_for(note_id, &bytes);
    }

    /// Writes an accepted image into the note's own asset directory and tells
    /// the page how the note should refer to it.
    ///
    /// The host never edits the note. It stores the bytes and sends back a
    /// relative path; the page puts that into the document and the existing
    /// autosave writes the file — one authority over the document, the same
    /// rule a clipboard capture follows.
    fn import_image_for(&self, note_id: Uuid, bytes: &[u8]) {
        let assets_dir = self
            .context
            .borrow()
            .core
            .storage()
            .assets_dir()
            .to_path_buf();
        match import_image(&assets_dir, note_id, bytes) {
            Ok(asset) => {
                diagnostics::log(format_args!(
                    "event=image-imported format={} bytes={}",
                    asset.format.extension(),
                    bytes.len()
                ));
                let window = self.context.borrow().windows.get(&note_id).cloned();
                if let Some(window) = window {
                    window.send_image_inserted(&asset.relative_path());
                }
            }
            Err(error) => {
                // The reason, never the picture and never where it came from.
                diagnostics::log(format_args!("event=image-refused reason={error:?}"));
                self.report_image_failure(note_id, error.message());
            }
        }
    }

    fn report_image_failure(&self, note_id: Uuid, message: &str) {
        let window = self.context.borrow().windows.get(&note_id).cloned();
        if let Some(window) = window {
            window.send_image_failed(message);
        }
    }

    fn report_data_result(&self, requester: Uuid, action: &str, ok: bool, message: &str) {
        let ctx = self.context.borrow();
        if let Some(window) = ctx.windows.get(&requester) {
            window.send_data_result(action, ok, message);
        }
    }

    fn instantiate_note_by_id(&self, id: Uuid, mode: LayerMode) {
        if self.context.borrow().windows.contains_key(&id) {
            return;
        }

        let (doc_res, win_state) = {
            let ctx = self.context.borrow();
            let doc = ctx.core.read_note(&id);
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

/// Serves a note's own images to its WebView, and nothing else.
///
/// The page asks for `note-it-asset:/<note>/<asset>.<ext>` rather than for a
/// file, which is the same rule the rest of Note-it follows: the frontend
/// never spells a path, so there is nothing for it to traverse. Both halves of
/// the request are parsed as `Uuid`s before anything touches the disk, so a
/// `..`, an absolute path or an encoded separator does not resolve to a file —
/// it does not parse at all.
///
/// Registered once, at startup, on the default web context every note's
/// WebView is built from. `register_uri_scheme_as_local` puts it in the same
/// class as `file:`, which is what lets a page loaded from disk display one.
fn register_asset_scheme(assets_dir: PathBuf) {
    let Some(context) = webkit6::WebContext::default() else {
        eprintln!("Images unavailable: no WebKit context to serve them from");
        return;
    };

    context.register_uri_scheme(ASSET_SCHEME, move |request| {
        let path = request
            .path()
            .map(|path| path.to_string())
            .unwrap_or_default();
        diagnostics::log(format_args!("event=asset-request path={path}"));

        let Some(asset) = parse_asset_request(&path) else {
            let mut error = glib::Error::new(gio::IOErrorEnum::InvalidArgument, "not an asset");
            request.finish_error(&mut error);
            return;
        };

        let file = asset.file_path(&assets_dir);
        let length = std::fs::metadata(&file)
            .map(|metadata| metadata.len() as i64)
            .unwrap_or(-1);
        match gio::File::for_path(&file).read(gio::Cancellable::NONE) {
            Ok(stream) => {
                let response = webkit6::URISchemeResponse::new(&stream, length);
                response.set_content_type(asset.format.mime_type());
                request.finish_with_response(&response);
            }
            Err(_) => {
                // A note pointing at an image that is no longer there is a
                // note with a broken picture, not a broken note: the page
                // shows its own placeholder and carries on.
                let mut error = glib::Error::new(gio::IOErrorEnum::NotFound, "no such image");
                request.finish_error(&mut error);
            }
        }
    });

    if let Some(security) = context.security_manager() {
        security.register_uri_scheme_as_local(ASSET_SCHEME);
    }
}

/// Whether the clipboard is offering something that can be read as text.
///
/// The same question `gdk_clipboard_read_text_async` asks itself before it
/// transfers anything, asked first so an image, a file list or an unknown
/// binary format is declined without a byte of it being read. Nothing about
/// the *content* is examined here — only what the owner says it can provide.
fn clipboard_offers_text(clipboard: &gdk::Clipboard) -> bool {
    let formats = clipboard.formats();
    formats
        .clone()
        .union_deserialize_types()
        .contains_type(glib::types::Type::STRING)
        || formats.contain_mime_type("text/plain")
        || formats.contain_mime_type("text/plain;charset=utf-8")
}

fn persist_state_now(ctx: &mut AppContext, reason: &str) -> Result<(), String> {
    ctx.layer_state_persistence.cancel();
    diagnostics::log(format_args!("event=state-persist-now reason={reason}"));
    if !ctx.can_persist_state {
        diagnostics::log(format_args!(
            "event=state-persist-skipped reason=persistence-blocked"
        ));
        return Ok(());
    }
    ctx.state
        .save_to_file(&ctx.core.storage().state_file_path())
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

/// Deleting a note, in order, with what each failure means written down.
///
/// Everything before the move can fail with the note still live: the flush
/// that could not write the latest text, and the move that could not happen.
/// In both cases nothing has changed and the caller keeps the note open.
///
/// The move is the commit point. From it onwards the note is in the trash, so
/// neither of the two steps that follow may turn into a failure of the
/// deletion. The window state is written best-effort — a stale entry for a
/// note that is no longer in `notes/` costs nothing, because what is restored
/// on startup comes from the files on disk — and the surface is destroyed
/// either way, because the note it was showing is not there any more.
fn commit_trash<M, S, D>(
    flush: Result<(), String>,
    move_to_trash: M,
    persist_state: S,
    destroy_surface: D,
) -> Result<(), String>
where
    M: FnOnce() -> Result<(), String>,
    S: FnOnce() -> Result<(), String>,
    D: FnOnce(),
{
    flush.map_err(|error| {
        format!("the note could not be saved, so it was not moved to the trash: {error}")
    })?;
    move_to_trash()?;

    // Past the commit point.
    if let Err(error) = persist_state() {
        eprintln!(
            "The note was moved to the trash, but the window state could not be \
             persisted: {error}"
        );
    }
    destroy_surface();
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

    let app_scale = app_controller.clone();
    let on_ui_scale_changed = Rc::new(move |percent: u16| {
        app_scale.set_ui_scale(percent);
    });

    let app_clone6 = app_controller.clone();
    let on_search = Rc::new(move |requester, request_id, query: String| {
        app_clone6.answer_search(requester, request_id, &query);
    });

    let app_clone7 = app_controller.clone();
    let on_open_search_result = Rc::new(move |requester, target, query: String| {
        app_clone7.open_search_result(requester, target, query);
    });

    let app_clone8 = app_controller.clone();
    let on_trash_note = Rc::new(move |id| {
        app_clone8.trash_note(id);
    });

    let app_clone9 = app_controller.clone();
    let on_list_trash = Rc::new(move |requester, request_id| {
        app_clone9.answer_trash_list(requester, request_id);
    });

    let app_clone10 = app_controller.clone();
    let on_restore_note = Rc::new(move |requester, target| {
        app_clone10.restore_note(requester, target);
    });

    let app_clone11 = app_controller.clone();
    let on_backup = Rc::new(move |requester| {
        app_clone11.create_backup(requester);
    });

    let app_clone12 = app_controller.clone();
    let on_timer_finished = Rc::new(move |_id, kind| {
        app_clone12.announce_timer_finished(kind);
    });

    let app_clone13 = app_controller.clone();
    let on_autopaste_requested = Rc::new(move |id, active| {
        app_clone13.set_autopaste(id, active);
    });

    let app_clone14 = app_controller.clone();
    let on_capture_delimiter_changed = Rc::new(move |delimiter| {
        app_clone14.set_capture_delimiter(delimiter);
    });

    let app_clone15 = app_controller.clone();
    let on_insert_image = Rc::new(move |id| {
        app_clone15.choose_image(id);
    });

    let app_clone16 = app_controller.clone();
    let on_image_bytes = Rc::new(move |id, data: String| {
        app_clone16.import_image_bytes(id, &data);
    });

    let app_clone17 = app_controller.clone();
    let on_study_catalog = Rc::new(move |requester, request_id| {
        app_clone17.answer_study_catalog(requester, request_id);
    });

    let app_clone18 = app_controller.clone();
    let on_study_rating = Rc::new(move |requester, request_id, review_key, rating| {
        app_clone18.rate_study(requester, request_id, review_key, rating);
    });

    NoteWindow::new(NoteWindowOptions {
        app,
        document: doc,
        state: win_state,
        layer_mode,
        storage: ctx.core.storage().clone(),
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
        ui_scale_percent: ctx.config.ui_scale_percent,
        on_ui_scale_changed,
        on_search,
        on_open_search_result,
        on_trash_note,
        on_list_trash,
        on_restore_note,
        on_backup,
        on_study_catalog,
        on_study_rating,
        on_timer_finished,
        capture_delimiter: delimiter_from_name(&ctx.config.capture_delimiter),
        on_autopaste_requested,
        on_capture_delimiter_changed,
        on_insert_image,
        on_image_bytes,
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
        apply_shared_layer_transition, commit_hidden_transition, commit_quit, commit_trash,
        effective_layer_mode, is_live_layer_noop, plan_startup, plan_summon_layer,
        preferred_layer_after_new_note, prepare_new_note, FlushBatch, LifecycleCoordinator,
        LifecycleOperation, StartupPlan, StatePersistenceDebouncer, SummonLayerPlan,
    };
    use noteit_core::settings::AppConfig;
    use noteit_core::state::{AppState, LayerMode};
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

    // ------------------------------------------------------------------
    // Phase 3.9 — deleting a note, and what each failure along the way means.
    // ------------------------------------------------------------------

    /// What the three steps after the flush actually did.
    #[derive(Debug, Default)]
    struct TrashRun {
        moved: Cell<bool>,
        persisted: Cell<bool>,
        destroyed: Cell<bool>,
    }

    fn run_trash(
        run: &TrashRun,
        flush: Result<(), String>,
        move_result: Result<(), String>,
        persist_result: Result<(), String>,
    ) -> Result<(), String> {
        commit_trash(
            flush,
            || {
                run.moved.set(true);
                move_result
            },
            || {
                run.persisted.set(true);
                persist_result
            },
            || run.destroyed.set(true),
        )
    }

    #[test]
    fn failed_flush_does_not_trash_the_note() {
        let run = TrashRun::default();
        let error = run_trash(
            &run,
            Err("timed out waiting for latest WebView content".to_string()),
            Ok(()),
            Ok(()),
        )
        .expect_err("a note whose text could not be saved must not be deleted");

        assert!(error.contains("could not be saved"), "{error}");
        assert!(
            !run.moved.get(),
            "nothing may be moved before the text is safe"
        );
        assert!(!run.persisted.get());
        assert!(
            !run.destroyed.get(),
            "the note has to stay open so the reader can try again"
        );
    }

    #[test]
    fn failed_move_does_not_close_or_forget_the_note() {
        let run = TrashRun::default();
        let error = run_trash(
            &run,
            Ok(()),
            Err("Failed to move note to the trash: permission denied".to_string()),
            Ok(()),
        )
        .expect_err("a move that did not happen is not a deletion");

        assert!(error.contains("permission denied"), "{error}");
        assert!(run.moved.get(), "the move was attempted");
        assert!(
            !run.persisted.get(),
            "a note still in the store must not be recorded as closed"
        );
        assert!(!run.destroyed.get(), "its window must stay open");
    }

    #[test]
    fn a_state_write_that_fails_after_the_move_still_completes_the_deletion() {
        // Phase 3.4R.2's rule, applied to the trash: the move is the commit
        // point, so past it nothing may claim the deletion did not happen. The
        // window goes, because the note it was showing is not there any more.
        let run = TrashRun::default();
        run_trash(
            &run,
            Ok(()),
            Ok(()),
            Err("Failed to write the window state".to_string()),
        )
        .expect("past the commit point the deletion has happened");

        assert!(run.moved.get());
        assert!(run.persisted.get());
        assert!(run.destroyed.get());
    }

    #[test]
    fn a_successful_deletion_runs_flush_move_state_and_surface_in_that_order() {
        let order = Rc::new(RefCell::new(Vec::new()));
        let move_order = Rc::clone(&order);
        let persist_order = Rc::clone(&order);
        let destroy_order = Rc::clone(&order);

        commit_trash(
            Ok(()),
            move || {
                move_order.borrow_mut().push("move");
                Ok(())
            },
            move || {
                persist_order.borrow_mut().push("state");
                Ok(())
            },
            move || destroy_order.borrow_mut().push("surface"),
        )
        .expect("deletion");

        assert_eq!(&*order.borrow(), &["move", "state", "surface"]);
    }

    #[test]
    fn stale_state_for_a_trashed_note_does_not_recreate_it() {
        // A note moved to the trash leaves its entry in `state.json`, closed.
        // What is restored comes from the files on disk, so an entry naming a
        // note that is no longer in `notes/` can never bring one back — and it
        // must not be mistaken for a note to create either.
        let trashed = Uuid::new_v4();
        let live = Uuid::new_v4();

        let mut state = AppState::default();
        state.notes.insert(
            trashed,
            noteit_core::state::NoteWindowState {
                is_open: false,
                ..Default::default()
            },
        );
        state.notes.insert(
            live,
            noteit_core::state::NoteWindowState {
                is_open: true,
                ..Default::default()
            },
        );

        match plan_startup(false, vec![live], &state) {
            StartupPlan::Restore { note_ids, .. } => assert_eq!(note_ids, vec![live]),
            other => panic!("unexpected plan: {other:?}"),
        }

        // And a state file left claiming the deleted note is still open — a
        // write that failed after the move — cannot resurrect it either.
        state.notes.get_mut(&trashed).expect("entry").is_open = true;
        match plan_startup(false, vec![live], &state) {
            StartupPlan::Restore { note_ids, .. } => {
                assert_eq!(note_ids, vec![live]);
                assert!(!note_ids.contains(&trashed));
            }
            other => panic!("unexpected plan: {other:?}"),
        }

        // With every live note gone too, the fallback offers a new note rather
        // than a note that is in the trash.
        let mut only_trashed = AppState::default();
        only_trashed.notes.insert(
            trashed,
            noteit_core::state::NoteWindowState {
                is_open: true,
                ..Default::default()
            },
        );
        assert_eq!(
            plan_startup(false, Vec::new(), &only_trashed),
            StartupPlan::CreateNew
        );
    }

    #[test]
    fn a_note_with_no_state_entry_at_all_is_still_restored() {
        // Reliability audit, case J. A note file with nothing said about it in
        // `state.json` — one copied in by hand, or one whose state write was
        // lost — is treated as open, which is the reading that never hides a
        // note the user still has.
        let known = Uuid::new_v4();
        let unknown = Uuid::new_v4();

        let mut state = AppState::default();
        state.notes.insert(
            known,
            noteit_core::state::NoteWindowState {
                is_open: true,
                ..Default::default()
            },
        );

        match plan_startup(false, vec![unknown, known], &state) {
            StartupPlan::Restore { note_ids, .. } => {
                assert_eq!(note_ids, vec![unknown, known]);
            }
            other => panic!("unexpected plan: {other:?}"),
        }
    }
    // Lifecycle against external writes -----------------------------------------
    //
    // Hiding, quitting, deleting a note and an external write all reach into
    // the same windows and the same files. Two of them overlapping is how a
    // note gets written by one and destroyed by another, so there is one
    // coordinator and the rule is symmetric.

    #[test]
    fn hiding_or_quitting_is_refused_while_an_external_write_holds_a_note() {
        // The window is holding its editor still and its unsaved text has been
        // handed to a writer. Destroying the WebView now would take that text
        // with it.
        let mut coordinator = LifecycleCoordinator::default();
        coordinator
            .begin_external_write()
            .expect("nothing else is happening");

        assert!(coordinator.begin(LifecycleOperation::Hide).is_err());
        assert!(coordinator.begin(LifecycleOperation::Quit).is_err());
        assert!(coordinator.is_active());
        assert!(coordinator
            .ensure_structural_action_allowed("moving a note to the trash")
            .is_err());

        coordinator.finish_external_write();
        assert!(coordinator.begin(LifecycleOperation::Hide).is_ok());
    }

    #[test]
    fn an_external_write_is_refused_while_the_application_is_hiding_or_quitting() {
        // The other direction. The writer is told the store is busy, which is
        // true and which it can act on — rather than being allowed to start a
        // barrier on a window that is about to go away.
        for operation in [LifecycleOperation::Hide, LifecycleOperation::Quit] {
            let mut coordinator = LifecycleCoordinator::default();
            coordinator.begin(operation).expect("start the lifecycle");

            let refusal = coordinator
                .begin_external_write()
                .expect_err("an external write must not start here");
            assert!(refusal.contains("ocupado"), "{refusal}");

            coordinator.finish(operation);
            assert!(coordinator.begin_external_write().is_ok());
        }
    }

    #[test]
    fn a_failed_external_write_never_leaves_the_application_unable_to_quit() {
        // Every path out of an external write releases the claim: committed,
        // refused, or timed out. A claim left behind would make Note-it
        // impossible to close.
        let mut coordinator = LifecycleCoordinator::default();
        coordinator.begin_external_write().expect("claim");
        coordinator.finish_external_write();
        assert!(!coordinator.is_active());
        assert!(coordinator.begin(LifecycleOperation::Quit).is_ok());
    }

    #[test]
    fn releasing_a_claim_that_is_not_held_cannot_unlock_someone_else_s() {
        let mut coordinator = LifecycleCoordinator::default();
        coordinator.finish_external_write();
        coordinator.begin_external_write().expect("claim");
        coordinator.finish_external_write();
        coordinator.finish_external_write();
        assert!(!coordinator.is_active());
    }

    #[test]
    fn structural_actions_wait_for_an_external_write_rather_than_racing_it() {
        let mut coordinator = LifecycleCoordinator::default();
        coordinator.begin_external_write().expect("claim");
        for action in [
            "new note creation",
            "collapsing every note",
            "moving a note to the trash",
        ] {
            assert!(
                coordinator
                    .ensure_structural_action_allowed(action)
                    .is_err(),
                "{action} was allowed during an external write"
            );
        }
        coordinator.finish_external_write();
        assert!(coordinator
            .ensure_structural_action_allowed("new note creation")
            .is_ok());
    }
}
