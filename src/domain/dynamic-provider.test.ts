import assert from "node:assert/strict";
import test from "node:test";
import {
  DYNAMIC_PAID_TEST_WARNING_KEY,
  buildDynamicProviderCreateBody,
  buildDynamicProviderUpdateBody,
  dynamicAuthRequiresKey,
  dynamicProviderActionNeedsConfirm,
  dynamicProviderFormEnterAction,
  emptyDynamicProviderDraft,
  isDynamicCatalogEntry,
  normalizeDynamicMappings,
  omitSecretFromRecord,
  providerSourceLabel,
  sanitizeDynamicProviderDraft,
  validateDynamicProviderDraft,
} from "./dynamic-provider.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";

function entry(extra: Partial<ProviderCatalogEntry> = {}): ProviderCatalogEntry {
  return {
    provider_id: "opencode",
    display_name: "OpenCode Go",
    display_family: "OpenCode",
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    creation_availability: "available",
    verification_policy: "not_required",
    verification_runtime_availability: "not_applicable",
    routable: true,
    managed_registration: false,
    pricing_availability: "available",
    usage_availability: "available",
    manual_usage_calibration: false,
    quota_unit: "usd",
    model_source: "opencode_get_models",
    auth_schemes: ["bearer"],
    upstream_protocols: ["chat_completions"],
    form_fields: [],
    model_aliases: [],
    ...extra,
  };
}

test("source labels distinguish built-in catalog rows from user-defined Providers", () => {
  assert.equal(providerSourceLabel(entry()), "builtin");
  assert.equal(providerSourceLabel(entry({ model_source: "dynamic_provider" })), "user-defined");
  assert.equal(isDynamicCatalogEntry(entry({ model_source: "dynamic_provider" })), true);
});

test("mapping validation requires one unique public model and allows repeated upstream IDs", () => {
  assert.equal(normalizeDynamicMappings([]), "missing_mappings");
  assert.equal(
    normalizeDynamicMappings([
      { public_model: "Opus", upstream_model: "a" },
      { public_model: "opus", upstream_model: "b" },
    ]),
    "duplicate_public_model",
  );
  assert.deepEqual(
    normalizeDynamicMappings([
      { public_model: "opus", upstream_model: "vendor/a" },
      { public_model: "sonnet", upstream_model: "vendor/a" },
    ]),
    [
      { public_model: "opus", upstream_model: "vendor/a" },
      { public_model: "sonnet", upstream_model: "vendor/a" },
    ],
  );
});

test("create payload requires a Key only when auth is not none and never keeps blank mappings", () => {
  const draft = emptyDynamicProviderDraft();
  draft.name = "Lab";
  draft.endpoint_url = "http://127.0.0.1:9";
  draft.models = [{ public_model: "lab-opus", upstream_model: "vendor/opus" }];
  assert.equal(validateDynamicProviderDraft(draft, { mode: "create" }), "missing_key");
  draft.key = "sk-lab";
  const body = buildDynamicProviderCreateBody(draft);
  assert.equal(body.key, "sk-lab");
  assert.equal(body.authKind, "bearer");
  draft.auth_kind = "none";
  draft.key = "should-not-send";
  const noneBody = buildDynamicProviderCreateBody(draft);
  assert.equal(noneBody.key, undefined);
  assert.ok(dynamicAuthRequiresKey("bearer"));
  assert.equal(dynamicAuthRequiresKey("none"), false);
});

test("edit from none to keyed requires an explicit replacement Key", () => {
  const draft = emptyDynamicProviderDraft();
  draft.name = "Lab";
  draft.endpoint_url = "http://127.0.0.1:9";
  draft.auth_kind = "bearer";
  draft.models = [{ public_model: "lab-opus", upstream_model: "vendor/opus" }];
  assert.equal(
    validateDynamicProviderDraft(draft, { mode: "edit", previousAuthKind: "none" }),
    "missing_replacement_key",
  );
  draft.key = "sk-now";
  const body = buildDynamicProviderUpdateBody(draft, "none");
  assert.equal(body.key, "sk-now");
});

test("sanitization drops the write-only Key from draft and response-shaped records", () => {
  const draft = emptyDynamicProviderDraft();
  draft.key = "sk-secret";
  assert.equal(sanitizeDynamicProviderDraft(draft).key, "");
  assert.deepEqual(omitSecretFromRecord({ name: "Lab", key: "sk-secret", apiKey: "x" }), { name: "Lab" });
});

test("save does not require discovery or a prior model test", () => {
  const draft = emptyDynamicProviderDraft();
  draft.name = "Lab";
  draft.endpoint_url = "http://127.0.0.1:9";
  draft.auth_kind = "none";
  draft.models = [{ public_model: "lab-opus", upstream_model: "vendor/opus" }];
  assert.equal(validateDynamicProviderDraft(draft, { mode: "create" }), null);
  const body = buildDynamicProviderCreateBody(draft);
  assert.equal(body.key, undefined);
  assert.equal(body.models.length, 1);
});

test("paid tests and deletes require confirmation; Enter submits save", () => {
  assert.equal(dynamicProviderFormEnterAction(), "save");
  assert.equal(dynamicProviderActionNeedsConfirm("test"), true);
  assert.equal(dynamicProviderActionNeedsConfirm("delete"), true);
  assert.equal(dynamicProviderActionNeedsConfirm("save"), false);
  assert.equal(dynamicProviderActionNeedsConfirm("discover"), false);
  assert.equal(DYNAMIC_PAID_TEST_WARNING_KEY, "真实测试会消耗上游额度或产生费用。确定继续？");
});
