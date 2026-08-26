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
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

static NEXT_FLUSH_ID: AtomicU64 = AtomicU64::new(1);

pub struct AppContext {
    pub storage: StorageManager,
    pub config: AppConfig,
    pub state: AppState,
    pub windows: HashMap<Uuid, NoteWindow>,
    pub ui_dist_path: PathBuf,
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

        let (all_ids, mut target_mode) = {
            let ctx = self.context.borrow();
            let ids = match ctx.storage.list_notes() {
                Ok(ids) => ids,
                Err(error) => {
                    eprintln!("Failed to list saved notes: {error}");
                    Vec::new()
                }
            };
            let mode = if ctx.state.active_layer_mode == LayerMode::Hidden {
                LayerMode::Overlay
            } else {
                ctx.state.active_layer_mode
            };
            (ids, mode)
        };

        if target_mode == LayerMode::Hidden {
            target_mode = LayerMode::Overlay;
        }

        // Filter only notes that are open (is_open == true). Unsaved state defaults to is_open = true for backward compatibility.
        let open_ids: Vec<Uuid> = {
            let ctx = self.context.borrow();
            all_ids
                .into_iter()
                .filter(|id| ctx.state.notes.get(id).map(|s| s.is_open).unwrap_or(true))
                .collect()
        };

        if open_ids.is_empty() {
            self.create_new_note();
            return;
        }

        for id in open_ids {
            self.instantiate_note_by_id(id, target_mode);
        }

        let mut ctx = self.context.borrow_mut();
        ctx.state.active_layer_mode = target_mode;
    }

    pub fn create_new_note(&self) {
        let mut doc = NoteDocument::new_empty();
        let note_id = doc.metadata.id;

        let (mut target_mode, win_state) = {
            let ctx = self.context.borrow();
            doc.metadata.color = ctx.config.default_color.clone();
            doc.metadata.font_size = ctx.config.default_font_size;

            if let Err(error) = ctx.storage.save_note_atomic(&doc) {
                eprintln!("Failed to save new note {note_id}: {error}");
                return;
            }

            let mode = if ctx.state.active_layer_mode == LayerMode::Hidden {
                LayerMode::Overlay
            } else {
                ctx.state.active_layer_mode
            };

            let display = gtk4::gdk::Display::default();
            let (_, conn_name, mon_w, mon_h) = find_monitor_by_connector(display.as_ref(), None);

            let (cx, cy) = calculate_cascade_position(
                ctx.windows.len(),
                mon_w,
                mon_h,
                ctx.config.default_width,
                ctx.config.default_height,
            );

            let win_state = NoteWindowState {
                x: cx,
                y: cy,
                width: ctx.config.default_width,
                height: ctx.config.default_height,
                is_open: true,
                monitor: conn_name,
            };

            (mode, win_state)
        };

        if target_mode == LayerMode::Hidden {
            target_mode = LayerMode::Overlay;
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

        let total = windows.len();
        let completed = Rc::new(Cell::new(0usize));
        let error_slot = Rc::new(RefCell::new(None::<String>));
        let on_complete_rc = Rc::new(RefCell::new(Some(on_complete)));
        let request_id = NEXT_FLUSH_ID.fetch_add(1, Ordering::SeqCst);

        for window in windows {
            let completed_clone = Rc::clone(&completed);
            let error_slot_clone = Rc::clone(&error_slot);
            let on_complete_slot = Rc::clone(&on_complete_rc);

            window.request_flush(request_id, move |result| {
                if let Err(e) = result {
                    let mut err_guard = error_slot_clone.borrow_mut();
                    if err_guard.is_none() {
                        *err_guard = Some(e);
                    }
                }
                let next = completed_clone.get() + 1;
                completed_clone.set(next);

                if next == total {
                    let maybe_error = error_slot_clone.borrow_mut().take();
                    if let Some(cb) = on_complete_slot.borrow_mut().take() {
                        if let Some(err) = maybe_error {
                            cb(Err(err));
                        } else {
                            cb(Ok(()));
                        }
                    }
                }
            });
        }
    }

    pub fn set_layer_mode(&self, mode: LayerMode) {
        if mode == LayerMode::Hidden {
            let self_clone = self.clone();
            self.flush_all_windows(move |result| match result {
                Ok(()) => {
                    let mut ctx = self_clone.context.borrow_mut();
                    for window in ctx.windows.values() {
                        window.close_after_save();
                    }
                    ctx.windows.clear();
                    ctx.state.active_layer_mode = LayerMode::Hidden;
                    if let Err(error) = ctx.state.save_to_file(&ctx.storage.state_file_path()) {
                        eprintln!("Failed to persist Hidden layer mode: {error}");
                    }
                }
                Err(error) => {
                    eprintln!(
                        "Hide operation aborted because one or more notes failed to save: {error}"
                    );
                }
            });
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
            let all_ids = {
                let ctx = self.context.borrow();
                ctx.storage.list_notes().unwrap_or_default()
            };
            let open_ids: Vec<Uuid> = {
                let ctx = self.context.borrow();
                all_ids
                    .into_iter()
                    .filter(|id| ctx.state.notes.get(id).map(|s| s.is_open).unwrap_or(true))
                    .collect()
            };
            for id in open_ids {
                self.instantiate_note_by_id(id, mode);
            }
        }

        let ctx = self.context.borrow();
        if let Err(error) = ctx.state.save_to_file(&ctx.storage.state_file_path()) {
            eprintln!("Failed to persist layer mode: {error}");
        }
    }

    pub fn save_and_quit(&self) {
        let self_clone = self.clone();
        self.flush_all_windows(move |result| match result {
            Ok(()) => {
                let ctx = self_clone.context.borrow();
                if let Err(error) = ctx.state.save_to_file(&ctx.storage.state_file_path()) {
                    eprintln!(
                        "Quit cancelled because application state could not be saved: {error}"
                    );
                    return;
                }
                self_clone.app.quit();
            }
            Err(error) => {
                eprintln!("Quit cancelled because one or more notes could not be saved: {error}");
            }
        });
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
