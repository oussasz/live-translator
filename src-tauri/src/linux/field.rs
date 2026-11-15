//! The Linux `Field`: Backspace through the virtual keyboard, then paste the translation.
use crate::controller::{EnterMods, Field, FocusInfo, WriteOutcome};
use crate::core::replace::ReplacePlan;
use crate::core::types::Rect;
use crate::linux::input::{chord_seq, enter_seq, paste_seq, play_clean, tap_seq};
use crate::linux::keys::{KEY_BACKSPACE, KEY_C, KEY_LEFTCTRL};
use crate::linux::{clipboard, focus};
use std::time::Duration;

#[derive(Default)]
pub struct NativeField;

impl NativeField {
    pub fn new() -> Self {
        Self
    }
}

pub fn backspace_seq(n: usize) -> Vec<(u16, i32)> {
    (0..n).flat_map(|_| tap_seq(KEY_BACKSPACE)).collect()
}

impl Field for NativeField {
    fn focus(&self) -> Option<FocusInfo> {
        focus::focused_info()
    }

    fn write(&self, plan: &ReplacePlan, _expect: &str) -> WriteOutcome {
        if plan.select_left > 0 {
            play_clean(backspace_seq(plan.select_left));
        }
        if !plan.type_text.is_empty() {
            let terminal = focus::is_terminal();
            clipboard::with_restored(|| {
                clipboard::set(&plan.type_text);
                std::thread::sleep(Duration::from_millis(30));
                play_clean(paste_seq(terminal));
                // apps read the clipboard asynchronously: keep our text there a moment
                std::thread::sleep(Duration::from_millis(180));
            });
        }
        WriteOutcome::Ok
    }

    fn send_enter(&self, mods: EnterMods) {
        play_clean(enter_seq(mods));
    }

    fn set_clipboard(&self, text: &str) {
        clipboard::set(text);
    }

    fn clipboard_text(&self) -> Option<String> {
        clipboard::get()
    }

    fn read_selection(&self) -> Option<(String, Option<Rect>)> {
        if let Some(sel) = focus::selection() {
            return Some(sel);
        }
        clipboard::with_restored(|| {
            clipboard::set("");
            play_clean(chord_seq(&[KEY_LEFTCTRL], KEY_C));
            std::thread::sleep(Duration::from_millis(150));
            clipboard::get().filter(|t| !t.trim().is_empty()).map(|t| (t, None))
        })
    }

    fn is_focused(&self, hwnd: isize) -> bool {
        hwnd == 0 || focus::current_id() == hwnd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backspaces_are_press_release_pairs() {
        assert_eq!(backspace_seq(2), vec![(KEY_BACKSPACE, 1), (KEY_BACKSPACE, 0), (KEY_BACKSPACE, 1), (KEY_BACKSPACE, 0)]);
        assert!(backspace_seq(0).is_empty());
    }
}
