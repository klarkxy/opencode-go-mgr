<!--
  Dashboard 现代化重写。参考 Vercel / Linear / Stripe dashboard 设计语言:
    - 卡片用柔和阴影 + 细边框,避免重描边
    - 4 个 KPI 卡片占顶部一行,各自有图标徽章 + 主数值 + 趋势副文本
    - 中间主打区是按日按模型分段的堆叠柱状图(占主视觉)
    - 右侧次区服务连接信息 + Key 复制
    - 底部账号概览用紧凑卡片网格替代长 list
-->
<template>
  <div class="dashboard">
    <section class="hero-card" aria-label="OCG Manager">
      <div class="hero-copy">
        <span class="hero-kicker">OCG Manager</span>
        <h2>多账号，一处管理</h2>
        <p>集中查看账号、用量与转发配置。</p>
      </div>
      <img :src="characterImage" alt="" class="hero-character" />
    </section>

    <!-- KPI 卡片行 -->
    <section class="kpi-row">
      <div class="kpi-card">
        <div class="kpi-head">
          <span class="kpi-badge green">
            <n-icon size="16"><KeyOutlined /></n-icon>
          </span>
          <span class="kpi-label">账号</span>
        </div>
        <div class="kpi-main">
          <span class="kpi-value">{{ summary.available_accounts }}<span class="kpi-unit">/{{ summary.total_accounts }}</span></span>
          <span class="kpi-title">可用 / 总数</span>
        </div>
      </div>

      <div class="kpi-card">
        <div class="kpi-head">
          <span class="kpi-badge blue">
            <n-icon size="16"><CalendarOutlined /></n-icon>
          </span>
          <span class="kpi-label">今日</span>
        </div>
        <div class="kpi-main">
          <span class="kpi-value money">${{ formatCost(summary.today_cost) }}</span>
          <span class="kpi-title">今日消耗</span>
        </div>
      </div>

      <div class="kpi-card">
        <div class="kpi-head">
          <span class="kpi-badge amber">
            <n-icon size="16"><ClockCircleOutlined /></n-icon>
          </span>
          <span class="kpi-label">本周</span>
        </div>
        <div class="kpi-main">
          <span class="kpi-value money">${{ formatCost(summary.week_cost) }}</span>
          <span class="kpi-title">本周累计</span>
        </div>
      </div>

      <div class="kpi-card">
        <div class="kpi-head">
          <span class="kpi-badge violet">
            <n-icon size="16"><WalletOutlined /></n-icon>
          </span>
          <span class="kpi-label">本月</span>
        </div>
        <div class="kpi-main">
          <span class="kpi-value money">${{ formatCost(summary.month_cost) }}</span>
          <span class="kpi-title">30 天累计</span>
        </div>
      </div>
    </section>

    <!-- 主体: 图表 + 服务连接侧栏 -->
    <section class="main-grid">
      <div class="card chart-card">
        <div class="card-head">
          <div>
            <h3 class="card-title">每日消耗(按模型分段)</h3>
            <p class="card-desc">最近 30 天,仅统计成功请求</p>
          </div>
          <div class="legend">
            <span v-for="m in legendModels" :key="m.model" class="legend-item">
              <span class="dot" :style="{ background: m.color }" />
              {{ m.model }}
            </span>
          </div>
        </div>
        <n-spin :show="loading">
          <n-empty v-if="!loading && totalChartCost === 0" description="暂无消耗数据" />
          <StackedBarChart v-else :data="dailyCosts" :days="30" />
        </n-spin>
      </div>

      <div class="side-stack">
        <div class="card service-card">
          <div class="card-head">
            <h3 class="card-title">服务连接</h3>
          </div>
          <div class="gw-rows">
            <div class="gw-row">
              <span class="gw-key">API 地址</span>
              <span class="gw-val mono">{{ serviceApiUrl }}</span>
            </div>
            <div class="gw-row">
              <span class="gw-key">上游地址</span>
              <span class="gw-val mono">{{ serviceConfig.upstream_base_url || "—" }}</span>
            </div>
            <div class="gw-row align-top">
              <span class="gw-key">Gateway Key</span>
              <div class="gw-val key-box">
                <span class="mono">{{ maskedKey }}</span>
                <n-button size="tiny" quaternary @click="copyKey">
                  <template #icon><CopyOutlined /></template>
                </n-button>
              </div>
            </div>
          </div>
        </div>

        <div class="card mini-stats">
          <div class="mini-row">
            <span class="mini-label">活跃模型数</span>
            <span class="mini-value">{{ legendModels.length }}</span>
          </div>
          <div class="mini-divider" />
          <div class="mini-row">
            <span class="mini-label">30天总额</span>
            <span class="mini-value money">${{ formatCost(totalChartCost) }}</span>
          </div>
          <div class="mini-divider" />
          <div class="mini-row">
            <span class="mini-label">日均</span>
            <span class="mini-value money">${{ formatCost(totalChartCost / 30) }}</span>
          </div>
        </div>
      </div>
    </section>

    <!-- 账号概览 -->
    <section class="card accounts-card">
      <div class="card-head">
        <h3 class="card-title">账号概览</h3>
        <span class="card-desc muted">{{ accounts.length }} 个账号</span>
      </div>
      <n-empty v-if="accounts.length === 0" description="暂无账号,请前往账号管理添加" />
      <div v-else class="acct-grid">
        <div v-for="account in accounts" :key="account.id" class="acct-cell">
          <div class="acct-top">
            <span class="acct-name">{{ account.name }}</span>
            <span
              class="acct-status"
              :class="account.enabled ? (isCoolingDown(account) ? 'cooling' : 'active') : 'disabled'"
            >
              {{ statusLabel(account) }}
            </span>
          </div>
          <div class="acct-usage mono muted">{{ getUsageText(account.id) }}</div>
        </div>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import {
  NIcon,
  NButton,
  NEmpty,
  NSpin,
  useMessage,
} from "naive-ui";
import {
  KeyOutlined,
  CalendarOutlined,
  ClockCircleOutlined,
  WalletOutlined,
  CopyOutlined,
} from "@vicons/antd";
import StackedBarChart from "../components/StackedBarChart.vue";
import { tauriApi } from "../api/tauri";
import type { Account, AppConfig, UsageWindow, DashboardSummary, DailyModelCost } from "../api/tauri";

