//! Real-desktop integration tests: real hook, real SendInput, real UI Automation,
//! a fake translation API. They take over the keyboard for a few seconds each.
//! Run: cargo test --features test-support --test harness -- --ignored --test-threads=1
#![cfg(feature = "test-support")]

use live_translator_lib::controller::{Controller, ControllerHandle, HotkeyId, PopupState, StripState, UiSink};
use live_translator_lib::core::hotkey::Hotkey;
use live_translator_lib::core::settings::Settings;
use live_translator_lib::core::test_support::{reverse_words_upper, FakeApi};
use live_translator_lib::core::translator::Translator;
use live_translator_lib::core::usage::UsageMeter;
use live_translator_lib::win::field::WinField;
use live_translator_lib::win::hook::start_hook;
use live_translator_lib::win::input::{ki, push_text, send, tap};
use live_translator_lib::win::uia;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_BACK, VK_ESCAPE, VK_LCONTROL, VK_LEFT, VK_LMENU,
    VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, GetMessageW, GetWindowTextW, PostMessageW,
    RegisterClassW, SetForegroundWindow, TranslateMessage, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WNDCLASSW,
    WS_BORDER, WS_CHILD, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

unsafe extern "system" fn host_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> windows::Win32::Foundation::LRESULT {
    unsafe { DefWindowProcW(h, m, w, l) }
}

struct NullUi;
impl UiSink for NullUi {
    fn strip(&self, _s: &StripState) {}
    fn popup(&self, _p: &PopupState) {}
    fn notify(&self, _t: &str, _b: &str) {}
}

struct Rig {
    _rt: tokio::runtime::Runtime,
    handle: ControllerHandle,
}

static RIG: OnceLock<Rig> = OnceLock::new();
static SERIAL: Mutex<()> = Mutex::new(());

fn rig() -> &'static Rig {
    RIG.get_or_init(|| {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        let _g = rt.enter();
        let dir = tempfile::tempdir().unwrap().keep();
        let usage = UsageMeter::load(dir.join("usage.json"), chrono::Local::now().date_naive());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let api = Arc::new(FakeApi::new(reverse_words_upper));
        let translator = Translator::new(api, Arc::new(Mutex::new(usage)), Arc::new(AtomicBool::new(true)), tx);
        let settings = Arc::new(RwLock::new(Settings::default()));
        let handle = Controller::spawn(Arc::new(WinField::new()), Arc::new(NullUi), translator, rx, settings, dir.join("settings.json"));
        let hotkeys = Arc::new(RwLock::new(vec![
            (Hotkey::parse("Ctrl+Alt+T").unwrap(), HotkeyId::Toggle),
            (Hotkey::parse("Ctrl+Alt+L").unwrap(), HotkeyId::CycleTarget),
            (Hotkey::parse("Ctrl+Alt+R").unwrap(), HotkeyId::ReadSelection),
        ]));
        start_hook(hotkeys, handle.state.clone(), handle.input.clone());
        uia::watch_focus(handle.input.clone());
        std::thread::sleep(Duration::from_millis(300));
        Rig { _rt: rt, handle }
    })
}

#[derive(Clone, Copy)]
enum Kind {
    Edit,
    RichEdit,
    Password,
}

/// Opens a top-level window with one focused text control; returns (host, control).
fn open_window(kind: Kind) -> (isize, isize) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW {
            lpfnWndProc: Some(host_proc),
            hInstance: hinst.into(),
            lpszClassName: w!("LTHarnessHost"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let host = CreateWindowExW(
            WS_EX_TOPMOST,
            w!("LTHarnessHost"),
            w!("LT Harness"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            200,
            200,
            700,
            160,
            None,
            None,
            Some(hinst.into()),
            None,
        )
        .unwrap();
        let (cls, extra): (PCWSTR, u32) = match kind {
            Kind::Edit => (w!("EDIT"), 0x80),
            Kind::Password => (w!("EDIT"), 0x80 | 0x20),
            Kind::RichEdit => {
                let _ = LoadLibraryW(w!("Msftedit.dll"));
                (w!("RICHEDIT50W"), 0x80)
            }
        };
        let ctrl = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            cls,
            w!(""),
            WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(extra),
            10,
            10,
            660,
            34,
            Some(host),
            None,
            Some(hinst.into()),
            None,
        )
        .unwrap();
        // Alt tap lifts the foreground lock so this process may take focus.
        let mut v = Vec::new();
        tap(&mut v, VK_MENU, false, 0);
        send(&v);
        let _ = SetForegroundWindow(host);
        let _ = SetFocus(Some(ctrl));
        tx.send((host.0 as isize, ctrl.0 as isize)).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    let r = rx.recv().unwrap();
    // Wait until the window is really in front: the app refuses to type into any other window.
    let start = Instant::now();
    while unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }.0 as isize != r.0
        && start.elapsed() < Duration::from_secs(3)
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    r
}

fn close(host: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(host as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

fn text_of(ctrl: isize) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(HWND(ctrl as *mut _), &mut buf) };
    String::from_utf16_lossy(&buf[..n as usize])
}

