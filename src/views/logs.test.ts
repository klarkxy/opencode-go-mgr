import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { DashboardRequestError, tauriApi } from "../api/tauri.ts";
import { computeTimeRange, resolveTimeRange } from "./log-time-range.ts";

test("forward log API sends remote paging and filter parameters", async () => {
  let requested = "";
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      location: { pathname: "/dashboard" },
      dispatchEvent() {},
    },
  });
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async (input: string) => {
      requested = input;
      return new Response(JSON.stringify({
        items: [],
        summary: {
          total_requests: 0,
          prompt_tokens: 0,
          completion_tokens: 0,
          cached_tokens: 0,
          cost: 0,
        },
      }), { headers: { "Content-Type": "application/json" } });
    },
  });

  await tauriApi.getForwardLogs({
    limit: 20,
    offset: 40,
    status: "success",
    account_id: "account 117",
    request_id: "ocg-test id",
  });

  const query = new URL(requested, "http://localhost").searchParams;
  assert.equal(query.get("limit"), "20");
  assert.equal(query.get("offset"), "40");
  assert.equal(query.get("status"), "success");
  assert.equal(query.get("account_id"), "account 117");
  assert.equal(query.get("request_id"), "ocg-test id");
});

test("dashboard request errors preserve status for localized handling", async () => {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { pathname: "/dashboard" }, dispatchEvent() {} },
  });
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async () => new Response(JSON.stringify({ error: "raw fallback" }), {
      status: 409,
      headers: { "Content-Type": "application/json" },
    }),
  });

  await assert.rejects(
    () => tauriApi.registerAdmin("admin", "password123"),
    (error) => error instanceof DashboardRequestError
      && error.status === 409
      && error.message === "raw fallback",
  );
});

test("dashboard request errors preserve a non-JSON proxy response body", async () => {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { pathname: "/dashboard" }, dispatchEvent() {} },
  });
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async () => new Response("<h1>Bad Gateway</h1>", {
      status: 502,
      statusText: "Bad Gateway",
      headers: { "Content-Type": "text/html" },
    }),
  });

  await assert.rejects(
    () => tauriApi.registerAdmin("admin", "password123"),
    (error) => error instanceof DashboardRequestError
      && error.status === 502
      && error.message === "<h1>Bad Gateway</h1>",
  );
});

test("settings API maps the loaded revision to conditional writes and returns new revisions", async () => {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { pathname: "/dashboard" }, dispatchEvent() {} },
  });

  const requests: Array<{ url: string; body: Record<string, unknown> | null }> = [];
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async (input: string, init: RequestInit = {}) => {
      requests.push({
        url: input,
        body: init.body ? JSON.parse(String(init.body)) as Record<string, unknown> : null,
      });
      const response = input.endsWith("/regenerate-gateway-key")
        ? { key: "ocg-new-key", revision: 9 }
        : { revision: 8 };
      return new Response(JSON.stringify(response), {
        headers: { "Content-Type": "application/json" },
      });
    },
  });

  const result = await tauriApi.updateSettings({
    revision: 7,
    gateway_port: 9042,
    gateway_key: "ocg-old-key",
    upstream_base_url: "https://opencode.ai/zen/go",
    client_root_url: "",
    client_root_url_from_env: false,
    auto_start: false,
    auto_start_supported: false,
    show_dock_icon: true,
    dock_visibility_supported: false,
    connect_timeout_secs: 30,
    non_stream_timeout_secs: 900,
    stream_idle_timeout_secs: 300,
  });
  const regenerated = await tauriApi.regenerateGatewayKey();

  assert.deepEqual(result, { revision: 8 });
  assert.deepEqual(regenerated, { key: "ocg-new-key", revision: 9 });
  assert.equal(requests[0]?.body?.expected_revision, 7);
  assert.equal("revision" in (requests[0]?.body ?? {}), false);
});