const message = useMessage();
const characterImage = new URL("../../assets/opencode娘.png", import.meta.url).href;
const accounts = ref<Account[]>([]);
const usageMap = ref<Record<string, UsageWindow>>({});
const dailyCosts = ref<DailyModelCost[]>([]);
const loading = ref(true);
const serviceConfig = ref<AppConfig>({
  gateway_port: 9042,
  gateway_key: "",
  upstream_base_url: "",
  auto_start: false,
});
const summary = ref<DashboardSummary>({
  total_accounts: 0,
  available_accounts: 0,
  today_cost: 0,
  week_cost: 0,
  month_cost: 0,
});

// --- 图表调色板,与 StackedBarChart 保持一致 ---
const palette = [
  "#18a058", "#2080f0", "#f0a020", "#d03050",
  "#7a4df0", "#0fb5a8", "#909399", "#e08040",
];

const legendModels = computed(() => {
  const totals = new Map<string, number>();
  for (const r of dailyCosts.value) {
    totals.set(r.model, (totals.get(r.model) ?? 0) + r.cost);
  }
  const models = [...totals.keys()].sort((a, b) => (totals.get(b)! - totals.get(a)!));
  return models.map((m, i) => ({ model: m, color: palette[i % palette.length] }));
});

const totalChartCost = computed(() =>
  dailyCosts.value.reduce((s, r) => s + r.cost, 0)
);

function formatCost(v: number): string {
  if (v === 0) return "0.00";
  if (v < 0.01) return v.toFixed(4);
  return v.toFixed(2);
}

