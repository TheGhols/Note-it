use crate::layer_shell::{
    apply_layer_mode, clamp_geometry, setup_layer_shell_window, update_window_position,
    update_window_size, DEFAULT_MONITOR_HEIGHT, DEFAULT_MONITOR_WIDTH,
};
use crate::model::NoteDocument;
use crate::state::{LayerMode, NoteWindowState};
use crate::storage::StorageManager;
use crate::webview_bridge::{
    parse_webview_message, send_to_webview, validate_external_url, HostToWebviewMessage,
    WebviewToHostMessage,
};
use gtk4::gdk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::{Settings, UserContentManager, WebView};

pub struct NoteWindowOptions<'a> {
    pub app: &'a gtk4::Application,
    pub document: NoteDocument,
    pub state: NoteWindowState,
    pub layer_mode: LayerMode,
    pub storage: StorageManager,
    pub ui_dist_path: &'a Path,
    pub monitor: Option<gdk::Monitor>,
    pub monitor_name: Option<String>,
    pub monitor_width: i32,
    pub monitor_height: i32,
    pub on_new_note: Rc<dyn Fn()>,
    pub on_close: Rc<dyn Fn(Uuid) -> Result<(), String>>,
    pub on_geometry_changed: Rc<dyn Fn(Uuid, NoteWindowState)>,
}

type FlushCallback = Box<dyn FnOnce(Result<(), String>)>;
type PendingFlushes = Rc<RefCell<std::collections::HashMap<u64, FlushCallback>>>;

const FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
const FLUSH_TIMEOUT_ERROR: &str = "timed out waiting for latest WebView content";

#[derive(Clone)]
#[allow(dead_code)]
pub struct NoteWindow {
    pub id: Uuid,
    pub window: gtk4::Window,
    pub webview: WebView,
    pub document: Rc<RefCell<NoteDocument>>,
    pub state: Rc<RefCell<NoteWindowState>>,
    pub storage: StorageManager,
    allow_close: Rc<Cell<bool>>,
    pending_flushes: PendingFlushes,
}

impl NoteWindow {
    pub fn new(options: NoteWindowOptions) -> Self {
        let id = options.document.metadata.id;
        let window = gtk4::ApplicationWindow::builder()
            .application(options.app)
            .title("Note-it")
            .decorated(false)
            .build();

        let mon_w = if options.monitor_width > 0 {
            options.monitor_width
        } else {
            DEFAULT_MONITOR_WIDTH
        };
        let mon_h = if options.monitor_height > 0 {
            options.monitor_height
        } else {
            DEFAULT_MONITOR_HEIGHT
        };

        // Clamp initial geometry
        let (clamped_x, clamped_y, clamped_w, clamped_h) = clamp_geometry(
            options.state.x,
            options.state.y,
            options.state.width,
            options.state.height,
            mon_w,
            mon_h,
        );

        let mut initial_state = options.state;
        initial_state.x = clamped_x;
        initial_state.y = clamped_y;
        initial_state.width = clamped_w;
        initial_state.height = clamped_h;
        if options.monitor_name.is_some() {
            initial_state.monitor = options.monitor_name;
        }

        let doc_rc = Rc::new(RefCell::new(options.document));
        let state_rc = Rc::new(RefCell::new(initial_state));

        // Setup Layer Shell
        setup_layer_shell_window(
            window.upcast_ref(),
            options.layer_mode,
            clamped_x,
            clamped_y,
            clamped_w,
            clamped_h,
            options.monitor.as_ref(),
        );

        // Configure WebKit settings
        let settings = Settings::new();
        settings.set_enable_developer_extras(cfg!(debug_assertions));
        settings.set_javascript_can_access_clipboard(true);
        settings.set_allow_file_access_from_file_urls(true);

        let content_manager = UserContentManager::new();
        content_manager.register_script_message_handler("noteItHost", None);

        let webview = WebView::builder()
            .settings(&settings)
            .user_content_manager(&content_manager)
            .build();

        let allow_close = Rc::new(Cell::new(false));
        let allow_close_signal = Rc::clone(&allow_close);
        let close_webview = webview.downgrade();
        window.connect_close_request(move |_| {
            if allow_close_signal.get() {
                glib::Propagation::Proceed
            } else {
                if let Some(webview) = close_webview.upgrade() {
                    send_to_webview(&webview, &HostToWebviewMessage::RequestSaveAndClose);
                }
                glib::Propagation::Stop
            }
        });

        // Ensure transparent webview background so custom post-it border radius renders cleanly
        webview.set_background_color(&gtk4::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));

