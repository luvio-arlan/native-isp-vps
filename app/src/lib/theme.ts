export type ThemeId = "titanium" | "obsidian";

const STORAGE_KEY = "nisp-theme";

export function normalizeTheme(value: string | null | undefined): ThemeId {
  return value === "obsidian" ? "obsidian" : "titanium";
}

/** Last applied skin (persisted per WebView origin) so the first paint matches user settings. */
export function readCachedTheme(): ThemeId {
  try {
    return normalizeTheme(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return "titanium";
  }
}

export function applyTheme(theme: ThemeId): void {
  document.documentElement.setAttribute("data-theme", theme);
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Storage may be unavailable (private mode); the attribute is still applied.
  }
}