test("account API sends purchase dates and the complete reorder payload", async () => {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { pathname: "/dashboard" }, dispatchEvent() {} },
  });

  const requests: Array<{ url: string; method: string; body: unknown }> = [];
  const account = {
    id: "account-2",
    name: "Second",
    username: "",
    password: "",
    key: "",
    enabled: true,
    purchase_date: "2026-07-15",
    expires_on: "2026-08-15",
    cooldown_until: null,
    last_error: null,
    created_at: "2026-07-15T00:00:00Z",
    updated_at: "2026-07-15T00:00:00Z",
  };
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async (input: string, init: RequestInit = {}) => {
      requests.push({
        url: input,
        method: init.method ?? "GET",
        body: init.body ? JSON.parse(String(init.body)) : null,
      });
      const response = input.endsWith("/accounts/order") ? [account] : account;
      return new Response(JSON.stringify(response), {
        headers: { "Content-Type": "application/json" },
      });
    },
  });

  const created = await tauriApi.createAccount({
    name: "Second",
    key: "sk-test",
    purchase_date: "2026-07-15",
  });
  const reordered = await tauriApi.reorderAccounts(["account-2", "account-1"]);

  assert.equal(created.purchase_date, "2026-07-15");
  assert.equal(created.expires_on, "2026-08-15");
  assert.equal(reordered[0]?.id, "account-2");
  assert.deepEqual(requests, [
    {
      url: "/dashboard/api/accounts",
      method: "POST",
      body: { name: "Second", key: "sk-test", purchase_date: "2026-07-15" },
    },
    {
      url: "/dashboard/api/accounts/order",
      method: "PUT",
      body: { account_ids: ["account-2", "account-1"] },
    },
  ]);
});

