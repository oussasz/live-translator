//! Pure key classification and swallow decisions used by the low-level hook.
pub use crate::controller::{EnterMods, HotkeyId, KeyInput};
use crate::core::compose::EditOp;
pub use crate::core::hotkey::Mods;

pub const VK_BACK: u16 = 0x08;
pub const VK_TAB: u16 = 0x09;
pub const VK_RETURN: u16 = 0x0D;
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_END: u16 = 0x23;
pub const VK_HOME: u16 = 0x24;
pub const VK_LEFT: u16 = 0x25;
pub const VK_RIGHT: u16 = 0x27;
pub const VK_DELETE: u16 = 0x2E;
pub const VK_V: u16 = 0x56;
pub const VK_PACKET: u16 = 0xE7;

/// Maps one key-down to its meaning. `text` is what the layout produces for it
/// (from ToUnicodeEx or a VK_PACKET), if anything.
pub fn classify(vk: u16, mods: Mods, text: Option<String>) -> KeyInput {
    let real_ctrl = mods.ctrl && !mods.altgr;
    let real_alt = mods.alt && !mods.altgr;
    let any_mod = real_ctrl || real_alt || mods.win;
    match vk {
        VK_BACK if !real_alt && !mods.win => {
            return KeyInput::Edit(if real_ctrl { EditOp::DeleteWordBack } else { EditOp::Backspace });
        }
        VK_DELETE | VK_LEFT | VK_RIGHT | VK_HOME | VK_END => {
            if any_mod || mods.shift {
                return KeyInput::Other;
            }
            return KeyInput::Edit(match vk {
                VK_DELETE => EditOp::Delete,
                VK_LEFT => EditOp::Left,
                VK_RIGHT => EditOp::Right,
                VK_HOME => EditOp::Home,
                _ => EditOp::End,
            });
        }
        VK_RETURN => {
            return if real_alt || mods.win {
                KeyInput::Other
            } else if real_ctrl || mods.shift {
                KeyInput::ModEnter(EnterMods { shift: mods.shift, ctrl: real_ctrl })
            } else {
                KeyInput::Enter
            };
        }
        VK_ESCAPE if !any_mod => return KeyInput::Escape,
        VK_V if real_ctrl && !real_alt && !mods.win && !mods.shift => return KeyInput::Paste,
        _ => {}
    }
    match text {
        Some(t) if !any_mod && !t.is_empty() && !t.chars().any(char::is_control) => KeyInput::Text(t),
        _ => KeyInput::Other,
    }
}

/// True = swallow the key so the focused app never sees it.
pub fn decide(input: &KeyInput, active: bool, tail_empty: bool) -> bool {
    match input {
        KeyInput::Hotkey(_) => true,
        _ if !active => false,
        KeyInput::Text(_) | KeyInput::Paste | KeyInput::Escape => true,
        KeyInput::Enter | KeyInput::ModEnter(_) | KeyInput::Edit(_) => !tail_empty,
        KeyInput::Other => false,
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn shift_or_ctrl_enter_is_mod_enter() {
        assert_eq!(
            classify(VK_RETURN, Mods { shift: true, ..m() }, None),
            KeyInput::ModEnter(EnterMods { shift: true, ctrl: false })
        );
        assert_eq!(
            classify(VK_RETURN, Mods { ctrl: true, ..m() }, None),
            KeyInput::ModEnter(EnterMods { shift: false, ctrl: true })
        );
        assert_eq!(classify(VK_RETURN, Mods { alt: true, ..m() }, None), KeyInput::Other);
    }

    #[test]
    fn decide_mod_enter_like_enter() {
        let k = KeyInput::ModEnter(EnterMods { shift: true, ctrl: false });
        assert!(decide(&k, true, false));
        assert!(!decide(&k, true, true));
        assert!(!decide(&k, false, false));
    }
    use super::*;
    use crate::core::compose::EditOp;

    fn m() -> Mods {
        Mods::default()
    }

    #[test]
    fn classify_maps_edit_keys() {
        assert_eq!(classify(VK_BACK, m(), None), KeyInput::Edit(EditOp::Backspace));
        assert_eq!(classify(VK_BACK, Mods { ctrl: true, ..m() }, None), KeyInput::Edit(EditOp::DeleteWordBack));
        assert_eq!(classify(VK_LEFT, m(), None), KeyInput::Edit(EditOp::Left));
        assert_eq!(classify(VK_DELETE, m(), None), KeyInput::Edit(EditOp::Delete));
        assert_eq!(classify(VK_HOME, m(), None), KeyInput::Edit(EditOp::Home));
    }

    #[test]
    fn modified_navigation_is_other() {
        assert_eq!(classify(VK_LEFT, Mods { shift: true, ..m() }, None), KeyInput::Other);
        assert_eq!(classify(VK_LEFT, Mods { ctrl: true, ..m() }, None), KeyInput::Other);
    }

    #[test]
    fn classify_enter_escape_paste() {
        assert_eq!(classify(VK_RETURN, m(), None), KeyInput::Enter);
        assert_eq!(classify(VK_ESCAPE, m(), None), KeyInput::Escape);
        assert_eq!(classify(0x56, Mods { ctrl: true, ..m() }, Some("\u{16}".into())), KeyInput::Paste);
    }

    #[test]
    fn classify_text() {
        assert_eq!(classify(0x41, m(), Some("ب".into())), KeyInput::Text("ب".into()));
        assert_eq!(classify(0x41, Mods { shift: true, ..m() }, Some("A".into())), KeyInput::Text("A".into()));
        assert_eq!(classify(0x41, m(), None), KeyInput::Other);
    }

    #[test]
    fn classify_altgr_text() {
        let altgr = Mods { ctrl: true, alt: true, altgr: true, ..m() };
        assert_eq!(classify(0x45, altgr, Some("€".into())), KeyInput::Text("€".into()));
    }

    #[test]
    fn classify_ctrl_a_is_other() {
        assert_eq!(classify(0x41, Mods { ctrl: true, ..m() }, Some("\u{1}".into())), KeyInput::Other);
    }

    #[test]
    fn control_chars_are_not_text() {
        assert_eq!(classify(VK_TAB, m(), Some("\t".into())), KeyInput::Other);
    }

    #[test]
    fn decide_inactive_passes_text() {
        assert!(!decide(&KeyInput::Text("a".into()), false, true));
        assert!(!decide(&KeyInput::Escape, false, true));
    }

    #[test]
    fn decide_hotkeys_always_swallowed() {
        assert!(decide(&KeyInput::Hotkey(HotkeyId::Toggle), false, true));
    }

    #[test]
    fn decide_active_swallows_text() {
        assert!(decide(&KeyInput::Text("a".into()), true, true));
        assert!(decide(&KeyInput::Paste, true, true));
        assert!(decide(&KeyInput::Escape, true, true));
        assert!(!decide(&KeyInput::Other, true, false));
    }

    #[test]
    fn decide_edit_on_empty_tail_passes() {
        assert!(!decide(&KeyInput::Edit(EditOp::Backspace), true, true));
        assert!(decide(&KeyInput::Edit(EditOp::Backspace), true, false));
    }

    #[test]
    fn decide_enter_empty_tail_passes() {
        assert!(!decide(&KeyInput::Enter, true, true));
        assert!(decide(&KeyInput::Enter, true, false));
    }
}
