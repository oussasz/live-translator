# Live Translator

Type in Arabic, French or English inside **any** text field on Windows, and the field fills with the translation as you type. Your original text stays visible in a small strip near the cursor. Select text anywhere and press a shortcut to read it in your language.

Personal tool. Runs locally and uses your own API key for **Groq** (default, free plan works) or **Google Cloud Translation**. Pick the service, and for Groq the model, in Settings.

## Use it

| Shortcut | What it does |
|---|---|
| `Ctrl+Alt+T` | Turn translate mode on or off in the focused text field |
| `Esc` | Leave translate mode (the translated text stays) |
| `Ctrl+Alt+L` | Switch the target language (English → French → Arabic) |
| `Ctrl+Alt+R` | Translate the selected text in a popup |
| `Enter` | Sends your message once the translation is in the field |

All shortcuts can be changed in Settings (tray icon → Settings).

- Translate mode never starts in password fields.
- Clicking elsewhere or switching windows ends translate mode; the translation already written stays.
- If Google can't be reached, the strip says so and retries. When you leave, you can insert your original text instead.
- Usage is counted locally: Groq requests per day (free plan: 1,000/day, 30/minute; the app paces itself), or Google characters per month (stops at the free 500,000 unless you turn that off).

## Get a Groq API key

1. Sign in at console.groq.com.
2. API Keys → Create API key.
3. Paste it into the app's first-run screen (Groq is selected by default).

Models offered: GPT-OSS 20B (fastest, default), GPT-OSS 120B (best quality), Llama 3.3 70B, Llama 3.1 8B.

## Get a Google API key (optional)

1. Open Google Cloud Console and create (or pick) a project.
2. Enable **Cloud Translation API**.
3. APIs and Services → Credentials → Create credentials → API key.
4. Restrict the key to the Cloud Translation API.
5. Paste it into the app's first-run screen. It is tested once, then stored in Windows Credential Manager.

## Build from source

Requirements: Windows 10/11, Rust (MSVC toolchain), Node 20+, pnpm, Visual Studio Build Tools (C++), WebView2 runtime (built into Windows 11).

```bash
pnpm install
pnpm tauri dev      # run in development
pnpm tauri build    # installer in src-tauri/target/release/bundle/nsis/
```

## Tests

```bash
cargo test --manifest-path src-tauri/Cargo.toml          # core + controller unit tests
pnpm test                                                # UI tests
cargo test --manifest-path src-tauri/Cargo.toml --features test-support --test harness -- --ignored --test-threads=1
```

The last command drives the real desktop for about 15 seconds (test windows and a Chrome window). Keep your hands off the keyboard while it runs.

`pnpm preview:ui` opens a browser gallery of the strip, popup and settings in light and dark.

## Antivirus

The app uses a system-wide keyboard hook, which some antivirus tools flag. It never writes keystrokes to disk or sends them anywhere except the text you translate to the service you chose. If it is flagged, allow `Live Translator.exe` in your antivirus.
