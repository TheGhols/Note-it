use crate::state::LayerMode;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub const MIN_NOTE_WIDTH: i32 = 220;
pub const MIN_NOTE_HEIGHT: i32 = 160;
pub const DEFAULT_MONITOR_WIDTH: i32 = 1920;
pub const DEFAULT_MONITOR_HEIGHT: i32 = 1080;

/// Clamps note geometry against a monitor rectangle to ensure the note is
/// accessible, visible on-screen, and within valid size bounds.
pub fn clamp_geometry(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    monitor_width: i32,
    monitor_height: i32,
) -> (i32, i32, i32, i32) {
    let mon_w = monitor_width.max(MIN_NOTE_WIDTH);
    let mon_h = monitor_height.max(MIN_NOTE_HEIGHT);

    let clamped_w = width.clamp(MIN_NOTE_WIDTH, mon_w);
    let clamped_h = height.clamp(MIN_NOTE_HEIGHT, mon_h);

    // Keep at least 50px horizontally and 30px vertically visible on screen
    let max_x = (mon_w - 50).max(0);
    let max_y = (mon_h - 30).max(0);

    let clamped_x = x.clamp(0, max_x);
    let clamped_y = y.clamp(0, max_y);

    (clamped_x, clamped_y, clamped_w, clamped_h)
}

/// Calculates a cascading screen position for new notes so they don't spawn
/// directly on top of each other while avoiding off-screen placement.
pub fn calculate_cascade_position(
    existing_notes_count: usize,
    monitor_width: i32,
    monitor_height: i32,
    note_width: i32,
    note_height: i32,
) -> (i32, i32) {
    let base_x = 100;
    let base_y = 100;
    let step = 30;

    let available_w = (monitor_width - note_width - base_x).max(step);
    let available_h = (monitor_height - note_height - base_y).max(step);

    let slots_x = (available_w / step).max(1);
    let slots_y = (available_h / step).max(1);
    let total_slots = slots_x.min(slots_y).max(1);

    let slot = (existing_notes_count as i32) % total_slots;
    let raw_x = base_x + slot * step;
    let raw_y = base_y + slot * step;

    let (x, y, _, _) = clamp_geometry(
        raw_x,
        raw_y,
        note_width,
        note_height,
        monitor_width,
        monitor_height,
    );
    (x, y)
}

/// Discovers a monitor by its connector name or falls back to the first available monitor.
/// Returns (Option<gdk::Monitor>, connector_name, monitor_width, monitor_height).
pub fn find_monitor_by_connector(
    display: Option<&gdk::Display>,
    preferred_connector: Option<&str>,
) -> (Option<gdk::Monitor>, Option<String>, i32, i32) {
    let display = match display {
        Some(d) => d,
        None => return (None, None, DEFAULT_MONITOR_WIDTH, DEFAULT_MONITOR_HEIGHT),
    };

    let monitors_list = display.monitors();
    let n_monitors = monitors_list.n_items();

    if n_monitors == 0 {
        return (None, None, DEFAULT_MONITOR_WIDTH, DEFAULT_MONITOR_HEIGHT);
    }

    let mut first_monitor: Option<gdk::Monitor> = None;
    let mut first_connector: Option<String> = None;
    let mut first_geom = (DEFAULT_MONITOR_WIDTH, DEFAULT_MONITOR_HEIGHT);

    for i in 0..n_monitors {
        if let Some(mon) = monitors_list
            .item(i)
            .and_then(|item| item.downcast::<gdk::Monitor>().ok())
        {
            let connector = mon.connector().map(|s| s.to_string());
            let geom = mon.geometry();
            let (w, h) = (geom.width().max(640), geom.height().max(480));

            if first_monitor.is_none() {
                first_monitor = Some(mon.clone());
                first_connector = connector.clone();
                first_geom = (w, h);
            }

            if let (Some(pref), Some(ref conn)) = (preferred_connector, &connector) {
                if pref == conn {
                    return (Some(mon), Some(conn.clone()), w, h);
                }
            }
        }
    }

    (first_monitor, first_connector, first_geom.0, first_geom.1)
}

