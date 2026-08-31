import assert from "node:assert/strict";
import test from "node:test";
import type { Account } from "../api/dashboard.ts";
import type {
  CustomEndpointContract,
  ProviderCatalogEntry,
  ProviderContractGroup,
  ProviderContractsResponse,
  ProviderProtocol,
} from "../api/providers.ts";
import {
  accountContractSummary,
  applyModelContractToResponse,
  catalogRefreshSupported,
  enabledProtocols,
  findAccountScopeView,
  flattenProviderScopes,
  isSafeSourceUrl,
  normalizeProviderContractsResponse,
  parseProviderScopeKey,
  protocolDisplayName,
  protocolEvidenceStatus,
  protocolProbeSupported,
  providerScopeKey,
  selectProviderScope,
  uniqueProtocols,
  type ProviderModelContract,
} from "./provider-contracts.ts";

const catalogEntry = (
  provider_id: string,
  offering_id: string,
  display_name: string,
): ProviderCatalogEntry => ({
  provider_id,
  offering_id,
  display_name,
  display_family: provider_id,
  credential_kind: "api_key",
  quota_scope: "key",
  singleton: false,
  creation_availability: "available",
  verification_policy: "not_required",
  verification_runtime_availability: "optional",
  routable: true,
  managed_registration: false,
  pricing_availability: "available",
  usage_availability: "available",
  manual_usage_calibration: false,
  quota_unit: "usd",
  model_source: "test",
  auth_schemes: ["bearer"],
  upstream_protocols: ["chat_completions"],
  form_fields: [],
  model_aliases: [],
});

function modelContract(
  modelId: string,
  enabled: Partial<Record<ProviderProtocol, boolean>> = {},
  alias = "",
): ProviderModelContract {
  const protocols = {
    chat_completions: {
      protocol: "chat_completions" as const,
      available: true,
      enabled: enabled.chat_completions ?? false,
      source: "static" as const,
      verified_at: null,
      observed_at: null,
      last_probe_result: null,
      last_probe_at: null,
      last_probe_error: null,
      override: "auto" as const,
    },
    responses: {
      protocol: "responses" as const,
      available: true,
      enabled: enabled.responses ?? false,
      source: "preset" as const,
      verified_at: null,
      observed_at: null,
      last_probe_result: null,
      last_probe_at: null,
      last_probe_error: null,
      override: "auto" as const,
    },
    messages: {
      protocol: "messages" as const,
      available: false,
      enabled: enabled.messages ?? false,
      source: "static" as const,
      verified_at: null,
      observed_at: null,
      last_probe_result: null,
      last_probe_at: null,
      last_probe_error: null,
      override: "auto" as const,
    },
  };
  return {
    alias,
    model_id: modelId,
    preferred_protocol: "responses",
    protocols,
    routable: Object.values(enabled).some(Boolean),
    disabled_reasons: [],
  };
}

function providerGroup(overrides: Partial<ProviderContractGroup> = {}): ProviderContractGroup {
  return {
    scope_kind: "provider",
    scope_id: "opencode",
    provider_id: "opencode",
    static_protocol_snapshot_date: "2026-08-14",
    offerings: [{
      offering_id: "go",
      display_name: "OpenCode Go",
      routable: true,
      accounts: [{ id: "go-1", name: "Go 1", enabled: true, verification_status: "not_required" }],
    }],
    catalog: {
      source: "static",
      source_url: "https://opencode.ai/docs/go/",
      refreshed_at: null,
      models: ["gpt-5.6-luna"],
      refresh_supported: true,
    },
    models: [modelContract("gpt-5.6-luna", { chat_completions: true, responses: true })],
    pricing: { availability: "available" },
    usage: { availability: "available" },
    card: {
      fetch_zen_models: false,
      discover_models: false,
      protocol_probe: true,
      catalog_refresh: true,
    },
    catalog_routable: true,
    production_inference: true,
    disabled_reasons: [],
    revision: 3,
    ...overrides,
  };
}

