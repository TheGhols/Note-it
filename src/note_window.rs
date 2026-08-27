use crate::layer_shell::{
    apply_layer_mode, apply_paper_color, clamp_geometry_with_min_height, min_note_height,
    setup_layer_shell_window, update_window_position, update_window_size, WindowGeometry,
    COLLAPSED_NOTE_HEIGHT, DEFAULT_MONITOR_HEIGHT, DEFAULT_MONITOR_WIDTH,
};
use crate::model::{paper_intensity_name, paper_type_name, NoteDocument, NoteFrontMatter};
use crate::settings::theme_name;
use crate::state::{clamp_zoom_percent, LayerMode, NoteWindowState};
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
    pub on_toggle_layer_mode: Rc<dyn Fn()>,
    /// Interface theme in force when the note opens. Shared by every note, so
    /// the window is told about it rather than reading it back from the store.
    pub theme: String,
    pub on_theme_changed: Rc<dyn Fn(String)>,
}

type FlushCallback = Box<dyn FnOnce(Result<(), String>)>;
type PendingFlushes = Rc<RefCell<std::collections::HashMap<u64, FlushCallback>>>;

const FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
const FLUSH_TIMEOUT_ERROR: &str = "timed out waiting for latest WebView content";

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
    theme: Rc<RefCell<String>>,
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

        // Clamp initial geometry. A note restored collapsed keeps the header
        // bar height instead of being forced back to the expanded minimum.
        let restored_collapsed = options.state.collapsed;
        let (clamped_x, clamped_y, clamped_w, clamped_h) = clamp_geometry_with_min_height(
            options.state.x,
            options.state.y,
            options.state.width,
            options.state.height,
            mon_w,
            mon_h,
            min_note_height(restored_collapsed),
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
        let layer_mode_cell = Rc::new(Cell::new(options.layer_mode));
        let theme_cell = Rc::new(RefCell::new(theme_name(&options.theme).to_string()));

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
                                        collapsed: state_clone.borrow().collapsed,
                                        created_at: doc.metadata.created_at,
                                        updated_at: doc.metadata.updated_at,
                                        zoom_percent: state_clone.borrow().zoom_percent,
                                        layer_mode: layer_mode_clone.get().as_str().to_string(),
                                        theme: theme_clone.borrow().clone(),
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
                        WebviewToHostMessage::ThemeChanged { theme } => {
                            on_theme_changed_clone(theme_name(&theme).to_string());
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
                                if !apply_collapse_to_state(&mut st, collapsed, mon_w, mon_h) {
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
                                min_note_height(st.collapsed),
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
                                min_note_height(false),
                            );
                            st.width = cw;
                            st.height = ch;
                            if let Some(win) = window_weak.upgrade() {
                                update_window_size(win.upcast_ref(), cw, ch, false);
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
            monitor_size: (mon_w, mon_h),
            layer_mode: layer_mode_cell,
            theme: theme_cell,
            allow_close,
            pending_flushes,
        }
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
            if !apply_collapse_to_state(&mut state, collapsed, monitor_width, monitor_height) {
                return None;
            }
            state.clone()
        };

        update_window_size(
            self.window.upcast_ref(),
            snapshot.width,
            snapshot.height,
            snapshot.collapsed,
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

    /// Dresses this note's chrome with the shared interface theme. The paper
    /// is untouched: a yellow note stays yellow under the dark theme.
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

    pub fn set_layer_mode(&self, mode: LayerMode) {
        self.layer_mode.set(mode);
        apply_layer_mode(&self.window, mode);
        // Keep the note's own menu showing the shared mode.
        send_to_webview(
            &self.webview,
            &HostToWebviewMessage::SetLayerMode {
                layer_mode: mode.as_str().to_string(),
            },
        );
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

    pub fn close_after_save(&self) {
        self.allow_close.set(true);
        self.window.close();
    }
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
) -> bool {
    if !state.apply_collapsed(collapsed, COLLAPSED_NOTE_HEIGHT) {
        return false;
    }

    let (x, y, width, height) = clamp_geometry_with_min_height(
        state.x,
        state.y,
        state.width,
        state.height,
        monitor_width,
        monitor_height,
        min_note_height(collapsed),
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
        if doc.content == content {
            return Ok(());
        }
        let mut candidate = doc.clone();
        candidate.content = content;
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
        apply_collapse_to_state, complete_flush_response, expire_flush_request, file_uri_for_path,
        save_and_close, save_content, save_metadata, PendingFlushes, SubpixelDeltaAccumulator,
        FLUSH_TIMEOUT_ERROR,
    };
    use crate::layer_shell::{COLLAPSED_NOTE_HEIGHT, MIN_NOTE_HEIGHT};
    use crate::model::NoteDocument;
    use crate::state::NoteWindowState;
    use crate::storage::StorageManager;
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

        assert!(apply_collapse_to_state(&mut state, true, 1920, 1080));
        assert!(state.collapsed);
        assert_eq!(state.height, COLLAPSED_NOTE_HEIGHT);
        assert_eq!(state.width, 508);

        // Moving the collapsed bar and expanding restores the previous size in place.
        state.x = 40;
        state.y = 900;
        assert!(apply_collapse_to_state(&mut state, false, 1920, 1080));
        assert!(!state.collapsed);
        assert_eq!((state.width, state.height), (508, 552));
        assert_eq!((state.x, state.y), (40, 900));
    }

    #[test]
    fn a_repeated_collapse_request_is_ignored() {
        let mut state = NoteWindowState {
            width: 400,
            height: 500,
            ..NoteWindowState::default()
        };

        assert!(apply_collapse_to_state(&mut state, true, 1920, 1080));
        assert!(!apply_collapse_to_state(&mut state, true, 1920, 1080));
        assert_eq!(state.expanded_height, Some(500));

        assert!(apply_collapse_to_state(&mut state, false, 1920, 1080));
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
        assert!(apply_collapse_to_state(&mut state, true, 1920, 1080));

        // The note is expanded again after the display shrank.
        assert!(apply_collapse_to_state(&mut state, false, 1280, 720));
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
}
