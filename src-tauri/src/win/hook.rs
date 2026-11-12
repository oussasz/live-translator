//! System-wide low-level keyboard + mouse hook on its own thread.
//! The callback never blocks: it classifies, sends an event, and returns.
use crate::controller::{HookState, HotkeyId, InputEvent, KeyInput};
use crate::core::hotkey::{Hotkey, Mods};
use crate::win::keymap::{classify, decide, VK_PACKET};
use std::cell::{Cell, RefCell};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::mpsc::UnboundedSender;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, ToUnicodeEx};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW,
    GetWindowThreadProcessId, RegisterClassW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK,
    KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOW_EX_STYLE, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WNDCLASSW, WS_OVERLAPPED,
};

/// Every input this app injects carries this tag so the hook can ignore it.
pub const INJECT_TAG: usize = 0x4C54_5452;

const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;

pub type HotkeyTable = Arc<RwLock<Vec<(Hotkey, HotkeyId)>>>;

struct Ctx {
    hotkeys: HotkeyTable,
    state: Arc<HookState>,
    tx: UnboundedSender<InputEvent>,
}

static CTX: OnceLock<Ctx> = OnceLock::new();

thread_local! {
    static HOOKS: Cell<(Option<HHOOK>, Option<HHOOK>)> = const { Cell::new((None, None)) };
    static MODS: Cell<ModState> = const { Cell::new(ModState::new()) };
    static SWALLOWED: RefCell<[bool; 256]> = const { RefCell::new([false; 256]) };
    static HIGH_SURROGATE: Cell<Option<u16>> = const { Cell::new(None) };
}

#[derive(Clone, Copy)]
struct ModState {
    lctrl: bool,
    rctrl: bool,
    lalt: bool,
    ralt: bool,
    shift_l: bool,
    shift_r: bool,
    lwin: bool,
    rwin: bool,
}

impl ModState {
    const fn new() -> Self {
        Self { lctrl: false, rctrl: false, lalt: false, ralt: false, shift_l: false, shift_r: false, lwin: false, rwin: false }
    }
    /// Returns true if `vk` is a modifier (state updated).
    fn update(&mut self, vk: u16, down: bool) -> bool {
        match vk {
            0xA2 => self.lctrl = down,
            0xA3 => self.rctrl = down,
            0xA4 => self.lalt = down,
            0xA5 => self.ralt = down,
            0xA0 => self.shift_l = down,
            0xA1 => self.shift_r = down,
            0x5B => self.lwin = down,
            0x5C => self.rwin = down,
            0x10 | 0x11 | 0x12 => {}
            _ => return false,
        }
        true
    }
    fn mods(&self) -> Mods {
        Mods {
            ctrl: self.lctrl || self.rctrl,
            alt: self.lalt || self.ralt,
            shift: self.shift_l || self.shift_r,
            win: self.lwin || self.rwin,
            altgr: self.ralt,
        }
    }
}

/// Characters the active keyboard layout produces for this key (handles dead keys).
fn layout_text(vk: u16, scan: u32, mods: Mods) -> Option<String> {
    let mut ks = [0u8; 256];
    if mods.shift {
        ks[0x10] = 0x80;
    }
    if mods.ctrl {
        ks[0x11] = 0x80;
    }
    if mods.alt {
        ks[0x12] = 0x80;
    }
    if mods.altgr {
        ks[0xA5] = 0x80;
        ks[0x11] = 0x80;
        ks[0x12] = 0x80;
    }
    unsafe {
        if GetKeyState(0x14) & 1 != 0 {
            ks[0x14] = 0x01;
        }
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let layout = GetKeyboardLayout(thread);
        let mut buf = [0u16; 8];
        let n = ToUnicodeEx(vk as u32, scan, &ks, &mut buf, 0, Some(layout));
        if n > 0 {
            Some(String::from_utf16_lossy(&buf[..n as usize]))
        } else {
            None
        }
    }
}

