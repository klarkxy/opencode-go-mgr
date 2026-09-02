import assert from "node:assert/strict";
import test from "node:test";
import { presentOllamaUsage, providerApi } from "./providers.ts";
import { installFetchMock, setupControlPlane } from "../test-helpers/dashboard-v3-fetch.ts";

test("Go protocol probe sends only provider, model, and protocol intent", async () => {
  setupControlPlane(12, 42, "p1");
  const requests = installFetchMock(({ url }) => {
    if (url.endsWith("/providers/opencode/protocol-probes")) {
      return {
        accountId: null,
        providerId: "opencode",
        modelId: "gpt-5.6-luna",
        results: [{ protocol: "responses", success: true, skipped: false, error: null }],
        contract: null,
        revision: 12,
        processGeneration: 42,
        pricingRevision: "p1",
      };
    }
    throw new Error(`unexpected request ${url}`);
  });

  const result = await providerApi.runProtocolProbes("opencode", {
    model_id: "gpt-5.6-luna",
    protocols: ["responses"],
  });

  assert.equal(result.model_id, "gpt-5.6-luna");
  assert.deepEqual(requests[0], {
    url: "/dashboard/api/v3/providers/opencode/protocol-probes",
    method: "POST",
    body: {
      modelId: "gpt-5.6-luna",
      protocols: ["responses"],
      expectedRevision: 12,
      processGeneration: 42,
    },
  });
});

test("Go and GOAT model refresh use the selected account on the provider route", async () => {
  setupControlPlane(12, 42, "p1");
  const providers: Record<string, string> = {
    "go-account": "opencode",
    "goat-account": "command-code",
  };
  const requests = installFetchMock(({ url, method }) => {
    const accountId = Object.keys(providers).find((id) => url.endsWith(`/accounts/${id}`));
    if (accountId) {
      return {
        id: accountId,
        providerId: providers[accountId],
        revision: 12,
        processGeneration: 42,
      };
    }
    if (url.includes("/models/refresh") && method === "POST") {
      const providerId = url.includes("/providers/opencode/") ? "opencode" : "command-code";
      const body = requests.at(-1)!.body!;
      return {
        providerId,
        accountId: body.accountId,
        models: ["model-one"],
        refreshedAt: "2026-08-24T00:00:00Z",
        sourceUrl: "https://example.test/v1/models",
        revision: 12,
        processGeneration: 42,
        pricingRevision: "p1",
      };
    }
    throw new Error(`unexpected request ${url}`);
  });

  await providerApi.refreshProviderModels("go-account");
  await providerApi.refreshProviderModels("goat-account");

  assert.deepEqual(requests.filter(({ url }) => url.endsWith("/models/refresh")), [
    {
      url: "/dashboard/api/v3/providers/opencode/models/refresh",
      method: "POST",
      body: { accountId: "go-account", expectedRevision: 12, processGeneration: 42 },
    },
    {
      url: "/dashboard/api/v3/providers/command-code/models/refresh",
      method: "POST",
      body: { accountId: "goat-account", expectedRevision: 12, processGeneration: 42 },
    },
  ]);
});

test("unified catalog refresh sends only the selected contract scope and CAS tokens", async () => {
  setupControlPlane(12, 42, "p1");
  const requests = installFetchMock(({ url, method }) => {
    if (url.endsWith("/provider-contracts/provider/opencode/catalog/refresh") && method === "POST") {
      return {
        revision: 13,
        processGeneration: 42,
        pricingRevision: "p1",
        providers: [],
        customEndpoints: [],
      };
    }
    throw new Error(`unexpected request ${url}`);
  });

  await providerApi.refreshContractCatalog("provider", "opencode");

  assert.deepEqual(requests, [{
    url: "/dashboard/api/v3/provider-contracts/provider/opencode/catalog/refresh",
    method: "POST",
    body: { expectedRevision: 12, processGeneration: 42 },
  }]);
});

test("Custom endpoint protocol probe stays blocked while overrides use the model-protocol-overrides route", async () => {
  setupControlPlane(8, 42, "p1");
  const requests = installFetchMock(({ url, method }) => {
    if (url.endsWith("/accounts/custom-1")) {
      return { id: "custom-1", providerId: "custom", revision: 8, processGeneration: 42 };
    }
    if (url.endsWith("/provider-contracts/custom-endpoint/custom-1/model-protocol-overrides") && method === "PUT") {
      return {
        revision: 9,
        processGeneration: 42,
        pricingRevision: "p1",
        providers: [],
        customEndpoints: [],
      };
    }
    throw new Error(`unsupported request ${url}`);
  });

  await assert.rejects(
    () => providerApi.runProtocolProbes("custom", {
      model_id: "Org/Model",
      protocols: ["chat_completions"],
    }),
    /尚未纳入 Dashboard V3 合同/,
  );
  await providerApi.updateModelProtocolOverrides(
    "custom_endpoint",
    "custom-1",
    [{ model_id: "Org/Model", protocol: "chat_completions", state: "force_off" }],
  );
  assert.deepEqual(requests.map(({ method, url }) => ({ method, url })), [
    {
      method: "PUT",
      url: "/dashboard/api/v3/provider-contracts/custom-endpoint/custom-1/model-protocol-overrides",
    },
  ]);
  assert.deepEqual(requests[0]?.body, {
    overrides: [{ modelId: "Org/Model", protocol: "chat_completions", state: "force_off" }],
    expectedRevision: 8,
    processGeneration: 42,
  });
});

