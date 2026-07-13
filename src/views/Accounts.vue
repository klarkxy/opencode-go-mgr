<template>
  <div class="accounts-view">
  <n-space vertical :size="16" class="accounts-content">
    <n-space justify="space-between" align="center" class="accounts-toolbar">
      <n-h3 style="margin: 0">{{ t("账号管理") }}</n-h3>
      <n-button type="primary" @click="showCreateModal = true">
        <template #icon>
          <n-icon :component="PlusOutlined" />
        </template>
        {{ t("新增账号") }}
      </n-button>
    </n-space>

    <n-empty v-if="accounts.length === 0" :description="t('暂无账号')" />

    <n-card
      v-for="account in accounts"
      :key="account.id"
      size="small"
      class="account-card"
      :class="{ 'account-card--cooling': isCooling(account) }"
    >
      <template #header>
        <div class="account-title">
          <span>{{ account.name }}</span>
          <n-tag :type="account.enabled ? 'success' : 'default'" size="small">
            {{ account.enabled ? t("已启用") : t("已禁用") }}
          </n-tag>
          <n-tooltip v-if="isCooling(account)" :disabled="!account.last_error">
            <template #trigger>
              <n-tag type="error" size="small">{{ t("熔断 · 剩 {time}", { time: formatRemaining(account) }) }}</n-tag>
            </template>
            {{ account.last_error }}
          </n-tooltip>
        </div>
      </template>
      <template #header-extra>
        <n-space align="center" :size="8">
          <n-tooltip trigger="hover">
            <template #trigger>
              <n-button
                circle
                quaternary
                size="small"
                :aria-label="t('测试账号 {name}', { name: account.name })"
                :loading="pinging[account.id]"
                @click="pingAccount(account.id)"
              >
                <template #icon><n-icon :component="ThunderboltOutlined" /></template>
              </n-button>
            </template>
            {{ t("测试连接") }}
          </n-tooltip>
          <n-switch
            :value="account.enabled"
            :aria-label="account.enabled ? t('禁用账号 {name}', { name: account.name }) : t('启用账号 {name}', { name: account.name })"
            @update:value="toggleAccount(account.id)"
          >
            <template #checked>{{ t("启用") }}</template>
            <template #unchecked>{{ t("禁用") }}</template>
          </n-switch>
          <n-tooltip trigger="hover">
            <template #trigger>
              <n-button
                circle
                quaternary
                size="small"
                :aria-label="expanded[account.id] ? t('收起账号 {name}', { name: account.name }) : t('展开账号 {name}', { name: account.name })"
                @click="toggleExpanded(account.id)"
              >
                <template #icon>
                  <n-icon :component="expanded[account.id] ? UpOutlined : DownOutlined" />
                </template>
              </n-button>
            </template>
            {{ expanded[account.id] ? t("收起") : t("展开") }}
          </n-tooltip>
          <n-popconfirm
            :positive-text="t('删除')"
            :negative-text="t('取消')"
            @positive-click="deleteAccount(account.id)"
          >
            <template #trigger>
              <n-tooltip trigger="hover">
                <template #trigger>
                  <n-button
                    circle
                    quaternary
                    size="small"
                    type="error"
                    :aria-label="t('删除账号 {name}', { name: account.name })"
                  >
                    <template #icon><n-icon :component="DeleteOutlined" /></template>
                  </n-button>
                </template>
                {{ t("删除账号") }}
              </n-tooltip>
            </template>
            {{ t("确定删除账号 {name} 吗？", { name: account.name }) }}
          </n-popconfirm>
        </n-space>
      </template>

      <div class="usage-strip">
        <div v-for="limit in usageLimits" :key="limit.key" class="usage-segment">
          <div class="usage-meta">
            <span>{{ limit.label }}</span>
            <strong>{{ formatCost(usageCost(account.id, limit.key)) }} / {{ formatCost(limit.limit) }}</strong>
          </div>
          <n-progress
            type="line"
            :height="8"
            :percentage="usagePercent(account, limit.key, limit.limit)"
            :status="usageStatus(account, limit.key, limit.limit)"
            :show-indicator="false"
          />
        </div>
      </div>

      <div v-if="expanded[account.id]" class="account-detail">
        <n-form :model="drafts[account.id]" label-placement="top" :show-feedback="false">
          <div class="detail-grid">
            <n-form-item :label="t('名称')">
              <n-input
                v-model:value="drafts[account.id].name"
                :input-props="{ 'aria-label': t('{name} 名称', { name: account.name }) }"
              />
            </n-form-item>
            <n-form-item :label="t('账号')">
              <n-input
                v-model:value="drafts[account.id].username"
                :input-props="{ 'aria-label': t('{name} 登录账号', { name: account.name }) }"
                :placeholder="t('OpenCode-Go 账号')"
              />
            </n-form-item>
            <n-form-item :label="t('密码')">
              <div class="secret-field">
                <n-input
                  v-model:value="drafts[account.id].password"
                  :input-props="{ 'aria-label': t('{name} 密码', { name: account.name }) }"
                  type="password"
                  show-password-on="click"
                  :placeholder="t('留空不修改')"
                  :disabled="drafts[account.id].clearPassword"
                />
                <n-button
                  text
                  size="tiny"
                  type="warning"
                  @click="drafts[account.id].clearPassword = !drafts[account.id].clearPassword"
                >
                  {{ drafts[account.id].clearPassword ? t("取消清除密码") : t("清除已存密码") }}
                </n-button>
              </div>
            </n-form-item>
            <n-form-item label="API Key">
              <n-input
                v-model:value="drafts[account.id].key"
                :input-props="{ 'aria-label': t('{name} API Key', { name: account.name }) }"
                type="password"
                show-password-on="click"
                :placeholder="t('留空不修改')"
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
            {{ t("重置冷却") }}
          </n-button>
          <n-button size="small" type="primary" :loading="busy" @click="saveAccount(account)">
            {{ t("保存") }}
          </n-button>
        </n-space>
      </div>
    </n-card>
  </n-space>

  <n-modal
    v-model:show="showCreateModal"
    preset="card"
    :title="t('新增账号')"
    class="account-modal"
    style="width: 560px; max-width: calc(100vw - 32px)"
    :mask-closable="false"
  >
    <n-form :model="newAccount" label-placement="top" :show-feedback="false">
      <div class="modal-grid">
        <n-form-item :label="t('名称')">
          <n-input
            v-model:value="newAccount.name"
            :input-props="{ 'aria-label': t('名称') }"
            :placeholder="t('主号')"
          />
        </n-form-item>
        <n-form-item :label="t('账号')">
          <n-input
            v-model:value="newAccount.username"
            :input-props="{ 'aria-label': t('登录账号') }"
            :placeholder="t('OpenCode-Go 账号')"
          />
        </n-form-item>
        <n-form-item :label="t('密码')">
          <n-input
            v-model:value="newAccount.password"
            :input-props="{ 'aria-label': t('密码') }"
            type="password"
            show-password-on="click"
            :placeholder="t('OpenCode-Go 密码')"
          />
        </n-form-item>
        <n-form-item label="API Key">
          <n-input
            v-model:value="newAccount.key"
            :input-props="{ 'aria-label': 'API Key' }"
            type="password"
            show-password-on="click"
            placeholder="sk-..."
          />
        </n-form-item>
      </div>
    </n-form>
    <template #footer>
      <n-space justify="end">
        <n-button @click="showCreateModal = false">{{ t("取消") }}</n-button>
        <n-button type="primary" :loading="busy" @click="createAccount">{{ t("保存") }}</n-button>
      </n-space>
    </template>
  </n-modal>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  NButton,
  NCard,
  NEmpty,
  NForm,
  NFormItem,
  NH3,
  NIcon,
  NInput,
  NModal,
  NPopconfirm,
  NProgress,
  NSpace,
  NSwitch,
  NTag,
  NTooltip,
  useMessage,
} from "naive-ui";
import {
  DeleteOutlined,
  DownOutlined,
  PlusOutlined,
  ThunderboltOutlined,
  UpOutlined,
} from "@vicons/antd";
import { DashboardRequestError, tauriApi } from "../api/tauri";
import type { Account, AccountInput, AccountUpdate, UsageWindow } from "../api/tauri";
import { isCooling, isUsageLimitReached, usageProgressStatus } from "./accounts-usage";
import type { UsageKey } from "./accounts-usage";
import { locale, t } from "../i18n/index.ts";

