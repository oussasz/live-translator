# Live Translator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Windows tray app that, in translate mode, captures what the owner types and keeps only its Google translation (AR/FR/EN) in any app's focused text field, live; plus a select-and-translate popup.

**Architecture:** Tauri 2. Rust holds all logic. A pure, platform-free `core` (compose buffer, replace planning, translator scheduler, usage, settings, hotkeys, placement) is driven by a `Controller` that talks to Windows only through two traits (`Field`, `UiSink`). That lets the whole typing flow be unit-tested against a fake field. A `win` module implements those traits with a low-level keyboard/mouse hook, `SendInput`, UI Automation and Credential Manager. React windows (strip, popup, settings) only render state events and call commands.

**Tech Stack:** Rust 1.94 (MSVC), Tauri 2, tokio, reqwest (rustls), `windows` crate, keyring, arboard, unicode-segmentation, lru, chrono; React 18 + TypeScript + Vite, Vitest + Testing Library; pnpm 9.

**Spec:** `docs/superpowers/specs/2026-11-12-live-translator-design.md`

## Global Constraints

- Windows 10/11 x64 only; build target `x86_64-pc-windows-msvc`.
- Translation API: `POST https://translation.googleapis.com/language/translate/v2` with `q`, `target`, `format=text`, `key`; source auto-detected (no `source` param).
- Languages: favourites default `["en","fr","ar"]`, default target `"en"`.
- Default hotkeys: toggle `Ctrl+Alt+T`, exit `Esc` (only while a session is active), cycle target `Ctrl+Alt+L`, read selection `Ctrl+Alt+R`.
- Debounce 200 ms after last keystroke; immediate after space or punctuation. HTTP timeout 3 s.
- Sentence-ending punctuation: `.` `!` `?` `؟` `…`, locked when followed by a space, or on Enter.
- Free tier `500_000` characters per calendar month; warnings at 80% and 100%; hard cap ON by default.
- Settings file `%APPDATA%\LiveTranslator\settings.json`; usage file `%APPDATA%\LiveTranslator\usage.json`; API key only in Windows Credential Manager (service `LiveTranslator`, user `google-api-key`). Keystrokes are never written to disk or logs.
- Clipboard is never used on the normal write path; only for the paste fallback and SelectionReader fallback (which restores it).
- UI: Inter + IBM Plex Sans Arabic bundled (no network fonts); teal accent; light/dark follows system with override; sentence-case copy, never "please"/"successfully"; strip and popup never take focus.
- Targets: field updated ≤ 400 ms after pause; idle RAM < 50 MB.
- **Refinement of spec §2.1:** the hook ignores only events the app itself injected (tagged `dwExtraInfo == INJECT_TAG = 0x4C54_5452`), not all `LLKHF_INJECTED` events. This way on-screen keyboards, the emoji panel and the test harness still work.
- **Refinement of spec §3:** while a session is active, any key that passes through to the app (shortcuts like Ctrl+A/Z, Tab) first locks the current sentence, so replacements are never computed against a caret state the app changed.

## Review Focus

1. **Enter before the latest translation arrived:** the message must not be sent untranslated or stale. Enter is held, the pending translation is fetched immediately (3 s max), written, then Enter is sent. On error, Enter is still sent with the field as is and the strip shows the error. → Task 10 test `enter_flushes_pending_translation_before_sending`.
2. **Backspace/arrows/Delete when the current sentence is empty:** they must reach the app (editing already-written English normally), not be silently swallowed. → Task 7 `edit_ops_on_empty_tail_pass_through`, Task 10 `backspace_with_empty_tail_reaches_app`.
3. **AltGr characters on AZERTY (AltGr = Ctrl+RightAlt) and shortcuts during a session:** AltGr+E (`€`) is text, not a hotkey; Ctrl+A passes through and locks the tail. → Task 4 `altgr_is_not_ctrl_alt_hotkey`, Task 10 `passthrough_shortcut_locks_tail`.
4. **Corrupt or missing settings/usage JSON:** the app starts with defaults, renames the bad file to `*.corrupt.json`, and never crashes. → Task 2 `corrupt_settings_fall_back_to_defaults`, Task 3 `corrupt_usage_starts_at_zero`.
5. **Apostrophes/quotes/`<` in translations:** must arrive as literal characters, never `&#39;` or `&lt;`. → Task 5 `format_text_and_entities_decoded` (sends `format=text` and HTML-unescapes defensively).

---

## File Structure

```
package.json, vite.config.ts, vitest.config.ts, tsconfig.json, index.html
src/
  main.tsx                 window router by Tauri window label: strip | popup | settings
  lib/ipc.ts               typed invoke()/listen() wrappers (only place that imports @tauri-apps/api)
  lib/types.ts             StripState, PopupState, Settings, Usage (mirror Rust serde types)
  lib/theme.ts             useTheme(): applies data-theme from settings + system
  styles/tokens.css        CSS variables light/dark, fonts, motion
  windows/ComposeStrip.tsx
  windows/ReadPopup.tsx
  windows/settings/SettingsWindow.tsx, FirstRun.tsx, HotkeyRecorder.tsx, UsageBar.tsx
  test/setup.ts            Testing Library + mockIPC setup
src-tauri/
  Cargo.toml, tauri.conf.json, capabilities/default.json
  src/main.rs              calls live_translator_lib::run()
  src/lib.rs               Tauri builder: plugins, tray, windows, starts Controller
  src/core/mod.rs
  src/core/types.rs        Lang, Translation, TranslateError, Rect, Point, Size, LengthUnit
  src/core/settings.rs     Settings load/save
  src/core/usage.rs        UsageMeter
  src/core/hotkey.rs       Hotkey parse/match, Mods
  src/core/google.rs       GoogleV2 (TranslateApi impl), TranslateApi trait
  src/core/translator.rs   Translator: debounce, seq, cache, cap
  src/core/compose.rs      ComposeSession, EditOp
  src/core/replace.rs      plan_replace, ReplacePlan
  src/core/placement.rs    place_near
  src/controller.rs        Controller, Field + UiSink traits, StripState/PopupState, InputEvent
  src/ui_sink.rs           TauriUiSink (emits events, notifications)
  src/commands.rs          #[tauri::command]s
  src/tray.rs              tray menu
  src/win/mod.rs
  src/win/keymap.rs        vk → KeyInput (ToUnicodeEx, VK_PACKET)
  src/win/hook.rs          WH_KEYBOARD_LL + WH_MOUSE_LL thread, decide()
  src/win/input.rs         SendInput: execute ReplacePlan, send Enter, tagged
  src/win/uia.rs           focus info, password check, caret rect, value read, selection read, focus events
  src/win/clipboard.rs     get/set/restore
  src/win/keystore.rs      Credential Manager
  src/win/window_style.rs  WS_EX_NOACTIVATE + positioning helpers
  src/win/field.rs         WinField: impl Field
  examples/field_harness.rs   Win32 window: Edit, RichEdit, password Edit
  tests/harness.rs            #[ignore] real-desktop integration test
  tests/fixtures/textarea.html
docs/manual-checklist.md
```

---

