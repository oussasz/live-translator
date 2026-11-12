// The only module that talks to Tauri. Components import from here so tests can mock it.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { PopupState, Settings, StripState, UsageSnapshot } from "./types";

export function onStrip(cb: (s: StripState) => void): Promise<UnlistenFn> {
  return listen<StripState>("strip-state", (e) => cb(e.payload));
}

export function onPopup(cb: (p: PopupState) => void): Promise<UnlistenFn> {
  return listen<PopupState>("popup-state", (e) => cb(e.payload));
}

export function onSettingsChanged(cb: (s: Settings) => void): Promise<UnlistenFn> {
  return listen<Settings>("settings-changed", (e) => cb(e.payload));
}

export const cmd = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  hasApiKey: () => invoke<boolean>("has_api_key"),
  setApiKey: (key: string) => invoke<void>("set_api_key", { key }),
  getUsage: () => invoke<UsageSnapshot>("get_usage"),
  cycleTarget: () => invoke<void>("cycle_target"),
  setTarget: (lang: string) => invoke<void>("set_target", { lang }),
  popupReply: (lang: string) => invoke<void>("popup_reply", { lang }),
  popupClose: () => invoke<void>("popup_close"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  offlineChoice: (insertOriginal: boolean) => invoke<void>("offline_choice", { insertOriginal }),
  openSettings: () => invoke<void>("open_settings"),
};
