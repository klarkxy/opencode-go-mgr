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
      :class="{ 'account-card--cooling': accountIsCooling(account) }"
    >
      <template #header>
        <div class="account-title">
          <span>{{ account.name }}</span>
          <n-tag :type="account.enabled ? 'success' : 'default'" size="small">
            {{ account.enabled ? t("已启用") : t("已禁用") }}
          </n-tag>
          <n-tooltip v-if="accountIsCooling(account)" :disabled="!account.last_error">
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

      <div v-if="usageLoadErrors[account.id]" class="usage-load-error" role="alert">
        <span>{{ t("用量加载失败") }}</span>
        <n-button
          text
          size="tiny"
          type="primary"
          :loading="usageLoading[account.id]"
          @click="loadAccountUsage(account.id)"
        >
          {{ t("重试") }}
        </n-button>
      </div>
      <div v-else class="usage-strip">
        <div v-for="limit in usageLimits" :key="limit.key" class="usage-segment">
          <div class="usage-meta">
            <span>{{ limit.label }}</span>
            <strong v-if="usageEdits[account.id]">
              {{ formatCost(usageCost(account.id, limit.key)) }} / {{ formatCost(limit.limit) }}
            </strong>
          </div>
          <n-progress
            v-if="accountUsageLimitReached(account, limit.key)"
            type="line"
            :height="8"
            :percentage="100"
            status="error"
            :show-indicator="false"
          />
          <template v-else-if="usageEdits[account.id]">
            <div class="usage-editor">
              <n-input-number
                :value="usageEdits[account.id][limit.key].draft"
                :min="0"
                :max="100"
                :step="0.1"
                :precision="1"
                size="tiny"
                :show-button="false"
                :loading="usageEdits[account.id][limit.key].saving"
                :disabled="usageLoading[account.id] || usageEdits[account.id][limit.key].saving"
                :status="usageEdits[account.id][limit.key].error ? 'error' : undefined"
                :input-props="{
                  'aria-label': t('{name} {period} 当前用量百分比', {
                    name: account.name,
                    period: limit.label,
                  }),
                }"
                @update:value="updateUsageDraft(account.id, limit.key, $event)"
                @blur="saveUsage(account.id, limit.key)"
                @keydown.enter.prevent="saveUsage(account.id, limit.key)"
              >
                <template #suffix>%</template>
              </n-input-number>
              <n-slider
                v-usage-slider-label="t('{name} {period} 当前用量百分比', {
                  name: account.name,
                  period: limit.label,
                })"
                :value="usageEdits[account.id][limit.key].draft"
                :min="0"
                :max="100"
                :step="0.1"
                :disabled="usageLoading[account.id] || usageEdits[account.id][limit.key].saving"
                @update:value="updateUsageDraft(account.id, limit.key, $event)"
                @dragend="saveUsage(account.id, limit.key)"
                @focusout="saveUsage(account.id, limit.key)"
              />
            </div>
            <span
              v-if="usageEdits[account.id][limit.key].error"
              class="usage-save-error"
              role="alert"
            >
              {{ t("用量保存失败: {error}", {
                error: usageEdits[account.id][limit.key].error || "",
              }) }}
            </span>
          </template>
          <n-progress
            v-else
            type="line"
            :height="8"
            :percentage="0"
            processing
            :show-indicator="false"
          />
        </div>
      </div>
      <p class="usage-hint">
        {{ t("手动值保存后会继续累加本机用量；100% 仅为提示，收到真实 429 后才会熔断。") }}
      </p>

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
            v-if="accountIsCooling(account)"
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
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  NButton,
  NCard,
  NEmpty,
  NForm,
  NFormItem,
  NH3,
  NIcon,
  NInput,
  NInputNumber,
  NModal,
  NPopconfirm,
  NProgress,
  NSlider,
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
import {
  isCooling,
  isUsageLimitReached,
  mergeUsageEdit,
  normalizeUsagePercent,
  usagePercentFromCost,
} from "./accounts-usage";
import type { UsageEditState, UsageKey } from "./accounts-usage";
import { locale, t } from "../i18n/index.ts";

type AccountDraft = {
  name: string;
  username: string;
  password: string;
  key: string;
  clearPassword: boolean;
};

type AccountUsageEdits = Record<UsageKey, UsageEditState>;

function setUsageSliderLabel(el: HTMLElement, label: string) {
  el.querySelector<HTMLElement>("[role='slider']")?.setAttribute("aria-label", label);
}

const vUsageSliderLabel = {
  mounted: (el: HTMLElement, { value }: { value: string }) => setUsageSliderLabel(el, value),
  updated: (el: HTMLElement, { value }: { value: string }) => setUsageSliderLabel(el, value),
};

