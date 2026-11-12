//! User settings persisted as JSON in %APPDATA%\LiveTranslator.
use crate::core::types::Lang;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub toggle: String,
    pub cycle_target: String,
    pub read_selection: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            toggle: "Ctrl+Alt+T".into(),
            cycle_target: "Ctrl+Alt+L".into(),
            read_selection: "Ctrl+Alt+R".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub favorites: Vec<Lang>,
    pub target: Lang,
    pub hotkeys: Hotkeys,
    pub theme: Theme,
    pub autostart: bool,
    pub cap_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            favorites: vec!["en".into(), "fr".into(), "ar".into()],
            target: "en".into(),
            hotkeys: Hotkeys::default(),
            theme: Theme::System,
            autostart: true,
            cap_enabled: true,
        }
    }
}

impl Settings {
    /// Loads settings; a missing file gives defaults, a corrupt one is renamed
    /// to `<stem>.corrupt.json` and defaults are returned. Never fails.
    pub fn load(path: &Path) -> Settings {
        load_json_or_default(path)
    }

    /// Atomic save: write a temp file next to the target, then rename over it.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        save_json_atomic(path, self)
    }

    /// `%APPDATA%\LiveTranslator`.
    pub fn default_dir() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        base.join("LiveTranslator")
    }
}

/// Shared by settings and usage files.
pub(crate) fn load_json_or_default<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> T {
    let Ok(text) = fs::read_to_string(path) else {
        return T::default();
    };
    match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            let _ = fs::rename(path, path.with_file_name(format!("{stem}.corrupt.json")));
            T::default()
        }
    }
}

pub(crate) fn save_json_atomic<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(&dir.path().join("settings.json")), Settings::default());
    }

    #[test]
    fn defaults_match_spec() {
        let s = Settings::default();
        assert_eq!(s.favorites, vec!["en", "fr", "ar"]);
        assert_eq!(s.target, "en");
        assert_eq!(s.hotkeys.toggle, "Ctrl+Alt+T");
        assert_eq!(s.hotkeys.cycle_target, "Ctrl+Alt+L");
        assert_eq!(s.hotkeys.read_selection, "Ctrl+Alt+R");
        assert_eq!(s.theme, Theme::System);
        assert!(s.autostart);
        assert!(s.cap_enabled);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let s = Settings { target: "fr".into(), theme: Theme::Dark, ..Settings::default() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
    }

    #[test]
    fn corrupt_settings_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        assert!(dir.path().join("settings.corrupt.json").exists());
    }

    #[test]
    fn unknown_and_missing_fields_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"target":"ar","extra":1}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.target, "ar");
        assert_eq!(s.favorites, Settings::default().favorites);
    }
}
