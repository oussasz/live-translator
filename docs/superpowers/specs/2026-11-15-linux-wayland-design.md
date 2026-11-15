# Live Translator for Linux (Kubuntu, Wayland) — Design Spec

- **Date:** 2026-11-15
- **Status:** Approved in conversation; implementation follows `docs/superpowers/plans/2026-11-15-linux-wayland.md`
- **Target:** Kubuntu with KDE Plasma 6 on **Wayland** (X11 must also work). x86_64.

## 1. Goal

The same app as on Windows — live in-field translation with Groq/Google, the draggable floating strip, read popup, settings, tray — running on the owner's Kubuntu PC. The core (`core/*`, `controller.rs`, React UI, Groq/Google clients, pacing, Enter/Shift+Enter logic) is shared unchanged. Only the operating-system layer is new.

## 2. Constraints that shape the design

Wayland forbids an app from reading global keystrokes or injecting text into other apps through the display server. Therefore:

| Need | Linux mechanism |
|---|---|
| Capture keystrokes system-wide, swallow them in translate mode | Read keyboards from `/dev/input/event*` with **evdev**, **grab** them exclusively (`EVIOCGRAB`) and re-emit every key we don't swallow through a **uinput** virtual keyboard (the keyd approach). Works on Wayland and X11. |
| Turn key codes into characters (Arabic, French dead keys) | **xkbcommon** state built from the active Plasma layout (`org.kde.keyboard` D-Bus `/Layouts`), falling back to `XKB_DEFAULT_LAYOUT` then `us`. Re-read on layout change. |
| Remove the previous translation | Backspace × N through uinput (logical order, works in RTL). |
| Insert the new translation (Unicode) | Put it on the clipboard, send **Ctrl+V** (Ctrl+Shift+V when the focused object is a terminal), then restore the previous clipboard text. uinput cannot type arbitrary Unicode. |
| Focused field, caret position, password fields, focus changes | **AT-SPI2** over D-Bus (`atspi` crate): focus events, `Role::PasswordText`, caret character extents. |
| "Never type into another window" guard | The focused AT-SPI object at session start must still be the focused one. If AT-SPI is unavailable, the guard falls back to "no focus-change event received". |
| Overlay strip/popup at a screen position without stealing focus | Run the GTK webviews under **XWayland** (`GDK_BACKEND=x11` set at startup) so absolute positioning works; windows are non-focusable (`accept-focus=false`), keep-above, skip-taskbar. Drag handle and pinned position work as on Windows. |
| API keys | Secret Service (KWallet on Plasma 6) via `keyring` `sync-secret-service`. |
| Tray, autostart, notifications | Tauri plugins (StatusNotifierItem tray, XDG autostart, libnotify). |

## 3. Architecture

```
core/* + controller.rs + React UI          (shared, unchanged)
            │ Field / UiSink traits, HookState, InputEvent
platform    ── cfg(windows) → win/*   (existing)
            └─ cfg(linux)   → linux/* (new)
```

Both OS layers expose the same surface, re-exported as `crate::platform`:
`start_hook(hotkeys, state, tx)`, `NativeField` (impl `Field`), `watch_focus(tx)`, `keystore::{get,set,delete}_api_key(provider)`, `clipboard::{get,set,with_restored}`, `window::{make_noactivate, show_at, hide}`.
The pure key logic (`classify`, `decide`) moves from `win/keymap.rs` to `core/keys.rs` so both layers share it.

### Linux units (`src-tauri/src/linux/`)

- `keys.rs` — pure: evdev key code → Windows-style virtual key for `classify` (Backspace, Enter, arrows, V, letters…), modifier tracking, xkbcommon text lookup with dead-key composition.
- `input.rs` — device discovery (keyboards = devices reporting `KEY_A..KEY_Z`), exclusive grab, uinput virtual keyboard; `forward(event)`, `backspaces(n)`, `paste(terminal)`, `enter(mods)`, `ctrl_c()`.
- `hook.rs` — one read thread per keyboard: for each key decide swallow vs forward (shared `decide`), send `InputEvent`s; pointer devices are read (not grabbed) for `MouseDown`.
- `atspi.rs` — focus tracking (`FocusChanged`), `focused() -> FocusInfo` (password role, caret rect, work area), `selection()`.
- `field.rs` — `NativeField`: `write` = backspaces + clipboard paste + restore; `send_enter`, `read_selection` (AT-SPI selection, else Ctrl+C fallback), `is_focused`.
- `keystore.rs`, `clipboard.rs`, `window.rs` (GTK hints), `layout.rs` (Plasma layout via D-Bus).

## 4. Permissions and install

- The `.deb` ships a udev rule giving the `input` group access to `/dev/uinput`.
- On Linux the first-run screen checks read access to `/dev/input/event*` and write access to `/dev/uinput`; if missing it shows: `sudo usermod -aG input $USER`, then log out and back in.
- If the keyboard can't be grabbed, translate mode is unavailable and the strip says so; the read popup still works.

## 5. Safety

- The kernel releases the grab when the process exits or crashes, so the keyboard can never stay captured.
- Keystrokes are never logged or stored (same rule as Windows).

## 6. Build and delivery

- GitHub Actions `ubuntu-24.04`: install system deps, run `cargo test` and `pnpm test`, then `pnpm tauri build --bundles deb,appimage`; upload both as artifacts; on a `v*` tag attach them to a GitHub Release.
- A `windows-latest` job runs the Windows unit tests so both platforms stay green.

## 7. Limitations (accepted)

- Inserting uses the clipboard briefly (text restored afterwards; images/files aren't preserved).
- Chromium/Electron apps expose caret and password fields only with accessibility enabled. Without it the strip uses its pinned/last position, and password fields in those apps can't be detected.
- Real-hardware validation is done by the owner on Kubuntu; CI covers compilation, unit tests and packaging.

## 8. Out of scope

GNOME-specific tweaks, ARM builds, Flatpak/Snap, an input-method (Fcitx/IBus) engine.
