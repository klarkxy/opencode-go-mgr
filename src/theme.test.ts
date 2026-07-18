import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
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

function mixHex(foreground: string, background: string, foregroundWeight: number): string {
  const foregroundChannels = foreground.slice(1).match(/../g)!.map((value) => Number.parseInt(value, 16));
  const backgroundChannels = background.slice(1).match(/../g)!.map((value) => Number.parseInt(value, 16));
  return `#${foregroundChannels.map((value, index) => Math.round(
    value * foregroundWeight + backgroundChannels[index] * (1 - foregroundWeight),
  ).toString(16).padStart(2, "0")).join("")}`;
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
    const overrides = toNaiveThemeOverrides(tokens);
    const common = overrides.common!;

    assert.equal(root.dataset.theme, resolved);
    const expectedProperties = {
      "--ocg-font-xs": "12px",
      "--ocg-font-sm": "13px",
      "--ocg-font-md": "14px",
      "--ocg-font-lg": "16px",
      "--ocg-font-xl": "20px",
      "--ocg-font-2xl": "24px",
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
    assert.equal(common.inputColor, tokens.surfaceRaised);
    assert.equal(common.actionColor, tokens.primarySoft);
    assert.equal(common.tableHeaderColor, tokens.primarySoft);
    assert.equal(common.hoverColor, tokens.primarySoft);
    assert.equal(common.pressedColor, tokens.canvas);
    const expectedFontSizes = {
      fontSize: "14px",
      fontSizeMini: "12px",
      fontSizeTiny: "12px",
      fontSizeSmall: "13px",
      fontSizeMedium: "14px",
      fontSizeLarge: "16px",
      fontSizeHuge: "20px",
    } as const;
    for (const [key, expected] of Object.entries(expectedFontSizes)) {
      assert.equal(common[key as keyof typeof expectedFontSizes], expected, `${resolved} ${key}`);
    }
    assert.equal(common.lineHeight, "1.6");
    assert.equal(
      overrides.Input!.border,
      `1px solid color-mix(in srgb, ${tokens.muted} 68%, ${tokens.surfaceRaised})`,
    );
    assert.ok(
      contrast(mixHex(tokens.muted, tokens.surfaceRaised, 0.68), tokens.surfaceRaised) >= 3,
      `${resolved} input boundary`,
    );
  }
});

test("approved theme identities and light semantic colors stay stable", () => {
  const identities = {
    white: ["light", "#F7F7F8", "#FFFFFF", "#FFFFFF", "#18181B", "#18181B", "#303038", "#050506", "#ECECEF", "#E3E3E8", "#E9E9ED"],
    black: ["dark", "#000000", "#0B0B0D", "#151519", "#F5F5F7", "#F5F5F7", "#FFFFFF", "#D6D6DC", "#1D1D22", "#28272F", "#222228"],
    violet: ["light", "#E5DBF0", "#EEE5F6", "#F8F3FC", "#211A2D", "#5B44B4", "#6C55C7", "#49349A", "#D9CBEF", "#BFAAD5", "#D3C5E2"],
    azure: ["light", "#D2E3F2", "#DEEBF7", "#EEF5FB", "#172435", "#0F50E5", "#2A61E6", "#043DBF", "#C9DCF3", "#A9C2DB", "#C3D5E6"],
    celadon: ["light", "#D2E5DC", "#DDECE5", "#EEF6F2", "#172721", "#0B666B", "#127277", "#075358", "#C4DED4", "#A4C6B7", "#BFD8CD"],
    copper: ["light", "#E9D7C8", "#F2E5DA", "#FAF2EB", "#30221B", "#8A4F34", "#9A5D41", "#6F3F2A", "#E4C6B2", "#CAA58E", "#DDC0AD"],
  } as const;
  for (const [name, expected] of Object.entries(identities) as Array<[keyof typeof identities, readonly string[]]>) {
    const tokens = THEME_TOKENS[name];
    assert.deepEqual([
      tokens.colorScheme,
      tokens.canvas,
      tokens.surface,
      tokens.surfaceRaised,
      tokens.ink,
      tokens.primary,
      tokens.primaryHover,
      tokens.primaryPressed,
      tokens.primarySoft,
      tokens.border,
      tokens.divider,
    ], expected);
  }

  const lightNames = ["white", "violet", "azure", "celadon", "copper"] as const;
  for (const name of lightNames) {
    const tokens = THEME_TOKENS[name];
    assert.deepEqual(
      [tokens.success, tokens.successSoft, tokens.warning, tokens.warningSoft, tokens.error, tokens.info],
      ["#0B6844", "#E6F4EE", "#8A4D00", "#FFF1D8", "#A92742", "#245DB6"],
    );
  }
  const black = THEME_TOKENS.black;
  assert.deepEqual(
    [black.success, black.successSoft, black.warning, black.warningSoft, black.error, black.info],
    ["#56C596", "#18372C", "#E7AE55", "#3C2E18", "#F08095", "#74A6F6"],
  );
});

