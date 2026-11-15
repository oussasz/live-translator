//! Grabs the keyboards, decides per key whether to swallow it (translate mode) or forward it
//! through the virtual keyboard, and reports events to the controller.
use crate::controller::{HookState, InputEvent, KeyInput};
use crate::core::hotkey::Mods;
use crate::core::keys::{classify, decide, HotkeyTable};
use crate::linux::input::{self, VirtualKeyboard, MODS, VKBD};
use crate::linux::keys::{is_modifier, vk_for, KeyText, XkbText};
use crate::linux::{layout, POINTER_IN_OVERLAY};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    Forward,
    Swallow,
}

/// Per-key memory: was the press swallowed (so its repeats/release are too)?
pub struct Swallowed([bool; 768]);

impl Default for Swallowed {
    fn default() -> Self {
        Self([false; 768])
    }
}

/// Pure routing for one non-modifier key event.
/// `text` is only consulted for presses while active. Returns the route and the event to report.
pub fn route(
    code: u16,
    value: i32,
    mods: Mods,
    hotkey: Option<crate::controller::HotkeyId>,
    text: impl FnOnce() -> KeyText,
    state: (bool, bool, bool), // (active, tail_empty, paused)
    swallowed: &mut Swallowed,
) -> (Route, Option<KeyInput>) {
    let (active, tail_empty, paused) = state;
    let idx = (code as usize).min(767);
    if value == 0 {
        let was = std::mem::replace(&mut swallowed.0[idx], false);
        return (if was { Route::Swallow } else { Route::Forward }, None);
    }
    if value == 2 && !swallowed.0[idx] {
        return (Route::Forward, None); // repeat of a key the app already has
    }
    if paused {
        return (Route::Forward, None);
    }
    let input = match hotkey {
        Some(id) => KeyInput::Hotkey(id),
        None if !active => return (Route::Forward, None),
        None => match text() {
            KeyText::Dead => {
                swallowed.0[idx] = true;
                return (Route::Swallow, None);
            }
            KeyText::Text(t) => classify(vk_for(code), mods, Some(t)),
            KeyText::Nothing => classify(vk_for(code), mods, None),
        },
    };
    let swallow = decide(&input, active, tail_empty);
    swallowed.0[idx] = swallow;
    if swallow {
        (Route::Swallow, Some(input))
    } else {
        (Route::Forward, if active { Some(KeyInput::Other) } else { None })
    }
}

struct Shared {
    hotkeys: HotkeyTable,
    state: Arc<HookState>,
    tx: UnboundedSender<InputEvent>,
    xkb: Mutex<Option<XkbText>>,
    swallowed: Mutex<Swallowed>,
}

fn handle_key(sh: &Shared, code: u16, value: i32) {
    // keep modifier + xkb state in sync for every key
    let mods = {
        let mut m = MODS.lock().unwrap();
        m.update(code, value);
        m.mods()
    };
    if let Some(x) = sh.xkb.lock().unwrap().as_mut() {
        x.track(code, value);
    }
    if is_modifier(code) {
        if let Some(v) = VKBD.lock().unwrap().as_mut() {
            v.forward(code, value);
        }
        return;
    }
    let vk = vk_for(code);
    let hotkey = if value == 1 {
        sh.hotkeys.read().ok().and_then(|t| t.iter().find(|(h, _)| h.matches(vk, mods)).map(|(_, id)| *id))
    } else {
        None
    };
    let st = &sh.state;
    let state = (
        st.active.load(Ordering::SeqCst),
        st.tail_empty.load(Ordering::SeqCst),
        st.paused.load(Ordering::SeqCst),
    );
    let (route, input) = {
        let mut sw = sh.swallowed.lock().unwrap();
        route(
            code,
            value,
            mods,
            hotkey,
            || sh.xkb.lock().unwrap().as_mut().map(|x| x.text(code)).unwrap_or(KeyText::Nothing),
            state,
            &mut sw,
        )
    };
    if let Some(i) = input {
        let _ = sh.tx.send(InputEvent::Key(i));
    }
    if route == Route::Forward {
        if let Some(v) = VKBD.lock().unwrap().as_mut() {
            v.forward(code, value);
        }
    }
}

fn keyboard_thread(sh: Arc<Shared>, mut dev: evdev::Device) {
    if let Ok(keys) = dev.get_key_state() {
        let pressed: Vec<u16> = keys.iter().map(|k| k.0).collect();
        let mut m = MODS.lock().unwrap();
        for c in pressed {
            m.update(c, 1);
        }
    }
    if dev.grab().is_err() {
        return;
    }
    loop {
        let events: Vec<(u16, i32)> = match dev.fetch_events() {
            Ok(evs) => evs.filter(|e| e.event_type() == evdev::EventType::KEY).map(|e| (e.code(), e.value())).collect(),
            Err(_) => return, // device unplugged; the kernel drops the grab
        };
        for (code, value) in events {
            handle_key(&sh, code, value);
        }
    }
}

fn pointer_thread(tx: UnboundedSender<InputEvent>, mut dev: evdev::Device) {
    loop {
        let Ok(evs) = dev.fetch_events() else { return };
        for e in evs {
            let left_or_right = e.code() == evdev::KeyCode::BTN_LEFT.0 || e.code() == evdev::KeyCode::BTN_RIGHT.0;
            if e.event_type() == evdev::EventType::KEY && left_or_right && e.value() == 1 && !POINTER_IN_OVERLAY.load(Ordering::SeqCst)
            {
                // Linux can't read global pointer coordinates: report "somewhere outside our windows".
                let _ = tx.send(InputEvent::MouseDown { x: i32::MIN / 2, y: i32::MIN / 2 });
            }
        }
    }
}

