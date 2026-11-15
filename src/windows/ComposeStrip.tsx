import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { cmd, onStrip } from "../lib/ipc";
import { graphemes } from "../lib/langs";
import { STATUS_COPY } from "../lib/messages";
import type { StripState } from "../lib/types";
import "./strip.css";

const WARN = new Set(["offline", "key_error", "cap", "quota_error", "rate_limited", "error", "paste_fallback", "password"]);

export default function ComposeStrip() {
  const [s, setS] = useState<StripState | null>(null);
  const [pulse, setPulse] = useState(false);
  const lastTarget = useRef<string | null>(null);
  const caretRef = useRef<HTMLSpanElement>(null);
  // Drag: screen position where it started, and how far (physical px) we've already moved the window.
  const drag = useRef<{ x0: number; y0: number; sentX: number; sentY: number } | null>(null);

  // Long sentences: keep the caret (where you're typing) in view instead of cutting off the end.
  useLayoutEffect(() => {
    caretRef.current?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  });

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
    <div className="strip-host" onMouseEnter={() => cmd.overlayHover(true)} onMouseLeave={() => cmd.overlayHover(false)}>
      <div className="strip" data-tone={tone}>
        <button
          className="grip"
          aria-label="Move the bar (double-click to follow the cursor again)"
          title="Drag to move · double-click to follow the cursor"
          onPointerDown={(e) => {
            if (e.button !== 0) return;
            drag.current = { x0: e.screenX, y0: e.screenY, sentX: 0, sentY: 0 };
            e.currentTarget.setPointerCapture?.(e.pointerId);
          }}
          onPointerMove={(e) => {
            const d = drag.current;
            if (!d) return;
            const dpr = window.devicePixelRatio || 1;
            const tx = Math.round((e.screenX - d.x0) * dpr);
            const ty = Math.round((e.screenY - d.y0) * dpr);
            const dx = tx - d.sentX;
            const dy = ty - d.sentY;
            if (dx !== 0 || dy !== 0) {
              d.sentX = tx;
              d.sentY = ty;
              cmd.stripDragBy(dx, dy);
            }
          }}
          onPointerUp={(e) => {
            if (!drag.current) return;
            drag.current = null;
            e.currentTarget.releasePointerCapture?.(e.pointerId);
            cmd.stripDragEnd();
          }}
          onDoubleClick={() => cmd.stripUnpin()}
        >
          <svg width="8" height="14" viewBox="0 0 8 14" aria-hidden="true">
            {[0, 1].map((c) => [0, 1, 2].map((r) => <circle key={`${c}${r}`} cx={2 + c * 4} cy={2 + r * 5} r="1.3" />))}
          </svg>
        </button>
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
              <span className="caret" ref={caretRef} aria-hidden="true" />
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
