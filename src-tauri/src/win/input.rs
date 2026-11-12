//! Synthesized keyboard input. Every event carries INJECT_TAG so our own hook ignores it.
use crate::core::replace::ReplacePlan;
use crate::win::hook::INJECT_TAG;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_DELETE, VK_LEFT, VK_RETURN, VK_SHIFT,
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

/// Builds the keystrokes for a plan.
pub fn plan_inputs(plan: &ReplacePlan) -> Vec<INPUT> {
    let mut v = Vec::new();
    if plan.select_left > 0 {
        v.push(ki(VK_SHIFT, 0, KEYBD_EVENT_FLAGS(0), INJECT_TAG));
        for _ in 0..plan.select_left {
            tap(&mut v, VK_LEFT, true, INJECT_TAG);
        }
        v.push(ki(VK_SHIFT, 0, KEYEVENTF_KEYUP, INJECT_TAG));
        if plan.type_text.is_empty() {
            tap(&mut v, VK_DELETE, true, INJECT_TAG);
        }
    }
    push_text(&mut v, &plan.type_text, INJECT_TAG);
    v
}

pub fn execute(plan: &ReplacePlan) {
    send(&plan_inputs(plan));
}

pub fn send_enter() {
    let mut v = Vec::new();
    tap(&mut v, VK_RETURN, false, INJECT_TAG);
    send(&v);
}

pub fn send_ctrl_c() {
    let mut v = vec![ki(VK_CONTROL, 0, KEYBD_EVENT_FLAGS(0), INJECT_TAG)];
    tap(&mut v, VIRTUAL_KEY(0x43), false, INJECT_TAG);
    v.push(ki(VK_CONTROL, 0, KEYEVENTF_KEYUP, INJECT_TAG));
    send(&v);
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
    use super::*;

    fn parts(i: &INPUT) -> (u16, u16, u32, usize) {
        let k = unsafe { i.Anonymous.ki };
        (k.wVk.0, k.wScan, k.dwFlags.0, k.dwExtraInfo)
    }

    #[test]
    fn plan_inputs_select_then_type_all_tagged() {
        let v = plan_inputs(&ReplacePlan { select_left: 2, type_text: "é".into() });
        // shift down, 2x(left down/up), shift up, 'é' down/up
        assert_eq!(v.len(), 1 + 4 + 1 + 2);
        assert_eq!(parts(&v[0]).0, VK_SHIFT.0);
        assert_eq!(parts(&v[1]).0, VK_LEFT.0);
        assert_eq!(parts(&v[6]).1, 'é' as u16);
        assert!(v.iter().all(|i| parts(i).3 == INJECT_TAG));
    }

    #[test]
    fn plan_inputs_delete_when_empty() {
        let v = plan_inputs(&ReplacePlan { select_left: 1, type_text: String::new() });
        assert_eq!(parts(&v[v.len() - 2]).0, VK_DELETE.0);
    }

    #[test]
    fn emoji_sends_surrogate_pair() {
        let mut v = Vec::new();
        push_text(&mut v, "👍", INJECT_TAG);
        assert_eq!(v.len(), 4);
    }
}
