use crate::state::LayerMode;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub const MIN_NOTE_WIDTH: i32 = 220;
pub const MIN_NOTE_HEIGHT: i32 = 160;
/// Height of a collapsed note: the header bar plus the surrounding border.
pub const COLLAPSED_NOTE_HEIGHT: i32 = 30;
pub const DEFAULT_MONITOR_WIDTH: i32 = 1920;
pub const DEFAULT_MONITOR_HEIGHT: i32 = 1080;

/// Paper colours, mirroring the palette in `ui/src/styles/theme.css`.
///
/// The host needs these because a WebView repaints asynchronously: when a fast
/// resize grows the surface, the newly exposed strip is presented before the
/// page has painted it. Backing the window with the note's own colour means
/// that strip shows paper instead of the default dark window background.
pub const PAPER_COLORS: &[(&str, &str)] = &[
    ("yellow", "#FEF9C3"),
    ("blue", "#E0F2FE"),
    ("green", "#DCFCE7"),
    ("pink", "#FCE7F3"),
    ("purple", "#F3E8FF"),
    ("gray", "#F1F5F9"),
    ("black", "#18181B"),
];

pub const DEFAULT_PAPER_COLOR: &str = "yellow";
const SURFACE_CSS_CLASS: &str = "note-it-surface";
const NOTE_BORDER_RADIUS_PX: i32 = 12;

/// Resolves a stored colour name to the palette, falling back to the default
/// so an unknown name can never leave the surface unpainted.
pub fn paper_color_name(color: &str) -> &'static str {
    PAPER_COLORS
        .iter()
        .find(|(name, _)| *name == color)
        .map(|(name, _)| *name)
        .unwrap_or(DEFAULT_PAPER_COLOR)
}

/// Stylesheet backing every note surface with its paper colour. Rounded
/// corners are kept on the window itself, so filling the resize gap does not
/// turn the note into a plain rectangle.
pub fn paper_color_stylesheet() -> String {
    let mut css = String::new();
    for (name, hex) in PAPER_COLORS {
        css.push_str(&format!(
            ".{SURFACE_CSS_CLASS}.note-it-paper-{name} {{ background-color: {hex}; border-radius: {NOTE_BORDER_RADIUS_PX}px; }}\n"
        ));
    }
    css
}

/// Installs the paper stylesheet once per display.
pub fn install_paper_color_styles(display: &gdk::Display) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&paper_color_stylesheet());
    gtk4::style_context_add_provider_for_display(
        display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// Points the window at one palette entry. Swapping a class keeps a single
/// shared stylesheet instead of one provider per note.
pub fn apply_paper_color(window: &gtk4::Window, color: &str) {
    window.add_css_class(SURFACE_CSS_CLASS);
    for (name, _) in PAPER_COLORS {
        window.remove_css_class(&format!("note-it-paper-{name}"));
    }
    window.add_css_class(&format!("note-it-paper-{}", paper_color_name(color)));
}

/// Position, size and collapse state of a note surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub collapsed: bool,
}

/// Minimum window height allowed for a note in the given collapse state.
/// A collapsed note only needs to fit its header bar.
pub fn min_note_height(collapsed: bool) -> i32 {
    if collapsed {
        COLLAPSED_NOTE_HEIGHT
    } else {
        MIN_NOTE_HEIGHT
    }
}

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
    clamp_geometry_with_min_height(
        x,
        y,
        width,
        height,
        monitor_width,
        monitor_height,
        MIN_NOTE_HEIGHT,
    )
}

