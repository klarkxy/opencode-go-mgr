import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { DashboardRequestError, dashboardV3 } from "../api/dashboard-v3.ts";
import { dashboardApi } from "../api/dashboard.ts";
import { useConnectionStore } from "../stores/connection.ts";
import {
  installFetchMock,
  setupControlPlane,
  v3AccountDto,
} from "../test-helpers/dashboard-v3-fetch.ts";
import { computeTimeRange, resolveTimeRange } from "./log-time-range.ts";

function v3ForwardLogs(): object {
  return {
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
  };
}

test("forward log API sends remote paging and filter parameters", async () => {
  const requests = installFetchMock(() => v3ForwardLogs());

  await dashboardApi.getForwardLogs({
    limit: 20,
    offset: 40,
    status: "success",
    account_id: "account 117",
    request_id: "ocg-test id",
    sort_by: "attempt",
    sort_order: "asc",
  });

  const query = new URL(requests[0]!.url, "http://localhost").searchParams;
  assert.equal(query.get("limit"), "20");
  assert.equal(query.get("offset"), "40");
  assert.equal(query.get("status"), "success");
  assert.equal(query.get("accountId"), "account 117");
  assert.equal(query.get("requestId"), "ocg-test id");
  assert.equal(query.get("sortBy"), "attempt");
  assert.equal(query.get("sortOrder"), "asc");
});

test("dashboard request errors preserve status for localized handling", async () => {
  installFetchMock(() => new Response(JSON.stringify({ code: "conflict", message: "raw fallback" }), {
    status: 409,
    headers: { "Content-Type": "application/json" },
  }));

  await assert.rejects(
    () => dashboardV3.registerAdmin("admin", "password123", { expectedRevision: 0, processGeneration: 0 }),
    (error) => error instanceof DashboardRequestError
      && error.status === 409
      && error.message === "raw fallback",
  );
});

test("dashboard request errors preserve a non-JSON proxy response body", async () => {
  installFetchMock(() => new Response("<h1>Bad Gateway</h1>", {
    status: 502,
    statusText: "Bad Gateway",
    headers: { "Content-Type": "text/html" },
  }));

  await assert.rejects(
    () => dashboardV3.registerAdmin("admin", "password123", { expectedRevision: 0, processGeneration: 0 }),
    (error) => error instanceof DashboardRequestError
      && error.status === 502
      && error.message === "<h1>Bad Gateway</h1>",
  );
});

test("settings update writes CAS tokens and reloads the full config", async () => {
  setupControlPlane(7);
  const requests = installFetchMock(({ url, method }) => {
    if (method === "PUT" && url.endsWith("/settings")) {
      return { revision: 8, processGeneration: 99 };
    }
    if (method === "GET" && url.endsWith("/settings")) {
      return {
        revision: 8,
        processGeneration: 99,
        pricingRevision: null,
        gatewayPort: 9042,
        gatewayPortFromEnv: true,
        upstreamBaseUrl: "https://opencode.ai/zen/go",
        proxyMode: "auto",
        proxyUrl: "",
        proxyListDirection: "whitelist",
        proxyListModels: [],
        proxySupportedModels: [],
        opencodeInviteUrl: "https://opencode.ai/go?ref=68XPB6NP8V",
        clientRootUrl: "",
        clientRootUrlFromEnv: false,
        autoStart: false,
        autoStartSupported: false,
        showDockIcon: true,
        dockVisibilitySupported: false,
        connectTimeoutSecs: 30,
        nonStreamTimeoutSecs: 900,
        streamIdleTimeoutSecs: 300,
        routingMode: "strict-priority",
        conversationSticky: false,
      };
    }
    throw new Error(`unexpected request ${method} ${url}`);
  });

  const result = await dashboardApi.updateSettings({
    revision: 7,
    gateway_port: 9042,
    gateway_port_from_env: true,
    upstream_base_url: "https://opencode.ai/zen/go",
    proxy_mode: "auto",
    proxy_url: "",
    proxy_list_direction: "whitelist",
    proxy_list_models: [],
    proxy_supported_models: [],
    opencode_invite_url: "https://opencode.ai/go?ref=68XPB6NP8V",
    client_root_url: "",
    client_root_url_from_env: false,
    auto_start: false,
    auto_start_supported: false,
    show_dock_icon: true,
    dock_visibility_supported: false,
    connect_timeout_secs: 30,
    non_stream_timeout_secs: 900,
    stream_idle_timeout_secs: 300,
    routing_mode: "strict-priority",
    conversation_sticky: false,
  });

  assert.equal(requests[0]?.body?.expectedRevision, 7);
  assert.equal(requests[0]?.body?.processGeneration, 99);
  assert.equal("revision" in (requests[0]?.body ?? {}), false);
  assert.equal("gatewayPort" in (requests[0]?.body ?? {}), false);
  assert.equal(result.revision, 8);
  assert.equal(result.gateway_port, 9042);
  assert.equal(result.gateway_port_from_env, true);
});

