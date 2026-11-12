//! Text clipboard helpers (fallback paths only).
pub fn get() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

pub fn set(text: &str) {
    if let Ok(mut c) = arboard::Clipboard::new() {
        let _ = c.set_text(text.to_string());
    }
}

/// Runs `f` and then puts the previous clipboard text back.
pub fn with_restored<T>(f: impl FnOnce() -> T) -> T {
    let saved = get();
    let out = f();
    if let Some(s) = saved {
        set(&s);
    }
    out
}