fn hotkey_toggle() {
    let v = vec![
        ki(VK_LCONTROL, 0, KEYBD_EVENT_FLAGS(0), 0),
        ki(VK_LMENU, 0, KEYBD_EVENT_FLAGS(0), 0),
        ki(VIRTUAL_KEY(0x54), 0, KEYBD_EVENT_FLAGS(0), 0),
        ki(VIRTUAL_KEY(0x54), 0, KEYEVENTF_KEYUP, 0),
        ki(VK_LMENU, 0, KEYEVENTF_KEYUP, 0),
        ki(VK_LCONTROL, 0, KEYEVENTF_KEYUP, 0),
    ];
    send(&v);
    std::thread::sleep(Duration::from_millis(250));
}

fn type_text(s: &str) {
    use unicode_segmentation::UnicodeSegmentation;
    for g in s.graphemes(true) {
        let mut v = Vec::new();
        push_text(&mut v, g, 0);
        send(&v);
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn press(vk: VIRTUAL_KEY, extended: bool, n: usize) {
    for _ in 0..n {
        let mut v = Vec::new();
        tap(&mut v, vk, extended, 0);
        send(&v);
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn end_session() {
    if rig().handle.state.active.load(Ordering::SeqCst) {
        press(VK_ESCAPE, false, 1);
    }
    std::thread::sleep(Duration::from_millis(100));
}

fn settle() {
    std::thread::sleep(Duration::from_millis(900));
}

#[test]
#[ignore = "drives the real desktop"]
fn edit_control_gets_translation_only() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    rig();
    let (host, ctrl) = open_window(Kind::Edit);
    hotkey_toggle();
    assert!(rig().handle.state.active.load(Ordering::SeqCst), "translate mode did not start");
    type_text("le chat noir");
    settle();
    let got = text_of(ctrl);
    end_session();
    close(host);
    assert_eq!(got, "NOIR CHAT LE");
}

#[test]
#[ignore = "drives the real desktop"]
fn richedit_mid_edit() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    rig();
    let (host, ctrl) = open_window(Kind::RichEdit);
    hotkey_toggle();
    type_text("le chat noir");
    settle();
    let first = text_of(ctrl);
    press(VK_LEFT, true, 5);
    press(VK_BACK, false, 5);
    settle();
    let got = text_of(ctrl);
    end_session();
    close(host);
    assert_eq!(first, "NOIR CHAT LE");
    assert_eq!(got, "NOIR LE");
}

#[test]
#[ignore = "drives the real desktop"]
fn arabic_and_emoji() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    rig();
    let (host, ctrl) = open_window(Kind::RichEdit);
    hotkey_toggle();
    let src = "مرحبا بك 👍🏽";
    type_text(src);
    settle();
    let got = text_of(ctrl);
    end_session();
    close(host);
    assert_eq!(got, reverse_words_upper(src));
}

#[test]
#[ignore = "drives the real desktop"]
fn password_field_refused() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    rig();
    let (host, ctrl) = open_window(Kind::Password);
    hotkey_toggle();
    let active = rig().handle.state.active.load(Ordering::SeqCst);
    type_text("abc");
    std::thread::sleep(Duration::from_millis(300));
    let got = text_of(ctrl);
    end_session();
    close(host);
    assert!(!active, "translate mode must not start in a password field");
    assert_eq!(got, "abc");
}

fn run_chrome(fixture_name: &str, title: PCWSTR, text: &str) -> Option<String> {
    let chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
    if !std::path::Path::new(chrome).exists() {
        eprintln!("Chrome not installed; skipping");
        return None;
    }
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture_name);
    let url = format!("file:///{}", fixture.display().to_string().replace('\\', "/"));
    let profile = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(chrome)
        .arg(format!("--app={url}"))
        .arg(format!("--user-data-dir={}", profile.path().display()))
        .args(["--no-first-run", "--no-default-browser-check", "--disable-extensions"])
        .spawn()
        .unwrap();
    let start = Instant::now();
    let hwnd = loop {
        let h = unsafe { FindWindowW(None, title) };
        if let Ok(h) = h {
            if !h.is_invalid() {
                break h;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(15), "Chrome window did not appear");
        std::thread::sleep(Duration::from_millis(200));
    };
    std::thread::sleep(Duration::from_millis(1500));
    unsafe {
        let mut v = Vec::new();
        tap(&mut v, VK_MENU, false, 0);
        send(&v);
        let _ = SetForegroundWindow(hwnd);
    }
    std::thread::sleep(Duration::from_millis(500));
    hotkey_toggle();
    type_text(text);
    settle();
    let got = uia::focused_value();
    end_session();
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    std::thread::sleep(Duration::from_millis(500));
    let _ = child.kill();
    Some(got.unwrap_or_default())
}

#[test]
#[ignore = "drives the real desktop"]
fn chromium_textarea() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    rig();
    if let Some(got) = run_chrome("textarea.html", w!("LT Harness Textarea"), "le chat noir") {
        assert_eq!(got, "NOIR CHAT LE");
    }
}

#[test]
#[ignore = "drives the real desktop"]
fn chromium_textarea_rtl() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    rig();
    let src = "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643} \u{64a}\u{627} \u{635}\u{62f}\u{64a}\u{642}\u{64a}";
    if let Some(got) = run_chrome("textarea_rtl.html", w!("LT Harness RTL"), src) {
        assert_eq!(got, reverse_words_upper(src));
    }
}
