//! Tauri wiring: app state, UI sink, tray, and non-activating overlay windows.
use crate::controller::{Controller, ControllerHandle, HotkeyId, PopupState, StripState, UiSink};
use crate::core::google::{GoogleV2, SwappableApi, TranslateApi};
use crate::core::groq::GroqClient;
use crate::core::settings::Provider;
use crate::core::translator::Pacing;
use crate::core::hotkey::Hotkey;
use crate::core::settings::Settings;
use crate::core::translator::Translator;
use crate::core::types::Point;
use crate::core::usage::UsageMeter;
use crate::win::field::WinField;
use crate::win::hook::{start_hook, HotkeyTable};
use crate::win::{keystore, uia};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, Wry};
use tauri_plugin_notification::NotificationExt;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

pub struct AppState {
    pub handle: ControllerHandle,
    pub settings: Arc<RwLock<Settings>>,
    pub settings_path: PathBuf,
    pub usage: Arc<Mutex<UsageMeter>>,
    pub cap_enabled: Arc<AtomicBool>,
    pub api: Arc<SwappableApi>,
    pub hotkeys: HotkeyTable,
    pub translator: Arc<Translator>,
}

/// The client for a service and key (Groq uses the chosen model).
pub fn client_for(provider: Provider, key: String, groq_model: &str) -> Arc<dyn TranslateApi> {
    match provider {
        Provider::Google => Arc::new(GoogleV2::new(key)),
        Provider::Groq => Arc::new(GroqClient::new(key, groq_model.to_string())),
    }
}

/// The stored-key client for the chosen service, if a key exists.
pub fn api_for(s: &Settings) -> Option<Arc<dyn TranslateApi>> {
    keystore::get_api_key(s.provider).map(|k| client_for(s.provider, k, &s.groq_model))
}

pub fn pacing_for(p: Provider) -> Pacing {
    match p {
        Provider::Google => Pacing::Google,
        Provider::Groq => Pacing::Groq,
    }
}

pub const LANG_NAMES: [(&str, &str); 3] = [("en", "English"), ("fr", "French"), ("ar", "Arabic")];

pub fn hotkey_table(s: &Settings) -> Vec<(Hotkey, HotkeyId)> {
    [
        (&s.hotkeys.toggle, HotkeyId::Toggle),
        (&s.hotkeys.cycle_target, HotkeyId::CycleTarget),
        (&s.hotkeys.read_selection, HotkeyId::ReadSelection),
    ]
    .into_iter()
    .filter_map(|(t, id)| Hotkey::parse(t).ok().map(|h| (h, id)))
    .collect()
}

// ---------- non-activating overlay windows ----------

fn hwnd_of(w: &WebviewWindow) -> Option<HWND> {
    w.hwnd().ok().map(|h| HWND(h.0))
}

pub fn make_noactivate(w: &WebviewWindow) {
    if let Some(h) = hwnd_of(w) {
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
        }
    }
}

fn show_at(w: &WebviewWindow, p: Point) {
    if let Some(h) = hwnd_of(w) {
        unsafe {
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), p.x, p.y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    }
}

fn hide(w: &WebviewWindow) {
    if let Some(h) = hwnd_of(w) {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

// ---------- UI sink ----------

pub struct TauriUiSink {
    app: AppHandle,
    mode_item: Mutex<Option<MenuItem<Wry>>>,
    lang_items: Mutex<Vec<(String, CheckMenuItem<Wry>)>>,
    last_target: Mutex<String>,
}

impl TauriUiSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app, mode_item: Mutex::new(None), lang_items: Mutex::new(Vec::new()), last_target: Mutex::new(String::new()) }
    }

    pub fn set_tray(&self, mode: MenuItem<Wry>, langs: Vec<(String, CheckMenuItem<Wry>)>) {
        *self.mode_item.lock().unwrap() = Some(mode);
        *self.lang_items.lock().unwrap() = langs;
    }

    pub fn check_target(&self, target: &str) {
        let mut last = self.last_target.lock().unwrap();
        if *last == target {
            return;
        }
        *last = target.to_string();
        for (code, item) in self.lang_items.lock().unwrap().iter() {
            let _ = item.set_checked(code == target);
        }
    }
}

impl UiSink for TauriUiSink {
    fn strip(&self, s: &StripState) {
        let _ = self.app.emit_to("strip", "strip-state", s);
        if let Some(w) = self.app.get_webview_window("strip") {
            match (s.visible, s.pos) {
                (true, Some(p)) => show_at(&w, p),
                _ => hide(&w),
            }
        }
        self.check_target(&s.target);
    }

