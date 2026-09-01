import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import type { Account } from "../api/dashboard.ts";
import {
  accountMenuOptions,
  accountStatusLabel,
  accountStatusTagType,
} from "../domain/account-display.ts";

const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
const form = readFileSync(new URL("../components/AccountFormModal.vue", import.meta.url), "utf8");
const testModal = readFileSync(new URL("../components/AccountConnectionTestModal.vue", import.meta.url), "utf8");
const usage = readFileSync(new URL("../domain/useAccountUsage.ts", import.meta.url), "utf8");

function customAccount(overrides: Partial<Account> = {}): Account {
  return {
    id: "custom-1",
    name: "Custom",
    username: "",
    password: "",
    key: "key",
    enabled: false,
    account_type: "key",
    setup_step: "ready",
    provider_id: "custom",
    offering_id: "api",
    credential_kind: "api_key",
    quota_scope: "key",
    purchase_date: "",
    expires_on: "",
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
    created_at: "2026-08-21T00:00:00Z",
    updated_at: "2026-08-21T00:00:00Z",
    verification_status: "pending",
    connection_verified_at: null,
    verification_error: null,
    plan_routable: true,
    model_capabilities: [],
    ...overrides,
  };
}

test("routable Custom account status follows live enablement rather than legacy verification state", () => {
  const pending = customAccount();
  assert.equal(accountStatusLabel(pending), "已禁用");
  assert.equal(accountStatusTagType(pending), "default");

  const failed = customAccount({ verification_status: "failed" });
  assert.equal(accountStatusLabel(failed), "已禁用");
  assert.equal(accountStatusTagType(failed), "default");

  const verified = customAccount({ verification_status: "verified" });
  assert.equal(accountStatusLabel(verified), "已禁用");
  assert.equal(accountStatusTagType(verified), "default");
});

test("Custom cards drop Go-only console/profile actions but keep edit and delete", () => {
  const keys = accountMenuOptions(customAccount(), Date.now()).map(({ key }) => key);
  assert.deepEqual(keys, ["edit", "delete"]);
});

test("the unified connection test stays account-scoped and avoids control-plane mutation", () => {
  assert.match(accounts, /@test-connection="openAccountTest\(account\.id\)"/);
  assert.match(accounts, /<AccountConnectionTestModal[\s\S]*?:account="testingAccount"/);
  assert.match(testModal, /dashboardApi\.testAccountModel\(account\.id, model\.modelId\)/);
  assert.match(testModal, /for \(const model of queue\)[\s\S]*?await testOne\(model, generation\)/);
  assert.doesNotMatch(testModal, /verifyAccountConnection|runWithFreshSettingsRevision|providerApi\.runProtocolProbes/);
});

