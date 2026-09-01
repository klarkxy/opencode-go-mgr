import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  DEFAULT_OPENCODE_INVITE_URL,
  browserViewUrl,
  nextSetupStep,
  normalizeOpenCodeInviteUrl,
  setupBrowserTarget,
  setupStepIndex,
} from "./managed-account.ts";

async function readSource(url: string | URL): Promise<string> {
  return (await readFile(url, "utf8")).replace(/\r\n/g, "\n");
}

test("managed signup steps advance in order and map to allowed browser targets", () => {
  assert.equal(setupStepIndex("google_account"), 0);
  assert.equal(nextSetupStep("google_account"), "opencode_registration");
  assert.equal(nextSetupStep("opencode_registration"), "payment");
  assert.equal(nextSetupStep("payment"), "key_verification");
  assert.equal(nextSetupStep("key_verification"), "ready");
  assert.equal(nextSetupStep("ready"), null);
  assert.equal(setupBrowserTarget("google_account"), "google_signup");
  assert.equal(setupBrowserTarget("opencode_registration"), "invite");
  assert.equal(setupBrowserTarget("payment"), "console");
  assert.equal(setupBrowserTarget("key_verification"), "console");
});

test("OpenCode invite URLs are HTTPS, credential-free, bounded, and host allowlisted", () => {
  // The demo default must itself pass the allowlist unchanged.
  assert.equal(
    normalizeOpenCodeInviteUrl(DEFAULT_OPENCODE_INVITE_URL),
    DEFAULT_OPENCODE_INVITE_URL,
  );
  assert.equal(normalizeOpenCodeInviteUrl("  "), "");
  assert.equal(
    normalizeOpenCodeInviteUrl("https://opencode.ai/invite/demo"),
    "https://opencode.ai/invite/demo",
  );
  assert.equal(
    normalizeOpenCodeInviteUrl("https://console.opencode.ai/register?invite=demo"),
    "https://console.opencode.ai/register?invite=demo",
  );
  assert.throws(() => normalizeOpenCodeInviteUrl("http://opencode.ai/invite"), /HTTPS/);
  assert.throws(() => normalizeOpenCodeInviteUrl("https://user:pass@opencode.ai/invite"), /用户名或密码/);
  assert.throws(() => normalizeOpenCodeInviteUrl("https://opencode.ai.example/invite"), /域名/);
  assert.throws(() => normalizeOpenCodeInviteUrl(`https://opencode.ai/${"x".repeat(2049)}`), /2048/);
});

test("remote browser view URL preserves dashboard location and carries the opaque session token", () => {
  assert.equal(
    browserViewUrl("https://mgr.example/dashboard/?view=accounts", "abc/123"),
    "https://mgr.example/dashboard/?view=browser#session=abc%2F123",
  );
});

