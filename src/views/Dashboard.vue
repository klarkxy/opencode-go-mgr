<template>
  <div class="dashboard">
    <section class="connection-hero" aria-labelledby="connection-title">
      <div class="connection-content">
        <div class="connection-head">
          <h2 id="connection-title">⚡ {{ t("接入中心") }}</h2>
          <span class="ready-mark"><span aria-hidden="true" /> {{ t("就绪") }}</span>
        </div>

        <div class="connection-rows">
          <div class="connection-row">
            <n-icon size="18" aria-hidden="true"><ApiOutlined /></n-icon>
            <div class="connection-value">
              <span class="sr-only">API Base URL</span>
              <code>{{ serviceApiUrl }}</code>
            </div>
            <n-tooltip trigger="hover" :delay="200">
              <template #trigger>
                <n-button
                  circle
                  quaternary
                  size="small"
                  :aria-label="t('复制 API Base URL')"
                  @click="copyConnection('api', serviceApiUrl, 'API Base URL')"
                >
                  <template #icon>
                    <n-icon :component="copiedTarget === 'api' ? CheckOutlined : CopyOutlined" />
                  </template>
                </n-button>
              </template>
              {{ t("复制 API Base URL") }}
            </n-tooltip>
          </div>

          <div class="connection-row">
            <n-icon size="18" aria-hidden="true"><KeyOutlined /></n-icon>
            <div class="connection-value">
              <span class="sr-only">Key</span>
              <code>{{ maskedKey }}</code>
            </div>
            <div class="row-actions">
              <n-popconfirm
                :positive-text="t('生成新 Key')"
                :negative-text="t('取消')"
                @positive-click="regenerateKey"
              >
                <template #trigger>
                  <n-tooltip trigger="hover" :delay="200">
                    <template #trigger>
                      <n-button
                        circle
                        quaternary
                        size="small"
                        :aria-label="t('刷新 Key')"
                        :loading="refreshingKey"
                      >
                        <template #icon><n-icon :component="ReloadOutlined" /></template>
                      </n-button>
                    </template>
                    {{ t("刷新 Key") }}
                  </n-tooltip>
                </template>
                {{ t("旧 Key 将立即失效，继续生成新 Key？") }}
              </n-popconfirm>
              <n-tooltip trigger="hover" :delay="200">
                <template #trigger>
                  <n-button
                    circle
                    quaternary
                    size="small"
                    :aria-label="t('复制 Key')"
                    :disabled="!serviceConfig.gateway_key"
                    @click="copyConnection('key', serviceConfig.gateway_key, 'Key')"
                  >
                    <template #icon>
                      <n-icon :component="copiedTarget === 'key' ? CheckOutlined : CopyOutlined" />
                    </template>
                  </n-button>
                </template>
                {{ t("复制 Key") }}
              </n-tooltip>
            </div>
          </div>

          <div class="connection-row">
            <n-icon size="18" aria-hidden="true"><CloudServerOutlined /></n-icon>
            <div class="connection-value">
              <span class="sr-only">{{ t("上游地址") }}</span>
              <code>{{ serviceConfig.upstream_base_url || t("未设置") }}</code>
            </div>
            <n-tooltip trigger="hover" :delay="200">
              <template #trigger>
                <n-button
                  circle
                  quaternary
                  size="small"
                  :aria-label="t('复制上游地址')"
                  :disabled="!serviceConfig.upstream_base_url"
                  @click="copyConnection('upstream', serviceConfig.upstream_base_url, t('上游地址'))"
                >
                  <template #icon>
                    <n-icon :component="copiedTarget === 'upstream' ? CheckOutlined : CopyOutlined" />
                  </template>
                </n-button>
              </template>
              {{ t("复制上游地址") }}
            </n-tooltip>
          </div>
        </div>
        <p v-if="connectionUrls.insecureHttp" class="connection-warning" role="status">
          {{ t("非本机 HTTP 会明文传输 Gateway Key 与请求内容，请仅在可信网络中使用。") }}
        </p>
      </div>
      <img :src="characterImage" alt="" class="hero-character" aria-hidden="true" />
    </section>

    <section class="kpi-row" :aria-label="t('用量摘要')">
      <article class="kpi-card">
        <span class="kpi-badge success"><n-icon aria-hidden="true"><KeyOutlined /></n-icon></span>
        <div><strong>{{ formatNumber(summary.available_accounts) }}<small>/{{ formatNumber(summary.total_accounts) }}</small></strong><span>{{ t("可用账号") }}</span></div>
      </article>
      <article class="kpi-card">
        <span class="kpi-badge info"><n-icon aria-hidden="true"><CalendarOutlined /></n-icon></span>
        <div><strong>{{ formatCost(summary.today_cost) }}</strong><span>{{ t("今日") }}</span></div>
      </article>
      <article class="kpi-card">
        <span class="kpi-badge warning"><n-icon aria-hidden="true"><ClockCircleOutlined /></n-icon></span>
        <div><strong>{{ formatCost(summary.week_cost) }}</strong><span>{{ t("本周") }}</span></div>
      </article>
      <article class="kpi-card">
        <span class="kpi-badge primary"><n-icon aria-hidden="true"><WalletOutlined /></n-icon></span>
        <div><strong>{{ formatCost(summary.month_cost) }}</strong><span>{{ t("本月") }}</span></div>
      </article>
    </section>

    <section class="card chart-card">
      <div class="card-head chart-head">
        <div>
          <h3 class="card-title">{{ t("每日消耗") }}</h3>
          <p class="card-desc">{{ t("最近 {days} 天 · 成功请求", { days: 30 }) }}</p>
        </div>
        <div class="chart-stats" :aria-label="t('图表摘要')">
          <span>{{ t("模型：{count}", { count: formatNumber(legendModels.length) }) }}</span>
          <span><b>{{ formatCost(totalChartCost) }}</b> {{ t("{days} 天合计", { days: 30 }) }}</span>
          <span><b>{{ formatCost(totalChartCost / 30) }}</b> {{ t("日均") }}</span>
        </div>
      </div>
      <div class="legend" :aria-label="t('模型图例')">
        <span v-for="model in legendModels" :key="model.model" class="legend-item">
          <span class="legend-dot" :style="{ background: model.color }" aria-hidden="true" />
          {{ model.model }}
        </span>
      </div>
      <n-spin :show="loading">
        <n-empty v-if="!loading && totalChartCost === 0" :description="t('暂无消耗数据')" />
        <StackedBarChart v-else :data="dailyCosts" :days="30" />
      </n-spin>
    </section>

    <section class="card accounts-card">
      <div class="card-head">
        <h3 class="card-title">{{ t("账号概览") }}</h3>
        <span class="card-desc">{{ t("账号数：{count}", { count: formatNumber(accounts.length) }) }}</span>
      </div>
      <n-empty v-if="accounts.length === 0" :description="t('暂无账号，请前往账号页添加')" />
      <div v-else class="account-grid">
        <article v-for="account in accounts" :key="account.id" class="account-cell">
          <div class="account-top">
            <strong>{{ account.name }}</strong>
            <span
              class="account-status"
              :class="account.enabled ? (isCoolingDown(account) ? 'cooling' : 'active') : 'disabled'"
            >{{ statusLabel(account) }}</span>
          </div>
          <div class="account-usage mono">{{ getUsageText(account.id) }}</div>
        </article>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { NButton, NEmpty, NIcon, NPopconfirm, NSpin, NTooltip, useMessage } from "naive-ui";