const maskedKey = computed(() => {
  const key = serviceConfig.value.gateway_key;
  if (!key) return "未设置";
  if (key.length <= 8) return key;
  return key.slice(0, 4) + "…" + key.slice(-4);
});
const serviceApiUrl = window.location.pathname.startsWith("/dashboard")
  ? `${window.location.origin}/v1`
  : "http://127.0.0.1:9042/v1";

function isCoolingDown(account: Account): boolean {
  if (!account.cooldown_until) return false;
  const until = Date.parse(account.cooldown_until);
  return Number.isFinite(until) && until > Date.now();
}

function statusLabel(account: Account): string {
  if (!account.enabled) return "已禁用";
  return isCoolingDown(account) ? "冷却中" : "可用";
}

function getUsageText(accountId: string) {
  const usage = usageMap.value[accountId];
  if (!usage) return "暂无用量";
  return `5h $${usage.window_5h.toFixed(3)} · 周 $${usage.window_week.toFixed(3)} · 月 $${usage.window_month.toFixed(3)}`;
}

async function copyKey() {
  if (!serviceConfig.value.gateway_key) return;
  try {
    await navigator.clipboard.writeText(serviceConfig.value.gateway_key);
    message.success("已复制 Gateway Key");
  } catch {
    message.error("复制失败");
  }
}

onMounted(async () => {
  try {
    accounts.value = await tauriApi.getAccounts();
    serviceConfig.value = await tauriApi.getSettings();
    summary.value = await tauriApi.getDashboardSummary();
    dailyCosts.value = await tauriApi.getDailyCostByModel(30);
    for (const account of accounts.value) {
      usageMap.value[account.id] = await tauriApi.getAccountUsage(account.id);
    }
  } catch (e) {
    message.error(`加载仪表盘失败: ${e}`);
  } finally {
    loading.value = false;
  }
});
</script>

