// Mirrors the Rust serde types (src-tauri/src/controller.rs, core/settings.rs, core/usage.rs).
export type StripStatus =
  | "idle"
  | "translating"
  | "ok"
  | "offline"
  | "key_error"
  | "cap"
  | "quota_error"
  | "error"
  | "paste_fallback"
  | "password";

export interface Point {
  x: number;
  y: number;
}

export interface StripState {
  visible: boolean;
  source: string;
  caret: number;
  target: string;
  detected: string | null;
  status: StripStatus;
  pos: Point | null;
  offer_original: boolean;
}

export type TranslateError = "Offline" | "InvalidKey" | "QuotaExceeded" | "CapReached" | { Other: string };

export interface PopupState {
  visible: boolean;
  source: string;
  translation: string | null;
  detected: string | null;
  target: string;
  error: TranslateError | null;
  pos: Point | null;
}

export type Theme = "system" | "light" | "dark";

export interface Hotkeys {
  toggle: string;
  cycle_target: string;
  read_selection: string;
}

export interface Settings {
  favorites: string[];
  target: string;
  hotkeys: Hotkeys;
  theme: Theme;
  autostart: boolean;
  cap_enabled: boolean;
}

export interface UsageSnapshot {
  month: string;
  chars: number;
  limit: number;
}