type AccountDraft = {
  name: string;
  username: string;
  password: string;
  key: string;
  clearPassword: boolean;
};

const usageLimits = computed<Array<{ key: UsageKey; label: string; limit: number }>>(() => [
  { key: "window_5h", label: "5h", limit: 12 },
  { key: "window_week", label: t("本周"), limit: 30 },
  { key: "window_month", label: t("本月"), limit: 60 },
]);

const message = useMessage();
const accounts = ref<Account[]>([]);
const usageMap = ref<Record<string, UsageWindow>>({});
const drafts = ref<Record<string, AccountDraft>>({});
const expanded = ref<Record<string, boolean>>({});
const pinging = ref<Record<string, boolean>>({});
const showCreateModal = ref(false);
const busy = ref(false);
const newAccount = ref<AccountDraft>({
  name: "",
  username: "",
  password: "",
  key: "",
  clearPassword: false,
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
    password: "",
    key: "",
    clearPassword: false,
  };
}

function getUsage(accountId: string): UsageWindow {
  return usageMap.value[accountId] || blankUsage(accountId);
}

function usageCost(accountId: string, key: UsageKey): number {
  return getUsage(accountId)[key];
}

function usagePercent(account: Account, key: UsageKey, limit: number): number {
  if (isUsageLimitReached(account, key)) return 100;
  return Math.min(100, Math.round((usageCost(account.id, key) / limit) * 1000) / 10);
}

