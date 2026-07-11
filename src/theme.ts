import type { GlobalThemeOverrides } from "naive-ui";

export type ThemeName = "default" | "white" | "black" | "violet" | "azure" | "celadon" | "copper";
export type ResolvedTheme = Exclude<ThemeName, "default">;

export interface ThemeTokens {
  colorScheme: "light" | "dark";
  canvas: string;
  surface: string;
  surfaceRaised: string;
  ink: string;
  muted: string;
  subtle: string;
  primary: string;
  primaryHover: string;
  primaryPressed: string;
  primarySoft: string;
  success: string;
  successSoft: string;
  warning: string;
  warningSoft: string;
  error: string;
  info: string;
  border: string;
  divider: string;
  shadowSm: string;
  shadowLg: string;
  mascotHalo: string;
  mascotRim: string;
}

export const THEME_STORAGE_KEY = "ocg-manager.theme";

export const THEME_OPTIONS: ReadonlyArray<{ value: ThemeName; label: string; swatch: string }> = [
  { value: "default", label: "默认", swatch: "linear-gradient(135deg, #fff 0 50%, #000 50%)" },
  { value: "white", label: "皓白", swatch: "#FFFFFF" },
  { value: "black", label: "曜黑", swatch: "#000000" },
  { value: "violet", label: "藤紫", swatch: "#6257C8" },
  { value: "azure", label: "霁蓝", swatch: "#1456F0" },
  { value: "celadon", label: "青瓷", swatch: "#0F7C82" },
  { value: "copper", label: "暖铜", swatch: "#8A5A44" },
];

const lightSemantic = {
  success: "#16845B",
  successSoft: "#E6F4EE",
  warning: "#A85F00",
  warningSoft: "#FFF1D8",
  error: "#C33B55",
  info: "#2F6FD4",
  shadowSm: "0 1px 2px rgba(24, 24, 32, 0.05)",
  shadowLg: "0 22px 60px rgba(35, 30, 66, 0.12)",
} as const;

export const THEME_TOKENS: Record<ResolvedTheme, ThemeTokens> = {
  white: {
    colorScheme: "light",
    canvas: "#F7F7F8",
    surface: "#FFFFFF",
    surfaceRaised: "#FFFFFF",
    ink: "#18181B",
    muted: "#5F6068",
    subtle: "#6F6F78",
    primary: "#18181B",
    primaryHover: "#303038",
    primaryPressed: "#050506",
    primarySoft: "#ECECEF",
    border: "#E3E3E8",
    divider: "#E9E9ED",
    mascotHalo: "rgba(24, 24, 27, 0.06)",
    mascotRim: "transparent",
    ...lightSemantic,
  },
  black: {
    colorScheme: "dark",
    canvas: "#000000",
    surface: "#0B0B0D",
    surfaceRaised: "#151519",
    ink: "#F5F5F7",
    muted: "#B6B3BF",
    subtle: "#8C8994",
    primary: "#F5F5F7",
    primaryHover: "#FFFFFF",
    primaryPressed: "#D6D6DC",
    primarySoft: "#1D1D22",
    success: "#56C596",
    successSoft: "#18372C",
    warning: "#E7AE55",
    warningSoft: "#3C2E18",
    error: "#F08095",
    info: "#74A6F6",
    border: "#28272F",
    divider: "#222228",
    shadowSm: "none",
    shadowLg: "0 22px 60px rgba(0, 0, 0, 0.60)",
    mascotHalo: "rgba(255, 255, 255, 0.14)",
    mascotRim: "rgba(255, 255, 255, 0.48)",
  },
  violet: {
    colorScheme: "light",
    canvas: "#F6F6FA",
    surface: "#FFFFFF",
    surfaceRaised: "#FFFFFF",
    ink: "#181820",
    muted: "#5E5D6A",
    subtle: "#706E7A",
    primary: "#6257C8",
    primaryHover: "#6C61D0",
    primaryPressed: "#4F45AD",
    primarySoft: "#ECE9FF",
    border: "#E3E1EA",
    divider: "#E9E7EE",
    mascotHalo: "rgba(98, 87, 200, 0.10)",
    mascotRim: "transparent",
    ...lightSemantic,
  },
  azure: {
    colorScheme: "light",
    canvas: "#F3F6FA",
    surface: "#FFFFFF",
    surfaceRaised: "#FFFFFF",
    ink: "#18202B",
    muted: "#5D6673",
    subtle: "#68717D",
    primary: "#1456F0",
    primaryHover: "#336DF4",
    primaryPressed: "#0442D2",
    primarySoft: "#F0F4FF",
    border: "#DCE3EC",
    divider: "#E6EAF0",
    mascotHalo: "rgba(20, 86, 240, 0.10)",
    mascotRim: "transparent",
    ...lightSemantic,
  },
  celadon: {
    colorScheme: "light",
    canvas: "#F2F7F8",
    surface: "#FFFFFF",
    surfaceRaised: "#FFFFFF",
    ink: "#172126",
    muted: "#53666B",
    subtle: "#617377",
    primary: "#0F7C82",
    primaryHover: "#16747A",
    primaryPressed: "#0A5F64",
    primarySoft: "#E4F4F4",
    border: "#D7E4E7",
    divider: "#E1EBED",
    mascotHalo: "rgba(15, 124, 130, 0.10)",
    mascotRim: "transparent",
    ...lightSemantic,
  },
  copper: {
    colorScheme: "light",
    canvas: "#FAF7F4",
    surface: "#FFFDFB",
    surfaceRaised: "#FFFFFF",
    ink: "#2A211D",
    muted: "#6C5A50",
    subtle: "#77665C",
    primary: "#8A5A44",
    primaryHover: "#9C6950",
    primaryPressed: "#6F4635",
    primarySoft: "#F3EAE5",
    border: "#E6DAD3",
    divider: "#ECE3DE",
    mascotHalo: "rgba(138, 90, 68, 0.10)",
    mascotRim: "transparent",
    ...lightSemantic,
  },
};

