use crate::state::LayerMode;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub fn setup_layer_shell_window(
    window: &gtk4::Window,
    mode: LayerMode,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    if !window.is_layer_window() {
        window.init_layer_shell();
    }

    window.set_namespace(Some("note-it"));
    window.set_exclusive_zone(0);

    // Anchors for positioning at (x, y) coordinates
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Bottom, false);
    window.set_anchor(Edge::Right, false);

    window.set_margin(Edge::Top, y.max(0));
    window.set_margin(Edge::Left, x.max(0));

    window.set_default_size(width.max(180), height.max(180));

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

#[allow(dead_code)]
pub fn update_window_position(window: &gtk4::Window, x: i32, y: i32) {
    if window.is_layer_window() {
        window.set_margin(Edge::Left, x.max(0));
        window.set_margin(Edge::Top, y.max(0));
    }
}
