import assert from "node:assert/strict";
import test from "node:test";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import {
  buildChooserGroups,
  chooserOptionIconKey,
  chooserSelectOptions,
  chooserUniverse,
  defaultChooserOptionId,
  describeChooserSelection,
  isValidChooserOption,
  visibleChooserOptions,
  type ChooserOption,
  type PresetChooserOption,
} from "./account-add-chooser.ts";
import { buildPlatformKindOptions } from "./platform-accounts.ts";
import { PROVIDER_PRESETS, providerPresetOffering } from "./provider-presets.ts";

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

function fullCatalog(): ProviderCatalogEntry[] {
  // Blank display names keep the static family labels (OpenCode Go, …).
  return [
    catalogEntry("opencode", { display_name: "", routable: true }),
    catalogEntry("command-code", { display_name: "", routable: true }),
    catalogEntry("minimax", { display_name: "", routable: true }),
    catalogEntry("kimi", { display_name: "", routable: true }),
    catalogEntry("ollama", { display_name: "", routable: true }),
    catalogEntry("custom", { display_name: "", routable: true }),
  ];
}

test("groups: Plan holds built-in subscriptions and plan presets; API heads Custom API and trails platform kinds", () => {
  const groups = buildChooserGroups(fullCatalog(), null, "");
  assert.deepEqual(groups.map((group) => group.id), ["plan", "api"]);

  const planIds = groups[0]!.options.map((option) => option.optionId);
  assert.deepEqual(planIds.slice(0, 5), [
    "opencode-go",
    "command-code-goat",
    "minimax-cn",
    "kimi-cn",
    "ollama-cloud",
  ]);
  const planPresetCount = PROVIDER_PRESETS.filter(
    (preset) => providerPresetOffering(preset) === "plan",
  ).length;
  assert.ok(planPresetCount > 0, "preset data must contain plan-offering presets");
  assert.equal(planIds.length, 5 + planPresetCount);
  assert.ok(planIds.slice(5).every((id) => id.startsWith("preset:")));

  const apiIds = groups[1]!.options.map((option) => option.optionId);
  assert.equal(apiIds[0], "custom-endpoint");
  assert.deepEqual(apiIds.slice(-2), ["platform:new_api", "platform:sub2api"]);
});

test("search filters the complete chooser without invalidating the saved selection", () => {
  const groups = buildChooserGroups(fullCatalog(), null, "azure");
  const planIds = groups[0]!.options.map((option) => option.optionId);
  assert.deepEqual(planIds, []);
  const apiIds = groups[1]!.options.map((option) => option.optionId);
  assert.deepEqual(apiIds, ["preset:azure-openai"]);

  const universe = chooserUniverse(fullCatalog(), null);
  assert.equal(isValidChooserOption(universe, "preset:deepseek"), true);
  assert.equal(isValidChooserOption(universe, "preset:nope"), false);
  for (const [query, expected] of [[" Custom ", "custom-endpoint"], ["new api", "platform:new_api"], ["Ollama", "ollama-cloud"]]) {
    assert.deepEqual(visibleChooserOptions(buildChooserGroups(fullCatalog(), null, query)).map((item) => item.optionId), [expected]);
  }
  assert.equal(visibleChooserOptions(buildChooserGroups(fullCatalog(), null, "no-such-preset")).length, 0);
});

test("saved user-defined Providers follow their persisted preset offering", () => {
  const planPreset = PROVIDER_PRESETS.find((preset) => providerPresetOffering(preset) === "plan")!;
  const catalog = [
    ...fullCatalog(),
    catalogEntry("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", {
      display_name: "Coding Plan",
      model_source: "dynamic_provider",
      routable: true,
    }),
    catalogEntry("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", {
      display_name: "Manual API",
      model_source: "dynamic_provider",
      routable: true,
    }),
  ];
  const presetIds = new Map<string, string | null>([
    ["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", planPreset.id],
  ]);
  const groups = buildChooserGroups(catalog, presetIds, "");
  assert.ok(groups[0]!.options.some((option) => (
    option.optionId === "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
  )));
  const apiIds = groups[1]!.options.map((option) => option.optionId);
  assert.equal(apiIds[0], "custom-endpoint");
  assert.ok(apiIds.includes("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"));
  assert.equal(apiIds.includes("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"), false);
});