function customEndpoint(overrides: Partial<CustomEndpointContract> = {}): CustomEndpointContract {
  return {
    scope_kind: "custom_endpoint",
    scope_id: "custom-1",
    provider_id: "custom",
    account: { id: "custom-1", name: "Home Lab", enabled: true, verification_status: "verified" },
    catalog: {
      source: "account_declared",
      source_url: "",
      refreshed_at: null,
      models: ["local-model"],
      refresh_supported: false,
    },
    models: [modelContract("local-model", { chat_completions: true })],
    pricing: { availability: "unpriced" },
    usage: { availability: "unavailable" },
    card: {
      fetch_zen_models: false,
      discover_models: true,
      protocol_probe: true,
      catalog_refresh: false,
    },
    catalog_routable: true,
    production_inference: true,
    disabled_reasons: [],
    revision: 2,
    ...overrides,
  };
}

function contracts(overrides: Partial<ProviderContractsResponse> = {}): ProviderContractsResponse {
  return {
    revision: 11,
    providers: [providerGroup()],
    custom_endpoints: [customEndpoint()],
    ...overrides,
  };
}

function account(overrides: Partial<Account> = {}): Account {
  return {
    id: "go-1",
    name: "Go 1",
    username: "",
    password: "",
    key: "sk-test",
    enabled: true,
    account_type: "key",
    setup_step: "ready",
    provider_id: "opencode",
    offering_id: "go",
    credential_kind: "api_key",
    quota_scope: "key",
    purchase_date: "2026-01-01",
    expires_on: "2027-01-01",
    cooldown_until: null,
    cooldown_generic_until: null,
    cooldown_5h_until: null,
    cooldown_week_until: null,
    cooldown_month_until: null,
    cooldown_free_until: null,
    last_error: null,
    auth_error: null,
    notes: "",
    usage_sync_last_success_at: null,
    usage_sync_next_allowed_at: null,
    created_at: "",
    updated_at: "",
    verification_status: "not_required",
    connection_verified_at: null,
    verification_error: null,
    plan_routable: true,
    model_capabilities: [],
    ...overrides,
  };
}

test("scope keys round-trip and accounts match backend-owned exact scopes", () => {
  assert.equal(providerScopeKey("provider", "command-code"), "provider:command-code");
  assert.deepEqual(parseProviderScopeKey("custom_endpoint:abc"), {
    scope_kind: "custom_endpoint",
    scope_id: "abc",
  });
  assert.equal(parseProviderScopeKey("nope"), null);
  const scopes = flattenProviderScopes(normalizeProviderContractsResponse(contracts()));
  assert.equal(findAccountScopeView(scopes, account())?.scope_id, "opencode");
  assert.equal(findAccountScopeView(scopes, account({
    id: "c1",
    provider_id: "custom",
    offering_id: "api",
  })), undefined);
  assert.equal(findAccountScopeView(scopes, account({
    id: "custom-1",
    provider_id: "custom",
    offering_id: "api",
  }))?.scope_id, "custom-1");
});

test("account scope matching distinguishes offerings under one provider", () => {
  const scopes = flattenProviderScopes(normalizeProviderContractsResponse(contracts({
    providers: [
      providerGroup(),
      providerGroup({
        scope_id: "opencode-go-plus",
        offerings: [{
          offering_id: "go-plus",
          display_name: "OpenCode Go Plus",
          routable: true,
          accounts: [{ id: "plus-1", name: "Plus 1", enabled: true, verification_status: "not_required" }],
        }],
      }),
    ],
  })));
  assert.equal(findAccountScopeView(scopes, account({
    id: "plus-1",
    provider_id: "opencode",
    offering_id: "go-plus",
  }))?.scope_id, "opencode-go-plus");
});

