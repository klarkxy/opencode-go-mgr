import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { DashboardRequestError, tauriApi } from "../api/tauri.ts";

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
  });

  const query = new URL(requested, "http://localhost").searchParams;
  assert.equal(query.get("limit"), "20");
  assert.equal(query.get("offset"), "40");
  assert.equal(query.get("status"), "success");
  assert.equal(query.get("account_id"), "account 117");
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
  assert.match(template, /v-model:value="timeRange"/);
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
  assert.match(source, /Promise\.all\(\[loadForwardLogs\(\), loadForwardLogModels\(\)\]\)/);
  const clearFilters = source.slice(source.indexOf("function clearFilters"), source.indexOf("function toggleSortOrder"));
  assert.doesNotMatch(clearFilters, /sortBy\.value|sortOrder\.value/);
});
