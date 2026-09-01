import assert from "node:assert/strict";
import test from "node:test";
import {
  APP_NAVIGATION,
  APP_NAVIGATION_GROUPS,
  CORE_APP_NAVIGATION,
  EXTENSION_APP_NAVIGATION,
  applyAccountViewSearchParams,
  applyAppViewSearchParams,
  isLegacyPricingView,
  readAccountDeepLink,
  readProviderScopeQuery,
  resolveAppViewKey,
} from "./app-navigation.ts";

test("navigation metadata keeps the fixed core order while staged extensions stay hidden", () => {
  assert.deepEqual(
    CORE_APP_NAVIGATION.map(({ key }) => key),
    ["dashboard", "keys", "accounts", "providers", "aliases", "apps", "logs", "settings"],
  );
  assert.deepEqual(EXTENSION_APP_NAVIGATION, []);
  assert.equal(APP_NAVIGATION_GROUPS.extensions.label, "扩展");
  assert.equal(APP_NAVIGATION.some(({ key }) => String(key) === "cpa"), false);
});

test("legacy pricing view keys resolve to providers without inventing a second entry", () => {
  assert.equal(isLegacyPricingView("pricing"), true);
  assert.equal(resolveAppViewKey("pricing"), "providers");
  assert.equal(resolveAppViewKey("providers"), "providers");
  assert.equal(resolveAppViewKey("aliases"), "aliases");
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

test("Custom account edit links target Accounts and preserve the exact account id", () => {
  const url = applyAccountViewSearchParams(
    new URL("http://127.0.0.1:9042/dashboard/?view=aliases&scope_kind=provider&scope_id=custom"),
    "custom-account-9",
  );
  assert.equal(url.searchParams.get("view"), "accounts");
  assert.equal(readAccountDeepLink(url.search), "custom-account-9");
  assert.equal(url.searchParams.get("scope_kind"), null);
  assert.equal(url.searchParams.get("scope_id"), null);
});
