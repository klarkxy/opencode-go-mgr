import assert from "node:assert/strict";
import test from "node:test";
import {
  applyTheme,
  getThemeStorage,
  getThemeTokens,
  readTheme,
  resolveTheme,
  THEME_OPTIONS,
  THEME_STORAGE_KEY,
  THEME_TOKENS,
  toNaiveThemeOverrides,
  writeTheme,
} from "./theme.ts";

function storage(value: string | null) {
  return { getItem: (key: string) => key === THEME_STORAGE_KEY ? value : null };
}

function luminance(hex: string): number {
  const rgb = [1, 3, 5].map((start) => Number.parseInt(hex.slice(start, start + 2), 16) / 255)
    .map((channel) => channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4);
  return 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
}

function contrast(a: string, b: string): number {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (lighter + 0.05) / (darker + 0.05);
}

test("readTheme accepts current themes and migrates legacy values", () => {
  for (const { value } of THEME_OPTIONS) assert.equal(readTheme(storage(value)), value);
  assert.equal(readTheme(storage("system")), "default");
  assert.equal(readTheme(storage("light")), "white");
  assert.equal(readTheme(storage("dark")), "black");
  assert.equal(readTheme(storage("legacy")), "default");
  assert.equal(readTheme(null), "default");
  assert.equal(readTheme({ getItem: () => { throw new Error("blocked"); } }), "default");
  assert.equal(getThemeStorage(), null);

  assert.doesNotThrow(() => writeTheme({ setItem: () => { throw new Error("blocked"); } }, "black"));
});

test("only the default theme follows the operating system", () => {
  assert.equal(resolveTheme("default", "light"), "white");
  assert.equal(resolveTheme("default", "dark"), "black");
  assert.equal(resolveTheme("default", null), "white");
  for (const theme of ["white", "black", "violet", "azure", "celadon", "copper"] as const) {
    assert.equal(resolveTheme(theme, "dark"), theme);
    assert.equal(resolveTheme(theme, "light"), theme);
  }
});

test("theme tokens drive CSS and Naive UI from the same values", () => {
  for (const resolved of Object.keys(THEME_TOKENS) as Array<keyof typeof THEME_TOKENS>) {
    const tokens = getThemeTokens(resolved, "light");
    const properties = new Map<string, string>();
    const root = {
      dataset: {},
      style: {
        colorScheme: "",
        setProperty: (name: string, value: string) => properties.set(name, value),
      },
    } as unknown as HTMLElement;
    applyTheme(root, resolved, tokens);
    const common = toNaiveThemeOverrides(tokens).common!;

    assert.equal(root.dataset.theme, resolved);
    const expectedProperties = {
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
    assert.deepEqual(Object.fromEntries(properties), expectedProperties);
    assert.equal(common.primaryColor, tokens.primary);
    assert.equal(common.primaryColorSuppl, tokens.primaryHover);
    assert.equal(common.bodyColor, tokens.canvas);
  }
});

test("approved theme identities and light semantic colors stay stable", () => {
  const identities = {
    white: ["#F7F7F8", "#FFFFFF", "#FFFFFF", "#18181B", "#18181B", "#303038", "#050506", "#ECECEF", "#E3E3E8"],
    black: ["#000000", "#0B0B0D", "#151519", "#F5F5F7", "#F5F5F7", "#FFFFFF", "#D6D6DC", "#1D1D22", "#28272F"],
    violet: ["#F6F6FA", "#FFFFFF", "#FFFFFF", "#181820", "#6257C8", "#6C61D0", "#4F45AD", "#ECE9FF", "#E3E1EA"],
    azure: ["#F3F6FA", "#FFFFFF", "#FFFFFF", "#18202B", "#1456F0", "#336DF4", "#0442D2", "#F0F4FF", "#DCE3EC"],
    celadon: ["#F2F7F8", "#FFFFFF", "#FFFFFF", "#172126", "#0F7C82", "#16747A", "#0A5F64", "#E4F4F4", "#D7E4E7"],
    copper: ["#FAF7F4", "#FFFDFB", "#FFFFFF", "#2A211D", "#8A5A44", "#9C6950", "#6F4635", "#F3EAE5", "#E6DAD3"],
  } as const;
  for (const [name, expected] of Object.entries(identities) as Array<[keyof typeof identities, readonly string[]]>) {
    const tokens = THEME_TOKENS[name];
    assert.deepEqual([
      tokens.canvas,
      tokens.surface,
      tokens.surfaceRaised,
      tokens.ink,
      tokens.primary,
      tokens.primaryHover,
      tokens.primaryPressed,
      tokens.primarySoft,
      tokens.border,
    ], expected);
  }

  const lightNames = ["white", "violet", "azure", "celadon", "copper"] as const;
  for (const name of lightNames) {
    const tokens = THEME_TOKENS[name];
    assert.deepEqual(
      [tokens.success, tokens.successSoft, tokens.warning, tokens.warningSoft, tokens.error, tokens.info],
      ["#16845B", "#E6F4EE", "#A85F00", "#FFF1D8", "#C33B55", "#2F6FD4"],
    );
  }
  const black = THEME_TOKENS.black;
  assert.deepEqual(
    [black.success, black.successSoft, black.warning, black.warningSoft, black.error, black.info],
    ["#56C596", "#18372C", "#E7AE55", "#3C2E18", "#F08095", "#74A6F6"],
  );
});

test("theme text and primary actions meet WCAG AA contrast", () => {
  for (const [name, tokens] of Object.entries(THEME_TOKENS)) {
    const buttonText = name === "black" ? "#000000" : "#FFFFFF";
    assert.ok(contrast(tokens.ink, tokens.canvas) >= 4.5, `${name} body text`);
    assert.ok(contrast(tokens.subtle, tokens.canvas) >= 4.5, `${name} subtle text on canvas`);
    assert.ok(contrast(tokens.subtle, tokens.surface) >= 4.5, `${name} subtle text on surface`);
    assert.ok(contrast(tokens.primary, buttonText) >= 4.5, `${name} primary`);
    assert.ok(contrast(tokens.primaryHover, buttonText) >= 4.5, `${name} hover`);
    assert.ok(contrast(tokens.primaryPressed, buttonText) >= 4.5, `${name} pressed`);
  }
});
