use crate::layer_shell::{
    apply_layer_mode, apply_paper_color, clamp_geometry_with_min_height, min_note_height,
    setup_layer_shell_window, update_window_position, update_window_size, WindowGeometry,
    COLLAPSED_NOTE_HEIGHT, DEFAULT_MONITOR_HEIGHT, DEFAULT_MONITOR_WIDTH,
};
use crate::model::{paper_intensity_name, paper_type_name, NoteDocument};
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
                            let mut doc = doc_clone.borrow_mut();
                            doc.metadata.color = color;
                            // Keep the surface backing in step, so the next
                            // resize cannot flash the previous paper colour.
                            if let Some(win) = window_weak.upgrade() {
                                apply_paper_color(win.upcast_ref(), &doc.metadata.color);
                            }
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
                            if let Err(error) = storage_clone.save_note_atomic(&doc) {
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
                            let mut doc = doc_clone.borrow_mut();
                            // Whatever the page sends is resolved against the
                            // supported set before it reaches the note file.
                            doc.metadata.paper_type = paper_type_name(&paper_type).to_string();
                            doc.metadata.paper_intensity =
                                paper_intensity_name(&paper_intensity).to_string();
                            // Appearance only: the note is saved without its
                            // modification date moving.
                            if let Err(error) = storage_clone.save_note_atomic(&doc) {
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
    doc.touch_content_modified();
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
        apply_collapse_to_state, complete_flush_response, expire_flush_request, file_uri_for_path,
        save_and_close, save_content, PendingFlushes, SubpixelDeltaAccumulator,
        FLUSH_TIMEOUT_ERROR,
    };
    use crate::layer_shell::{COLLAPSED_NOTE_HEIGHT, MIN_NOTE_HEIGHT};
    use crate::model::NoteDocument;
    use crate::state::NoteWindowState;
    use crate::storage::StorageManager;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::Path;
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
