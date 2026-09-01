use crate::layer_shell::{
    apply_live_layer_mode, apply_paper_color, clamp_geometry_with_min_height,
    collapsed_note_height, min_note_height_for_scale, setup_layer_shell_window,
    show_initial_layer_surface, update_window_position, update_window_size, WindowGeometry,
    DEFAULT_MONITOR_HEIGHT, DEFAULT_MONITOR_WIDTH,
};
use crate::webview_bridge::{
    parse_webview_message, send_to_webview, send_to_webview_confirmed, validate_external_url,
    HostToWebviewMessage, MetadataView, WebviewToHostMessage,
};
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use noteit_core::autopaste::CaptureDelimiter;
use noteit_core::diagnostics;
use noteit_core::metadata::NoteMetadata;
use noteit_core::model::{paper_intensity_name, paper_type_name, NoteDocument, NoteFrontMatter};
use noteit_core::settings::{clamp_ui_scale_percent, theme_name};
use noteit_core::state::{clamp_zoom_percent, LayerMode, NoteWindowState};
use noteit_core::storage::StorageManager;
use noteit_core::study::Rating;
use noteit_core::timer::TimerFinishKind;
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
    pub on_toggle_layer_mode: Rc<dyn Fn()>,
    /// Interface theme in force when the note opens. Shared by every note, so
    /// the window is told about it rather than reading it back from the store.
    pub theme: String,
    pub on_theme_changed: Rc<dyn Fn(String)>,
    /// Application-chrome scale in force when this note opens.
    pub ui_scale_percent: u16,
    pub on_ui_scale_changed: Rc<dyn Fn(u16)>,
    /// Asks the host to search every note. Carries the identifier of the note
    /// that asked, so the answer goes back to the page that is waiting for it.
    pub on_search: Rc<dyn Fn(Uuid, u64, String)>,
    /// Asks the host to bring a search result to the front: the note that
    /// asked, the note to reveal, and what was being looked for.
    pub on_open_search_result: Rc<dyn Fn(Uuid, Uuid, String)>,
    /// Asks the host to move this note to the trash. The reader has already
    /// confirmed; the host still flushes before anything is moved.
    pub on_trash_note: Rc<dyn Fn(Uuid)>,
    /// Asks the host for the contents of the trash: the note that asked and
    /// the number the answer must carry back.
    pub on_list_trash: Rc<dyn Fn(Uuid, u64)>,
    /// Asks the host to restore one note: the note that asked, and the note to
    /// bring back.
    pub on_restore_note: Rc<dyn Fn(Uuid, Uuid)>,
    /// Asks the host for a snapshot now, and to answer the note that asked.
    pub on_backup: Rc<dyn Fn(Uuid)>,
    /// Requests every live/stored note document plus the study metadata.
    pub on_study_catalog: Rc<dyn Fn(Uuid, u64)>,
    /// Requests one host-timed, atomically persisted rating.
    pub on_study_rating: Rc<dyn Fn(Uuid, u64, String, Rating)>,
    /// One run reached zero. The host posts the notification; the page only
    /// says which kind of run it was.
    pub on_timer_finished: Rc<dyn Fn(Uuid, TimerFinishKind)>,
    /// How a capture is laid out. Application-wide, so the window is told it
    /// rather than reading it back from the store.
    pub capture_delimiter: CaptureDelimiter,
    /// Asks the host to make this note the AutoPaste target, or to stop.
    pub on_autopaste_requested: Rc<dyn Fn(Uuid, bool)>,
    /// Asks the host to store a different capture delimiter.
    pub on_capture_delimiter_changed: Rc<dyn Fn(CaptureDelimiter)>,
    /// Asks the host to show a file chooser and import the chosen image.
    pub on_insert_image: Rc<dyn Fn(Uuid)>,
    /// Bytes of an image the reader pasted or dropped, base64 on the wire.
    pub on_image_bytes: Rc<dyn Fn(Uuid, String)>,
}

type FlushCallback = Box<dyn FnOnce(Result<(), String>)>;
type PendingFlushes = Rc<RefCell<std::collections::HashMap<u64, FlushCallback>>>;
/// What the host is waiting for while an external write holds the editor.
type ExternalWriteCallback = Box<dyn FnOnce(Result<String, String>)>;
type PendingExternalWrite = Rc<RefCell<Option<(Uuid, ExternalWriteCallback)>>>;

const FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
const FLUSH_TIMEOUT_ERROR: &str = "timed out waiting for latest WebView content";

/// How long the page is given to stop editing and hand back its text.
///
/// Longer than the ordinary flush, because this one has a person's unsaved
/// paragraph in it and giving up on that is expensive. Bounded all the same: a
/// page that never answers must not leave the editor held for ever, and it
/// must not leave the writer that asked waiting for ever either.
const EXTERNAL_WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(4000);
const EXTERNAL_WRITE_TIMEOUT_ERROR: &str =
    "a nota aberta não respondeu a tempo. Nada foi alterado.";
const CLICK_FOCUS_RESTORE_DELAY: std::time::Duration = std::time::Duration::from_millis(60);

#[derive(Default)]
struct SubpixelDeltaAccumulator {
    remainder_x: f64,
    remainder_y: f64,
}

impl SubpixelDeltaAccumulator {
    fn reset(&mut self) {
        self.remainder_x = 0.0;
        self.remainder_y = 0.0;
    }

