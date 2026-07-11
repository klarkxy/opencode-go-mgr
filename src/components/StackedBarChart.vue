<!--
  组合式 (self-contained) 堆叠柱状图。纯 SVG + Vue computed,零第三方依赖。
  设计参考 Vercel / Linear / Stripe 现代克制的 dashboard 风格:
    - 细网格线、克制的坐标轴
    - 圆角柱条、垂直渐变填充
    - hover 显示当日各模型明细 tooltip
    - 图例使用小圆点 + 模型名,颜色按模型稳定分配
  颜色使用共享色板；第一系列跟随当前主题强调色，其余系列保持固定。
-->
<template>
  <div ref="rootRef" class="stacked-bar-chart">
    <svg
      :viewBox="`0 0 ${width} ${height}`"
      :width="width"
      :height="height"
      preserveAspectRatio="xMidYMid meet"
      class="chart-svg"
      role="img"
      :aria-labelledby="`chart-title-${gid}`"
    >
      <title :id="`chart-title-${gid}`">最近 30 天按模型分段的每日消耗</title>
      <defs>
        <linearGradient
          v-for="(c, idx) in CHART_PALETTE"
          :id="`bar-grad-${idx}-${gid}`"
          :key="idx"
          x1="0"
          y1="0"
          x2="0"
          y2="1"
        >
          <stop offset="0%" :stop-color="c" stop-opacity="0.95" />
          <stop offset="100%" :stop-color="c" stop-opacity="0.85" />
        </linearGradient>
      </defs>

      <!-- 横向网格线 + Y 轴刻度 -->
      <g class="grid">
        <line
          v-for="t in yTicks"
          :key="`g-${t.value}`"
          :x1="padL"
          :x2="width - padR"
          :y1="t.y"
          :y2="t.y"
          class="grid-line"
        />
        <text
          v-for="t in yTicks"
          :key="`y-${t.value}`"
          :x="padL - 8"
          :y="t.y + 3"
          text-anchor="end"
          class="axis-text"
        >{{ t.label }}</text>
      </g>

      <!-- 柱条 -->
      <g class="bars">
        <g
          v-for="(bar, bi) in bars"
          :key="`col-${bi}`"
          class="bar-col"
          :transform="`translate(${bar.x}, 0)`"
          @mouseenter="onEnter(bi, $event)"
          @mousemove="onMove(bi, $event)"
          @mouseleave="onLeave"
        >
          <rect
            v-for="(seg, si) in bar.segments"
            :key="si"
            :x="2"
            :y="seg.y"
            :width="barWidth - 4"
            :height="seg.h"
            :fill="`url(#bar-grad-${seg.idx}-${gid})`"
            :rx="si === 0 ? 3 : 0"
            :ry="si === bar.segments.length - 1 ? 3 : 0"
            class="bar-seg"
          />
          <!-- 透明 hit-box 让整列都可 hover,即使柱条之间有间隙 -->
          <rect
            :x="0"
            :y="padT"
            :width="barWidth"
            :height="chartH"
            fill="transparent"
          />
        </g>
      </g>

      <!-- X 轴日期 -->
      <g class="x-axis">
        <text
          v-for="(label, i) in xLabels"
          :key="`x-${i}`"
          :x="label.x"
          :y="height - padB + 16"
          text-anchor="middle"
          class="axis-text"
        >{{ label.text }}</text>
      </g>
    </svg>

    <!-- tooltip -->
    <div
      v-if="tooltip.show"
      class="chart-tooltip"
      :style="{ left: tooltip.x + 'px', top: tooltip.y + 'px' }"
    >
      <div class="tooltip-title">{{ tooltip.title }}</div>
      <div class="tooltip-total">合计 ${{ tooltip.total.toFixed(4) }}</div>
      <div
        v-for="row in tooltip.rows"
        :key="row.model"
        class="tooltip-row"
      >
        <span class="dot" :style="{ background: row.color }" />
        <span class="model">{{ row.model }}</span>
        <span class="cost">${{ row.cost.toFixed(4) }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, useId } from "vue";
