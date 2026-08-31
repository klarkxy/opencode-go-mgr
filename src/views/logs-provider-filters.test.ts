import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { dashboardApi } from "../api/dashboard.ts";
import { installFetchMock } from "../test-helpers/dashboard-v3-fetch.ts";

const logs = readFileSync(new URL("./Logs.vue", import.meta.url), "utf8");
const presenters = readFileSync(new URL("../api/dashboard-presenters.ts", import.meta.url), "utf8");

test("forward log API sends the provider attribution filters as exact query params", async () => {
  const requests = installFetchMock(() => ({
    revision: 1,
    processGeneration: 99,
    pricingRevision: null,
    items: [],
    summary: {
      totalRequests: 0,
      promptTokens: 0,
      completionTokens: 0,
      cachedTokens: 0,
      cost: 0,
    },
  }));

  await dashboardApi.getForwardLogs({
    limit: 20,
    offset: 40,
    provider_id: "opencode",
    offering_id: "go",
    route_account_id: "route 1",
    credential_account_id: "cred 2",
  });

  const query = new URL(requests[0]!.url, "http://localhost").searchParams;
  assert.equal(query.get("providerId"), "opencode");
  assert.equal(query.get("offeringId"), "go");
  assert.equal(query.get("routeAccountId"), "route 1");
  assert.equal(query.get("credentialAccountId"), "cred 2");
  assert.equal(query.get("limit"), "20");
  assert.equal(query.get("offset"), "40");
});

test("forward log DTO declares nullable provider attribution and cost fields", () => {
  for (const field of [
    "route_account_id",
    "provider_id",
    "offering_id",
    "credential_account_id",
    "raw_cost_usd",
    "quota_debit",
    "effective_paid_cost_usd",
    "native_cost_value",
    "native_cost_unit",
    "native_cost_currency",
  ]) {
    assert.match(presenters, new RegExp(`${field}: [^;]*\\| null`));
  }
});

test("Alias column and detail titles distinguish effective Alias from the requested model", () => {
  assert.match(logs, /title: t\("模型别名"\)[^\n]*forwardLogAlias\(row\)/);
  assert.match(logs, /\[t\("请求模型"\), forwardLogRequestedModel\(row\)\]/);
  assert.match(logs, /\[t\("解析别名"\), forwardLogResolvedAlias\(row\)\]/);
});

test("forward filters are remote query params, reset paging, and are never local-page filtering", () => {
  assert.match(logs, /provider_id: providerFilter\.value/);
  assert.match(logs, /offering_id: offeringFilter\.value/);
  assert.match(logs, /route_account_id: routeAccountFilter\.value/);
  assert.match(logs, /credential_account_id: credentialAccountFilter\.value/);
  // The four selects join the remote-reload watcher that resets to page 1.
  const watcher = logs.slice(logs.indexOf("watch(\n  ["));
  assert.match(watcher, /providerFilter/);
  assert.match(watcher, /offeringFilter/);
  assert.match(watcher, /routeAccountFilter/);
  assert.match(watcher, /credentialAccountFilter/);
  assert.match(watcher, /forwardPage\.value = 1/);
  assert.match(watcher, /loadForwardLogs\(\)/);
  // Remote table stays remote; no client-side filtering of the loaded page.
  assert.match(logs, /:pagination="forwardPagination"[\s\S]*?remote/);
  assert.doesNotMatch(logs, /forwardLogs\.value\.filter|forwardLogs\.filter\(/);
});

test("provider filters keep accessible labels and participate in clear/has-filters state", () => {
  for (const label of ["服务商", "服务方案", "路由账号", "凭证账号"]) {
    assert.ok(logs.includes(`<span class="filter-label">{{ t("${label}") }}</span>`), label);
  }
  assert.match(logs, /providerFilter\.value = ""/);
  assert.match(logs, /credentialAccountFilter\.value = ""/);
  assert.match(logs, /!!providerFilter\.value/);
  assert.match(logs, /!!credentialAccountFilter\.value/);
});

test("provider filter selects keep an accessible name after selection", () => {
  // Once a value is chosen the placeholder is replaced by the selection, so it
  // can no longer serve as the accessible name. Each select must carry a
  // static aria-label using the same translated label, independent of value.
  for (const [filter, label] of [
    ["providerFilter", "服务商"],
    ["offeringFilter", "服务方案"],
    ["routeAccountFilter", "路由账号"],
    ["credentialAccountFilter", "凭证账号"],
  ] as const) {
    const start = logs.indexOf(`v-model:value="${filter}"`);
    assert.notEqual(start, -1, filter);
    const selectBlock = logs.slice(start, logs.indexOf("/>", start));
    assert.ok(
      selectBlock.includes(`:aria-label="t('${label}')"`),
      `${filter} must keep aria-label "${label}" after selection`,
    );
  }
});

test("row details render provider attribution and costs, with null as unknown and never $0", () => {
  assert.match(logs, /renderProviderCost\(row\)/);
  for (const label of ["服务商与费用", "原始供应商成本", "额度扣减", "有效付费成本"]) {
    assert.ok(logs.includes(label), label);
  }
  const detail = logs.slice(logs.indexOf("function renderProviderCost"));
  assert.match(detail, /value === null \|\| value === undefined \? t\("未知"\)/);
  assert.doesNotMatch(detail, /formatCost\(value \?\? 0|\|\| 0\)/);
  assert.doesNotMatch(detail.slice(0, detail.indexOf("function renderDiagnostic")), /native_cost_/);
});