test("colored themes keep broad lightness separation from white", () => {
  for (const name of ["violet", "azure", "celadon", "copper"] as const) {
    const tokens = THEME_TOKENS[name];
    assert.ok(contrast(tokens.canvas, THEME_TOKENS.white.canvas) >= 1.2, `${name} canvas`);
    assert.ok(contrast(tokens.surface, THEME_TOKENS.white.surface) >= 1.2, `${name} surface`);
  }
});

test("stacked chart reuses defined gradients when models exceed the palette", async () => {
  const source = await readFile(new URL("./components/StackedBarChart.vue", import.meta.url), "utf8");
  const segments = source.slice(source.indexOf("const bars = computed"), source.indexOf("// X 轴标签"));

  assert.match(segments, /idx: models\.indexOf\(model\) % CHART_PALETTE\.length/);
});

test("stacked chart exposes daily detail to keyboard and assistive technology", async () => {
  const source = await readFile(new URL("./components/StackedBarChart.vue", import.meta.url), "utf8");

  assert.match(source, /:tabindex="dates\[bi\]\?\.total > 0 \? 0 : -1"/);
  assert.match(source, /:aria-label="barAriaLabel\(bi\)"/);
  assert.match(source, /@focus="onFocus\(bi\)"/);
  assert.match(source, /<desc :id="`chart-description-\$\{gid\}`">\{\{ chartDescription \}\}<\/desc>/);
  assert.match(source, /class="bar-hitbox"/);
  assert.match(source, /\.bar-col:focus-visible \.bar-hitbox[\s\S]*?stroke-width: 2;[\s\S]*?vector-effect: non-scaling-stroke/);
});

test("theme menu closes with Escape from the trigger or any menu descendant", async () => {
  const source = await readFile(new URL("./App.vue", import.meta.url), "utf8");

  assert.match(source, /@keydown\.esc\.prevent\.stop="closeThemeMenu"/);
  assert.match(source, /function closeOpenThemeMenuOnEscape[\s\S]*?event\.key !== "Escape"[\s\S]*?closeThemeMenu\(\)/);
  assert.match(source, /document\.addEventListener\("keydown", closeOpenThemeMenuOnEscape\)/);
  assert.match(source, /document\.removeEventListener\("keydown", closeOpenThemeMenuOnEscape\)/);
});

test("theme text and primary actions meet WCAG AA contrast", () => {
  for (const [name, tokens] of Object.entries(THEME_TOKENS)) {
    const buttonText = name === "black" ? "#000000" : "#FFFFFF";
    assert.ok(contrast(tokens.ink, tokens.canvas) >= 4.5, `${name} body text`);
    assert.ok(contrast(tokens.muted, tokens.canvas) >= 4.5, `${name} muted text on canvas`);
    assert.ok(contrast(tokens.muted, tokens.surface) >= 4.5, `${name} muted text on surface`);
    assert.ok(contrast(tokens.subtle, tokens.canvas) >= 4.5, `${name} subtle text on canvas`);
    assert.ok(contrast(tokens.subtle, tokens.surface) >= 4.5, `${name} subtle text on surface`);
    assert.ok(contrast(tokens.primary, buttonText) >= 4.5, `${name} primary`);
    assert.ok(contrast(tokens.primaryHover, buttonText) >= 4.5, `${name} hover`);
    assert.ok(contrast(tokens.primaryPressed, buttonText) >= 4.5, `${name} pressed`);
    assert.ok(contrast(tokens.primary, tokens.canvas) >= 4.5, `${name} primary on canvas`);
    for (const [status, color] of Object.entries({
      success: tokens.success,
      warning: tokens.warning,
      error: tokens.error,
      info: tokens.info,
    })) {
      assert.ok(contrast(color, tokens.canvas) >= 4.5, `${name} ${status} on canvas`);
      assert.ok(contrast(color, tokens.surface) >= 4.5, `${name} ${status} on surface`);
    }
    assert.ok(contrast(tokens.success, tokens.successSoft) >= 4.5, `${name} success soft`);
    assert.ok(contrast(tokens.warning, tokens.warningSoft) >= 4.5, `${name} warning soft`);
  }
});