### Task 1: Scaffold Tauri 2 + React app, git, test runners

**Files:** Create everything under the scaffold; `src-tauri/src/core/mod.rs`, `src-tauri/src/core/types.rs`, `src/test/setup.ts`, `vitest.config.ts`, `.gitignore`.

**Interfaces:**
- Produces (`core/types.rs`):
  - `pub type Lang = String;`
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct Translation { pub text: String, pub detected: Option<Lang> }`
  - `#[derive(Clone, Debug, PartialEq, thiserror::Error, Serialize)] pub enum TranslateError { Offline, InvalidKey, QuotaExceeded, CapReached, Other(String) }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Serialize)] pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }`, `Point { x, y }`, `Size { w, h }`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum LengthUnit { Graphemes, Utf16 }`
  - Crate lib name `live_translator_lib`.

- [ ] **Step 1: Scaffold into a temp folder and move it up** (the repo root already holds `docs/`)

```bash
cd /d/Project/translator && pnpm create tauri-app@latest _scaffold --template react-ts --manager pnpm --identifier com.livetranslator.desktop --yes
```
Move all of `_scaffold/*` (including dotfiles) into the repo root, delete `_scaffold`, set `package.json` name `live-translator`, Cargo package name `live-translator`, lib name `live_translator_lib`, product name `Live Translator`.

- [ ] **Step 2: Add dependencies**

```bash
cd src-tauri && cargo add tauri --features tray-icon && cargo add tauri-plugin-autostart tauri-plugin-single-instance tauri-plugin-notification serde --features serde/derive && cargo add serde_json async-trait thiserror unicode-segmentation lru chrono arboard keyring --features keyring/windows-native && cargo add tokio --features rt-multi-thread,macros,time,sync && cargo add reqwest --no-default-features --features json,rustls-tls,form && cargo add windows --features Win32_Foundation,Win32_UI_WindowsAndMessaging,Win32_UI_Input_KeyboardAndMouse,Win32_UI_Accessibility,Win32_System_Com,Win32_System_Variant,Win32_System_Ole,Win32_System_Threading,Win32_System_LibraryLoader,Win32_Graphics_Gdi,Win32_UI_TextServices && cargo add --dev wiremock tempfile && cargo add --dev tokio --features test-util,macros,rt-multi-thread
```
```bash
pnpm add @fontsource/inter @fontsource/ibm-plex-sans-arabic @tauri-apps/plugin-autostart @tauri-apps/plugin-notification && pnpm add -D vitest jsdom @testing-library/react @testing-library/jest-dom @testing-library/user-event
```

- [ ] **Step 3: Write `core/types.rs` with the types above and one smoke test** `translate_error_serializes_as_tag`: `serde_json::to_string(&TranslateError::InvalidKey).unwrap() == "\"InvalidKey\""`. Add `"test": "vitest run"` to package.json; `vitest.config.ts` with `environment: "jsdom"`, `setupFiles: ["src/test/setup.ts"]`; one smoke test `src/test/smoke.test.ts` asserting `1 + 1 === 2`.

- [ ] **Step 4: Verify**

Run: `cargo test --manifest-path src-tauri/Cargo.toml` → PASS (1 test). `pnpm test` → PASS. `pnpm tauri dev` → a window opens without errors (close it).

- [ ] **Step 5: git init and commit**

`.gitignore` includes `node_modules/`, `dist/`, `src-tauri/target/`, `*.corrupt.json`.
```bash
git init && git add -A && git commit -m "chore: scaffold Tauri 2 + React app"
```

---

### Task 2: Settings and API key store

**Files:** Create `src-tauri/src/core/settings.rs`, `src-tauri/src/win/keystore.rs`, `src-tauri/src/win/mod.rs`.

**Interfaces:**
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(default)] pub struct Settings { pub favorites: Vec<Lang>, pub target: Lang, pub hotkeys: Hotkeys, pub theme: Theme, pub autostart: bool, pub cap_enabled: bool }` with `Default` = `["en","fr","ar"]`, `"en"`, defaults below, `Theme::System`, `true`, `true`.
  - `pub struct Hotkeys { pub toggle: String, pub cycle_target: String, pub read_selection: String }` defaults `"Ctrl+Alt+T"`, `"Ctrl+Alt+L"`, `"Ctrl+Alt+R"`.
  - `#[serde(rename_all = "lowercase")] pub enum Theme { System, Light, Dark }`
  - `impl Settings { pub fn load(path: &Path) -> Settings; pub fn save(&self, path: &Path) -> std::io::Result<()>; pub fn default_dir() -> PathBuf }` (`%APPDATA%\LiveTranslator`). `save` writes to a `.tmp` file then renames it (atomic).
  - `keystore::{get_api_key() -> Option<String>, set_api_key(key: &str) -> Result<(), String>, delete_api_key() -> Result<(), String>}` using `keyring::Entry::new("LiveTranslator", "google-api-key")`.

- [ ] **Step 1: Write failing tests** (in `settings.rs`, using `tempfile::tempdir()`)
  - `missing_file_gives_defaults`: `Settings::load(dir/"settings.json") == Settings::default()`.
  - `save_then_load_roundtrips`: set target `"fr"`, theme `Dark`; save; load; equal.
  - `corrupt_settings_fall_back_to_defaults`: write `"{not json"`; load equals default; `dir/"settings.corrupt.json"` exists.
  - `unknown_and_missing_fields_tolerated`: file `{"target":"ar","extra":1}` loads with `target=="ar"` and other fields default.
  - In `keystore.rs`: `#[ignore] set_get_delete_roundtrip` (touches the real Credential Manager; run manually).
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml settings` → FAIL (not defined).
- [ ] **Step 3: Implement** settings and keystore as specified.
- [ ] **Step 4: Run** the same command → PASS; then `cargo test --manifest-path src-tauri/Cargo.toml keystore -- --ignored` → PASS.
- [ ] **Step 5: Commit** `feat: settings persistence and API key store`

---

### Task 3: Usage meter

**Files:** Create `src-tauri/src/core/usage.rs`.

**Interfaces:**
- Produces:
  - `pub const FREE_TIER_CHARS: u64 = 500_000;`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum Threshold { Warn80, Reached100 }`
  - `#[derive(Serialize)] pub struct UsageSnapshot { pub month: String, pub chars: u64, pub limit: u64 }`
  - `pub struct UsageMeter { … }` with `pub fn load(path: PathBuf, today: NaiveDate) -> Self`, `pub fn can_spend(&self, n: u64, cap_enabled: bool) -> bool`, `pub fn record(&mut self, n: u64, today: NaiveDate) -> Option<Threshold>` (persists on each call), `pub fn snapshot(&self) -> UsageSnapshot`.
  - File JSON: `{"month":"2026-11","chars":41200}`. Characters counted as `text.chars().count()`.