import {
  ApiOutlined,
  CalendarOutlined,
  CheckOutlined,
  ClockCircleOutlined,
  CloudServerOutlined,
  CopyOutlined,
  KeyOutlined,
  ReloadOutlined,
  WalletOutlined,
} from "@vicons/antd";
import StackedBarChart from "../components/StackedBarChart.vue";
import { tauriApi } from "../api/tauri";
import type { Account, DailyModelCost, DashboardSummary, UsageWindow } from "../api/tauri";
import { CHART_PALETTE } from "../theme";
import { locale, t } from "../i18n/index.ts";
import { maskConnectionKey, resolveConnectionUrls, writeConnectionValue } from "./dashboard-connection";

type ConnectionTarget = "api" | "key" | "upstream";

const message = useMessage();
const characterImage = new URL("../../assets/opencode-mascot.png", import.meta.url).href;
const accounts = ref<Account[]>([]);
const usageMap = ref<Record<string, UsageWindow>>({});
const dailyCosts = ref<DailyModelCost[]>([]);
const loading = ref(true);
const refreshingKey = ref(false);
const copiedTarget = ref<ConnectionTarget | null>(null);
let copyTimer: ReturnType<typeof setTimeout> | undefined;

const serviceConfig = ref({
  gateway_port: 9042,
  gateway_key: "",
  upstream_base_url: "",
  client_root_url: "",
});
const summary = ref<DashboardSummary>({
  total_accounts: 0,
  available_accounts: 0,
  today_cost: 0,
  week_cost: 0,
  month_cost: 0,
});

const legendModels = computed(() => {
  const totals = new Map<string, number>();
  for (const row of dailyCosts.value) totals.set(row.model, (totals.get(row.model) ?? 0) + row.cost);
  return [...totals.keys()]
    .sort((a, b) => totals.get(b)! - totals.get(a)!)
    .map((model, index) => ({ model, color: CHART_PALETTE[index % CHART_PALETTE.length] }));
});
const totalChartCost = computed(() => dailyCosts.value.reduce((sum, row) => sum + row.cost, 0));
const maskedKey = computed(() => maskConnectionKey(serviceConfig.value.gateway_key));
const connectionUrls = computed(() => resolveConnectionUrls(
  serviceConfig.value.client_root_url,
  window.location.origin,
  serviceConfig.value.gateway_port,
  import.meta.env.DEV,
));
const serviceApiUrl = computed(() => connectionUrls.value.apiBaseUrl);

