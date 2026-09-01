import assert from "node:assert/strict";
import test from "node:test";
import type { Account } from "../api/dashboard.ts";
import type { ProviderScopeView } from "./provider-contracts.ts";
import { dynamicProviderAliasRows, providerAliasRows } from "./provider-aliases.ts";

const protocol = {
  protocol: "chat_completions" as const,
  available: true,
  enabled: true,
  source: "static" as const,
  verified_at: null,
  observed_at: null,
  last_probe_result: null,
  last_probe_at: null,
  last_probe_error: null,
  override: "auto" as const,
};

const builtinScope = {
  key: "provider:go",
  scope_kind: "provider",
  scope_id: "go",
  label: "OpenCode Go",
  models: [{
    alias: "gpt-5.6",
    model_id: "gpt-5.6-upstream",
    preferred_protocol: "chat_completions",
    protocols: { chat_completions: protocol },
    routable: true,
    disabled_reasons: [],
  }, {
    alias: "",
    model_id: "raw-only-model",
    preferred_protocol: "chat_completions",
    protocols: { chat_completions: protocol },
    routable: true,
    disabled_reasons: [],
  }],
} as unknown as ProviderScopeView;

const customScope = {
  key: "custom_endpoint:custom-1",
  scope_kind: "custom_endpoint",
  scope_id: "custom-1",
  label: "Home Lab",
  accounts: [{ id: "custom-1", name: "Home Lab", enabled: true, verification_status: "verified" }],
  models: [{
    alias: "",
    model_id: "public-model",
    preferred_protocol: "chat_completions",
    protocols: { chat_completions: protocol },
    routable: false,
    disabled_reasons: [],
  }],
} as unknown as ProviderScopeView;

const customAccount = {
  id: "custom-1",
  name: "Home Lab",
  provider_id: "custom",
  enabled: true,
  plan_routable: true,
  model_capabilities: [{
    public_model: "public-model",
    upstream_model: "vendor/model:free",
    protocol: "chat_completions",
    verified_at: null,
    source: "discovered",
  }],
} as Account;

test("Alias rows combine provider contracts with Custom public-to-upstream mappings", () => {
  assert.deepEqual(providerAliasRows([builtinScope, customScope], [customAccount]), [
    {
      key: "provider:go:gpt-5.6:gpt-5.6-upstream",
      public_model: "gpt-5.6",
      provider_plan: "OpenCode Go",
      custom_account: null,
      upstream_model: "gpt-5.6-upstream",
      routable: true,
      custom_account_id: null,
    },
    {
      key: "custom:custom-1:public-model:vendor/model:free",
      public_model: "public-model",
      provider_plan: "Home Lab",
      custom_account: "Home Lab",
      upstream_model: "vendor/model:free",
      routable: false,
      custom_account_id: "custom-1",
    },
  ]);
});

test("Custom Alias routeability includes account readiness and built-in raw conflicts", () => {
  const scope = {
    ...customScope,
    models: [{
      ...customScope.models[0],
      alias: "",
      model_id: "raw-only-model",
      routable: true,
    }],
  } as ProviderScopeView;
  const account = {
    ...customAccount,
    setup_step: "ready",
    model_capabilities: [{
      ...customAccount.model_capabilities[0],
      public_model: "raw-only-model",
      upstream_model: "vendor/mapped:latest",
    }],
  } as Account;
  const row = providerAliasRows([builtinScope, scope], [account]).at(-1);
  assert.equal(row?.public_model, "raw-only-model");
  assert.equal(row?.upstream_model, "vendor/mapped:latest");
  assert.equal(row?.routable, false);
});

test("user-defined Provider mappings appear as Alias rows labelled by Provider name", () => {
  assert.deepEqual(dynamicProviderAliasRows([{
    id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    name: "Lab",
    endpoint_url: "http://127.0.0.1:9",
    upstream_protocol: "chat_completions",
    auth_kind: "bearer",
    models: [{ public_model: "lab-opus", upstream_model: "vendor/opus" }],
    created_at: "",
    updated_at: "",
    revision: 1,
    process_generation: 1,
  }]), [{
    key: "dynamic:aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa:lab-opus:vendor/opus",
    public_model: "lab-opus",
    provider_plan: "Lab",
    custom_account: null,
    upstream_model: "vendor/opus",
    routable: true,
    custom_account_id: null,
  }]);
});
