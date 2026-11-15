//! Overlay windows (strip, popup) on Linux. The app runs its windows under XWayland
//! (GDK_BACKEND=x11, set in main.rs) so they can be placed at screen positions.
use crate::core::types::Point;
use gtk::prelude::*;
use tauri::WebviewWindow;

pub fn make_noactivate(w: &WebviewWindow) {
    if let Ok(g) = w.gtk_window() {
        g.set_accept_focus(false);
        g.set_focus_on_map(false);
        g.set_keep_above(true);
        g.set_skip_taskbar_hint(true);
        g.set_skip_pager_hint(true);
        g.set_type_hint(gtk::gdk::WindowTypeHint::Utility);
    }
}

pub fn show_at(w: &WebviewWindow, p: Point) {
    let _ = w.set_position(tauri::PhysicalPosition::new(p.x, p.y));
    let _ = w.show();
}

pub fn hide(w: &WebviewWindow) {
    let _ = w.hide();
}
