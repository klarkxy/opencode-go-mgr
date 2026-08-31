import assert from "node:assert/strict";
import test from "node:test";
import {
  GOAT_PRICING_REFERENCE,
  PRICING_REFERENCE_CHECKED_AT,
} from "./pricing-references.ts";

test("GOAT reference mirrors the official plan summary and 40 included models", () => {
  assert.equal(PRICING_REFERENCE_CHECKED_AT, "2026-08-24");
  assert.equal(GOAT_PRICING_REFERENCE.includedModelCount, 40);
  assert.equal(GOAT_PRICING_REFERENCE.models.length, 40);
  assert.deepEqual(
    GOAT_PRICING_REFERENCE.models.find(({ model }) => model === "GPT-5.6 Sol"),
    {
      model: "GPT-5.6 Sol",
      input: 5,
      output: 30,
      cacheRead: 0.5,
      cacheWrite: 6.25,
      quotaMultiplier: 1,
    },
  );
  assert.equal(
    GOAT_PRICING_REFERENCE.models.find(({ model }) => model === "Gemini 3.7 Flash")
      ?.quotaMultiplier,
    1.75,
  );
  assert.deepEqual(
    GOAT_PRICING_REFERENCE.models.find(({ model }) => model === "Ox Alpha"),
    {
      model: "Ox Alpha",
      input: "free",
      output: "free",
      cacheRead: "free",
      cacheWrite: null,
      quotaMultiplier: null,
    },
  );
  assert.equal(
    GOAT_PRICING_REFERENCE.models.filter(({ quotaMultiplier }) => quotaMultiplier === null).length,
    2,
  );
  assert.equal(GOAT_PRICING_REFERENCE.models.some(({ model }) => model.startsWith("Claude")), false);
  assert.match(GOAT_PRICING_REFERENCE.sourceUrl, /commandcode\.ai\/docs\/plans\/goat$/);
  assert.match(GOAT_PRICING_REFERENCE.pricingUrl, /commandcode\.ai\/docs\/plans\/goat#models-included$/);
});