import type { DailyModelCost } from "../api/tauri";
import { CHART_PALETTE } from "../theme";

const props = withDefaults(defineProps<{
  data: DailyModelCost[];
  days?: number; // 实际展示的天数(用于补零)
}>(), {
  days: 30,
});

// --- 布局常量 ---
const padL = 48;
const padR = 16;
const padT = 16;
const padB = 28;
const width = ref(720);
const height = 280;
const gid = useId(); // 渐变 id 唯一化,避免多实例冲突

const rootRef = ref<HTMLElement | null>(null);

function measureWidth() {
  if (!rootRef.value) return;
  // 组合图表宽度跟随容器,但有最小值,避免窄屏柱条挤成线
  const w = rootRef.value.clientWidth;
  if (w > 0) width.value = Math.max(480, w);
}

let ro: ResizeObserver | null = null;
onMounted(() => {
  measureWidth();
  if (typeof ResizeObserver !== "undefined" && rootRef.value) {
    ro = new ResizeObserver(() => measureWidth());
    ro.observe(rootRef.value);
  }
});
onBeforeUnmount(() => {
  ro?.disconnect();
});

function modelColor(model: string, models: string[]): string {
  const idx = models.indexOf(model);
  return CHART_PALETTE[idx % CHART_PALETTE.length];
}

// --- 数据处理:按日期补零,得到连续的日期序列 ---
function padZeroDates(rows: DailyModelCost[], days: number) {
  const map = new Map<string, Map<string, number>>();
  for (const r of rows) {
    if (!map.has(r.date)) map.set(r.date, new Map());
    const m = map.get(r.date)!;
    m.set(r.model, (m.get(r.model) ?? 0) + r.cost);
  }
  // 生成最近 `days` 天的日期(UTC),缺失的天用空 map 填充
  const today = new Date();
  const dates: { date: string; models: Map<string, number>; total: number }[] = [];
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(today);
    d.setUTCDate(today.getUTCDate() - i);
    const ds = d.toISOString().slice(0, 10);
    const models = map.get(ds) ?? new Map<string, number>();
    let total = 0;
    models.forEach((v) => (total += v));
    dates.push({ date: ds, models, total });
  }
  return dates;
}

// 模型稳定排序(按总量 desc),保证图例顺序稳定
const sortedModels = computed(() => {
  const totals = new Map<string, number>();
  for (const r of props.data) {
    totals.set(r.model, (totals.get(r.model) ?? 0) + r.cost);
  }
  return [...totals.keys()].sort((a, b) => (totals.get(b)! - totals.get(a)!));
});

const dates = computed(() => padZeroDates(props.data, props.days));

const chartW = computed(() => Math.max(0, width.value - padL - padR));
const chartH = height - padT - padB;

const maxCost = computed(() => {
  let m = 0;
  for (const d of dates.value) if (d.total > m) m = d.total;
  if (m === 0) m = 0.01; // 避免除零
  return m;
});

// 'nice' 的 Y 轴上限:向上取整到一个可读刻度
function niceCeil(v: number): number {
  if (v <= 0) return 1;
  const pow = Math.pow(10, Math.floor(Math.log10(v)));
  const n = v / pow;
  let nice: number;
  if (n <= 1) nice = 1;
  else if (n <= 2) nice = 2;
  else if (n <= 5) nice = 5;
  else nice = 10;
  return nice * pow;
}

const ceil = computed(() => niceCeil(maxCost.value));

const yTicks = computed(() => {
  const steps = 4;
  const out: { value: number; y: number; label: string }[] = [];
  for (let i = 0; i <= steps; i++) {
    const val = (ceil.value * i) / steps;
    const y = padT + chartH - (val / ceil.value) * chartH;
    out.push({
      value: val,
      y,
      label: val < 0.001 ? "$0" : `$${val < 1 ? val.toFixed(3) : val.toFixed(2)}`,
    });
  }
  return out;
});

const barWidth = computed(() => {
  const n = dates.value.length || 1;
  return chartW.value / n;
});