test("Custom edits validate before dispatching only their changed sections", () => {
  const start = accounts.indexOf("async function saveCustomAccountEdit");
  const body = accounts.slice(start, accounts.indexOf("async function toggleAccount"));
  assert.match(body, /executeCustomAccountEdit\(editing, payload/);
  // Success (and closing the modal) still happen only after the executor returns.
  const executeAt = body.indexOf("executeCustomAccountEdit");
  const successAt = body.indexOf('message.success(t("账号已更新"))');
  const closeAt = body.indexOf("showModal.value = false");
  assert.ok(executeAt < successAt && successAt < closeAt);
  const catchBody = body.slice(body.indexOf("catch"));
  assert.doesNotMatch(catchBody, /showModal\.value = false/);
  assert.match(catchBody, /refreshAccountState\(editing\.id\)/);
});

test("no account usage or official refresh is ever requested for Custom accounts", () => {
  assert.match(accounts, /if \(accountHasUsageDisplay\(created\) && accountIsReady\(created\)\)/);
  assert.match(accounts, /function accountHasUsageDisplay[\s\S]*isCommandCodeGoatAccount[\s\S]*provider_id === "opencode" && account\.offering_id === "go"/);
  assert.doesNotMatch(accounts.slice(accounts.indexOf("function accountHasUsageDisplay"), accounts.indexOf("async function refreshAccountState")), /isCustomApiAccount/);
  assert.doesNotMatch(accounts, /isCustomApiAccount\(created\)/);
  assert.match(accounts, /accountIsReady\(account\) && accountHasUsageDisplay\(account\)/);
  assert.match(usage, /async function retryQuotaLimits[\s\S]*?account\.provider_id === "opencode"[\s\S]*?account\.offering_id === "go"/);
});

test("every ready account card exposes the same test action without gating the enable switch", () => {
  assert.match(card, /class="account-action account-action--test"/);
  assert.match(card, /:disabled="!accountIsReady\(account\)"/);
  assert.match(card, /@click="emit\('test-connection'\)"/);
  assert.doesNotMatch(card, /@click="emit\('verify'\)"|:loading="verifying"/);
  // Testing is independent of the routing switch and remains available for
  // disabled, cooling, or historically unverified ready accounts.
  assert.doesNotMatch(card, /customAccountToggleBlocked/);
  assert.match(card, /:disabled="!!toggleBlockedReason"/);
  // Custom endpoints never present a subscription expiry, even if legacy
  // account data still carries lifecycle dates.
  assert.match(card, /<n-popover[\s\S]*?v-if="hasValidityPeriod"/);
  assert.match(card, /accountIsReady\(props\.account\)[\s\S]*?!isCustom\.value[\s\S]*?!isZen\.value[\s\S]*?purchase_date[\s\S]*?expires_on/);
  assert.match(form, /if \(hasField\("purchase_date"\)\)/);
  assert.doesNotMatch(card, /isScnet/);
  assert.doesNotMatch(card, /目标端点由管理员自行选择并负责/);
});

test("the form declares one API URL and one account-level protocol", () => {
  assert.match(form, /目标端点由管理员自行选择并负责/);
  assert.doesNotMatch(form, /\$emit\('verify'\)/);
  assert.match(form, /v-model:value="form\.endpointUrl"/);
  assert.match(form, /v-model:value="form\.upstreamProtocol"/);
  assert.match(form, /customApiUrlSupportsModelDiscovery/);
  assert.match(form, /customApiUrlNeedsManualModels/);
  assert.match(form, /v-if="showManualModelHint"/);
  assert.match(form, /推荐填写不带 \/v1 的 API 根地址/);
  assert.match(form, /非标准完整 Endpoint 无法自动推导 \/models；请手动添加模型映射。/);
  assert.doesNotMatch(form, /capabilityProtocol/);
  assert.doesNotMatch(form, /authScheme|auth_scheme/);
  // API URL and protocol stay editable after create; no immutability hints.
  assert.doesNotMatch(form, /fieldImmutableAfterCreate/);
  assert.doesNotMatch(form, /创建后不可修改/);
  assert.match(form, /customEndpointUrlIssue\(value \?\? ""\)/);
  // Edit mode forwards the API URL, account-wide protocol, and canonical rows.
  assert.match(form, /payload\.endpoint_url = form\.value\.endpointUrl\.trim\(\)/);
  assert.match(form, /payload\.upstream_protocol = form\.value\.upstreamProtocol/);
  assert.match(form, /public_model: capability\.public_model/);
  assert.match(form, /upstream_model: capability\.upstream_model/);
  assert.match(form, /v-model:value="selectedDiscoveredModels"/);
  assert.match(form, /for \(const model of selectedDiscoveredModels\.value\)/);
  assert.match(form, /modelMapping\(\{ public_model: model, upstream_model: model \}\)/);
});

test("model discovery ignores responses after the form context changes", () => {
  assert.match(form, /let discoveryGeneration = 0/);
  assert.match(form, /\{ flush: "sync" \}/);
  assert.match(form, /generation !== discoveryGeneration \|\| !modelDiscoveryContextMatches\(context\)/);
  assert.match(form, /generation === discoveryGeneration && modelDiscoveryContextMatches\(context\)/);
});
