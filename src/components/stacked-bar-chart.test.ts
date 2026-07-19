import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("chart tooltip is teleported out of the clipped card and keeps a bounded width", async () => {
  const source = await readFile(new URL("./StackedBarChart.vue", import.meta.url), "utf8");
  assert.match(source, /\.chart-tooltip\s*\{[^}]*max-width:\s*200px/);
  assert.match(source, /<Teleport to="body">/);
  assert.match(source, /\.chart-tooltip\s*\{[^}]*position:\s*fixed/);
});

test("chart tooltip clamps both axes to the viewport using rendered dimensions", async () => {
  const source = await readFile(new URL("./StackedBarChart.vue", import.meta.url), "utf8");
  assert.match(source, /ref="tooltipRef"/);
  assert.match(source, /document\.documentElement\.clientWidth\s*-\s*tip\.offsetWidth/);
  assert.match(source, /document\.documentElement\.clientHeight\s*-\s*tip\.offsetHeight/);
  assert.match(source, /tooltip\.value\.y\s*=\s*Math\.min/);
});

test("x-axis labels always include the last day so today gets a label", async () => {
  const source = await readFile(new URL("./StackedBarChart.vue", import.meta.url), "utf8");
  // 回归:30 天 step=5 会落在 0/5/10/15/20/25,跳过 index 29 (today),需补一行兜底
  assert.match(source, /lastIndex % step !== 0/);
  assert.match(source, /dates\.value\[lastIndex\]\.date/);
});

test("dashboard chart card does not enable horizontal scroll for the chart", async () => {
  const source = await readFile(new URL("../views/Dashboard.vue", import.meta.url), "utf8");
  // overflow-x: auto 会和 ResizeObserver 形成反馈循环 → 图表抽搐
  assert.doesNotMatch(source, /\.chart-card :deep\(\.n-spin-content\)[^{]*\{[^}]*overflow-x:\s*auto/);
  assert.match(source, /\.chart-card :deep\(\.n-spin-content\)[^{]*\{[^}]*overflow:\s*hidden/);
});
