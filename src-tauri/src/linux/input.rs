//! Keyboard discovery and the uinput virtual keyboard we type through.
use crate::controller::EnterMods;
use crate::linux::keys::*;
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, Device, EventType, InputEvent, KeyCode};
use std::path::PathBuf;
use std::sync::Mutex;

pub const VIRTUAL_NAME: &str = "Live Translator virtual keyboard";

/// Keyboards = devices that report letter keys (skipping our own virtual device).
pub fn keyboards() -> Vec<(PathBuf, Device)> {
    evdev::enumerate()
        .filter(|(_, d)| d.name() != Some(VIRTUAL_NAME))
        .filter(|(_, d)| {
            d.supported_keys()
                .is_some_and(|k| k.contains(KeyCode::KEY_A) && k.contains(KeyCode::KEY_Z) && k.contains(KeyCode::KEY_ENTER))
        })
        .collect()
}

/// Pointer devices (for "clicked somewhere" detection; read, never grabbed).
pub fn pointers() -> Vec<(PathBuf, Device)> {
    evdev::enumerate()
        .filter(|(_, d)| d.name() != Some(VIRTUAL_NAME))
        .filter(|(_, d)| d.supported_keys().is_some_and(|k| k.contains(KeyCode::BTN_LEFT)))
        .filter(|(_, d)| !d.supported_keys().is_some_and(|k| k.contains(KeyCode::KEY_A)))
        .collect()
}

fn key_event(code: u16, value: i32) -> InputEvent {
    InputEvent::new(EventType::KEY.0, code, value)
}

/// Key sequences (pure, so they can be tested).
pub fn tap_seq(code: u16) -> Vec<(u16, i32)> {
    vec![(code, 1), (code, 0)]
}

pub fn chord_seq(mods: &[u16], code: u16) -> Vec<(u16, i32)> {
    let mut v: Vec<(u16, i32)> = mods.iter().map(|m| (*m, 1)).collect();
    v.extend(tap_seq(code));
    v.extend(mods.iter().rev().map(|m| (*m, 0)));
    v
}

/// Release the modifiers the user is holding, run `seq`, then press them again,
/// so injected keys never combine with a held Shift/Ctrl/Alt.
pub fn clean_seq(held: &[u16], seq: Vec<(u16, i32)>) -> Vec<(u16, i32)> {
    let mut v: Vec<(u16, i32)> = held.iter().map(|m| (*m, 0)).collect();
    v.extend(seq);
    v.extend(held.iter().map(|m| (*m, 1)));
    v
}

pub fn paste_seq(terminal: bool) -> Vec<(u16, i32)> {
    if terminal {
        chord_seq(&[KEY_LEFTCTRL, KEY_LEFTSHIFT], KEY_V)
    } else {
        chord_seq(&[KEY_LEFTCTRL], KEY_V)
    }
}

pub fn enter_seq(mods: EnterMods) -> Vec<(u16, i32)> {
    let mut m = Vec::new();
    if mods.ctrl {
        m.push(KEY_LEFTCTRL);
    }
    if mods.shift {
        m.push(KEY_LEFTSHIFT);
    }
    chord_seq(&m, KEY_ENTER)
}

pub struct VirtualKeyboard {
    dev: VirtualDevice,
}

impl VirtualKeyboard {
    pub fn new() -> std::io::Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for code in 1..=248u16 {
            keys.insert(KeyCode(code));
        }
        let dev = VirtualDevice::builder()?.name(VIRTUAL_NAME).with_keys(&keys)?.build()?;
        Ok(Self { dev })
    }

    /// Forward one physical key event unchanged.
    pub fn forward(&mut self, code: u16, value: i32) {
        let _ = self.dev.emit(&[key_event(code, value)]);
    }

    pub fn play(&mut self, seq: &[(u16, i32)]) {
        for (code, value) in seq {
            let _ = self.dev.emit(&[key_event(*code, *value)]);
            // a short gap keeps compositors and apps from coalescing or reordering keys
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

/// The one virtual keyboard, shared by the hook threads (forwarding) and the field (typing).
pub static VKBD: Mutex<Option<VirtualKeyboard>> = Mutex::new(None);

/// Current physical modifiers (updated by the hook threads).
pub static MODS: Mutex<ModTracker> = Mutex::new(ModTracker {
    lctrl: false, rctrl: false, lshift: false, rshift: false, lalt: false, ralt: false, lmeta: false, rmeta: false,
});

pub fn play_clean(seq: Vec<(u16, i32)>) {
    let held = MODS.lock().unwrap().held();
    if let Some(v) = VKBD.lock().unwrap().as_mut() {
        v.play(&clean_seq(&held, seq));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_combo_for_terminal_role() {
        assert_eq!(paste_seq(false), vec![(KEY_LEFTCTRL, 1), (KEY_V, 1), (KEY_V, 0), (KEY_LEFTCTRL, 0)]);
        assert_eq!(
            paste_seq(true),
            vec![(KEY_LEFTCTRL, 1), (KEY_LEFTSHIFT, 1), (KEY_V, 1), (KEY_V, 0), (KEY_LEFTSHIFT, 0), (KEY_LEFTCTRL, 0)]
        );
    }

    #[test]
    fn enter_with_shift_and_clean_modifiers() {
        let seq = clean_seq(&[KEY_RIGHTSHIFT], enter_seq(EnterMods { shift: true, ctrl: false }));
        assert_eq!(
            seq,
            vec![
                (KEY_RIGHTSHIFT, 0),
                (KEY_LEFTSHIFT, 1),
                (KEY_ENTER, 1),
                (KEY_ENTER, 0),
                (KEY_LEFTSHIFT, 0),
                (KEY_RIGHTSHIFT, 1)
            ]
        );
    }
}
