//! The real `Field`: writes with SendInput, reads with UI Automation.
use crate::controller::{Field, FocusInfo, WriteOutcome};
use crate::core::replace::ReplacePlan;
use crate::core::types::Rect;
use crate::win::{clipboard, input, uia};
use std::time::Duration;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

#[derive(Default)]
pub struct WinField;

impl WinField {
    pub fn new() -> Self {
        Self
    }
}

impl Field for WinField {
    fn focus(&self) -> Option<FocusInfo> {
        uia::focused()
    }

    fn write(&self, plan: &ReplacePlan, expect: &str) -> WriteOutcome {
        input::prepare_injection();
        input::execute(plan);
        // UIA updates cross-process asynchronously: give it up to ~180 ms before calling it a failure.
        for _ in 0..6 {
            std::thread::sleep(Duration::from_millis(30));
            match uia::focused_value() {
                None => return WriteOutcome::Ok,
                Some(v) if v.contains(expect) => return WriteOutcome::Ok,
                Some(_) => {}
            }
        }
        WriteOutcome::Failed
    }

    fn send_enter(&self) {
        input::prepare_injection();
        input::send_enter();
    }

    fn is_focused(&self, hwnd: isize) -> bool {
        unsafe { GetForegroundWindow().0 as isize == hwnd }
    }

    fn set_clipboard(&self, text: &str) {
        clipboard::set(text);
    }

    fn clipboard_text(&self) -> Option<String> {
        clipboard::get()
    }

    fn read_selection(&self) -> Option<(String, Option<Rect>)> {
        if let Some(sel) = uia::selection() {
            return Some(sel);
        }
        input::wait_modifiers_released(600);
        clipboard::with_restored(|| {
            clipboard::set("");
            input::send_ctrl_c();
            std::thread::sleep(Duration::from_millis(150));
            clipboard::get().filter(|t| !t.trim().is_empty()).map(|t| (t, None))
        })
    }
}
