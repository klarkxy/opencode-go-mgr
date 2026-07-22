<template>
  <section class="logs-card">
    <n-tabs v-model:value="activeTab" type="line" animated>
      <n-tab-pane name="gateway" :tab="t('运行日志')">
        <div class="log-toolbar">
          <n-input
            v-model:value="requestIdFilter"
            clearable
            class="request-id-filter"
            :placeholder="t('按请求 ID 精确搜索')"
          />
          <n-tooltip trigger="hover">
            <template #trigger>
              <n-button
                circle
                quaternary
                :loading="gatewayLoading"
                :aria-label="t('刷新运行日志')"
                @click="loadGatewayLogs"
              >
                <template #icon><n-icon :component="ReloadOutlined" /></template>
              </n-button>
            </template>
            {{ t("刷新运行日志") }}
          </n-tooltip>
        </div>
        <n-alert v-if="gatewayError" type="error" :title="t('加载运行日志失败: {error}', { error: gatewayError })">
          <n-button size="small" secondary @click="loadGatewayLogs">{{ t("重试") }}</n-button>
        </n-alert>
        <p class="log-limit-note">{{ t("仅显示最近 {count} 条运行日志", { count: 200 }) }}</p>
        <n-data-table
          :columns="gatewayColumns"
          :data="gatewayLogs"
          :row-key="logRowKey"
          :loading="gatewayLoading"
          :pagination="gatewayPagination"
          :scroll-x="1200"
          size="small"
          @update:page="changeGatewayPage"
        />
      </n-tab-pane>
      <n-tab-pane name="forward" :tab="t('请求日志')">
        <div class="stats-row">
          <div class="stat-card">
            <div class="stat-label">{{ t("请求数") }}</div>
            <div class="stat-value">{{ formatNumber(forwardTotals.total_requests) }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">{{ t("输入") }}</div>
            <div class="stat-value">{{ formatNumber(forwardTotals.prompt_tokens) }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">{{ t("输出") }}</div>
            <div class="stat-value">{{ formatNumber(forwardTotals.completion_tokens) }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">{{ t("缓存") }}</div>
            <div class="stat-value">{{ formatNumber(forwardTotals.cached_tokens) }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">{{ t("额度消耗（估算）") }}</div>
            <div class="stat-value">{{ formatCost(forwardTotals.cost, 5) }}</div>
          </div>
        </div>
        <div class="filter-bar">
          <div class="filter-field request-id-field">
            <span class="filter-label">{{ t("请求 ID") }}</span>
            <n-input
              v-model:value="requestIdFilter"
              clearable
              :placeholder="t('按请求 ID 精确搜索')"
            />
          </div>
          <div class="filter-field">
            <span class="filter-label">{{ t("状态") }}</span>
            <n-select
              v-model:value="statusFilter"
              :options="statusOptions"
              :placeholder="t('状态')"
            />
          </div>
          <div class="filter-field">
            <span class="filter-label">{{ t("账号") }}</span>
            <n-select
              v-model:value="accountFilter"
              :options="accountOptions"
              :placeholder="t('账号')"
            />
          </div>
          <div class="filter-field">
            <span class="filter-label">{{ t("模型") }}</span>
            <n-select
              v-model:value="modelFilter"
              :options="modelOptions"
              :placeholder="t('模型')"
            />
          </div>
          <div class="filter-field time-range-field">
            <span class="filter-label">{{ t("时间范围") }}</span>
            <n-popover
              trigger="click"
              placement="bottom-start"
              :show="showTimePanel"
              @update:show="showTimePanel = $event"
            >
              <template #trigger>
                <n-button class="time-range-trigger">
                  <template #icon>
                    <n-icon :component="CalendarOutlined" />
                  </template>
                  {{ timeRangeLabel }}
                </n-button>
              </template>
              <div class="time-range-panel">
                <div class="preset-list">
                  <n-button
                    v-for="item in timePresetOptions"
                    :key="item.value"
                    quaternary
                    :type="activePreset === item.value ? 'primary' : 'default'"
                    class="preset-item"
                    @click="applyTimePreset(item.value)"
                  >
                    {{ item.label }}
                  </n-button>
                </div>
                <div
                  class="custom-range-wrapper"
                  :class="{ 'is-visible': activePreset === 'custom' }"
                >
                  <span class="custom-range-title">{{ t("自定义范围") }}</span>
                  <n-date-picker
                    v-model:value="customTimeRange"
                    type="daterange"
                    :panel="true"
                    :actions="null"
                    class="custom-time-picker"
                    @update:value="applyCustomTimeRange"
                  />
                </div>
              </div>
            </n-popover>
          </div>
          <div class="filter-field">
            <span class="filter-label">{{ t("排序") }}</span>
            <n-select
              v-model:value="sortBy"
              :options="sortOptions"
              :placeholder="t('排序')"
              :consistent-menu-width="false"
              class="sort-select"
            />
          </div>
          <n-tooltip trigger="hover">
            <template #trigger>
              <n-button
                circle
                quaternary
                :aria-label="sortOrder === 'asc' ? t('升序') : t('降序')"
                @click="toggleSortOrder"
              >
                <template #icon>
                  <n-icon :component="sortOrder === 'asc' ? ArrowUpOutlined : ArrowDownOutlined" />
                </template>
              </n-button>
            </template>
            {{ sortOrder === "asc" ? t("升序") : t("降序") }}
          </n-tooltip>
          <div class="filter-actions">
            <n-tooltip v-if="hasFilters" trigger="hover">
              <template #trigger>
                <n-button circle quaternary :aria-label="t('清除筛选')" @click="clearFilters">
                  <template #icon><n-icon :component="ClearOutlined" /></template>
                </n-button>
              </template>
              {{ t("清除筛选") }}
            </n-tooltip>
            <n-tooltip trigger="hover">
              <template #trigger>
                <n-button
                  circle
                  quaternary
                  :loading="forwardLoading"
                  :aria-label="t('刷新请求日志')"
                  @click="refreshForwardLogs"
                >
                  <template #icon><n-icon :component="ReloadOutlined" /></template>
                </n-button>
              </template>
              {{ t("刷新请求日志") }}
            </n-tooltip>
          </div>
        </div>
        <n-data-table
          :columns="forwardColumns"
          :data="forwardLogs"
          :row-key="logRowKey"
          :loading="forwardLoading"
          :pagination="forwardPagination"
          :scroll-x="1830"
          remote
          size="small"
          @update:page="changeForwardPage"
        >
          <template #empty>
            <n-empty :description="t('仅记录经本机 API 转发的请求，账号 Ping 见运行日志')" />
          </template>
        </n-data-table>
      </n-tab-pane>
    </n-tabs>
  </section>
</template>

<script setup lang="ts">
import { computed, h, onMounted, onUnmounted, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NDataTable,
  NDatePicker,
  NEmpty,
  NIcon,
  NInput,
  NPopover,
  NSelect,
  NTabPane,
  NTabs,
  NTag,
  NTooltip,
  useMessage,
} from "naive-ui";
import { ArrowDownOutlined, ArrowUpOutlined, CalendarOutlined, CheckOutlined, ClearOutlined, CopyOutlined, ReloadOutlined } from "@vicons/antd";
import { tauriApi } from "../api/tauri";
import type { Account, ForwardLog, ForwardLogSummary, GatewayLog } from "../api/tauri";
import { t } from "../i18n/index.ts";
import { locale } from "../i18n/index.ts";
import { formatCost, formatNumber, useClipboard } from "../utils/format.ts";
import { computeTimeRange, resolveTimeRange, timePresetValues } from "./log-time-range.ts";
import type { TimePreset } from "./log-time-range.ts";

type LogTab = "gateway" | "forward";
type SortBy = "timestamp" | "attempt" | "prompt_tokens" | "completion_tokens" | "cached_tokens" | "cost";
type SortOrder = "asc" | "desc";
const sortValues = new Set<SortBy>([
  "timestamp",
  "attempt",
  "prompt_tokens",
  "completion_tokens",
  "cached_tokens",
  "cost",
]);

const query = new URLSearchParams(window.location.search);
const message = useMessage();
const { copiedTarget, copy, cleanup } = useClipboard();
const activeTab = ref<LogTab>(query.get("tab") === "forward" ? "forward" : "gateway");
const gatewayLogs = ref<GatewayLog[]>([]);
const forwardLogs = ref<ForwardLog[]>([]);
const accounts = ref<Account[]>([]);
const models = ref<string[]>([]);
const gatewayLoading = ref(false);
const gatewayError = ref("");
const forwardLoading = ref(false);
const statusFilter = ref<string>(query.get("status") ?? "");
const accountFilter = ref<string>(query.get("account") ?? "");
const modelFilter = ref<string>(query.get("model") ?? "");
const requestIdFilter = ref<string>(query.get("request_id") ?? "");
const querySort = query.get("sort");
const queryOrder = query.get("order");
const sortBy = ref<SortBy>(
  querySort !== null && sortValues.has(querySort as SortBy) ? querySort as SortBy : "timestamp",
);
const sortOrder = ref<SortOrder>(queryOrder === "asc" || queryOrder === "desc" ? queryOrder : "desc");
function parseQueryTimeRange(): [number, number] | null {
  const start = query.get("start");
  const end = query.get("end");
  if (!start || !end) return null;
  const startMs = Date.parse(start);
  const endMs = Date.parse(end);
  if (Number.isNaN(startMs) || Number.isNaN(endMs) || startMs > endMs) return null;
  return [startMs, endMs];
}

const initialTimeRange = parseQueryTimeRange();
const queryPreset = query.get("range");
const initialPreset: TimePreset = initialTimeRange
  ? "custom"
  : queryPreset !== null
      && queryPreset !== "custom"
      && timePresetValues.has(queryPreset as TimePreset)
    ? queryPreset as TimePreset
    : "last24h";
const initialRange = initialTimeRange ?? resolveTimeRange(initialPreset, null);
const timeRange = ref<[number, number] | null>(initialRange);
const activePreset = ref<TimePreset>(initialPreset);
const customTimeRange = ref<[number, number] | null>(initialRange);
const showTimePanel = ref(false);
const forwardPage = ref(1);
const gatewayPage = ref(1);
const pageSize = 20;
const gatewayPagination = computed(() => ({
  page: gatewayPage.value,
  pageSize,
}));
const emptySummary = (): ForwardLogSummary => ({
  total_requests: 0,
  prompt_tokens: 0,
  completion_tokens: 0,
  cached_tokens: 0,
  cost: 0,
});
const forwardTotals = ref<ForwardLogSummary>(emptySummary());
const forwardPagination = computed(() => ({
  page: forwardPage.value,
  pageSize,
  itemCount: forwardTotals.value.total_requests,
}));

const dateFormatter = computed(() => new Intl.DateTimeFormat(locale.value, {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
}));
const dateOnlyFormatter = computed(() => new Intl.DateTimeFormat(locale.value, {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
}));
const timePresetOptions = computed(() => [
  { label: t("24 小时内"), value: "last24h" as TimePreset },
  { label: t("最近 7 天"), value: "last7d" as TimePreset },
  { label: t("最近 30 天"), value: "last30d" as TimePreset },
  { label: t("本月"), value: "thisMonth" as TimePreset },
  { label: t("上月"), value: "lastMonth" as TimePreset },
  { label: t("全部"), value: "all" as TimePreset },
  { label: t("自定义"), value: "custom" as TimePreset },
]);
const timeRangeLabel = computed(() => {
  if (!timeRange.value || activePreset.value === "all") return t("全部");
  const preset = timePresetOptions.value.find((item) => item.value === activePreset.value);
  if (preset && activePreset.value !== "custom") return preset.label;
  const [start, end] = timeRange.value;
  return `${dateOnlyFormatter.value.format(new Date(start))} ~ ${dateOnlyFormatter.value.format(new Date(end))}`;
});
const statusMeta = computed<Record<string, { label: string; type: "success" | "warning" | "error" | "default" }>>(() => ({
  success: { label: t("成功"), type: "success" },
  success_no_usage: { label: t("成功·无用量"), type: "success" },
  success_unpriced: { label: t("无价格"), type: "warning" },
  outcome_unknown: { label: t("结果未知"), type: "warning" },
  streaming: { label: t("进行中"), type: "warning" },
  client_error: { label: t("客户端错误"), type: "error" },
  error: { label: t("错误"), type: "error" },
}));
const allOption = computed(() => ({ label: t("全部"), value: "" }));
const statusOptions = computed(() => [allOption.value, ...Object.entries(statusMeta.value).map(([value, meta]) => ({ label: meta.label, value }))]);
const accountOptions = computed(() => [allOption.value, ...accounts.value.map((account) => ({ label: account.name, value: account.id }))]);
const modelOptions = computed(() => [allOption.value, ...models.value.map((model) => ({ label: model, value: model }))]);
const sortOptions = computed(() => [
  { label: t("时间"), value: "timestamp" },
  { label: t("尝试次数"), value: "attempt" },
  { label: t("输入"), value: "prompt_tokens" },
  { label: t("输出"), value: "completion_tokens" },
  { label: t("缓存"), value: "cached_tokens" },
  { label: t("额度消耗（估算）"), value: "cost" },
]);
const hasFilters = computed(() =>
  !!statusFilter.value
  || !!accountFilter.value
  || !!modelFilter.value
  || !!requestIdFilter.value
  || !!timeRange.value,
);

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : dateFormatter.value.format(date);
}

function toIsoString(ms: number): string {
  return new Date(ms).toISOString();
}

function applyTimePreset(preset: TimePreset) {
  if (preset === "custom") {
    const currentRange = resolveTimeRange(activePreset.value, timeRange.value);
    activePreset.value = "custom";
    timeRange.value = currentRange;
    customTimeRange.value = currentRange;
    showTimePanel.value = true;
    return;
  }
  if (preset === "all") {
    activePreset.value = "all";
    timeRange.value = null;
    customTimeRange.value = null;
    showTimePanel.value = false;
    return;
  }
  const range = computeTimeRange(preset);
  activePreset.value = preset;
  timeRange.value = range;
  customTimeRange.value = range;
  showTimePanel.value = false;
}

function applyCustomTimeRange(value: [number, number] | null) {
  if (!value) return;
  const start = new Date(value[0]);
  start.setHours(0, 0, 0, 0);
  const end = new Date(value[1]);
  end.setHours(23, 59, 59, 999);
  const range: [number, number] = [start.getTime(), end.getTime()];
  activePreset.value = "custom";
  timeRange.value = range;
  customTimeRange.value = range;
  showTimePanel.value = false;
}

function formatQuotaCost(row: ForwardLog): string {
  if (row.cost === null || row.cost_state === "unpriced" || row.cost_state === "outcome_unknown") {
    return "—";
  }
  return formatCost(row.cost, 5);
}

async function copyText(target: string, value: string, label: string) {
  try {
    await copy(target, value, label);
    message.success(t("已复制 {label}", { label }));
  } catch (e) {
    message.error(e instanceof Error ? e.message : t("复制失败"));
  }
}

function renderRequestId(row: GatewayLog | ForwardLog) {
  const requestId = row.request_id;
  if (!requestId) return "—";
  const target = `request-id-${row.id}`;
  return h("div", { class: "request-id-cell" }, [
    h(NButton, {
      text: true,
      type: "primary",
      class: "request-id-link",
      title: requestId,
      onClick: () => focusRequestChain(requestId),
    }, { default: () => h("code", requestId) }),
    h(NButton, {
      text: true,
      type: "primary",
      "aria-label": t("复制请求 ID"),
      onClick: () => copyText(target, requestId, t("请求 ID")),
    }, {
      icon: () => h(NIcon, { component: copiedTarget.value === target ? CheckOutlined : CopyOutlined }),
    }),
  ]);
}

function logRowKey(row: GatewayLog | ForwardLog): number {
  return row.id;
}

function focusRequestChain(requestId: string) {
  requestIdFilter.value = requestId;
  sortBy.value = "attempt";
  sortOrder.value = "asc";
}

function renderDiagnostic(row: GatewayLog | ForwardLog) {
  const diagnostic = row.diagnostic;
  const items = [
    [t("错误来源"), row.error_source ?? diagnostic?.error_source],
    [t("失败阶段"), row.error_stage ?? diagnostic?.error_stage],
    [t("协议路径"), diagnostic?.upstream_format
      ? `${diagnostic.client_format} → ${diagnostic.upstream_format}`
      : diagnostic?.client_format],
    [t("尝试次数"), diagnostic?.attempt ?? ("attempt" in row ? row.attempt : null)],
    [t("耗时"), row.duration_ms !== null && row.duration_ms !== undefined
      ? `${row.duration_ms} ms`
      : diagnostic ? `${diagnostic.duration_ms} ms` : null],
    [t("上游响应头耗时"), diagnostic?.upstream_wait_ms !== null && diagnostic?.upstream_wait_ms !== undefined
      ? `${diagnostic.upstream_wait_ms} ms` : null],
    [t("重试动作"), diagnostic?.retry_action],
  ].filter((item) => item[1] !== null && item[1] !== undefined && item[1] !== "");
  const detailBlocks = [
    diagnostic?.upstream_headers && [t("上游 Trace ID"), diagnostic.upstream_headers],
    diagnostic?.request_summary && [t("请求结构与指纹"), {
      fingerprint: diagnostic.request_fingerprint,
      summary: diagnostic.request_summary,
    }],
    diagnostic?.upstream_error && [t("脱敏上游错误"), diagnostic.upstream_error],
  ].filter(Boolean) as Array<[string, unknown]>;
  const errorMessage = "error_message" in row ? row.error_message : row.message;
  return h("div", { class: "diagnostic-detail" }, [
    h("dl", { class: "diagnostic-meta" }, items.flatMap(([label, value]) => [
      h("dt", String(label)),
      h("dd", String(value)),
    ])),
    errorMessage ? h("section", [
      h("h4", t("错误")),
      h("pre", { class: "error-text" }, errorMessage),
    ]) : null,
    ...detailBlocks.map(([label, value]) => h("section", [
      h("h4", label),
      h("pre", { class: "diagnostic-json" }, JSON.stringify(value, null, 2)),
    ])),
  ]);
}

const gatewayColumns = computed(() => [
  {
    type: "expand" as const,
    width: 44,
    expandable: (row: GatewayLog) => !!row.diagnostic || !!row.error_source,
    renderExpand: renderDiagnostic,
  },
  { title: t("时间"), key: "created_at", width: 150, render: (row: GatewayLog) => formatDate(row.created_at) },
  { title: t("请求 ID"), key: "request_id", width: 245, render: renderRequestId },
  { title: t("级别"), key: "level", width: 80 },
  { title: t("分类"), key: "category", width: 100 },
  { title: t("消息"), key: "message", minWidth: 480, ellipsis: { tooltip: true } },
]);
const forwardColumns = computed(() => [
  {
    type: "expand" as const,
    width: 44,
    expandable: (row: ForwardLog) => !!row.error_message || !!row.diagnostic,
    renderExpand: renderDiagnostic,
  },
  { title: t("时间"), key: "timestamp", width: 150, render: (row: ForwardLog) => formatDate(row.timestamp) },
  { title: t("请求 ID"), key: "request_id", width: 245, render: renderRequestId },
  {
    title: t("尝试次数"),
    key: "attempt",
    width: 82,
    align: "center" as const,
    render: (row: ForwardLog) => row.attempt ? `#${row.attempt}` : "—",
  },
  { title: t("模型"), key: "model", width: 160, ellipsis: { tooltip: true } },
  { title: t("账号"), key: "account_name", width: 120, ellipsis: { tooltip: true } },
  {
    title: t("状态"),
    key: "status",
    width: 112,
    render: (row: ForwardLog) => {
      const sourceLabel = row.error_source === "upstream"
        ? t("上游拒绝")
        : row.error_source === "transport"
          ? t("上游连接错误")
          : row.error_source === "client" || (row.error_source === "gateway" && ["auth", "parse", "validation", "body_limit"].includes(row.error_stage ?? ""))
            ? t("请求错误")
            : row.error_source === "downstream"
              ? t("下游断开")
              : null;
      const meta = sourceLabel
        ? { label: sourceLabel, type: row.error_source === "downstream" ? "warning" as const : "error" as const }
        : statusMeta.value[row.status] ?? { label: row.status, type: "default" as const };
      const tags = [h(NTag, { type: meta.type, size: "small", bordered: false }, { default: () => meta.label })];
      if (row.cost_state === "legacy_estimate") {
        tags.push(h(NTag, { type: "default", size: "small", bordered: false }, { default: () => t("旧口径") }));
      }
      return h("div", { class: "status-tags" }, tags);
    },
  },
  { title: "HTTP", key: "http_status", width: 72 },
  { title: t("输入"), key: "prompt_tokens", width: 92, align: "right" as const, render: (row: ForwardLog) => formatNumber(row.prompt_tokens) },
  { title: t("输出"), key: "completion_tokens", width: 92, align: "right" as const, render: (row: ForwardLog) => formatNumber(row.completion_tokens) },
  { title: t("缓存"), key: "cached_tokens", width: 92, align: "right" as const, render: (row: ForwardLog) => formatNumber(row.cached_tokens) },
  { title: t("缓存写"), key: "cache_creation_tokens", width: 92, align: "right" as const, render: (row: ForwardLog) => formatNumber(row.cache_creation_tokens) },
  { title: t("额度消耗（估算）"), key: "cost", width: 152, align: "right" as const, render: formatQuotaCost },
  { title: t("错误"), key: "error_message", minWidth: 220, ellipsis: { tooltip: true } },
]);

function clearFilters() {
  statusFilter.value = "";
  accountFilter.value = "";
  modelFilter.value = "";
  requestIdFilter.value = "";
  activePreset.value = "all";
  timeRange.value = null;
  customTimeRange.value = null;
  showTimePanel.value = false;
}

function toggleSortOrder() {
  sortOrder.value = sortOrder.value === "asc" ? "desc" : "asc";
}

function syncQueryState() {
  const url = new URL(window.location.href);
  url.searchParams.set("tab", activeTab.value);
  if (statusFilter.value) url.searchParams.set("status", statusFilter.value);
  else url.searchParams.delete("status");
  if (accountFilter.value) url.searchParams.set("account", accountFilter.value);
  else url.searchParams.delete("account");
  if (modelFilter.value) url.searchParams.set("model", modelFilter.value);
  else url.searchParams.delete("model");
  if (requestIdFilter.value) url.searchParams.set("request_id", requestIdFilter.value);
  else url.searchParams.delete("request_id");
  if (activePreset.value === "custom" && timeRange.value) {
    url.searchParams.set("start", toIsoString(timeRange.value[0]));
    url.searchParams.set("end", toIsoString(timeRange.value[1]));
    url.searchParams.delete("range");
  } else {
    url.searchParams.delete("start");
    url.searchParams.delete("end");
    url.searchParams.set("range", activePreset.value);
  }
  if (sortBy.value) url.searchParams.set("sort", sortBy.value);
  else url.searchParams.delete("sort");
  if (sortOrder.value) url.searchParams.set("order", sortOrder.value);
  else url.searchParams.delete("order");
  window.history.replaceState(null, "", url);
}

let gatewayRequest = 0;

async function loadGatewayLogs() {
  const request = ++gatewayRequest;
  gatewayLoading.value = true;
  gatewayError.value = "";
  try {
    const logs = await tauriApi.getGatewayLogs(200, requestIdFilter.value);
    if (request !== gatewayRequest) return;
    gatewayLogs.value = logs;
    gatewayPage.value = 1;
  } catch (e) {
    if (request === gatewayRequest) {
      gatewayError.value = e instanceof Error ? e.message : String(e);
      message.error(t("加载运行日志失败: {error}", { error: gatewayError.value }));
    }
  } finally {
    if (request === gatewayRequest) gatewayLoading.value = false;
  }
}

let forwardRequest = 0;

async function loadForwardLogs() {
  const request = ++forwardRequest;
  forwardLoading.value = true;
  forwardLogs.value = [];
  forwardTotals.value = emptySummary();
  try {
    const requestRange = resolveTimeRange(activePreset.value, timeRange.value);
    const result = await tauriApi.getForwardLogs({
      limit: pageSize,
      offset: (forwardPage.value - 1) * pageSize,
      status: statusFilter.value,
      account_id: accountFilter.value,
      model: modelFilter.value,
      request_id: requestIdFilter.value,
      start_time: requestRange ? toIsoString(requestRange[0]) : null,
      end_time: requestRange ? toIsoString(requestRange[1]) : null,
      sort_by: sortBy.value,
      sort_order: sortOrder.value,
    });
    if (request !== forwardRequest) return;
    forwardLogs.value = result.items;
    forwardTotals.value = result.summary;
  } catch (e) {
    if (request === forwardRequest) {
      forwardLogs.value = [];
      forwardTotals.value = emptySummary();
      message.error(t("加载请求日志失败: {error}", { error: String(e) }));
    }
  } finally {
    if (request === forwardRequest) forwardLoading.value = false;
  }
}

async function loadAccounts() {
  try {
    accounts.value = await tauriApi.getAccounts();
  } catch (e) {
    message.error(t("加载账号筛选失败: {error}", { error: String(e) }));
  }
}

async function loadForwardLogModels() {
  try {
    models.value = await tauriApi.getForwardLogModels();
  } catch (e) {
    message.error(t("加载模型筛选失败: {error}", { error: String(e) }));
  }
}

async function refreshForwardLogs() {
  await Promise.all([loadForwardLogs(), loadForwardLogModels()]);
}

function changeForwardPage(page: number) {
  forwardPage.value = page;
  void loadForwardLogs();
}

function changeGatewayPage(page: number) {
  gatewayPage.value = page;
}

watch(activeTab, syncQueryState);
watch(
  [statusFilter, accountFilter, modelFilter, requestIdFilter, timeRange, activePreset, sortBy, sortOrder],
  () => {
    forwardPage.value = 1;
    syncQueryState();
    void loadForwardLogs();
  },
);
watch(requestIdFilter, () => {
  gatewayPage.value = 1;
  void loadGatewayLogs();
});

onMounted(() => {
  syncQueryState();
  void loadGatewayLogs();
  void loadForwardLogs();
  void loadAccounts();
  void loadForwardLogModels();
});

onUnmounted(cleanup);
</script>

<style scoped>
.log-limit-note {
  margin: 6px 0 10px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
}

.logs-card {
  max-width: 1480px;
  margin: 0 auto;
  padding: 4px 18px 18px;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.stats-row {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 12px;
  margin-bottom: 16px;
}
.stat-card {
  padding: 12px 14px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-bg);
}
.stat-label {
  margin-bottom: 6px;
  font-size: var(--ocg-font-xs);
  color: var(--ocg-text-secondary);
}
.stat-value {
  font-size: var(--ocg-font-xl);
  font-weight: 600;
  color: var(--ocg-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.filter-bar {
  display: grid;
  grid-template-columns: 240px 140px 180px 180px auto 120px auto 1fr;
  align-items: end;
  gap: 8px;
  margin-bottom: 12px;
}
.filter-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}
.filter-label {
  font-size: var(--ocg-font-xs);
  color: var(--ocg-subtle);
  line-height: 1.2;
}
.time-range-trigger {
  min-width: 120px;
  max-width: 240px;
  justify-content: flex-start;
}
.time-range-trigger :deep(.n-button__content) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.time-range-panel {
  display: inline-flex;
  flex-direction: row;
  gap: 8px;
  max-width: calc(100vw - 48px);
}
.preset-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 100px;
}
.preset-item {
  justify-content: flex-start;
}
.preset-item :deep(.n-button__content) {
  white-space: nowrap;
}
.custom-range-wrapper {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: auto;
  max-width: 0;
  opacity: 0;
  overflow: hidden;
  transition: max-width 0.2s ease, opacity 0.2s ease;
  border-left: 1px solid transparent;
}
.custom-range-wrapper.is-visible {
  max-width: 600px;
  opacity: 1;
  padding-left: 8px;
  border-left-color: var(--ocg-border);
}
.custom-range-title {
  font-size: var(--ocg-font-sm);
  color: var(--ocg-text-secondary);
  white-space: nowrap;
}
.custom-time-picker {
  min-width: 0;
}
.custom-time-picker :deep(.n-date-panel) {
  box-shadow: none;
  background: transparent;
}
.custom-time-picker :deep(.n-date-panel-header),
.custom-time-picker :deep(.n-date-panel-calendar__picker-col),
.custom-time-picker :deep(.n-date-panel-actions) {
  background: transparent;
}
.sort-select {
  min-width: 110px;
}
.log-toolbar,
.filter-actions {
  display: flex;
  justify-content: flex-end;
  gap: 4px;
}
.log-toolbar {
  margin-bottom: 8px;
}
.request-id-filter {
  width: min(360px, 100%);
  margin-right: auto;
}
:deep(.request-id-cell) {
  display: flex;
  align-items: center;
  gap: 6px;
}
:deep(.request-id-cell code) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
:deep(.status-tags) {
  display: flex;
  flex-wrap: wrap;
  gap: 3px;
}
.diagnostic-detail {
  display: grid;
  gap: 12px;
  padding: 8px 0;
}
.diagnostic-detail h4 {
  margin: 0 0 5px;
  color: var(--ocg-text-secondary);
  font-size: var(--ocg-font-sm);
}
.diagnostic-meta {
  display: grid;
  grid-template-columns: max-content minmax(120px, 1fr) max-content minmax(120px, 1fr);
  gap: 5px 12px;
  margin: 0;
}
.diagnostic-meta dt {
  color: var(--ocg-subtle);
}
.diagnostic-meta dd {
  margin: 0;
  font-family: "Cascadia Mono", Consolas, monospace;
  word-break: break-word;
}
.diagnostic-json,
.error-text {
  margin: 0;
  padding: 10px 12px;
  border: 1px solid var(--ocg-border);
  border-radius: 6px;
  background: var(--ocg-bg);
  color: var(--ocg-ink);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: var(--ocg-font-sm);
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}
.diagnostic-json {
  max-height: 320px;
  overflow: auto;
}

@media (max-width: 1200px) {
  .filter-bar {
    grid-template-columns: repeat(4, 1fr) auto;
  }
  .request-id-field {
    grid-column: span 2;
  }
  .time-range-field {
    grid-column: span 2;
  }
}

@media (max-width: 860px) {
  .time-range-panel {
    flex-direction: column;
  }
  .custom-range-wrapper.is-visible {
    width: auto;
    max-width: 100%;
    border-left: none;
    border-top: 1px solid var(--ocg-border);
    padding-left: 0;
    padding-top: 8px;
  }
  .custom-time-picker {
    overflow-x: auto;
  }
}

@media (max-width: 760px) {
  .stats-row {
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .filter-bar {
    grid-template-columns: 1fr 1fr auto;
  }
  .time-range-field {
    grid-column: span 2;
  }
}

@media (max-width: 560px) {
  .logs-card {
    padding: 2px 12px 12px;
  }
  .stats-row {
    grid-template-columns: repeat(2, 1fr);
  }
  .filter-bar {
    grid-template-columns: 1fr 1fr auto;
  }
  .time-range-field {
    grid-column: span 2;
  }
}
</style>
