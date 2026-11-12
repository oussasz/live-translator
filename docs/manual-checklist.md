# Manual sign-off checklist (v1)

Install the build, enter your real Google key, then check each row. Mark ✅ / ❌ and note what happened.

## Per app

Apps: WhatsApp Desktop, WhatsApp Web (Chrome), Gmail (Chrome), Telegram Desktop, Word, Notepad, Outlook.

| App | AR→EN typing | AR→FR typing | Edit mid-sentence | Enter sends translation | Read popup on received text |
|---|---|---|---|---|---|
| WhatsApp Desktop | | | | | |
| WhatsApp Web | | | | | |
| Gmail (Chrome) | | | | | |
| Telegram | | | | | |
| Word | | | | | |
| Notepad | | | | | |
| Outlook | | | | | |

## Global

| Check | Result |
|---|---|
| Translation appears ≤ 400 ms after pausing | |
| No visible flicker in the field while typing | |
| Idle RAM < 50 MB (Task Manager, 2 minutes idle) | |
| Password field: translate mode refuses to start | |
| Alt+Tab mid-sentence: English stays, mode ends | |
| French dead keys (^ then e → ê) in translate mode | |
| Strip badge click cycles the language without finishing the sentence | |
| Clicking the strip/popup does not steal focus from the app | |
| 80% notification (edit %APPDATA%\\LiveTranslator\\usage.json to 400000, type a word) | |
| Free limit reached message at 500,000 | |
| Wrong API key: strip shows "API key problem" + Open settings | |
| Wi-Fi off: strip shows "Offline — will retry"; Esc offers Insert original | |
| Start with Windows works (reboot) | |
| Tray: Pause stops all capture; Quit exits | |
