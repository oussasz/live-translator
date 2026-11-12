# Live Translator — Design Spec

- **Date:** 2026-11-12
- **Status:** Draft, awaiting owner review
- **Working name:** Live Translator (final name to be chosen by the owner)

## 1. Purpose

A personal Windows desktop tool that translates what the owner types **directly inside any app's input field, in real time**, without switching windows or copy-pasting. Example: the owner types Arabic in WhatsApp, and the message field shows the English translation, updated live as they type, like Google Translate. It also translates text the owner selects (incoming messages).

### Decisions made with the owner

| Topic | Decision |
|---|---|
| Audience | Personal tool, one user, one PC. No accounts, no backend, no billing. |
| Translation provider | Google Cloud Translation **v2 (Basic)** with the owner's own API key. |
| Typing behaviour | The field shows **only the translation**, updated live. Original text is shown in a floating strip. |
| Activation | Toggle hotkey per use (not always-on). |
| Languages | Arabic, French, English, any direction. Source auto-detected; target chosen from favourites. |
| Reading | Select text + hotkey shows a translation popup. |
| App shell | System tray app + settings window. No history, no main translate window. |
| Platform | Windows 10/11 only. |
| Stack | Tauri 2: Rust core + React/TypeScript UI. |

### Success criteria

1. In WhatsApp Desktop/Web, Gmail (Chrome), Telegram, Word, Notepad and Outlook, typing Arabic in translate mode leaves only the correct English (or French) in the field.
2. The translation appears in the field ≤ 400 ms after the owner pauses typing (normal network).
3. No visible flicker or corrupted text in the field; text that was already in the field before translate mode is never modified.
4. Idle memory < 50 MB; installer ≈ 10 MB.
5. The app never spends money beyond the Google free tier unless the owner disables the cap.

## 2. Architecture

```
┌──────────────────────── Rust core (Tauri backend) ────────────────────────┐
│ KeyboardHook ──► ComposeSession ──► Translator ──► FieldWriter            │
│      │               │   (buffer, sentence lock)   (v2 API, cache,        │
│      │               │                              debounce, seq ids)    │
│ HotkeyRouter    FocusWatcher (UIA)   UsageMeter    Settings + KeyStore    │
└──────┬──────────────────┬───────────────────────────────────┬─────────────┘
       │ events           │ events                            │ commands
┌──────▼──────┐   ┌───────▼───────┐                   ┌───────▼────────┐
│ ComposeStrip│   │  ReadPopup    │                   │ SettingsWindow │
│ (React)     │   │  (React)      │                   │ (React)        │
└─────────────┘   └───────────────┘                   └────────────────┘
```

### 2.1 Rust core units

Each unit has one job and a small interface, and is testable in isolation.

- **KeyboardHook.** Installs a `WH_KEYBOARD_LL` hook on a dedicated thread. Always passes keys through except: registered hotkeys, and, while a compose session is active, text-producing keys and editing keys (Backspace, Delete, Ctrl+Backspace, ←/→, Home/End, Ctrl+V), which are **swallowed** and forwarded to the session. Converts virtual keys to characters with `ToUnicodeEx` using the foreground window's keyboard layout, including dead keys for French accents. Ignores injected events (`LLKHF_INJECTED`) so the app never captures its own output.
- **HotkeyRouter.** Matches configurable hotkeys. Defaults: `Ctrl+Alt+T` toggle translate mode, `Esc` exit translate mode, `Ctrl+Alt+L` cycle target language, `Ctrl+Alt+R` translate selection.
- **ComposeSession.** Owns the state of one translate-mode session:
  - `source_buffer`: the owner's original text, with its own caret.
  - `locked_output`: translations of finished sentences, already final in the field.
  - `live_output`: the current sentence's translation as last written to the field, plus its length in UTF-16 code units and in grapheme clusters.
  - A sentence is **locked** when the owner types sentence-ending punctuation (`. ! ? ؟ …`) followed by a space, or presses Enter. Only the unlocked tail is re-translated.
  - **Enter** passes through to the app (sends the message). The session stays active and starts a fresh sentence.