const usageLimits = computed<Array<{ key: UsageKey; label: string; limit: number }>>(() => [
  { key: "window_5h", label: "5h", limit: 12 },
  { key: "window_week", label: t("本周"), limit: 30 },
  { key: "window_month", label: t("本月"), limit: 60 },
]);

const message = useMessage();
const accounts = ref<Account[]>([]);
const usageMap = ref<Record<string, UsageWindow>>({});
const usageEdits = ref<Record<string, AccountUsageEdits>>({});
const usageLoading = ref<Record<string, boolean>>({});
const usageLoadErrors = ref<Record<string, string | null>>({});
const drafts = ref<Record<string, AccountDraft>>({});
const expanded = ref<Record<string, boolean>>({});
const pinging = ref<Record<string, boolean>>({});
const showCreateModal = ref(false);
const busy = ref(false);
const now = ref(Date.now());
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

function usageLimit(key: UsageKey): number {
  return usageLimits.value.find((limit) => limit.key === key)?.limit ?? 0;
}

function usageEditsFromWindow(usage: UsageWindow): AccountUsageEdits {
  return Object.fromEntries(usageLimits.value.map(({ key, limit }) => {
    const percent = usagePercentFromCost(usage[key], limit);
    return [key, { draft: percent, saved: percent, saving: false, error: null }];
  })) as AccountUsageEdits;
}

function syncUsageEdits(accountId: string, usage: UsageWindow) {
  const existing = usageEdits.value[accountId];
  if (!existing) {
    usageEdits.value[accountId] = usageEditsFromWindow(usage);
    return;
  }
  const account = accounts.value.find(({ id }) => id === accountId);
  for (const { key, limit } of usageLimits.value) {
    const saved = usagePercentFromCost(usage[key], limit);
    const edit = existing[key];
    const wasActuallyReset = account && isUsageLimitReached(account, key, now.value);
    Object.assign(edit, mergeUsageEdit(edit, saved, Boolean(wasActuallyReset)));
  }
}

function updateUsageDraft(accountId: string, key: UsageKey, value: number | null) {
  const edit = usageEdits.value[accountId]?.[key];
  if (!edit || edit.saving || value === null) return;
  edit.draft = normalizeUsagePercent(value);
}

async function saveUsage(accountId: string, key: UsageKey) {
  const edit = usageEdits.value[accountId]?.[key];
  if (!edit || edit.saving) return;
  const percent = normalizeUsagePercent(edit.draft);
  edit.draft = percent;
  if (percent === edit.saved && !edit.error) return;
  edit.saving = true;
  edit.error = null;
  try {
    const usage = await tauriApi.updateAccountUsage(accountId, key, percent);
    usageMap.value[accountId] = {
      ...getUsage(accountId),
      [key]: usage[key],
    };
    const saved = usagePercentFromCost(usage[key], usageLimit(key));
    edit.draft = saved;
    edit.saved = saved;
  } catch (error) {
    edit.error = String(error);
  } finally {
    edit.saving = false;
  }
}

function accountIsCooling(account: Account): boolean {
  return isCooling(account, now.value);
}

function accountUsageLimitReached(account: Account, key: UsageKey): boolean {
  return isUsageLimitReached(account, key, now.value);
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
  const ms = new Date(account.cooldown_until).getTime() - now.value;
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
    await Promise.all(loaded.map((account) => loadAccountUsage(account.id)));
  } catch (e) {
    message.error(t("加载账号失败: {error}", { error: String(e) }));
  }
}

async function loadAccountUsage(accountId: string) {
  usageLoading.value[accountId] = true;
  usageLoadErrors.value[accountId] = null;
  try {
    const usage = await tauriApi.getAccountUsage(accountId);
    usageMap.value[accountId] = usage;
    syncUsageEdits(accountId, usage);
  } catch (error) {
    usageLoadErrors.value[accountId] = String(error);
  } finally {
    usageLoading.value[accountId] = false;
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

let clock: number | undefined;
onMounted(() => {
  clock = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
  void loadAccounts();
});
onUnmounted(() => window.clearInterval(clock));
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
  gap: 6px;
  min-width: 0;
}

.usage-editor {
  display: grid;
  grid-template-columns: 78px minmax(0, 1fr);
  align-items: center;
  gap: 10px;
}

.usage-editor :deep(.n-input-number) {
  width: 78px;
}

.usage-load-error {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 42px;
  color: var(--n-error-color);
}

.usage-save-error {
  color: var(--n-error-color);
  font-size: 11px;
}

.usage-hint {
  margin: 8px 0 0;
  color: var(--n-text-color-3);
  font-size: 11px;
}

.usage-meta {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 16px;
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