pub fn setup_layer_shell_window(
    window: &gtk4::Window,
    mode: LayerMode,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    monitor: Option<&gdk::Monitor>,
) {
    if !window.is_layer_window() {
        window.init_layer_shell();
    }

    window.set_namespace(Some("note-it"));
    window.set_exclusive_zone(0);

    if let Some(mon) = monitor {
        window.set_monitor(Some(mon));
    }

    // Anchors for positioning at (x, y) coordinates
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Bottom, false);
    window.set_anchor(Edge::Right, false);

    let w = width.max(MIN_NOTE_WIDTH);
    let h = height.max(MIN_NOTE_HEIGHT);

    window.set_margin(Edge::Left, x.max(0));
    window.set_margin(Edge::Top, y.max(0));

    window.set_default_size(w, h);
    window.set_size_request(MIN_NOTE_WIDTH, MIN_NOTE_HEIGHT);

    apply_layer_mode(window, mode);
}

pub fn apply_layer_mode(window: &gtk4::Window, mode: LayerMode) {
    if !window.is_layer_window() {
        return;
    }

    match mode {
        LayerMode::Desktop => {
            window.set_layer(Layer::Bottom);
            window.set_keyboard_mode(KeyboardMode::OnDemand);
            window.set_visible(true);
            window.present();
        }
        LayerMode::Overlay => {
            window.set_layer(Layer::Overlay);
            window.set_keyboard_mode(KeyboardMode::OnDemand);
            window.set_visible(true);
            window.present();
        }
        LayerMode::Hidden => {
            window.set_keyboard_mode(KeyboardMode::None);
            window.set_visible(false);
        }
    }
}

pub fn update_window_position(window: &gtk4::Window, x: i32, y: i32) {
    if window.is_layer_window() {
        window.set_margin(Edge::Left, x.max(0));
        window.set_margin(Edge::Top, y.max(0));
    }
}

pub fn update_window_size(window: &gtk4::Window, width: i32, height: i32) {
    let w = width.max(MIN_NOTE_WIDTH);
    let h = height.max(MIN_NOTE_HEIGHT);
    window.set_default_size(w, h);
    window.queue_resize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_geometry_negative_and_overflow() {
        let (x, y, w, h) = clamp_geometry(-50, -100, 100, 50, 1920, 1080);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
        assert_eq!(w, MIN_NOTE_WIDTH);
        assert_eq!(h, MIN_NOTE_HEIGHT);

        let (x2, y2, w2, h2) = clamp_geometry(3000, 2000, 5000, 3000, 1920, 1080);
        assert_eq!(x2, 1920 - 50);
        assert_eq!(y2, 1080 - 30);
        assert_eq!(w2, 1920);
        assert_eq!(h2, 1080);
    }

    #[test]
    fn test_clamp_geometry_valid_values() {
        let (x, y, w, h) = clamp_geometry(300, 200, 400, 350, 1920, 1080);
        assert_eq!(x, 300);
        assert_eq!(y, 200);
        assert_eq!(w, 400);
        assert_eq!(h, 350);
    }

    #[test]
    fn test_cascade_position_wraparound() {
        let (x0, y0) = calculate_cascade_position(0, 1920, 1080, 360, 300);
        assert_eq!((x0, y0), (100, 100));

        let (x1, y1) = calculate_cascade_position(1, 1920, 1080, 360, 300);
        assert_eq!((x1, y1), (130, 130));

        let (x_large, y_large) = calculate_cascade_position(1000, 1920, 1080, 360, 300);
        assert!((100..1920).contains(&x_large));
        assert!((100..1080).contains(&y_large));
    }

    #[test]
    fn test_monitor_fallback_when_none() {
        let (mon, conn, w, h) = find_monitor_by_connector(None, Some("DP-1"));
        assert!(mon.is_none());
        assert!(conn.is_none());
        assert_eq!(w, DEFAULT_MONITOR_WIDTH);
        assert_eq!(h, DEFAULT_MONITOR_HEIGHT);
    }
}
