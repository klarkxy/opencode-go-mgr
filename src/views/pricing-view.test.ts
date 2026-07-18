import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import type { PricingModel } from "../api/tauri.ts";
import {
  buildPricingTableRows,
  effectivePricingRate,
  formatPricingMultiplier,
  formatPricingRate,
} from "./pricing-view.ts";

const pricingLabels = {
  highspeed: "高速别名",
  minimaxM3Upper: "> 512K 输入",
  priorityService: "优先服务",
  minimaxM3UpperPriority: "> 512K 输入 + 优先服务",
};

function pricingModel(overrides: Partial<PricingModel>): PricingModel {
  return {
    model_id: "example-model",
    display_name: "Example Model",
    input: 1,
    output: 2,
    cache_read: 0.5,
    cache_write: 0.75,
    usage: 60,
    quota_multiplier: 1,
    min_input_tokens: null,
    max_input_tokens: null,
    adjustments: [],
    ...overrides,
  };
}

function assertRatesClose(actual: Array<number | null | undefined>, expected: number[]): void {
  assert.equal(actual.length, expected.length);
  actual.forEach((value, index) => {
    assert.ok(typeof value === "number" && Math.abs(value - (expected[index] ?? 0)) < 1e-12);
  });
}

test("pricing rates use two decimals and expose tiny exact values", () => {
  assert.deepEqual(formatPricingRate(null, "en-US"), { label: "—", exact: null });
  assert.deepEqual(formatPricingRate(0, "en-US"), { label: "$0.00", exact: null });
  assert.deepEqual(formatPricingRate(1.236, "en-US"), { label: "$1.24", exact: null });
  assert.deepEqual(formatPricingRate(0.003625, "en-US"), {
    label: "<$0.01",
    exact: "$0.003625",
  });
});

test("effective pricing retains nulls and applies the Go quota multiplier", () => {
  assert.equal(effectivePricingRate(null, 4), null);
  assert.equal(effectivePricingRate(0.5, 4), 2);
  assert.equal(formatPricingMultiplier(1.5), "×1.5");
});

test("pricing rows group duplicate tiers under stable unique keys", () => {
  const rows = buildPricingTableRows([
    pricingModel({
      model_id: "qwen3.7-plus",
      display_name: "Qwen3.7 Plus (> 256K tokens)",
      min_input_tokens: 256_001,
      input: 3,
      output: 6,
    }),
    pricingModel({
      model_id: "qwen3.7-plus",
      display_name: "Qwen3.7 Plus (≤ 256K tokens)",
      max_input_tokens: 256_000,
    }),
    pricingModel({ model_id: "glm-5.2", display_name: "GLM-5.2" }),
  ], pricingLabels);

  assert.equal(rows.length, 2);
  const qwen = rows[0];
  assert.equal(qwen?.kind, "group");
  assert.equal(qwen?.display_name, "Qwen3.7 Plus");
  assert.equal(qwen?.input, 1);
  assert.equal(qwen?.output, 2);
  assert.equal(qwen?.editable_multiplier, true);
  assert.deepEqual(qwen?.children?.map(({ display_name }) => display_name), ["> 256K tokens"]);
  assert.equal(qwen?.children?.[0]?.input, 3);
  assert.equal(qwen?.children?.[0]?.output, 6);
  assert.ok(qwen?.children?.every(({ model_id, editable_multiplier }) => (
    model_id === "qwen3.7-plus" && !editable_multiplier
  )));
  const keys = rows.flatMap((row) => [row.row_key, ...(row.children ?? []).map((child) => child.row_key)]);
  assert.equal(new Set(keys).size, keys.length);
});

test("MiniMax roots retain standard rates while every upgrade is materialized below", () => {
  const rows = buildPricingTableRows([
    pricingModel({
      model_id: "minimax-m3",
      display_name: "MiniMax M3",
      input: 0.3,
      output: 1.2,
      cache_read: 0.06,
      cache_write: null,
      adjustments: [
        { label: ">512K input", multiplier: 2, applies_to: "input,output,cache_read,cache_write" },
        { label: "priority service tier", multiplier: 1.5, applies_to: "input,output,cache_read,cache_write" },
        { label: ">512K + priority", multiplier: 3, applies_to: "input,output,cache_read,cache_write" },
      ],
    }),
    pricingModel({
      model_id: "minimax-m2.7",
      display_name: "MiniMax M2.7",
      input: 0.3,
      output: 1.2,
      cache_read: 0.06,
      cache_write: 0.375,
      adjustments: [{ label: "highspeed alias", multiplier: 2, applies_to: "input,output" }],
    }),
  ], pricingLabels);

  const m3 = rows[0];
  assert.equal(m3?.input, 0.3);
  assert.equal(m3?.output, 1.2);
  assert.deepEqual(m3?.children?.map(({ display_name }) => display_name), [
    "> 512K 输入",
    "优先服务",
    "> 512K 输入 + 优先服务",
  ]);
  assertRatesClose(m3?.children?.map(({ input }) => input) ?? [], [0.6, 0.45, 0.9]);
  assertRatesClose(m3?.children?.map(({ output }) => output) ?? [], [2.4, 1.8, 3.6]);
  assertRatesClose(m3?.children?.map(({ cache_read }) => cache_read) ?? [], [0.12, 0.09, 0.18]);
  assert.ok(m3?.children?.every(({ cache_write, editable_multiplier }) => (
    cache_write === null && !editable_multiplier
  )));

  const m27 = rows[1];
  assert.equal(m27?.input, 0.3);
  assert.equal(m27?.output, 1.2);
  assert.deepEqual(m27?.children?.map(({ display_name }) => display_name), ["高速别名"]);
  assert.equal(m27?.children?.[0]?.input, 0.6);
  assert.equal(m27?.children?.[0]?.output, 2.4);
  assert.equal(m27?.children?.[0]?.cache_read, 0.06);
  assert.equal(m27?.children?.[0]?.cache_write, 0.375);
});