function formatCost(value: number): string {
  const digits = value !== 0 && value < 0.01 ? 4 : 2;
  return new Intl.NumberFormat(locale.value, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat(locale.value).format(value);
}

function isCoolingDown(account: Account): boolean {
  if (!account.cooldown_until) return false;
  const until = Date.parse(account.cooldown_until);
  return Number.isFinite(until) && until > Date.now();
}

function statusLabel(account: Account): string {
  if (!account.enabled) return t("已禁用");
  return isCoolingDown(account) ? t("冷却中") : t("可用");
}

function getUsageText(accountId: string): string {
  const usage = usageMap.value[accountId];
  if (!usage) return t("暂无用量");
  return t("5h {five} · 周 {week} · 月 {month}", {
    five: formatCost(usage.window_5h),
    week: formatCost(usage.window_week),
    month: formatCost(usage.window_month),
  });
}

async function copyConnection(target: ConnectionTarget, value: string, label: string) {
  try {
    const writeText = navigator.clipboard?.writeText?.bind(navigator.clipboard);
    await writeConnectionValue(writeText, value);
    copiedTarget.value = target;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copiedTarget.value = null; }, 1500);
    message.success(t("已复制 {label}", { label }));
  } catch (e) {
    message.error(e instanceof Error ? e.message : t("复制失败"));
  }
}

async function regenerateKey() {
  refreshingKey.value = true;
  try {
    serviceConfig.value.gateway_key = await tauriApi.regenerateGatewayKey();
    message.success(t("Key 已刷新"));
  } catch (e) {
    message.error(t("刷新 Key 失败: {error}", { error: String(e) }));
  } finally {
    refreshingKey.value = false;
  }
}

onMounted(async () => {
  const [loadedAccounts, settings, loadedSummary, costs] = await Promise.allSettled([
      tauriApi.getAccounts(),
      tauriApi.getSettings(),
      tauriApi.getDashboardSummary(),
      tauriApi.getDailyCostByModel(30),
  ]);
  if (loadedAccounts.status === "fulfilled") {
    accounts.value = loadedAccounts.value;
    const usage = await Promise.allSettled(loadedAccounts.value.map(async (account) => [account.id, await tauriApi.getAccountUsage(account.id)] as const));
    usageMap.value = Object.fromEntries(usage.flatMap((result) => result.status === "fulfilled" ? [result.value] : []));
  }
  if (settings.status === "fulfilled") serviceConfig.value = settings.value;
  if (loadedSummary.status === "fulfilled") summary.value = loadedSummary.value;
  if (costs.status === "fulfilled") dailyCosts.value = costs.value;
  if ([loadedAccounts, settings, loadedSummary, costs].some((result) => result.status === "rejected")) {
    message.error(t("部分仪表盘数据加载失败"));
  }
  loading.value = false;
});

onUnmounted(() => clearTimeout(copyTimer));
</script>

<style scoped>
.dashboard {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 1480px;
  margin: 0 auto;
}

