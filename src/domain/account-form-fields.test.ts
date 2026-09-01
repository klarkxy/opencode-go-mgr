import assert from "node:assert/strict";
import test from "node:test";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import {
  accountFormFieldIsImmutable,
  resolveAccountFormFields,
} from "./account-form-fields.ts";
import { OPENCODE_GO_PLAN, type PlanDefinition } from "./plans.ts";

function goCatalog(form_fields: ProviderCatalogEntry["form_fields"]): ProviderCatalogEntry {
  return {
    provider_id: "opencode",
    display_name: "OpenCode Go",
    display_family: "OpenCode",
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    creation_availability: "available",
    verification_policy: "not_required",
    verification_runtime_availability: "optional",
    routable: true,
    managed_registration: true,
    pricing_availability: "available",
    usage_availability: "available",
    manual_usage_calibration: false,
    quota_unit: "usd",
    model_source: "builtin",
    auth_schemes: ["bearer"],
    upstream_protocols: ["chat_completions", "responses", "messages"],
    form_fields,
    model_aliases: ["gpt-5.6-luna"],
  };
}

test("OpenCode Go import always keeps a required Key field across catalog states", () => {
  for (const entry of [
    undefined,
    goCatalog([]),
    goCatalog([{ id: "name", kind: "text", required: true, immutable_after_create: false }]),
    goCatalog([
      { id: "name", kind: "text", required: true, immutable_after_create: false },
      { id: "key", kind: "secret", required: true, immutable_after_create: false },
    ]),
  ]) {
    const fields = resolveAccountFormFields(OPENCODE_GO_PLAN, entry);
    assert.ok(fields.some(({ id, required }) => id === "key" && required), JSON.stringify(entry));
    assert.ok(fields.some(({ id, required }) => id === "name" && required), JSON.stringify(entry));
  }
});

test("non-legacy plans never invent fields when their catalog entry is absent", () => {
  const custom = { ...OPENCODE_GO_PLAN, id: "custom-endpoint", legacy: false } as const;
  assert.deepEqual(resolveAccountFormFields(custom, undefined), []);
});

test("dynamic Provider account fields are name/Key/notes and never Endpoint or mappings", () => {
  const plan: PlanDefinition = {
    ...OPENCODE_GO_PLAN,
    id: "dynamic-http",
    provider_id: "11111111-1111-4111-8111-111111111111",
    legacy: false,
  };
  const fields = resolveAccountFormFields(plan, undefined);
  assert.deepEqual(fields.map((field) => field.id), ["name", "key", "notes"]);
  const nonePlan = { ...plan, credential_kind: "none" as const };
  assert.deepEqual(resolveAccountFormFields(nonePlan, undefined).map((field) => field.id), ["name", "notes"]);
});

test("catalog immutable fields lock only while editing", () => {
  const field = {
    id: "upstream_protocol",
    kind: "select",
    required: true,
    immutable_after_create: true,
  } as const;
  assert.equal(accountFormFieldIsImmutable(field, true), true);
  assert.equal(accountFormFieldIsImmutable(field, false), false);
  assert.equal(accountFormFieldIsImmutable({ ...field, immutable_after_create: false }, true), false);
  assert.equal(accountFormFieldIsImmutable(undefined, true), false);
});