test("pricing catalog keeps refresh explicit and exposes accessible grouped multiplier editing", () => {
  const catalog = readFileSync(new URL("../components/PricingCatalog.vue", import.meta.url), "utf8");
  const pricing = readFileSync(new URL("./Pricing.vue", import.meta.url), "utf8");
  const settings = readFileSync(new URL("./Settings.vue", import.meta.url), "utf8");
  const app = readFileSync(new URL("../App.vue", import.meta.url), "utf8");
  const menu = app.slice(app.indexOf("const menuOptions"), app.indexOf("const currentTitle"));
  assert.match(pricing, /<PricingCatalog \/>/);
  assert.doesNotMatch(settings, /PricingCatalog/);
  assert.match(app, /<Pricing v-if="activeKey === 'pricing'" \/>/);
  assert.match(app, /key: "pricing"/);
  assert.ok(menu.indexOf('key: "pricing"') < menu.indexOf('key: "apps"'));
  assert.match(catalog, /onMounted\(\(\) => void loadPricing\(\)\)/);
  assert.doesNotMatch(catalog, /onMounted\(\(\) => void performPricingRefresh\(\)\)/);
  assert.match(catalog, /@click="requestPricingRefresh"/);
  assert.match(catalog, /result\.error \|\| t\("价格表刷新失败，详见页面提示"\)/);
  assert.match(catalog, /message\.warning\(t\("价格表刷新失败，详见页面提示"\)\)/);
  assert.match(catalog, /trigger: "focus"/);
  assert.match(catalog, /class: "tiny-rate",[\s\S]*?tabindex: 0,[\s\S]*?"aria-label"/);
  assert.match(catalog, /:row-key="rowKey"/);
  assert.match(catalog, /"aria-expanded": rowExpanded\(row\)/);
  assert.match(catalog, /\.n-data-table-expand-trigger/);
  assert.doesNotMatch(catalog, /h\(NTag|pricing-group-row|pricing-variant-label/);
  assert.doesNotMatch(catalog, /renderAdjustments|title: t\("本地调整"\)|:row-class-name/);
  assert.doesNotMatch(catalog, /function renderEffectiveRates[\s\S]*?row\.kind === "group"/);
  assert.match(catalog, /updatePricingMultipliers\(active\.revision/);
  assert.match(catalog, /updateValueOnInput: true/);
  assert.match(catalog, /onUpdateValue:[\s\S]*updateMultiplierDraft\(row\.model_id/);
  assert.match(catalog, /disabled: disabled \|\| savingModelId\.value !== null \|\| refreshing\.value/);
  assert.match(catalog, /true,\s*!valid,/);
  assert.match(catalog, /function hasMultiplierDraft[\s\S]*?multiplierDrafts\.value\[modelId\] !== undefined/);
  assert.doesNotMatch(catalog, /hasOwnProperty\.call\(multiplierDrafts\.value/);
  assert.doesNotMatch(catalog, /precision: 4/);
  assert.match(catalog, /refresh_status === "needs_confirmation"/);
  assert.match(catalog, /expected_official_content_hash: expectedOfficialContentHash/);
  assert.match(catalog, /result\.official_content_hash/);
  assert.match(catalog, /async function reloadPricingAfterRevisionChange[\s\S]*?tauriApi\.getPricing\(\)/);
  assert.match(catalog, /detail\.includes\("pricing revision changed"\)[\s\S]*?reloadPricingAfterRevisionChange\(\)/);
  assert.match(catalog, /async function saveMultiplier[\s\S]*?reloadPricingAfterRevisionChange\(\)/);
  assert.match(catalog, /performPricingRefresh\("keep_current"|apply\("keep_current"\)/);
  assert.match(catalog, /performPricingRefresh\("use_official"|apply\("use_official"\)/);
  assert.match(catalog, /模型价格为 OpenCode Go 表中的美元\/百万 tokens；官方倍率用于换算额度消耗，可按活动手动调整。/);
  assert.doesNotMatch(catalog, /本地条件价格最后叠加/);
  assert.doesNotMatch(catalog, /official_price_multiplier|表价已含|Go 倍率/);
});
