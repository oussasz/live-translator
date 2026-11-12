import { useEffect, useState, type ReactNode } from "react";
import { cmd, onSettingsChanged } from "../lib/ipc";
import { useTheme } from "../lib/theme";
import type { Theme } from "../lib/types";

/** Loads the theme from settings and follows changes. */
export function ThemedRoot({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<Theme>("system");
  useTheme(theme);
  useEffect(() => {
    cmd.getSettings().then((s) => setTheme(s.theme)).catch(() => {});
    let off: (() => void) | undefined;
    onSettingsChanged((s) => setTheme(s.theme)).then((u) => (off = u));
    return () => off?.();
  }, []);
  return <>{children}</>;
}