- [ ] **Step 1: Write failing tests**
  - `record_accumulates_and_persists`: record 100 then 50; reload from the same path → `snapshot().chars == 150`.
  - `month_rollover_resets`: record 100 on 2026-11-30; `record(1, 2026-12-01)` → chars `1`, month `"2026-12"`.
  - `thresholds_fire_once`: record 399_999 → `None`; record 1 → `Some(Warn80)`; record 10 → `None`; record 100_000 → `Some(Reached100)`.
  - `cap_blocks_when_enabled_only`: at 499_990, `can_spend(20, true) == false`, `can_spend(20, false) == true`, `can_spend(10, true) == true`.
  - `corrupt_usage_starts_at_zero`: garbage file → chars `0`, `usage.corrupt.json` exists.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml usage` → FAIL.
- [ ] **Step 3: Implement.** Thresholds fire when a `record` call crosses 400_000 or 500_000 (previous < bound ≤ new).
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat: monthly usage meter with free-tier cap`

---

### Task 4: Hotkey parsing and matching

**Files:** Create `src-tauri/src/core/hotkey.rs`.

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Default)] pub struct Mods { pub ctrl: bool, pub alt: bool, pub shift: bool, pub win: bool, pub altgr: bool }` (`altgr` = RightAlt held; the hook sets it).
  - `#[derive(Clone, Debug, PartialEq)] pub struct Hotkey { pub mods: Mods, pub vk: u16 }`
  - `impl Hotkey { pub fn parse(s: &str) -> Result<Hotkey, String>; pub fn matches(&self, vk: u16, mods: Mods) -> bool }` and `Display` producing the canonical form `"Ctrl+Alt+T"`.
  - Supported key names: `A`–`Z`, `0`–`9`, `F1`–`F12`, `Space`, `Esc`. Modifier order in Display: Ctrl, Alt, Shift, Win.

- [ ] **Step 1: Write failing tests**
  - `parse_and_display_roundtrip`: `"ctrl+alt+t"` parses; `to_string() == "Ctrl+Alt+T"`; vk `0x54`.
  - `rejects_modifier_only_and_unknown`: `"Ctrl+Alt"` → Err; `"Ctrl+Foo"` → Err; `"T"` (no modifier) → Err.
  - `matches_exact_mods_only`: `Ctrl+Alt+T` doesn't match `Ctrl+Alt+Shift+T`.
  - `altgr_is_not_ctrl_alt_hotkey`: `Ctrl+Alt+E` does **not** match `vk=0x45, Mods{ctrl:true, alt:true, altgr:true, ..}`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml hotkey` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat: hotkey parsing and matching`

---

### Task 5: Google Translate v2 client

**Files:** Create `src-tauri/src/core/google.rs`.

**Interfaces:**
- Produces:
  - `#[async_trait] pub trait TranslateApi: Send + Sync { async fn translate(&self, text: &str, target: &str) -> Result<Translation, TranslateError>; }`
  - `pub struct GoogleV2 { … }` with `pub fn new(key: String) -> Self` (base `https://translation.googleapis.com`) and `pub fn with_base(key: String, base: String) -> Self`. 3 s timeout on the reqwest client.
  - Request: form body `q`, `target`, `format=text`, `key` to `{base}/language/translate/v2`. Response `data.translations[0].translatedText`, `detectedSourceLanguage`.
  - Error mapping: connect/timeout errors → `Offline`; any error body containing `API_KEY_INVALID` or `API key not valid`, or 401 → `InvalidKey`; 429, or 403 with reason `dailyLimitExceeded|userRateLimitExceeded|rateLimitExceeded|quotaExceeded` → `QuotaExceeded`; anything else → `Other(message)`.

- [ ] **Step 1: Write failing tests** (wiremock `MockServer`, `#[tokio::test]`)
  - `sends_expected_form_and_parses`: the mock matches path `/language/translate/v2`, body contains `format=text`, `target=en`, `key=k`; returns `{"data":{"translations":[{"translatedText":"Hello","detectedSourceLanguage":"ar"}]}}` → `Translation{text:"Hello", detected:Some("ar")}`.
  - `format_text_and_entities_decoded`: mock returns `"l&#39;heure &lt;3"` → text `"l'heure <3"`.
  - `invalid_key_maps`: 400 with `{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}` → `InvalidKey`.
  - `quota_maps`: 429 → `QuotaExceeded`.
  - `offline_maps`: `with_base("http://127.0.0.1:1")` → `Offline`.
  - `timeout_maps_offline`: mock delays 4 s → `Offline` (test finishes in ~3 s).
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml google` → FAIL.
- [ ] **Step 3: Implement.** Unescape `&#39; &quot; &amp; &lt; &gt;` after parsing (`&amp;` last).
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat: Google Translate v2 client with error mapping`

---

### Task 6: Translator scheduler (debounce, latest-wins, cache, cap)

**Files:** Create `src-tauri/src/core/translator.rs`.

**Interfaces:**
- Consumes: `TranslateApi` (Task 5), `UsageMeter`/`Threshold` (Task 3), `Translation`/`TranslateError` (Task 1).
- Produces:
  - `pub struct TranslatorOutput { pub seq: u64, pub result: Result<Translation, TranslateError>, pub threshold: Option<Threshold> }`
  - `pub struct Translator` with
    - `pub fn new(api: Arc<dyn TranslateApi>, usage: Arc<Mutex<UsageMeter>>, cap_enabled: Arc<AtomicBool>, out: mpsc::UnboundedSender<TranslatorOutput>) -> Arc<Self>`
    - `pub fn submit(self: &Arc<Self>, text: String, target: Lang, immediate: bool) -> u64`: returns a new seq; after 200 ms (0 if `immediate`) translates, unless a newer submit happened meanwhile (then it does nothing). It also sends nothing if a newer seq was issued before the response arrived.
    - `pub async fn translate_now(&self, text: &str, target: &str) -> (Result<Translation, TranslateError>, Option<Threshold>)`: no debounce; shared cache/usage path (used by Enter flush and popup). Bumps the seq so in-flight submits are dropped.
    - `pub fn latest_seq(&self) -> u64`
  - Empty or whitespace-only text → `Ok(Translation{text: same whitespace, detected: None})` with no API call and no usage.
  - Cache: `LruCache<(String, Lang), Translation>` capacity 512; cache hits cost no usage.
  - Cap: if `!usage.can_spend(chars, cap_enabled)` → `Err(CapReached)` with no API call.

- [ ] **Step 1: Write failing tests** (`#[tokio::test(start_paused = true)]`, a `FakeApi` that records calls and returns `text.to_uppercase()`, with optional per-call delay and scripted errors; put `FakeApi` in `core/test_support.rs` behind `#[cfg(test)]` so Task 10 reuses it)
  - `debounces_rapid_submits`: submit "a","ab","abc" 50 ms apart; advance 250 ms → exactly 1 API call with `"abc"`; one output with the last seq.
  - `immediate_skips_delay`: `submit("hi", "en", true)`; advance 1 ms → output received.
  - `stale_response_dropped`: FakeApi delay 300 ms; submit "a" immediate; at 100 ms submit "ab" immediate → only one output, text `"AB"`.
  - `cache_hit_no_call_no_usage`: `translate_now("x")` twice → 1 API call; usage chars == 1.
  - `cap_reached_blocks_call`: usage preloaded 500_000, cap on → `Err(CapReached)`, 0 API calls.
  - `whitespace_only_no_call`: `translate_now("  ")` → `Ok("  ")`, 0 calls.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml translator` → FAIL.
- [ ] **Step 3: Implement** with `tokio::spawn` + `tokio::time::sleep`, and an `AtomicU64` seq checked before calling and before sending.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat: debounced latest-wins translator with cache and cap`

