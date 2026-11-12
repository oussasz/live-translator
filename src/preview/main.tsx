import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/inter/400.css";
import "@fontsource/inter/600.css";
import "@fontsource/ibm-plex-sans-arabic/400.css";
import "@fontsource/ibm-plex-sans-arabic/600.css";
import "../styles/tokens.css";
import ComposeStrip from "../windows/ComposeStrip";
import ReadPopup from "../windows/ReadPopup";
import SettingsWindow from "../windows/settings/SettingsWindow";
import { popupQueue, stripQueue } from "./mockIpc";
import type { StripState } from "../lib/types";

const base: StripState = {
  visible: true, source: "", caret: 0, target: "en", detected: null, status: "idle", pos: { x: 0, y: 0 }, offer_original: false,
};
const strips: [string, Partial<StripState>][] = [
  ["Typing Arabic, translation up to date", { source: "مرحبا، هل يمكننا تأجيل الاجتماع؟", caret: 26, detected: "ar", status: "ok" }],
  ["Translating", { source: "Bonjour, je serai en retard", caret: 27, detected: "fr", status: "translating" }],
  ["Just started", { status: "idle" }],
  ["Offline after exit", { source: "salut", caret: 5, detected: "fr", status: "offline", offer_original: true }],
  ["Key problem", { source: "hello", caret: 5, status: "key_error" }],
];

function Strips() {
  return (
    <div style={{ display: "grid", gap: 6 }}>
      {strips.map(([label]) => (
        <div key={label}>
          <div style={{ fontSize: 11, opacity: 0.7, margin: "4px 8px" }}>{label}</div>
          <div style={{ width: 420 }}><ComposeStrip /></div>
        </div>
      ))}
    </div>
  );
}

function Panel({ theme }: { theme: "light" | "dark" }) {
  return (
    <div data-theme={theme} style={{ padding: 20, display: "flex", gap: 24, flexWrap: "wrap", color: "var(--text)" }}>
      <Strips />
      <div style={{ width: 360 }}><ReadPopup /></div>
    </div>
  );
}

for (let i = 0; i < 2; i++) {
  for (const [, s] of strips) stripQueue.push({ ...base, ...s });
  popupQueue.push({
    visible: true, source: "La facture vous sera envoyée vendredi au plus tard.",
    translation: "The invoice will be sent to you by Friday at the latest.", detected: "fr", target: "en", error: null, pos: { x: 0, y: 0 },
  });
}

const view = new URLSearchParams(location.search).get("view");
const wall = "linear-gradient(135deg, #7aa7c7 0%, #c9b6d9 50%, #e7c9a9 100%)";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {view === "settings" ? (
      <div data-theme={new URLSearchParams(location.search).get("theme") ?? "light"} style={{ background: "var(--surface-solid)", minHeight: "100vh" }}>
        <SettingsWindow />
      </div>
    ) : (
      <div style={{ background: wall, minHeight: "100vh" }}>
        <Panel theme="light" />
        <Panel theme="dark" />
      </div>
    )}
  </React.StrictMode>,
);
