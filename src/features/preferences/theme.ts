import { useCallback, useEffect, useLayoutEffect, useState } from "react";

import { readLocalValue, writeLocalValue } from "@/lib/local-storage";

export type ThemePreference = "system" | "light" | "dark";

const THEME_STORAGE_KEY = "lure.theme";

export function readThemePreference(): ThemePreference {
  const stored = readLocalValue(THEME_STORAGE_KEY);
  return stored === "light" || stored === "dark" ? stored : "system";
}

function systemPrefersDark() {
  return typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function useThemePreference() {
  const [preference, setPreferenceState] = useState<ThemePreference>(readThemePreference);
  const [systemDark, setSystemDark] = useState(systemPrefersDark);
  const resolvedTheme = preference === "system" ? (systemDark ? "dark" : "light") : preference;

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    setSystemDark(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  useLayoutEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", resolvedTheme === "dark");
    root.dataset.theme = resolvedTheme;
  }, [resolvedTheme]);

  const setPreference = useCallback((next: ThemePreference) => {
    writeLocalValue(THEME_STORAGE_KEY, next === "system" ? null : next);
    setPreferenceState(next);
  }, []);

  return { preference, resolvedTheme, setPreference };
}
