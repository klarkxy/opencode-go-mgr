<template>
  <n-space vertical :size="16">
    <n-tabs v-model:value="activeTab" type="line">
      <n-tab-pane name="gateway" tab="网关日志">
        <n-data-table
          :columns="gatewayColumns"
          :data="gatewayLogs"
          :pagination="pagination"
          size="small"
        />
      </n-tab-pane>
      <n-tab-pane name="forward" tab="透传日志">
        <n-space vertical :size="12">
          <n-space>
            <n-select
              v-model:value="statusFilter"
              :options="statusOptions"
              placeholder="状态筛选"
              clearable
              style="width: 140px"
            />
            <n-select
              v-model:value="accountFilter"
              :options="accountOptions"
              placeholder="账号筛选"
              clearable
              style="width: 160px"
            />
          </n-space>
          <n-data-table
            :columns="forwardColumns"
            :data="filteredForwardLogs"
            :pagination="pagination"
            size="small"
          />
        </n-space>
      </n-tab-pane>
    </n-tabs>
  </n-space>
</template>

<script setup lang="ts">
import { ref, onMounted, h, computed } from "vue";
import {
  NSpace,
  NTabs,
  NTabPane,
  NDataTable,
  NSelect,
  useMessage,
  NTag,
} from "naive-ui";
import { tauriApi } from "../api/tauri";
import type { GatewayLog, ForwardLog, Account } from "../api/tauri";

const message = useMessage();
const activeTab = ref("gateway");
const gatewayLogs = ref<GatewayLog[]>([]);
const forwardLogs = ref<ForwardLog[]>([]);
const accounts = ref<Account[]>([]);
const statusFilter = ref<string | null>(null);
const accountFilter = ref<string | null>(null);
const pagination = { pageSize: 20 };

const statusOptions = [
  { label: "成功", value: "success" },
  { label: "成功(无用量)", value: "success_no_usage" },
  { label: "进行中", value: "streaming" },
  { label: "客户端错误", value: "client_error" },
  { label: "错误", value: "error" },
];

const accountOptions = computed(() =>
  accounts.value.map((a) => ({ label: a.name, value: a.id }))
);

const filteredForwardLogs = computed(() => {
  return forwardLogs.value.filter((log) => {
    if (statusFilter.value && log.status !== statusFilter.value) return false;
    if (accountFilter.value && log.account_id !== accountFilter.value) return false;
    return true;
  });
});

const gatewayColumns = [
  { title: "时间", key: "created_at", width: 180 },
  { title: "级别", key: "level", width: 80 },
  { title: "分类", key: "category", width: 100 },
  { title: "消息", key: "message" },
];

const forwardColumns = [
  { title: "时间", key: "timestamp", width: 180 },
  { title: "模型", key: "model", width: 160 },
  { title: "账号", key: "account_name", width: 120 },
  {
    title: "状态",
    key: "status",
    width: 80,
    render: (row: ForwardLog) =>
      h(
        NTag,
        { type: row.status === "success" ? "success" : "error", size: "small" },
        { default: () => row.status }
      ),
  },
  { title: "HTTP", key: "http_status", width: 80 },
  { title: "Prompt", key: "prompt_tokens", width: 90 },
  { title: "Completion", key: "completion_tokens", width: 100 },
  { title: "Cached", key: "cached_tokens", width: 90 },
  {
    title: "成本",
    key: "cost",
    width: 100,
    render: (row: ForwardLog) => `$${row.cost.toFixed(5)}`,
  },
  { title: "错误", key: "error_message", ellipsis: true },
];

onMounted(async () => {
  try {
    gatewayLogs.value = await tauriApi.getGatewayLogs(200);
    forwardLogs.value = await tauriApi.getForwardLogs(200);
    accounts.value = await tauriApi.getAccounts();
  } catch (e) {
    message.error(`加载日志失败: ${e}`);
  }
});
</script>
