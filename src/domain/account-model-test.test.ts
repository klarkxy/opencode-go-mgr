import assert from "node:assert/strict";
import test from "node:test";

import type { Account } from "../api/dashboard.ts";
import type { ProviderContractsResponse } from "../api/providers.ts";
import { accountTestModels, filterAccountTestModels } from "./account-model-test.ts";

const account = {
  id: "account-1",
  provider_id: "provider-1",
} as Account;

const contracts = {
  revision: 1,
  providers: [{
    scope_kind: "provider",
    scope_id: "provider-1",
    provider_id: "provider-1",
    static_protocol_snapshot_date: null,
    accounts: [{
      id: "account-1",
      name: "Account One",
      enabled: true,
      verification_status: "not_required",
    }],
    catalog: { source: "test", source_url: "", refreshed_at: null, models: [], refresh_supported: false },
    models: [
      { alias: "Beta", model_id: "provider/beta", preferred_protocol: "messages", protocols: {}, routable: true, disabled_reasons: [] },
      { alias: "alpha", model_id: "provider/alpha", preferred_protocol: "chat_completions", protocols: {}, routable: true, disabled_reasons: [] },
      { alias: "off", model_id: "provider/off", preferred_protocol: "responses", protocols: {}, routable: false, disabled_reasons: ["off"] },
    ],
    pricing: { availability: "unpriced" },
    usage: { availability: "unavailable" },
    card: { fetch_zen_models: false, discover_models: false, protocol_probe: false, catalog_refresh: false },
    catalog_routable: true,
    production_inference: true,
    disabled_reasons: [],
    revision: 1,
  }],
  custom_endpoints: [],
} satisfies ProviderContractsResponse;

test("account tests use only routable models from the exact account scope", () => {
  assert.deepEqual(accountTestModels(account, contracts), [
    { modelId: "provider/alpha", alias: "alpha", protocol: "chat_completions" },
    { modelId: "provider/beta", alias: "Beta", protocol: "messages" },
  ]);
  assert.deepEqual(accountTestModels({ ...account, provider_id: "other" }, contracts), []);
});

test("account model filtering matches raw ids and aliases without changing order", () => {
  const models = accountTestModels(account, contracts);
  assert.deepEqual(filterAccountTestModels(models, "BETA"), [models[1]]);
  assert.deepEqual(filterAccountTestModels(models, "provider/a"), [models[0]]);
  assert.deepEqual(filterAccountTestModels(models, ""), models);
});
