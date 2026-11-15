import { useEffect, useState } from "react";
import { cmd, onPopup } from "../lib/ipc";
import { langName } from "../lib/langs";
import { errorCopy } from "../lib/messages";
import type { PopupState } from "../lib/types";
import "./popup.css";

export default function ReadPopup() {
  const [p, setP] = useState<PopupState | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let off: (() => void) | undefined;
    onPopup((next) => {
      setCopied(false);
      setP(next);
    }).then((u) => (off = u));
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") cmd.popupClose();
    };
    document.addEventListener("keydown", onKey);
    return () => {
      off?.();
      document.removeEventListener("keydown", onKey);
    };
  }, []);

  if (!p || !p.visible) return null;

  const copy = async () => {
    if (!p.translation) return;
    await cmd.copyText(p.translation);
    setCopied(true);
    setTimeout(() => cmd.popupClose(), 600);
  };
  const canReply = !!p.detected && p.detected !== p.target;

  return (
    <div className="popup-host" onMouseEnter={() => cmd.overlayHover(true)} onMouseLeave={() => cmd.overlayHover(false)}>
      <div className="popup" role="dialog" aria-label="Translation">
        <div className="popup-head">
          <span className="popup-langs">
            {langName(p.detected)} detected → {langName(p.target)}
          </span>
          <button className="icon-btn" aria-label="Close" onClick={() => cmd.popupClose()}>
            ×
          </button>
        </div>
        {p.error ? (
          <div className="popup-text error">{errorCopy(p.error)}</div>
        ) : (
          <div className="popup-text" dir="auto">
            {p.translation}
          </div>
        )}
        {!p.error && (
          <div className="popup-actions">
            <button className="btn primary" onClick={copy}>
              {copied ? "Copied" : "Copy"}
            </button>
            {canReply && (
              <button className="btn" onClick={() => cmd.popupReply(p.detected!)}>
                Reply in {langName(p.detected)}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
