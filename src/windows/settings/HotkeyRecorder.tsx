import { useState, type KeyboardEvent } from "react";

const MODS = new Set(["Control", "Alt", "Shift", "Meta", "AltGraph"]);

/** Turns a keydown into "Ctrl+Alt+T" style, or null for a bare modifier / unsupported key. */
export function comboFromEvent(e: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): string | null {
  if (MODS.has(e.key)) return null;
  let key: string | null = null;
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit[0-9]$/.test(e.code)) key = e.code.slice(5);
  else if (/^F([1-9]|1[0-2])$/.test(e.code)) key = e.code;
  else if (e.code === "Space") key = "Space";
  else if (e.code === "Escape") key = "Esc";
  if (!key) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Win");
  parts.push(key);
  return parts.join("+");
}

interface Props {
  label: string;
  value: string;
  others: { label: string; value: string }[];
  onChange: (combo: string) => void;
}

export function HotkeyRecorder({ label, value, others, onChange }: Props) {
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    e.preventDefault();
    if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.metaKey) {
      setRecording(false);
      return;
    }
    const combo = comboFromEvent(e);
    if (!combo) return;
    if (!(e.ctrlKey || e.altKey || e.metaKey)) {
      setError("Add Ctrl, Alt or Win");
      return;
    }
    const clash = others.find((o) => o.value.toLowerCase() === combo.toLowerCase());
    if (clash) {
      setError(`Already used for ${clash.label}`);
      return;
    }
    setError(null);
    setRecording(false);
    onChange(combo);
  };

  return (
    <div className="field">
      <div className="row">
        <span className="row-label">{label}</span>
        <button
          type="button"
          className={"kbd-btn" + (recording ? " recording" : "")}
          aria-label={`${label} shortcut: ${value}`}
          onClick={() => {
            setError(null);
            setRecording(!recording);
          }}
          onBlur={() => setRecording(false)}
          onKeyDown={onKeyDown}
        >
          {recording ? "Press keys…" : value}
        </button>
      </div>
      {error && <p className="error-text">{error}</p>}
    </div>
  );
}