function usageStatus(
  account: Account,
  key: UsageKey,
  limit: number,
): "success" | "warning" | "error" {
  const percent = usagePercent(account, key, limit);
  return usageProgressStatus(account, key, percent);
}

function formatCost(value: number): string {
  const digits = value !== 0 && value < 0.01 ? 4 : 2;
  return new Intl.NumberFormat(locale.value, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
}

function formatRemaining(account: Account): string {
  if (!account.cooldown_until) return "";
  const ms = new Date(account.cooldown_until).getTime() - Date.now();
  if (ms <= 0) return t("{seconds}秒", { seconds: 0 });
  const min = Math.floor(ms / 60000);
  if (min < 60) return t("{minutes}分钟", { minutes: min });
  const hr = Math.floor(min / 60);
  if (hr < 24) return t("{hours}小时{minutes}分钟", { hours: hr, minutes: min % 60 });
  const day = Math.floor(hr / 24);
  return t("{days}天{hours}小时", { days: day, hours: hr % 24 });
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

function trimmedUpdate(draft: AccountDraft): AccountUpdate {
  const update: AccountUpdate = {
    name: draft.name.trim(),
    username: draft.username.trim(),
  };
  const password = draft.password.trim();
  const key = draft.key.trim();
  if (draft.clearPassword) update.password = "";
  else if (password) update.password = password;
  if (key) update.key = key;
  return update;
}

async function loadAccounts() {
  try {
    const loaded = await tauriApi.getAccounts();
    const nextDrafts: Record<string, AccountDraft> = {};
    for (const account of loaded) {
      nextDrafts[account.id] = drafts.value[account.id] || draftFromAccount(account);
    }
    accounts.value = loaded;
    drafts.value = nextDrafts;
    const usage = await Promise.all(loaded.map(async (account) => {
      try {
        return [account.id, await tauriApi.getAccountUsage(account.id)] as const;
      } catch {
        return [account.id, blankUsage(account.id)] as const;
      }
    }));
    usageMap.value = Object.fromEntries(usage);
  } catch (e) {
    message.error(t("加载账号失败: {error}", { error: String(e) }));
  }
}

async function createAccount() {
  const input = trimmedDraft(newAccount.value);
  if (!input.name || !input.key) {
    message.warning(t("请填写名称和 API Key"));
    return;
  }
  busy.value = true;
  try {
    await tauriApi.createAccount(input);
    newAccount.value = {
      name: "",
      username: "",
      password: "",
      key: "",
      clearPassword: false,
    };
    showCreateModal.value = false;
    message.success(t("账号已添加"));
    await loadAccounts();
  } catch (e) {
    message.error(t("保存失败: {error}", { error: String(e) }));
  } finally {
    busy.value = false;
  }
}

async function saveAccount(account: Account) {
  const draft = drafts.value[account.id];
  const update = trimmedUpdate(draft);
  if (!update.name) {
    message.warning(t("名称不能为空"));
    return;
  }
  busy.value = true;
  try {
    const saved = await tauriApi.updateAccount(account.id, update);
    drafts.value[account.id] = draftFromAccount(saved);
    message.success(t("账号已更新"));
    await loadAccounts();
  } catch (e) {
    message.error(t("保存失败: {error}", { error: String(e) }));
  } finally {
    busy.value = false;
  }
}

async function pingAccount(id: string) {
  pinging.value[id] = true;
  try {
    await tauriApi.testAccount(id);
    message.success(t("账号连接成功"));
  } catch (e) {
    message.error(e instanceof DashboardRequestError && e.status === 429
      ? t("账号达到额度或限流，已进入冷却")
      : t("Ping 失败: {error}", { error: String(e) }));
  } finally {
    pinging.value[id] = false;
    await loadAccounts();
  }
}

async function toggleAccount(id: string) {
  try {
    await tauriApi.toggleAccount(id);
    await loadAccounts();
  } catch (e) {
    message.error(t("切换失败: {error}", { error: String(e) }));
  }
}

async function deleteAccount(id: string) {
  try {
    await tauriApi.deleteAccount(id);
    message.success(t("账号已删除"));
    await loadAccounts();
  } catch (e) {
    message.error(t("删除失败: {error}", { error: String(e) }));
  }
}

async function resetCooldown(id: string) {
  try {
    await tauriApi.resetAccountCooldown(id);
    message.success(t("已重置冷却"));
    await loadAccounts();
  } catch (e) {
    message.error(t("重置失败: {error}", { error: String(e) }));
  }
}

onMounted(loadAccounts);
</script>

<style scoped>
.accounts-view {
  position: relative;
  max-width: 1280px;
  margin: 0 auto;
}

.accounts-content {
  position: relative;
  z-index: 1;
}

.accounts-toolbar {
  min-height: 34px;
}

.detail-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
  align-items: start;
}

.secret-field {
  display: grid;
  gap: 6px;
  width: 100%;
  justify-items: start;
}

.secret-field :deep(.n-input) {
  width: 100%;
}

.account-card :deep(.n-card-header) {
  align-items: center;
}

.account-card {
  border-radius: 14px;
  box-shadow: var(--ocg-shadow-sm);
}

.account-card--cooling {
  border-color: rgba(208, 48, 80, 0.45);
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

.usage-strip {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
}

.usage-segment {
  display: grid;
  gap: 4px;
  min-width: 0;
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

.modal-grid {
  display: grid;
  gap: 12px;
}

.account-detail {
  margin-top: 14px;
  padding-top: 14px;
  border-top: 1px solid var(--n-border-color);
}

@media (max-width: 900px) {
  .detail-grid,
  .usage-strip {
    grid-template-columns: 1fr;
  }

  .account-card :deep(.n-card-header) {
    align-items: flex-start;
  }

  .account-card :deep(.n-card-header__extra) {
    margin-left: 8px;
  }
}
</style>
