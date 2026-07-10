<template>
  <n-space vertical :size="16">
    <n-space justify="space-between" align="center">
      <n-h3 style="margin: 0">账号管理</n-h3>
    </n-space>

    <n-card title="快速创建" size="small">
      <n-form :model="newAccount" label-placement="top" :show-feedback="false">
        <div class="quick-grid">
          <n-form-item label="名称">
            <n-input v-model:value="newAccount.name" placeholder="主号" />
          </n-form-item>
          <n-form-item label="账号">
            <n-input v-model:value="newAccount.username" placeholder="OpenCode-Go 账号" />
          </n-form-item>
          <n-form-item label="密码">
            <n-input v-model:value="newAccount.password" placeholder="OpenCode-Go 密码" />
          </n-form-item>
          <n-form-item label="API Key">
            <n-input v-model:value="newAccount.key" placeholder="sk-..." />
          </n-form-item>
          <n-button type="primary" :loading="busy" @click="createAccount">保存</n-button>
        </div>
      </n-form>
    </n-card>

    <n-empty v-if="accounts.length === 0" description="暂无账号" />

    <n-card v-for="account in accounts" :key="account.id" size="small" class="account-card">
      <template #header>
        <div class="account-title">
          <span>{{ account.name }}</span>
          <n-tag :type="account.enabled ? 'success' : 'default'" size="small">
            {{ account.enabled ? "已启用" : "已禁用" }}
          </n-tag>
          <n-tooltip v-if="isCooling(account)" :disabled="!account.last_error">
            <template #trigger>
              <n-tag type="error" size="small">冷却中 · 剩 {{ formatRemaining(account) }}</n-tag>
            </template>
            {{ account.last_error }}
          </n-tooltip>
        </div>
      </template>
      <template #header-extra>
        <n-space align="center" :size="8">
          <n-switch :value="account.enabled" @update:value="toggleAccount(account.id)">
            <template #checked>启用</template>
            <template #unchecked>禁用</template>
          </n-switch>
          <n-button quaternary size="small" @click="toggleExpanded(account.id)">
            {{ expanded[account.id] ? "收起" : "展开" }}
          </n-button>
        </n-space>
      </template>

      <div class="usage-stack">
        <div v-for="limit in usageLimits" :key="limit.key" class="usage-row">
          <div class="usage-meta">
            <span>{{ limit.label }}</span>
            <strong>${{ formatCost(usageCost(account.id, limit.key)) }} / ${{ limit.limit }}</strong>
          </div>
          <n-progress
            type="line"
            :height="8"
            :percentage="usagePercent(account.id, limit.key, limit.limit)"
            :status="usageStatus(account.id, limit.key, limit.limit)"
            :show-indicator="false"
          />
        </div>
      </div>

      <div v-if="expanded[account.id]" class="account-detail">
        <n-form :model="drafts[account.id]" label-placement="top" :show-feedback="false">
          <div class="detail-grid">
            <n-form-item label="名称">
              <n-input v-model:value="drafts[account.id].name" />
            </n-form-item>
            <n-form-item label="账号">
              <n-input v-model:value="drafts[account.id].username" />
            </n-form-item>
            <n-form-item label="密码">
              <n-input v-model:value="drafts[account.id].password" />
            </n-form-item>
            <n-form-item label="API Key">
              <n-input
                v-model:value="drafts[account.id].key"
                type="textarea"
                :autosize="{ minRows: 2, maxRows: 4 }"
              />
            </n-form-item>
          </div>
        </n-form>
        <n-space justify="end" align="center">
          <n-button
            v-if="isCooling(account)"
            size="small"
            type="warning"
            @click="resetCooldown(account.id)"
          >
            重置冷却
          </n-button>
          <n-button size="small" type="primary" :loading="busy" @click="saveAccount(account)">
            保存
          </n-button>
          <n-popconfirm
            positive-text="删除"
            negative-text="取消"
            @positive-click="deleteAccount(account.id)"
          >
            <template #trigger>
              <n-button size="small" type="error">删除</n-button>
            </template>
            确定删除账号 {{ account.name }} 吗？
          </n-popconfirm>
        </n-space>
      </div>
    </n-card>
  </n-space>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  NButton,
  NCard,
  NEmpty,
  NForm,
  NFormItem,
  NH3,
  NInput,
  NPopconfirm,
  NProgress,
  NSpace,
  NSwitch,
  NTag,
  NTooltip,
  useMessage,
} from "naive-ui";
import { tauriApi } from "../api/tauri";
import type { Account, AccountInput, AccountUpdate, UsageWindow } from "../api/tauri";

type UsageKey = "window_5h" | "window_week" | "window_month";
type AccountDraft = {
  name: string;
  username: string;
  password: string;
  key: string;
};

const usageLimits: Array<{ key: UsageKey; label: string; limit: number }> = [
  { key: "window_5h", label: "5h", limit: 12 },
  { key: "window_week", label: "本周", limit: 30 },
  { key: "window_month", label: "本月", limit: 60 },
];

const message = useMessage();
const accounts = ref<Account[]>([]);
const usageMap = ref<Record<string, UsageWindow>>({});
const drafts = ref<Record<string, AccountDraft>>({});
const expanded = ref<Record<string, boolean>>({});
const busy = ref(false);
const newAccount = ref<AccountDraft>({
  name: "",
  username: "",
  password: "",
  key: "",
});