test("managed account UI isolates pending controls and renders noVNC in a dedicated view", async () => {
  const [accounts, card, chooser, wizard, browser, app] = await Promise.all([
    readSource(new URL("../views/Accounts.vue", import.meta.url)),
    readSource(new URL("../components/AccountCard.vue", import.meta.url)),
    readSource(new URL("../components/AccountAddModal.vue", import.meta.url)),
    readSource(new URL("../components/ManagedAccountWizard.vue", import.meta.url)),
    readSource(new URL("../views/BrowserSession.vue", import.meta.url)),
    readSource(new URL("../App.vue", import.meta.url)),
  ]);

  assert.match(chooser, /导入已有 Key/);
  assert.match(chooser, /注册新账号/);
  assert.match(chooser, /注册新账号（Beta）/);
  assert.match(chooser, /:disabled="!managedAvailable"/);
  assert.match(chooser, /独立 Profile：登录 → 邀请 → 支付 → 验证 Key。/);
  assert.doesNotMatch(chooser, /注册前请先在设置中填写你自己的邀请链接/);
  assert.match(accounts, /请确认邀请链接是你自己的（默认仅演示）。修改后会写入设置。草稿可随时继续。/);
  assert.match(accounts, /managedDraft\.inviteUrl/);
  assert.match(accounts, /ensureInviteUrlSaved/);
  assert.match(accounts, /canCreateManagedDraft/);
  assert.match(wizard, /注册新账号：\{name\}/);
  assert.match(wizard, /n-tag[\s\S]*?Beta/);
  assert.doesNotMatch(wizard, /托管注册与独立浏览器 Profile 为 Beta 功能/);
  assert.match(card, /v-if="\(isGo \|\| isOfficialCn\) && accountIsReady\(account\)"[\s\S]*?@click="emit\('refresh-usage'\)/);
  assert.match(card, /v-if="accountIsReady\(account\)"[\s\S]*?<n-switch/);
  assert.match(accounts, /loaded\.filter\(\(account\) => \([\s\S]*?accountIsReady\(account\)[\s\S]*?account\.provider_id === "opencode"[\s\S]*?\)\)/);
  assert.match(accounts, /window\.open\("", "_blank"\)[\s\S]*?remoteTab\.location\.replace/);
  assert.match(wizard, /google_account[\s\S]*?opencode_registration[\s\S]*?payment[\s\S]*?key_verification/);
  assert.doesNotMatch(wizard, /重新打开页面/);
  assert.match(wizard, /goToStep|canGoToStep/);
  const registrationStage = wizard.slice(
    wizard.indexOf(`account.setup_step === 'opencode_registration'`),
    wizard.indexOf(`account.setup_step === 'payment'`),
  );
  const paymentStage = wizard.slice(
    wizard.indexOf(`account.setup_step === 'payment'`),
    wizard.indexOf(`account.setup_step === 'key_verification'`),
  );
  assert.match(registrationStage, /openBrowser', 'invite'/);
  assert.match(registrationStage, /我已完成登录\/注册/);
  assert.match(paymentStage, /openBrowser', 'console'/);
  assert.match(paymentStage, /打开控制台/);
  const googleStage = wizard.slice(
    wizard.indexOf(`account.setup_step === 'google_account'`),
    wizard.indexOf(`account.setup_step === 'opencode_registration'`),
  );
  assert.match(googleStage, /openBrowser', 'google_signup'/);
  assert.match(googleStage, /openBrowser', 'github_signup'/);
  assert.match(googleStage, /跳过此步/);
  assert.doesNotMatch(googleStage, /打开 Google 登录/);
  assert.doesNotMatch(accounts, /skipGoogle/);
  assert.doesNotMatch(wizard, /不代填密码、不代点支付/);
  assert.match(accounts, /仅作备注/);
  assert.match(browser, /from "@novnc\/novnc"/);
  assert.match(browser, /clipboardPasteFrom/);
  assert.match(browser, /const client = rfb;[\s\S]*?rfb = null;[\s\S]*?client\.disconnect\(\)/);
  assert.doesNotMatch(browser, /detail\.clean/);
  const initialization = accounts.slice(
    accounts.indexOf("async function initializeAccounts"),
    accounts.indexOf("async function onFormSave"),
  );
  assert.ok(initialization.indexOf("loadRegistrationOptions()") < initialization.indexOf("await loadQuotaLimits()"));
  assert.ok(initialization.indexOf("await loadAccounts()") < initialization.indexOf("Promise.allSettled([registrationOptions"));
  assert.match(accounts, /recoverManagedSetupConflict\(accountId, error\)/);
  assert.match(app, /window\.location\.hash\.slice\(1\)[\s\S]*?sanitizedBrowserUrl\.hash = ""[\s\S]*?history\.replaceState/);
});

test("managed account creation cannot be cancelled while its request is pending", async () => {
  const accounts = await readSource(new URL("../views/Accounts.vue", import.meta.url));
  const managedCreateModal = accounts.slice(
    accounts.indexOf('<n-modal\n      :show="showManagedCreate"'),
    accounts.indexOf("<ManagedAccountWizard"),
  );

  assert.match(managedCreateModal, /:close-on-esc="!busy"/);
  assert.match(managedCreateModal, /@update:show="setManagedCreateVisible"/);
  assert.match(managedCreateModal, /<n-button :disabled="busy" @click="setManagedCreateVisible\(false\)">\s*\{\{ busy \? t\("加载中…"\) : t\("取消"\) \}\}/);
  assert.match(accounts, /function setManagedCreateVisible\(show: boolean\): void \{\s*if \(!show && busy\.value\) return;\s*showManagedCreate\.value = show;/);
});

test("Vite proxies remote browser WebSockets during dashboard development", async () => {
  const source = await readSource(new URL("../../vite.config.ts", import.meta.url));
  assert.match(source, /OCG_GATEWAY_PORT/);
  assert.match(source, /"\/dashboard\/api": \{[\s\S]*?target: `http:\/\/127\.0\.0\.1:\$\{gatewayPort\}`[\s\S]*?ws: true/);
});