/// Error shown to the user when the keyboard can't be captured.
pub static HOOK_ERROR: Mutex<Option<String>> = Mutex::new(None);

pub fn start_hook(hotkeys: HotkeyTable, state: Arc<HookState>, tx: UnboundedSender<InputEvent>) {
    match VirtualKeyboard::new() {
        Ok(v) => *VKBD.lock().unwrap() = Some(v),
        Err(e) => {
            *HOOK_ERROR.lock().unwrap() = Some(format!("Can't create the virtual keyboard (/dev/uinput): {e}"));
            return;
        }
    }
    let (layout_name, variant) = layout::current();
    let sh = Arc::new(Shared {
        hotkeys,
        state,
        tx: tx.clone(),
        xkb: Mutex::new(XkbText::new(&layout_name, &variant).or_else(|| XkbText::new("us", ""))),
        swallowed: Mutex::new(Swallowed::default()),
    });
    let kbds = input::keyboards();
    if kbds.is_empty() {
        *HOOK_ERROR.lock().unwrap() = Some("No readable keyboard in /dev/input".into());
    }
    for (_, dev) in kbds {
        let sh = sh.clone();
        std::thread::Builder::new().name("lt-kbd".into()).spawn(move || keyboard_thread(sh, dev)).ok();
    }
    for (_, dev) in input::pointers() {
        let tx = tx.clone();
        std::thread::Builder::new().name("lt-ptr".into()).spawn(move || pointer_thread(tx, dev)).ok();
    }
    // follow keyboard layout switches (e.g. Meta+Space between us and ara)
    let sh2 = sh.clone();
    std::thread::Builder::new()
        .name("lt-layout".into())
        .spawn(move || {
            let mut last = (layout_name, variant);
            loop {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let now = layout::current();
                if now != last {
                    if let Some(x) = XkbText::new(&now.0, &now.1) {
                        *sh2.xkb.lock().unwrap() = Some(x);
                    }
                    last = now;
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::HotkeyId;

    fn m() -> Mods {
        Mods::default()
    }
    fn txt(s: &str) -> impl FnOnce() -> KeyText + '_ {
        move || KeyText::Text(s.to_string())
    }

    #[test]
    fn forward_decision_passes_repeats_and_releases() {
        let mut sw = Swallowed::default();
        // inactive: press, repeat, release all forwarded, nothing reported
        assert_eq!(route(30, 1, m(), None, txt("a"), (false, true, false), &mut sw), (Route::Forward, None));
        assert_eq!(route(30, 2, m(), None, txt("a"), (false, true, false), &mut sw), (Route::Forward, None));
        assert_eq!(route(30, 0, m(), None, txt("a"), (false, true, false), &mut sw), (Route::Forward, None));
    }

    #[test]
    fn swallowed_press_also_swallows_release() {
        let mut sw = Swallowed::default();
        let (r, i) = route(30, 1, m(), None, txt("a"), (true, true, false), &mut sw);
        assert_eq!(r, Route::Swallow);
        assert_eq!(i, Some(KeyInput::Text("a".into())));
        // key held: repeats are swallowed and reported again
        assert_eq!(route(30, 2, m(), None, txt("a"), (true, false, false), &mut sw).0, Route::Swallow);
        assert_eq!(route(30, 0, m(), None, txt("a"), (true, false, false), &mut sw), (Route::Swallow, None));
    }

    #[test]
    fn release_of_key_pressed_before_translate_mode_is_forwarded() {
        let mut sw = Swallowed::default();
        route(30, 1, m(), None, txt("a"), (false, true, false), &mut sw);
        // translate mode starts while the key is held; its release must still reach the app
        assert_eq!(route(30, 0, m(), None, txt("a"), (true, true, false), &mut sw), (Route::Forward, None));
    }

    #[test]
    fn hotkey_swallowed_even_when_inactive() {
        let mut sw = Swallowed::default();
        let (r, i) = route(20, 1, Mods { ctrl: true, alt: true, ..m() }, Some(HotkeyId::Toggle), txt("t"), (false, true, false), &mut sw);
        assert_eq!(r, Route::Swallow);
        assert_eq!(i, Some(KeyInput::Hotkey(HotkeyId::Toggle)));
    }

    #[test]
    fn dead_key_swallowed_without_event() {
        let mut sw = Swallowed::default();
        assert_eq!(route(26, 1, m(), None, || KeyText::Dead, (true, true, false), &mut sw), (Route::Swallow, None));
    }

    #[test]
    fn passthrough_while_active_reports_other() {
        let mut sw = Swallowed::default();
        // Backspace with an empty tail goes to the app, and the controller hears "Other"
        let (r, i) = route(14, 1, m(), None, || KeyText::Nothing, (true, true, false), &mut sw);
        assert_eq!(r, Route::Forward);
        assert_eq!(i, Some(KeyInput::Other));
    }

    #[test]
    fn paused_forwards_everything() {
        let mut sw = Swallowed::default();
        assert_eq!(route(30, 1, m(), None, txt("a"), (true, false, true), &mut sw), (Route::Forward, None));
    }
}