const ZEN_FREE_ACCOUNT_ID = "00000000-0000-0000-0000-000000000002";

function zenFreeAccountDto(overrides: Record<string, unknown> = {}) {
  return {
    id: ZEN_FREE_ACCOUNT_ID,
    name: "OpenCode Zen Free",
    username: null,
    enabled: true,
    accountType: "key",
    setupStep: "ready",
    providerId: "opencode-zen-free",
    offeringId: "anonymous-free",
    credentialKind: "none",
    quotaScope: "egress-ip",
    revision: 12,
    processGeneration: 42,
    purchaseDate: "2026-01-01",
    expiresOn: "2026-02-01",
    cooldownUntil: null,
    cooldownGenericUntil: null,
    cooldown5hUntil: null,
    cooldownWeekUntil: null,
    cooldownMonthUntil: null,
    cooldownFreeUntil: null,
    lastError: null,
    authError: null,
    notes: null,
    usageSyncLastSuccessAt: null,
    usageSyncNextAllowedAt: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    verificationStatus: "not_required",
    connectionVerifiedAt: null,
    verificationError: null,
    planRoutable: true,
    customConfig: null,
    modelCapabilities: [],
    ...overrides,
  };
}

test("Zen Free provider settings reject non-Zen accounts before the dedicated write", async () => {
  setupControlPlane(12, 42, "p1");
  const requests = installFetchMock(({ url }) => {
    if (url.endsWith("/accounts/go-account-2")) {
      return { id: "go-account-2", providerId: "opencode", revision: 12, processGeneration: 42 };
    }
    throw new Error(`unexpected request ${url}`);
  });

  await assert.rejects(
    () => providerApi.updateProviderSettings("go-account-2", { enabled: false }),
    /only Zen Free has provider settings/,
  );
  assert.deepEqual(requests.map(({ method, url }) => ({ method, url })), [
    { method: "GET", url: "/dashboard/api/v3/accounts/go-account-2" },
  ]);
});

test("Zen Free enable switch writes the catalog provider through PATCH /providers/zen-free", async () => {
  setupControlPlane(12, 42, "p1");
  let enabled = true;
  const requests = installFetchMock(({ url, method }) => {
    if (url.endsWith(`/accounts/${ZEN_FREE_ACCOUNT_ID}`)) {
      return zenFreeAccountDto({ enabled, revision: enabled ? 12 : 13 });
    }
    if (url.endsWith("/providers/zen-free") && method === "PATCH") {
      enabled = false;
      return {
        accountId: ZEN_FREE_ACCOUNT_ID,
        enabled: false,
        revision: 13,
        processGeneration: 42,
        pricingRevision: "p1",
      };
    }
    throw new Error(`unexpected request ${url}`);
  });

  const result = await providerApi.updateProviderSettings(ZEN_FREE_ACCOUNT_ID, { enabled: false });

  assert.equal(result.account.provider_id, "opencode-zen-free");
  assert.equal(result.account.enabled, false);
  assert.equal(result.revision, 13);
  assert.deepEqual(requests[1], {
    url: "/dashboard/api/v3/providers/zen-free",
    method: "PATCH",
    body: {
      enabled: false,
      expectedRevision: 12,
      processGeneration: 42,
    },
  });
});

test("presentOllamaUsage keeps null snapshot fields null instead of coercing them", () => {
  const presented = presentOllamaUsage({
    accountId: "ollama-1",
    cookieConfigured: true,
    status: "failed",
    snapshot: {
      windows: [
        { window: "5h", used_percent: null, reset_at: null },
        { window: "7d", used_percent: 42.5, reset_at: "2026-09-02T12:00:00Z" },
      ],
      models: [
        { model: "deepseek-v4-flash:0731", requests_5h: null, requests_7d: 9 },
      ],
      plan: null,
      balance: null,
    },
    lastError: "upstream failed",
    lastSuccessAt: null,
    lastAttemptAt: "2026-09-02T10:00:00Z",
    nextEligibleAt: null,
    failureStreak: 2,
    revision: 1,
    processGeneration: 1,
  });

  assert.equal(presented.account_id, "ollama-1");
  assert.equal(presented.cookie_configured, true);
  assert.equal(presented.status, "failed");
  assert.equal(presented.last_error, "upstream failed");
  // null must stay null: Number(null) is 0 and String(null) is "null".
  assert.deepEqual(presented.snapshot?.windows, [
    { window: "5h", used_percent: null, reset_at: null },
    { window: "7d", used_percent: 42.5, reset_at: "2026-09-02T12:00:00Z" },
  ]);
  assert.deepEqual(presented.snapshot?.models, [
    { model: "deepseek-v4-flash:0731", requests_5h: null, requests_7d: 9 },
  ]);
  assert.equal(presented.snapshot?.plan, null);
  assert.equal(presented.snapshot?.balance, null);

  const unconfigured = presentOllamaUsage({
    accountId: "ollama-2",
    cookieConfigured: false,
    status: "unconfigured",
    snapshot: null,
    lastError: null,
    lastSuccessAt: null,
    lastAttemptAt: null,
    nextEligibleAt: null,
    failureStreak: 0,
    revision: 1,
    processGeneration: 1,
  });
  assert.equal(unconfigured.snapshot, null);
});
