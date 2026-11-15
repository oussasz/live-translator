# Linux (Wayland) Port Implementation Plan

> Execution: native (inline), same as the Windows plan. Spec: `docs/superpowers/specs/2026-11-15-linux-wayland-design.md`.

**Goal:** Build and package Live Translator for Kubuntu (Plasma 6, Wayland) with the shared core unchanged.

**Compile/test loop:** No Linux toolchain on the dev PC, so Linux code is compiled and tested by GitHub Actions on every push to `feat/linux`. Windows stays verified locally after every task.

## Global Constraints

- Shared code (`core/*`, `controller.rs`, React) must not gain OS-specific branches; OS code lives in `win/` (cfg windows) or `linux/` (cfg linux) behind `crate::platform`.
- Windows behaviour and all existing tests unchanged: `cargo test --lib` and `pnpm test` stay green on Windows after every task.
- Linux deps only under `[target.'cfg(target_os = "linux")'.dependencies]`; `windows` crate only under `cfg(windows)`.
- Keystrokes never logged or stored. Grab released on exit.
- Inserting on Linux = Backspace×N + clipboard paste + restore previous clipboard text.

## Review Focus

1. Keyboard left grabbed or keys dropped: every non-swallowed key must be forwarded exactly once, including key-repeat (value 2) and releases of keys pressed before translate mode. → L3 test `forward_decision_passes_repeats_and_releases`.
2. Modifier stuck after grab starts while a modifier is held. → L3 test `initial_modifier_state_is_read_from_device`.
3. Dead keys on French layout (^ + e → ê) and Arabic layout letters. → L2 tests `xkb_french_dead_key`, `xkb_arabic_letter`.
4. Paste into terminals needs Ctrl+Shift+V. → L4 test `paste_combo_for_terminal_role`.
5. No permission for /dev/input or /dev/uinput: clear message, no crash. → L5 test `permission_report_lists_fixes`.

## Tasks

### L1: Platform layer (Windows stays green)
- Move `classify`, `decide`, `VK_*` constants from `win/keymap.rs` into `core/keys.rs` (re-exported from `win::keymap`).
- `src/platform.rs`: cfg re-exports of the surface listed in the spec (`NativeField` = `WinField` on Windows).
- `app.rs`, `commands.rs` use only `crate::platform`.
- `windows` dep → `[target.'cfg(windows)'.dependencies]`; `mod win` gated `#[cfg(windows)]`; harness `#![cfg(all(windows, feature = "test-support"))]`.
- Verify on Windows: `cargo test --lib` 140 pass, `pnpm test` 28 pass, `cargo build` clean.

### L2: Linux keys (pure)
- `linux/keys.rs`: `vk_for(code) -> Option<u16>` (evdev KEY_* → VK), `Modifiers` tracker, `XkbText` (layout, variant) with `feed(code, pressed) -> Option<String>` handling dead keys via xkb compose.
- Tests: `vk_maps_editing_keys`, `modifier_tracking`, `xkb_us_letter`, `xkb_arabic_letter`, `xkb_french_dead_key`.

### L3: Linux input + hook
- `linux/input.rs`: keyboard discovery, grab, uinput `VirtualKeyboard` (`forward`, `backspaces`, `paste`, `enter`, `ctrl_c`).
- `linux/hook.rs`: read threads; pure `route(...)` reusing `core::keys::decide`; pointer button → `MouseDown`.
- Tests: `forward_decision_passes_repeats_and_releases`, `swallowed_press_also_swallows_release`, `initial_modifier_state_is_read_from_device`.

### L4: AT-SPI focus + field
- `linux/atspi.rs`: focus listener → `FocusChanged`, `focused() -> FocusInfo`, `selection()`.
- `linux/field.rs`: `NativeField` impl `Field`; pure `paste_combo(is_terminal)`.
- Tests: `paste_combo_for_terminal_role`.

### L5: Linux glue
- `linux/keystore.rs`, `linux/clipboard.rs`, `linux/window.rs`, `linux/layout.rs`, `linux/permissions.rs` (`check() -> PermissionReport`) + `permissions` command + Settings banner on Linux; `main.rs` sets `GDK_BACKEND=x11` on Linux.
- Test: `permission_report_lists_fixes`.

### L6: Packaging + CI
- udev rule bundled in the .deb; `.github/workflows/build.yml` with `linux` (deps, tests, deb + AppImage, artifacts, release on tag) and `windows` (unit tests) jobs.

### L7: Ship
- Push `feat/linux`, fix CI until green, merge to `main`, tag `v0.2.0`, give the owner the download link and install steps.
