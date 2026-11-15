import type { PopupState, Settings, StripState, UsageSnapshot } from "../lib/types";

export const stripQueue: StripState[] = [];
export const popupQueue: PopupState[] = [];

const settings: Settings = {
  favorites: ["en", "fr", "ar"],
  target: "en",
  hotkeys: { toggle: "Ctrl+Alt+T", cycle_target: "Ctrl+Alt+L", read_selection: "Ctrl+Alt+R" },
  theme: "system",
  autostart: true,
  cap_enabled: true,
  provider: "groq",
  groq_model: "openai/gpt-oss-20b",
};

export function onStrip(cb: (s: StripState) => void) {
  const s = stripQueue.shift();
  if (s) setTimeout(() => cb(s));
  return Promise.resolve(() => {});
}

export function onPopup(cb: (p: PopupState) => void) {
  const p = popupQueue.shift();
  if (p) setTimeout(() => cb(p));
  return Promise.resolve(() => {});
}

export function onSettingsChanged() {
  return Promise.resolve(() => {});
}

const usage: UsageSnapshot = { month: "2026-11", chars: 41200, limit: 500000, requests_today: 120, request_limit: 1000 };

export const cmd = {
  getSettings: () => Promise.resolve(structuredClone(settings)),
  saveSettings: () => Promise.resolve(),
  hasApiKey: () => { const k = new URLSearchParams(location.search).get("firstrun") === null; return Promise.resolve({ google: k, groq: k }); },
  setApiKey: () => Promise.reject("InvalidKey"),
  getUsage: () => Promise.resolve(usage),
  cycleTarget: () => Promise.resolve(),
  setTarget: () => Promise.resolve(),
  popupReply: () => Promise.resolve(),
  popupClose: () => Promise.resolve(),
  copyText: () => Promise.resolve(),
  offlineChoice: () => Promise.resolve(),
  openSettings: () => Promise.resolve(),
  stripDragBy: () => Promise.resolve(),
  stripDragEnd: () => Promise.resolve(),
  stripUnpin: () => Promise.resolve(),
};
