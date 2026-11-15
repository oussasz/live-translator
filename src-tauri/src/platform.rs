//! The operating-system layer behind one name. Shared code uses only `crate::platform`.
pub use crate::core::keys::HotkeyTable;

#[cfg(windows)]
pub use crate::win::{
    clipboard, field::WinField as NativeField, hook::start_hook, keystore, uia::watch_focus, window,
};

#[cfg(target_os = "linux")]
pub use crate::linux::{clipboard, field::NativeField, hook::start_hook, keystore, focus::watch_focus, window};
