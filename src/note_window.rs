use crate::layer_shell::{apply_layer_mode, setup_layer_shell_window};
use crate::model::NoteDocument;
use crate::state::{LayerMode, NoteWindowState};
use crate::storage::StorageManager;
use crate::webview_bridge::{
    parse_webview_message, send_to_webview, validate_external_url, HostToWebviewMessage,
    WebviewToHostMessage,
};
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
    pub on_new_note: Rc<dyn Fn()>,
    pub on_close: Rc<dyn Fn(Uuid) -> Result<(), String>>,
}

#[allow(dead_code)]
pub struct NoteWindow {
    pub id: Uuid,
    pub window: gtk4::Window,
    pub webview: WebView,
    pub document: Rc<RefCell<NoteDocument>>,
    pub state: Rc<RefCell<NoteWindowState>>,
    pub storage: StorageManager,
    allow_close: Rc<Cell<bool>>,
}

impl NoteWindow {
    pub fn new(options: NoteWindowOptions) -> Self {
        let id = options.document.metadata.id;
        let window = gtk4::ApplicationWindow::builder()
            .application(options.app)
            .title("Note-it")
            .decorated(false)
            .build();

        let doc_rc = Rc::new(RefCell::new(options.document));
        let state_rc = Rc::new(RefCell::new(options.state.clone()));

        // Setup Layer Shell
        setup_layer_shell_window(
            window.upcast_ref(),
            options.layer_mode,
            options.state.x,
            options.state.y,
            options.state.width,
            options.state.height,
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

        // Connect Webview Messages
        let doc_clone = Rc::clone(&doc_rc);
        let storage_clone = options.storage.clone();
        let webview_weak = webview.downgrade();
        let on_new_note_clone = Rc::clone(&options.on_new_note);
        let on_close_clone = Rc::clone(&options.on_close);

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
        }
    }

    pub fn set_layer_mode(&self, mode: LayerMode) {
        apply_layer_mode(&self.window, mode);
    }

    pub fn save_now(&self) -> Result<(), String> {
        let doc = self.document.borrow();
        self.storage.save_note_atomic(&doc).map(|_| ())
    }

    pub fn close_after_save(&self) {
        self.allow_close.set(true);
        self.window.close();
    }
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
    use super::{file_uri_for_path, save_and_close, save_content};
    use crate::model::NoteDocument;
    use crate::storage::StorageManager;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::Path;
    use std::rc::Rc;
    use tempfile::tempdir;

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
}