fn packet_text(unit: u16) -> Option<String> {
    if (0xD800..0xDC00).contains(&unit) {
        HIGH_SURROGATE.with(|h| h.set(Some(unit)));
        return None;
    }
    let high = HIGH_SURROGATE.with(|h| h.take());
    let units: Vec<u16> = match high {
        Some(h) if (0xDC00..0xE000).contains(&unit) => vec![h, unit],
        _ => vec![unit],
    };
    Some(String::from_utf16_lossy(&units))
}

/// Core of the keyboard callback: returns whether to swallow.
fn on_key(ctx: &Ctx, vk: u16, scan: u32, down: bool) -> bool {
    let is_mod = MODS.with(|m| {
        let mut s = m.get();
        let r = s.update(vk, down);
        m.set(s);
        r
    });
    if is_mod {
        return false;
    }
    let idx = (vk & 0xFF) as usize;
    if !down {
        return SWALLOWED.with(|s| std::mem::replace(&mut s.borrow_mut()[idx], false));
    }
    if ctx.state.paused.load(Ordering::SeqCst) {
        return false;
    }
    let mods = MODS.with(|m| m.get().mods());
    let active = ctx.state.active.load(Ordering::SeqCst);
    let tail_empty = ctx.state.tail_empty.load(Ordering::SeqCst);

    let hotkey = ctx.hotkeys.read().ok().and_then(|t| t.iter().find(|(h, _)| h.matches(vk, mods)).map(|(_, id)| *id));
    let input = match hotkey {
        Some(id) => KeyInput::Hotkey(id),
        None if !active => return false,
        None => {
            let text = if vk == VK_PACKET { packet_text(scan as u16) } else { layout_text(vk, scan, mods) };
            if vk == VK_PACKET && text.is_none() {
                // First half of a surrogate pair: swallow and wait for the second.
                SWALLOWED.with(|s| s.borrow_mut()[idx] = true);
                return true;
            }
            classify(vk, mods, text)
        }
    };
    let swallow = decide(&input, active, tail_empty);
    let event = if swallow { input } else { KeyInput::Other };
    if swallow || active {
        let _ = ctx.tx.send(InputEvent::Key(event));
    }
    SWALLOWED.with(|s| s.borrow_mut()[idx] = swallow);
    swallow
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        if let Some(ctx) = CTX.get() {
            let k = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            if k.dwExtraInfo != INJECT_TAG {
                let msg = wparam.0 as u32;
                let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
                if on_key(ctx, k.vkCode as u16, k.scanCode, down) {
                    return LRESULT(1);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        if msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN {
            if let Some(ctx) = CTX.get() {
                let m = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
                if m.dwExtraInfo != INJECT_TAG {
                    let _ = ctx.tx.send(InputEvent::MouseDown { x: m.pt.x, y: m.pt.y });
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn install() {
    unsafe {
        let (k, m) = HOOKS.with(|h| h.get());
        if let Some(k) = k {
            let _ = UnhookWindowsHookEx(k);
        }
        if let Some(m) = m {
            let _ = UnhookWindowsHookEx(m);
        }
        let hmod = GetModuleHandleW(None).ok().map(|h| h.into());
        let k = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0).ok();
        let m = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0).ok();
        HOOKS.with(|h| h.set((k, m)));
    }
}

unsafe extern "system" fn power_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_POWERBROADCAST && wparam.0 == PBT_APMRESUMEAUTOMATIC {
        install();
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Starts the hook thread. Call once per process.
pub fn start_hook(hotkeys: HotkeyTable, state: Arc<HookState>, tx: UnboundedSender<InputEvent>) {
    if CTX.set(Ctx { hotkeys, state, tx }).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("lt-hook".into())
        .spawn(|| unsafe {
            install();
            let hinst = GetModuleHandleW(None).ok().map(|h| h.into());
            let class = WNDCLASSW {
                lpfnWndProc: Some(power_wndproc),
                hInstance: hinst.unwrap_or_default(),
                lpszClassName: w!("LiveTranslatorPower"),
                ..Default::default()
            };
            RegisterClassW(&class);
            // Hidden top-level window (never shown) so WM_POWERBROADCAST reaches us.
            let _ = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("LiveTranslatorPower"),
                w!(""),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                hinst,
                None,
            );
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("spawn hook thread");
}