---

### Task 7: Compose session (original-text buffer and sentence locking)

**Files:** Create `src-tauri/src/core/compose.rs`.

**Interfaces:**
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub enum EditOp { Insert(String), Backspace, Delete, DeleteWordBack, Left, Right, Home, End }`
  - `#[derive(Debug, PartialEq)] pub enum Applied { Changed { lock_now: bool, immediate: bool }, CaretOnly, PassThrough }`
  - `pub struct ComposeSession` with
    - `pub fn new(target: Lang) -> Self`
    - `pub fn apply(&mut self, op: EditOp) -> Applied`
    - `pub fn tail(&self) -> &str` (unlocked original text), `pub fn tail_caret(&self) -> usize` (grapheme index within tail)
    - `pub fn lock_tail(&mut self)`: moves the tail into locked; clears `live_output`
    - `pub fn live_output(&self) -> &str`, `pub fn set_live_output(&mut self, s: String)`
    - `pub fn target(&self) -> &str`, `pub fn set_target(&mut self, t: Lang)`
    - `pub fn is_tail_empty(&self) -> bool`
  - Rules:
    - Caret moves only within the tail (graphemes).
    - Editing ops (`Backspace`, `Delete`, `DeleteWordBack`, `Left`, `Right`, `Home`, `End`) on an empty tail → `PassThrough`.
    - `Insert` of a string ending in a space whose preceding non-space char is in `. ! ? ؟ …` → `Changed{lock_now:true, immediate:true}`. The controller translates immediately, then locks after writing.
    - `Insert` ending in space or punctuation → `immediate:true`.
    - Multi-char `Insert` (paste) goes at the caret.

- [ ] **Step 1: Write failing tests**
  - `insert_and_backspace_with_caret`: insert "abcde"; Left×2; Backspace → tail `"abde"`, caret 2.
  - `arabic_caret_by_grapheme`: insert "مرحبا"; Left; Backspace → tail `"مرحا"`.
  - `ctrl_backspace_deletes_word`: tail `"bonjour le monde"` caret at end → DeleteWordBack → `"bonjour le "`.
  - `sentence_end_requests_lock`: insert "Hi." then insert " " → `Changed{lock_now:true, immediate:true}`.
  - `arabic_question_mark_locks`: "كيف حالك؟" + " " → `lock_now:true`.
  - `space_is_immediate_not_lock`: "hi" + " " → `Changed{lock_now:false, immediate:true}`.
  - `edit_ops_on_empty_tail_pass_through`: new session → Backspace, Left, Delete each → `PassThrough`.
  - `paste_inserts_at_caret`: tail "ac", Left, Insert("b") → "abc".
  - `lock_tail_clears_tail_and_live_output`.
  - `emoji_is_one_grapheme`: insert "a👍🏽" then Backspace → tail "a".
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml compose` → FAIL.
- [ ] **Step 3: Implement** with `unicode_segmentation::UnicodeSegmentation::graphemes`.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat: compose session buffer with sentence locking`

---

### Task 8: Replace planner

**Files:** Create `src-tauri/src/core/replace.rs`.

**Interfaces:**
- Produces:
  - `#[derive(Debug, PartialEq, Clone)] pub struct ReplacePlan { pub select_left: usize, pub type_text: String }`. Execution: press Shift+Left `select_left` times, then type `type_text`; if `type_text` is empty and `select_left > 0`, press Delete once.
  - `pub fn plan_replace(prev: &str, next: &str, unit: LengthUnit) -> ReplacePlan`: keeps the longest common **grapheme** prefix of `prev` and `next` (less flicker), selects the rest of `prev` (counted in `unit`), and types the rest of `next`.
  - `pub fn len_in(s: &str, unit: LengthUnit) -> usize` (graphemes or UTF-16 code units).