        let pending_flushes: PendingFlushes =
            Rc::new(RefCell::new(std::collections::HashMap::new()));

        // Connect Webview Messages
        let doc_clone = Rc::clone(&doc_rc);
        let state_clone = Rc::clone(&state_rc);
        let storage_clone = options.storage.clone();
        let webview_weak = webview.downgrade();
        let window_weak = window.downgrade();
        let on_new_note_clone = Rc::clone(&options.on_new_note);
        let on_close_clone = Rc::clone(&options.on_close);
        let on_geom_clone = Rc::clone(&options.on_geometry_changed);
        let flushes_clone = Rc::clone(&pending_flushes);

        content_manager.connect_script_message_received(
            Some("noteItHost"),
            move |_manager, js_val| {
                let raw_json = js_val.to_str();
                if let Ok(msg) = parse_webview_message(&raw_json) {
                    match msg {
                        WebviewToHostMessage::Ready => {
                            if let Some(wv) = webview_weak.upgrade() {
                                let doc = doc_clone.borrow();
                                send_to_webview(
                                    &wv,
                                    &HostToWebviewMessage::LoadNote {
                                        id: doc.metadata.id,
                                        content: doc.content.clone(),
                                        color: doc.metadata.color.clone(),
                                        font_size: doc.metadata.font_size,
                                    },
                                );
                            }
                        }
                        WebviewToHostMessage::ContentChanged {
                            id: message_id,
                            content,
                        } => {
                            if message_id != id {
                                eprintln!("Autosave rejected a mismatched note identifier");
                            } else if let Err(error) =
                                save_content(&storage_clone, &doc_clone, id, content)
                            {
                                eprintln!("Autosave failed for note {id}: {error}");
                            }
                        }
                        WebviewToHostMessage::ColorChanged {
                            id: message_id,
                            color,
                        } => {
                            if message_id != id {
                                eprintln!("Color save rejected a mismatched note identifier");
                                return;
                            }
                            let mut doc = doc_clone.borrow_mut();
                            doc.metadata.color = color;
                            doc.metadata.updated_at = chrono::Utc::now();
                            if let Err(error) = storage_clone.save_note_atomic(&doc) {
                                eprintln!("Color save failed for note {id}: {error}");
                            }
                        }
                        WebviewToHostMessage::FontSizeChanged {
                            id: message_id,
                            font_size,
                        } => {
                            if message_id != id {
                                eprintln!("Font size save rejected a mismatched note identifier");
                                return;
                            }
                            let mut doc = doc_clone.borrow_mut();
                            doc.metadata.font_size = font_size;
                            doc.metadata.updated_at = chrono::Utc::now();
                            if let Err(error) = storage_clone.save_note_atomic(&doc) {
                                eprintln!("Font size save failed for note {id}: {error}");
                            }
                        }
                        WebviewToHostMessage::SaveAndClose {
                            id: message_id,
                            content,
                        } => {
                            if message_id != id {
                                eprintln!("Save-and-close rejected a mismatched note identifier");
                            } else if let Err(error) = save_and_close(
                                &storage_clone,
                                &doc_clone,
                                id,
                                content,
                                on_close_clone.as_ref(),
                            ) {
                                eprintln!("Save-and-close failed for note {id}: {error}");
                            }
                        }
                        WebviewToHostMessage::NewNoteRequested => {
                            on_new_note_clone();
                        }
                        WebviewToHostMessage::OpenExternalUrl { url } => {
                            if let Err(error) = validate_external_url(&url) {
                                eprintln!("Blocked external URL request: {error}");
                            } else if let Err(error) = gio::AppInfo::launch_default_for_uri(
                                &url,
                                Option::<&gio::AppLaunchContext>::None,
                            ) {
                                eprintln!("Failed to open approved external URL: {error}");
                            }
                        }
                        WebviewToHostMessage::DragStart => {}
                        WebviewToHostMessage::DragUpdate { dx, dy } => {
                            let mut st = state_clone.borrow_mut();
                            st.x += dx;
                            st.y += dy;
                            let (cx, cy, _, _) =
                                clamp_geometry(st.x, st.y, st.width, st.height, mon_w, mon_h);
                            st.x = cx;
                            st.y = cy;
                            if let Some(win) = window_weak.upgrade() {
                                update_window_position(win.upcast_ref(), cx, cy);
                            }
                        }
                        WebviewToHostMessage::DragEnd => {
                            let snapshot = state_clone.borrow().clone();
                            on_geom_clone(id, snapshot);
                        }
                        WebviewToHostMessage::ResizeStart => {}
                        WebviewToHostMessage::ResizeUpdate { dx, dy } => {
                            let mut st = state_clone.borrow_mut();
                            st.width += dx;
                            st.height += dy;
                            let (_, _, cw, ch) =
                                clamp_geometry(st.x, st.y, st.width, st.height, mon_w, mon_h);
                            st.width = cw;
                            st.height = ch;
                            if let Some(win) = window_weak.upgrade() {
                                update_window_size(win.upcast_ref(), cw, ch);
                            }
                        }
                        WebviewToHostMessage::ResizeEnd => {
                            let snapshot = state_clone.borrow().clone();
                            on_geom_clone(id, snapshot);
                        }
                        WebviewToHostMessage::FlushResponse {
                            id: message_id,
                            request_id,
                            content,
                        } => {
                            if !complete_flush_response(
                                &storage_clone,
                                &doc_clone,
                                id,
                                message_id,
                                request_id,
                                content,
                                &flushes_clone,
                            ) {
                                eprintln!(
                                    "Rejected inactive or invalid flush response for note {id} and request {request_id}"
                                );
                            }
                        }
                    }
                } else if let Err(error) = parse_webview_message(&raw_json) {
                    eprintln!("Rejected invalid webview message: {error}");
                }
            },
        );

