import assert from "node:assert/strict";
import test from "node:test";
import type { PricingSnapshot } from "../api/dashboard.ts";
import type { StoredProviderPricingSnapshot } from "../api/providers.ts";
import {
  buildScopedPlanPricingGroups,
  resolvePlanPricingDisplay,
  type PlanPricingContent,
  type PlanPricingGroup,
  type PricingAvailability,
} from "./pricing-plans.ts";
import { PLAN_DEFINITIONS, type PlanId } from "./plans.ts";

const goSnapshot = (models: PricingSnapshot["models"] = []): PricingSnapshot => ({
  revision: "go-r1",
  activated_at: "2026-08-21T00:00:00Z",
  document_updated_at: null,
  source_url: "https://opencode.ai/docs/go/",
  content_hash: "hash",
  adjustment_policy_version: "v1",
  limits: { window_5h: 1, window_week: 2, window_month: 3 },
  models,
});

const providerSnapshot: StoredProviderPricingSnapshot = {
  provider_id: "provider",
  revision: "r1",
  activated_at: "2026-08-21T00:00:00Z",
  document_updated_at: null,
  source_url: "https://example.com/pricing",
  content_hash: "hash",
  snapshot_json: "{}",
};

function group(
  planId: PlanId,
  pricingAvailability: PricingAvailability,
  content: PlanPricingContent,
): PlanPricingGroup {
  const plan = PLAN_DEFINITIONS.find(({ id }) => id === planId)!;
  return { plan, label: plan.label, pricingAvailability, content };
}

test("pricing state machine makes error and every catalog availability reachable", () => {
  const apiEmpty = group("command-code-goat", "available", { kind: "api-key", snapshot: null });
  assert.deepEqual(resolvePlanPricingDisplay(apiEmpty, "offline"), {
    state: "error",
    messageKey: "加载额度价格表失败: {error}",
    error: "offline",
  });
  assert.equal(
    resolvePlanPricingDisplay(group("command-code-goat", "unavailable", { kind: "api-key", snapshot: null })).state,
    "unavailable",
  );
  assert.equal(
    resolvePlanPricingDisplay(group("custom-endpoint", "unpriced", { kind: "custom", snapshot: null })).state,
    "unpriced",
  );
  assert.equal(
    resolvePlanPricingDisplay(group("zen-free", "not_applicable", { kind: "free", snapshot: null })).state,
    "not_applicable",
  );
  assert.equal(resolvePlanPricingDisplay(apiEmpty).state, "available-empty");
  assert.equal(
    resolvePlanPricingDisplay(group("command-code-goat", "available", {
      kind: "api-key",
      snapshot: providerSnapshot,
    })).state,
    "available-table",
  );
});

test("pricing copy is kind-aware and never turns missing data into a price table", () => {
  assert.equal(
    resolvePlanPricingDisplay(group("command-code-goat", "unavailable", { kind: "api-key", snapshot: null })).messageKey,
    "实验性接入，尚未配置价格目录，不展示价格表。",
  );
  assert.equal(
    resolvePlanPricingDisplay(group("custom-endpoint", "unpriced", { kind: "custom", snapshot: null })).messageKey,
    "自定义端点由你自行维护，Gateway 无法验证其价格、额度与协议兼容性。",
  );
  assert.equal(
    resolvePlanPricingDisplay(group("zen-free", "not_applicable", { kind: "free", snapshot: null })).messageKey,
    "零价格；额度按出口 IP 共享，429 后整条 free 通道冷却。",
  );
  assert.equal(
    resolvePlanPricingDisplay(group("opencode-go", "available", {
      kind: "opencode-go",
      snapshot: goSnapshot(),
    })).state,
    "available-empty",
  );
  assert.equal(
    resolvePlanPricingDisplay(group("opencode-go", "available", {
      kind: "opencode-go",
      snapshot: goSnapshot([{
        model_id: "model",
        display_name: "Model",
        input: 1,
        output: 2,
        cache_read: null,
        cache_write: null,
        usage: 3,
        quota_multiplier: 1,
        adjustments: [],
      }]),
    })).messageKey,
    "只在你主动刷新时访问官方文档；刷新失败会继续使用当前快照。",
  );
});

test("scoped pricing groups stay on one provider and include Zen Free or Custom when asked", () => {
  const zen = buildScopedPlanPricingGroups("opencode-zen-free", null, goSnapshot(), {});
  assert.deepEqual(zen.map(({ plan }) => plan.id), ["zen-free"]);
  assert.equal(zen[0]?.pricingAvailability, "not_applicable");
  const custom = buildScopedPlanPricingGroups("custom", null, goSnapshot(), {});
  assert.deepEqual(custom.map(({ plan }) => plan.id), ["custom-endpoint"]);
  assert.equal(custom[0]?.pricingAvailability, "unpriced");
  const go = buildScopedPlanPricingGroups("opencode", null, goSnapshot(), {});
  assert.deepEqual(go.map(({ plan }) => plan.id), ["opencode-go"]);
});

test("GOAT exposes provider pricing for local quota estimates", () => {
  const goat = buildScopedPlanPricingGroups("command-code", null, goSnapshot(), {})[0]!;

  assert.equal(goat.pricingAvailability, "available");
  assert.equal(goat.content.kind, "goat-reference");
  assert.deepEqual(resolvePlanPricingDisplay(goat), {
    state: "reference",
    messageKey: "未知价格不会参与费用估算",
    error: null,
  });
});
