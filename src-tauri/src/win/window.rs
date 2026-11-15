//! Non-activating overlay windows (strip, popup) on Windows.
use crate::core::types::Point;
use tauri::WebviewWindow;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

fn hwnd_of(w: &WebviewWindow) -> Option<HWND> {
    w.hwnd().ok().map(|h| HWND(h.0))
}

pub fn make_noactivate(w: &WebviewWindow) {
    if let Some(h) = hwnd_of(w) {
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
        }
    }
}

pub fn show_at(w: &WebviewWindow, p: Point) {
    if let Some(h) = hwnd_of(w) {
        unsafe {
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), p.x, p.y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    }
}

pub fn hide(w: &WebviewWindow) {
    if let Some(h) = hwnd_of(w) {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}
