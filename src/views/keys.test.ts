import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

test("the keys page owns lifecycle CRUD and does not render plaintext values", async () => {
  const source = await readFile(new URL("./Keys.vue", import.meta.url), "utf8");
  const template = source.slice(source.indexOf("<template>"), source.indexOf("<script setup"));

  assert.match(template, /id="gateway-keys-title"/);
  assert.match(template, /class="gateway-key-row gateway-key-row--primary"/);
  assert.match(template, /v-for="entry in connection\.sub_keys"/);
  assert.match(template, /t\(['"]新建 Key['"]\)/);
  assert.match(template, />\{\{ t\("新建"\) \}\}<\/n-button>/);
  assert.match(template, /class="gateway-key-actions"/);
  assert.match(template, /class="gateway-key-split"/);
  assert.ok(
    template.indexOf("t('复制 Key')") < template.indexOf("t('刷新 Key')")
      && template.lastIndexOf("t('刷新 Key')") < template.lastIndexOf("t('启用或停用 Key')")
      && template.lastIndexOf("t('启用或停用 Key')") < template.lastIndexOf("t('删除 Key')"),
  );
  assert.doesNotMatch(template, /t\("保存主 Key 值"\)/);
  assert.doesNotMatch(template, /v-model:value="primaryKeyDraft"/);
  assert.doesNotMatch(template, /t\("自定义主 Key 值"\)/);
  assert.match(template, /\{\{ maskConnectionKey\(connection\.primary_key\) \}\}/);
  assert.match(template, /\{\{ maskConnectionKey\(entry\.value\) \}\}/);
  assert.doesNotMatch(template, /<code>\{\{ connection\.primary_key \}\}<\/code>/);
  assert.doesNotMatch(template, /<code>\{\{ entry\.value \}\}<\/code>/);
});

test("the keys page uses ConnectionInfo and resets the primary key instead of editing it", async () => {
  const source = await readFile(new URL("./Keys.vue", import.meta.url), "utf8");

  assert.match(source, /useConnectionStore\(\)/);
  assert.match(source, /computed\(\(\) => connectionStore\.info \?\? EMPTY_CONNECTION\)/);
  assert.doesNotMatch(source, /ref<AppConfig>/);
  assert.match(source, /connectionStore\.load\(\)/);
  assert.match(source, /connectionStore\.createKey\(name\)/);
  assert.match(source, /connectionStore\.updateKey\(entry\.id, \{ enabled \}\)/);
  assert.match(source, /connectionStore\.deleteKey\(entry\.id\)/);
  assert.match(source, /connectionStore\.regeneratePrimaryKey\(\)/);
  assert.match(source, /connectionStore\.regenerateKey\(entry\.id\)/);
  assert.doesNotMatch(source, /dashboardApi\.getSettings\(\)|dashboardApi\.updateSettings\(/);
  assert.match(source, /onActivated\(\(\) => \{\s*if \(!loading\.value\) void loadConnection\(\);/);
  assert.doesNotMatch(source, /validatePrimaryKey\(\)|savePrimaryKey|primaryKeyDraft/);
});

test("the dashboard consume surface does not host key lifecycle controls", async () => {
  const dashboard = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const template = dashboard.slice(dashboard.indexOf("<template>"), dashboard.indexOf("<script setup"));

  assert.match(template, /t\("管理接入 Key"\)/);
  assert.doesNotMatch(template, /t\("新建 Key"\)/);
  assert.doesNotMatch(template, /t\("删除 Key"\)/);
  assert.doesNotMatch(template, /t\("启用或停用 Key"\)/);
  assert.doesNotMatch(template, /t\("自定义主 Key 值"\)/);
});

test("app registers the keys view between dashboard and accounts", async () => {
  const app = await readFile(new URL("../App.vue", import.meta.url), "utf8");
  const navigation = await readFile(new URL("./app-navigation.ts", import.meta.url), "utf8");

  assert.match(app, /type ViewKey = AppViewKey/);
  assert.match(app, /<Keys v-else-if="activeKey === 'keys'" \/>/);
  assert.match(app, /<Dashboard v-if="activeKey === 'dashboard'" @navigate="selectView" \/>/);
  assert.match(app, /import\("\.\/views\/Keys\.vue"\)/);
  assert.match(app, /CORE_APP_NAVIGATION\.map\(menuOption\)/);
  assert.ok(
    navigation.indexOf('{ key: "dashboard"')
      < navigation.indexOf('{ key: "keys", label: "接入 Key"')
      && navigation.indexOf('{ key: "keys", label: "接入 Key"')
        < navigation.indexOf('{ key: "accounts", label: "账号"'),
  );
});
