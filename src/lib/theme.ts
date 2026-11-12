import { useEffect } from "react";
import type { Theme } from "./types";

function resolve(theme: Theme): "light" | "dark" {
  if (theme !== "system") return theme;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** Applies the theme to <html data-theme>, following the system when theme is "system". */
export function useTheme(theme: Theme) {
  useEffect(() => {
    const apply = () => {
      document.documentElement.dataset.theme = resolve(theme);
    };
    apply();
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    mq?.addEventListener?.("change", apply);
    return () => mq?.removeEventListener?.("change", apply);
  }, [theme]);
}
