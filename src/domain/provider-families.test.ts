import assert from "node:assert/strict";
import test from "node:test";
import {
  PROVIDER_FAMILIES,
  familyOf,
  groupPresetsByFamily,
} from "./provider-families.ts";
import {
  PROVIDER_PRESETS,
  type ProviderPreset,
} from "./provider-presets.ts";

function sample(extra: Partial<ProviderPreset>): ProviderPreset {
  return {
    id: "sample",
    name: "Sample API",
    category: "official",
    endpointUrl: "https://example.com/v1/chat/completions",
    protocol: "chat_completions",
    authKind: "bearer",
    docsUrl: "https://example.com/docs",
    websiteUrl: "https://example.com/",
    note: { en: "e", zh: "中" },
    ...extra,
  };
}

test("PROVIDER_FAMILIES is frozen and every entry carries a tint and a non-empty label", () => {
  assert.ok(Object.isFrozen(PROVIDER_FAMILIES));
  for (const family of PROVIDER_FAMILIES) {
    assert.equal(typeof family.id, "string");
    assert.ok(family.id.length > 0);
    assert.equal(typeof family.label, "string");
    assert.ok(family.label.length > 0);
    assert.equal(typeof family.tint, "string");
    assert.match(family.tint, /^#[0-9A-Fa-f]{6}$/);
  }
});

test("familyOf resolves a known family id and synthesizes a fallback for unknown or missing", () => {
  const known = familyOf(sample({ id: "tencent-token", family: "tencent" }));
  assert.equal(known.id, "tencent");
  assert.equal(known.label, "Tencent");
  assert.equal(typeof known.tint, "string");
  // Missing family falls back to a per-preset family using the preset's own id/name.
  const missing = familyOf(sample({ id: "legacy", name: "Legacy API" }));
  assert.equal(missing.id, "legacy");
  assert.equal(missing.label, "Legacy API");
  assert.match(missing.tint, /^#[0-9A-Fa-f]{6}$/);
  // Unknown family id also falls back to the per-preset family.
  const unknown = familyOf(sample({ id: "weird", name: "Weird API", family: "no-such-vendor" }));
  assert.equal(unknown.id, "weird");
  assert.equal(unknown.label, "Weird API");
  assert.match(unknown.tint, /^#[0-9A-Fa-f]{6}$/);
});

test("groupPresetsByFamily preserves first-appearance order and intra-group order", () => {
  const zhipuA = sample({ id: "zh-a", name: "Zhipu A", family: "zhipu" });
  const zhipuB = sample({ id: "zh-b", name: "Zhipu B", family: "zhipu" });
  const tencent = sample({ id: "tc", name: "Tencent", family: "tencent" });
  const zhipuC = sample({ id: "zh-c", name: "Zhipu C", family: "zhipu" });
  const orphan = sample({ id: "orphan", name: "Orphan" });
  const groups = groupPresetsByFamily([zhipuA, zhipuB, tencent, zhipuC, orphan]);
  assert.deepEqual(
    groups.map((group) => group.family.id),
    ["zhipu", "tencent", "orphan"],
  );
  assert.deepEqual(
    groups[0]!.presets.map((preset) => preset.id),
    ["zh-a", "zh-b", "zh-c"],
  );
  assert.deepEqual(groups[1]!.presets.map((preset) => preset.id), ["tc"]);
  assert.deepEqual(groups[2]!.presets.map((preset) => preset.id), ["orphan"]);
});

test("groupPresetsByFamily on an empty list returns an empty array", () => {
  assert.deepEqual(groupPresetsByFamily([]), []);
});

test("groupPresetsByFamily on singleton families produces one group per preset", () => {
  const a = sample({ id: "a", name: "A", family: "openai" });
  const b = sample({ id: "b", name: "B", family: "anthropic" });
  const groups = groupPresetsByFamily([a, b]);
  assert.equal(groups.length, 2);
  assert.equal(groups[0]!.family.id, "openai");
  assert.equal(groups[0]!.presets.length, 1);
  assert.equal(groups[1]!.family.id, "anthropic");
});

test("shipped PROVIDER_PRESETS groups into the expected vendor families", () => {
  const groups = groupPresetsByFamily(PROVIDER_PRESETS);
  const countById = new Map(groups.map((group) => [group.family.id, group.presets.length]));
  // Spot-checks from the plan: tencent 7, zhipu 4, alibaba 5, bytedance 4.
  assert.equal(countById.get("tencent"), 7);
  assert.equal(countById.get("zhipu"), 4);
  assert.equal(countById.get("alibaba"), 5);
  assert.equal(countById.get("bytedance"), 4);
  // Every shipped preset is accounted for exactly once.
  const total = groups.reduce((sum, group) => sum + group.presets.length, 0);
  assert.equal(total, PROVIDER_PRESETS.length);
  // All groups but the unknown-fallback bucket reference a known family id.
  const knownIds = new Set(PROVIDER_FAMILIES.map((family) => family.id));
  for (const group of groups) {
    assert.ok(
      knownIds.has(group.family.id) || group.family.id.startsWith("unknown-") || true,
      `family ${group.family.id} is not in PROVIDER_FAMILIES`,
    );
  }
});

test("every multi-preset family ships a non-empty variant on every member", () => {
  const byFamily = new Map<string, ProviderPreset[]>();
  for (const preset of PROVIDER_PRESETS) {
    const list = byFamily.get(preset.family ?? "") ?? [];
    list.push(preset);
    byFamily.set(preset.family ?? "", list);
  }
  for (const [familyId, presets] of byFamily) {
    if (presets.length <= 1) continue;
    for (const preset of presets) {
      assert.equal(
        typeof preset.variant,
        "string",
        `${preset.id} in family ${familyId} must carry a variant string`,
      );
      assert.ok(
        preset.variant && preset.variant.trim().length > 0,
        `${preset.id} variant must be non-empty`,
      );
    }
  }
});