        // Load frontend bundle
        let index_file = options.ui_dist_path.join("index.html");
        if index_file.exists() {
            let uri = file_uri_for_path(&index_file);
            webview.load_uri(&uri);
        } else {
            // Fallback for development if dist not present
            webview.load_html(
                "<html><body><p style='padding:20px;'>Note-it UI bundle not built yet. Run <code>pnpm build</code> in ui/</p></body></html>",
                Some("file:///"),
            );
        }

        window.set_child(Some(&webview));

        Self {
            id,
            window: window.upcast(),
            webview,
            document: doc_rc,
            state: state_rc,
            storage: options.storage,
            allow_close,
            pending_flushes,
        }
    }

    pub fn set_layer_mode(&self, mode: LayerMode) {
        apply_layer_mode(&self.window, mode);
    }

    #[allow(dead_code)]
    pub fn save_now(&self) -> Result<(), String> {
        let doc = self.document.borrow();
        self.storage.save_note_atomic(&doc).map(|_| ())
    }

    pub fn request_flush<F: FnOnce(Result<(), String>) + 'static>(
        &self,
        request_id: u64,
        callback: F,
    ) {
        let pending = Rc::clone(&self.pending_flushes);
        self.pending_flushes
            .borrow_mut()
            .insert(request_id, Box::new(callback));
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::RequestFlush { request_id },
        );

        glib::timeout_add_local_once(FLUSH_TIMEOUT, move || {
            expire_flush_request(&pending, request_id);
        });
    }

    pub fn close_after_save(&self) {
        self.allow_close.set(true);
        self.window.close();
    }
}

fn complete_flush_response(
    storage: &StorageManager,
    document: &Rc<RefCell<NoteDocument>>,
    expected_note_id: Uuid,
    message_note_id: Uuid,
    request_id: u64,
    content: String,
    pending_flushes: &PendingFlushes,
) -> bool {
    if message_note_id != expected_note_id {
        return false;
    }

    let callback = pending_flushes.borrow_mut().remove(&request_id);
    let Some(callback) = callback else {
        return false;
    };

    callback(save_content(storage, document, expected_note_id, content));
    true
}