test("default selection skips disabled options and falls back to the first option", () => {
  // A failed catalog disables everything except the legacy OpenCode Go import.
  assert.equal(defaultChooserOptionId(chooserUniverse(null, null)), "opencode-go");
  assert.equal(defaultChooserOptionId(chooserUniverse(fullCatalog(), null)), "opencode-go");
  assert.equal(defaultChooserOptionId([]), "");
});

test("visible options follow rail order across both groups", () => {
  const groups = buildChooserGroups(fullCatalog(), null, "");
  const visible = visibleChooserOptions(groups);
  assert.deepEqual(visible.map((option) => option.optionId), [
    ...groups[0]!.options.map((option) => option.optionId),
    ...groups[1]!.options.map((option) => option.optionId),
  ]);
});

test("mobile select groups mirror the rail and mark user-defined entries", () => {
  const catalog = [
    ...fullCatalog(),
    catalogEntry("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", {
      display_name: "Lab",
      model_source: "dynamic_provider",
      routable: true,
    }),
  ];
  const groups = buildChooserGroups(catalog, null, "");
  const select = chooserSelectOptions(groups, "用户定义");
  assert.deepEqual(select.map((group) => group.key), ["plan", "api"]);
  const lab = select
    .flatMap((group) => group.children)
    .find((child) => child.value === "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!;
  assert.equal(lab.label, "Lab · 用户定义");
});

test("describeChooserSelection covers plan, preset, and platform details", () => {
  const universe = chooserUniverse(fullCatalog(), null);
  const byId = (id: string): ChooserOption => (
    universe.find((option) => option.optionId === id)!
  );

  const go = describeChooserSelection(byId("opencode-go"));
  assert.deepEqual(go, {
    kind: "plan",
    iconKey: "opencode-go",
    title: "OpenCode Go",
    tag: null,
    links: null,
  });

  const custom = describeChooserSelection(byId("custom-endpoint"));
  assert.equal(custom.kind, "plan");
  assert.deepEqual(custom.tag, { label: "自定义端点", type: "default" });

  const preset = describeChooserSelection(byId("preset:azure-openai"));
  assert.equal(preset.kind, "preset");
  assert.equal(preset.iconKey, "api");
  assert.equal(preset.title, "Azure OpenAI v1");
  assert.deepEqual(preset.tag, { label: "供应商预设", type: "default" });
  assert.equal(preset.links!.docsUrl.startsWith("https://"), true);
  assert.equal(preset.links!.websiteUrl.startsWith("https://"), true);

  const platform = describeChooserSelection(byId("platform:new_api"));
  assert.deepEqual(platform, {
    kind: "platform",
    iconKey: "database",
    title: "New API",
    tag: null,
    links: null,
  });
});

test("user-defined plan options carry the 用户定义 tag and preset icon keys", () => {
  const catalog = [
    ...fullCatalog(),
    catalogEntry("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", {
      display_name: "Lab",
      model_source: "dynamic_provider",
      routable: true,
    }),
  ];
  const universe = chooserUniverse(catalog, null);
  const lab = universe.find((option) => option.optionId === "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!;
  const detail = describeChooserSelection(lab);
  assert.equal(detail.kind, "plan");
  assert.deepEqual(detail.tag, { label: "用户定义", type: "default" });
  assert.equal(detail.title, "Lab");

  const preset = universe.find((option) => option.optionId === "preset:openai")! as PresetChooserOption;
  assert.equal(chooserOptionIconKey(preset), "api");
  assert.equal(chooserOptionIconKey(buildPlatformKindOptions()[0]!), "database");
  assert.equal(chooserOptionIconKey(lab), "dynamic-http");
});
