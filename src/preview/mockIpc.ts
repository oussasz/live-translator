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

const usage: UsageSnapshot = { month: "2026-11", chars: 41200, limit: 500000 };

export const cmd = {
  getSettings: () => Promise.resolve(structuredClone(settings)),
  saveSettings: () => Promise.resolve(),
  hasApiKey: () => Promise.resolve(new URLSearchParams(location.search).get("firstrun") === null),
  setApiKey: () => Promise.reject("InvalidKey"),
  getUsage: () => Promise.resolve(usage),
  cycleTarget: () => Promise.resolve(),
  setTarget: () => Promise.resolve(),
  popupReply: () => Promise.resolve(),
  popupClose: () => Promise.resolve(),
  copyText: () => Promise.resolve(),
  offlineChoice: () => Promise.resolve(),
  openSettings: () => Promise.resolve(),
};
