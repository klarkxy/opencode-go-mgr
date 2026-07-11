import assert from "node:assert/strict";
import test from "node:test";
import { readThemeMode, THEME_STORAGE_KEY } from "./theme.ts";

test("readThemeMode accepts only supported persisted values", () => {
  const storage = (value: string | null) => ({
    getItem: (key: string) => key === THEME_STORAGE_KEY ? value : null,
  });

  assert.equal(readThemeMode(storage("dark")), "dark");
  assert.equal(readThemeMode(storage("light")), "light");
  assert.equal(readThemeMode(storage("legacy")), "system");
  assert.equal(readThemeMode(null), "system");
});

