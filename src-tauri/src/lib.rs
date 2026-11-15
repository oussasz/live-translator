pub mod app;
pub mod commands;
pub mod controller;
pub mod core;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod platform;
#[cfg(windows)]
pub mod win;

use tauri::WindowEvent;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| app::open_settings(app)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::has_api_key,
            commands::set_api_key,
            commands::get_usage,
            commands::set_target,
            commands::cycle_target,
            commands::popup_reply,
            commands::popup_close,
            commands::copy_text,
            commands::offline_choice,
            commands::open_settings,
            commands::strip_drag_by,
            commands::strip_drag_end,
            commands::strip_unpin,
        ])
        .setup(|app| app::setup(app))
        .on_window_event(|window, event| {
            if window.label() == "settings" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // Keep running in the tray when all windows are hidden.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
