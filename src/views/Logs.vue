<template>
  <section class="logs-card">
    <n-tabs v-model:value="activeTab" type="line" animated>
      <n-tab-pane name="gateway" :tab="t('运行日志')">
        <div class="log-toolbar">
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
        <n-data-table
          :columns="gatewayColumns"
          :data="gatewayLogs"
          :loading="gatewayLoading"
          :pagination="gatewayPagination"
          :scroll-x="920"
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
            <div class="stat-label">{{ t("成本") }}</div>
            <div class="stat-value">{{ formatCost(forwardTotals.cost, 5) }}</div>
          </div>
        </div>
        <div class="filter-bar">
          <n-select
            v-model:value="statusFilter"
            :options="statusOptions"
            :placeholder="t('状态')"
            clearable
          />
          <n-select
            v-model:value="accountFilter"
            :options="accountOptions"
            :placeholder="t('账号')"
            clearable
          />
          <n-select
            v-model:value="modelFilter"
            :options="modelOptions"
            :placeholder="t('模型')"
            clearable
          />
          <n-date-picker
            v-model:value="timeRange"
            type="datetimerange"
            clearable
            :placeholder="t('选择时间范围')"
            class="time-range-picker"
          />
          <n-select
            v-model:value="sortBy"
            :options="sortOptions"
            :placeholder="t('排序')"
            :consistent-menu-width="false"
            class="sort-select"
          />
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
          :loading="forwardLoading"
          :pagination="forwardPagination"
          :scroll-x="1280"
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
import { computed, h, onMounted, ref, watch } from "vue";
import {
  NButton,
  NDataTable,
  NDatePicker,
  NEmpty,
  NIcon,
  NSelect,
  NTabPane,
  NTabs,
  NTag,
  NTooltip,
  useMessage,
} from "naive-ui";
import { ArrowDownOutlined, ArrowUpOutlined, ClearOutlined, ReloadOutlined } from "@vicons/antd";
import { tauriApi } from "../api/tauri";
import type { Account, ForwardLog, ForwardLogSummary, GatewayLog } from "../api/tauri";
import { t } from "../i18n/index.ts";
import { locale } from "../i18n/index.ts";
import { formatCost, formatNumber } from "../utils/format.ts";

type LogTab = "gateway" | "forward";
type SortBy = "timestamp" | "prompt_tokens" | "completion_tokens" | "cached_tokens" | "cost";
type SortOrder = "asc" | "desc";

const sortValues = new Set<SortBy>([
  "timestamp",
  "prompt_tokens",
  "completion_tokens",
  "cached_tokens",
  "cost",
]);

const query = new URLSearchParams(window.location.search);
const message = useMessage();
const activeTab = ref<LogTab>(query.get("tab") === "forward" ? "forward" : "gateway");
const gatewayLogs = ref<GatewayLog[]>([]);
const forwardLogs = ref<ForwardLog[]>([]);
const accounts = ref<Account[]>([]);
const models = ref<string[]>([]);
const gatewayLoading = ref(false);
const forwardLoading = ref(false);
const statusFilter = ref<string | null>(query.get("status"));
const accountFilter = ref<string | null>(query.get("account"));
const modelFilter = ref<string | null>(query.get("model"));
const querySort = query.get("sort");
const queryOrder = query.get("order");
const sortBy = ref<SortBy>(
  querySort !== null && sortValues.has(querySort as SortBy) ? querySort as SortBy : "timestamp",
);
const sortOrder = ref<SortOrder>(queryOrder === "asc" || queryOrder === "desc" ? queryOrder : "desc");
const timeRange = ref<[number, number] | null>((() => {
  const start = query.get("start");
  const end = query.get("end");
  if (!start || !end) return null;
  const startMs = Date.parse(start);
  const endMs = Date.parse(end);
  if (Number.isNaN(startMs) || Number.isNaN(endMs) || startMs > endMs) return null;
  return [startMs, endMs];
})());
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
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
}));
const statusMeta = computed<Record<string, { label: string; type: "success" | "warning" | "error" | "default" }>>(() => ({
  success: { label: t("成功"), type: "success" },
  success_no_usage: { label: t("成功·无用量"), type: "success" },
  streaming: { label: t("进行中"), type: "warning" },
  client_error: { label: t("客户端错误"), type: "error" },
  error: { label: t("错误"), type: "error" },
}));
const statusOptions = computed(() => Object.entries(statusMeta.value).map(([value, meta]) => ({ label: meta.label, value })));
const accountOptions = computed(() => accounts.value.map((account) => ({ label: account.name, value: account.id })));
const modelOptions = computed(() => models.value.map((model) => ({ label: model, value: model })));
const sortOptions = computed(() => [
  { label: t("时间"), value: "timestamp" },
  { label: t("输入"), value: "prompt_tokens" },
  { label: t("输出"), value: "completion_tokens" },
  { label: t("缓存"), value: "cached_tokens" },
  { label: t("成本"), value: "cost" },
]);
const hasFilters = computed(() =>
  !!statusFilter.value
  || !!accountFilter.value
  || !!modelFilter.value
  || !!timeRange.value,
);

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : dateFormatter.value.format(date);
}