function blankUsage(accountId: string): UsageWindow {
  return {
    account_id: accountId,
    window_5h: 0,
    window_week: 0,
    window_month: 0,
  };
}

function draftFromAccount(account: Account): AccountDraft {
  return {
    name: account.name,
    username: account.username,
    password: account.password,
    key: account.key,
  };
}

function getUsage(accountId: string): UsageWindow {
  return usageMap.value[accountId] || blankUsage(accountId);
}

function usageCost(accountId: string, key: UsageKey): number {
  return getUsage(accountId)[key];
}

function usagePercent(accountId: string, key: UsageKey, limit: number): number {
  return Math.min(100, Math.round((usageCost(accountId, key) / limit) * 1000) / 10);
}

function usageStatus(
  accountId: string,
  key: UsageKey,
  limit: number,
): "success" | "warning" | "error" {
  const percent = usagePercent(accountId, key, limit);
  if (percent >= 100) return "error";
  if (percent >= 80) return "warning";
  return "success";
}

function formatCost(value: number): string {
  if (value === 0) return "0.00";
  if (value < 0.01) return value.toFixed(4);
  return value.toFixed(2);
}

function isCooling(account: Account): boolean {
  if (!account.cooldown_until) return false;
  return new Date(account.cooldown_until).getTime() > Date.now();
}

function formatRemaining(account: Account): string {
  if (!account.cooldown_until) return "";
  const ms = new Date(account.cooldown_until).getTime() - Date.now();
  if (ms <= 0) return "0s";
  const min = Math.floor(ms / 60000);
  if (min < 60) return `${min}min`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h${min % 60}m`;
  const day = Math.floor(hr / 24);
  return `${day}天${hr % 24}h`;
}

function toggleExpanded(id: string) {
  expanded.value[id] = !expanded.value[id];
}

function trimmedDraft(draft: AccountDraft): AccountInput {
  return {
    name: draft.name.trim(),
    username: draft.username.trim(),
    password: draft.password.trim(),
    key: draft.key.trim(),
  };
}

async function loadAccounts() {
  try {
    const loaded = await tauriApi.getAccounts();
    const nextDrafts: Record<string, AccountDraft> = {};
    const nextUsage: Record<string, UsageWindow> = {};
    for (const account of loaded) {
      nextDrafts[account.id] = drafts.value[account.id] || draftFromAccount(account);
      nextUsage[account.id] = await tauriApi.getAccountUsage(account.id);
    }
    accounts.value = loaded;
    drafts.value = nextDrafts;
    usageMap.value = nextUsage;
  } catch (e) {
    message.error(`加载账号失败: ${e}`);
  }
}

async function createAccount() {
  const input = trimmedDraft(newAccount.value);
  if (!input.name || !input.key) {
    message.warning("请填写名称和 API Key");
    return;
  }
  busy.value = true;
  try {
    await tauriApi.createAccount(input);
    newAccount.value = { name: "", username: "", password: "", key: "" };
    message.success("账号已添加");
    await loadAccounts();
  } catch (e) {
    message.error(`保存失败: ${e}`);
  } finally {
    busy.value = false;
  }
}

async function saveAccount(account: Account) {
  const draft = drafts.value[account.id];
  const update: AccountUpdate = trimmedDraft(draft);
  if (!update.name || !update.key) {
    message.warning("名称和 API Key 不能为空");
    return;
  }
  busy.value = true;
  try {
    await tauriApi.updateAccount(account.id, update);
    message.success("账号已更新");
    await loadAccounts();
  } catch (e) {
    message.error(`保存失败: ${e}`);
  } finally {
    busy.value = false;
  }
}

async function toggleAccount(id: string) {
  try {
    await tauriApi.toggleAccount(id);
    await loadAccounts();
  } catch (e) {
    message.error(`切换失败: ${e}`);
  }
}

async function deleteAccount(id: string) {
  try {
    await tauriApi.deleteAccount(id);
    message.success("账号已删除");
    await loadAccounts();
  } catch (e) {
    message.error(`删除失败: ${e}`);
  }
}

async function resetCooldown(id: string) {
  try {
    await tauriApi.resetAccountCooldown(id);
    message.success("已重置冷却");
    await loadAccounts();
  } catch (e) {
    message.error(`重置失败: ${e}`);
  }
}

onMounted(loadAccounts);
</script>

<style scoped>
.quick-grid,
.detail-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(160px, 1fr)) auto;
  gap: 12px;
  align-items: end;
}

.detail-grid {
  grid-template-columns: repeat(3, minmax(180px, 1fr));
  margin-bottom: 12px;
}

.detail-grid :deep(.n-form-item:last-child) {
  grid-column: 1 / -1;
}

.account-card :deep(.n-card-header) {
  align-items: center;
}

.account-title {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.account-title > span:first-child {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.usage-stack {
  display: grid;
  gap: 10px;
}

.usage-row {
  display: grid;
  gap: 4px;
}

.usage-meta {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
  color: var(--n-text-color-2);
}

.usage-meta strong {
  color: var(--n-text-color);
  font-weight: 600;
}

.account-detail {
  margin-top: 14px;
  padding-top: 14px;
  border-top: 1px solid var(--n-border-color);
}

@media (max-width: 900px) {
  .quick-grid,
  .detail-grid {
    grid-template-columns: 1fr;
  }
}
</style>