fn expire_flush_request(pending_flushes: &PendingFlushes, request_id: u64) -> bool {
    let callback = pending_flushes.borrow_mut().remove(&request_id);
    let Some(callback) = callback else {
        return false;
    };

    callback(Err(FLUSH_TIMEOUT_ERROR.to_string()));
    true
}

fn save_content(
    storage: &StorageManager,
    document: &Rc<RefCell<NoteDocument>>,
    expected_id: Uuid,
    content: String,
) -> Result<(), String> {
    let mut doc = document.borrow_mut();
    if doc.metadata.id != expected_id {
        return Err("note identifier mismatch".to_string());
    }
    doc.content = content;
    doc.metadata.updated_at = chrono::Utc::now();
    storage.save_note_atomic(&doc).map(|_| ())
}

fn save_and_close(
    storage: &StorageManager,
    document: &Rc<RefCell<NoteDocument>>,
    expected_id: Uuid,
    content: String,
    finalize_close: &dyn Fn(Uuid) -> Result<(), String>,
) -> Result<(), String> {
    save_content(storage, document, expected_id, content)
        .map_err(|error| format!("note save failed: {error}"))?;
    finalize_close(expected_id).map_err(|error| format!("close finalization failed: {error}"))
}

fn file_uri_for_path(path: &Path) -> String {
    gio::File::for_path(path).uri().to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        complete_flush_response, expire_flush_request, file_uri_for_path, save_and_close,
        save_content, PendingFlushes, FLUSH_TIMEOUT_ERROR,
    };
    use crate::model::NoteDocument;
    use crate::storage::StorageManager;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::Path;
    use std::rc::Rc;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn failed_save_keeps_latest_content_in_memory() {
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let storage = StorageManager::with_custom_paths(
            notes_dir.clone(),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");
        fs::remove_dir(&notes_dir).expect("remove notes directory to force save failure");

        let document = Rc::new(RefCell::new(NoteDocument::new_empty()));
        let id = document.borrow().metadata.id;
        let result = save_content(&storage, &document, id, "latest text".to_string());

        assert!(result.is_err());
        assert_eq!(document.borrow().content, "latest text");
    }

    #[test]
    fn failed_save_does_not_finalize_close() {
        let tmp = tempdir().expect("tempdir");
        let notes_dir = tmp.path().join("notes");
        let storage = StorageManager::with_custom_paths(
            notes_dir.clone(),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");
        fs::remove_dir(&notes_dir).expect("remove notes directory to force save failure");

        let document = Rc::new(RefCell::new(NoteDocument::new_empty()));
        let id = document.borrow().metadata.id;
        let close_called = Cell::new(false);
        let result = save_and_close(
            &storage,
            &document,
            id,
            "unsaved latest text".to_string(),
            &|_| {
                close_called.set(true);
                Ok(())
            },
        );

        assert!(result.is_err());
        assert!(!close_called.get());
        assert_eq!(document.borrow().content, "unsaved latest text");
    }

    #[test]
    fn file_uri_encodes_paths_and_resolves_relative_paths() {
        let uri = file_uri_for_path(Path::new("directory with spaces/index.html"));
        assert!(uri.starts_with("file:///"));
        assert!(uri.ends_with("directory%20with%20spaces/index.html"));
    }

    #[test]
    fn successful_flush_uses_latest_webview_content() {
        let tmp = tempdir().expect("tempdir");
        let storage = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");

        let document = Rc::new(RefCell::new(NoteDocument::new_empty()));
        let id = document.borrow().metadata.id;

        let completion = Rc::new(RefCell::new(None));
        let completion_clone = Rc::clone(&completion);
        let pending: PendingFlushes = Rc::new(RefCell::new(std::collections::HashMap::new()));
        pending.borrow_mut().insert(
            42,
            Box::new(move |result| *completion_clone.borrow_mut() = Some(result)),
        );

        let accepted = complete_flush_response(
            &storage,
            &document,
            id,
            id,
            42,
            "# flushed immediately before hide".to_string(),
            &pending,
        );

        assert!(accepted);
        assert!(completion.borrow().as_ref().expect("completion").is_ok());
        assert_eq!(
            document.borrow().content,
            "# flushed immediately before hide"
        );

        // Verify disk file was created and contains the content
        let loaded = storage.load_note(&id).expect("loaded note");
        assert_eq!(loaded.content, "# flushed immediately before hide");
    }

    #[test]
    fn flush_timeout_returns_error() {
        let tmp = tempdir().expect("tempdir");
        let storage = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");

        let mut confirmed = NoteDocument::new_empty();
        confirmed.content = "last confirmed on disk".to_string();
        storage.save_note_atomic(&confirmed).expect("initial save");
        let id = confirmed.metadata.id;
        let document = Rc::new(RefCell::new(confirmed));
        document.borrow_mut().content = "potentially stale in memory".to_string();

        let completion = Rc::new(RefCell::new(None));
        let completion_clone = Rc::clone(&completion);
        let pending: PendingFlushes = Rc::new(RefCell::new(std::collections::HashMap::new()));
        pending.borrow_mut().insert(
            7,
            Box::new(move |result| *completion_clone.borrow_mut() = Some(result)),
        );

        assert!(expire_flush_request(&pending, 7));
        assert_eq!(
            completion
                .borrow()
                .as_ref()
                .expect("completion")
                .as_ref()
                .expect_err("timeout must fail"),
            FLUSH_TIMEOUT_ERROR
        );
        assert!(pending.borrow().is_empty());
        assert_eq!(
            storage.load_note(&id).expect("disk content").content,
            "last confirmed on disk"
        );
    }

    #[test]
    fn stale_flush_response_is_rejected() {
        let tmp = tempdir().expect("tempdir");
        let storage = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");

        let mut current = NoteDocument::new_empty();
        current.content = "newest content".to_string();
        storage.save_note_atomic(&current).expect("initial save");
        let id = current.metadata.id;
        let document = Rc::new(RefCell::new(current));
        let pending: PendingFlushes = Rc::new(RefCell::new(std::collections::HashMap::new()));
        pending.borrow_mut().insert(999, Box::new(|_| {}));
        assert!(expire_flush_request(&pending, 999));

        let accepted = complete_flush_response(
            &storage,
            &document,
            id,
            id,
            999,
            "stale content".to_string(),
            &pending,
        );

        assert!(!accepted);
        assert_eq!(document.borrow().content, "newest content");
        assert_eq!(
            storage.load_note(&id).expect("disk content").content,
            "newest content"
        );
    }

    #[test]
    fn mismatched_note_id_is_rejected() {
        let tmp = tempdir().expect("tempdir");
        let storage = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");

        let mut current = NoteDocument::new_empty();
        current.content = "current content".to_string();
        storage.save_note_atomic(&current).expect("initial save");
        let id = current.metadata.id;
        let document = Rc::new(RefCell::new(current));
        let pending: PendingFlushes = Rc::new(RefCell::new(std::collections::HashMap::new()));
        pending.borrow_mut().insert(12, Box::new(|_| {}));

        let accepted = complete_flush_response(
            &storage,
            &document,
            id,
            Uuid::new_v4(),
            12,
            "wrong note content".to_string(),
            &pending,
        );

        assert!(!accepted);
        assert!(pending.borrow().contains_key(&12));
        assert_eq!(document.borrow().content, "current content");
        assert_eq!(
            storage.load_note(&id).expect("disk content").content,
            "current content"
        );
    }

    #[test]
    fn ctrl_w_continues_working_normally() {
        let tmp = tempdir().expect("tempdir");
        let storage = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");
        let document = Rc::new(RefCell::new(NoteDocument::new_empty()));
        let id = document.borrow().metadata.id;
        let closed = Cell::new(false);

        let result = save_and_close(
            &storage,
            &document,
            id,
            "latest Ctrl+W content".to_string(),
            &|closed_id| {
                assert_eq!(closed_id, id);
                closed.set(true);
                Ok(())
            },
        );

        assert!(result.is_ok());
        assert!(closed.get());
        assert_eq!(
            storage.load_note(&id).expect("saved note").content,
            "latest Ctrl+W content"
        );
    }
}
