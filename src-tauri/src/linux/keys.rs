//! evdev key codes -> the shared (Windows-style) virtual keys, modifier tracking,
//! and xkbcommon text lookup for the active layout (dead keys via compose).
use crate::core::hotkey::Mods;
use xkbcommon::xkb;

pub const KEY_ESC: u16 = 1;
pub const KEY_BACKSPACE: u16 = 14;
pub const KEY_TAB: u16 = 15;
pub const KEY_ENTER: u16 = 28;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_RIGHTSHIFT: u16 = 54;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_SPACE: u16 = 57;
pub const KEY_KPENTER: u16 = 96;
pub const KEY_RIGHTCTRL: u16 = 97;
pub const KEY_RIGHTALT: u16 = 100;
pub const KEY_HOME: u16 = 102;
pub const KEY_LEFT: u16 = 105;
pub const KEY_RIGHT: u16 = 106;
pub const KEY_END: u16 = 107;
pub const KEY_DELETE: u16 = 111;
pub const KEY_LEFTMETA: u16 = 125;
pub const KEY_RIGHTMETA: u16 = 126;
pub const KEY_V: u16 = 47;
pub const KEY_C: u16 = 46;
pub const KEY_LEFTBRACE: u16 = 26;
pub const KEY_E: u16 = 18;
pub const KEY_H: u16 = 35;
pub const KEY_A: u16 = 30;

const LETTERS: [(u16, u8); 26] = [
    (30, b'A'), (48, b'B'), (46, b'C'), (32, b'D'), (18, b'E'), (33, b'F'), (34, b'G'), (35, b'H'), (23, b'I'),
    (36, b'J'), (37, b'K'), (38, b'L'), (50, b'M'), (49, b'N'), (24, b'O'), (25, b'P'), (16, b'Q'), (19, b'R'),
    (31, b'S'), (20, b'T'), (22, b'U'), (47, b'V'), (17, b'W'), (45, b'X'), (21, b'Y'), (44, b'Z'),
];

/// The shared virtual-key code for an evdev key (0 = a plain text key with no special meaning).
/// Letters/digits are positional (US positions), like Windows hotkeys.
pub fn vk_for(code: u16) -> u16 {
    match code {
        KEY_ESC => 0x1B,
        KEY_BACKSPACE => 0x08,
        KEY_TAB => 0x09,
        KEY_ENTER | KEY_KPENTER => 0x0D,
        KEY_SPACE => 0x20,
        KEY_HOME => 0x24,
        KEY_LEFT => 0x25,
        KEY_RIGHT => 0x27,
        KEY_END => 0x23,
        KEY_DELETE => 0x2E,
        2..=10 => 0x31 + (code - 2), // 1..9
        11 => 0x30,                  // 0
        59..=68 => 0x70 + (code - 59), // F1..F10
        87 => 0x7A,                  // F11
        88 => 0x7B,                  // F12
        _ => LETTERS.iter().find(|(c, _)| *c == code).map(|(_, v)| *v as u16).unwrap_or(0),
    }
}

pub fn is_modifier(code: u16) -> bool {
    matches!(
        code,
        KEY_LEFTCTRL | KEY_RIGHTCTRL | KEY_LEFTSHIFT | KEY_RIGHTSHIFT | KEY_LEFTALT | KEY_RIGHTALT | KEY_LEFTMETA | KEY_RIGHTMETA
    )
}

/// Physical modifier state, fed from key events (and the device's initial state at grab time).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModTracker {
    pub lctrl: bool,
    pub rctrl: bool,
    pub lshift: bool,
    pub rshift: bool,
    pub lalt: bool,
    pub ralt: bool,
    pub lmeta: bool,
    pub rmeta: bool,
}

impl ModTracker {
    /// value: 1 press, 0 release, 2 repeat. Returns true if `code` is a modifier.
    pub fn update(&mut self, code: u16, value: i32) -> bool {
        let down = value != 0;
        match code {
            KEY_LEFTCTRL => self.lctrl = down,
            KEY_RIGHTCTRL => self.rctrl = down,
            KEY_LEFTSHIFT => self.lshift = down,
            KEY_RIGHTSHIFT => self.rshift = down,
            KEY_LEFTALT => self.lalt = down,
            KEY_RIGHTALT => self.ralt = down,
            KEY_LEFTMETA => self.lmeta = down,
            KEY_RIGHTMETA => self.rmeta = down,
            _ => return false,
        }
        true
    }

    pub fn from_pressed(codes: &[u16]) -> Self {
        let mut m = Self::default();
        for c in codes {
            m.update(*c, 1);
        }
        m
    }

    pub fn mods(&self) -> Mods {
        Mods {
            ctrl: self.lctrl || self.rctrl,
            alt: self.lalt || self.ralt,
            shift: self.lshift || self.rshift,
            win: self.lmeta || self.rmeta,
            altgr: self.ralt,
        }
    }