test("primary key regeneration reloads plaintext from the connection endpoint", async () => {
  setupControlPlane(7);
  let primaryKey = "ocg-old-key";
  const requests = installFetchMock(({ url, method }) => {
    if (method === "POST" && url.endsWith("/keys/primary/regenerate")) {
      primaryKey = "ocg-new-key";
      return { revision: 8, processGeneration: 99 };
    }
    if (method === "GET" && url.endsWith("/connection")) {
      return {
        revision: 8,
        processGeneration: 99,
        gatewayPort: 9042,
        clientRootUrl: "http://127.0.0.1:9042",
        upstreamBaseUrl: "https://opencode.ai/zen/go",
        primaryKey,
        subKeys: [],
      };
    }
    throw new Error(`unexpected request ${method} ${url}`);
  });

  const store = useConnectionStore();
  const regenerated = await store.regeneratePrimaryKey();

  assert.equal(regenerated, "ocg-new-key");
  assert.deepEqual(
    requests.filter(({ method }) => method === "POST").map(({ body }) => body),
    [{ expectedRevision: 7, processGeneration: 99 }],
  );
});

test("account API sends purchase dates and the complete reorder payload", async () => {
  setupControlPlane(1);
  const requests = installFetchMock(({ url, method }) => {
    if (method === "POST" && url.endsWith("/accounts")) {
      return { account: v3AccountDto("account-2"), revision: 1, processGeneration: 99 };
    }
    if (method === "PUT" && url.endsWith("/accounts/order")) {
      return { accounts: [v3AccountDto("account-2")], revision: 1, processGeneration: 99 };
    }
    throw new Error(`unexpected request ${method} ${url}`);
  });

  const created = await dashboardApi.createAccount({
    name: "Second",
    key: "sk-test",
    purchase_date: "2026-07-15",
  });
  const reordered = await dashboardApi.reorderAccounts(["account-2", "account-1"]);

  assert.equal(created.purchase_date, "2026-07-15");
  assert.equal(created.expires_on, "2026-08-15");
  assert.equal(reordered[0]?.id, "account-2");
  assert.deepEqual(requests, [
    {
      url: "/dashboard/api/v3/accounts",
      method: "POST",
      body: {
        name: "Second",
        key: "sk-test",
        purchaseDate: "2026-07-15",
        expectedRevision: 1,
        processGeneration: 99,
      },
    },
    {
      url: "/dashboard/api/v3/accounts/order",
      method: "PUT",
      body: { accountIds: ["account-2", "account-1"], expectedRevision: 1, processGeneration: 99 },
    },
  ]);
});

test("managed account API uses ordered setup, browser targets, and profile reset routes", async () => {
  setupControlPlane(1);
  const requests = installFetchMock(({ url }) => {
    if (url.endsWith("/browser/capabilities")) {
      return { mode: "remote", reason: null, revision: 1, processGeneration: 99, pricingRevision: null };
    }
    if (url.endsWith("/browser-profile")) {
      return { account: v3AccountDto("managed-1"), revision: 1, processGeneration: 99 };
    }
    if (url.endsWith("/browser")) {
      return { mode: "remote", sessionToken: "session-1", revision: 1, processGeneration: 99, pricingRevision: null };
    }
    return { account: v3AccountDto("managed-1"), revision: 1, processGeneration: 99 };
  });

  await dashboardApi.createManagedAccount({ name: "Managed", username: "note@example.com" });
  await dashboardApi.advanceAccountSetup("managed-1", "opencode_registration");
  await dashboardApi.verifyManagedAccountKey("managed-1", "sk-secret");
  assert.deepEqual(await dashboardApi.getBrowserCapabilities(), { mode: "remote", reason: null });
  assert.deepEqual(await dashboardApi.openAccountBrowser("managed-1", "invite"), {
    mode: "remote",
    session_token: "session-1",
  });
  await dashboardApi.resetAccountBrowserProfile("managed-1");

  assert.deepEqual(requests.map(({ url, method, body }) => ({
    path: new URL(url, "http://localhost").pathname,
    method,
    body,
  })), [
    { path: "/dashboard/api/v3/accounts/managed", method: "POST", body: { name: "Managed", username: "note@example.com", expectedRevision: 1, processGeneration: 99 } },
    { path: "/dashboard/api/v3/accounts/managed-1/setup", method: "PATCH", body: { setupStep: "opencode_registration", expectedRevision: 1, processGeneration: 99 } },
    { path: "/dashboard/api/v3/accounts/managed-1/setup/verify-key", method: "POST", body: { key: "sk-secret", expectedRevision: 1, processGeneration: 99 } },
    { path: "/dashboard/api/v3/browser/capabilities", method: "GET", body: null },
    { path: "/dashboard/api/v3/accounts/managed-1/browser", method: "POST", body: { target: "invite", expectedRevision: 1, processGeneration: 99 } },
    { path: "/dashboard/api/v3/accounts/managed-1/browser-profile", method: "DELETE", body: { expectedRevision: 1, processGeneration: 99 } },
  ]);
});