    fn consume(&mut self, dx: f64, dy: f64) -> (i32, i32) {
        self.remainder_x += dx;
        self.remainder_y += dy;
        let pixel_x = self.remainder_x.trunc() as i32;
        let pixel_y = self.remainder_y.trunc() as i32;
        self.remainder_x -= f64::from(pixel_x);
        self.remainder_y -= f64::from(pixel_y);
        (pixel_x, pixel_y)
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct NoteWindow {
    pub id: Uuid,
    pub window: gtk4::Window,
    pub webview: WebView,
    pub document: Rc<RefCell<NoteDocument>>,
    pub state: Rc<RefCell<NoteWindowState>>,
    pub storage: StorageManager,
    monitor_size: (i32, i32),
    layer_mode: Rc<Cell<LayerMode>>,
    layer_transition_generation: Rc<Cell<u64>>,
    theme: Rc<RefCell<String>>,
    ui_scale_percent: Rc<Cell<u16>>,
    allow_close: Rc<Cell<bool>>,
    pending_flushes: PendingFlushes,
    /// Which run of this note's document the page is currently on.
    ///
    /// Starts at zero when the note is loaded and goes up by one every time
    /// something outside the window commits a change. Every message from the
    /// page that carries content quotes the generation it was composed
    /// against, and one quoting an older number is refused — that is the whole
    /// mechanism that stops an autosave already in flight from writing over a
    /// commit that has just landed.
    ///
    /// Runtime only: it is never stored, never in the front matter, and
    /// meaningless once the window is gone.
    external_generation: Rc<Cell<u64>>,
    /// The external write this window is currently holding still for.
    ///
    /// At most one at a time. Two of them would each snapshot the same text
    /// and the second commit would silently undo the first.
    pending_external_write: PendingExternalWrite,
    /// False until the page has said `Ready` and been handed its note. A
    /// message that assumes a loaded document has to wait for this.
    loaded: Rc<Cell<bool>>,
    /// The layout preference this note was last told about, so a reload sends
    /// the current one rather than the one the window was built with.
    capture_delimiter: Rc<Cell<CaptureDelimiter>>,
    /// A match to reveal once the page is loaded. A note opened *by* a search
    /// is told what to look for before it exists, so the request waits here
    /// rather than being sent into a page that is about to replace its
    /// content.
    pending_reveal: Rc<RefCell<Option<String>>>,
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

        // Clamp initial geometry. A note restored collapsed keeps the header
        // bar height instead of being forced back to the expanded minimum.
        let restored_collapsed = options.state.collapsed;
        let ui_scale_percent = clamp_ui_scale_percent(options.ui_scale_percent);
        let (clamped_x, clamped_y, clamped_w, clamped_h) = clamp_geometry_with_min_height(
            options.state.x,
            options.state.y,
            options.state.width,
            options.state.height,
            mon_w,
            mon_h,
            min_note_height_for_scale(restored_collapsed, ui_scale_percent),
        );

        let mut initial_state = options.state;
        initial_state.x = clamped_x;
        initial_state.y = clamped_y;
        initial_state.width = clamped_w;
        initial_state.height = clamped_h;
        initial_state.zoom_percent = clamp_zoom_percent(initial_state.zoom_percent);
        if options.monitor_name.is_some() {
            initial_state.monitor = options.monitor_name;
        }

        let doc_rc = Rc::new(RefCell::new(options.document));
        let state_rc = Rc::new(RefCell::new(initial_state));
        let layer_mode_cell = Rc::new(Cell::new(options.layer_mode));
        let theme_cell = Rc::new(RefCell::new(theme_name(&options.theme).to_string()));
        let ui_scale_cell = Rc::new(Cell::new(ui_scale_percent));

        // Back the surface with the note's paper colour so a fast resize never
        // exposes the default dark window background before the page repaints.
        apply_paper_color(window.upcast_ref(), &doc_rc.borrow().metadata.color);

        // Setup Layer Shell
        setup_layer_shell_window(
            window.upcast_ref(),
            options.layer_mode,
            WindowGeometry {
                x: clamped_x,
                y: clamped_y,
                width: clamped_w,
                height: clamped_h,
                collapsed: restored_collapsed,
            },
            options.monitor.as_ref(),
            ui_scale_percent,
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
        let on_toggle_layer_clone = Rc::clone(&options.on_toggle_layer_mode);
        let layer_mode_clone = Rc::clone(&layer_mode_cell);
        let theme_clone = Rc::clone(&theme_cell);
        let on_theme_changed_clone = Rc::clone(&options.on_theme_changed);
        let ui_scale_clone = Rc::clone(&ui_scale_cell);
        let on_ui_scale_changed_clone = Rc::clone(&options.on_ui_scale_changed);
        let on_search_clone = Rc::clone(&options.on_search);
        let on_open_search_result_clone = Rc::clone(&options.on_open_search_result);
        let on_trash_note_clone = Rc::clone(&options.on_trash_note);
        let on_list_trash_clone = Rc::clone(&options.on_list_trash);
        let on_restore_note_clone = Rc::clone(&options.on_restore_note);
        let on_backup_clone = Rc::clone(&options.on_backup);
        let on_study_catalog_clone = Rc::clone(&options.on_study_catalog);
        let on_study_rating_clone = Rc::clone(&options.on_study_rating);
        let on_timer_finished_clone = Rc::clone(&options.on_timer_finished);
        let on_autopaste_requested_clone = Rc::clone(&options.on_autopaste_requested);
        let on_capture_delimiter_clone = Rc::clone(&options.on_capture_delimiter_changed);
        let on_insert_image_clone = Rc::clone(&options.on_insert_image);
        let on_image_bytes_clone = Rc::clone(&options.on_image_bytes);
        let capture_delimiter_cell = Rc::new(Cell::new(options.capture_delimiter));
        let capture_delimiter_clone = Rc::clone(&capture_delimiter_cell);
        let loaded = Rc::new(Cell::new(false));
        let pending_reveal: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let loaded_clone = Rc::clone(&loaded);
        let pending_reveal_clone = Rc::clone(&pending_reveal);
        let external_generation = Rc::new(Cell::new(0u64));
        let generation_clone = Rc::clone(&external_generation);
        let pending_external_write: PendingExternalWrite = Rc::new(RefCell::new(None));
        let pending_external_clone = Rc::clone(&pending_external_write);
        let drag_deltas = Rc::new(RefCell::new(SubpixelDeltaAccumulator::default()));
        let resize_deltas = Rc::new(RefCell::new(SubpixelDeltaAccumulator::default()));

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
                                        paper_type: paper_type_name(&doc.metadata.paper_type)
                                            .to_string(),
                                        paper_intensity: paper_intensity_name(
                                            &doc.metadata.paper_intensity,
                                        )
                                        .to_string(),
                                        font_size: doc.metadata.font_size,
                                        metadata: MetadataView::from(&doc.user_metadata),
                                        collapsed: state_clone.borrow().collapsed,
                                        created_at: doc.metadata.created_at,
                                        updated_at: doc.metadata.updated_at,
                                        zoom_percent: state_clone.borrow().zoom_percent,
                                        layer_mode: layer_mode_clone.get().as_str().to_string(),
                                        theme: theme_clone.borrow().clone(),
                                        ui_scale_percent: ui_scale_clone.get(),
                                        timer: state_clone.borrow().timer,
                                        capture_delimiter: capture_delimiter_clone.get(),
                                        generation: generation_clone.get(),
                                    },
                                );
                                loaded_clone.set(true);
                                // A note opened by a search was told what to
                                // look for before it had a document. Now it
                                // has one.
                                if let Some(query) = pending_reveal_clone.borrow_mut().take() {
                                    send_to_webview(
                                        &wv,
                                        &HostToWebviewMessage::RevealMatch { query },
                                    );
                                }
                            }
                        }
                        WebviewToHostMessage::ContentChanged {
                            id: message_id,
                            content,
                            generation,
                        } => {
                            if message_id != id {
                                eprintln!("Autosave rejected a mismatched note identifier");
                            } else if !accepts_generation(&generation_clone, generation, "autosave")
                            {
                                // Composed against a document that has since
                                // been replaced from outside this window.
                                // Writing it would undo that change.
                            } else if let Err(error) =
                                save_content(&storage_clone, &doc_clone, id, content)
                            {
                                eprintln!("Autosave failed for note {id}: {error}");
                            } else if let Some(wv) = webview_weak.upgrade() {
                                let doc = doc_clone.borrow();
                                send_to_webview(
                                    &wv,
                                    &HostToWebviewMessage::SetTimestamps {
                                        created_at: doc.metadata.created_at,
                                        updated_at: doc.metadata.updated_at,
                                    },
                                );
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
                            // Keep the surface backing in step with the page,
                            // so the next resize cannot flash the previous
                            // paper colour. This mirrors what the note already
                            // shows, not what is stored, so it stands whether
                            // or not the save below goes through.
                            if let Some(win) = window_weak.upgrade() {
                                apply_paper_color(win.upcast_ref(), &color);
                            }
                            if let Err(error) =
                                save_metadata(&storage_clone, &doc_clone, id, |metadata| {
                                    metadata.color = color;
                                })
                            {
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
                            if let Err(error) =
                                save_metadata(&storage_clone, &doc_clone, id, |metadata| {
                                    metadata.font_size = font_size;
                                })
                            {
                                eprintln!("Font size save failed for note {id}: {error}");
                            }
                        }
                        WebviewToHostMessage::PaperChanged {
                            id: message_id,
                            paper_type,
                            paper_intensity,
                        } => {
                            if message_id != id {
                                eprintln!("Paper save rejected a mismatched note identifier");
                                return;
                            }
                            // Appearance only: the note is saved without its
                            // modification date moving, and whatever the page
                            // sends is resolved against the supported set
                            // before it reaches the note file.
                            if let Err(error) =
                                save_metadata(&storage_clone, &doc_clone, id, |metadata| {
                                    metadata.paper_type = paper_type_name(&paper_type).to_string();
                                    metadata.paper_intensity =
                                        paper_intensity_name(&paper_intensity).to_string();
                                })
                            {
                                eprintln!("Paper save failed for note {id}: {error}");
                            }
                        }
                        WebviewToHostMessage::MetadataCatalogRequested { request_id } => {
                            if let Some(wv) = webview_weak.upgrade() {
                                send_to_webview(
                                    &wv,
                                    &HostToWebviewMessage::MetadataCatalogResult {
                                        request_id,
                                        catalog: storage_clone.metadata_catalog(),
                                    },
                                );
                            }
                        }
                        WebviewToHostMessage::MetadataSuggestionsRequested {
                            request_id,
                            kind,
                            query,
                        } => {
                            let catalog = storage_clone.metadata_catalog();
                            let suggestions = match kind {
                                crate::webview_bridge::MetadataSuggestionKind::Tag => {
                                    catalog.tag_suggestions(&query)
                                }
                                crate::webview_bridge::MetadataSuggestionKind::PropertyKey => {
                                    catalog.property_key_suggestions(&query)
                                }
                            };
                            if let Some(wv) = webview_weak.upgrade() {
                                send_to_webview(
                                    &wv,
                                    &HostToWebviewMessage::MetadataSuggestionsResult {
                                        request_id,
                                        suggestions,
                                    },
                                );
                            }
                        }
                        WebviewToHostMessage::MetadataChanged {
                            request_id,
                            id: message_id,
                            content,
                            generation,
                            tags,
                            properties,
                        } => {
                            let result = if message_id != id {
                                Err("a nota da solicitação não corresponde à janela".to_string())
                            } else if !accepts_generation(
                                &generation_clone,
                                generation,
                                "metadata save",
                            ) {
                                // The body travelling with this metadata is
                                // older than the note is. Applying it would
                                // bring the previous text back with the tag.
                                Err("a nota mudou; reabra o painel de metadados".to_string())
                            } else {
                                NoteMetadata::try_new(tags, properties)
                                    .map_err(|error| error.to_string())
                                    .and_then(|metadata| {
                                        save_user_metadata(
                                            &storage_clone,
                                            &doc_clone,
                                            id,
                                            content,
                                            metadata,
                                        )
                                    })
                            };
                            if let Some(wv) = webview_weak.upgrade() {
                                let metadata = MetadataView::from(&doc_clone.borrow().user_metadata);
                                send_to_webview(
                                    &wv,
                                    &HostToWebviewMessage::MetadataSaveResult {
                                        request_id,
                                        ok: result.is_ok(),
                                        message: result
                                            .err()
                                            .unwrap_or_else(|| "Metadados salvos".to_string()),
                                        metadata,
                                    },
                                );
                            }
                        }
                        WebviewToHostMessage::ThemeChanged { theme } => {
                            on_theme_changed_clone(theme_name(&theme).to_string());
                        }
                        WebviewToHostMessage::UiScaleChanged { ui_scale_percent } => {
                            on_ui_scale_changed_clone(clamp_ui_scale_percent(ui_scale_percent));
                        }
                        WebviewToHostMessage::SaveAndClose {
                            id: message_id,
                            content,
                            generation,
                        } => {
                            if message_id != id {
                                eprintln!("Save-and-close rejected a mismatched note identifier");
                            } else if !accepts_generation(
                                &generation_clone,
                                generation,
                                "save-and-close",
                            ) {
                                // The window still closes; what it must not do
                                // is take an older body down with it.
                                if let Err(error) = on_close_clone(id) {
                                    eprintln!("Close finalization failed for note {id}: {error}");
                                }
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
                        WebviewToHostMessage::CollapseChanged {
                            id: message_id,
                            collapsed,
                        } => {
                            if message_id != id {
                                eprintln!("Collapse request rejected a mismatched note identifier");
                                return;
                            }
                            let snapshot = {
                                let mut st = state_clone.borrow_mut();
                                if !apply_collapse_to_state(
                                    &mut st,
                                    collapsed,
                                    mon_w,
                                    mon_h,
                                    ui_scale_clone.get(),
                                ) {
                                    return;
                                }
                                st.clone()
                            };
                            if let Some(win) = window_weak.upgrade() {
                                update_window_size(
                                    win.upcast_ref(),
                                    snapshot.width,
                                    snapshot.height,
                                    snapshot.collapsed,
                                    ui_scale_clone.get(),
                                );
                            }
                            on_geom_clone(id, snapshot);
                        }
                        WebviewToHostMessage::ZoomChanged {
                            id: message_id,
                            zoom_percent,
                        } => {
                            if message_id != id {
                                eprintln!("Zoom change rejected a mismatched note identifier");
                                return;
                            }
                            let snapshot = {
                                let mut st = state_clone.borrow_mut();
                                let clamped = clamp_zoom_percent(zoom_percent);
                                if st.zoom_percent == clamped {
                                    return;
                                }
                                st.zoom_percent = clamped;
                                st.clone()
                            };
                            // A view preference: persisted with the window
                            // state, never written to the note document.
                            on_geom_clone(id, snapshot);
                        }
                        WebviewToHostMessage::ToggleLayerMode => {
                            on_toggle_layer_clone();
                        }
                        WebviewToHostMessage::SearchRequested { request_id, query } => {
                            on_search_clone(id, request_id, query);
                        }
                        WebviewToHostMessage::OpenSearchResult { note_id, query } => {
                            on_open_search_result_clone(id, note_id, query);
                        }
                        WebviewToHostMessage::TrashNoteRequested { id: message_id } => {
                            // A note can only ask for its own deletion. The
                            // page has no way to name another one here, and a
                            // mismatched identifier is a message this window
                            // will not act on.
                            if message_id != id {
                                eprintln!("Trash request rejected a mismatched note identifier");
                                return;
                            }
                            on_trash_note_clone(id);
                        }
                        WebviewToHostMessage::TrashListRequested { request_id } => {
                            on_list_trash_clone(id, request_id);
                        }
                        WebviewToHostMessage::RestoreNoteRequested { note_id } => {
                            on_restore_note_clone(id, note_id);
                        }
                        WebviewToHostMessage::BackupRequested => {
                            on_backup_clone(id);
                        }
                        WebviewToHostMessage::StudyCatalogRequested { request_id } => {
                            on_study_catalog_clone(id, request_id);
                        }
                        WebviewToHostMessage::StudyRatingRequested {
                            request_id,
                            review_key,
                            rating,
                        } => {
                            on_study_rating_clone(id, request_id, review_key, rating);
                        }
                        WebviewToHostMessage::TimerChanged {
                            id: message_id,
                            timer,
                        } => {
                            if message_id != id {
                                eprintln!("Timer change rejected a mismatched note identifier");
                                return;
                            }
                            let snapshot = {
                                let mut st = state_clone.borrow_mut();
                                // Whatever the page sends is resolved against
                                // the supported shape before it is stored, the
                                // same way a zoom is.
                                let sanitized = timer.and_then(|timer| timer.sanitize());
                                if st.timer == sanitized {
                                    return;
                                }
                                st.timer = sanitized;
                                st.clone()
                            };
                            // Operational state, persisted with the window
                            // geometry. The note document is not opened, not
                            // written and not dated by any of this.
                            on_geom_clone(id, snapshot);
                        }
                        WebviewToHostMessage::AutoPasteRequested {
                            id: message_id,
                            active,
                        } => {
                            // A note can only ask about its own capture. The
                            // host owns the single target, so what comes back
                            // is a `SetAutoPaste` to every note affected —
                            // nothing here assumes the request was granted.
                            if message_id != id {
                                eprintln!("AutoPaste request rejected a mismatched note identifier");
                                return;
                            }
                            on_autopaste_requested_clone(id, active);
                        }
                        WebviewToHostMessage::InsertImageRequested { id: message_id } => {
                            if message_id != id {
                                eprintln!("Image request rejected a mismatched note identifier");
                                return;
                            }
                            on_insert_image_clone(id);
                        }
                        WebviewToHostMessage::ImageBytesReceived {
                            id: message_id,
                            data,
                        } => {
                            if message_id != id {
                                eprintln!("Image bytes rejected a mismatched note identifier");
                                return;
                            }
                            on_image_bytes_clone(id, data);
                        }
                        WebviewToHostMessage::CaptureDelimiterChanged { delimiter } => {
                            on_capture_delimiter_clone(delimiter);
                        }
                        WebviewToHostMessage::TimerFinished {
                            id: message_id,
                            kind,
                        } => {
                            if message_id != id {
                                eprintln!("Timer completion rejected a mismatched note identifier");
                                return;
                            }
                            on_timer_finished_clone(id, kind);
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
                        WebviewToHostMessage::DragStart => {
                            drag_deltas.borrow_mut().reset();
                        }
                        WebviewToHostMessage::DragUpdate { dx, dy } => {
                            let (dx, dy) = drag_deltas.borrow_mut().consume(dx, dy);
                            if dx == 0 && dy == 0 {
                                return;
                            }
                            let mut st = state_clone.borrow_mut();
                            st.x += dx;
                            st.y += dy;
                            let (cx, cy, _, _) = clamp_geometry_with_min_height(
                                st.x,
                                st.y,
                                st.width,
                                st.height,
                                mon_w,
                                mon_h,
                                min_note_height_for_scale(st.collapsed, ui_scale_clone.get()),
                            );
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
                        WebviewToHostMessage::ResizeStart => {
                            resize_deltas.borrow_mut().reset();
                        }
                        WebviewToHostMessage::ResizeUpdate { dx, dy } => {
                            let (dx, dy) = resize_deltas.borrow_mut().consume(dx, dy);
                            if dx == 0 && dy == 0 {
                                return;
                            }
                            let mut st = state_clone.borrow_mut();
                            // A collapsed note is only a header bar; resizing it
                            // would produce an incoherent expanded geometry.
                            if st.collapsed {
                                return;
                            }
                            st.width += dx;
                            st.height += dy;
                            let (_, _, cw, ch) = clamp_geometry_with_min_height(
                                st.x,
                                st.y,
                                st.width,
                                st.height,
                                mon_w,
                                mon_h,
                                min_note_height_for_scale(false, ui_scale_clone.get()),
                            );
                            st.width = cw;
                            st.height = ch;
                            if let Some(win) = window_weak.upgrade() {
                                update_window_size(
                                    win.upcast_ref(),
                                    cw,
                                    ch,
                                    false,
                                    ui_scale_clone.get(),
                                );
                            }
                        }
                        WebviewToHostMessage::ResizeEnd => {
                            let snapshot = state_clone.borrow().clone();
                            if snapshot.collapsed {
                                return;
                            }
                            on_geom_clone(id, snapshot);
                        }
                        WebviewToHostMessage::FlushResponse {
                            id: message_id,
                            request_id,
                            content,
                            generation,
                        } => {
                            if !accepts_generation(&generation_clone, generation, "flush") {
                                // A flush answer from a superseded run of the
                                // document. The request is still resolved, so
                                // whatever is waiting on it is not left
                                // hanging — it is simply told, rather than
                                // handed an old body to write.
                                if let Some(callback) =
                                    flushes_clone.borrow_mut().remove(&request_id)
                                {
                                    callback(Err(
                                        "a nota mudou enquanto o texto era recolhido".to_string()
                                    ));
                                }
                            } else if !complete_flush_response(
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
                        WebviewToHostMessage::ExternalWriteReady {
                            id: message_id,
                            request_id,
                            generation,
                            content,
                        } => {
                            if !settle_external_write(
                                &pending_external_clone,
                                &generation_clone,
                                id,
                                message_id,
                                request_id,
                                generation,
                                content,
                            ) {
                                // Either the host already gave up on this
                                // request, or the answer belongs to another
                                // note or another run of this one. A host that
                                // has declared a timeout must never be able to
                                // commit the late answer it timed out on.
                                eprintln!(
                                    "Rejected a stale or unknown external-write answer for note {id}"
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

        let mapped_webview = webview.downgrade();
        window.connect_map(move |win| {
            let webview_focus = mapped_webview
                .upgrade()
                .map(|webview| webview.has_focus())
                .unwrap_or(false);
            diagnostics::log(format_args!(
                "event=map note={} layer={:?} keyboard={:?} active={} webview_focus={} visible={} mapped={}",
                id,
                win.layer(),
                win.keyboard_mode(),
                win.is_active(),
                webview_focus,
                win.is_visible(),
                win.is_mapped()
            ));
        });
        let unmapped_webview = webview.downgrade();
        window.connect_unmap(move |win| {
            let webview_focus = unmapped_webview
                .upgrade()
                .map(|webview| webview.has_focus())
                .unwrap_or(false);
            diagnostics::log(format_args!(
                "event=unmap note={} layer={:?} keyboard={:?} active={} webview_focus={} visible={} mapped={}",
                id,
                win.layer(),
                win.keyboard_mode(),
                win.is_active(),
                webview_focus,
                win.is_visible(),
                win.is_mapped()
            ));
        });

        // Keep the page as the window's focus widget whenever the surface holds
        // keyboard focus.
        //
        // A layer-shell window is mapped with no focus widget at all, so GDK
        // receives key events and drops them before WebKit: nothing reaches the
        // page, and every in-note shortcut is dead until a click happens to
        // focus the WebView by accident. Focusing the WebView whenever the
        // window becomes active covers initial presentation, a later click and
        // the deliberate promotion remap: the local shortcuts are ready as soon
        // as the compositor grants the note keyboard focus.
        let focus_target = webview.downgrade();
        window.connect_is_active_notify(move |win| {
            diagnostics::log(format_args!(
                "event=active-notify note={} active={} mapped={}",
                id,
                win.is_active(),
                win.is_mapped()
            ));
            if !win.is_active() {
                return;
            }
            if let Some(webview) = focus_target.upgrade() {
                if !webview.has_focus() {
                    webview.grab_focus();
                }
            }
        });
        // The window may already be active by the time the child is attached,
        // in which case the notification above has been and gone.
        if window.is_active() {
            webview.grab_focus();
        }

        let focus_window = window.downgrade();
        webview.connect_has_focus_notify(move |webview| {
            diagnostics::log(format_args!(
                "event=webview-focus-notify note={} webview_focus={} window_active={} mapped={}",
                id,
                webview.has_focus(),
                focus_window
                    .upgrade()
                    .map(|window| window.is_active())
                    .unwrap_or(false),
                webview.is_mapped()
            ));
        });

        let layer_transition_generation = Rc::new(Cell::new(0));
        show_initial_layer_surface(window.upcast_ref(), options.layer_mode);
        if options.layer_mode == LayerMode::Desktop {
            schedule_click_focus_restore(
                window.upcast_ref(),
                options.layer_mode,
                Rc::clone(&layer_transition_generation),
                0,
            );
        }

        Self {
            id,
            window: window.upcast(),
            webview,
            document: doc_rc,
            state: state_rc,
            storage: options.storage,
            monitor_size: (mon_w, mon_h),
            layer_mode: layer_mode_cell,
            layer_transition_generation,
            theme: theme_cell,
            ui_scale_percent: ui_scale_cell,
            allow_close,
            pending_flushes,
            loaded,
            capture_delimiter: capture_delimiter_cell,
            pending_reveal,
            external_generation,
            pending_external_write,
        }
    }

    /// Brings this note to the reader, without touching the shared layer.
    ///
    /// Search says "show me that note", not "change how every note is
    /// stacked", so `present` is the whole of it: no `set_layer`, no keyboard
    /// mode change, no visibility flag. Desktop stays Desktop and Overlay
    /// stays Overlay, and the focus that follows is the one ADR-023 already
    /// installed — the WebView takes it when the window becomes active.
    ///
    /// A note on the Desktop layer is revealed *on that layer*, which means a
    /// window sitting over it still sits over it. That is what the layer means,
    /// and quietly promoting the note to Overlay would change a shared setting
    /// the reader did not ask about.
    pub fn reveal(&self) {
        self.window.present();
    }

    /// Tells the page to find `query` in its own document and show it.
    ///
    /// Sent now if the page has a document, and remembered until it does if
    /// not: a note that search has just opened is told what to look for before
    /// it has been loaded.
    pub fn reveal_match(&self, query: String) {
        if self.loaded.get() {
            send_to_webview(&self.webview, &HostToWebviewMessage::RevealMatch { query });
        } else {
            *self.pending_reveal.borrow_mut() = Some(query);
        }
    }

    /// Tells the page that a note it offered no longer exists.
    pub fn report_missing_note(&self, note_id: Uuid) {
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::SearchResultMissing { note_id },
        );
    }

    /// Hands the page the answer to the search it asked for.
    pub fn send_search_results(
        &self,
        request_id: u64,
        results: Vec<noteit_core::search::SearchResult>,
    ) {
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::SearchResults {
                request_id,
                results,
            },
        );
    }

    /// Hands the page the contents of the trash it asked for.
    pub fn send_trash_entries(
        &self,
        request_id: u64,
        entries: Vec<noteit_core::trash::TrashEntry>,
    ) {
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::TrashEntries {
                request_id,
                entries,
            },
        );
    }

    /// Tells the page what became of a data action it asked for.
    ///
    /// The sentence travels ready to be shown. A page that composed its own
    /// from a code would end up with two places deciding what a failure means,
    /// and the one nearer the filesystem is the one that knows.
    pub fn send_data_result(&self, action: &str, ok: bool, message: &str) {
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::DataResult {
                action: action.to_string(),
                ok,
                message: message.to_string(),
            },
        );
    }

    pub fn send_study_catalog(
        &self,
        request_id: u64,
        notes: Vec<crate::webview_bridge::StudyCatalogNote>,
        study_state: Option<noteit_core::study::StudyState>,
        error: Option<String>,
    ) {
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::StudyCatalogResult {
                request_id,
                notes,
                study_state,
                error,
            },
        );
    }

    pub fn send_study_rating(
        &self,
        request_id: u64,
        review_key: String,
        result: Result<noteit_core::study::StudyState, String>,
    ) {
        let (ok, study_state, message) = match result {
            Ok(state) => (true, Some(state), "Avaliação salva.".to_string()),
            Err(error) => {
                eprintln!("Study rating could not be persisted: {error}");
                (
                    false,
                    None,
                    "Não foi possível salvar a avaliação. Tente novamente.".to_string(),
                )
            }
        };
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::StudyRatingResult {
                request_id,
                review_key,
                ok,
                study_state,
                message,
            },
        );
    }

    /// Collapses or expands the note from the host side.
    ///
    /// Goes through the same state transition, resize and persistence as a
    /// request from the note's own menu; only the trigger differs. Returns the
    /// new geometry when something actually changed.
    pub fn set_collapsed(&self, collapsed: bool) -> Option<NoteWindowState> {
        let snapshot = {
            let mut state = self.state.borrow_mut();
            let (monitor_width, monitor_height) = self.monitor_size;
            if !apply_collapse_to_state(
                &mut state,
                collapsed,
                monitor_width,
                monitor_height,
                self.ui_scale_percent.get(),
            ) {
                return None;
            }
            state.clone()
        };

        update_window_size(
            self.window.upcast_ref(),
            snapshot.width,
            snapshot.height,
            snapshot.collapsed,
            self.ui_scale_percent.get(),
        );
        // The page owns the collapsed presentation, so it has to hear about a
        // change it did not start itself.
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::SetCollapsed {
                collapsed: snapshot.collapsed,
            },
        );
        Some(snapshot)
    }

    pub fn is_collapsed(&self) -> bool {
        self.state.borrow().collapsed
    }

    /// Applies the global chrome scale. When this note is collapsed only the
    /// live bar height changes; its remembered expanded geometry is preserved.
    pub fn set_ui_scale(&self, percent: u16) -> Option<NoteWindowState> {
        let percent = clamp_ui_scale_percent(percent);
        self.ui_scale_percent.set(percent);
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::SetUiScale {
                ui_scale_percent: percent,
            },
        );

        let snapshot = {
            let mut state = self.state.borrow_mut();
            if !state.collapsed {
                return None;
            }
            let height = collapsed_note_height(percent);
            if state.height == height {
                return None;
            }
            state.height = height;
            state.clone()
        };
        update_window_size(
            self.window.upcast_ref(),
            snapshot.width,
            snapshot.height,
            true,
            percent,
        );
        Some(snapshot)
    }

    /// Dresses this note's chrome with the shared interface theme. The paper
    /// is untouched: a yellow note stays yellow under the dark theme.
    /// Tells the page whether it is the capture target, and how a capture
    /// would be laid out.
    ///
    /// Pushed rather than asked for: the target is exclusive, so a note that
    /// has just lost it has to hear so — its own menu and its own bar are
    /// still claiming it otherwise.
    pub fn set_autopaste(&self, active: bool, delimiter: CaptureDelimiter) {
        self.capture_delimiter.set(delimiter);
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::SetAutoPaste { active, delimiter },
        );
    }

    /// Tells the page how this note should refer to an image now in the store.
    pub fn send_image_inserted(&self, src: &str) {
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::ImageInserted {
                src: src.to_string(),
            },
        );
    }

    /// Says why an image was not taken in. One sentence, and never a path.
    pub fn send_image_failed(&self, message: &str) {
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::ImageImportFailed {
                message: message.to_string(),
            },
        );
    }

    /// Hands one clipboard capture to the note's editor.
    ///
    /// The text goes nowhere else. It is not stored here, not written to disk
    /// by this path and never logged: the page appends it as plain text and
    /// the ordinary autosave carries it to the file, which is what makes a
    /// capture an edit like any other.
    pub fn send_autopaste_capture(&self, text: &str) {
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::AutoPasteCaptured {
                text: text.to_string(),
            },
        );
    }

    pub fn set_theme(&self, theme: &str) {
        let resolved = theme_name(theme);
        *self.theme.borrow_mut() = resolved.to_string();
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::SetTheme {
                theme: resolved.to_string(),
            },
        );
    }

    pub fn set_layer_mode(&self, mode: LayerMode) -> bool {
        diagnostics::log(format_args!(
            "event=window-layer-begin note={} requested={} stored={} layer={:?} keyboard={:?} active={} webview_focus={} visible={} mapped={}",
            self.id,
            mode.as_str(),
            self.layer_mode.get().as_str(),
            self.window.layer(),
            self.window.keyboard_mode(),
            self.window.is_active(),
            self.webview.has_focus(),
            self.window.is_visible(),
            self.window.is_mapped()
        ));
        if self.layer_mode.get() == mode {
            diagnostics::log(format_args!(
                "event=window-layer-noop note={} requested={}",
                self.id,
                mode.as_str()
            ));
            return false;
        }

        let retain_keyboard_focus = self.window.is_active();
        let generation = self.layer_transition_generation.get().wrapping_add(1);
        self.layer_transition_generation.set(generation);
        let changed = apply_live_layer_mode(&self.window, mode, retain_keyboard_focus);
        self.layer_mode.set(mode);
        if mode != LayerMode::Hidden
            && !retain_keyboard_focus
            && self.window.keyboard_mode() == gtk4_layer_shell::KeyboardMode::None
        {
            schedule_click_focus_restore(
                &self.window,
                mode,
                Rc::clone(&self.layer_transition_generation),
                generation,
            );
        }
        diagnostics::log(format_args!(
            "event=window-layer-end note={} requested={} layer={:?} keyboard={:?} active={} webview_focus={} visible={} mapped={}",
            self.id,
            mode.as_str(),
            self.window.layer(),
            self.window.keyboard_mode(),
            self.window.is_active(),
            self.webview.has_focus(),
            self.window.is_visible(),
            self.window.is_mapped()
        ));
        // Keep the note's own menu showing the shared mode.
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::SetLayerMode {
                layer_mode: mode.as_str().to_string(),
            },
        );
        changed
    }

    /// Rewrites the note exactly as it is held. Since a document is only
    /// adopted in memory once it has been written, that is the note already on
    /// disk — this re-persists it, it does not collect unsaved text. The
    /// editor's latest text is reached with [`Self::request_flush`].
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

    /// Whether the page has been handed its document yet.
    ///
    /// A window whose page has not said `Ready` holds no text of its own, so
    /// asking it for a snapshot would answer with an empty document and an
    /// external write would store that. Nothing asks.
    pub fn is_loaded(&self) -> bool {
        self.loaded.get()
    }

    /// Which run of the document the page is on.
    pub fn external_generation(&self) -> u64 {
        self.external_generation.get()
    }

    /// Asks the page to stop editing and hand back exactly what it holds.
    ///
    /// This is the barrier, and the order inside it is the whole point. The
    /// page freezes *before* it reads its own text; from the moment it answers
    /// until the write is committed or abandoned, nothing can change the
    /// document. A plain flush cannot do this job: it asks for the text and
    /// the reader keeps typing, so the answer is already out of date when it
    /// arrives and the character typed in between is written over.
    ///
    /// The callback is given the page's Markdown, or the reason there is none.
    /// It runs exactly once: a second answer to the same request, or one that
    /// arrives after the timeout has already fired, is ignored.
    pub fn begin_external_write<F: FnOnce(Result<String, String>) + 'static>(
        &self,
        request_id: Uuid,
        callback: F,
    ) {
        if self.pending_external_write.borrow().is_some() {
            callback(Err(
                "a nota já está sendo alterada por outra solicitação".to_string()
            ));
            return;
        }
        if !self.loaded.get() {
            callback(Err("a nota ainda não terminou de abrir".to_string()));
            return;
        }

        *self.pending_external_write.borrow_mut() = Some((request_id, Box::new(callback)));
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::BeginExternalWrite {
                request_id,
                generation: self.external_generation.get(),
            },
        );

        let pending = Rc::clone(&self.pending_external_write);
        let webview = self.webview.downgrade();
        glib::timeout_add_local_once(EXTERNAL_WRITE_TIMEOUT, move || {
            let expired = take_external_write(&pending, request_id);
            if let Some(callback) = expired {
                // The request is dropped here, so the page's late answer finds
                // nothing waiting and cannot be committed afterwards.
                callback(Err(EXTERNAL_WRITE_TIMEOUT_ERROR.to_string()));
                if let Some(webview) = webview.upgrade() {
                    send_to_webview(
                        &webview,
                        &HostToWebviewMessage::AbortExternalWrite { request_id },
                    );
                }
            }
        });
    }

    /// Hands the committed note back to the page and lets it edit again.
    ///
    /// Called only after the atomic write returned, so what the page adopts is
    /// what is on disk. The generation goes up here and nowhere else: from
    /// this moment anything still in flight from the previous run is refused.
    pub fn finish_external_write<F: FnOnce(Result<(), String>) + 'static>(
        &self,
        request_id: Uuid,
        committed: &NoteDocument,
        done: F,
    ) {
        let generation = self.external_generation.get().wrapping_add(1);
        self.external_generation.set(generation);
        send_to_webview_confirmed(
            &self.webview,
            &HostToWebviewMessage::ApplyExternalDocument {
                request_id,
                generation,
                content: committed.content.clone(),
                metadata: MetadataView::from(&committed.user_metadata),
                created_at: committed.metadata.created_at,
                updated_at: committed.metadata.updated_at,
            },
            done,
        );
    }

    /// Nothing was written; the page simply carries on where it left off.
    pub fn abort_external_write(&self, request_id: Uuid) {
        let _ = take_external_write(&self.pending_external_write, request_id);
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::AbortExternalWrite { request_id },
        );
    }

    /// Replaces the document this window holds with one already on disk.
    ///
    /// Used when a note is changed from outside while its window exists but
    /// its page has not loaded yet: there is no live text to lose, and the
    /// document in memory must keep describing the file.
    pub fn adopt_committed_document(&self, committed: NoteDocument) {
        *self.document.borrow_mut() = committed;
        self.external_generation
            .set(self.external_generation.get().wrapping_add(1));
    }

    pub fn close_after_save(&self) {
        self.allow_close.set(true);
        self.window.close();
    }
}

