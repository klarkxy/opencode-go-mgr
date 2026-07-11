import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { maskConnectionKey, writeConnectionValue } from "./dashboard-connection.ts";

test("connection helpers mask display values and copy the complete value", async () => {
  assert.equal(maskConnectionKey(""), "未设置");
  assert.equal(maskConnectionKey("ocg-1234567890"), "ocg-…7890");

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
