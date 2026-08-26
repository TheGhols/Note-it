use crate::cli::CliCommand;
use crate::model::NoteDocument;
use crate::note_window::{NoteWindow, NoteWindowOptions};
use crate::settings::AppConfig;
use crate::state::{AppState, LayerMode, NoteWindowState};
use crate::storage::StorageManager;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use uuid::Uuid;

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
}

impl NoteItApp {
    pub fn new(app: &gtk4::Application) -> Self {
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
        }
    }

    pub fn handle_command(&self, command: Option<CliCommand>, is_background: bool) {
        match command {
            Some(CliCommand::New) => {
                self.create_new_note();
            }
            Some(CliCommand::Toggle) => {
                let mut ctx = self.context.borrow_mut();
                let next_mode = match ctx.state.active_layer_mode {
                    LayerMode::Desktop => LayerMode::Overlay,
                    LayerMode::Overlay => LayerMode::Desktop,
                    LayerMode::Hidden => LayerMode::Overlay,
                };
                self.set_layer_mode_internal(&mut ctx, next_mode);
            }
            Some(CliCommand::Show) => {
                let mut ctx = self.context.borrow_mut();
                self.set_layer_mode_internal(&mut ctx, LayerMode::Overlay);
            }
            Some(CliCommand::Hide) => {
                let mut ctx = self.context.borrow_mut();
                self.set_layer_mode_internal(&mut ctx, LayerMode::Hidden);
            }
            Some(CliCommand::Quit) => {
                let mut ctx = self.context.borrow_mut();
                self.save_and_quit(&mut ctx);
            }
            None => {
                if is_background {
                    self.restore_saved_notes(true);
                } else {
                    self.restore_saved_notes(false);
                }
            }
        }
    }

    fn restore_saved_notes(&self, is_background: bool) {
        let (note_ids, target_mode) = {
            let ctx = self.context.borrow();
            let ids = ctx.storage.list_notes().unwrap_or_default();
            let mode = if is_background {
                LayerMode::Desktop
            } else {
                ctx.state.active_layer_mode
            };
            (ids, mode)
        };

        if note_ids.is_empty() && !is_background {
            self.create_new_note();
            return;
        }

        let mut ctx = self.context.borrow_mut();
        for id in note_ids {
            if !ctx.windows.contains_key(&id) {
                if let Ok(doc) = ctx.storage.load_note(&id) {
                    let win_state = ctx
                        .state
                        .notes
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(NoteWindowState::default);

                    let on_new_note = {
                        let self_clone = self.clone_rc();
                        Rc::new(move || {
                            self_clone.create_new_note();
                        })
                    };

                    let on_close = {
                        let self_clone = self.clone_rc();
                        Rc::new(move |note_id| {
                            self_clone.close_note(&note_id);
                        })
                    };

                    let note_window = NoteWindow::new(NoteWindowOptions {
                        app: &self.app,
                        document: doc,
                        state: win_state,
                        layer_mode: target_mode,
                        storage: ctx.storage.clone(),
                        ui_dist_path: &ctx.ui_dist_path,
                        on_new_note,
                        on_close,
                    });

                    ctx.windows.insert(id, note_window);
                }
            }
        }

        ctx.state.active_layer_mode = target_mode;
    }

    pub fn create_new_note(&self) {
        let mut ctx = self.context.borrow_mut();
        let mut doc = NoteDocument::new_empty();
        doc.metadata.color = ctx.config.default_color.clone();
        doc.metadata.font_size = ctx.config.default_font_size;

        let _ = ctx.storage.save_note_atomic(&doc);

        let note_id = doc.metadata.id;
        let win_state = NoteWindowState {
            x: 120 + (ctx.windows.len() as i32 * 30),
            y: 120 + (ctx.windows.len() as i32 * 30),
            width: ctx.config.default_width,
            height: ctx.config.default_height,
            is_open: true,
            monitor: None,
        };

        let mode = if ctx.state.active_layer_mode == LayerMode::Hidden {
            LayerMode::Overlay
        } else {
            ctx.state.active_layer_mode
        };

        let on_new_note = {
            let self_clone = self.clone_rc();
            Rc::new(move || {
                self_clone.create_new_note();
            })
        };

        let on_close = {
            let self_clone = self.clone_rc();
            Rc::new(move |id| {
                self_clone.close_note(&id);
            })
        };

        let note_window = NoteWindow::new(NoteWindowOptions {
            app: &self.app,
            document: doc,
            state: win_state.clone(),
            layer_mode: mode,
            storage: ctx.storage.clone(),
            ui_dist_path: &ctx.ui_dist_path,
            on_new_note,
            on_close,
        });

        ctx.state.notes.insert(note_id, win_state);
        ctx.windows.insert(note_id, note_window);
        ctx.state.active_layer_mode = mode;
        let _ = ctx.state.save_to_file(&ctx.storage.state_file_path());
    }

    pub fn close_note(&self, id: &Uuid) {
        let mut ctx = self.context.borrow_mut();
        if let Some(win) = ctx.windows.remove(id) {
            win.save_now();
            win.window.close();
        }
        if let Some(entry) = ctx.state.notes.get_mut(id) {
            entry.is_open = false;
        }
        let _ = ctx.state.save_to_file(&ctx.storage.state_file_path());
    }

    fn set_layer_mode_internal(&self, ctx: &mut AppContext, mode: LayerMode) {
        ctx.state.active_layer_mode = mode;
        for window in ctx.windows.values() {
            window.set_layer_mode(mode);
        }
        let _ = ctx.state.save_to_file(&ctx.storage.state_file_path());
    }

    fn save_and_quit(&self, ctx: &mut AppContext) {
        for window in ctx.windows.values() {
            window.save_now();
        }
        let _ = ctx.state.save_to_file(&ctx.storage.state_file_path());
        self.app.quit();
    }

    fn clone_rc(&self) -> NoteItAppClone {
        NoteItAppClone {
            app: self.app.clone(),
            context: Rc::clone(&self.context),
        }
    }
}

pub struct NoteItAppClone {
    pub app: gtk4::Application,
    pub context: Rc<RefCell<AppContext>>,
}

impl NoteItAppClone {
    pub fn create_new_note(&self) {
        let app = NoteItApp {
            app: self.app.clone(),
            context: Rc::clone(&self.context),
        };
        app.create_new_note();
    }

    pub fn close_note(&self, id: &Uuid) {
        let app = NoteItApp {
            app: self.app.clone(),
            context: Rc::clone(&self.context),
        };
        app.close_note(id);
    }
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