/// Whether a message from the page belongs to the run of the document the
/// window is actually on.
///
/// Anything older is refused rather than written. It was composed against text
/// that has since been replaced by a committed external change, and storing it
/// would put the old body back — which is exactly the failure the generation
/// exists to prevent.
fn accepts_generation(current: &Rc<Cell<u64>>, quoted: u64, what: &str) -> bool {
    let live = current.get();
    if quoted == live {
        return true;
    }
    eprintln!("Rejected a stale {what}: it quotes generation {quoted} and the note is on {live}");
    false
}

/// Takes the waiting external write out, if it is the one named.
fn take_external_write(
    pending: &PendingExternalWrite,
    request_id: Uuid,
) -> Option<ExternalWriteCallback> {
    let mut slot = pending.borrow_mut();
    match slot.as_ref() {
        Some((waiting, _)) if *waiting == request_id => slot.take().map(|(_, callback)| callback),
        _ => None,
    }
}

/// Resolves one external-write answer from the page.
///
/// Answers `false` — and does nothing at all — when the message is not the one
/// being waited for: another note, another run of this note, or a request the
/// host has already given up on. That last case is the one that matters: once
/// a timeout has fired, the late answer must not be able to commit anything.
fn settle_external_write(
    pending: &PendingExternalWrite,
    generation: &Rc<Cell<u64>>,
    expected_note_id: Uuid,
    message_note_id: Uuid,
    request_id: Uuid,
    quoted_generation: u64,
    content: String,
) -> bool {
    if message_note_id != expected_note_id || quoted_generation != generation.get() {
        return false;
    }
    let Some(callback) = take_external_write(pending, request_id) else {
        return false;
    };
    callback(Ok(content));
    true
}