test("flatten keeps built-in providers grouped and Custom endpoints unflattened", () => {
  const catalog = [
    catalogEntry("opencode", "go", "OpenCode Go"),
    catalogEntry("custom", "api", "Custom API"),
  ];
  const scopes = flattenProviderScopes(normalizeProviderContractsResponse(contracts({
    providers: [
      providerGroup(),
    ],
    custom_endpoints: [
      customEndpoint(),
      customEndpoint({
        scope_id: "custom-2",
        account: { id: "custom-2", name: "Office", enabled: false, verification_status: "pending" },
      }),
    ],
  })), catalog);

  assert.deepEqual(scopes.map(({ key }) => key), [
    "provider:opencode",
    "custom_endpoint:custom-1",
    "custom_endpoint:custom-2",
  ]);
  assert.equal(scopes[1]?.label, "Home Lab");
  assert.equal(scopes[2]?.label, "Office");
});

test("stale or missing scope selection falls back to the first scope", () => {
  const scopes = flattenProviderScopes(normalizeProviderContractsResponse(contracts()));
  assert.equal(selectProviderScope(scopes, "provider", "opencode").fellBack, false);
  assert.equal(selectProviderScope(scopes, "provider", "opencode").scope?.scope_id, "opencode");
  const missing = selectProviderScope(scopes, "provider", "missing");
  assert.equal(missing.fellBack, true);
  assert.equal(missing.scope?.scope_id, "opencode");
  assert.equal(selectProviderScope([], "provider", "opencode").scope, null);
});

test("normalization preserves a provider model alias alongside its raw id", () => {
  const response = normalizeProviderContractsResponse(contracts({
    providers: [providerGroup({
      models: [modelContract("upstream-model-2026", { responses: true }, "gpt-5.6-luna")],
    })],
  }));
  const model = flattenProviderScopes(response)[0]?.models[0];
  assert.equal(model?.alias, "gpt-5.6-luna");
  assert.equal(model?.model_id, "upstream-model-2026");
});

test("account summaries use contract facts for protocol availability and unroutable reasons", () => {
  const closed = contracts({
    providers: [providerGroup({
      models: [modelContract("gpt-5.6-luna")],
      catalog_routable: false,
      production_inference: false,
      disabled_reasons: ["no enabled upstream protocol is available for this model"],
    })],
  });
  const summary = accountContractSummary(account(), closed);
  assert.ok(summary);
  assert.equal(summary.scope_kind, "provider");
  assert.equal(summary.scope_id, "opencode");
  assert.equal(summary.allProtocolsDisabled, true);
  assert.deepEqual(summary.enabledProtocols, []);

  const unroutable = accountContractSummary(account(), contracts({
    providers: [providerGroup({
      catalog_routable: false,
      production_inference: false,
      disabled_reasons: ["catalog is empty"],
    })],
  }));
  assert.ok(unroutable);
  assert.equal(unroutable.unroutable, true);
  assert.deepEqual(unroutable.disabledReasons, ["catalog is empty"]);
  assert.deepEqual(unroutable.enabledProtocols, ["chat_completions", "responses"]);

  const custom = accountContractSummary(account({
    id: "custom-1",
    name: "Home Lab",
    provider_id: "custom",
    offering_id: "api",
  }), contracts());
  assert.ok(custom);
  assert.equal(custom.scope_kind, "custom_endpoint");
  assert.equal(custom.label, "Home Lab");
});

test("a missing contract snapshot is not an empty-protocol summary and last-good remains", () => {
  const acc = account();
  assert.equal(accountContractSummary(acc, null), null);
  assert.equal(accountContractSummary(acc, undefined), null);

  const lastGood = contracts();
  const summary = accountContractSummary(acc, lastGood);
  assert.ok(summary);
  assert.notDeepEqual(summary.enabledProtocols, []);
  assert.deepEqual(
    accountContractSummary(acc, lastGood)?.enabledProtocols,
    summary.enabledProtocols,
  );
});

