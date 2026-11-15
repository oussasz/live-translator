//! The operating-system layer behind one name. Shared code uses only `crate::platform`.
pub use crate::core::keys::HotkeyTable;
use serde::Serialize;

#[cfg(windows)]
pub use crate::win::{
    clipboard, field::WinField as NativeField, hook::start_hook, keystore, uia::watch_focus, window,
};

#[cfg(target_os = "linux")]
pub use crate::linux::{clipboard, field::NativeField, focus::watch_focus, hook::start_hook, keystore, window};

/// Something the user must fix for translate mode to work (Linux permissions).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PlatformStatus {
    pub os: &'static str,
    pub problem: Option<String>,
    pub fix: Option<String>,
}

#[cfg(windows)]
pub fn init(_app: &tauri::AppHandle) {}

#[cfg(windows)]
pub fn overlay_hover(_inside: bool) {}

#[cfg(windows)]
pub fn status() -> PlatformStatus {
    PlatformStatus { os: "windows", ..Default::default() }
}

#[cfg(target_os = "linux")]
pub fn init(app: &tauri::AppHandle) {
    if let Ok(Some(m)) = app.primary_monitor() {
        let a = m.work_area();
        crate::linux::focus::set_work_area(crate::core::types::Rect {
            x: a.position.x,
            y: a.position.y,
            w: a.size.width as i32,
            h: a.size.height as i32,
        });
    }
}

#[cfg(target_os = "linux")]
pub fn overlay_hover(inside: bool) {
    crate::linux::POINTER_IN_OVERLAY.store(inside, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(target_os = "linux")]
pub fn status() -> PlatformStatus {
    let p = crate::linux::permissions::check();
    let hook_err = crate::linux::hook::HOOK_ERROR.lock().unwrap().clone();
    let problem = if !p.input_ok || !p.uinput_ok {
        Some("Live Translator can't use the keyboard yet.".to_string())
    } else {
        hook_err
    };
    PlatformStatus { os: "linux", problem, fix: p.fix }
}
