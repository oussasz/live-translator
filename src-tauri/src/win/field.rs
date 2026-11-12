//! The real `Field`: writes with SendInput, reads with UI Automation.
use crate::controller::{Field, FocusInfo, WriteOutcome};
use crate::core::replace::ReplacePlan;
use crate::core::types::Rect;
use crate::win::{clipboard, input, uia};
use std::time::Duration;

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
        input::execute(plan);
        std::thread::sleep(Duration::from_millis(30));
        match uia::focused_value() {
            Some(v) if !v.contains(expect) => WriteOutcome::Failed,
            _ => WriteOutcome::Ok,
        }
    }

    fn send_enter(&self) {
        input::send_enter();
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