fn schedule_click_focus_restore(
    window: &gtk4::Window,
    mode: LayerMode,
    live_generation: Rc<Cell<u64>>,
    generation: u64,
) {
    let window = window.downgrade();
    glib::timeout_add_local_once(CLICK_FOCUS_RESTORE_DELAY, move || {
        if live_generation.get() != generation {
            return;
        }
        let Some(window) = window.upgrade() else {
            return;
        };
        let expected_layer = match mode {
            LayerMode::Desktop => gtk4_layer_shell::Layer::Bottom,
            LayerMode::Overlay => gtk4_layer_shell::Layer::Overlay,
            LayerMode::Hidden => return,
        };
        if window.is_visible()
            && window.layer() == expected_layer
            && window.keyboard_mode() != gtk4_layer_shell::KeyboardMode::OnDemand
        {
            window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
            diagnostics::log(format_args!(
                "event=keyboard-mode-restored generation={generation}"
            ));
        }
    });
}

/// Applies a collapse/expand request to the window state and re-clamps the
/// resulting geometry. Returns `false` when the note is already in the
/// requested state, so a duplicate request cannot overwrite the stored
/// expanded geometry.
fn apply_collapse_to_state(
    state: &mut NoteWindowState,
    collapsed: bool,
    monitor_width: i32,
    monitor_height: i32,
    ui_scale_percent: u16,
) -> bool {
    if !state.apply_collapsed(collapsed, collapsed_note_height(ui_scale_percent)) {
        return false;
    }

    let (x, y, width, height) = clamp_geometry_with_min_height(
        state.x,
        state.y,
        state.width,
        state.height,
        monitor_width,
        monitor_height,
        min_note_height_for_scale(collapsed, ui_scale_percent),
    );
    state.x = x;
    state.y = y;
    state.width = width;
    state.height = height;
    true
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

/// Persists content arriving from the page.
///
/// The three paths that carry content back — autosave, the flush before hide
/// or quit, and save-and-close — all funnel through here, and all three can
/// arrive with content identical to what is already stored: closing and
/// flushing send whatever the editor holds, whether or not it was touched, and
/// autosave can fire on an edit that serialises back to the same Markdown.
/// Comparing first is what keeps `updated_at` a record of edits rather than of
/// visits, and it means an untouched note is not rewritten at all.
///
/// An identical save still reports success. Close and both flushes wait on
/// this result before the window may go, so "nothing changed" must never turn
/// into "nothing answered".
///
/// That comparison is only sound while the document in memory holds what is
/// actually on disk, so the edit is prepared on a copy and adopted only once
/// the file has been written. A save that fails leaves the document exactly as
/// it was, and the same payload arriving again is therefore still a difference
/// and is written for real, instead of being answered with a success the disk
/// never earned.
fn save_content(
    storage: &StorageManager,
    document: &Rc<RefCell<NoteDocument>>,
    expected_id: Uuid,
    content: String,
) -> Result<(), String> {
    let candidate = {
        let doc = document.borrow();
        if doc.metadata.id != expected_id {
            return Err("note identifier mismatch".to_string());
        }
        // Compared and stored in the one canonical spelling, so the blank line
        // the page's serializer puts after a trailing list — or the newline
        // another editor ended the file with — is not mistaken for an edit.
        let content = NoteDocument::canonical_content(&content);
        if doc.content == content {
            return Ok(());
        }
        let mut candidate = doc.clone();
        candidate.content = content.to_string();
        candidate.touch_content_modified();
        candidate
    };

    commit_once_written(storage, document, candidate)
}

/// Persists an appearance change — paper colour, paper type, pattern intensity
/// or font size — the same way [`save_content`] persists text.
///
/// These change the very document the content comparison is made against, so
/// they are prepared on a copy too. A colour that could not be written is not
/// left sitting in memory as though it had been: the note keeps describing
/// what is stored, and choosing that colour again writes it.
///
/// `updated_at` is deliberately not touched here. Appearance is not content.
fn save_metadata(
    storage: &StorageManager,
    document: &Rc<RefCell<NoteDocument>>,
    expected_id: Uuid,
    change: impl FnOnce(&mut NoteFrontMatter),
) -> Result<(), String> {
    let candidate = {
        let doc = document.borrow();
        if doc.metadata.id != expected_id {
            return Err("note identifier mismatch".to_string());
        }
        let mut candidate = doc.clone();
        change(&mut candidate.metadata);
        candidate
    };

    commit_once_written(storage, document, candidate)
}

/// Persists semantic metadata against the Markdown currently in the WebView.
///
/// A pending editor debounce cannot be overwritten by an older body: if the
/// page's Markdown differs, this one candidate includes that text edit (and
/// only then moves `updated_at`) together with the metadata. The committed
/// document is adopted only after the canonical atomic writer succeeds.
fn save_user_metadata(
    storage: &StorageManager,
    document: &Rc<RefCell<NoteDocument>>,
    expected_id: Uuid,
    content: String,
    metadata: NoteMetadata,
) -> Result<(), String> {
    let candidate = {
        let doc = document.borrow();
        if doc.metadata.id != expected_id {
            return Err("note identifier mismatch".to_string());
        }
        let content = NoteDocument::canonical_content(&content);
        if doc.content == content && doc.user_metadata == metadata {
            return Ok(());
        }
        let mut candidate = doc.clone();
        if candidate.content != content {
            candidate.content = content.to_string();
            candidate.touch_content_modified();
        }
        candidate.user_metadata = metadata;
        candidate
    };

    commit_once_written(storage, document, candidate)
}

/// Writes a prepared note and, only if that succeeded, makes it the document
/// held in memory.
///
/// The order is the whole point: `save_note_atomic` either replaced the file
/// or left the previous one untouched, and until it has said which, the
/// in-memory document must keep describing the note that is on disk. Nothing
/// runs between the write and the adoption — the save is synchronous and the
/// main loop is not re-entered — so there is no change to lose by replacing
/// the document wholesale.
fn commit_once_written(
    storage: &StorageManager,
    document: &Rc<RefCell<NoteDocument>>,
    candidate: NoteDocument,
) -> Result<(), String> {
    storage.save_note_atomic(&candidate)?;
    *document.borrow_mut() = candidate;
    Ok(())
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
        accepts_generation, apply_collapse_to_state, complete_flush_response, expire_flush_request,
        file_uri_for_path, save_and_close, save_content, save_metadata, save_user_metadata,
        settle_external_write, take_external_write, ExternalWriteCallback, PendingExternalWrite,
        PendingFlushes, SubpixelDeltaAccumulator, FLUSH_TIMEOUT_ERROR,
    };
    use crate::layer_shell::{COLLAPSED_NOTE_HEIGHT, MIN_NOTE_HEIGHT};
    use noteit_core::metadata::{NoteMetadata, NoteProperty};
    use noteit_core::model::NoteDocument;
    use noteit_core::state::NoteWindowState;
    use noteit_core::storage::StorageManager;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn fractional_drag_preserves_small_accumulated_deltas() {
        let mut accumulator = SubpixelDeltaAccumulator::default();
        assert_eq!(accumulator.consume(0.3, 0.2), (0, 0));
        assert_eq!(accumulator.consume(0.3, 0.2), (0, 0));
        assert_eq!(accumulator.consume(0.3, 0.2), (0, 0));
        assert_eq!(accumulator.consume(0.3, 0.4), (1, 1));
        assert!((accumulator.remainder_x - 0.2).abs() < f64::EPSILON * 4.0);
        assert!(accumulator.remainder_y.abs() < f64::EPSILON * 4.0);
    }

    #[test]
    fn fractional_drag_preserves_negative_deltas_without_overshoot() {
        let mut accumulator = SubpixelDeltaAccumulator::default();
        assert_eq!(accumulator.consume(-0.6, -0.4), (0, 0));
        assert_eq!(accumulator.consume(-0.6, -0.7), (-1, -1));
        assert!((accumulator.remainder_x + 0.2).abs() < f64::EPSILON * 4.0);
        assert!((accumulator.remainder_y + 0.1).abs() < f64::EPSILON * 4.0);
    }

    #[test]
    fn fractional_resize_uses_the_same_subpixel_accumulation() {
        let mut accumulator = SubpixelDeltaAccumulator::default();
        let updates = [(0.45, 0.35), (0.45, 0.35), (0.45, 0.35)];
        let total = updates.into_iter().fold((0, 0), |(x, y), (dx, dy)| {
            let (pixel_x, pixel_y) = accumulator.consume(dx, dy);
            (x + pixel_x, y + pixel_y)
        });
        assert_eq!(total, (1, 1));
        assert!((accumulator.remainder_x - 0.35).abs() < f64::EPSILON * 4.0);
        assert!((accumulator.remainder_y - 0.05).abs() < f64::EPSILON * 4.0);
    }

    #[test]
    fn a_failed_first_save_leaves_the_note_holding_nothing() {
        // The same rule as `a_content_save_that_fails_is_not_recorded_as_persisted`
        // at the other boundary: a note whose very first write fails. The
        // document must not come away holding text the file never received,
        // because the next attempt resends exactly that text and would then be
        // answered by the identical-content shortcut instead of writing.
        //
        // The editor's own copy is not at stake here: the page holds the live
        // text and resends it on every autosave, flush and close. This
        // document is the record of what is on disk.
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
        assert_eq!(document.borrow().content, "");
        // Sending it again is still a difference, so it is still attempted
        // rather than reported as already stored.
        assert!(save_content(&storage, &document, id, "latest text".to_string()).is_err());
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
        // The note does not come away claiming the text reached the file.
        assert_eq!(document.borrow().content, "");
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

    #[test]
    fn collapsing_and_expanding_reuses_the_existing_geometry_pipeline() {
        let mut state = NoteWindowState {
            x: 700,
            y: 300,
            width: 508,
            height: 552,
            ..NoteWindowState::default()
        };

        assert!(apply_collapse_to_state(&mut state, true, 1920, 1080, 100));
        assert!(state.collapsed);
        assert_eq!(state.height, COLLAPSED_NOTE_HEIGHT);
        assert_eq!(state.width, 508);

        // Moving the collapsed bar and expanding restores the previous size in place.
        state.x = 40;
        state.y = 900;
        assert!(apply_collapse_to_state(&mut state, false, 1920, 1080, 100));
        assert!(!state.collapsed);
        assert_eq!((state.width, state.height), (508, 552));
        assert_eq!((state.x, state.y), (40, 900));
    }

    #[test]
    fn scaled_collapse_keeps_the_expanded_geometry_for_the_next_expand() {
        let mut state = NoteWindowState {
            x: 80,
            y: 120,
            width: 620,
            height: 540,
            ..NoteWindowState::default()
        };

        assert!(apply_collapse_to_state(&mut state, true, 1920, 1080, 160));
        assert_eq!(state.height, 48);
        assert_eq!(
            (state.expanded_width, state.expanded_height),
            (Some(620), Some(540))
        );

        assert!(apply_collapse_to_state(&mut state, false, 1920, 1080, 160));
        assert_eq!((state.width, state.height), (620, 540));
        assert_eq!((state.expanded_width, state.expanded_height), (None, None));
    }

    #[test]
    fn a_repeated_collapse_request_is_ignored() {
        let mut state = NoteWindowState {
            width: 400,
            height: 500,
            ..NoteWindowState::default()
        };

        assert!(apply_collapse_to_state(&mut state, true, 1920, 1080, 100));
        assert!(!apply_collapse_to_state(&mut state, true, 1920, 1080, 100));
        assert_eq!(state.expanded_height, Some(500));

        assert!(apply_collapse_to_state(&mut state, false, 1920, 1080, 100));
        assert_eq!(state.height, 500);
    }

    #[test]
    fn expanding_re_clamps_against_a_smaller_monitor() {
        let mut state = NoteWindowState {
            x: 100,
            y: 100,
            width: 1600,
            height: 900,
            ..NoteWindowState::default()
        };
        assert!(apply_collapse_to_state(&mut state, true, 1920, 1080, 100));

        // The note is expanded again after the display shrank.
        assert!(apply_collapse_to_state(&mut state, false, 1280, 720, 100));
        assert!(state.width <= 1280);
        assert!(state.height <= 720);
        assert!(state.height >= MIN_NOTE_HEIGHT);
    }

    #[test]
    fn changing_the_paper_colour_does_not_count_as_a_content_edit() {
        let tmp = tempdir().expect("tempdir");
        let storage = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");

        let mut doc = NoteDocument::new_empty();
        doc.content = "texto original".to_string();
        storage.save_note_atomic(&doc).expect("initial save");
        let id = doc.metadata.id;
        let created_at = doc.metadata.created_at;
        let updated_at = doc.metadata.updated_at;

        // Same path the ColorChanged handler takes: metadata changes, no touch.
        doc.metadata.color = "blue".to_string();
        storage.save_note_atomic(&doc).expect("colour save");

        let reloaded = storage.load_note(&id).expect("reload note");
        assert_eq!(reloaded.metadata.color, "blue");
        assert_eq!(reloaded.metadata.updated_at, updated_at);
        assert_eq!(reloaded.metadata.created_at, created_at);
    }

    #[test]
    fn changing_the_paper_pattern_does_not_count_as_a_content_edit() {
        let tmp = tempdir().expect("tempdir");
        let storage = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");

        let mut doc = NoteDocument::new_empty();
        doc.content = "texto original".to_string();
        storage.save_note_atomic(&doc).expect("initial save");
        let id = doc.metadata.id;
        let created_at = doc.metadata.created_at;
        let updated_at = doc.metadata.updated_at;

        // Same path the PaperChanged handler takes: metadata changes, no touch.
        doc.metadata.paper_type = "lined".to_string();
        doc.metadata.paper_intensity = "subtle".to_string();
        storage.save_note_atomic(&doc).expect("paper save");

        let reloaded = storage.load_note(&id).expect("reload note");
        assert_eq!(reloaded.metadata.paper_type, "lined");
        assert_eq!(reloaded.metadata.paper_intensity, "subtle");
        assert_eq!(reloaded.metadata.updated_at, updated_at);
        assert_eq!(reloaded.metadata.created_at, created_at);
        assert_eq!(reloaded.content, "texto original");
    }

    #[test]
    fn each_note_keeps_its_own_paper_across_a_restart() {
        let tmp = tempdir().expect("tempdir");
        let storage = StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage");

        let papers = [
            ("yellow", "lined", "subtle"),
            ("blue", "dotted", "normal"),
            ("black", "grid-large", "strong"),
        ];
        let ids: Vec<_> = papers
            .iter()
            .map(|(color, paper_type, intensity)| {
                let mut doc = NoteDocument::new_empty();
                doc.metadata.color = (*color).to_string();
                doc.metadata.paper_type = (*paper_type).to_string();
                doc.metadata.paper_intensity = (*intensity).to_string();
                storage.save_note_atomic(&doc).expect("save note");
                doc.metadata.id
            })
            .collect();

        // Changing the first note must not reach the other two.
        let mut first = storage.load_note(&ids[0]).expect("load first");
        first.metadata.paper_type = "grid-small".to_string();
        storage.save_note_atomic(&first).expect("save first");

        // Reopening from disk is the restart: nothing is held in memory.
        let reloaded: Vec<_> = ids
            .iter()
            .map(|id| storage.load_note(id).expect("reload note"))
            .collect();

        assert_eq!(reloaded[0].metadata.paper_type, "grid-small");
        assert_eq!(reloaded[0].metadata.paper_intensity, "subtle");
        assert_eq!(reloaded[1].metadata.paper_type, "dotted");
        assert_eq!(reloaded[1].metadata.paper_intensity, "normal");
        assert_eq!(reloaded[2].metadata.paper_type, "grid-large");
        assert_eq!(reloaded[2].metadata.paper_intensity, "strong");
        assert_eq!(reloaded[2].metadata.color, "black");
    }

    /// A note already stored, exactly as it sits on disk after a save.
    fn stored_note(storage: &StorageManager, content: &str) -> Rc<RefCell<NoteDocument>> {
        let mut doc = NoteDocument::new_empty();
        doc.content = content.to_string();
        storage.save_note_atomic(&doc).expect("initial save");
        Rc::new(RefCell::new(doc))
    }

    /// Makes every write into a notes directory fail, reversibly.
    ///
    /// The directory is moved aside and a plain file put in its place, so the
    /// kernel refuses every create and rename underneath it with `ENOTDIR`.
    /// That is path resolution rather than a permission bit, so it holds for
    /// every user, including the root the Rust CI job runs as. The notes
    /// themselves wait untouched in the directory that was moved aside and
    /// come back exactly as they were, which is what lets these tests check
    /// that a failed save left the stored note alone.
    struct FailingWrites {
        notes_dir: PathBuf,
        moved_aside: PathBuf,
    }

    impl FailingWrites {
        fn engage(storage: &StorageManager) -> Self {
            let notes_dir = storage.notes_dir().to_path_buf();
            let moved_aside = notes_dir.with_extension("moved-aside");
            fs::rename(&notes_dir, &moved_aside).expect("move the notes directory aside");
            fs::write(&notes_dir, b"not a directory").expect("block the notes directory");
            Self {
                notes_dir,
                moved_aside,
            }
        }

        fn lift(self) {
            fs::remove_file(&self.notes_dir).expect("unblock the notes directory");
            fs::rename(&self.moved_aside, &self.notes_dir).expect("restore the notes directory");
        }
    }

    fn storage_in(tmp: &tempfile::TempDir) -> StorageManager {
        StorageManager::with_custom_paths(
            tmp.path().join("notes"),
            tmp.path().join("config"),
            tmp.path().join("state"),
            tmp.path().join("runtime"),
        )
        .expect("storage")
    }

    #[test]
    fn a_note_written_by_another_editor_is_not_edited_by_merely_opening_it() {
        // 3.5R. Editors conventionally terminate a file with a newline, and
        // that terminator is not part of the note: the page serialises the same
        // document back without it. Comparing the two forms directly made a
        // plain open-and-close look like an edit and moved `updated_at` once,
        // for a note nobody touched.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);

        // A note as `vim` or any other editor would leave it on disk.
        let mut written_elsewhere = NoteDocument::new_empty();
        written_elsewhere.content = "# Lista\n\n- um\n- dois\n".to_string();
        storage
            .save_note_atomic(&written_elsewhere)
            .expect("store the externally written note");
        let id = written_elsewhere.metadata.id;

        // Note-it opens it exactly as any restore would.
        let loaded = storage.load_note(&id).expect("load the external note");
        let updated_at = loaded.metadata.updated_at;
        let file_before = fs::read(storage.note_path(&id)).expect("read the stored file");
        let document = Rc::new(RefCell::new(loaded));

        // Closing sends back what the editor holds: the same document, without
        // the file's trailing newline.
        // Exactly what the page sends back for this note: its serializer
        // terminates a document ending in a list with a blank line. Verified
        // against the real editor in
        // `ui/tests/markdown_roundtrip.test.ts`.
        save_and_close(
            &storage,
            &document,
            id,
            "# Lista\n\n- um\n- dois\n\n".to_string(),
            &|_| Ok(()),
        )
        .expect("closing an untouched note must succeed");

        // And again, because a note must not be rewritten on the second open
        // either.
        save_and_close(
            &storage,
            &document,
            id,
            "# Lista\n\n- um\n- dois\n\n".to_string(),
            &|_| Ok(()),
        )
        .expect("closing it again must still succeed");

        assert_eq!(
            document.borrow().metadata.updated_at,
            updated_at,
            "opening a note written elsewhere is not an edit"
        );
        assert_eq!(
            fs::read(storage.note_path(&id)).expect("read the stored file again"),
            file_before,
            "an untouched note must not be rewritten at all"
        );
    }

    #[test]
    fn a_real_edit_after_an_external_newline_still_moves_updated_at() {
        // The guarantee runs both ways: ignoring the file terminator must not
        // start ignoring genuine edits.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);

        let mut written_elsewhere = NoteDocument::new_empty();
        written_elsewhere.content = "texto\n".to_string();
        storage.save_note_atomic(&written_elsewhere).expect("store");
        let id = written_elsewhere.metadata.id;

        let loaded = storage.load_note(&id).expect("load");
        let updated_at = loaded.metadata.updated_at;
        let document = Rc::new(RefCell::new(loaded));

        save_content(&storage, &document, id, "texto editado".to_string()).expect("save the edit");

        assert!(
            document.borrow().metadata.updated_at > updated_at,
            "a real edit must still move the modification date"
        );
        assert_eq!(
            storage.load_note(&id).expect("reload").content,
            "texto editado"
        );
    }

    #[test]
    fn closing_a_note_nobody_edited_is_not_a_content_edit() {
        // Case A. Closing always sends whatever the editor holds, edited or
        // not, so this arrives with content identical to what is stored.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "# Minha nota\nConteúdo intacto");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;
        let created_at = document.borrow().metadata.created_at;

        let closed = Cell::new(false);
        save_and_close(
            &storage,
            &document,
            id,
            "# Minha nota\nConteúdo intacto".to_string(),
            &|_| {
                closed.set(true);
                Ok(())
            },
        )
        .expect("closing an untouched note still succeeds");

        // The window must still be allowed to go.
        assert!(closed.get(), "close was not finalized");
        assert_eq!(document.borrow().metadata.updated_at, updated_at);
        assert_eq!(document.borrow().metadata.created_at, created_at);
        assert_eq!(
            storage.load_note(&id).expect("reload").metadata.updated_at,
            updated_at
        );
    }

    #[test]
    fn a_note_full_of_calculations_is_not_edited_by_being_recalculated() {
        // 3.6, and 3.7 with it. Every result in a note — an arithmetic one and
        // a converted quantity alike — is a decoration in the page and never
        // part of the document, so opening the note, recomputing all of it and
        // closing it sends back exactly the Markdown that was stored. Nothing
        // about a calculation can reach this side at all — but the guarantee is
        // worth a test on the side that owns `updated_at`.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);

        let note = concat!(
            "preco := 120\n",
            "quantidade := 3\n",
            "= preco * quantidade\n",
            "= 10% de 200\n",
            "= sum\n",
            "distancia := 5\n",
            "= distancia km em m\n",
            "= 0 C em F\n",
            "= 1 GiB em MiB"
        );
        let document = stored_note(&storage, note);
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;
        let path = storage.note_path(&id);
        let file_before = fs::read(&path).expect("read the stored note");

        // Coarse filesystem timestamps would still separate these.
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Opened, recalculated, closed. Twice, because a second open must not
        // be an edit either.
        for _ in 0..2 {
            save_and_close(&storage, &document, id, note.to_string(), &|_| Ok(()))
                .expect("closing a recalculated note must succeed");
        }

        assert_eq!(
            document.borrow().metadata.updated_at,
            updated_at,
            "recalculating a note is not editing it"
        );
        assert_eq!(
            fs::read(&path).expect("read the stored note again"),
            file_before,
            "a note nobody edited must not be rewritten, results included"
        );
    }

    #[test]
    fn editing_an_expression_moves_updated_at_once_however_many_results_change() {
        // The other half of the same guarantee: one edit to the variable every
        // expression below depends on is one content change, so it moves the
        // modification date exactly once. The results that followed it are not
        // content and cost nothing.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);

        let before = "preco := 100\n= preco * 2\n= preco * 3\n= preco + 50";
        let after = "preco := 150\n= preco * 2\n= preco * 3\n= preco + 50";
        let document = stored_note(&storage, before);
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;

        save_content(&storage, &document, id, after.to_string()).expect("save the edit");
        let once = document.borrow().metadata.updated_at;
        assert!(once > updated_at, "editing an expression is an edit");

        // The page recalculates three results from that one edit and sends
        // nothing further, so the next save is a no-op.
        save_content(&storage, &document, id, after.to_string()).expect("no-op save");
        assert_eq!(
            document.borrow().metadata.updated_at,
            once,
            "a recalculated result must not move the date a second time"
        );
        assert_eq!(storage.load_note(&id).expect("reload").content, after);
    }

    #[test]
    fn closing_a_note_that_was_edited_records_the_edit() {
        // Case B.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "Conteúdo A");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;
        let created_at = document.borrow().metadata.created_at;

        save_and_close(&storage, &document, id, "Conteúdo B".to_string(), &|_| {
            Ok(())
        })
        .expect("save and close");

        let reloaded = storage.load_note(&id).expect("reload");
        assert_eq!(reloaded.content, "Conteúdo B");
        assert!(reloaded.metadata.updated_at > updated_at);
        assert_eq!(reloaded.metadata.created_at, created_at);
    }

    #[test]
    fn a_flush_with_nothing_pending_leaves_the_note_alone() {
        // Case C: the flush before hide and before quit both answer with the
        // editor's content whether or not it was touched.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "texto estável");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;

        let pending: PendingFlushes = Rc::new(RefCell::new(std::collections::HashMap::new()));
        let outcome = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&outcome);
        pending
            .borrow_mut()
            .insert(7, Box::new(move |result| *sink.borrow_mut() = Some(result)));

        let accepted = complete_flush_response(
            &storage,
            &document,
            id,
            id,
            7,
            "texto estável".to_string(),
            &pending,
        );

        assert!(accepted);
        // The lifecycle still hears a success, or hide and quit would stall.
        assert!(matches!(outcome.borrow().as_ref(), Some(Ok(()))));
        assert_eq!(
            storage.load_note(&id).expect("reload").metadata.updated_at,
            updated_at
        );
    }

    #[test]
    fn a_flush_carrying_a_pending_edit_persists_it() {
        // Case D.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "antes do flush");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;

        let pending: PendingFlushes = Rc::new(RefCell::new(std::collections::HashMap::new()));
        pending.borrow_mut().insert(8, Box::new(|_| {}));

        assert!(complete_flush_response(
            &storage,
            &document,
            id,
            id,
            8,
            "depois do flush".to_string(),
            &pending,
        ));

        let reloaded = storage.load_note(&id).expect("reload");
        assert_eq!(reloaded.content, "depois do flush");
        assert!(reloaded.metadata.updated_at > updated_at);
    }

    #[test]
    fn repeating_an_autosave_does_not_keep_moving_the_date() {
        // Case E: the first save records the edit, the identical repeat does
        // not, so a note does not age just because autosave fired again.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "primeiro");
        let id = document.borrow().metadata.id;
        let first = document.borrow().metadata.updated_at;

        save_content(&storage, &document, id, "segundo".to_string()).expect("real edit");
        let second = storage.load_note(&id).expect("reload").metadata.updated_at;
        assert!(second > first);

        save_content(&storage, &document, id, "segundo".to_string()).expect("identical repeat");
        assert_eq!(
            storage.load_note(&id).expect("reload").metadata.updated_at,
            second
        );
    }

    #[test]
    fn an_untouched_note_is_not_rewritten_at_all() {
        // Nothing changed, so there is nothing to write: no temp file, no
        // rename, no fsync, and the file's own timestamp stays put.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo idêntico");
        let id = document.borrow().metadata.id;
        let path = storage.note_path(&id);
        let before = fs::metadata(&path)
            .and_then(|m| m.modified())
            .expect("mtime");

        // Far enough apart that a filesystem with coarse timestamps would
        // still show the difference if a write happened.
        std::thread::sleep(std::time::Duration::from_millis(20));
        save_content(&storage, &document, id, "conteúdo idêntico".to_string()).expect("no-op save");

        let after = fs::metadata(&path)
            .and_then(|m| m.modified())
            .expect("mtime");
        assert_eq!(before, after, "an identical save rewrote the file");
    }

    #[test]
    fn a_full_edit_and_reopen_cycle_never_moves_created_at() {
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "inicial");
        let id = document.borrow().metadata.id;
        let created_at = document.borrow().metadata.created_at;
        assert!(created_at.is_some());

        // open → edit → save → close → reopen
        save_content(&storage, &document, id, "editado".to_string()).expect("edit");
        save_and_close(&storage, &document, id, "editado".to_string(), &|_| Ok(())).expect("close");
        let reopened = storage.load_note(&id).expect("reopen");
        assert_eq!(reopened.metadata.created_at, created_at);

        // ...and again, this time without touching anything.
        let reopened_doc = Rc::new(RefCell::new(reopened));
        save_and_close(&storage, &reopened_doc, id, "editado".to_string(), &|_| {
            Ok(())
        })
        .expect("close untouched");
        assert_eq!(
            storage.load_note(&id).expect("reopen").metadata.created_at,
            created_at
        );
    }

    #[test]
    fn no_appearance_change_is_ever_a_content_edit() {
        // Case F, in one place: everything the menu can change about how a
        // note looks goes through the metadata save, never through
        // `save_content`, so none of it moves the modification date.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo que ninguém tocou");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;
        let created_at = document.borrow().metadata.created_at;

        {
            let mut doc = document.borrow_mut();
            doc.metadata.color = "black".to_string();
            doc.metadata.paper_type = "grid-large".to_string();
            doc.metadata.paper_intensity = "strong".to_string();
            doc.metadata.font_size = 22;
            storage.save_note_atomic(&doc).expect("appearance save");
        }

        let reloaded = storage.load_note(&id).expect("reload");
        assert_eq!(reloaded.metadata.color, "black");
        assert_eq!(reloaded.metadata.paper_type, "grid-large");
        assert_eq!(reloaded.metadata.paper_intensity, "strong");
        assert_eq!(reloaded.metadata.font_size, 22);
        assert_eq!(reloaded.metadata.updated_at, updated_at);
        assert_eq!(reloaded.metadata.created_at, created_at);
        assert_eq!(reloaded.content, "conteúdo que ninguém tocou");

        // Zoom, theme, collapse, geometry and the layer never reach the
        // document at all: they live in `state.json` and `config.toml`.
        assert_eq!(document.borrow().metadata.updated_at, updated_at);
    }

    /// The five writes a Timer makes over one run, as the window makes them.
    ///
    /// The window's message handler sanitises what the page sent, puts it on
    /// the note's window state and hands that to the geometry callback, which
    /// is the ordinary `state.json` write. Nothing in that path opens a note
    /// file, which is the property the tests below turn into an assertion.
    fn timer_run() -> [Option<noteit_core::timer::NoteTimerState>; 5] {
        use noteit_core::timer::{NoteTimerState, TimerRunState};
        let deadline = 1_800_000_000_000_i64 + 25 * 60_000;
        [
            Some(NoteTimerState {
                state: TimerRunState::Running,
                deadline_ms: Some(deadline),
                ..NoteTimerState::default()
            }),
            Some(NoteTimerState {
                state: TimerRunState::Paused,
                remaining_ms: Some(18 * 60_000 + 42_000),
                ..NoteTimerState::default()
            }),
            Some(NoteTimerState {
                state: TimerRunState::Running,
                deadline_ms: Some(deadline + 7 * 60_000),
                ..NoteTimerState::default()
            }),
            Some(NoteTimerState {
                state: TimerRunState::Finished,
                ..NoteTimerState::default()
            }),
            // Cancelled: the note goes back to having no timer at all.
            None,
        ]
    }

    #[test]
    fn a_whole_timer_run_leaves_the_note_file_byte_for_byte_as_it_was() {
        // The rule the phase rests on: a timer is operational state, not
        // content. Starting, pausing, resuming, finishing and cancelling one
        // are five writes to `state.json` and nought writes to the note.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "# Reunião\n\n- pauta\n- 25 minutos de foco");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;
        let created_at = document.borrow().metadata.created_at;
        let file_before = fs::read(storage.note_path(&id)).expect("read the stored note");

        let state_path = storage.state_file_path();
        let mut app_state = noteit_core::state::AppState::default();
        app_state.notes.insert(id, NoteWindowState::default());

        for timer in timer_run() {
            let entry = app_state
                .notes
                .get_mut(&id)
                .expect("the note's window state");
            entry.timer = timer.and_then(|timer| timer.sanitize());
            app_state
                .save_to_file(&state_path)
                .expect("persist the timer");
        }

        // Byte for byte. Not "equivalent", not "the same content" — the same
        // file, so front matter, ordering and the terminating newline are all
        // exactly as they were.
        assert_eq!(
            fs::read(storage.note_path(&id)).expect("read the note again"),
            file_before,
        );

        let reopened = storage.load_note(&id).expect("reopen");
        assert_eq!(reopened.metadata.updated_at, updated_at);
        assert_eq!(reopened.metadata.created_at, created_at);
        assert_eq!(
            reopened.content,
            "# Reunião\n\n- pauta\n- 25 minutos de foco"
        );

        // And nothing about a timer is written into the Markdown anywhere,
        // in any form — no comment, no front-matter key, no marker.
        let text = String::from_utf8(file_before).expect("the note is text");
        for forbidden in [
            "timer",
            "pomodoro",
            "deadline",
            "focus",
            "phase",
            "remaining",
            "25:00",
        ] {
            assert!(
                !text.to_lowercase().contains(forbidden),
                "the note file mentions {forbidden:?}",
            );
        }
    }

    #[test]
    fn an_image_is_content_but_its_plumbing_is_not() {
        // A picture is real content — the note holds it, the file records it,
        // and a reader can delete it. What is *not* content is how it is
        // stored: the identifiers, the width, the alignment and the path are
        // machinery, and searching for any of them must not find the note.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let note_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();

        let body = format!(
            "# Biópsia hepática\n\n             <img src=\"../assets/{note_id}/{asset_id}.png\" alt=\"\"              data-note-it-width=\"320\" data-note-it-align=\"left\">\n\n             encefalopatia hepática"
        );
        let document = stored_note(&storage, &body);
        let id = document.borrow().metadata.id;

        let bodies = storage.read_note_bodies_by_recency();
        let search = |query: &str| {
            noteit_core::search::search_notes(
                query,
                bodies.iter().map(|(id, text)| (*id, text.as_str())),
            )
        };

        // None of the plumbing is findable.
        for query in [
            asset_id.to_string(),
            note_id.to_string(),
            "assets".to_string(),
            "../assets".to_string(),
            "data-note-it-width".to_string(),
            "data-note-it-align".to_string(),
            "320".to_string(),
            ".png".to_string(),
            "img src".to_string(),
        ] {
            assert!(
                search(&query).is_empty(),
                "searching {query:?} found a note only because of how a picture is stored",
            );
        }

        // The words around it still are.
        assert_eq!(search("encefalopatia").len(), 1);
        assert_eq!(search("Biópsia").len(), 1);
        // ...and the snippet the palette shows carries none of the machinery.
        let result = &search("encefalopatia")[0];
        assert_eq!(result.label, "Biópsia hepática");
        for technical in ["<img", "data-note-it", "assets", ".png"] {
            assert!(
                !result.snippet.contains(technical),
                "the snippet leaked {technical:?}",
            );
            assert!(!result.label.contains(technical));
        }

        // The trash names it the same way, and leaks nothing either.
        storage.move_note_to_trash(&id).expect("move to the trash");
        let entries = storage.list_trash();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "Biópsia hepática");
        for technical in ["<img", "data-note-it", "assets", ".png"] {
            assert!(!entries[0].snippet.contains(technical));
        }
    }

    #[test]
    fn a_note_that_is_only_a_picture_is_still_a_note_nobody_named() {
        // Every image this application inserts carries an empty alternative
        // text, so a note holding one picture and no words has no title to
        // take from it.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let note_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();

        for body in [
            format!("![](../assets/{note_id}/{asset_id}.png)"),
            format!(
                "<img src=\"../assets/{note_id}/{asset_id}.png\" alt=\"\"                  data-note-it-width=\"320\">"
            ),
        ] {
            let document = stored_note(&storage, &body);
            let id = document.borrow().metadata.id;
            storage.move_note_to_trash(&id).expect("move to the trash");
        }

        for entry in storage.list_trash() {
            // The store's own word for a note with nothing to read, which is
            // what a note holding one picture and no text is. What matters is
            // that no part of how the picture is stored became its name.
            assert_eq!(
                entry.label,
                noteit_core::search::EMPTY_LABEL,
                "a note that is one picture was given a name from its plumbing",
            );
            assert!(entry.snippet.is_empty(), "a picture became a preview");
        }
    }

    #[test]
    fn moving_a_note_with_a_picture_never_rewrites_the_reference() {
        // The whole reason the stored path is relative. `notes/` and `trash/`
        // are siblings, so `../assets/…` resolves the same from either, and a
        // note travels between them byte for byte.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let note_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();
        let body = format!("uma nota\n\n![](../assets/{note_id}/{asset_id}.png)");

        let document = stored_note(&storage, &body);
        let id = document.borrow().metadata.id;
        let before = fs::read(storage.note_path(&id)).expect("read the stored note");

        storage.move_note_to_trash(&id).expect("to the trash");
        let in_trash =
            fs::read(storage.trash_dir().join(format!("{id}.md"))).expect("read it in the trash");
        assert_eq!(in_trash, before, "the file changed on its way to the trash");

        storage
            .restore_note_from_trash(&id)
            .expect("and back again");
        assert_eq!(
            fs::read(storage.note_path(&id)).expect("read it back"),
            before,
            "the file changed on its way out of the trash",
        );
    }

    #[test]
    fn a_running_timer_is_invisible_to_search_the_trash_and_the_title() {
        // Searching "25:00" must not find a note merely because it has a
        // twenty-five minute Pomodoro on it. That holds structurally: search
        // reads `notes/`, the timer lives in `state.json`, and the two never
        // meet — this is the assertion that the arrangement really is that.
        use noteit_core::timer::{NoteTimerState, PomodoroPhase, TimerMode, TimerRunState};
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "Comprar pão e café");
        let id = document.borrow().metadata.id;

        let mut app_state = noteit_core::state::AppState::default();
        app_state.notes.insert(
            id,
            NoteWindowState {
                timer: NoteTimerState {
                    mode: TimerMode::Pomodoro,
                    state: TimerRunState::Running,
                    deadline_ms: Some(1_800_000_000_000),
                    phase: PomodoroPhase::Focus,
                    focus_completed: 2,
                    ..NoteTimerState::default()
                }
                .sanitize(),
                ..NoteWindowState::default()
            },
        );
        app_state
            .save_to_file(&storage.state_file_path())
            .expect("persist the timer");

        let bodies = storage.read_note_bodies_by_recency();
        for query in ["25:00", "pomodoro", "timer", "foco", "deadline"] {
            let results = noteit_core::search::search_notes(
                query,
                bodies.iter().map(|(id, body)| (*id, body.as_str())),
            );
            assert!(
                results.is_empty(),
                "searching {query:?} found a note that only has a timer",
            );
        }
        // The note is still found by what it actually says.
        assert_eq!(
            noteit_core::search::search_notes(
                "café",
                bodies.iter().map(|(id, body)| (*id, body.as_str())),
            )
            .len(),
            1,
        );

        // The trash names a note by its own first line, and the timer is not
        // part of that either.
        storage.move_note_to_trash(&id).expect("move to the trash");
        let entries = storage.list_trash();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "Comprar pão e café");
        assert!(!entries[0].snippet.to_lowercase().contains("pomodoro"));
        assert!(!entries[0].snippet.contains("25:00"));
    }

    #[test]
    fn two_notes_keep_their_own_timers() {
        // The record hangs off the note's identifier, so there is no shared
        // slot for one note's countdown to appear on another.
        use noteit_core::timer::{NoteTimerState, TimerRunState};
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let first = stored_note(&storage, "nota A").borrow().metadata.id;
        let second = stored_note(&storage, "nota B").borrow().metadata.id;
        let state_path = storage.state_file_path();

        let mut app_state = noteit_core::state::AppState::default();
        app_state.notes.insert(
            first,
            NoteWindowState {
                timer: NoteTimerState {
                    state: TimerRunState::Running,
                    deadline_ms: Some(1_800_000_600_000),
                    ..NoteTimerState::default()
                }
                .sanitize(),
                ..NoteWindowState::default()
            },
        );
        app_state.notes.insert(second, NoteWindowState::default());
        app_state.save_to_file(&state_path).expect("persist");

        let reloaded = noteit_core::state::AppState::load_from_file(&state_path);
        assert!(reloaded.notes[&first].timer.is_some());
        assert!(
            reloaded.notes[&second].timer.is_none(),
            "a note with no timer must not inherit another note's",
        );

        // The second note starts its own, and neither disturbs the other.
        app_state.notes.get_mut(&second).expect("note B").timer = NoteTimerState {
            state: TimerRunState::Paused,
            remaining_ms: Some(90_000),
            ..NoteTimerState::default()
        }
        .sanitize();
        app_state.save_to_file(&state_path).expect("persist");

        let reloaded = noteit_core::state::AppState::load_from_file(&state_path);
        assert_eq!(
            reloaded.notes[&first]
                .timer
                .expect("A keeps its own")
                .deadline_ms,
            Some(1_800_000_600_000),
        );
        assert_eq!(
            reloaded.notes[&second]
                .timer
                .expect("B keeps its own")
                .remaining_ms,
            Some(90_000),
        );
    }

    #[test]
    fn recency_now_follows_the_last_edit_rather_than_the_last_close() {
        // Summon reopens the most recently *saved* note when everything is
        // closed, and that ordering comes from the file's own mtime. Skipping
        // identical saves therefore shifts what "last used" means: closing an
        // untouched note no longer moves it to the front. The note last
        // written in does, which is the note a summon should bring back.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);

        let first = stored_note(&storage, "nota antiga");
        // Distinct mtimes, so the ordering is decided by the writes and not by
        // the identifier tie-break.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let second = stored_note(&storage, "nota recente");
        let first_id = first.borrow().metadata.id;
        let second_id = second.borrow().metadata.id;

        assert_eq!(
            storage.list_notes_by_recency().expect("listing")[0],
            second_id,
        );

        // Closing the newer one untouched leaves the ordering alone...
        std::thread::sleep(std::time::Duration::from_millis(20));
        save_and_close(
            &storage,
            &second,
            second_id,
            "nota recente".to_string(),
            &|_| Ok(()),
        )
        .expect("close untouched");
        assert_eq!(
            storage.list_notes_by_recency().expect("listing")[0],
            second_id,
            "an untouched close must not reorder anything",
        );

        // ...while a real edit to the older one brings it to the front, so a
        // summon answers with the note that was actually written in.
        std::thread::sleep(std::time::Duration::from_millis(20));
        save_content(
            &storage,
            &first,
            first_id,
            "nota antiga, editada".to_string(),
        )
        .expect("edit");
        let ordering = storage.list_notes_by_recency().expect("listing");
        assert_eq!(ordering[0], first_id);
        assert_eq!(ordering.len(), 2);
    }

    #[test]
    fn semantic_metadata_save_keeps_both_timestamps_when_text_is_unchanged() {
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo A");
        let id = document.borrow().metadata.id;
        let created_at = document.borrow().metadata.created_at;
        let updated_at = document.borrow().metadata.updated_at;
        let metadata = NoteMetadata::try_new(
            ["Medicina".into()],
            [NoteProperty {
                key: "status".into(),
                value: "revisando".into(),
            }],
        )
        .expect("metadata");

        save_user_metadata(
            &storage,
            &document,
            id,
            "conteúdo A".into(),
            metadata.clone(),
        )
        .expect("save metadata");

        let reloaded = storage.load_note(&id).expect("reload");
        assert_eq!(reloaded.metadata.created_at, created_at);
        assert_eq!(reloaded.metadata.updated_at, updated_at);
        assert_eq!(reloaded.user_metadata, metadata);
    }

    #[test]
    fn semantic_metadata_save_commits_unsaved_webview_text_in_the_same_candidate() {
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "texto no disco");
        let id = document.borrow().metadata.id;
        let previous_updated_at = document.borrow().metadata.updated_at;
        let metadata = NoteMetadata::try_new(["PBL".into()], std::iter::empty()).expect("metadata");

        save_user_metadata(
            &storage,
            &document,
            id,
            "texto novo ainda na WebView".into(),
            metadata.clone(),
        )
        .expect("combined save");

        let reloaded = storage.load_note(&id).expect("reload");
        assert_eq!(reloaded.content, "texto novo ainda na WebView");
        assert_eq!(reloaded.user_metadata, metadata);
        assert!(reloaded.metadata.updated_at > previous_updated_at);
    }

    #[test]
    fn failed_semantic_metadata_save_is_not_adopted_and_the_same_retry_works() {
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo A");
        let id = document.borrow().metadata.id;
        let metadata =
            NoteMetadata::try_new(["Projeto".into()], std::iter::empty()).expect("metadata");

        let fault = FailingWrites::engage(&storage);
        save_user_metadata(
            &storage,
            &document,
            id,
            "conteúdo A".into(),
            metadata.clone(),
        )
        .expect_err("first save fails");
        fault.lift();
        assert!(document.borrow().user_metadata.tags.is_empty());
        assert!(storage
            .load_note(&id)
            .expect("disk")
            .user_metadata
            .tags
            .is_empty());

        save_user_metadata(
            &storage,
            &document,
            id,
            "conteúdo A".into(),
            metadata.clone(),
        )
        .expect("retry");
        assert_eq!(document.borrow().user_metadata, metadata);
        assert_eq!(
            storage.load_note(&id).expect("disk").user_metadata,
            metadata
        );
    }

    #[test]
    fn a_content_save_that_fails_is_not_recorded_as_persisted() {
        // 3.4R.1, case 1. The document in memory is the record of what is on
        // disk — it is what the identical-content check is compared against —
        // so a write that never landed must leave it exactly as it was.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo A");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;

        let fault = FailingWrites::engage(&storage);
        let failure = save_content(&storage, &document, id, "conteúdo B".to_string())
            .expect_err("a save that cannot be written must report the failure");
        fault.lift();

        assert!(
            failure.contains("temp file"),
            "unexpected failure: {failure}"
        );
        assert_eq!(document.borrow().content, "conteúdo A");
        assert_eq!(document.borrow().metadata.updated_at, updated_at);

        let on_disk = storage.load_note(&id).expect("reload");
        assert_eq!(on_disk.content, "conteúdo A");
        assert_eq!(on_disk.metadata.updated_at, updated_at);
    }

    #[test]
    fn resending_the_same_content_after_a_failed_save_writes_it_for_real() {
        // 3.4R.1, case 2. Autosave, the flushes and save-and-close all resend
        // whatever the editor holds, so the payload that failed arrives again
        // unchanged. The identical-content shortcut must not answer for it.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo A");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;
        let created_at = document.borrow().metadata.created_at;

        let fault = FailingWrites::engage(&storage);
        save_content(&storage, &document, id, "conteúdo B".to_string())
            .expect_err("the first attempt must fail");
        fault.lift();

        save_content(&storage, &document, id, "conteúdo B".to_string())
            .expect("the retry must write rather than report a phantom success");

        let on_disk = storage.load_note(&id).expect("reload");
        assert_eq!(on_disk.content, "conteúdo B");
        assert!(on_disk.metadata.updated_at > updated_at);
        assert_eq!(on_disk.metadata.created_at, created_at);
        assert_eq!(document.borrow().content, "conteúdo B");
    }

    #[test]
    fn save_and_close_does_not_finalize_a_close_over_a_failed_save() {
        // 3.4R.1, case 3. The window may only go once the text it holds is on
        // disk; otherwise closing is how the edit is lost.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo A");
        let id = document.borrow().metadata.id;

        let closed = Cell::new(false);
        let fault = FailingWrites::engage(&storage);
        let failure = save_and_close(&storage, &document, id, "conteúdo B".to_string(), &|_| {
            closed.set(true);
            Ok(())
        })
        .expect_err("the close must not be finalized over a failed save");
        fault.lift();

        assert!(
            failure.starts_with("note save failed"),
            "unexpected failure: {failure}"
        );
        assert!(!closed.get(), "the note was closed over an unsaved edit");
        assert_eq!(
            storage.load_note(&id).expect("reload").content,
            "conteúdo A"
        );
    }

    #[test]
    fn the_close_goes_through_once_the_write_finally_succeeds() {
        // 3.4R.1, case 4. The retry after the fault is lifted has to persist
        // and only then close — a failed save must not turn into a note that
        // can never be closed either.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo A");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;

        let closed = Cell::new(false);
        let fault = FailingWrites::engage(&storage);
        save_and_close(&storage, &document, id, "conteúdo B".to_string(), &|_| {
            closed.set(true);
            Ok(())
        })
        .expect_err("the first attempt must fail");
        fault.lift();

        save_and_close(&storage, &document, id, "conteúdo B".to_string(), &|_| {
            closed.set(true);
            Ok(())
        })
        .expect("the retry must save and close");

        assert!(closed.get(), "the note could no longer be closed");
        let on_disk = storage.load_note(&id).expect("reload");
        assert_eq!(on_disk.content, "conteúdo B");
        assert!(on_disk.metadata.updated_at > updated_at);
    }

    #[test]
    fn a_flush_whose_save_fails_reports_the_failure_to_the_lifecycle() {
        // The flush before hide and before quit both wait on this result
        // before destroying surfaces or exiting, so a failed write has to
        // reach them as a failure rather than as a success.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo A");
        let id = document.borrow().metadata.id;

        let pending: PendingFlushes = Rc::new(RefCell::new(std::collections::HashMap::new()));
        let outcome = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&outcome);
        pending.borrow_mut().insert(
            21,
            Box::new(move |result| *sink.borrow_mut() = Some(result)),
        );

        let fault = FailingWrites::engage(&storage);
        let accepted = complete_flush_response(
            &storage,
            &document,
            id,
            id,
            21,
            "conteúdo B".to_string(),
            &pending,
        );
        fault.lift();

        assert!(accepted);
        assert!(matches!(outcome.borrow().as_ref(), Some(Err(_))));
        assert_eq!(document.borrow().content, "conteúdo A");
        assert_eq!(
            storage.load_note(&id).expect("reload").content,
            "conteúdo A"
        );
    }

    #[test]
    fn an_appearance_save_that_fails_leaves_the_note_describing_what_is_stored() {
        // 3.4R.1, case 5. Paper colour, paper type, intensity and font size all
        // change the same document the content check is compared against, so a
        // failed appearance save must not be adopted in memory either — and the
        // content no-op that follows a close must not stand in for it.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo intacto");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;

        let fault = FailingWrites::engage(&storage);
        save_metadata(&storage, &document, id, |metadata| {
            metadata.color = "black".to_string();
            metadata.paper_type = "grid-large".to_string();
            metadata.paper_intensity = "strong".to_string();
            metadata.font_size = 22;
        })
        .expect_err("an appearance save that cannot be written must report the failure");
        fault.lift();

        for note in [
            document.borrow().clone(),
            storage.load_note(&id).expect("reload"),
        ] {
            assert_eq!(note.metadata.color, "yellow");
            assert_eq!(note.metadata.paper_type, "blank");
            assert_eq!(note.metadata.paper_intensity, "normal");
            assert_eq!(note.metadata.font_size, 15);
        }

        // Closing the note now succeeds, because its *text* really is stored —
        // and that success must not be mistaken for the appearance having
        // landed. The note on disk still says exactly what memory says.
        save_and_close(
            &storage,
            &document,
            id,
            "conteúdo intacto".to_string(),
            &|_| Ok(()),
        )
        .expect("an untouched note still closes");
        assert_eq!(
            storage.load_note(&id).expect("reload").metadata.color,
            "yellow"
        );

        // Choosing the same appearance again writes it, and appearance still
        // never moves the modification date.
        save_metadata(&storage, &document, id, |metadata| {
            metadata.color = "black".to_string();
            metadata.font_size = 22;
        })
        .expect("the retry must write");

        let on_disk = storage.load_note(&id).expect("reload");
        assert_eq!(on_disk.metadata.color, "black");
        assert_eq!(on_disk.metadata.font_size, 22);
        assert_eq!(on_disk.metadata.updated_at, updated_at);
        assert_eq!(document.borrow().metadata.color, "black");
        assert_eq!(on_disk.content, "conteúdo intacto");
    }

    #[test]
    fn a_save_committed_but_not_synced_is_adopted_like_any_other() {
        // 3.4R.1 rolls back a save that failed; 3.4R.2 draws the line at the
        // rename. Past that point the file *is* the new note, so refusing to
        // adopt it would leave memory describing a version the file no longer
        // holds — the same divergence 3.4R.1 closed, mirrored.
        let tmp = tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "conteúdo A");
        let id = document.borrow().metadata.id;
        let updated_at = document.borrow().metadata.updated_at;
        let created_at = document.borrow().metadata.created_at;

        // The same store, through a handle whose directory sync fails after
        // the rename has already replaced the file.
        let unsyncable = storage.clone().failing_directory_sync();

        let closed = Cell::new(false);
        save_and_close(
            &unsyncable,
            &document,
            id,
            "conteúdo B".to_string(),
            &|_| {
                closed.set(true);
                Ok(())
            },
        )
        .expect("a completed rename is a completed save");

        // The lifecycle hears a success, and the note may close.
        assert!(closed.get(), "the note was refused a close it had earned");
        // Memory and file describe the same version, not opposite ones.
        assert_eq!(document.borrow().content, "conteúdo B");
        let on_disk = storage.load_note(&id).expect("reload");
        assert_eq!(on_disk.content, "conteúdo B");
        assert_eq!(
            document.borrow().metadata.updated_at,
            on_disk.metadata.updated_at
        );
        assert!(on_disk.metadata.updated_at > updated_at);
        assert_eq!(on_disk.metadata.created_at, created_at);

        // Resending it is a genuine no-op now, because the note really is B —
        // there is no pending write for the shortcut to swallow. What was left
        // undone is durability, and that is not per-note state: the next real
        // save syncs the directory and carries the earlier rename with it.
        let path = storage.note_path(&id);
        let before = fs::metadata(&path)
            .and_then(|m| m.modified())
            .expect("mtime");
        std::thread::sleep(std::time::Duration::from_millis(20));
        save_content(&storage, &document, id, "conteúdo B".to_string()).expect("no-op save");
        assert_eq!(
            fs::metadata(&path)
                .and_then(|m| m.modified())
                .expect("mtime"),
            before,
            "an identical save rewrote a note that was already stored"
        );

        save_content(&storage, &document, id, "conteúdo C".to_string()).expect("a later edit");
        assert_eq!(
            storage.load_note(&id).expect("reload").content,
            "conteúdo C"
        );
    }

    #[test]
    fn saving_content_moves_updated_at_and_keeps_created_at() {
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
        let created_at = document.borrow().metadata.created_at;
        let updated_at = document.borrow().metadata.updated_at;

        save_content(&storage, &document, id, "texto editado".to_string()).expect("save");

        let reloaded = storage.load_note(&id).expect("reload note");
        assert_eq!(reloaded.metadata.created_at, created_at);
        assert!(reloaded.metadata.updated_at >= updated_at);
        assert!(reloaded.metadata.updated_at.is_some());
    }
    // The runtime generation ---------------------------------------------------
    //
    // A note changed from outside its window gets a new generation. Everything
    // the page sends that carries content quotes the generation it was
    // composed against, and anything quoting an older one is refused. That is
    // the whole mechanism that stops an autosave already in flight from
    // putting the previous body back over a commit that has just landed.

    fn generation(start: u64) -> Rc<Cell<u64>> {
        Rc::new(Cell::new(start))
    }

    #[test]
    fn a_message_from_the_current_run_of_the_document_is_accepted() {
        let live = generation(0);
        assert!(accepts_generation(&live, 0, "autosave"));

        live.set(8);
        assert!(accepts_generation(&live, 8, "autosave"));
    }

    #[test]
    fn a_message_from_a_superseded_run_is_refused() {
        // 4.0E §28. An external write committed generation 8; an autosave
        // composed against 7 arrives afterwards. It was written against a
        // document that no longer exists and must not reach the file.
        let live = generation(8);
        assert!(!accepts_generation(&live, 7, "autosave"));
        assert!(!accepts_generation(&live, 0, "save-and-close"));
        assert!(!accepts_generation(&live, 6, "metadata save"));
        assert!(!accepts_generation(&live, 5, "flush"));
    }

    #[test]
    fn a_message_quoting_a_run_that_does_not_exist_yet_is_refused_too() {
        // Nothing legitimate can be ahead of the host: the host is what moves
        // the generation on. Anything claiming to be is not from this window.
        let live = generation(3);
        assert!(!accepts_generation(&live, 4, "autosave"));
    }

    #[test]
    fn a_stale_autosave_cannot_undo_an_external_commit() {
        // The acceptance criterion, end to end over the two pieces that decide
        // it: disk holds A, the page still has B from before, and an external
        // write has committed A+C. The stale B is refused, so the file keeps
        // A+C. Without the generation, B would be written and C would vanish.
        let tmp = tempfile::tempdir().expect("tempdir");
        let storage = storage_in(&tmp);
        let document = stored_note(&storage, "A");
        let id = document.borrow().metadata.id;
        let live = generation(0);

        // The external write: A becomes A+C, and the generation moves on.
        let committed = {
            let mut candidate = document.borrow().clone();
            candidate.content = "A\nC".to_string();
            candidate.touch_content_modified();
            candidate
        };
        storage.save_note_atomic(&committed).expect("commit");
        *document.borrow_mut() = committed;
        live.set(live.get() + 1);

        // The autosave that was already in flight, carrying B.
        assert!(
            !accepts_generation(&live, 0, "autosave"),
            "a stale autosave was accepted"
        );

        // Refused, so nothing writes it. The file is still the committed one.
        let on_disk = storage.load_note(&id).expect("load");
        assert_eq!(on_disk.content, "A\nC");
        assert_eq!(document.borrow().content, "A\nC");

        // And an edit composed against the new run persists normally.
        assert!(accepts_generation(&live, 1, "autosave"));
        save_content(&storage, &document, id, "A\nC\nD".to_string()).expect("save");
        assert_eq!(storage.load_note(&id).expect("load").content, "A\nC\nD");
    }

    // The external write barrier -----------------------------------------------

    fn pending_slot() -> PendingExternalWrite {
        Rc::new(RefCell::new(None))
    }

    fn park(
        pending: &PendingExternalWrite,
        request_id: Uuid,
        answer: Rc<RefCell<Option<Result<String, String>>>>,
    ) {
        let callback: ExternalWriteCallback = Box::new(move |result| {
            *answer.borrow_mut() = Some(result);
        });
        *pending.borrow_mut() = Some((request_id, callback));
    }

    #[test]
    fn the_page_s_answer_resolves_the_write_that_asked_for_it() {
        let pending = pending_slot();
        let live = generation(2);
        let note_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let answer = Rc::new(RefCell::new(None));
        park(&pending, request_id, Rc::clone(&answer));

        assert!(settle_external_write(
            &pending,
            &live,
            note_id,
            note_id,
            request_id,
            2,
            "ABCD".to_string(),
        ));
        assert_eq!(answer.borrow().clone(), Some(Ok("ABCD".to_string())));
        assert!(pending.borrow().is_none(), "the request was not taken out");
    }

    #[test]
    fn an_answer_the_host_has_already_given_up_on_can_never_commit() {
        // 4.0E §26. The host times out, drops the request and tells the writer
        // nothing was changed. A late answer must then find nothing waiting:
        // committing it afterwards would write text on behalf of a command
        // that has already been told it failed.
        let pending = pending_slot();
        let live = generation(0);
        let note_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let answer = Rc::new(RefCell::new(None));
        park(&pending, request_id, Rc::clone(&answer));

        // The timeout fires and takes the request.
        let expired = take_external_write(&pending, request_id).expect("the timeout takes it");
        expired(Err("timed out".to_string()));
        assert!(answer.borrow().is_some());
        *answer.borrow_mut() = None;

        assert!(
            !settle_external_write(
                &pending,
                &live,
                note_id,
                note_id,
                request_id,
                0,
                "tarde demais".to_string(),
            ),
            "a late answer was accepted after the host gave up"
        );
        assert!(answer.borrow().is_none());
    }

    #[test]
    fn an_answer_for_another_note_or_another_run_is_ignored() {
        let pending = pending_slot();
        let live = generation(3);
        let note_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let answer = Rc::new(RefCell::new(None));
        park(&pending, request_id, Rc::clone(&answer));

        // Another note's window.
        assert!(!settle_external_write(
            &pending,
            &live,
            note_id,
            Uuid::new_v4(),
            request_id,
            3,
            "x".to_string(),
        ));
        // The right note, a superseded run.
        assert!(!settle_external_write(
            &pending,
            &live,
            note_id,
            note_id,
            request_id,
            2,
            "x".to_string(),
        ));
        // The right note and run, a request nobody is waiting for.
        assert!(!settle_external_write(
            &pending,
            &live,
            note_id,
            note_id,
            Uuid::new_v4(),
            3,
            "x".to_string(),
        ));

        assert!(answer.borrow().is_none());
        assert!(
            pending.borrow().is_some(),
            "a mismatched answer consumed the pending request"
        );
    }

    #[test]
    fn only_one_external_write_can_hold_a_note_at_a_time() {
        // Two of them would each take their own snapshot of the same text and
        // the second commit would silently undo the first.
        let pending = pending_slot();
        let first = Uuid::new_v4();
        let answer = Rc::new(RefCell::new(None));
        park(&pending, first, Rc::clone(&answer));

        assert!(pending.borrow().is_some());
        assert!(
            take_external_write(&pending, Uuid::new_v4()).is_none(),
            "another request took the one that was waiting"
        );
        assert!(take_external_write(&pending, first).is_some());
    }
}