    fn popup(&self, p: &PopupState) {
        let _ = self.app.emit_to("popup", "popup-state", p);
        if let Some(w) = self.app.get_webview_window("popup") {
            match (p.visible, p.pos) {
                (true, Some(pos)) => show_at(&w, pos),
                _ => hide(&w),
            }
        }
    }

    fn notify(&self, title: &str, body: &str) {
        let _ = self.app.notification().builder().title(title).body(body).show();
    }

    fn mode(&self, active: bool) {
        if let Some(item) = self.mode_item.lock().unwrap().as_ref() {
            let _ = item.set_text(if active { "Translate mode: on" } else { "Translate mode: off" });
        }
    }
}

// ---------- tray ----------

fn build_tray(app: &AppHandle, ui: &TauriUiSink, target: &str) -> tauri::Result<()> {
    let mode = MenuItem::with_id(app, "mode", "Translate mode: off", false, None::<&str>)?;
    let mut langs = Vec::new();
    for (code, name) in LANG_NAMES {
        langs.push((code.to_string(), CheckMenuItem::with_id(app, format!("lang:{code}"), name, true, code == target, None::<&str>)?));
    }
    let lang_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = langs.iter().map(|(_, i)| i as _).collect();
    let submenu = Submenu::with_items(app, "Target language", true, &lang_refs)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Pause", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&mode, &sep1, &submenu, &settings, &pause, &sep2, &quit])?;
    let pause_for_event = pause.clone();
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Live Translator")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, ev| {
            let id = ev.id().as_ref().to_string();
            let state = app.state::<AppState>();
            match id.as_str() {
                "settings" => open_settings(app),
                "quit" => app.exit(0),
                "pause" => {
                    let paused = pause_for_event.is_checked().unwrap_or(false);
                    state.handle.state.paused.store(paused, Ordering::SeqCst);
                }
                _ => {
                    if let Some(code) = id.strip_prefix("lang:") {
                        state.handle.set_target(code.to_string());
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                open_settings(tray.app_handle());
            }
        })
        .build(app)?;
    ui.set_tray(mode, langs);
    Ok(())
}

// ---------- startup ----------

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let dir = Settings::default_dir();
    let settings_path = dir.join("settings.json");
    let initial = Settings::load(&settings_path);
    let usage = Arc::new(Mutex::new(UsageMeter::load(dir.join("usage.json"), chrono::Local::now().date_naive())));
    let cap_enabled = Arc::new(AtomicBool::new(initial.cap_enabled));
    let first_api = api_for(&initial);
    let has_key = first_api.is_some();
    let provider = initial.provider;
    let api = Arc::new(SwappableApi::new(first_api));
    let hotkeys: HotkeyTable = Arc::new(RwLock::new(hotkey_table(&initial)));
    let target = initial.target.clone();
    let autostart = initial.autostart;
    let settings = Arc::new(RwLock::new(initial));

    let ui = Arc::new(TauriUiSink::new(app.handle().clone()));
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let translator = Translator::new(api.clone(), usage.clone(), cap_enabled.clone(), tx);
    translator.set_pacing(pacing_for(provider));
    let translator_for_state = translator.clone();
    let field = Arc::new(WinField::new());
    let ui_for_ctrl = ui.clone();
    let settings_for_ctrl = settings.clone();
    let path_for_ctrl = settings_path.clone();
    let handle = tauri::async_runtime::block_on(async move {
        Controller::spawn(field, ui_for_ctrl, translator, rx, settings_for_ctrl, path_for_ctrl)
    });
    start_hook(hotkeys.clone(), handle.state.clone(), handle.input.clone());
    uia::watch_focus(handle.input.clone());

    for label in ["strip", "popup"] {
        if let Some(w) = app.get_webview_window(label) {
            make_noactivate(&w);
        }
    }
    build_tray(app.handle(), &ui, &target)?;
    sync_autostart(app.handle(), autostart);

    app.manage(AppState { handle, settings, settings_path, usage, cap_enabled, api, hotkeys, translator: translator_for_state });
    if !has_key || !std::env::args().any(|a| a == "--minimized") {
        open_settings(app.handle());
    }
    Ok(())
}

pub fn sync_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    let on = al.is_enabled().unwrap_or(false);
    if enabled && !on {
        let _ = al.enable();
    } else if !enabled && on {
        let _ = al.disable();
    }
}
