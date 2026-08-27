use crate::cli::CliCommand;
use crate::layer_shell::{
    calculate_cascade_position, find_monitor_by_connector, DEFAULT_MONITOR_HEIGHT,
    DEFAULT_MONITOR_WIDTH,
};
use crate::model::NoteDocument;
use crate::note_window::{NoteWindow, NoteWindowOptions};
use crate::settings::AppConfig;
use crate::state::{AppState, LayerMode, NoteWindowState};
use crate::storage::StorageManager;
use gio::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

static NEXT_FLUSH_ID: AtomicU64 = AtomicU64::new(1);

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
}

pub struct NoteItApp {
    app: gtk4::Application,
    context: Rc<RefCell<AppContext>>,
    _hold_guard: gio::ApplicationHoldGuard,
}

impl NoteItApp {
    pub fn new(app: &gtk4::Application) -> Self {
        let hold_guard = app.hold();

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
        }));

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
                let current_mode = controller.context.borrow().state.active_layer_mode;
                let next_mode = match current_mode {
                    LayerMode::Desktop => LayerMode::Overlay,
                    LayerMode::Overlay => LayerMode::Desktop,
                    LayerMode::Hidden => LayerMode::Overlay,
                };
                controller.set_layer_mode(next_mode);
            }
            Some(CliCommand::Show) => {
                controller.set_layer_mode(LayerMode::Overlay);
            }
            Some(CliCommand::Hide) => {
                controller.set_layer_mode(LayerMode::Hidden);
            }
            Some(CliCommand::Quit) => {
                controller.save_and_quit();
            }
            None => {
                controller.restore_saved_notes(is_background);
            }
        }
    }
}

#[derive(Clone)]
pub struct NoteItAppClone {
    pub app: gtk4::Application,
    pub context: Rc<RefCell<AppContext>>,
}

impl NoteItAppClone {
    pub fn restore_saved_notes(&self, is_background: bool) {
        if is_background {
            let mut ctx = self.context.borrow_mut();
            ctx.state.active_layer_mode = LayerMode::Hidden;
            return;
        }

        let plan = {
            let ctx = self.context.borrow();
            let all_ids = match ctx.storage.list_notes() {
                Ok(ids) => ids,
                Err(error) => {
                    eprintln!("Failed to list saved notes: {error}");
                    Vec::new()
                }
            };
            plan_startup(false, all_ids, &ctx.state)
        };

        match plan {
            StartupPlan::Background => {}
            StartupPlan::CreateNew => self.create_new_note(),
            StartupPlan::Restore { note_ids, mode } => {
                for id in note_ids {
                    self.instantiate_note_by_id(id, mode);
                }
                self.context.borrow_mut().state.active_layer_mode = mode;
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
                ctx.state.active_layer_mode,
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
        ctx.state.active_layer_mode = target_mode;
        if let Err(error) = ctx.state.save_to_file(&ctx.storage.state_file_path()) {
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
        if let Err(error) = ctx.state.save_to_file(&ctx.storage.state_file_path()) {
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
        if let Err(error) = ctx.state.save_to_file(&ctx.storage.state_file_path()) {
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

    pub fn set_layer_mode(&self, mode: LayerMode) {
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
                        ..
                    } = &mut *ctx;
                    let state_path = storage.state_file_path();
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

        let needs_instantiation = {
            let mut ctx = self.context.borrow_mut();
            ctx.state.active_layer_mode = mode;
            if ctx.windows.is_empty() {
                true
            } else {
                for window in ctx.windows.values() {
                    window.set_layer_mode(mode);
                }
                false
            }
        };

        if needs_instantiation {
            let plan = {
                let ctx = self.context.borrow();
                let all_ids = ctx.storage.list_notes().unwrap_or_default();
                plan_startup(false, all_ids, &ctx.state)
            };
            match plan {
                StartupPlan::Background => {}
                StartupPlan::CreateNew => self.create_new_note(),
                StartupPlan::Restore { note_ids, .. } => {
                    for id in note_ids {
                        self.instantiate_note_by_id(id, mode);
                    }
                }
            }
        }

        let ctx = self.context.borrow();
        if let Err(error) = ctx.state.save_to_file(&ctx.storage.state_file_path()) {
            eprintln!("Failed to persist layer mode: {error}");
        }
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
                        let ctx = self_clone.context.borrow();
                        ctx.state.save_to_file(&ctx.storage.state_file_path())
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

fn note_ids_to_restore(all_ids: Vec<Uuid>, state: &AppState) -> Vec<Uuid> {
    all_ids
        .into_iter()
        .filter(|id| state.notes.get(id).map(|note| note.is_open).unwrap_or(true))
        .collect()
}

fn plan_startup(is_background: bool, all_ids: Vec<Uuid>, state: &AppState) -> StartupPlan {
    if is_background {
        return StartupPlan::Background;
    }

    let note_ids = note_ids_to_restore(all_ids, state);
    if note_ids.is_empty() {
        return StartupPlan::CreateNew;
    }

    let mode = if state.active_layer_mode == LayerMode::Hidden {
        LayerMode::Overlay
    } else {
        state.active_layer_mode
    };
    StartupPlan::Restore { note_ids, mode }
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
        commit_hidden_transition, commit_quit, plan_startup, prepare_new_note, FlushBatch,
        LifecycleCoordinator, LifecycleOperation, StartupPlan,
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
    fn normal_startup_without_open_notes_creates_one() {
        let closed_id = Uuid::new_v4();
        let mut state = AppState::default();
        state.notes.entry(closed_id).or_default().is_open = false;

        assert_eq!(
            plan_startup(false, vec![closed_id], &state),
            StartupPlan::CreateNew
        );
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
}