    /// Modifier key codes currently held (to release/re-press around injected keys).
    pub fn held(&self) -> Vec<u16> {
        [
            (self.lctrl, KEY_LEFTCTRL),
            (self.rctrl, KEY_RIGHTCTRL),
            (self.lshift, KEY_LEFTSHIFT),
            (self.rshift, KEY_RIGHTSHIFT),
            (self.lalt, KEY_LEFTALT),
            (self.ralt, KEY_RIGHTALT),
            (self.lmeta, KEY_LEFTMETA),
            (self.rmeta, KEY_RIGHTMETA),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, c)| c)
        .collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyText {
    Text(String),
    /// A dead key (French ^ or ¨): wait for the next key.
    Dead,
    Nothing,
}

/// xkbcommon keymap + state for one layout, plus compose (dead keys).
pub struct XkbText {
    state: xkb::State,
    compose: Option<xkb::compose::State>,
}

// xkbcommon objects are used from one thread at a time behind a Mutex.
unsafe impl Send for XkbText {}

impl XkbText {
    pub fn new(layout: &str, variant: &str) -> Option<Self> {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(&ctx, "evdev", "pc105", layout, variant, None, xkb::KEYMAP_COMPILE_NO_FLAGS)?;
        let state = xkb::State::new(&keymap);
        let locale = std::env::var_os("LC_ALL")
            .or_else(|| std::env::var_os("LC_CTYPE"))
            .or_else(|| std::env::var_os("LANG"))
            .filter(|l| !l.is_empty() && l != "C" && l != "POSIX")
            .unwrap_or_else(|| "en_US.UTF-8".into());
        let compose = xkb::compose::Table::new_from_locale(&ctx, &locale, xkb::compose::COMPILE_NO_FLAGS)
            .or_else(|_| xkb::compose::Table::new_from_locale(&ctx, std::ffi::OsStr::new("en_US.UTF-8"), xkb::compose::COMPILE_NO_FLAGS))
            .ok()
            .map(|t| xkb::compose::State::new(&t, xkb::compose::STATE_NO_FLAGS));
        Some(Self { state, compose })
    }

    /// Keep xkb's modifier state in sync. Call for every key press/release (value 1/0).
    pub fn track(&mut self, code: u16, value: i32) {
        let kc = xkb::Keycode::new(code as u32 + 8);
        match value {
            1 => {
                self.state.update_key(kc, xkb::KeyDirection::Down);
            }
            0 => {
                self.state.update_key(kc, xkb::KeyDirection::Up);
            }
            _ => {}
        }
    }

    pub fn reset_compose(&mut self) {
        if let Some(c) = self.compose.as_mut() {
            c.reset();
        }
    }

    /// Text produced by pressing `code` now (call after `track(code, 1)`).
    pub fn text(&mut self, code: u16) -> KeyText {
        let kc = xkb::Keycode::new(code as u32 + 8);
        let sym = self.state.key_get_one_sym(kc);
        if let Some(c) = self.compose.as_mut() {
            if c.feed(sym) == xkb::compose::FeedResult::Accepted {
                match c.status() {
                    xkb::compose::Status::Composing => return KeyText::Dead,
                    xkb::compose::Status::Composed => {
                        let s = c.utf8().unwrap_or_default();
                        c.reset();
                        return if s.is_empty() { KeyText::Nothing } else { KeyText::Text(s) };
                    }
                    xkb::compose::Status::Cancelled => {
                        c.reset();
                        return KeyText::Nothing;
                    }
                    xkb::compose::Status::Nothing => {}
                }
            }
        }
        let s = self.state.key_get_utf8(kc);
        if s.is_empty() {
            KeyText::Nothing
        } else {
            KeyText::Text(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(x: &mut XkbText, code: u16) -> KeyText {
        x.track(code, 1);
        let t = x.text(code);
        x.track(code, 0);
        t
    }

    #[test]
    fn vk_maps_editing_keys() {
        assert_eq!(vk_for(KEY_BACKSPACE), 0x08);
        assert_eq!(vk_for(KEY_ENTER), 0x0D);
        assert_eq!(vk_for(KEY_LEFT), 0x25);
        assert_eq!(vk_for(KEY_DELETE), 0x2E);
        assert_eq!(vk_for(KEY_V), 0x56);
        assert_eq!(vk_for(20), 0x54); // T
        assert_eq!(vk_for(2), 0x31); // 1
        assert_eq!(vk_for(88), 0x7B); // F12
        assert_eq!(vk_for(51), 0); // comma: plain text key
    }

    #[test]
    fn modifier_tracking() {
        let mut m = ModTracker::default();
        assert!(m.update(KEY_LEFTCTRL, 1));
        assert!(m.update(KEY_RIGHTALT, 1));
        assert!(!m.update(KEY_A, 1));
        let s = m.mods();
        assert!(s.ctrl && s.alt && s.altgr && !s.shift);
        assert_eq!(m.held(), vec![KEY_LEFTCTRL, KEY_RIGHTALT]);
        m.update(KEY_RIGHTALT, 0);
        assert!(!m.mods().altgr);
    }

    #[test]
    fn initial_modifier_state_is_read_from_device() {
        let m = ModTracker::from_pressed(&[KEY_LEFTSHIFT, KEY_A]);
        assert!(m.mods().shift);
        assert!(!m.mods().ctrl);
    }

    #[test]
    fn xkb_us_letter() {
        let mut x = XkbText::new("us", "").expect("us keymap");
        assert_eq!(press(&mut x, KEY_A), KeyText::Text("a".into()));
        x.track(KEY_LEFTSHIFT, 1);
        assert_eq!(press(&mut x, KEY_A), KeyText::Text("A".into()));
    }

    #[test]
    fn xkb_arabic_letter() {
        let mut x = XkbText::new("ara", "").expect("ara keymap");
        assert_eq!(press(&mut x, KEY_H), KeyText::Text("\u{627}".into()));
    }

    #[test]
    fn xkb_french_dead_key() {
        let mut x = XkbText::new("fr", "").expect("fr keymap");
        assert_eq!(press(&mut x, KEY_LEFTBRACE), KeyText::Dead);
        assert_eq!(press(&mut x, KEY_E), KeyText::Text("\u{ea}".into()));
    }
}