// 每根柱子: [{model, idx, y, h, cost}]
const bars = computed(() => {
  const models = sortedModels.value;
  const scale = chartH / ceil.value;
  return dates.value.map((d, i) => {
    let cursor = padT + chartH; // 从底往上堆
    const segments: { idx: number; model: string; y: number; h: number; cost: number }[] = [];
    // 按 sortedModels 顺序堆叠,保证颜色块在所有柱子里对齐
    for (const model of models) {
      const cost = d.models.get(model) ?? 0;
      if (cost <= 0) continue;
      const h = cost * scale;
      cursor -= h;
      segments.push({
        idx: models.indexOf(model),
        model,
        y: cursor,
        h: Math.max(0.5, h),
        cost,
      });
    }
    return { x: padL + barWidth.value * i, segments };
  });
});

// X 轴标签:太密时跳着显示,大约每 5~7 天一个标签
const xLabels = computed(() => {
  const n = dates.value.length;
  if (n === 0) return [];
  // 目标最多 ~6 个标签
  const step = Math.max(1, Math.round(n / 6));
  const out: { x: number; text: string }[] = [];
  for (let i = 0; i < n; i += step) {
    const ds = dates.value[i].date;
    // MM-DD
    const text = ds.slice(5);
    out.push({ x: padL + barWidth.value * (i + 0.5), text });
  }
  return out;
});

// --- tooltip ---
const tooltip = ref<{ show: boolean; x: number; y: number; title: string; total: number; rows: { model: string; cost: number; color: string }[] }>({
  show: false,
  x: 0,
  y: 0,
  title: "",
  total: 0,
  rows: [],
});

function onEnter(bi: number, e: MouseEvent) {
  updateTooltip(bi, e);
}
function onMove(bi: number, e: MouseEvent) {
  updateTooltip(bi, e);
}
function onLeave() {
  tooltip.value.show = false;
}

function updateTooltip(bi: number, e: MouseEvent) {
  const d = dates.value[bi];
  if (!d) return;
  const models = sortedModels.value;
  const rows = models
    .map((m) => ({ model: m, cost: d.models.get(m) ?? 0 }))
    .filter((r) => r.cost > 0)
    .sort((a, b) => b.cost - a.cost)
    .map((r) => ({ ...r, color: modelColor(r.model, models) }));
  tooltip.value = {
    show: true,
    title: d.date,
    total: d.total,
    rows,
    x: e.offsetX + 14,
    y: e.offsetY + 14,
  };
}

</script>

<style scoped>
.stacked-bar-chart {
  position: relative;
  width: 100%;
}
.chart-svg {
  display: block;
  width: 100%;
  height: auto;
  font-family: v-sans, system-ui, -apple-system, sans-serif;
}
.grid-line {
  stroke: rgba(127, 127, 127, 0.18);
  stroke-width: 1;
  shape-rendering: crispEdges;
}
.axis-text {
  fill: var(--n-text-color-3, rgba(127, 127, 127, 0.9));
  font-size: 11px;
}
.bar-seg {
  transition: opacity 0.15s ease;
}
.bar-col:hover .bar-seg {
  opacity: 0.82;
}
.chart-tooltip {
  position: absolute;
  pointer-events: none;
  z-index: 5;
  min-width: 168px;
  padding: 8px 10px;
  border-radius: 8px;
  background: var(--n-color-popover, #fff);
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.12);
  font-size: 12px;
}
.tooltip-title {
  font-weight: 600;
  margin-bottom: 2px;
}
.tooltip-total {
  color: var(--n-text-color-3, #888);
  margin-bottom: 6px;
  font-size: 11px;
}
.tooltip-row {
  display: flex;
  align-items: center;
  gap: 6px;
  line-height: 18px;
}
.tooltip-row .dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: 0 0 auto;
}
.tooltip-row .model {
  flex: 1 1 auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tooltip-row .cost {
  flex: 0 0 auto;
  font-variant-numeric: tabular-nums;
  color: var(--n-text-color-2, #555);
}
</style>
