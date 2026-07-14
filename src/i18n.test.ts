import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_LOCALE,
  LOCALE_OPTIONS,
  LOCALE_STORAGE_KEY,
  matchLocale,
  messages,
  readLocale,
  resolveLocale,
  setLocale,
  t,
  writeLocale,
} from "./i18n/index.ts";
import type { MessageKey } from "./i18n/index.ts";
import { formatCost } from "./utils/format.ts";

test("locale matching uses stored preference, browser languages, and a stable fallback", () => {
  assert.equal(matchLocale("zh-Hant-HK"), "zh-TW");
  assert.equal(matchLocale("pt_PT"), "pt-BR");
  assert.equal(matchLocale("es-MX"), "es-ES");
  assert.equal(resolveLocale("ru_RU", ["en-US"]), "ru-RU");
  assert.equal(resolveLocale("unknown", ["fr-CA", "en-US"]), "fr-FR");
  assert.equal(resolveLocale(null, ["unknown"]), DEFAULT_LOCALE);
});

test("locale preference can be read and written without requiring browser storage", () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
  };

  writeLocale(storage, "ja-JP");
  assert.equal(values.get(LOCALE_STORAGE_KEY), "ja-JP");
  assert.equal(readLocale(storage, ["en-US"]), "ja-JP");
  assert.equal(readLocale({ getItem: () => { throw new Error("blocked"); } }, ["ko-KR"]), "ko-KR");
});

test("all locale catalogs have identical keys and placeholders", () => {
  const expectedKeys = (Object.keys(messages[DEFAULT_LOCALE]) as MessageKey[]).sort();
  const placeholders = (value: string) => [...value.matchAll(/\{\w+\}/g)].map(([token]) => token).sort();

  for (const { value } of LOCALE_OPTIONS) {
    assert.deepEqual(Object.keys(messages[value]).sort(), expectedKeys, value);
    for (const key of expectedKeys) {
      assert.deepEqual(placeholders(messages[value][key]), placeholders(key), `${value}: ${key}`);
    }
  }
});

test("translations react to locale changes and preserve interpolation", () => {
  setLocale("en-US");
  assert.equal(t("已复制 {label}", { label: "API Base URL" }), "Copied API Base URL");
  setLocale("zh-CN");
  assert.equal(t("已复制 {label}", { label: "Key" }), "已复制 Key");
});

test("USD costs use the narrow dollar symbol and preserve requested precision", () => {
  for (const { value } of LOCALE_OPTIONS) {
    setLocale(value);
    assert.match(formatCost(0.00015, 5), /\$/);
    assert.doesNotMatch(formatCost(0.00015, 5), /US/);
  }
  setLocale(DEFAULT_LOCALE);
  assert.match(formatCost(0.00015, 5), /0\.00015/);
});
