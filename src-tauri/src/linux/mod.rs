//! Linux (Wayland and X11) operating-system layer: evdev/uinput keyboard, AT-SPI focus,
//! clipboard paste, Secret Service keys. Exposed through `crate::platform`.
pub mod clipboard;
pub mod field;
pub mod focus;
pub mod hook;
pub mod input;
pub mod keys;
pub mod keystore;
pub mod layout;
pub mod permissions;
pub mod window;

use std::sync::atomic::AtomicBool;

/// True while the mouse is over our strip or popup (reported by the webviews), so a click
/// there doesn't count as "clicked somewhere else". Linux can't read global pointer coordinates.
pub static POINTER_IN_OVERLAY: AtomicBool = AtomicBool::new(false);
