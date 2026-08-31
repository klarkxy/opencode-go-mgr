import assert from "node:assert/strict";
import test from "node:test";
import {
  APP_NAVIGATION,
  APP_NAVIGATION_GROUPS,
  CORE_APP_NAVIGATION,
  EXTERNAL_APP_NAVIGATION,
  applyAppViewSearchParams,
  isLegacyPricingView,
  readProviderScopeQuery,
  resolveAppViewKey,
} from "./app-navigation.ts";

test("navigation metadata keeps the fixed core order and CPA external group", () => {
  assert.deepEqual(
    CORE_APP_NAVIGATION.map(({ key }) => key),
    ["dashboard", "keys", "accounts", "providers", "apps", "logs", "settings"],
  );
  assert.deepEqual(EXTERNAL_APP_NAVIGATION.map(({ key }) => key), ["cpa"]);
  assert.equal(APP_NAVIGATION_GROUPS.external.label, "外部接入");
  assert.equal(APP_NAVIGATION.find(({ key }) => key === "cpa")?.label, "CPA");
});

test("legacy pricing view keys resolve to providers without inventing a second entry", () => {
  assert.equal(isLegacyPricingView("pricing"), true);
  assert.equal(resolveAppViewKey("pricing"), "providers");
  assert.equal(resolveAppViewKey("providers"), "providers");
  assert.equal(resolveAppViewKey("accounts"), "accounts");
  assert.equal(resolveAppViewKey("cpa"), "cpa");
  assert.equal(resolveAppViewKey("not-a-view"), "dashboard");
});

test("provider deep-link query fields round-trip on the providers view", () => {
  assert.deepEqual(readProviderScopeQuery("?view=providers&scope_kind=provider&scope_id=command-code"), {
    scope_kind: "provider",
    scope_id: "command-code",
  });
  const url = applyAppViewSearchParams(
    new URL("http://127.0.0.1:9042/dashboard/?view=accounts"),
    "providers",
    { scope_kind: "custom_endpoint", scope_id: "acc-9" },
  );
  assert.equal(url.searchParams.get("view"), "providers");
  assert.equal(url.searchParams.get("scope_kind"), "custom_endpoint");
  assert.equal(url.searchParams.get("scope_id"), "acc-9");
});

test("leaving providers strips scope query fields", () => {
  const url = applyAppViewSearchParams(
    new URL("http://127.0.0.1:9042/dashboard/?view=providers&scope_kind=provider&scope_id=opencode"),
    "logs",
  );
  assert.equal(url.searchParams.get("view"), "logs");
  assert.equal(url.searchParams.get("scope_kind"), null);
  assert.equal(url.searchParams.get("scope_id"), null);
});

test("leaving Accounts strips a stale account deep-link parameter", () => {
  const url = applyAppViewSearchParams(
    new URL("http://127.0.0.1:9042/dashboard/?view=accounts&account_id=custom-1"),
    "providers",
  );
  assert.equal(url.searchParams.get("view"), "providers");
  assert.equal(url.searchParams.get("account_id"), null);
});