function toIsoString(ms: number): string {
  return new Date(ms).toISOString();
}

const gatewayColumns = computed(() => [
  { title: t("时间"), key: "created_at", width: 150, render: (row: GatewayLog) => formatDate(row.created_at) },
  { title: t("级别"), key: "level", width: 80 },
  { title: t("分类"), key: "category", width: 100 },
  { title: t("消息"), key: "message", minWidth: 480, ellipsis: { tooltip: true } },
]);
const forwardColumns = computed(() => [
  { title: t("时间"), key: "timestamp", width: 150, render: (row: ForwardLog) => formatDate(row.timestamp) },
  { title: t("模型"), key: "model", width: 160, ellipsis: { tooltip: true } },
  { title: t("账号"), key: "account_name", width: 120, ellipsis: { tooltip: true } },
  {
    title: t("状态"),
    key: "status",
    width: 112,
    render: (row: ForwardLog) => {
      const meta = statusMeta.value[row.status] ?? { label: row.status, type: "default" as const };
      return h(NTag, { type: meta.type, size: "small", bordered: false }, { default: () => meta.label });
    },
  },
  { title: "HTTP", key: "http_status", width: 72 },
  { title: t("输入"), key: "prompt_tokens", width: 92, align: "right" as const, render: (row: ForwardLog) => formatNumber(row.prompt_tokens) },
  { title: t("输出"), key: "completion_tokens", width: 92, align: "right" as const, render: (row: ForwardLog) => formatNumber(row.completion_tokens) },
  { title: t("缓存"), key: "cached_tokens", width: 92, align: "right" as const, render: (row: ForwardLog) => formatNumber(row.cached_tokens) },
  { title: t("成本"), key: "cost", width: 112, align: "right" as const, render: (row: ForwardLog) => formatCost(row.cost, 5) },
  { title: t("错误"), key: "error_message", minWidth: 220, ellipsis: { tooltip: true } },
]);

function clearFilters() {
  statusFilter.value = null;
  accountFilter.value = null;
  modelFilter.value = null;
  timeRange.value = null;
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
  if (timeRange.value) {
    url.searchParams.set("start", toIsoString(timeRange.value[0]));
    url.searchParams.set("end", toIsoString(timeRange.value[1]));
  } else {
    url.searchParams.delete("start");
    url.searchParams.delete("end");
  }
  if (sortBy.value) url.searchParams.set("sort", sortBy.value);
  else url.searchParams.delete("sort");
  if (sortOrder.value) url.searchParams.set("order", sortOrder.value);
  else url.searchParams.delete("order");
  window.history.replaceState(null, "", url);
}

async function loadGatewayLogs() {
  gatewayLoading.value = true;
  try {
    gatewayLogs.value = await tauriApi.getGatewayLogs(200);
    gatewayPage.value = 1;
  } catch (e) {
    message.error(t("加载运行日志失败: {error}", { error: String(e) }));
  } finally {
    gatewayLoading.value = false;
  }
}

let forwardRequest = 0;

async function loadForwardLogs() {
  const request = ++forwardRequest;
  forwardLoading.value = true;
  try {
    const result = await tauriApi.getForwardLogs({
      limit: pageSize,
      offset: (forwardPage.value - 1) * pageSize,
      status: statusFilter.value,
      account_id: accountFilter.value,
      model: modelFilter.value,
      start_time: timeRange.value ? toIsoString(timeRange.value[0]) : null,
      end_time: timeRange.value ? toIsoString(timeRange.value[1]) : null,
      sort_by: sortBy.value,
      sort_order: sortOrder.value,
    });
    if (request !== forwardRequest) return;
    forwardLogs.value = result.items;
    forwardTotals.value = result.summary;
  } catch (e) {
    if (request === forwardRequest) message.error(t("加载请求日志失败: {error}", { error: String(e) }));
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
  [statusFilter, accountFilter, modelFilter, timeRange, sortBy, sortOrder],
  () => {
    forwardPage.value = 1;
    syncQueryState();
    void loadForwardLogs();
  },
);

onMounted(() => {
  syncQueryState();
  void loadGatewayLogs();
  void loadForwardLogs();
  void loadAccounts();
  void loadForwardLogModels();
});
</script>

<style scoped>
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
  font-size: 12px;
  color: var(--ocg-text-secondary);
}
.stat-value {
  font-size: 18px;
  font-weight: 600;
  color: var(--ocg-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.filter-bar {
  display: grid;
  grid-template-columns: 140px 180px 180px 320px 120px auto 1fr;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}
.time-range-picker {
  min-width: 0;
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

@media (max-width: 1200px) {
  .filter-bar {
    grid-template-columns: repeat(4, 1fr) auto;
  }
  .time-range-picker {
    grid-column: span 2;
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
  .time-range-picker {
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
  .time-range-picker {
    grid-column: span 2;
  }
}
</style>