export const CHART_PALETTE = [
  "var(--ocg-primary)",
  "#2F6FD4",
  "#16845B",
  "#A85F00",
  "#C33B55",
  "#0F8C91",
  "#7B7987",
  "#A454B8",
] as const;

const themeNames = new Set<ThemeName>(THEME_OPTIONS.map(({ value }) => value));

export function readTheme(storage: Pick<Storage, "getItem"> | null): ThemeName {
  let value: string | null | undefined;
  try {
    value = storage?.getItem(THEME_STORAGE_KEY);
  } catch {
    return "default";
  }
  if (value === "system") return "default";
  if (value === "light") return "white";
  if (value === "dark") return "black";
  return value && themeNames.has(value as ThemeName) ? value as ThemeName : "default";
}

export function getThemeStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function writeTheme(storage: Pick<Storage, "setItem"> | null, theme: ThemeName): void {
  try {
    storage?.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // A private or locked-down browser may reject persistence; the in-memory theme still works.
  }
}

export function resolveTheme(theme: ThemeName, osTheme: string | null | undefined): ResolvedTheme {
  return theme === "default" ? (osTheme === "dark" ? "black" : "white") : theme;
}

export function getThemeTokens(theme: ThemeName, osTheme: string | null | undefined): ThemeTokens {
  return THEME_TOKENS[resolveTheme(theme, osTheme)];
}

export function applyTheme(root: HTMLElement, resolved: ResolvedTheme, tokens: ThemeTokens): void {
  root.dataset.theme = resolved;
  root.style.colorScheme = tokens.colorScheme;
  const values: Record<string, string> = {
    "--ocg-canvas": tokens.canvas,
    "--ocg-surface": tokens.surface,
    "--ocg-surface-raised": tokens.surfaceRaised,
    "--ocg-ink": tokens.ink,
    "--ocg-muted": tokens.muted,
    "--ocg-subtle": tokens.subtle,
    "--ocg-primary": tokens.primary,
    "--ocg-primary-soft": tokens.primarySoft,
    "--ocg-success": tokens.success,
    "--ocg-success-soft": tokens.successSoft,
    "--ocg-warning": tokens.warning,
    "--ocg-warning-soft": tokens.warningSoft,
    "--ocg-error": tokens.error,
    "--ocg-info": tokens.info,
    "--ocg-border": tokens.border,
    "--ocg-divider": tokens.divider,
    "--ocg-shadow-sm": tokens.shadowSm,
    "--ocg-shadow-lg": tokens.shadowLg,
    "--ocg-mascot-halo": tokens.mascotHalo,
    "--ocg-mascot-rim": tokens.mascotRim,
  };
  for (const [name, value] of Object.entries(values)) root.style.setProperty(name, value);
}

export function toNaiveThemeOverrides(tokens: ThemeTokens): GlobalThemeOverrides {
  return {
    common: {
      bodyColor: tokens.canvas,
      cardColor: tokens.surface,
      modalColor: tokens.surfaceRaised,
      popoverColor: tokens.surfaceRaised,
      tableColor: tokens.surface,
      primaryColor: tokens.primary,
      primaryColorHover: tokens.primaryHover,
      primaryColorPressed: tokens.primaryPressed,
      primaryColorSuppl: tokens.primaryHover,
      textColorBase: tokens.ink,
      textColor1: tokens.ink,
      textColor2: tokens.muted,
      textColor3: tokens.subtle,
      successColor: tokens.success,
      warningColor: tokens.warning,
      errorColor: tokens.error,
      infoColor: tokens.info,
      borderColor: tokens.border,
      dividerColor: tokens.divider,
      borderRadius: "10px",
      fontFamily: '"Segoe UI Variable Text", "Noto Sans SC", "Microsoft YaHei UI", sans-serif',
      fontFamilyMono: '"Cascadia Mono", Consolas, monospace',
    },
  };
}
