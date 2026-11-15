// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("GDK_BACKEND").is_none() {
        // Overlay windows need absolute positions, which Wayland doesn't give apps; XWayland does.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }
    live_translator_lib::run()
}
