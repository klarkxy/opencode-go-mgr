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
        />
      </n-tab-pane>
      <n-tab-pane name="forward" :tab="t('请求日志')">
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
          <div class="filter-actions">
            <n-tooltip v-if="statusFilter || accountFilter" trigger="hover">
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
                  @click="loadForwardLogs"
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
          :summary="forwardSummary"
          remote
          size="small"
          summary-placement="bottom"
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
  NEmpty,
  NIcon,
  NSelect,
  NTabPane,
  NTabs,
  NTag,
  NTooltip,
  useMessage,
} from "naive-ui";
import type { DataTableCreateSummary } from "naive-ui";
import { ClearOutlined, ReloadOutlined } from "@vicons/antd";
import { tauriApi } from "../api/tauri";
import type { Account, ForwardLog, ForwardLogSummary, GatewayLog } from "../api/tauri";
import { t } from "../i18n/index.ts";
import { locale } from "../i18n/index.ts";
import { formatNumber } from "../utils/format.ts";

type LogTab = "gateway" | "forward";

const query = new URLSearchParams(window.location.search);
const message = useMessage();
const activeTab = ref<LogTab>(query.get("tab") === "forward" ? "forward" : "gateway");
const gatewayLogs = ref<GatewayLog[]>([]);
const forwardLogs = ref<ForwardLog[]>([]);
const accounts = ref<Account[]>([]);
const gatewayLoading = ref(false);
const forwardLoading = ref(false);
const statusFilter = ref<string | null>(query.get("status"));
const accountFilter = ref<string | null>(query.get("account"));
const forwardPage = ref(1);
const pageSize = 20;
const gatewayPagination = { pageSize };
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
const costFormatter = computed(() => new Intl.NumberFormat(locale.value, {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 5,
  maximumFractionDigits: 5,
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

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : dateFormatter.value.format(date);
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
  { title: t("成本"), key: "cost", width: 112, align: "right" as const, render: (row: ForwardLog) => costFormatter.value.format(row.cost) },
  { title: t("错误"), key: "error_message", minWidth: 220, ellipsis: { tooltip: true } },
]);

const forwardSummary: DataTableCreateSummary<ForwardLog> = () => ({
  timestamp: { value: t("请求数：{count}", { count: formatNumber(forwardTotals.value.total_requests) }), colSpan: 5 },
  prompt_tokens: { value: formatNumber(forwardTotals.value.prompt_tokens) },
  completion_tokens: { value: formatNumber(forwardTotals.value.completion_tokens) },
  cached_tokens: { value: formatNumber(forwardTotals.value.cached_tokens) },
  cost: { value: costFormatter.value.format(forwardTotals.value.cost) },
  error_message: { value: "" },
});

function clearFilters() {
  statusFilter.value = null;
  accountFilter.value = null;
}

function syncQueryState() {
  const url = new URL(window.location.href);
  url.searchParams.set("tab", activeTab.value);
  if (statusFilter.value) url.searchParams.set("status", statusFilter.value);
  else url.searchParams.delete("status");
  if (accountFilter.value) url.searchParams.set("account", accountFilter.value);
  else url.searchParams.delete("account");
  window.history.replaceState(null, "", url);
}

async function loadGatewayLogs() {
  gatewayLoading.value = true;
  try {
    gatewayLogs.value = await tauriApi.getGatewayLogs(200);
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

function changeForwardPage(page: number) {
  forwardPage.value = page;
  void loadForwardLogs();
}

watch(activeTab, syncQueryState);
watch([statusFilter, accountFilter], () => {
  forwardPage.value = 1;
  syncQueryState();
  void loadForwardLogs();
});

onMounted(() => {
  void loadGatewayLogs();
  void loadForwardLogs();
  void loadAccounts();
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
.filter-bar {
  display: grid;
  grid-template-columns: 150px 180px 1fr;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
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

@media (max-width: 560px) {
  .logs-card {
    padding: 2px 12px 12px;
  }
  .filter-bar {
    grid-template-columns: 1fr 1fr auto;
  }
}
</style>