test("logs view wires filters, race cancellation, debounce, and empty or error state", async () => {
  const source = await readFile(new URL("./Logs.vue", import.meta.url), "utf8");
  const template = source.slice(source.indexOf("<template>"), source.indexOf("<script setup"));

  assert.match(template, /v-model:value="modelFilter"/);
  assert.match(template, /v-model:value="requestIdFilter"/);
  assert.match(template, /v-model:value="customTimeRange"/);
  assert.match(template, /v-model:value="sortBy"/);
  assert.match(template, /:aria-label="t\('刷新运行日志'\)"/);
  assert.match(template, /:aria-label="t\('刷新请求日志'\)"/);
  assert.match(template, /t\('记录推理转发和协议探测请求；运行日志只记录进程与控制面事件'\)/);
  assert.match(template, /:loading="gatewayLoading"/);
  assert.match(template, /:loading="forwardLoading"/);
  assert.doesNotMatch(template, /:summary="forwardSummary"/);
  assert.doesNotMatch(source, /getForwardLogs\(200\)|filteredForwardLogs/);
  assert.match(source, /const request = \+\+forwardRequest/);
  assert.match(source, /request !== forwardRequest/);
  assert.match(source, /const request = \+\+gatewayRequest/);
  assert.match(source, /request !== gatewayRequest/);
  assert.match(source, /query\.get\("tab"\) === "gateway" \? "gateway" : "forward"/);
  const gatewayLoad = source.slice(
    source.indexOf("async function loadGatewayLogs"),
    source.indexOf("let forwardRequest"),
  );
  assert.match(gatewayLoad, /if \(request === gatewayRequest\) gatewayLoading\.value = false/);
  const forwardLoad = source.slice(
    source.indexOf("async function loadForwardLogs"),
    source.indexOf("async function loadAccounts"),
  );
  assert.ok(forwardLoad.indexOf("forwardLogs.value = []") < forwardLoad.indexOf("await dashboardApi.getForwardLogs"));
  assert.ok(forwardLoad.indexOf("forwardTotals.value = emptySummary()") < forwardLoad.indexOf("await dashboardApi.getForwardLogs"));
  assert.match(forwardLoad, /catch \(e\)[\s\S]*request === forwardRequest[\s\S]*forwardLogs\.value = \[\]/);
  assert.match(source, /Promise\.all\(\[loadForwardLogs\(\), loadForwardLogModels\(\), loadForwardLogKeys\(\)\]\)/);
  assert.match(source, /row\.cost_state === "legacy_estimate"/);
  assert.match(source, /row\.cost_state === "free"/);
  assert.match(source, /t\("免费"\)/);
  assert.match(source, /success_unpriced: \{ label: t\("无价格"\)/);
  assert.match(source, /outcome_unknown: \{ label: t\("结果未知"\)/);
  assert.match(source, /row\.error_source === "upstream"/);
  assert.match(source, /t\("上游拒绝"\)/);
  assert.match(source, /focusRequestChain\(requestId\)/);
  assert.match(source, /requestIdFilter\.value = requestId[\s\S]*sortBy\.value = "attempt"[\s\S]*sortOrder\.value = "asc"/);
  assert.match(source, /getGatewayLogs\(200, requestIdFilter\.value\)/);
  const requestIdWatchStart = source.indexOf("watch(requestIdFilter");
  const forwardFilterWatch = source.slice(
    source.indexOf("[statusFilter, accountFilter"),
    requestIdWatchStart,
  );
  const requestIdWatch = source.slice(requestIdWatchStart, source.indexOf("onMounted", requestIdWatchStart));
  // request-id typing is debounced: immediate watches must not reload per keystroke,
  // and the debounced trigger refreshes both lists exactly once per batch.
  assert.doesNotMatch(forwardFilterWatch, /requestIdFilter/);
  assert.doesNotMatch(forwardFilterWatch, /loadGatewayLogs/);
  assert.match(requestIdWatch, /setTimeout/);
  assert.match(requestIdWatch, /\b300\b/);
  assert.match(requestIdWatch, /clearTimeout\(requestIdDebounce\)/);
  assert.match(requestIdWatch, /loadGatewayLogs\(\)/);
  assert.match(requestIdWatch, /loadForwardLogs\(\)/);
  assert.match(requestIdWatch, /syncQueryState\(\)/);
  assert.match(source, /onUnmounted\(\(\) => \{[\s\S]*clearTimeout\(requestIdDebounce\)/);
  assert.doesNotMatch(source, /legacy_estimate: \{ label:/);
  assert.match(template, /t\("总 Tokens"\)/);
  assert.match(template, /forwardTotals\.prompt_tokens \+ forwardTotals\.completion_tokens/);
  const stats = template.slice(template.indexOf('class="stats-row"'), template.indexOf('class="filter-bar"'));
  assert.doesNotMatch(stats, /额度消耗（估算）|forwardTotals\.cost/);
  const clearFilters = source.slice(source.indexOf("function clearFilters"), source.indexOf("function toggleSortOrder"));
  assert.doesNotMatch(clearFilters, /sortBy\.value|sortOrder\.value/);
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

test("forward logs can be filtered by client key including the unattributed option", async () => {
  const source = await readFile(new URL("./Logs.vue", import.meta.url), "utf8");
  const template = source.slice(source.indexOf("<template>"), source.indexOf("<script setup"));

  // Selector data comes from the log table so deleted/dangling ids stay listed.
  assert.match(template, /v-model:value="keyFilter"/);
  assert.match(source, /clientKeys\.value = await dashboardApi\.getForwardLogKeys\(\)/);
  assert.match(source, /label: t\("未归因"\), value: UNATTRIBUTED_KEY_FILTER/);
  assert.match(source, /key_id: keyFilter\.value/);
  // Filter changes participate in pagination reset + query state sync.
  const forwardFilterWatch = source.slice(
    source.indexOf("watch(\n  [", source.indexOf("watch(activeTab")),
    source.indexOf("watch(requestIdFilter"),
  );
  assert.match(forwardFilterWatch, /keyFilter/);
  assert.match(source, /url\.searchParams\.set\("key", keyFilter\.value\)/);
  const clearFilters = source.slice(source.indexOf("function clearFilters"), source.indexOf("function toggleSortOrder"));
  assert.match(clearFilters, /keyFilter\.value = ""/);
  // Pre-upgrade usage lands on the primary key; the note explains that.
  assert.match(template, /升级前用量统一计入主 Key/);
  // The note lives outside the filter-bar grid so it never pushes the
  // remaining filters onto a second row.
  assert.ok(
    template.indexOf("key-filter-note") > template.indexOf("filter-actions"),
    "key filter note must render after the filter bar, not inside it",
  );
  const noteRule = source.slice(
    source.indexOf(".key-filter-note"),
    source.indexOf("}", source.indexOf(".key-filter-note")),
  );
  assert.doesNotMatch(noteRule, /grid-column/);
  // The key filter keeps a real column width (an `auto` column collapses an
  // empty select) and its menu expands to fit long key names.
  assert.match(source, /grid-template-columns: repeat\(5, 1fr\)/);
  const keyField = template.slice(template.indexOf("接入 Key"), template.indexOf("时间范围"));
  assert.match(keyField, /:consistent-menu-width="false"/);
});

test("request ids render shortened while the full value stays reachable", async () => {
  const source = await readFile(new URL("./Logs.vue", import.meta.url), "utf8");

  // Display keeps the head and tail; tooltip, copy, and chain focus keep
  // the complete id.
  assert.match(source, /function shortRequestId/);
  assert.match(source, /`\$\{requestId\.slice\(0, 13\)\}…\$\{requestId\.slice\(-4\)\}`/);
  assert.match(source, /h\("code", shortRequestId\(requestId\)\)/);
  assert.match(source, /title: requestId,/);
  assert.match(source, /copyText\(target, requestId, t\("请求 ID"\)\)/);
  assert.match(source, /focusRequestChain\(requestId\)/);
  assert.match(source, /width: 170, render: renderRequestId/);
});