<style scoped>
.dashboard {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.hero-card {
  position: relative;
  min-height: 168px;
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.9);
  border-radius: 16px;
  background:
    radial-gradient(circle at 74% 38%, rgba(122, 77, 240, 0.12), transparent 30%),
    linear-gradient(112deg, rgba(239, 250, 246, 0.96) 0%, rgba(246, 247, 252, 0.9) 58%, rgba(234, 237, 245, 0.78) 100%);
  box-shadow: 0 8px 32px rgba(24, 38, 56, 0.07);
}
.hero-copy {
  position: relative;
  z-index: 1;
  padding: 34px 36px;
}
.hero-kicker {
  color: #18a058;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
.hero-copy h2 {
  margin: 8px 0 6px;
  font-size: 26px;
  line-height: 1.25;
}
.hero-copy p {
  margin: 0;
  color: #667085;
}
.hero-character {
  position: absolute;
  right: 4%;
  top: -26px;
  width: 330px;
  max-width: 48%;
  mix-blend-mode: multiply;
  pointer-events: none;
  user-select: none;
}
@media (max-width: 720px) {
  .hero-copy {
    padding: 28px 24px;
  }
  .hero-character {
    right: -60px;
    max-width: 62%;
    opacity: 0.38;
  }
}

/* --- KPI 卡片行 --- */
.kpi-row {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
}
@media (max-width: 1080px) {
  .kpi-row {
    grid-template-columns: repeat(2, 1fr);
  }
}

.kpi-card {
  display: flex;
  flex-direction: column;
  padding: 16px 18px;
  background: rgba(255, 255, 255, 0.86);
  backdrop-filter: blur(5px);
  border: 1px solid var(--n-border-color, rgba(0, 0, 0, 0.05));
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03);
}
.kpi-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}
.kpi-badge {
  width: 28px;
  height: 28px;
  border-radius: 8px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.kpi-badge.green {
  background: rgba(24, 160, 88, 0.12);
  color: #18a058;
}
.kpi-badge.blue {
  background: rgba(32, 128, 240, 0.12);
  color: #2080f0;
}
.kpi-badge.amber {
  background: rgba(240, 160, 32, 0.12);
  color: #f0a020;
}
.kpi-badge.violet {
  background: rgba(122, 77, 240, 0.12);
  color: #7a4df0;
}
.kpi-label {
  font-size: 12px;
  color: var(--n-text-color-3, #888);
}
.kpi-main {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.kpi-value {
  font-size: 26px;
  font-weight: 700;
  line-height: 1.1;
  font-variant-numeric: tabular-nums;
  color: var(--n-text-color, #111);
}
.kpi-value.money {
  letter-spacing: -0.02em;
}
.kpi-unit {
  font-size: 16px;
  font-weight: 600;
  color: var(--n-text-color-3, #888);
}
.kpi-title {
  font-size: 12px;
  color: var(--n-text-color-3, #888);
}

/* --- 通用卡片 --- */
.card {
  background: rgba(255, 255, 255, 0.86);
  backdrop-filter: blur(5px);
  border: 1px solid var(--n-border-color, rgba(0, 0, 0, 0.05));
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03);
}
.card-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 14px 18px 8px;
  gap: 12px;
}
.card-title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--n-text-color, #111);
}
.card-desc {
  margin: 2px 0 0;
  font-size: 12px;
  color: var(--n-text-color-3, #888);
}
.muted {
  color: var(--n-text-color-3, #888);
}

/* --- 主区网格 --- */
.main-grid {
  display: grid;
  grid-template-columns: 1fr 320px;
  gap: 16px;
}
@media (max-width: 1080px) {
  .main-grid {
    grid-template-columns: 1fr;
  }
}
.chart-card {
  padding-bottom: 12px;
}
.chart-card :deep(.n-spin-content) {
  padding: 8px 12px 4px;
}
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 14px;
  max-width: 60%;
  justify-content: flex-end;
}
.legend-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: var(--n-text-color-2, #555);
}
.legend-item .dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

/* --- 侧栏 --- */
.side-stack {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.service-card {
  padding-bottom: 14px;
}
.gw-rows {
  padding: 0 18px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.gw-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.gw-row.align-top {
  align-items: flex-start;
}
.gw-key {
  font-size: 12px;
  color: var(--n-text-color-3, #888);
  flex: 0 0 auto;
}
.gw-val {
  font-size: 13px;
  color: var(--n-text-color, #222);
  text-align: right;
  word-break: break-all;
}
.gw-val.mono,
.mono {
  font-family: v-mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-variant-numeric: tabular-nums;
}
.key-box {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.mini-stats {
  padding: 14px 18px 16px;
}
.mini-row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}
.mini-label {
  font-size: 12px;
  color: var(--n-text-color-3, #888);
}
.mini-value {
  font-size: 16px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--n-text-color, #111);
}
.mini-value.money {
  letter-spacing: -0.01em;
}
.mini-divider {
  height: 1px;
  background: var(--n-divider-color, rgba(0, 0, 0, 0.06));
  margin: 10px 0;
}

/* --- 账号概览 --- */
.accounts-card {
  padding-bottom: 16px;
}
.acct-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 12px;
  padding: 8px 18px 0;
}
.acct-cell {
  padding: 12px 14px;
  border: 1px solid var(--n-border-color, rgba(0, 0, 0, 0.05));
  border-radius: 10px;
  background: var(--n-body-color, rgba(255, 255, 255, 0.04));
}
.acct-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}
.acct-name {
  font-weight: 600;
  font-size: 14px;
  color: var(--n-text-color, #111);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.acct-status {
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 999px;
  flex: 0 0 auto;
}
.acct-status.active {
  background: rgba(24, 160, 88, 0.12);
  color: #18a058;
}
.acct-status.cooling {
  background: rgba(240, 160, 32, 0.14);
  color: #c88012;
}
.acct-status.disabled {
  background: rgba(127, 127, 127, 0.14);
  color: var(--n-text-color-3, #888);
}
.acct-usage {
  font-size: 11px;
  line-height: 1.5;
}
</style>
