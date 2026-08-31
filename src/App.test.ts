import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const app = readFileSync(new URL("./App.vue", import.meta.url), "utf8");

test("mobile navigation exposes every sidebar page without responsive overflow", () => {
  assert.match(app, /<n-layout-sider[\s\S]*?<n-menu[\s\S]*:options="menuOptions"/);
  assert.match(app, /<n-dropdown[\s\S]*class="mobile-nav-dropdown"[\s\S]*:options="mobileMenuOptions"/);
  assert.doesNotMatch(app, /<n-menu\s+mode="horizontal"\s+responsive/);
  assert.match(app, /const mobileMenuOptions = computed<DropdownOption\[\]>/);
  assert.match(app, /CORE_APP_NAVIGATION\.map\(mobileMenuOption\)[\s\S]*mobile-external-integrations-divider[\s\S]*EXTERNAL_APP_NAVIGATION\.map\(mobileMenuOption\)/);
  assert.match(app, /function selectMobileView\(key: string \| number\)[\s\S]*mobileMenuShown\.value = false;[\s\S]*selectView\(String\(key\)\)/);
  assert.match(app, /aria-haspopup="menu"/);
  assert.match(app, /:aria-expanded="mobileMenuShown"/);
  assert.match(app, /"aria-checked": item\.key === activeKey\.value/);
});

test("account cards stay focused on account state instead of provider contracts", () => {
  const accounts = readFileSync(new URL("./views/Accounts.vue", import.meta.url), "utf8");
  assert.match(app, /<Accounts v-else-if="activeKey === 'accounts'" \/>/);
  assert.doesNotMatch(accounts, /openProvider|contractSummary|providerContracts/);
  assert.match(app, /<KeepAlive>/);
});
