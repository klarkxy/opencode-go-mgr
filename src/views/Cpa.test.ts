import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const cpa = readFileSync(new URL("./Cpa.vue", import.meta.url), "utf8");
const api = readFileSync(new URL("../api/dashboard-v3.ts", import.meta.url), "utf8");
const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");

test("CPA page uses only typed V3 endpoints and write-only key drafts", () => {
  assert.match(api, /getCpaIntegration[\s\S]*?\/external-integrations\/cpa/);
  assert.match(api, /startCpaOAuth[\s\S]*?\/external-integrations\/cpa\/oauth\/start/);
  assert.match(api, /deleteCpaAccount[\s\S]*?\/external-integrations\/cpa\/accounts/);
  assert.doesNotMatch(api, /management\/raw|managementPath|rawManagement/);
  assert.match(cpa, /draft\.value\.inferenceKey = ""/);
  assert.match(cpa, /draft\.value\.managementKey = ""/);
  assert.match(cpa, /baseUrlReadOnly/);
});

test("CPA OAuth polls at three seconds and is cancelled when the kept-alive page leaves", () => {
  assert.match(cpa, /window\.setInterval\(\(\) => void pollOAuth\(\), 3000\)/);
  assert.match(cpa, /onDeactivated\(cancelOAuthOnLeave\)/);
  assert.match(cpa, /addEventListener\("pagehide", cancelOAuthOnLeave\)/);
  assert.match(cpa, /onBeforeUnmount\(\(\) => \{/);
  assert.match(cpa, /cancelCpaOAuth/);
  assert.match(cpa, /runtimeOnly[\s\S]*?mutable/);
  assert.match(cpa, /if \(oauth\.value \|\| oauthStartingProvider\.value\) return/);
  assert.match(cpa, /:loading="oauthStartingProvider === provider\.id"/);
});

test("the CPA account singleton stays in account ordering but exposes only its CPA jump action", () => {
  assert.match(accounts, /function openCpa[\s\S]*?searchParams\.set\("view", "cpa"\)/);
  assert.match(card, /v-if="isCpa"[\s\S]*?CPA 订阅池/);
  assert.match(card, /v-if="!isCpa" class="account-action account-action--test"/);
  assert.match(card, /const isCpa = computed/);
});
