import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  effectivePricingRate,
  formatPricingMultiplier,
  formatPricingRate,
  officialPriceMultiplier,
} from "./pricing-view.ts";

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

test("official price multiplier defaults to one and preserves included multipliers", () => {
  assert.equal(officialPriceMultiplier(undefined), 1);
  assert.equal(officialPriceMultiplier(null), 1);
  assert.equal(officialPriceMultiplier(0), 1);
  assert.equal(officialPriceMultiplier(4), 4);
});

test("pricing catalog refreshes only on explicit action and keeps exact-rate affordances", () => {
  const catalog = readFileSync(new URL("../components/PricingCatalog.vue", import.meta.url), "utf8");
  const pricing = readFileSync(new URL("./Pricing.vue", import.meta.url), "utf8");
  const settings = readFileSync(new URL("./Settings.vue", import.meta.url), "utf8");
  const app = readFileSync(new URL("../App.vue", import.meta.url), "utf8");
  assert.match(pricing, /<PricingCatalog \/>/);
  assert.doesNotMatch(settings, /PricingCatalog/);
  assert.match(app, /<Pricing v-if="activeKey === 'pricing'" \/>/);
  assert.match(app, /key: "pricing"/);
  assert.match(catalog, /onMounted\(\(\) => void loadPricing\(\)\)/);
  assert.doesNotMatch(catalog, /onMounted\(\(\) => void refreshPricing\(\)\)/);
  assert.match(catalog, /@click="refreshPricing"/);
  assert.match(catalog, /result\.error \?\? ""/);
  assert.match(catalog, /class: "tiny-rate", tabindex: 0/);
  assert.match(catalog, /row\.adjustments/);
  assert.match(catalog, /row\.official_price_multiplier/);
});
