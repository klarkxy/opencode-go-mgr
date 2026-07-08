<template>
  <n-space vertical :size="16">
    <n-grid :cols="4" :x-gap="16" :y-gap="16">
      <n-gi>
        <n-card title="账号统计">
          <n-statistic label="可用 / 总数">
            <span>{{ summary.available_accounts }} / {{ summary.total_accounts }}</span>
          </n-statistic>
        </n-card>
      </n-gi>
      <n-gi>
        <n-card title="今日用量">
          <n-statistic :value="summary.today_cost" suffix="$" />
        </n-card>
      </n-gi>
      <n-gi>
        <n-card title="本周用量">
          <n-statistic :value="summary.week_cost" suffix="$" />
        </n-card>
      </n-gi>
      <n-gi>
        <n-card title="本月用量">
          <n-statistic :value="summary.month_cost" suffix="$" />
        </n-card>
      </n-gi>
    </n-grid>

    <n-card title="Gateway 状态">
      <n-descriptions bordered :column="2">
        <n-descriptions-item label="运行状态">
          <n-tag :type="gatewayStatus.running ? 'success' : 'error'">
            {{ gatewayStatus.running ? "运行中" : "已停止" }}
          </n-tag>
        </n-descriptions-item>
        <n-descriptions-item label="监听端口">
          {{ gatewayStatus.port }}
        </n-descriptions-item>
        <n-descriptions-item label="Gateway Key">
          <n-space>
            <span>{{ maskedKey }}</span>
            <n-button size="tiny" @click="copyKey">复制</n-button>
          </n-space>
        </n-descriptions-item>
        <n-descriptions-item label="上游地址">
          {{ gatewayStatus.upstream_base_url }}
        </n-descriptions-item>
      </n-descriptions>
    </n-card>

    <n-card title="账号概览">
      <n-empty v-if="accounts.length === 0" description="暂无账号，请前往账号管理添加" />
      <n-list v-else>
        <n-list-item v-for="account in accounts" :key="account.id">
          <n-thing :title="account.name">
            <template #description>
              <n-space>
                <n-tag :type="account.enabled ? 'success' : 'default'">
                  {{ account.enabled ? "已启用" : "已禁用" }}
                </n-tag>
                <span class="text-muted">{{ getUsageText(account.id) }}</span>
              </n-space>
            </template>
          </n-thing>
        </n-list-item>
      </n-list>
    </n-card>
  </n-space>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import {
  NSpace,
  NGrid,
  NGi,
  NCard,
  NStatistic,
  NDescriptions,
  NDescriptionsItem,
  NTag,
  NButton,
  NList,
  NListItem,
  NThing,
  NEmpty,
  useMessage,
} from "naive-ui";
import { tauriApi, Account, GatewayStatus, UsageWindow, DashboardSummary } from "../api/tauri";

const message = useMessage();
const accounts = ref<Account[]>([]);
const usageMap = ref<Record<string, UsageWindow>>({});
const gatewayStatus = ref<GatewayStatus>({
  running: false,
  port: 9042,
  key: "",
  upstream_base_url: "",
});
const summary = ref<DashboardSummary>({
  total_accounts: 0,
  available_accounts: 0,
  gateway_running: false,
  today_cost: 0,
  week_cost: 0,
  month_cost: 0,
});

const maskedKey = computed(() => {
  const key = gatewayStatus.value.key;
  if (key.length <= 8) return key || "未设置";
  return key.slice(0, 4) + "..." + key.slice(-4);
});

function getUsageText(accountId: string) {
  const usage = usageMap.value[accountId];
  if (!usage) return "";
  return `5h: $${usage.window_5h.toFixed(3)} / 周: $${usage.window_week.toFixed(3)} / 月: $${usage.window_month.toFixed(3)}`;
}

async function copyKey() {
  if (!gatewayStatus.value.key) return;
  try {
    await navigator.clipboard.writeText(gatewayStatus.value.key);
    message.success("已复制 Gateway Key");
  } catch {
    message.error("复制失败");
  }
}

onMounted(async () => {
  try {
    accounts.value = await tauriApi.getAccounts();
    gatewayStatus.value = await tauriApi.getGatewayStatus();
    summary.value = await tauriApi.getDashboardSummary();
    for (const account of accounts.value) {
      usageMap.value[account.id] = await tauriApi.getAccountUsage(account.id);
    }
  } catch (e) {
    message.error(`加载仪表盘失败: ${e}`);
  }
});
</script>