test("logs view shows top stats, extra filters, sorting, and a useful empty state", async () => {
  const source = await readFile(new URL("./Logs.vue", import.meta.url), "utf8");
  const template = source.slice(source.indexOf("<template>"), source.indexOf("<script setup"));

  assert.match(template, /class="stats-row"/);
  assert.match(template, /class="filter-bar"/);
  assert.match(template, /\bremote\b/);
  assert.match(template, /v-model:value="modelFilter"/);
  assert.match(template, /v-model:value="requestIdFilter"/);
  assert.match(template, /v-model:value="customTimeRange"/);
  assert.match(template, /v-model:value="sortBy"/);
  assert.match(template, /:aria-label="t\('刷新运行日志'\)"/);
  assert.match(template, /:aria-label="t\('刷新请求日志'\)"/);
  assert.match(template, /t\('仅记录经本机 API 转发的请求，账号 Ping 见运行日志'\)/);
  assert.match(template, /:loading="gatewayLoading"/);
  assert.match(template, /:loading="forwardLoading"/);
  assert.doesNotMatch(template, /:summary="forwardSummary"/);
  assert.doesNotMatch(template, /summary-placement="bottom"/);
  assert.doesNotMatch(source, /getForwardLogs\(200\)|filteredForwardLogs/);
  assert.match(source, /const request = \+\+forwardRequest/);
  assert.match(source, /request !== forwardRequest/);
  assert.match(source, /const request = \+\+gatewayRequest/);
  assert.match(source, /request !== gatewayRequest/);
  const gatewayLoad = source.slice(
    source.indexOf("async function loadGatewayLogs"),
    source.indexOf("let forwardRequest"),
  );
  assert.match(gatewayLoad, /if \(request === gatewayRequest\) gatewayLoading\.value = false/);
  const forwardLoad = source.slice(
    source.indexOf("async function loadForwardLogs"),
    source.indexOf("async function loadAccounts"),
  );
  assert.ok(forwardLoad.indexOf("forwardLogs.value = []") < forwardLoad.indexOf("await tauriApi.getForwardLogs"));
  assert.ok(forwardLoad.indexOf("forwardTotals.value = emptySummary()") < forwardLoad.indexOf("await tauriApi.getForwardLogs"));
  assert.match(forwardLoad, /catch \(e\)[\s\S]*request === forwardRequest[\s\S]*forwardLogs\.value = \[\]/);
  assert.match(source, /Promise\.all\(\[loadForwardLogs\(\), loadForwardLogModels\(\)\]\)/);
  assert.match(source, /row\.cost_state === "legacy_estimate"/);
  assert.match(source, /success_unpriced: \{ label: t\("无价格"\)/);
  assert.match(source, /outcome_unknown: \{ label: t\("结果未知"\)/);
  assert.match(source, /row\.error_source === "upstream"/);
  assert.match(source, /t\("上游拒绝"\)/);
  assert.match(source, /diagnostic\.request_fingerprint/);
  assert.match(source, /getGatewayLogs\(200, requestIdFilter\.value\)/);
  const requestIdWatchStart = source.indexOf("watch(requestIdFilter");
  const forwardFilterWatch = source.slice(
    source.indexOf("[statusFilter, accountFilter"),
    requestIdWatchStart,
  );
  const requestIdWatch = source.slice(requestIdWatchStart, source.indexOf("onMounted", requestIdWatchStart));
  assert.doesNotMatch(forwardFilterWatch, /requestIdFilter|loadGatewayLogs/);
  assert.match(requestIdWatch, /loadForwardLogs\(\)/);
  assert.match(requestIdWatch, /loadGatewayLogs\(\)/);
  assert.doesNotMatch(source, /legacy_estimate: \{ label:/);
  assert.match(template, /额度消耗（估算）/);
  const clearFilters = source.slice(source.indexOf("function clearFilters"), source.indexOf("function toggleSortOrder"));
  assert.doesNotMatch(clearFilters, /sortBy\.value|sortOrder\.value/);
});

test("logs time range selector renders presets and custom picker in a popover", async () => {
  const source = await readFile(new URL("./Logs.vue", import.meta.url), "utf8");
  const template = source.slice(source.indexOf("<template>"), source.indexOf("<script setup"));

  assert.match(template, /<n-popover[^>]*trigger="click"/);
  assert.match(template, /<n-button class="time-range-trigger">/);
  assert.doesNotMatch(template, /class="time-range-trigger"[^>]*:focusable="false"/);
  assert.match(template, /class="time-range-panel"/);
  assert.match(template, /class="preset-list"/);
  assert.match(template, /applyTimePreset\(item\.value\)/);
  assert.match(template, /class="custom-range-wrapper"/);
  assert.match(template, /:class="\{ 'is-visible': activePreset === 'custom' \}"/);
  assert.match(template, /type="daterange"/);
  assert.match(template, /:panel="true"/);
  assert.match(template, /:actions="null"/);
  assert.match(template, /v-model:value="customTimeRange"/);
  assert.match(template, /applyCustomTimeRange/);
});

test("logs time range helpers cover all presets", async () => {
  const now = new Date(2026, 6, 19, 12, 0, 0, 0);
  assert.deepEqual(computeTimeRange("last24h", now), [
    now.getTime() - 24 * 60 * 60 * 1000,
    now.getTime(),
  ]);
  assert.deepEqual(computeTimeRange("last7d", now), [
    now.getTime() - 7 * 24 * 60 * 60 * 1000,
    now.getTime(),
  ]);
  assert.deepEqual(computeTimeRange("last30d", now), [
    now.getTime() - 30 * 24 * 60 * 60 * 1000,
    now.getTime(),
  ]);
  assert.deepEqual(computeTimeRange("thisMonth", now), [
    new Date(2026, 6, 1).getTime(),
    now.getTime(),
  ]);
  assert.deepEqual(computeTimeRange("lastMonth", now), [
    new Date(2026, 5, 1).getTime(),
    new Date(2026, 5, 30, 23, 59, 59, 999).getTime(),
  ]);
});

test("rolling log presets resolve against the current refresh time", async () => {
  const first = new Date("2026-07-19T00:00:00Z");
  const later = new Date("2026-07-19T03:00:00Z");
  const staleSelection = computeTimeRange("last24h", first);

  assert.deepEqual(resolveTimeRange("last24h", staleSelection, later), computeTimeRange("last24h", later));
  assert.deepEqual(resolveTimeRange("custom", staleSelection, later), staleSelection);
  assert.equal(resolveTimeRange("all", staleSelection, later), null);

  const source = await readFile(new URL("./Logs.vue", import.meta.url), "utf8");
  const forwardLoad = source.slice(
    source.indexOf("async function loadForwardLogs"),
    source.indexOf("async function loadAccounts"),
  );
  assert.match(forwardLoad, /resolveTimeRange\(activePreset\.value, timeRange\.value\)/);
  assert.match(source, /url\.searchParams\.set\("range", activePreset\.value\)/);

  const clearFilters = source.slice(source.indexOf("function clearFilters"), source.indexOf("function toggleSortOrder"));
  assert.match(clearFilters, /activePreset\.value = "all"/);
  assert.match(clearFilters, /timeRange\.value = null/);
});