/// Same clamping rules as [`clamp_geometry`], but with an explicit minimum
/// height so a collapsed note can shrink down to its header bar.
pub fn clamp_geometry_with_min_height(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    monitor_width: i32,
    monitor_height: i32,
    min_height: i32,
) -> (i32, i32, i32, i32) {
    let mon_w = monitor_width.max(MIN_NOTE_WIDTH);
    let mon_h = monitor_height.max(min_height);

    let clamped_w = width.clamp(MIN_NOTE_WIDTH, mon_w);
    let clamped_h = height.clamp(min_height, mon_h);

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
    geometry: WindowGeometry,
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

    let min_h = min_note_height(geometry.collapsed);
    let w = geometry.width.max(MIN_NOTE_WIDTH);
    let h = geometry.height.max(min_h);

    window.set_margin(Edge::Left, geometry.x.max(0));
    window.set_margin(Edge::Top, geometry.y.max(0));

    window.set_default_size(w, h);
    window.set_size_request(MIN_NOTE_WIDTH, min_h);

    configure_layer_mode(window, mode);
}

/// Configures the layer-shell role without deciding whether the surface should
/// be presented. Initial creation and a live layer transition are different
/// operations: only creation needs to map the window.
fn configure_layer_mode(window: &gtk4::Window, mode: LayerMode) -> bool {
    if !window.is_layer_window() {
        return false;
    }

    let mut changed = false;
    match mode {
        LayerMode::Desktop => {
            if window.layer() != Layer::Bottom {
                window.set_layer(Layer::Bottom);
                changed = true;
            }
            // Map a restored desktop note without taking focus. NoteWindow
            // restores OnDemand after Niri has observed this initial map, so
            // the note remains click-to-focus without activating at startup.
            if window.keyboard_mode() != KeyboardMode::None {
                window.set_keyboard_mode(KeyboardMode::None);
                changed = true;
            }
        }
        LayerMode::Overlay => {
            if window.layer() != Layer::Overlay {
                window.set_layer(Layer::Overlay);
                changed = true;
            }
            if window.keyboard_mode() != KeyboardMode::OnDemand {
                window.set_keyboard_mode(KeyboardMode::OnDemand);
                changed = true;
            }
        }
        LayerMode::Hidden => {
            if window.keyboard_mode() != KeyboardMode::None {
                window.set_keyboard_mode(KeyboardMode::None);
                changed = true;
            }
            if window.is_visible() {
                window.set_visible(false);
                changed = true;
            }
        }
    }
    changed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LiveLayerTransitionPlan {
    set_layer: bool,
    set_keyboard_mode: bool,
    hide: bool,
    remap: bool,
}

fn plan_live_layer_transition(
    current: LayerMode,
    keyboard_mode_matches: bool,
    visible: bool,
    target: LayerMode,
) -> LiveLayerTransitionPlan {
    match target {
        LayerMode::Desktop | LayerMode::Overlay => LiveLayerTransitionPlan {
            set_layer: current != target,
            set_keyboard_mode: !keyboard_mode_matches,
            hide: false,
            // A fully occluded Bottom surface receives no timely frame
            // callback. Niri applies set_layer only on the next wl_surface
            // commit; queue_draw, queue_render and present all remained
            // throttled for close to a second. Remapping only this direction
            // forces that commit without permanently changing visibility.
            remap: current == LayerMode::Desktop && target == LayerMode::Overlay,
        },
        LayerMode::Hidden => LiveLayerTransitionPlan {
            set_layer: false,
            set_keyboard_mode: !keyboard_mode_matches,
            hide: visible,
            remap: false,
        },
    }
}

/// Maps a newly created note exactly once, after its child and focus handlers
/// have been installed.
pub fn show_initial_layer_surface(window: &gtk4::Window, mode: LayerMode) {
    configure_layer_mode(window, mode);
    if mode != LayerMode::Hidden && !window.is_visible() {
        window.present();
    }
}

/// Applies a mode change to an already visible surface.
///
/// gtk4-layer-shell protocol v4 lets Niri update the layer of a mapped surface
/// directly. Re-presenting it is not an unconditional part of that transition
/// and can create avoidable activation/focus churn, so the global inactive path
/// changes only properties that are actually different. `present()` is kept
/// only when a deliberate promotion must retain focus the note already owns.
pub fn apply_live_layer_mode(
    window: &gtk4::Window,
    mode: LayerMode,
    retain_keyboard_focus: bool,
) -> bool {
    if !window.is_layer_window() {
        return false;
    }

    let target_layer = match mode {
        LayerMode::Desktop => Some(Layer::Bottom),
        LayerMode::Overlay => Some(Layer::Overlay),
        LayerMode::Hidden => None,
    };
    let target_keyboard = if mode == LayerMode::Hidden
        || (!retain_keyboard_focus && window.keyboard_mode() == KeyboardMode::None)
    {
        KeyboardMode::None
    } else {
        KeyboardMode::OnDemand
    };
    let current = match window.layer() {
        Layer::Bottom => LayerMode::Desktop,
        Layer::Overlay => LayerMode::Overlay,
        _ => LayerMode::Hidden,
    };
    let plan = plan_live_layer_transition(
        current,
        window.keyboard_mode() == target_keyboard,
        window.is_visible(),
        mode,
    );
    if plan.remap {
        // Mapping an on-demand layer surface makes Niri give it keyboard focus,
        // even when Brave or another normal window was focused. Map the
        // promoted surface as non-interactive, then restore click-to-focus once
        // it exists on Overlay.
        window.set_keyboard_mode(KeyboardMode::None);
    }
    if plan.set_layer {
        window.set_layer(target_layer.expect("visible layer mode has a layer"));
    }
    if plan.set_keyboard_mode && !plan.remap {
        window.set_keyboard_mode(target_keyboard);
    }
    if plan.hide {
        window.set_visible(false);
    }
    if plan.remap {
        window.set_visible(false);
        window.set_visible(true);
        if retain_keyboard_focus {
            window.set_keyboard_mode(target_keyboard);
            window.present();
        }
    }
    plan.set_layer || plan.set_keyboard_mode || plan.hide
}

pub fn update_window_position(window: &gtk4::Window, x: i32, y: i32) {
    if window.is_layer_window() {
        window.set_margin(Edge::Left, x.max(0));
        window.set_margin(Edge::Top, y.max(0));
    }
}

pub fn update_window_size(window: &gtk4::Window, width: i32, height: i32, collapsed: bool) {
    let min_h = min_note_height(collapsed);
    let w = width.max(MIN_NOTE_WIDTH);
    let h = height.max(min_h);
    window.set_size_request(MIN_NOTE_WIDTH, min_h);
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
    fn collapsed_geometry_may_shrink_to_the_header_bar() {
        let (x, y, w, h) = clamp_geometry_with_min_height(
            300,
            200,
            400,
            COLLAPSED_NOTE_HEIGHT,
            1920,
            1080,
            min_note_height(true),
        );
        assert_eq!((x, y, w), (300, 200, 400));
        assert_eq!(h, COLLAPSED_NOTE_HEIGHT);

        // The same geometry is rejected while the note is expanded.
        let (_, _, _, expanded_h) = clamp_geometry_with_min_height(
            300,
            200,
            400,
            COLLAPSED_NOTE_HEIGHT,
            1920,
            1080,
            min_note_height(false),
        );
        assert_eq!(expanded_h, MIN_NOTE_HEIGHT);
    }

    #[test]
    fn test_monitor_fallback_when_none() {
        let (mon, conn, w, h) = find_monitor_by_connector(None, Some("DP-1"));
        assert!(mon.is_none());
        assert!(conn.is_none());
        assert_eq!(w, DEFAULT_MONITOR_WIDTH);
        assert_eq!(h, DEFAULT_MONITOR_HEIGHT);
    }

    #[test]
    fn a_live_layer_change_only_remaps_the_occluded_direction() {
        let to_overlay =
            plan_live_layer_transition(LayerMode::Desktop, true, true, LayerMode::Overlay);
        assert!(to_overlay.set_layer);
        assert!(to_overlay.remap);
        assert!(!to_overlay.hide);

        let to_desktop =
            plan_live_layer_transition(LayerMode::Overlay, true, true, LayerMode::Desktop);
        assert!(to_desktop.set_layer);
        assert!(!to_desktop.remap);
        assert!(!to_desktop.hide);
    }

    /// The layer transition and the collapse state never read each other.
    ///
    /// Phase 3.9UX.R was reported as collapse failing on the desktop layer. The
    /// plan a layer change produces is computed from the layer, the keyboard
    /// mode and whether the surface is mapped — and from nothing else — so a
    /// collapsed note and an expanded one take exactly the same path. The size
    /// floor is the mirror image: it reads the collapse and nothing about the
    /// layer.
    #[test]
    fn a_layer_transition_is_decided_without_reference_to_the_collapse() {
        for target in [LayerMode::Desktop, LayerMode::Overlay] {
            for current in [LayerMode::Desktop, LayerMode::Overlay] {
                let plan = plan_live_layer_transition(current, true, true, target);
                // Both collapse states resolve the same surface geometry rules,
                // so the plan cannot differ between them: there is no collapse
                // input to differ on.
                assert_eq!(plan.set_layer, current != target);
                assert!(!plan.hide);
            }
        }
    }

    #[test]
    fn the_height_floor_follows_the_collapse_and_never_the_layer() {
        assert_eq!(min_note_height(true), COLLAPSED_NOTE_HEIGHT);
        assert_eq!(min_note_height(false), MIN_NOTE_HEIGHT);

        // Collapsing while on the desktop shrinks to the same bar it shrinks to
        // on the overlay: the clamp has no layer input at all.
        for collapsed in [true, false] {
            let (_, _, _, h) = clamp_geometry_with_min_height(
                0,
                0,
                360,
                1,
                1920,
                1080,
                min_note_height(collapsed),
            );
            assert_eq!(
                h,
                if collapsed {
                    COLLAPSED_NOTE_HEIGHT
                } else {
                    MIN_NOTE_HEIGHT
                }
            );
        }
    }

    #[test]
    fn an_unchanged_live_layer_is_a_complete_no_op() {
        let plan = plan_live_layer_transition(LayerMode::Overlay, true, true, LayerMode::Overlay);
        assert_eq!(
            plan,
            LiveLayerTransitionPlan {
                set_layer: false,
                set_keyboard_mode: false,
                hide: false,
                remap: false,
            }
        );
    }

    #[test]
    fn the_surface_palette_matches_the_stylesheet() {
        // The host repaints the resize gap with the paper colour, so its copy
        // of the palette must not drift from the one the page renders.
        let theme = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/ui/src/styles/theme.css"
        ))
        .expect("read theme.css");

        for (name, hex) in PAPER_COLORS {
            let selector = format!("body[data-color=\"{name}\"]");
            let block_start = theme
                .find(&selector)
                .unwrap_or_else(|| panic!("theme.css has no palette entry for {name}"));
            let block = &theme[block_start..];
            let block_end = block.find('}').expect("palette block is closed");
            let declarations = &block[..block_end];

            let paper_bg = declarations
                .lines()
                .find_map(|line| line.trim().strip_prefix("--paper-bg:"))
                .map(|value| value.trim().trim_end_matches(';').to_string())
                .unwrap_or_else(|| panic!("no --paper-bg declared for {name}"));

            assert_eq!(
                paper_bg.to_ascii_uppercase(),
                hex.to_ascii_uppercase(),
                "surface colour for {name} drifted from the stylesheet"
            );
        }
    }

    #[test]
    fn an_unknown_paper_colour_falls_back_instead_of_going_unpainted() {
        assert_eq!(paper_color_name("blue"), "blue");
        assert_eq!(paper_color_name("chartreuse"), DEFAULT_PAPER_COLOR);
        assert_eq!(paper_color_name(""), DEFAULT_PAPER_COLOR);

        let css = paper_color_stylesheet();
        for (name, hex) in PAPER_COLORS {
            assert!(css.contains(&format!("note-it-paper-{name}")));
            assert!(css.contains(hex));
        }
        // Rounded corners survive the opaque backing.
        assert!(css.contains("border-radius"));
    }
}
