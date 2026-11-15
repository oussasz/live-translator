//! Clipboard. On Linux the owning `Clipboard` must stay alive to keep serving its content,
//! so one instance is kept for the app's lifetime.
use std::sync::Mutex;

static CLIP: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut arboard::Clipboard) -> T) -> Option<T> {
    let mut g = CLIP.lock().ok()?;
    if g.is_none() {
        *g = arboard::Clipboard::new().ok();
    }
    g.as_mut().map(f)
}

pub fn get() -> Option<String> {
    with(|c| c.get_text().ok()).flatten()
}

pub fn set(text: &str) {
    with(|c| {
        let _ = c.set_text(text.to_string());
    });
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
