//! Synthesized keyboard input. Every event carries INJECT_TAG so our own hook ignores it.
use crate::controller::EnterMods;
use crate::core::replace::ReplacePlan;
use crate::win::hook::INJECT_TAG;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_RETURN,
};

pub fn ki(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS, extra: usize) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: extra } },
    }
}

/// Press + release of a virtual key.
pub fn tap(out: &mut Vec<INPUT>, vk: VIRTUAL_KEY, extended: bool, extra: usize) {
    let ext = if extended { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) };
    out.push(ki(vk, 0, ext, extra));
    out.push(ki(vk, 0, ext | KEYEVENTF_KEYUP, extra));
}

/// Unicode text as KEYEVENTF_UNICODE packets, one per UTF-16 unit.
pub fn push_text(out: &mut Vec<INPUT>, text: &str, extra: usize) {
    for unit in text.encode_utf16() {
        out.push(ki(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE, extra));
        out.push(ki(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP, extra));
    }
}

pub fn send(inputs: &[INPUT]) {
    if !inputs.is_empty() {
        unsafe {
            SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

/// Builds the keystrokes for a plan: Backspace the old suffix, then type the new one.
/// Backspace deletes in logical order, so it also works in right-to-left fields where
/// arrow keys move visually.
pub fn plan_inputs(plan: &ReplacePlan) -> Vec<INPUT> {
    let mut v = Vec::new();
    for _ in 0..plan.select_left {
        tap(&mut v, VK_BACK, false, INJECT_TAG);
    }
    push_text(&mut v, &plan.type_text, INJECT_TAG);
    v
}

pub fn execute(plan: &ReplacePlan) {
    send(&plan_inputs(plan));
}

/// Enter, wrapped in the given modifiers (Shift+Enter = new line in chat apps).
pub fn enter_inputs(mods: EnterMods) -> Vec<INPUT> {
    let mut v = Vec::new();
    let held: Vec<VIRTUAL_KEY> = [(mods.ctrl, VK_CONTROL), (mods.shift, VIRTUAL_KEY(0x10))]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, k)| k)
        .collect();
    for k in &held {
        v.push(ki(*k, 0, KEYBD_EVENT_FLAGS(0), INJECT_TAG));
    }
    tap(&mut v, VK_RETURN, false, INJECT_TAG);
    for k in held.iter().rev() {
        v.push(ki(*k, 0, KEYEVENTF_KEYUP, INJECT_TAG));
    }
    v
}

pub fn send_enter(mods: EnterMods) {
    send(&enter_inputs(mods));
}

pub fn send_ctrl_c() {
    let mut v = vec![ki(VK_CONTROL, 0, KEYBD_EVENT_FLAGS(0), INJECT_TAG)];
    tap(&mut v, VIRTUAL_KEY(0x43), false, INJECT_TAG);
    v.push(ki(VK_CONTROL, 0, KEYEVENTF_KEYUP, INJECT_TAG));
    send(&v);
}

const MODIFIERS: [u16; 8] = [0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x5B, 0x5C];

/// Tagged key-ups for the given held modifier keys.
pub fn release_inputs(held: &[u16]) -> Vec<INPUT> {
    held.iter().map(|vk| ki(VIRTUAL_KEY(*vk), 0, KEYEVENTF_KEYUP, INJECT_TAG)).collect()
}

/// Before injecting: wait briefly for the user to let go of modifiers, then release any still held,
/// so Shift+Enter / Ctrl+Shift+Left / Alt+Shift (language switch) can't happen by accident.
pub fn prepare_injection() {
    wait_modifiers_released(150);
    let held: Vec<u16> = MODIFIERS
        .iter()
        .copied()
        .filter(|k| unsafe { GetAsyncKeyState(*k as i32) } as u16 & 0x8000 != 0)
        .collect();
    send(&release_inputs(&held));
}

/// Waits (up to `max_ms`) until Ctrl/Alt/Shift/Win are physically released.
pub fn wait_modifiers_released(max_ms: u64) {
    let keys = [0x10, 0x11, 0x12, 0x5B, 0x5C];
    let start = std::time::Instant::now();
    while start.elapsed().as_millis() < max_ms as u128 {
        let held = keys.iter().any(|k| unsafe { GetAsyncKeyState(*k) } as u16 & 0x8000 != 0);
        if !held {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn enter_inputs_wrap_modifiers_all_tagged() {
        let v = enter_inputs(EnterMods { shift: true, ctrl: false });
        // shift down, enter down, enter up, shift up
        assert_eq!(v.len(), 4);
        assert_eq!(parts(&v[0]).0, 0x10);
        assert_eq!(parts(&v[1]).0, 0x0D);
        assert_eq!(parts(&v[3]).0, 0x10);
        assert!(parts(&v[3]).2 & KEYEVENTF_KEYUP.0 != 0);
        assert!(v.iter().all(|i| parts(i).3 == INJECT_TAG));
        assert_eq!(enter_inputs(EnterMods::default()).len(), 2);
    }
    use super::*;

    fn parts(i: &INPUT) -> (u16, u16, u32, usize) {
        let k = unsafe { i.Anonymous.ki };
        (k.wVk.0, k.wScan, k.dwFlags.0, k.dwExtraInfo)
    }

    #[test]
    fn plan_inputs_backspace_then_type_all_tagged() {
        let v = plan_inputs(&ReplacePlan { select_left: 2, type_text: "é".into() });
        // 2x(backspace down/up), 'é' down/up — Backspace deletes in logical order, so RTL text works too
        assert_eq!(v.len(), 4 + 2);
        assert_eq!(parts(&v[0]).0, VK_BACK.0);
        assert_eq!(parts(&v[2]).0, VK_BACK.0);
        assert_eq!(parts(&v[4]).1, 'é' as u16);
        assert!(v.iter().all(|i| parts(i).3 == INJECT_TAG));
    }

    #[test]
    fn plan_inputs_only_backspaces_when_empty() {
        let v = plan_inputs(&ReplacePlan { select_left: 3, type_text: String::new() });
        assert_eq!(v.len(), 6);
        assert!(v.iter().all(|i| parts(i).0 == VK_BACK.0));
    }

    #[test]
    fn release_inputs_are_tagged_keyups() {
        let v = release_inputs(&[0xA0, 0xA2]);
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|i| parts(i).2 & KEYEVENTF_KEYUP.0 != 0 && parts(i).3 == INJECT_TAG));
        assert_eq!(parts(&v[1]).0, 0xA2);
    }

    #[test]
    fn emoji_sends_surrogate_pair() {
        let mut v = Vec::new();
        push_text(&mut v, "👍", INJECT_TAG);
        assert_eq!(v.len(), 4);
    }
}
