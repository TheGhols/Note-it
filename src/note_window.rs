use crate::layer_shell::{apply_layer_mode, setup_layer_shell_window};
use crate::model::NoteDocument;
use crate::state::{LayerMode, NoteWindowState};
use crate::storage::StorageManager;
use crate::webview_bridge::{
    parse_webview_message, send_to_webview, HostToWebviewMessage, WebviewToHostMessage,
};
use gtk4::prelude::*;
use std::cell::RefCell;
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
    pub on_close: Rc<dyn Fn(Uuid)>,
}

#[allow(dead_code)]
pub struct NoteWindow {
    pub id: Uuid,
    pub window: gtk4::Window,
    pub webview: WebView,
    pub document: Rc<RefCell<NoteDocument>>,
    pub state: Rc<RefCell<NoteWindowState>>,
    pub storage: StorageManager,
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

        let content_manager = UserContentManager::new();
        content_manager.register_script_message_handler("noteItHost", None);

        let webview = WebView::builder()
            .settings(&settings)
            .user_content_manager(&content_manager)
            .build();

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
                        WebviewToHostMessage::ContentChanged { content, .. } => {
                            let mut doc = doc_clone.borrow_mut();
                            doc.content = content;
                            doc.metadata.updated_at = chrono::Utc::now();
                            let _ = storage_clone.save_note_atomic(&doc);
                        }
                        WebviewToHostMessage::ColorChanged { color, .. } => {
                            let mut doc = doc_clone.borrow_mut();
                            doc.metadata.color = color;
                            doc.metadata.updated_at = chrono::Utc::now();
                            let _ = storage_clone.save_note_atomic(&doc);
                        }
                        WebviewToHostMessage::FontSizeChanged { font_size, .. } => {
                            let mut doc = doc_clone.borrow_mut();
                            doc.metadata.font_size = font_size;
                            doc.metadata.updated_at = chrono::Utc::now();
                            let _ = storage_clone.save_note_atomic(&doc);
                        }
                        WebviewToHostMessage::CloseRequested { id } => {
                            on_close_clone(id);
                        }
                        WebviewToHostMessage::NewNoteRequested => {
                            on_new_note_clone();
                        }
                        WebviewToHostMessage::OpenExternalUrl { url } => {
                            let _ = gio::AppInfo::launch_default_for_uri(
                                &url,
                                Option::<&gio::AppLaunchContext>::None,
                            );
                        }
                    }
                }
            },
        );

        // Load frontend bundle
        let index_file = options.ui_dist_path.join("index.html");
        if index_file.exists() {
            let uri = format!("file://{}", index_file.display());
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
        }
    }

    pub fn set_layer_mode(&self, mode: LayerMode) {
        apply_layer_mode(&self.window, mode);
    }

    pub fn save_now(&self) {
        let doc = self.document.borrow();
        let _ = self.storage.save_note_atomic(&doc);
    }
}