- **Translator.** Calls `POST https://translation.googleapis.com/language/translate/v2` with `q`, `target`, `format=text`, `key`. Source is auto-detected, and the detected language is shown in the strip.
  - Debounce: 200 ms after the last keystroke, or immediately after a space or punctuation.
  - Each request carries an increasing sequence id. A response is applied only if it is the newest issued for that session; older responses are dropped.
  - In-memory LRU cache keyed by `(text, target)` to avoid repeat charges.
  - Timeout 3 s. Errors are mapped to `Offline`, `InvalidKey`, `QuotaExceeded`, `Other(msg)`.
- **FieldWriter.** Replaces `live_output` in the focused field with the new translation:
  1. Select the previous live output with `Shift+←` × N, where N is its length in the units the target control uses (grapheme clusters by default; UTF-16 for controls detected via UIA as classic Win32 edits).
  2. Type the new text with `SendInput` + `KEYEVENTF_UNICODE` (typing over the selection replaces it). The clipboard is never used on this path.
  - If the new text is empty, it sends Delete once instead.
  - **Fallback:** if the write is detected to have failed (the UIA value is unchanged after the write, or the target is elevated/blocks input), the translation is put on the clipboard and the strip shows "Press Ctrl+V to paste".
- **FocusWatcher.** Uses UI Automation focus-changed events plus a foreground-window check.
  - Refuses to start a session in password fields (`IsPassword`).
  - Ends the session (keeping the English already written) when focus or the foreground window changes.
  - On a mouse click inside the session's field, locks the current sentence and restarts at the new caret.
  - Provides the caret rectangle (UIA `TextPattern`, falling back to `GetGUIThreadInfo`) to position the strip.
- **SelectionReader.** For `Ctrl+Alt+R`: reads the selected text through UIA `TextPattern.GetSelection`. Fallback: save clipboard → send Ctrl+C → read → restore clipboard.
- **UsageMeter.** Counts characters sent to Google, stored per calendar month in a local JSON file. Emits warnings at 80% and 100% of 500,000. **Hard cap at the free limit is on by default**: when reached, translation requests stop and the strip/popup explain why.
- **Settings + KeyStore.** Settings in `%APPDATA%\LiveTranslator\settings.json` (favourite languages, current target, hotkeys, theme, start with Windows, cap on/off). The API key is stored in Windows Credential Manager, never in a plain file.

### 2.2 React UI windows

All UI windows are transparent, frameless, always-on-top Tauri windows that **never take focus** (`WS_EX_NOACTIVATE`, `focus: false`), except Settings, which is a normal window.

1. **ComposeStrip.** A frosted pill near the caret, shown while translate mode is on. Shows the language badge (`AR → EN`, clickable to cycle), the original text with its caret (RTL-aware), and a status dot: green = up to date, shimmer = translating, amber = offline/error with a short message. Positioned above the caret; flips below it if there's no room.
2. **ReadPopup.** A card near the selection: detected language → target, the translation, `Copy`, and `Reply in <language>` (starts translate mode with that language as target). Closes on Esc, a click outside, or after copy.
3. **SettingsWindow.** Opened from the tray. Sections: API key (paste, Test button, status), favourite languages (ordered EN/FR/AR), hotkey recorder, theme (system/light/dark), start with Windows, free-tier cap toggle, monthly usage meter.
4. **First-run screen** (in the settings window): paste API key → Test → choose favourites → done.

**Tray menu:** Translate mode status, Target language ▸, Settings, Pause app, Quit.

### 2.3 Data flow (typing)

1. `Ctrl+Alt+T` → FocusWatcher checks the field (not a password field) → ComposeSession starts and the strip appears at the caret.
2. A key is pressed → KeyboardHook swallows it → ComposeSession updates `source_buffer` → the strip shows it instantly.
3. Debounce fires → Translator requests the unlocked tail (seq id n).
4. The response for n arrives and n is the newest → FieldWriter replaces `live_output` → the strip status turns green.
5. Punctuation + space, or Enter → the sentence is locked; the next keystrokes start a new tail.
6. Esc / toggle / focus change → the session ends, the field keeps the English, and the strip fades out.