.connection-hero {
  position: relative;
  min-height: 262px;
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.connection-hero::before {
  content: "";
  position: absolute;
  inset: 0 0 0 54%;
  opacity: 0.42;
  background-image:
    linear-gradient(var(--ocg-border) 1px, transparent 1px),
    linear-gradient(90deg, var(--ocg-border) 1px, transparent 1px);
  background-size: 24px 24px;
  mask-image: linear-gradient(90deg, transparent, #000 35%);
}
.connection-hero::after {
  content: "";
  position: absolute;
  z-index: 0;
  right: -12px;
  bottom: -28px;
  width: 390px;
  height: 300px;
  background: radial-gradient(ellipse at center, var(--ocg-mascot-halo, transparent), transparent 70%);
  pointer-events: none;
}
.connection-content {
  position: relative;
  z-index: 2;
  width: min(760px, calc(100% - 300px));
  padding: 24px;
}
.connection-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}
.connection-head h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 20px/1.2 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.ready-mark {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--ocg-success);
  font: 700 10px/1 "Cascadia Mono", Consolas, monospace;
  letter-spacing: 0.08em;
}
.ready-mark > span {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--ocg-success);
  box-shadow: 0 0 0 4px var(--ocg-success-soft);
}
.connection-rows {
  display: grid;
  gap: 8px;
}
.connection-row {
  display: grid;
  grid-template-columns: 28px minmax(0, 1fr) auto;
  align-items: center;
  min-height: 48px;
  padding: 6px 8px 6px 12px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: color-mix(in srgb, var(--ocg-canvas) 72%, var(--ocg-surface));
  color: var(--ocg-primary);
}
.connection-value {
  min-width: 0;
  color: var(--ocg-ink);
}
.connection-value code {
  display: block;
  overflow: hidden;
  font: 12px/1.4 "Cascadia Mono", Consolas, monospace;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-actions {
  display: flex;
  align-items: center;
  gap: 2px;
}
.connection-warning {
  margin: 10px 2px 0;
  color: var(--ocg-warning);
  font-size: 11px;
  line-height: 1.5;
}
.hero-character {
  position: absolute;
  z-index: 1;
  top: 4px;
  right: 28px;
  height: 380px;
  max-width: 34%;
  object-fit: contain;
  filter:
    drop-shadow(0 0 1px var(--ocg-mascot-rim, transparent))
    drop-shadow(0 18px 20px rgba(31, 27, 56, 0.14));
  pointer-events: none;
  user-select: none;
}

.kpi-row {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
}
.kpi-card {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
  padding: 14px 16px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.kpi-badge {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: 10px;
}
.kpi-badge.success { color: var(--ocg-success); background: var(--ocg-success-soft); }
.kpi-badge.info { color: #2f6fd4; background: color-mix(in srgb, #2f6fd4 12%, transparent); }
.kpi-badge.warning { color: var(--ocg-warning); background: var(--ocg-warning-soft); }
.kpi-badge.primary { color: var(--ocg-primary); background: var(--ocg-primary-soft); }
.kpi-card > div {
  display: grid;
  min-width: 0;
}
.kpi-card strong {
  color: var(--ocg-ink);
  font: 700 22px/1.1 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
  font-variant-numeric: tabular-nums;
  overflow: hidden;
  text-overflow: ellipsis;
}
.kpi-card small {
  color: var(--ocg-subtle);
  font-size: 14px;
}
.kpi-card span:last-child {
  margin-top: 3px;
  color: var(--ocg-subtle);
  font-size: 11px;
}

.card {
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.card-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 18px 10px;
}
.card-title {
  margin: 0;
  color: var(--ocg-ink);
  font: 650 15px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.card-desc {
  margin: 3px 0 0;
  color: var(--ocg-subtle);
  font-size: 11px;
}
.chart-card {
  padding-bottom: 12px;
}
.chart-stats {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px 16px;
  color: var(--ocg-subtle);
  font-size: 11px;
}
.chart-stats b {
  color: var(--ocg-ink);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-weight: 600;
}
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 7px 14px;
  padding: 0 18px 4px;
}
.legend-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--ocg-muted);
  font-size: 10px;
}
.legend-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
}
.chart-card :deep(.n-spin-content) {
  padding: 4px 12px 0;
  overflow-x: auto;
}

.accounts-card {
  padding-bottom: 16px;
}
.account-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
  gap: 10px;
  padding: 4px 18px 0;
}
.account-cell {
  padding: 11px 12px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: color-mix(in srgb, var(--ocg-canvas) 70%, var(--ocg-surface));
}
.account-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 5px;
}
.account-top strong {
  overflow: hidden;
  color: var(--ocg-ink);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.account-status {
  flex: 0 0 auto;
  font-size: 10px;
  font-weight: 650;
}
.account-status.active { color: var(--ocg-success); }
.account-status.cooling { color: var(--ocg-warning); }
.account-status.disabled { color: var(--ocg-subtle); }
.account-usage {
  color: var(--ocg-subtle);
  font-size: 10px;
  line-height: 1.5;
}

@media (max-width: 900px) {
  .connection-content {
    width: calc(100% - 210px);
  }
  .hero-character {
    right: 6px;
    max-width: 36%;
  }
  .kpi-row {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 640px) {
  .dashboard {
    gap: 12px;
  }
  .connection-hero {
    min-height: 256px;
  }
  .connection-content {
    width: 100%;
    padding: 18px 14px;
  }
  .hero-character {
    z-index: 0;
    top: auto;
    right: -50px;
    bottom: -58px;
    height: 282px;
    max-width: 58%;
    opacity: 0.12;
  }
  .connection-hero::after {
    right: -48px;
    bottom: -34px;
    width: 300px;
    height: 250px;
  }
  .connection-hero::before {
    inset: 0;
    opacity: 0.18;
  }
  .connection-row {
    background: color-mix(in srgb, var(--ocg-surface) 88%, transparent);
  }
  .kpi-card {
    padding: 12px;
  }
  .kpi-card strong {
    font-size: 18px;
  }
  .chart-head {
    align-items: flex-start;
  }
  .chart-stats {
    display: grid;
    gap: 2px;
  }
}
</style>
