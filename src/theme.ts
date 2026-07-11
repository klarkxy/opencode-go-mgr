export type ThemeMode = "system" | "light" | "dark";

export const THEME_STORAGE_KEY = "ocg-manager.theme";

export function readThemeMode(storage: Pick<Storage, "getItem"> | null): ThemeMode {
  const value = storage?.getItem(THEME_STORAGE_KEY);
  return value === "light" || value === "dark" ? value : "system";
}

