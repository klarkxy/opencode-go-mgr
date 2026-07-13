import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { connectionApiUrl, maskConnectionKey, writeConnectionValue } from "./dashboard-connection.ts";

test("connection helpers mask display values and copy the complete value", async () => {
  assert.equal(maskConnectionKey(""), "未设置");
  assert.equal(maskConnectionKey("ocg-1234567890"), "ocg-…7890");
  assert.equal(connectionApiUrl("http://127.0.0.1:30001", 9042, true), "http://127.0.0.1:9042/v1");
  assert.equal(connectionApiUrl("https://ocg.example.com", 9042, false), "https://ocg.example.com/v1");

  let copied = "";
  await writeConnectionValue(async (value) => { copied = value; }, "ocg-secret-value");
  assert.equal(copied, "ocg-secret-value");
  await assert.rejects(() => writeConnectionValue(undefined, "value"), /剪贴板/);
});

test("dashboard keeps the connection center first and protects key regeneration", async () => {
  const source = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const template = source.slice(source.indexOf("<template>"), source.indexOf("<script setup"));

  assert.ok(template.indexOf("接入中心") < template.indexOf("kpi-row"));
  assert.match(template, /旧 Key 将立即失效/);
  assert.match(template, /aria-label="复制 API 地址"/);
  assert.match(template, /aria-label="刷新 Key"/);
  assert.doesNotMatch(template, /Gateway Key/);
});

test("dashboard and settings keep partial data safe", async () => {
  const dashboard = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const settings = await readFile(new URL("./Settings.vue", import.meta.url), "utf8");
  const app = await readFile(new URL("../App.vue", import.meta.url), "utf8");

  assert.match(dashboard, /Promise\.allSettled/);
  assert.match(settings, /:disabled="!loaded \|\| regenerating"/);
  assert.match(settings, /if \(!loaded\.value\) return/);
  assert.match(app, /mode === "register"[\s\S]*getAuthStatus\(\)[\s\S]*status\?\.initialized/);
});

test("settings expose bounded request timeouts", async () => {
  const settings = await readFile(new URL("./Settings.vue", import.meta.url), "utf8");
  const api = await readFile(new URL("../api/tauri.ts", import.meta.url), "utf8");
  const dashboard = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");

  assert.match(settings, /请求超时/);
  assert.match(settings, /config\.connect_timeout_secs"\s+:min="1"\s+:max="300"\s+:precision="0"/);
  assert.match(settings, /config\.non_stream_timeout_secs"\s+:min="1"\s+:max="3600"\s+:precision="0"/);
  assert.match(settings, /config\.stream_idle_timeout_secs"\s+:min="1"\s+:max="3600"\s+:precision="0"/);
  assert.match(settings, /connect_timeout_secs: 30/);
  assert.match(settings, /non_stream_timeout_secs: 120/);
  assert.match(settings, /stream_idle_timeout_secs: 300/);
  assert.match(settings, /if \(!timeoutsValid\(\)\)/);
  assert.match(api, /connect_timeout_secs: number/);
  assert.match(api, /non_stream_timeout_secs: number/);
  assert.match(api, /stream_idle_timeout_secs: number/);
  assert.doesNotMatch(dashboard, /ref<AppConfig>/);
});

test("settings expose supported Windows auto-start safely", async () => {
  const settings = await readFile(new URL("./Settings.vue", import.meta.url), "utf8");
  const api = await readFile(new URL("../api/tauri.ts", import.meta.url), "utf8");

  assert.match(settings, /v-if="config\.auto_start_supported"/);
  assert.match(settings, /v-model:value="config\.auto_start"/);
  assert.match(settings, /登录 Windows 后在托盘后台启动，不自动打开 Dashboard/);
  assert.match(settings, /aria-label="随 Windows 登录自动启动 OCG Manager"/);
  assert.match(settings, /aria-describedby="startup-help"/);
  assert.match(settings, /:disabled="!loaded \|\| saving \|\| regenerating"/);
  assert.match(settings, /:loading="regenerating"\s+:disabled="saving"/);
  assert.match(settings, /:disabled="!loaded \|\| regenerating" @click="saveSettings"/);
  assert.match(settings, /config\.value\.auto_start = persistedAutoStart/);
  assert.match(api, /auto_start_supported: boolean/);
});
