//! Tauri commands invoked by the React windows.
use crate::core::hotkey::Hotkey;
use crate::core::settings::Settings;

use crate::app::{hotkey_table, open_settings as show_settings, sync_autostart, AppState};
use crate::controller::{HotkeyId, InputEvent, KeyInput};
use crate::core::google::{GoogleV2, TranslateApi};
use crate::core::types::TranslateError;
use crate::core::usage::UsageSnapshot;
use crate::win::{clipboard, keystore};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.read().unwrap().clone()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    validate_settings(&settings)?;
    settings.save(&state.settings_path).map_err(|e| e.to_string())?;
    *state.hotkeys.write().unwrap() = hotkey_table(&settings);
    state.cap_enabled.store(settings.cap_enabled, Ordering::SeqCst);
    sync_autostart(&app, settings.autostart);
    let target = settings.target.clone();
    *state.settings.write().unwrap() = settings.clone();
    state.handle.set_target(target);
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

#[tauri::command]
pub fn has_api_key() -> bool {
    keystore::get_api_key().is_some()
}

#[tauri::command]
pub async fn set_api_key(state: State<'_, AppState>, key: String) -> Result<(), TranslateError> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(TranslateError::InvalidKey);
    }
    let api = Arc::new(GoogleV2::new(key.clone()));
    api.translate("hello", "fr").await?;
    keystore::set_api_key(&key).map_err(TranslateError::Other)?;
    state.api.set(api);
    Ok(())
}

#[tauri::command]
pub fn get_usage(state: State<'_, AppState>) -> UsageSnapshot {
    state.usage.lock().unwrap().snapshot()
}

#[tauri::command]
pub fn set_target(state: State<'_, AppState>, lang: String) {
    state.handle.set_target(lang);
}

#[tauri::command]
pub fn cycle_target(state: State<'_, AppState>) {
    let _ = state.handle.input.send(InputEvent::Key(KeyInput::Hotkey(HotkeyId::CycleTarget)));
}

#[tauri::command]
pub fn popup_reply(state: State<'_, AppState>, lang: String) {
    state.handle.reply_in(lang);
}

#[tauri::command]
pub fn popup_close(state: State<'_, AppState>) {
    state.handle.close_popup();
}

#[tauri::command]
pub fn copy_text(text: String) {
    clipboard::set(&text);
}

#[tauri::command]
pub fn offline_choice(state: State<'_, AppState>, insert_original: bool) {
    state.handle.offline_choice(insert_original);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    show_settings(&app);
}

pub const SUPPORTED_LANGS: [&str; 3] = ["en", "fr", "ar"];

/// Checks settings before they are saved or applied.
pub fn validate_settings(s: &Settings) -> Result<(), String> {
    let keys = [
        ("Translate mode", &s.hotkeys.toggle),
        ("Cycle language", &s.hotkeys.cycle_target),
        ("Translate selection", &s.hotkeys.read_selection),
    ];
    let mut parsed: Vec<(&str, Hotkey)> = Vec::new();
    for (name, text) in keys {
        let h = Hotkey::parse(text).map_err(|e| format!("{name}: {e}"))?;
        if let Some((other, _)) = parsed.iter().find(|(_, p)| *p == h) {
            return Err(format!("{name}: Already used for {other}"));
        }
        parsed.push((name, h));
    }
    if s.favorites.is_empty() {
        return Err("Pick at least one language".into());
    }
    if let Some(bad) = s.favorites.iter().find(|f| !SUPPORTED_LANGS.contains(&f.as_str())) {
        return Err(format!("Unsupported language: {bad}"));
    }
    if !s.favorites.contains(&s.target) {
        return Err("The target language must be one of your languages".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::settings::Hotkeys;

    #[test]
    fn accepts_defaults() {
        assert!(validate_settings(&Settings::default()).is_ok());
    }

    #[test]
    fn rejects_bad_hotkey() {
        let s = Settings { hotkeys: Hotkeys { toggle: "Ctrl+".into(), ..Hotkeys::default() }, ..Settings::default() };
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn rejects_duplicate_hotkeys() {
        let s = Settings {
            hotkeys: Hotkeys { toggle: "Ctrl+Alt+L".into(), ..Hotkeys::default() },
            ..Settings::default()
        };
        assert!(validate_settings(&s).unwrap_err().contains("Already used"));
    }

    #[test]
    fn rejects_target_not_in_favorites() {
        let s = Settings { favorites: vec!["en".into()], target: "fr".into(), ..Settings::default() };
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn rejects_empty_or_unknown_favorites() {
        let empty = Settings { favorites: vec![], ..Settings::default() };
        assert!(validate_settings(&empty).is_err());
        let unknown = Settings { favorites: vec!["en".into(), "de".into()], ..Settings::default() };
        assert!(validate_settings(&unknown).is_err());
    }
}