test("protocol evidence maps probe and preset states without color-only meaning", () => {
  assert.equal(protocolEvidenceStatus("messages", undefined), "unsupported");
  assert.equal(protocolEvidenceStatus("chat_completions", {
    protocol: "chat_completions",
    available: true,
    enabled: true,
    source: "static",
    verified_at: null,
    observed_at: null,
    last_probe_result: null,
    last_probe_at: null,
    last_probe_error: null,
    override: "auto",
  }), "static");
  assert.equal(protocolEvidenceStatus("chat_completions", {
    protocol: "chat_completions",
    available: true,
    enabled: true,
    source: "probe_confirmed",
    verified_at: "2026-08-22T00:00:00Z",
    observed_at: "2026-08-22T00:00:00Z",
    last_probe_result: "success",
    last_probe_at: "2026-08-22T00:00:00Z",
    last_probe_error: null,
    override: "auto",
  }), "probe_confirmed");
  assert.equal(protocolEvidenceStatus("chat_completions", {
    protocol: "chat_completions",
    available: true,
    enabled: false,
    source: "probe_observed",
    verified_at: null,
    observed_at: "2026-08-22T00:00:00Z",
    last_probe_result: "failure",
    last_probe_at: "2026-08-22T00:00:00Z",
    last_probe_error: "upstream 500",
    override: "auto",
  }), "probe_failure");
});

test("refresh and probe capability follow card/catalog facts, not raw provider ids", () => {
  const go = flattenProviderScopes(normalizeProviderContractsResponse(contracts()))[0]!;
  const custom = flattenProviderScopes(normalizeProviderContractsResponse(contracts()))[1]!;
  assert.equal(catalogRefreshSupported(go), true);
  assert.equal(protocolProbeSupported(go), true);
  assert.equal(catalogRefreshSupported(custom), false);
  assert.equal(protocolProbeSupported(custom), true);
  assert.deepEqual(enabledProtocols(custom), ["chat_completions"]);
});

test("unique protocols drop duplicates and unknown values before a probe payload", () => {
  assert.deepEqual(uniqueProtocols(["responses", "responses", "chat_completions", "messages"]), [
    "responses",
    "chat_completions",
    "messages",
  ]);
  assert.equal(protocolDisplayName("chat_completions"), "Chat Completions");
});

test("enabled protocols are derived only from model evidence, not scope switches", () => {
  const scope = flattenProviderScopes(normalizeProviderContractsResponse(contracts({
    providers: [providerGroup({
      models: [modelContract("gpt-5.6-luna", { chat_completions: true, responses: true })],
    })],
  })))[0]!;
  assert.deepEqual(enabledProtocols(scope), ["chat_completions", "responses"]);

  const empty = { ...scope, models: [] };
  assert.deepEqual(enabledProtocols(empty), []);
});

test("source URLs with credentials are not treated as safe to render", () => {
  assert.equal(isSafeSourceUrl("https://opencode.ai/zen/v1/models"), true);
  assert.equal(isSafeSourceUrl("http://127.0.0.1:8080/v1/models"), true);
  assert.equal(isSafeSourceUrl("https://user:secret@opencode.ai/zen/v1/models"), false);
  assert.equal(isSafeSourceUrl("javascript:alert(1)"), false);
});

test("returned model contracts merge into the last good provider response", () => {
  const next = modelContract("gpt-5.6-luna", { messages: true });
  next.protocols.messages = {
    ...next.protocols.messages!,
    available: true,
    enabled: true,
    source: "probe_confirmed",
    last_probe_result: "success",
    last_probe_at: "2026-08-22T01:00:00Z",
  };
  const merged = applyModelContractToResponse(
    contracts(),
    { scope_kind: "provider", scope_id: "opencode" },
    next,
  );
  assert.equal(
    merged.providers[0]?.models[0]?.protocols.messages?.source,
    "probe_confirmed",
  );
});
