//! Global hotkey parsing ("Ctrl+Alt+T") and exact matching against hook state.
use std::fmt;

/// Modifier state as seen by the keyboard hook. `altgr` means Right-Alt is held;
/// Windows reports AltGr as Ctrl+Alt, so it must never satisfy a Ctrl+Alt hotkey.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub altgr: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub mods: Mods,
    pub vk: u16,
}

fn key_vk(name: &str) -> Option<u16> {
    let up = name.to_ascii_uppercase();
    match up.as_str() {
        "SPACE" => return Some(0x20),
        "ESC" | "ESCAPE" => return Some(0x1B),
        _ => {}
    }
    if up.len() == 1 {
        let c = up.as_bytes()[0];
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u16);
        }
    }
    if let Some(n) = up.strip_prefix('F').and_then(|n| n.parse::<u16>().ok()) {
        if (1..=12).contains(&n) {
            return Some(0x70 + n - 1);
        }
    }
    None
}

fn vk_name(vk: u16) -> String {
    match vk {
        0x20 => "Space".into(),
        0x1B => "Esc".into(),
        0x70..=0x7B => format!("F{}", vk - 0x70 + 1),
        _ => (vk as u8 as char).to_string(),
    }
}

impl Hotkey {
    pub fn parse(s: &str) -> Result<Hotkey, String> {
        let mut mods = Mods::default();
        let mut vk = None;
        for part in s.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "alt" => mods.alt = true,
                "shift" => mods.shift = true,
                "win" => mods.win = true,
                "" => return Err(format!("Incomplete shortcut: {s}")),
                _ => {
                    if vk.is_some() {
                        return Err(format!("Only one key allowed: {s}"));
                    }
                    vk = Some(key_vk(part).ok_or_else(|| format!("Unknown key: {part}"))?);
                }
            }
        }
        let vk = vk.ok_or_else(|| format!("Shortcut needs a key: {s}"))?;
        if !(mods.ctrl || mods.alt || mods.win) {
            return Err("Add Ctrl, Alt or Win".into());
        }
        Ok(Hotkey { mods, vk })
    }

    pub fn matches(&self, vk: u16, mods: Mods) -> bool {
        !mods.altgr
            && vk == self.vk
            && mods.ctrl == self.mods.ctrl
            && mods.alt == self.mods.alt
            && mods.shift == self.mods.shift
            && mods.win == self.mods.win
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.mods.ctrl { parts.push("Ctrl".to_string()); }
        if self.mods.alt { parts.push("Alt".to_string()); }
        if self.mods.shift { parts.push("Shift".to_string()); }
        if self.mods.win { parts.push("Win".to_string()); }
        parts.push(vk_name(self.vk));
        write!(f, "{}", parts.join("+"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display_roundtrip() {
        let h = Hotkey::parse("ctrl+alt+t").unwrap();
        assert_eq!(h.to_string(), "Ctrl+Alt+T");
        assert_eq!(h.vk, 0x54);
        assert!(h.mods.ctrl && h.mods.alt && !h.mods.shift && !h.mods.win);
    }

    #[test]
    fn parse_special_keys() {
        assert_eq!(Hotkey::parse("Win+Shift+F12").unwrap().to_string(), "Shift+Win+F12");
        assert_eq!(Hotkey::parse("Ctrl+Space").unwrap().vk, 0x20);
        assert_eq!(Hotkey::parse("Alt+5").unwrap().vk, 0x35);
    }

    #[test]
    fn rejects_modifier_only_and_unknown() {
        assert!(Hotkey::parse("Ctrl+Alt").is_err());
        assert!(Hotkey::parse("Ctrl+Foo").is_err());
        assert!(Hotkey::parse("T").is_err());
        assert!(Hotkey::parse("Ctrl+").is_err());
        assert!(Hotkey::parse("Ctrl+A+B").is_err());
    }

    #[test]
    fn matches_exact_mods_only() {
        let h = Hotkey::parse("Ctrl+Alt+T").unwrap();
        assert!(h.matches(0x54, Mods { ctrl: true, alt: true, ..Mods::default() }));
        assert!(!h.matches(0x54, Mods { ctrl: true, alt: true, shift: true, ..Mods::default() }));
        assert!(!h.matches(0x55, Mods { ctrl: true, alt: true, ..Mods::default() }));
    }

    #[test]
    fn altgr_is_not_ctrl_alt_hotkey() {
        let h = Hotkey::parse("Ctrl+Alt+E").unwrap();
        assert!(!h.matches(0x45, Mods { ctrl: true, alt: true, altgr: true, ..Mods::default() }));
    }
}