- [ ] **Step 1: Write failing tests**
  - `identical_is_noop`: `plan_replace("Hello","Hello",G) == {0, ""}`.
  - `append_only`: `("Hello","Hello there",G) == {0," there"}`.
  - `rewrite_suffix`: `("Hello world","Hello there",G) == {5,"there"}`.
  - `full_rewrite`: `("I go","We went",G) == {4,"We went"}`.
  - `shrink_to_empty`: `("abc","",G) == {3,""}`.
  - `emoji_counts_by_unit`: `("ok 👍🏽","ok",G).select_left == 2` and with `Utf16` == `5`.
  - `french_accent_combining`: prev `"cafe\u{301}"`, next `"cafe"` → G `{1,"e"}`; Utf16 `{2,"e"}` (the grapheme `e\u{301}` differs from `e`, so it's replaced whole).
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml replace` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat: minimal-diff replace planner`

---

### Task 9: Window placement

**Files:** Create `src-tauri/src/core/placement.rs`.

**Interfaces:**
- Produces: `pub fn place_near(anchor: Rect, size: Size, work_area: Rect, gap: i32) -> Point`. It puts the window above the anchor, left-aligned to it. It flips below if there's no room above, and clamps horizontally and vertically inside `work_area`. Callers use `gap = 8`.

- [ ] **Step 1: Write failing tests**
  - `above_when_room`: anchor `{x:500,y:500,w:2,h:20}`, size `{w:300,h:40}`, work `{0,0,1920,1040}` → `{x:500, y:452}`.
  - `flips_below_at_top`: anchor y=10 → y = 10+20+8 = 38.
  - `clamps_right_edge`: anchor x=1800 → x = 1920-300 = 1620.
  - `clamps_left_edge_negative_monitor`: work `{-1920,0,1920,1080}`, anchor x=-1950 → x=-1920.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml placement` → FAIL.
- [ ] **Step 3: Implement.** **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat: window placement`

---

### Task 10: Controller (the typing flow, fully testable)

**Files:** Create `src-tauri/src/controller.rs`.

**Interfaces:**
- Consumes: Tasks 2–9.
- Produces:
  - `pub enum KeyInput { Text(String), Edit(EditOp), Enter, Escape, Paste, Hotkey(HotkeyId), Other }`, `pub enum HotkeyId { Toggle, CycleTarget, ReadSelection }`
  - `pub enum InputEvent { Key(KeyInput), MouseDown { x: i32, y: i32 }, FocusChanged }`
  - `pub struct FocusInfo { pub hwnd: isize, pub is_password: bool, pub unit: LengthUnit, pub caret: Option<Rect>, pub work_area: Rect }`
  - `pub enum WriteOutcome { Ok, Failed }`
  - `pub trait Field: Send + Sync { fn focus(&self) -> Option<FocusInfo>; fn write(&self, plan: &ReplacePlan, expect_suffix: &str) -> WriteOutcome; fn send_enter(&self); fn set_clipboard(&self, text: &str); fn clipboard_text(&self) -> Option<String>; fn read_selection(&self) -> Option<(String, Option<Rect>)>; }`
  - `#[derive(Clone, Debug, PartialEq, Serialize)] #[serde(rename_all="snake_case")] pub enum StripStatus { Idle, Translating, Ok, Offline, KeyError, Cap, QuotaError, PasteFallback, Password }`
  - `#[derive(Clone, Debug, PartialEq, Serialize)] pub struct StripState { pub visible: bool, pub source: String, pub caret: usize, pub target: Lang, pub detected: Option<Lang>, pub status: StripStatus, pub pos: Option<Point>, pub offer_original: bool }`
  - `#[derive(Clone, Debug, PartialEq, Serialize)] pub struct PopupState { pub visible: bool, pub source: String, pub translation: Option<String>, pub detected: Option<Lang>, pub target: Lang, pub error: Option<TranslateError>, pub pos: Option<Point> }`
  - `pub trait UiSink: Send + Sync { fn strip(&self, s: &StripState); fn popup(&self, p: &PopupState); fn notify(&self, title: &str, body: &str); }`
  - `pub struct Controller` with `pub fn spawn(field: Arc<dyn Field>, ui: Arc<dyn UiSink>, translator: Arc<Translator>, outputs: UnboundedReceiver<TranslatorOutput>, settings: Arc<RwLock<Settings>>, settings_path: PathBuf) -> ControllerHandle`
  - `pub struct ControllerHandle { pub input: UnboundedSender<InputEvent>, pub state: Arc<HookState> }` and `pub struct HookState { pub active: AtomicBool, pub tail_empty: AtomicBool, pub paused: AtomicBool }` (read by the hook thread to decide swallowing without waiting on the controller).
  - Extra commands on the handle (sent as internal messages): `pub fn offline_choice(&self, insert_original: bool)`, `pub fn set_target(&self, t: Lang)`, `pub fn reply_in(&self, t: Lang)`, `pub fn close_popup(&self)`.
  - Constants: `STRIP_SIZE = Size{w:420,h:44}`, `POPUP_SIZE = Size{w:360,h:180}`.

Behaviour (each line is a test below):
- Toggle on a password field → strip status `Password` (visible ~1.5 s), no session.
- Toggle on a normal field → session starts with `settings.target`, strip visible at `place_near(caret)`.
- `Text`/`Edit`/`Paste` → `compose.apply` → strip updates instantly → `translator.submit(tail, target, immediate)`; strip `Translating`. `Paste` uses `field.clipboard_text()` as an `Insert`.
- `TranslatorOutput` with seq == latest → `plan_replace(live_output, text, unit)` → `field.write` → `set_live_output` → strip `Ok`/detected. If `lock_now` was pending for this seq → `lock_tail` after writing.
- `WriteOutcome::Failed` → `set_clipboard(text)`, strip `PasteFallback`.
- Errors → strip `Offline` / `KeyError` / `QuotaError` / `Cap`; a `threshold` → `ui.notify` ("You've used 80% of this month's free translations" / "Free translation limit reached").
- Enter: if the tail is non-empty and its translation isn't current → `translate_now(tail)` → write → lock → `send_enter`. Otherwise lock → `send_enter`.
- Escape or Toggle while active → session ends; if the last status was `Offline` and the tail is non-empty → strip stays with `offer_original:true` until `offline_choice` (true → write the original tail).
- `MouseDown` → `lock_tail`; if the popup is visible and the point is outside it → hide the popup. `FocusChanged` → end the session (no writes after it).
- `Key(Other)` while active (a passed-through shortcut) → `lock_tail`.
- `CycleTarget` → next favourite; persist to settings; if a session is active, re-submit the tail immediately.
- `ReadSelection` → `field.read_selection()` → `translate_now(text, target)` → `ui.popup` at `place_near(sel_rect)`. `reply_in(lang)` → set target, hide popup, start a session.

- [ ] **Step 1: Write failing tests** with `FakeField` (holds a `String` and a caret; `write` applies the ReplacePlan literally: removes `select_left` graphemes before the caret, inserts `type_text`; `send_enter` appends `"⏎"`) and `RecordingUi`. The Translator uses the Task 6 `FakeApi` (uppercase, configurable delay/error). Tokio paused time.
  - `typing_shows_translation_only`: toggle; type "bonjour" char by char; advance 300 ms → field `"BONJOUR"`.
  - `edit_mid_sentence_rewrites`: type "hello world", Left×6, Backspace×5 (removes "hello"), Insert "bye" → field `"BYE WORLD"`.
  - `preexisting_text_untouched`: field starts `"Hi: "` caret at end → after typing "ok" → `"Hi: OK"`.
  - `sentence_lock_then_new_sentence`: "one. two" → field `"ONE. TWO"`; a later Backspace×3 only affects `"TWO"` (field `"ONE. "`).
  - `enter_flushes_pending_translation_before_sending`: type "abc" and Enter within 50 ms (FakeApi delay 100 ms) → field `"ABC⏎"`, never containing `"⏎"` before `"ABC"`.
  - `backspace_with_empty_tail_reaches_app`: after Enter, `HookState.tail_empty == true` (so the hook passes Backspace through) and the field is unchanged by the controller.
  - `passthrough_shortcut_locks_tail`: type "ab", wait, `Key(Other)`, type "c" → field `"ABC"`, with the last write having `select_left == 0`.
  - `stale_output_ignored`: FakeApi delays: first 300 ms, second 10 ms → final field from the second.
  - `password_refused`: focus `is_password` → no session; strip status `Password`.
  - `focus_change_ends_session_without_writing`: FakeApi delay 200 ms; type "x"; FocusChanged; advance 300 ms → field unchanged, strip hidden.
  - `offline_offers_original_on_exit`: FakeApi returns Offline; type "salut"; Escape → strip `offer_original:true`; `offline_choice(true)` → field `"salut"`.
  - `write_failure_uses_clipboard`: FakeField write → Failed → clipboard `"HI"`, strip `PasteFallback`.
  - `cycle_target_resubmits`: active session, CycleTarget → target `"fr"`, saved settings target `"fr"`, an immediate translate is issued.
  - `threshold_notifies`: usage at 399_999, type "ab" → `RecordingUi.notify` called once.
  - `read_selection_shows_popup`: `read_selection` returns `("salut", rect)` → popup visible with translation `"SALUT"`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml controller` → FAIL.
- [ ] **Step 3: Implement** `Controller` as one tokio task `select!`-ing over `input`, internal commands, and `outputs`. It tracks `pending_lock_seq: Option<u64>` and `last_written_seq`. Update `HookState` after every state change.
- [ ] **Step 4: Run** → PASS. Also run the whole suite `cargo test --manifest-path src-tauri/Cargo.toml` → PASS.
- [ ] **Step 5: Commit** `feat: controller implementing the live typing flow`

---

### Task 11: Windows keyboard + mouse hook

**Files:** Create `src-tauri/src/win/keymap.rs`, `src-tauri/src/win/hook.rs`, `src-tauri/examples/hook_echo.rs`.

**Interfaces:**
- Consumes: `KeyInput`, `InputEvent`, `HookState`, `HotkeyId` (Task 10); `Hotkey`, `Mods` (Task 4); `EditOp` (Task 7).
- Produces:
  - `pub const INJECT_TAG: usize = 0x4C54_5452;`
  - `pub fn classify(vk: u16, mods: Mods, text: Option<String>) -> KeyInput` (pure; `text` = result of ToUnicodeEx or the VK_PACKET char). Mapping: Backspace→`Edit(Backspace)` (with Ctrl → `DeleteWordBack`), Delete, Left/Right/Home/End → Edit; Enter→Enter; Esc→Escape; Ctrl+V→Paste; printable text with no Ctrl/Win (or with AltGr) → `Text`; else `Other`. Bare modifier keys are not classified (the hook ignores them).
  - `pub fn decide(input: &KeyInput, active: bool, tail_empty: bool) -> bool` (true = swallow). Swallow: matched hotkeys always (Escape only when active); when active: `Text`, `Paste`, `Enter` only if `!tail_empty`, `Edit` only if `!tail_empty`. Everything else passes through. When `HookState.paused` is set, nothing is swallowed and no events are sent.
  - `pub fn start_hook(hotkeys: Arc<RwLock<Vec<(Hotkey, HotkeyId)>>>, state: Arc<HookState>, tx: UnboundedSender<InputEvent>)`: spawns a dedicated thread with `SetWindowsHookExW(WH_KEYBOARD_LL)` + `WH_MOUSE_LL` (left/right button down → `MouseDown{x,y}`) and a `GetMessageW` loop.
  - Hook callback rules:
    - Never blocks; only classifies, `tx.send`, and returns.
    - Skips events with `dwExtraInfo == INJECT_TAG`.
    - The key-up of a swallowed key-down is swallowed too.
    - Calls `ToUnicodeEx` only when `active` (avoids disturbing dead-key state for normal typing), with the layout of the foreground window's thread (`GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow()))`); handles dead keys (French `^` + `e` → `ê`).
    - `VK_PACKET` → char from `scanCode`.
  - Re-installs hooks on `WM_POWERBROADCAST`/`PBT_APMRESUMEAUTOMATIC` via a hidden message window on the same thread.

- [ ] **Step 1: Write failing tests** (pure functions only)
  - `classify_maps_edit_keys`: VK_BACK → `Edit(Backspace)`; VK_BACK+ctrl → `Edit(DeleteWordBack)`; VK_LEFT → `Edit(Left)`.
  - `classify_altgr_text`: vk 0x45, Mods{ctrl,alt,altgr}, text `"€"` → `Text("€")`.
  - `classify_ctrl_a_is_other`: → `Other`.
  - `decide_inactive_passes_text`: `decide(&Text("a"), false, true) == false`.
  - `decide_active_swallows_text`: `true`.
  - `decide_edit_on_empty_tail_passes`: `decide(&Edit(Backspace), true, true) == false`; with `tail_empty=false` → `true`.
  - `decide_enter_empty_tail_passes`: `decide(&Enter, true, true) == false`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml hook` → FAIL.