## 3. Error handling and edge cases

| Situation | Behaviour |
|---|---|
| Focus/window change | Session ends; translated text already written stays; nothing is ever typed into another window. |
| Mouse click in the field | Current sentence locked; new sentence starts at the new caret. |
| Password field | Translate mode refuses to start; the strip briefly shows "Not available in password fields". |
| Shortcuts (Ctrl+C/Z, Alt+Tab, Win, F-keys) | Pass through untouched. |
| Ctrl+V in translate mode | Clipboard text is inserted into `source_buffer` and translated. |
| Offline / timeout | Strip amber "Offline — will retry" with automatic retry; on exit, the owner chooses: insert original or discard. |
| Invalid/missing key | Strip "API key problem — open settings" (one click opens Settings). |
| Quota/billing error or local cap reached | Clear message; tray notification at 80% and 100%. |
| Out-of-order responses | Dropped via sequence ids. |
| RTL | Arabic is rendered with `dir="auto"` in the UI; Arabic output to the field is sent as plain Unicode (the target app handles direction). |
| Emoji / combined characters | Lengths tracked as grapheme clusters (UTF-16 for classic edits) so selection never splits a character. |
| Long text | Only the current sentence is re-translated; finished sentences are locked. |
| App blocks simulated input / elevated window | Clipboard fallback + "Press Ctrl+V to paste". The app does not run elevated by default. |

## 4. Visual design

- Windows 11 style: Mica/Acrylic frosted backgrounds, soft shadows, 12 px corners on cards, pill-shaped strip.
- Follows the Windows light/dark setting, with a manual override.
- One accent colour (teal) for active/connected; amber for warnings; red only for blocking errors.
- Fonts bundled: **Inter** (Latin) and **IBM Plex Sans Arabic** (Arabic).
- Motion: strip fade+slide ~120 ms; translation crossfade (no flicker); a short pulse on the language badge when the target changes. Respects Windows "reduce animations".
- UI copy is short, sentence case, and never says "please" or "successfully".

## 5. Testing

- **Rust unit tests:** ComposeSession editing operations; sentence locking; FieldWriter replace computation (counts and keystroke sequence) for Arabic, French accents and emoji; Translator debounce, stale-drop, cache, timeout and error mapping against a local mock HTTP server (never the real API); UsageMeter counting, month rollover, thresholds, cap.
- **UI tests:** Vitest + Testing Library for the strip, popup and settings in light/dark and RTL.
- **Integration harness:** a test window with a Win32 Edit, RichEdit, WebView2 textarea, and password box; scripted translate-mode typing (with the mock translator) asserts the final field contents, password refusal, and focus-change behaviour.
- **Manual checklist (v1 sign-off):** WhatsApp Desktop/Web, Gmail in Chrome, Telegram, Word, Notepad, Outlook. For each: AR→EN, AR→FR, edit mid-sentence, Enter to send, read popup on a received message. Check ≤ 400 ms latency, no flicker, idle RAM < 50 MB.

## 6. Out of scope for v1

macOS/Linux; translation history; per-app auto-on rules; Google-Translate-style main window; voice input; Cloud Translation v3, glossaries or custom models; auto-update; code signing; multiple user accounts.

## 7. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Some editors handle `Shift+←` selection differently (e.g. auto-formatting, mentions, emoji pickers). | Integration harness + manual checklist per target app; the clipboard fallback covers failures. |
| Antivirus may flag a global keyboard hook. | The app is local only, does no logging of keystrokes to disk, and is built from source on the owner's PC. Document how to allow it if flagged. |
| Network latency spikes. | Debounce + stale-drop + cache; the strip shows translating state so the owner knows to wait. |
| Google charges on re-translation. | Translate only the unlocked sentence; cache; free-tier hard cap on by default. |
