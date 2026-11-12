import { useEffect, useRef, useState } from "react";
import { cmd, onStrip } from "../lib/ipc";
import { graphemes } from "../lib/langs";
import { STATUS_COPY } from "../lib/messages";
import type { StripState } from "../lib/types";
import "./strip.css";

const WARN = new Set(["offline", "key_error", "cap", "quota_error", "error", "paste_fallback", "password"]);

export default function ComposeStrip() {
  const [s, setS] = useState<StripState | null>(null);
  const [pulse, setPulse] = useState(false);
  const lastTarget = useRef<string | null>(null);

  useEffect(() => {
    let off: (() => void) | undefined;
    onStrip(setS).then((u) => (off = u));
    return () => off?.();
  }, []);

  useEffect(() => {
    if (!s) return;
    if (lastTarget.current !== null && lastTarget.current !== s.target) {
      setPulse(true);
      const t = setTimeout(() => setPulse(false), 280);
      lastTarget.current = s.target;
      return () => clearTimeout(t);
    }
    lastTarget.current = s.target;
  }, [s?.target]);

  if (!s || !s.visible) return null;

  const g = graphemes(s.source);
  const before = g.slice(0, s.caret).join("");
  const after = g.slice(s.caret).join("");
  const from = (s.detected ?? "auto").toUpperCase();
  const to = s.target.toUpperCase();
  const msg = STATUS_COPY[s.status];
  const tone = WARN.has(s.status) ? "warn" : "normal";

  return (
    <div className="strip-host">
      <div className="strip" data-tone={tone}>
        <button
          className={"badge" + (pulse ? " pulse" : "")}
          onClick={() => cmd.cycleTarget()}
          title="Change target language"
          aria-label={`${from} to ${to}, change target language`}
        >
          {from} <span aria-hidden="true">→</span> {to}
        </button>
        <div className="source" dir="auto" data-testid="source">
          {s.source === "" && s.status !== "password" ? (
            <span className="placeholder">Type to translate…</span>
          ) : (
            <>
              <span data-testid="before-caret">{before}</span>
              <span className="caret" aria-hidden="true" />
              <span>{after}</span>
            </>
          )}
        </div>
        {msg && <span className="msg">{msg}</span>}
        {s.status === "key_error" && (
          <button className="link-btn" onClick={() => cmd.openSettings()}>
            Open settings
          </button>
        )}
        {s.offer_original && (
          <>
            <button className="link-btn" onClick={() => cmd.offlineChoice(true)}>
              Insert original
            </button>
            <button className="link-btn" onClick={() => cmd.offlineChoice(false)}>
              Discard
            </button>
          </>
        )}
        <span className="dot" data-status={s.status} aria-hidden="true" />
      </div>
    </div>
  );
}
