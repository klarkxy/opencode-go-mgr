import assert from "node:assert/strict";
import test from "node:test";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import { buildPlanChooserGroups, buildPlanOptions } from "./account-plan-options.ts";

function catalogEntry(
  provider_id: string,
  extra: Partial<ProviderCatalogEntry> = {},
): ProviderCatalogEntry {
  return {
    provider_id,
    display_name: provider_id,
    display_family: provider_id,
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    creation_availability: "available",
    verification_policy: "required",
    verification_runtime_availability: "unavailable",
    routable: false,
    managed_registration: false,
    pricing_availability: "unavailable",
    usage_availability: "unavailable",
    manual_usage_calibration: false,
    quota_unit: "credits",
    model_source: "test",
    auth_schemes: ["bearer"],
    upstream_protocols: ["chat_completions"],
    form_fields: [],
    model_aliases: [],
    ...extra,
  };
}

test("empty or failed catalogs keep the explicit OpenCode Go import option", () => {
  for (const catalog of [null, undefined, []] as const) {
    const options = buildPlanOptions(catalog);
    const go = options.find(({ plan }) => plan.id === "opencode-go")!;
    assert.equal(go.disabled, false);
    assert.equal(go.managed, true);
    assert.equal(go.label, "OpenCode Go");
    assert.equal(options.some(({ plan }) => plan.id === "zen-free"), false);
  }
});

test("add-account chooser omits singleton Zen Free and groups remaining families", () => {
  const catalog = [
    catalogEntry("opencode", {
      display_name: "OpenCode Go Catalog",
      routable: true,
      creation_availability: "available",
    }),
    catalogEntry("command-code", { routable: false, creation_availability: "available" }),
    catalogEntry("custom", { routable: true, creation_availability: "available" }),
  ];
  const options = buildPlanOptions(catalog);
  assert.equal(
    options.find(({ plan }) => plan.id === "custom-endpoint")?.creationHint,
    "创建后默认启用；可随时通过账号卡片测试连接。",
  );
  assert.deepEqual(options.map(({ plan }) => plan.id), [
    "opencode-go",
    "command-code-goat",
    "minimax-cn",
    "kimi-cn",
    "custom-endpoint",
  ]);
  assert.deepEqual(
    buildPlanChooserGroups(catalog).map((group) => [group.id, group.options.map(({ plan }) => plan.id)]),
    [
      ["available", ["opencode-go", "custom-endpoint"]],
      ["draft", ["command-code-goat"]],
      ["unavailable", ["minimax-cn", "kimi-cn"]],
    ],
  );
});

test("user-defined Providers appear in Add Account and none-auth stays a singleton", () => {
  const catalog = [
    catalogEntry("opencode", { routable: true, creation_availability: "available" }),
    catalogEntry("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", {
      display_name: "Lab",
      model_source: "dynamic_provider",
      routable: true,
      creation_availability: "available",
      singleton: false,
    }),
    catalogEntry("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", {
      display_name: "Open",
      model_source: "dynamic_provider",
      credential_kind: "none",
      singleton: true,
      creation_availability: "unavailable",
    }),
  ];
  const options = buildPlanOptions(catalog);
  const lab = options.find((option) => option.optionId === "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!;
  const open = options.find((option) => option.optionId === "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")!;
  assert.equal(lab.source, "user-defined");
  assert.equal(lab.disabled, false);
  assert.equal(lab.plan.id, "dynamic-http");
  assert.equal(open.disabled, true);
  assert.equal(open.disabledReason, "无鉴权供应商只能有一个账号。");
});

test("GOAT follows the catalog without inventing a Key-verification gate", () => {
  const routable = buildPlanChooserGroups([
    catalogEntry("command-code", { routable: true, creation_availability: "available" }),
  ]);
  const available = routable.find((group) => group.id === "available")!;
  const goat = available.options.find(({ plan }) => plan.id === "command-code-goat")!;
  assert.equal(goat.disabled, false);
  assert.equal(goat.creationHint, "");

  const draft = buildPlanChooserGroups([
    catalogEntry("command-code", { routable: false, creation_availability: "available" }),
  ]);
  const draftGoat = draft
    .find((group) => group.id === "draft")!
    .options.find(({ plan }) => plan.id === "command-code-goat")!;
  assert.equal(draftGoat.disabled, false);
  assert.equal(draftGoat.creationHint, "");
});

test("plan hints and disabled reasons are translation keys", () => {
  const catalog = [
    catalogEntry("opencode", { display_name: "OpenCode Go Catalog" }),
  ];
  const options = buildPlanOptions(catalog);
  const go = options.find(({ plan }) => plan.id === "opencode-go")!;
  const custom = options.find(({ plan }) => plan.id === "custom-endpoint")!;

  assert.equal(go.label, "OpenCode Go Catalog");
  assert.equal(custom.disabledReason, "服务商目录未提供该方案");

  const unavailable = buildPlanOptions([
    catalogEntry("command-code", {
      creation_availability: "unavailable",
      creation_unavailable_reason: "Raw backend English must not leak",
    }),
  ]).find(({ plan }) => plan.id === "command-code-goat")!;
  assert.equal(unavailable.disabledReason, "该方案暂不可用");
});