- [ ] **Step 3: Implement** `classify`, `decide`, then `start_hook` (callback context in `OnceLock` statics).
- [ ] **Step 4: Run** → PASS. Manual smoke: `cargo run --manifest-path src-tauri/Cargo.toml --example hook_echo` (prints `InputEvent`s with `active=true`) → typing in Notepad prints `Text("ب")` with the Arabic layout and `Text("ê")` via the French dead key; Notepad receives nothing while it runs.
- [ ] **Step 5: Commit** `feat: low-level keyboard and mouse hook`

---

### Task 12: Windows field I/O (SendInput, UIA, clipboard)

**Files:** Create `src-tauri/src/win/input.rs`, `src-tauri/src/win/uia.rs`, `src-tauri/src/win/clipboard.rs`, `src-tauri/src/win/field.rs`, `src-tauri/examples/field_harness.rs`, `src-tauri/tests/harness.rs`, `src-tauri/tests/fixtures/textarea.html`.

**Interfaces:**
- Consumes: `Field`, `FocusInfo`, `WriteOutcome` (Task 10); `ReplacePlan` (Task 8); `INJECT_TAG`, `start_hook` (Task 11).
- Produces:
  - `input::execute(plan: &ReplacePlan)`: Shift down, Left × n, Shift up, then each UTF-16 unit of `type_text` as `KEYEVENTF_UNICODE` down/up; Delete if the text is empty and n > 0. All `INPUT`s carry `dwExtraInfo = INJECT_TAG`, sent in one `SendInput` batch.
  - `input::send_enter()`, `input::send_ctrl_c()`, both tagged.
  - `uia::focused() -> Option<FocusInfo>`:
    - `IUIAutomation::GetFocusedElement`; `CurrentIsPassword`.
    - `unit = Utf16` when the class name is `Edit` or starts with `RichEdit`, else `Graphemes`.
    - Caret: `IUIAutomationTextPattern2::GetCaretRange` bounding rect, falling back to `GetGUIThreadInfo().rcCaret` mapped to the screen, then the element rect.
    - `work_area` from `MonitorFromPoint` + `GetMonitorInfoW`.
  - `uia::focused_value() -> Option<String>` via `ValuePattern`, else `TextPattern.DocumentRange.GetText(-1)`.
  - `uia::selection() -> Option<(String, Option<Rect>)>` via `TextPattern.GetSelection`.
  - `uia::watch_focus(tx: UnboundedSender<InputEvent>)`: a COM MTA thread that registers a focus-changed event handler and sends `FocusChanged` when the focused element changes to a different element (ignoring focus moving to the app's own strip/popup windows).
  - `clipboard::{get() -> Option<String>, set(&str), with_restored<T>(f: impl FnOnce() -> T) -> T}` (arboard).
  - `WinField` implements `Field`:
    - `write` = `execute`, wait 30 ms, then if `focused_value()` is `Some(v)` and `!v.contains(expect_suffix)` → `Failed`, else `Ok`.
    - `read_selection` = `uia::selection()`, else `with_restored(|| { send_ctrl_c(); sleep 80 ms; clipboard::get() })`.

- [ ] **Step 1: Write the harness and the failing ignored integration tests**
  - `examples/field_harness.rs`: a Win32 window titled `LT Harness` with a single-line `Edit`, a `RICHEDIT50W`, and an `ES_PASSWORD` Edit.
  - `tests/fixtures/textarea.html`: one autofocused `<textarea>`.
  - `tests/harness.rs` (`#[ignore]`, run on the real desktop):
    - Each test launches the harness (or Edge), starts the controller with `WinField`, a real hook, and a `FakeApi` that reverses word order and uppercases (`"le chat noir"` → `"NOIR CHAT LE"`). (Expose the FakeApi via a `test-support` cargo feature so integration tests can use it.)
    - It types via untagged `SendInput` VK_PACKET.
    - Tests:
      - `edit_control_gets_translation_only`: focus the Edit, send Ctrl+Alt+T, type "le chat noir", wait 600 ms → `WM_GETTEXT == "NOIR CHAT LE"`.
      - `richedit_mid_edit`: same in RichEdit, then Left×5, Backspace×5 (removes "chat ") → `"NOIR LE"`.
      - `arabic_and_emoji`: type `"مرحبا بك 👍🏽"` → the field equals the FakeApi output for that text.
      - `password_field_refused`: in the password Edit, Ctrl+Alt+T then "abc" → the field holds `"abc"` (typed normally, no session).
      - `chromium_textarea`: launch `msedge --app=file:///<repo>/src-tauri/tests/fixtures/textarea.html`, type "le chat noir" → `uia::focused_value() == Some("NOIR CHAT LE")`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --features test-support --test harness -- --ignored --test-threads=1` → FAIL.
- [ ] **Step 3: Implement** input, uia, clipboard, field.
- [ ] **Step 4: Run** the same command with hands off the keyboard → PASS (all 5). Run `cargo test --manifest-path src-tauri/Cargo.toml` → the non-ignored suite still passes.
- [ ] **Step 5: Commit** `feat: Windows field writer, UIA focus and clipboard fallback`

---

### Task 13: App wiring (Tauri windows, commands, events, tray)

**Files:** Create `src-tauri/src/ui_sink.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/win/window_style.rs`; modify `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`.

**Interfaces:**
- Consumes: everything above.
- Produces:
  - Windows in `tauri.conf.json`:
    - `strip`: 420×44, transparent, decorations false, alwaysOnTop, skipTaskbar, focus false, visible false, shadow true, `windowEffects: acrylic`.
    - `popup`: 360×180, same flags.
    - `settings`: 720×560, decorations true, visible false, `windowEffects: mica`.
  - `window_style::make_noactivate(window: &WebviewWindow)` sets `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`; `show_at(window, Point)` uses `SetWindowPos` with `SWP_NOACTIVATE | SWP_SHOWWINDOW`; `hide(window)`.
  - `TauriUiSink` implements `UiSink`:
    - Emits `"strip-state"` (`StripState`) to the `strip` window and positions/shows/hides it.
    - Emits `"popup-state"` to `popup`.
    - `notify` uses tauri-plugin-notification.
  - Commands (all `#[tauri::command]`, registered in `lib.rs`):
    - `get_settings() -> Settings`, `save_settings(s: Settings) -> Result<(), String>` (runs `validate_settings`, updates the hook's hotkey list, `cap_enabled` flag and autostart, then saves)
    - `has_api_key() -> bool`, `set_api_key(key: String) -> Result<(), TranslateError>` (tests with `GoogleV2::translate("hello","fr")` before storing; the error is returned as-is)
    - `get_usage() -> UsageSnapshot`
    - `set_target(lang: String)`, `cycle_target()`, `popup_reply(lang: String)`, `popup_close()`, `copy_text(text: String)`, `offline_choice(insert_original: bool)`, `open_settings()`
  - `pub fn validate_settings(s: &Settings) -> Result<(), String>`: all three hotkeys parse, are distinct, favourites are non-empty and contain only `en|fr|ar`, and the target is one of the favourites.
  - Tray: icon + menu:
    - "Translate mode: off/on" (disabled label, updated by the controller via the UiSink)
    - "Target language ▸ English/French/Arabic" (radio)
    - "Settings"
    - "Pause" (check item → `HookState.paused`)
    - "Quit"
    - Left-click opens Settings.
  - Startup: single-instance plugin (a second launch opens Settings); autostart plugin. If `has_api_key()` is false, open Settings (first-run).
  - The API key can change at runtime: `SwappableApi(RwLock<Arc<dyn TranslateApi>>)` implements `TranslateApi` and delegates; `set_api_key` swaps in a new `GoogleV2`.

- [ ] **Step 1: Write failing tests** `commands::tests`:
  - `rejects_bad_hotkey`: toggle `"Ctrl+"` → Err.
  - `rejects_duplicate_hotkeys`: toggle == cycle_target → Err.
  - `rejects_target_not_in_favorites`: favourites `["en"]`, target `"fr"` → Err.
  - `accepts_defaults`: `validate_settings(&Settings::default()).is_ok()`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml commands` → FAIL.
- [ ] **Step 3: Implement** wiring, commands and tray.
- [ ] **Step 4: Verify** `cargo test --manifest-path src-tauri/Cargo.toml` → PASS. Then `pnpm tauri dev`:
  - The tray icon appears.
  - Settings opens (first run, no key).
  - From the dev tools console, `window.__TAURI__.core.invoke('set_api_key', {key: '<owner key>'})` resolves (enable `withGlobalTauri` in dev only, or wait for Task 16).
  - In Notepad, Ctrl+Alt+T then typing "bonjour" leaves `Hello` in Notepad. The strip is blank (UI comes next) but appears near the caret.
- [ ] **Step 5: Commit** `feat: wire controller, windows, commands and tray`

---

### Task 14: UI foundation and compose strip

**Files:** Create `src/lib/types.ts`, `src/lib/ipc.ts`, `src/lib/theme.ts`, `src/styles/tokens.css`, `src/windows/ComposeStrip.tsx`, `src/windows/ComposeStrip.test.tsx`; modify `src/main.tsx`, `src/test/setup.ts`.

**Interfaces:**
- Consumes: event `"strip-state"` with `StripState` (snake_case fields as in Task 10); commands `cycle_target`, `offline_choice`, `open_settings`.
- Produces:
  - `types.ts` mirrors `StripState`, `StripStatus`, `PopupState`, `Settings`, `UsageSnapshot`, `TranslateError`.
  - `ipc.ts`: `onStrip(cb)`, `onPopup(cb)`, `cmd.getSettings()`, `cmd.saveSettings(s)`, `cmd.hasApiKey()`, `cmd.setApiKey(k)`, `cmd.getUsage()`, `cmd.cycleTarget()`, `cmd.setTarget(l)`, `cmd.popupReply(l)`, `cmd.popupClose()`, `cmd.copyText(t)`, `cmd.offlineChoice(b)`, `cmd.openSettings()`.
  - `theme.ts`: `useTheme(theme: "system"|"light"|"dark")` sets `document.documentElement.dataset.theme`.
  - `tokens.css`:
    - Colours: `--accent` teal (`#0F9D82` light / `#3CC9A8` dark), `--warn` amber `#D98E04`, `--danger` `#D64545`.
    - Translucent surfaces over the native acrylic/mica.
    - Radii: 12px cards, 999px pill. Fonts `"Inter", "IBM Plex Sans Arabic", system-ui`.
    - Motion: `--dur-fast: 120ms`; `@media (prefers-reduced-motion: reduce)` disables animation.
  - `main.tsx` imports `@fontsource/inter/400.css`, `/500.css`, `@fontsource/ibm-plex-sans-arabic/400.css`, `/500.css` and `tokens.css`, then renders by `getCurrentWebviewWindow().label`.
  - ComposeStrip:
    - Pill with a language badge `AR → EN` (`(detected ?? "auto")` uppercased → target uppercased; click → `cycleTarget`).
    - The source text with `dir="auto"` and a caret marker at `caret` (graphemes via `Intl.Segmenter`).
    - A status dot: `data-status` = status.
    - Status copy: `Offline — will retry`, `API key problem` + button `Open settings`, `Free limit reached`, `Google quota exceeded`, `Press Ctrl+V to paste`, `Not available in password fields`.
    - When `offer_original`: buttons `Insert original` / `Discard`.
    - The badge pulses on target change; the text crossfades.

- [ ] **Step 1: Write failing tests** (mock `ipc.ts` with `vi.mock`, driving the `onStrip` callback)
  - `renders_rtl_source_with_caret`: state `{source:"مرحبا", caret:2, …}` → element with `dir="auto"` contains the text; the caret marker is preceded by exactly 2 graphemes.
  - `badge_shows_detected_to_target`: `{detected:"ar", target:"en"}` → text `AR → EN`; click → `cmd.cycleTarget` called.
  - `offline_offer_buttons`: `{offer_original:true}` → click "Insert original" → `cmd.offlineChoice(true)`.
  - `key_error_opens_settings`: status `key_error` → button "Open settings" calls `cmd.openSettings`.
  - `hidden_when_not_visible`: `{visible:false}` → nothing rendered.
- [ ] **Step 2: Run** `pnpm test ComposeStrip` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS. `pnpm tauri dev`: the strip matches the approved mockup in light and dark (toggle the Windows theme).
- [ ] **Step 5: Commit** `feat: compose strip UI`

---

### Task 15: Read popup

**Files:** Create `src/windows/ReadPopup.tsx`, `src/windows/ReadPopup.test.tsx`.

**Interfaces:**
- Consumes: event `"popup-state"`; commands `copy_text`, `popup_reply`, `popup_close`.
- Produces:
  - A card showing `"<Detected language> detected → <Target>"` (names English/French/Arabic, else the code uppercased) and the translation (`dir="auto"`).
  - `Copy` (→ `copyText`; the label switches to `Copied`, then the popup closes after 600 ms via `popupClose`).
  - `Reply in <detected language name>` (hidden when detected is missing or equals the target).
  - A close (×) icon button → `popupClose`. Error copy reuses the strip messages.

- [ ] **Step 1: Write failing tests**
  - `shows_detected_and_translation`: `{detected:"fr", target:"en", translation:"The invoice will be sent."}` → `French detected → English` and the text.
  - `copy_invokes_and_confirms`: click Copy → `cmd.copyText` with the translation; label `Copied`.
  - `reply_in_detected_language`: click "Reply in French" → `cmd.popupReply("fr")`.
  - `no_reply_when_same_language`: detected `"en"`, target `"en"` → no reply button.
  - `error_state`: `{error:"Offline"}` → `Offline — will retry` text.
- [ ] **Step 2: Run** `pnpm test ReadPopup` → FAIL.
- [ ] **Step 3: Implement.** **Step 4: Run** → PASS; manual: select French text in Chrome, Ctrl+Alt+R → the popup appears beside the selection.
- [ ] **Step 5: Commit** `feat: read popup UI`

---

### Task 16: Settings window and first run

**Files:** Create `src/windows/settings/SettingsWindow.tsx`, `FirstRun.tsx`, `HotkeyRecorder.tsx`, `UsageBar.tsx`, `SettingsWindow.test.tsx`.

**Interfaces:**
- Consumes: commands `get_settings`, `save_settings`, `has_api_key`, `set_api_key`, `get_usage`.
- Produces:
  - **FirstRun** (shown when `hasApiKey()` is false):
    - Heading "Connect Google Translate", a key field (masked, with a show toggle), and `Test and save` → `setApiKey` with a spinner.
    - Error copy: InvalidKey "That key didn't work. Check it in Google Cloud Console.", Offline "Couldn't reach Google. Check your connection.", QuotaExceeded "This key is out of quota.".
    - On success it continues to "Pick your languages" (favourites EN/FR/AR checkboxes, at least one, reorderable with up/down buttons) and then `Done`.
  - **SettingsWindow** sections, auto-saving on change via `saveSettings` (the backend error is shown inline):
    - API key: status "Connected" and a `Change key` button that reopens the key field.
    - Languages: favourites order and the current target.
    - Hotkeys: a `HotkeyRecorder` per action (Translate mode, Cycle language, Translate selection). It records the next key combo and displays it like `Ctrl+Alt+T`. It rejects modifier-only or no-modifier combos with "Add Ctrl, Alt or Win", and duplicates with "Already used for <action>".
    - Appearance: System/Light/Dark.
    - Startup: "Start with Windows".
    - Usage: `UsageBar` "This month: 41,200 / 500,000 free" (`toLocaleString("en-US")`), a bar with `data-warn="true"` at ≥ 80%, and the toggle "Stop at the free limit" (`cap_enabled`).
  - Content max-width 560px on the Mica background.

- [ ] **Step 1: Write failing tests**
  - `first_run_when_no_key`: `hasApiKey → false` → heading "Connect Google Translate".
  - `invalid_key_message`: `setApiKey` rejects `"InvalidKey"` → the exact InvalidKey copy appears.
  - `hotkey_recorder_rejects_duplicate`: record `Ctrl+Alt+L` on Translate mode → "Already used for Cycle language".
  - `theme_change_saves`: choose Dark → `cmd.saveSettings` called with `theme:"dark"`.
  - `usage_bar_formats_and_warns`: usage `{chars:410000, limit:500000}` → text `410,000 / 500,000 free` and the bar has `data-warn="true"`.
- [ ] **Step 2: Run** `pnpm test SettingsWindow` → FAIL.
- [ ] **Step 3: Implement.** **Step 4: Run** → PASS; `pnpm test` (all) → PASS.
- [ ] **Step 5: Commit** `feat: settings window and first-run flow`

---

### Task 17: Release build and manual sign-off

**Files:** Create `docs/manual-checklist.md`, `README.md` (how to build, run, get a Google API key restricted to the Cloud Translation API, and allow the app in antivirus if flagged).

- [ ] **Step 1: Write `docs/manual-checklist.md`** from spec §5:
  - For each app (WhatsApp Desktop, WhatsApp Web, Gmail in Chrome, Telegram, Word, Notepad, Outlook): AR→EN, AR→FR, edit mid-sentence, Enter to send, read popup on a received message.
  - Global rows: ≤ 400 ms after pause; no flicker; idle RAM < 50 MB (Task Manager, 2 minutes idle); password field refused; Alt+Tab mid-sentence leaves the English and stops; the 80% notification (temporarily edit `usage.json`).
- [ ] **Step 2: Build** `pnpm tauri build` → expect `src-tauri/target/release/bundle/nsis/Live Translator_*_x64-setup.exe` (≈ 10 MB).
- [ ] **Step 3: Install and run the full checklist** with the owner's real key; record pass/fail per row in the checklist file. Fix failures as separate small TDD commits (first add a controller or harness test reproducing each one).
- [ ] **Step 4: Run all automated tests** `cargo test --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml --features test-support --test harness -- --ignored --test-threads=1 && pnpm test` → all PASS.
- [ ] **Step 5: Commit** `docs: manual checklist results and README`
