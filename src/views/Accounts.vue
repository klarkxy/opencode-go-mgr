<template>
  <div class="accounts-view">
    <n-space vertical :size="16" class="accounts-content">
      <n-space justify="space-between" align="center" class="accounts-toolbar">
        <n-h3 style="margin: 0">{{ t("账号") }}</n-h3>
        <n-button type="primary" @click="openCreateModal">
          <template #icon>
            <n-icon :component="PlusOutlined" />
          </template>
          {{ t("新增账号") }}
        </n-button>
      </n-space>

      <span id="account-order-instructions" class="sr-only">
        {{ t("使用上下方向键调整优先级") }}
      </span>

      <div
        v-if="accountListLoading"
        class="account-list-state"
        role="status"
        aria-live="polite"
        :aria-label="t('加载中…')"
      >
        <n-spin size="small" />
      </div>

      <n-alert v-else-if="accountListError" type="error" :title="t('加载账号失败: {error}', { error: accountListError })">
        <n-button size="small" secondary @click="loadAccounts">{{ t("重试") }}</n-button>
      </n-alert>

      <n-alert v-if="quotaLimitsError" type="warning" :title="t('用量加载失败')">
        <n-button
          size="small"
          secondary
          :loading="quotaLimitsLoading"
          @click="retryQuotaLimits"
        >{{ t("重试") }}</n-button>
      </n-alert>

      <n-empty v-if="!accountListLoading && !accountListError && accounts.length === 0" :description="t('暂无账号')">
        <template #extra>
          <n-button type="primary" @click="openCreateModal">
            <template #icon>
              <n-icon :component="PlusOutlined" />
            </template>
            {{ t("新增账号") }}
          </n-button>
        </template>
      </n-empty>

      <div v-if="!accountListLoading && !accountListError && accounts.length > 0" class="account-list">
        <n-card
          v-for="account in accounts"
          :key="account.id"
          :data-account-id="account.id"
          size="small"
          class="account-card"
          :class="{
            'account-card--cooling': accountIsCooling(account),
            'account-card--dragging': draggingAccountId === account.id,
          }"
        >
          <template #header>
            <div class="account-title">
              <n-tooltip trigger="hover">
                <template #trigger>
                  <n-button
                    circle
                    quaternary
                    size="small"
                    class="account-order-handle"
                    :class="{ 'account-order-handle--dragging': draggingAccountId === account.id }"
                    :disabled="orderSaving || busy || accounts.length < 2"
                    :aria-label="t('拖动调整账号 {name} 的优先级', { name: account.name })"
                    aria-describedby="account-order-instructions"
                    @click.prevent
                    @keydown="handleOrderKeydown($event, account.id)"
                    @pointerdown="startAccountDrag($event, account.id)"
                  >
                    <template #icon><n-icon :component="HolderOutlined" /></template>
                  </n-button>
                </template>
                {{ t("拖动调整账号 {name} 的优先级", { name: account.name }) }}
              </n-tooltip>
              <div class="account-heading">
                <div class="account-name-row">
                  <span class="account-name">{{ account.name }}</span>
                  <n-tooltip v-if="accountIsCooling(account)">
                    <template #trigger>
                      <n-tag :type="accountStatusTagType(account)" size="small">
                        {{ accountStatusLabel(account) }}
                      </n-tag>
                    </template>
                    {{ cooldownDetails(account) }}
                  </n-tooltip>
                  <n-tag v-else :type="accountStatusTagType(account)" size="small">
                    {{ accountStatusLabel(account) }}
                  </n-tag>
                  <n-tag size="small" :bordered="false">
                    {{ t("购买于 {date}", { date: account.purchase_date }) }}
                  </n-tag>
                  <n-tag size="small" :bordered="false">
                    {{ t("到期于 {date}", { date: account.expires_on }) }}
                  </n-tag>
                  <n-tag :type="accountExpiryTagType(account)" size="small" :bordered="false">
                    {{ accountExpiryLabel(account) }}
                  </n-tag>
                </div>
              </div>
            </div>
          </template>

          <template #header-extra>
            <n-space align="center" :size="8">
              <n-tooltip v-if="usageEdits[account.id]" trigger="hover">
                <template #trigger>
                  <n-button
                    circle
                    quaternary
                    size="small"
                    :aria-label="t('校准用量')"
                    @click="focusUsageEditor(account.id)"
                  >
                    <template #icon><n-icon :component="EditOutlined" /></template>
                  </n-button>
                </template>
                {{ t("校准用量") }}
              </n-tooltip>

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

              <n-tooltip trigger="hover">
                <template #trigger>
                  <n-switch
                    :value="account.enabled"
                    :aria-label="account.enabled ? t('禁用账号 {name}', { name: account.name }) : t('启用账号 {name}', { name: account.name })"
                    @update:value="toggleAccount(account.id)"
                  />
                </template>
                {{ account.enabled ? t("禁用账号 {name}", { name: account.name }) : t("启用账号 {name}", { name: account.name }) }}
              </n-tooltip>

              <n-dropdown
                :options="accountMenuOptions(account)"
                trigger="click"
                placement="bottom-end"
                @select="(key: string | number) => handleMenuSelect(key, account.id)"
              >
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button
                      circle
                      quaternary
                      size="small"
                      :aria-label="t('更多操作')"
                    >
                      <template #icon><n-icon :component="MoreOutlined" /></template>
                    </n-button>
                  </template>
                  {{ t("更多操作") }}
                </n-tooltip>
              </n-dropdown>
            </n-space>
          </template>

          <div v-if="!quotaLimitsError">
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
              <div class="usage-strip-body" role="group" :aria-label="t('用量')">
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
                :percentage="usageProgressPercentage(account, limit.key, 100)"
                :status="usageProgressStatus(account, limit.key, 100)"
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
                <div class="usage-resets-row">
                  <template v-if="WINDOW_FULL_MINUTES[limit.key] !== null">
                    <span class="usage-resets-hint">{{ t("距上游重置还剩") }}</span>
                    <n-input-number
                      :value="resetsFirstField(account.id, limit.key)"
                      :min="0"
                      :max="resetsFirstMax(limit.key)"
                      :step="1"
                      size="tiny"
                      :show-button="false"
                      :disabled="usageLoading[account.id] || usageEdits[account.id][limit.key].saving"
                      :input-props="{
                        'aria-label': t('{name} {period} 距上游重置还剩{unit}', {
                          name: account.name,
                          period: limit.label,
                          unit: resetsFirstLabel(limit.key),
                        }),
                      }"
                      @update:value="updateResetsFirstField(account.id, limit.key, $event)"
                      @blur="saveUsage(account.id, limit.key)"
                      @keydown.enter.prevent="saveUsage(account.id, limit.key)"
                    >
                      <template #suffix>{{ resetsFirstLabel(limit.key) }}</template>
                    </n-input-number>
                    <n-input-number
                      :value="resetsSecondField(account.id, limit.key)"
                      :min="0"
                      :max="resetsSecondMax(limit.key)"
                      :step="1"
                      size="tiny"
                      :show-button="false"
                      :disabled="usageLoading[account.id] || usageEdits[account.id][limit.key].saving"
                      :input-props="{
                        'aria-label': t('{name} {period} 距上游重置还剩{unit}', {
                          name: account.name,
                          period: limit.label,
                          unit: resetsSecondLabel(limit.key),
                        }),
                      }"
                      @update:value="updateResetsSecondField(account.id, limit.key, $event)"
                      @blur="saveUsage(account.id, limit.key)"
                      @keydown.enter.prevent="saveUsage(account.id, limit.key)"
                    >
                      <template #suffix>{{ resetsSecondLabel(limit.key) }}</template>
                    </n-input-number>
                  </template>
                  <span v-else class="usage-resets-hint">{{ t("到期于 {date}", { date: account.expires_on }) }}</span>
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
                :percentage="usageProgressPercentage(account, limit.key, 0)"
                :status="usageProgressStatus(account, limit.key, 0)"
                processing
                :show-indicator="false"
              />
              <span class="usage-reset-countdown"></span>
            </div>
            </div>
            </div>
          </div>
        </n-card>
      </div>

      <span class="sr-only" aria-live="polite" aria-atomic="true">{{ orderAnnouncement }}</span>
    </n-space>

    <AccountFormModal
      v-model:show="showModal"
      :account="editingAccount"
      :is-cooling="editingAccount ? accountIsCooling(editingAccount) : false"
      :busy="busy"
      @save="onFormSave"
      @reset-cooldown="resetCooldown(editingAccount!.id)"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  NAlert,
  NButton,
  NCard,
  NDropdown,
  NEmpty,
  NH3,
  NIcon,
  NInputNumber,
  NProgress,
  NSpin,
  NSlider,
  NSpace,
  NSwitch,
  NTag,
  NTooltip,
  useDialog,
  useMessage,
} from "naive-ui";
import {
  EditOutlined,
  HolderOutlined,
  MoreOutlined,
  PlusOutlined,
  ThunderboltOutlined,
} from "@vicons/antd";
import { DashboardRequestError, tauriApi } from "../api/tauri";
import type { Account, AccountInput, AccountUpdate, PricingLimits, UsageWindow } from "../api/tauri";
import {
  defaultResetsInMinutes,
  isCooling,
  isUsageLimitReached,
  isWindowCooling,
  mergeUsageEdit,
  normalizeUsagePercent,
  usagePercentFromCost,
  usageProgressPercentage,
  usageProgressStatus,
  WINDOW_FULL_MINUTES,
} from "./accounts-usage";
import type { UsageEditState, UsageKey } from "./accounts-usage";
import { daysUntilDate, expiryTagType, moveItem } from "./account-lifecycle";
import { t } from "../i18n/index.ts";
import { formatCost } from "../utils/format.ts";
import { userFacingError } from "../utils/errors.ts";
import { mapWithConcurrency } from "../utils/async.ts";
import AccountFormModal from "../components/AccountFormModal.vue";

type AccountMenuOption = {
  key: string | number;
  label?: string;
  accountId: string;
  accountName: string;
};

type AccountUsageEdits = Record<UsageKey, UsageEditState>;

type AccountDragState = {
  accountId: string;
  handle: HTMLElement;
  moved: boolean;
  pointerId: number;
  previous: Account[];
};

function setUsageSliderLabel(el: HTMLElement, label: string) {
  el.querySelector<HTMLElement>("[role='slider']")?.setAttribute("aria-label", label);
}

const vUsageSliderLabel = {
  mounted: (el: HTMLElement, { value }: { value: string }) => setUsageSliderLabel(el, value),
  updated: (el: HTMLElement, { value }: { value: string }) => setUsageSliderLabel(el, value),
};

const quotaLimits = ref<PricingLimits | null>(null);
const quotaLimitsLoading = ref(false);
const quotaLimitsError = ref("");
const usageLimits = computed<Array<{ key: UsageKey; label: string; limit: number }>>(() => {
  const limits = quotaLimits.value;
  if (!limits) return [];
  return [
    { key: "window_5h", label: t("5小时"), limit: limits.window_5h },
    { key: "window_week", label: t("本周"), limit: limits.window_week },
    { key: "window_month", label: t("本月"), limit: limits.window_month },
  ];
});

const dialog = useDialog();
const message = useMessage();
const accounts = ref<Account[]>([]);
const accountListLoading = ref(true);
const accountListError = ref("");
const usageMap = ref<Record<string, UsageWindow>>({});
const usageEdits = ref<Record<string, AccountUsageEdits>>({});
const usageLoading = ref<Record<string, boolean>>({});
const usageLoadErrors = ref<Record<string, string | null>>({});
const pinging = ref<Record<string, boolean>>({});
const showModal = ref(false);
const editingAccount = ref<Account | null>(null);
const busy = ref(false);
const orderSaving = ref(false);
const draggingAccountId = ref<string | null>(null);
const orderAnnouncement = ref("");
const now = ref(Date.now());
let accountDrag: AccountDragState | null = null;

function errorDetail(error: unknown): string {
  return userFacingError(error, t("无法连接到本地服务，请确认程序正在运行后重试"));
}

function blankUsage(accountId: string): UsageWindow {
  return {
    account_id: accountId,
    window_5h: 0,
    window_week: 0,
    window_month: 0,
    resets_in_5h: null,
    resets_in_week: null,
    resets_in_month: null,
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

function accountUsageLimitReached(account: Account, key: UsageKey): boolean {
  return isUsageLimitReached(account, key, now.value);
}

function usageEditsFromWindow(usage: UsageWindow): AccountUsageEdits {
  return Object.fromEntries(usageLimits.value.map(({ key, limit }) => {
    const percent = usagePercentFromCost(usage[key], limit);
    const resetsInMin = defaultResetsInMinutes(usage, key, now.value);
    return [key, {
      draft: percent,
      saved: percent,
      saving: false,
      error: null,
      resets_in_minutes_draft: resetsInMin,
      resets_in_minutes_saved: resetsInMin,
    }];
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
    if (!edit) {
      existing[key] = mergeUsageEdit(undefined, saved, Boolean(wasActuallyReset));
      continue;
    }
    Object.assign(edit, mergeUsageEdit(edit, saved, Boolean(wasActuallyReset)));
    // 同步 resets_in_minutes_saved；draft 未被用户改动时跟着 saved 走。
    const resetsInMin = defaultResetsInMinutes(usage, key, now.value);
    edit.resets_in_minutes_saved = resetsInMin;
    if (edit.resets_in_minutes_draft === null || !edit.saving && edit.resets_in_minutes_draft === edit.resets_in_minutes_saved) {
      edit.resets_in_minutes_draft = resetsInMin;
    }
  }
}

function updateUsageDraft(accountId: string, key: UsageKey, value: number | null) {
  const edit = usageEdits.value[accountId]?.[key];
  if (!edit || edit.saving || value === null) return;
  edit.draft = normalizeUsagePercent(value);
}

// 校准按钮入口：滚动到该账号卡片第一个用量输入框并聚焦。
// HEAD 的 inline editor 已经在卡片 body 中，这里只是 anchor 跳转。
function focusUsageEditor(accountId: string) {
  const card = document.querySelector<HTMLElement>(
    `.account-card[data-account-id="${accountId}"] .usage-editor .n-input-number input`,
  );
  if (card) {
    card.scrollIntoView({ behavior: "smooth", block: "center" });
    card.focus();
  }
}

// ponytail: 用户要求直接编辑"天+小时"或"小时+分钟"，而不是分钟总数。
// 5h 窗口（<1天）显示 [小时][分钟]；周窗口（≥1天）显示 [天][小时]。
function resetsFirstLabel(key: UsageKey): string {
  return key === "window_5h" ? t("小时") : t("天");
}

function resetsSecondLabel(key: UsageKey): string {
  return key === "window_5h" ? t("分钟") : t("小时");
}

function resetsFirstMax(key: UsageKey): number {
  if (key === "window_5h") return 5;
  if (key === "window_week") return 7;
  return 0;
}

function resetsSecondMax(key: UsageKey): number {
  if (key === "window_5h") return 59;
  if (key === "window_week") return 23;
  return 0;
}

function resetsFirstField(accountId: string, key: UsageKey): number {
  const edit = usageEdits.value[accountId]?.[key];
  if (!edit || edit.resets_in_minutes_draft === null) return 0;
  const m = edit.resets_in_minutes_draft;
  if (key === "window_5h") return Math.floor(m / 60);
  if (key === "window_week") return Math.floor(m / (24 * 60));
  return 0;
}

function resetsSecondField(accountId: string, key: UsageKey): number {
  const edit = usageEdits.value[accountId]?.[key];
  if (!edit || edit.resets_in_minutes_draft === null) return 0;
  const m = edit.resets_in_minutes_draft;
  if (key === "window_5h") return m % 60;
  if (key === "window_week") return Math.floor((m % (24 * 60)) / 60);
  return 0;
}

function fieldsToMinutes(first: number, second: number, key: UsageKey): number {
  if (key === "window_5h") return first * 60 + second;
  if (key === "window_week") return first * 24 * 60 + second * 60;
  return 0;
}

function updateResetsFirstField(accountId: string, key: UsageKey, value: number | null) {
  const edit = usageEdits.value[accountId]?.[key];
  if (!edit || edit.saving) return;
  if (WINDOW_FULL_MINUTES[key] === null) return;
  const v = value === null ? 0 : Math.max(0, Math.round(value));
  const second = resetsSecondField(accountId, key);
  const max = WINDOW_FULL_MINUTES[key] ?? 10080;
  edit.resets_in_minutes_draft = Math.min(max, fieldsToMinutes(v, second, key));
}

function updateResetsSecondField(accountId: string, key: UsageKey, value: number | null) {
  const edit = usageEdits.value[accountId]?.[key];
  if (!edit || edit.saving) return;
  if (WINDOW_FULL_MINUTES[key] === null) return;
  const v = value === null ? 0 : Math.max(0, Math.round(value));
  const first = resetsFirstField(accountId, key);
  const max = WINDOW_FULL_MINUTES[key] ?? 10080;
  edit.resets_in_minutes_draft = Math.min(max, fieldsToMinutes(first, v, key));
}

async function saveUsage(accountId: string, key: UsageKey) {
  const edit = usageEdits.value[accountId]?.[key];
  if (!edit || edit.saving) return;
  const percent = normalizeUsagePercent(edit.draft);
  edit.draft = percent;
  const resetsChanged = edit.resets_in_minutes_draft !== edit.resets_in_minutes_saved;
  if (percent === edit.saved && !resetsChanged && !edit.error) return;
  edit.saving = true;
  edit.error = null;
  // ponytail: Bug 3 — 之前用 defaultResetsInMinutes(usage, key, now.value) 重算剩余分钟，
  // 由于 now.value（每秒刷新的 Vue ref）始终 ≤ backend 处理请求时的 Utc::now()，
  // Math.ceil((ends_at - now)/60000) 会向上取整多出 1 分钟，每次保存累加。
  // 直接回写用户输入的值，这就是发给后端的值，无需重算。
  const userResetsInMin = edit.resets_in_minutes_draft;
  try {
    const usage = await tauriApi.updateAccountUsage(
      accountId,
      key,
      percent,
      WINDOW_FULL_MINUTES[key] === null ? null : userResetsInMin,
    );
    usageMap.value[accountId] = {
      ...getUsage(accountId),
      [key]: usage[key],
      ...(key === "window_5h" ? { resets_in_5h: usage.resets_in_5h } : {}),
      ...(key === "window_week" ? { resets_in_week: usage.resets_in_week } : {}),
      ...(key === "window_month" ? { resets_in_month: usage.resets_in_month } : {}),
    };
    const saved = usagePercentFromCost(usage[key], usageLimit(key));
    edit.draft = saved;
    edit.saved = saved;
    edit.resets_in_minutes_draft = userResetsInMin;
    edit.resets_in_minutes_saved = userResetsInMin;
  } catch (error) {
    edit.error = errorDetail(error);
  } finally {
    edit.saving = false;
  }
}

function accountIsCooling(account: Account): boolean {
  return isCooling(account, now.value);
}

function formatRemaining(account: Account): string {
  if (!account.cooldown_until) return "";
  const ms = new Date(account.cooldown_until).getTime() - now.value;
  if (ms <= 0) return t("{seconds}秒", { seconds: 0 });
  const seconds = Math.ceil(ms / 1000);
  if (seconds < 60) return t("{seconds}秒", { seconds });
  const min = Math.floor(ms / 60000);
  if (min < 60) return t("{minutes}分钟", { minutes: min });
  const hr = Math.floor(min / 60);
  if (hr < 24) return t("{hours}小时{minutes}分钟", { hours: hr, minutes: min % 60 });
  const day = Math.floor(hr / 24);
  return t("{days}天{hours}小时", { days: day, hours: hr % 24 });
}

function cooldownDetails(account: Account): string {
  const active = usageLimits.value
    .filter((limit) => isWindowCooling(account, limit.key, now.value))
    .map((limit) => limit.label);
  if (
    account.cooldown_generic_until
    && Date.parse(account.cooldown_generic_until) > now.value
  ) {
    active.unshift(t("冷却中"));
  }
  return active.length > 0 ? active.join(" · ") : t("冷却中");
}

function accountExpiryDays(account: Account): number {
  return daysUntilDate(account.expires_on, now.value);
}

function accountExpiryTagType(account: Account) {
  return expiryTagType(accountExpiryDays(account));
}

function accountExpiryLabel(account: Account): string {
  const days = accountExpiryDays(account);
  if (days === 1) return t("剩 1 天");
  if (days > 0) return t("剩 {days} 天", { days });
  if (days === 0) return t("今天到期");
  if (days === -1) return t("已到期 1 天");
  return t("已到期 {days} 天", { days: Number.isFinite(days) ? Math.abs(days) : 0 });
}

function accountStatusLabel(account: Account): string {
  if (!account.enabled) return t("已禁用");
  if (accountIsCooling(account)) return t("冷却中·剩 {time}", { time: formatRemaining(account) });
  return t("可用");
}

function accountStatusTagType(account: Account): "success" | "warning" | "default" {
  if (!account.enabled) return "default";
  if (accountIsCooling(account)) return "warning";
  return "success";
}

function accountMenuOptions(account: Account): AccountMenuOption[] {
  const options: AccountMenuOption[] = [
    { key: "edit", label: t("编辑账号"), accountId: account.id, accountName: account.name },
  ];
  if (accountIsCooling(account)) {
    options.push({
      key: "reset",
      label: t("重置冷却"),
      accountId: account.id,
      accountName: account.name,
    });
  }
  options.push({
    key: "delete",
    label: t("删除账号"),
    accountId: account.id,
    accountName: account.name,
  });
  return options;
}

function handleMenuSelect(key: string | number, accountId: string) {
  if (key === "edit") {
    openEditModal(accountId);
  } else if (key === "reset") {
    resetCooldown(accountId);
  } else if (key === "delete") {
    const account = accounts.value.find((item) => item.id === accountId);
    if (!account) return;
    dialog.warning({
      title: t("删除账号"),
      content: t("确定删除账号 {name} 吗？", { name: account.name }),
      positiveText: t("删除"),
      negativeText: t("取消"),
      onPositiveClick: () => deleteAccount(accountId),
    });
  }
}

function openCreateModal(): void {
  editingAccount.value = null;
  showModal.value = true;
}

function openEditModal(id: string): void {
  editingAccount.value = accounts.value.find((account) => account.id === id) ?? null;
  showModal.value = true;
}

function sameAccountOrder(left: readonly Account[], right: readonly Account[]): boolean {
  return left.length === right.length && left.every((account, index) => account.id === right[index]?.id);
}

function clearAccountDrag(state: AccountDragState): void {
  window.removeEventListener("pointermove", previewAccountDrag);
  window.removeEventListener("pointerup", finishAccountDrag);
  window.removeEventListener("pointercancel", cancelAccountDrag);
  accountDrag = null;
  draggingAccountId.value = null;
  if (state.handle.hasPointerCapture(state.pointerId)) {
    state.handle.releasePointerCapture(state.pointerId);
  }
}

async function persistAccountOrder(previous: Account[], movedAccountId: string): Promise<void> {
  if (sameAccountOrder(previous, accounts.value)) return;
  orderSaving.value = true;
  try {
    const saved = await tauriApi.reorderAccounts(accounts.value.map(({ id }) => id));
    applyLoadedAccounts(saved);
    const moved = accounts.value.find(({ id }) => id === movedAccountId);
    const position = accounts.value.findIndex(({ id }) => id === movedAccountId) + 1;
    if (moved && position > 0) {
      orderAnnouncement.value = t("账号 {name} 已移至第 {position} 位", {
        name: moved.name,
        position,
      });
    }
    message.success(t("账号顺序已更新"));
  } catch (error) {
    if (error instanceof DashboardRequestError && error.status === 409) {
      try {
        const knownIds = new Set(accounts.value.map(({ id }) => id));
        const loaded = await tauriApi.getAccounts();
        const loadedIds = new Set(loaded.map(({ id }) => id));
        for (const id of knownIds) {
          if (!loadedIds.has(id)) removeAccountState(id);
        }
        applyLoadedAccounts(loaded);
        await mapWithConcurrency(
          loaded.filter(({ id }) => !knownIds.has(id)),
          4,
          ({ id }) => loadAccountUsage(id),
        );
      } catch {
        accounts.value = previous;
      }
    } else {
      accounts.value = previous;
    }
    const failure = t("保存账号顺序失败: {error}", { error: errorDetail(error) });
    orderAnnouncement.value = failure;
    message.error(failure);
  } finally {
    orderSaving.value = false;
  }
}

function startAccountDrag(event: PointerEvent, accountId: string): void {
  if (
    orderSaving.value
    || busy.value
    || accounts.value.length < 2
    || accountDrag !== null
    || !event.isPrimary
    || (event.pointerType === "mouse" && event.button !== 0)
  ) return;
  const handle = event.currentTarget as HTMLElement;
  event.preventDefault();
  handle.setPointerCapture(event.pointerId);
  accountDrag = {
    accountId,
    handle,
    moved: false,
    pointerId: event.pointerId,
    previous: [...accounts.value],
  };
  draggingAccountId.value = accountId;
  window.addEventListener("pointermove", previewAccountDrag, { passive: false });
  window.addEventListener("pointerup", finishAccountDrag);
  window.addEventListener("pointercancel", cancelAccountDrag);
}

function previewAccountDrag(event: PointerEvent): void {
  const state = accountDrag;
  if (!state || state.pointerId !== event.pointerId) return;
  event.preventDefault();
  const target = document
    .elementFromPoint(event.clientX, event.clientY)
    ?.closest<HTMLElement>(".account-card[data-account-id]");
  const targetId = target?.dataset.accountId;
  if (!targetId || targetId === state.accountId) return;
  const fromIndex = accounts.value.findIndex(({ id }) => id === state.accountId);
  const toIndex = accounts.value.findIndex(({ id }) => id === targetId);
  if (fromIndex < 0 || toIndex < 0 || fromIndex === toIndex) return;
  accounts.value = moveItem(accounts.value, fromIndex, toIndex);
  state.moved = true;
}

async function finishAccountDrag(event: PointerEvent): Promise<void> {
  const state = accountDrag;
  if (!state || state.pointerId !== event.pointerId) return;
  event.preventDefault();
  clearAccountDrag(state);
  if (!state.moved || sameAccountOrder(state.previous, accounts.value)) return;
  await persistAccountOrder(state.previous, state.accountId);
}

function cancelAccountDrag(event: PointerEvent): void {
  const state = accountDrag;
  if (!state || state.pointerId !== event.pointerId) return;
  event.preventDefault();
  accounts.value = state.previous;
  clearAccountDrag(state);
}

async function handleOrderKeydown(event: KeyboardEvent, accountId: string): Promise<void> {
  if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
  event.preventDefault();
  if (orderSaving.value || busy.value || accounts.value.length < 2) return;
  const fromIndex = accounts.value.findIndex(({ id }) => id === accountId);
  const toIndex = fromIndex + (event.key === "ArrowUp" ? -1 : 1);
  if (fromIndex < 0 || toIndex < 0 || toIndex >= accounts.value.length) return;
  const previous = [...accounts.value];
  accounts.value = moveItem(accounts.value, fromIndex, toIndex);
  await persistAccountOrder(previous, accountId);
}

function applyLoadedAccounts(loaded: Account[]): void {
  accounts.value = loaded;
}

function replaceAccount(account: Account): void {
  accounts.value = accounts.value.map((item) => (item.id === account.id ? account : item));
  if (editingAccount.value?.id === account.id) editingAccount.value = account;
}

function addAccount(account: Account): void {
  accounts.value = [...accounts.value, account];
}

function removeAccountState(id: string): void {
  accounts.value = accounts.value.filter((item) => item.id !== id);
  delete usageMap.value[id];
  delete usageEdits.value[id];
  delete usageLoading.value[id];
  delete usageLoadErrors.value[id];
  delete pinging.value[id];
}

async function refreshAccountState(id: string): Promise<void> {
  const loaded = await tauriApi.getAccounts();
  applyLoadedAccounts(loaded);
  if (!loaded.some((account) => account.id === id)) {
    removeAccountState(id);
    message.warning(t("未找到该账号，已为你刷新列表"));
    return;
  }
  await loadAccountUsage(id);
}

async function loadAccounts() {
  accountListLoading.value = true;
  accountListError.value = "";
  try {
    const loaded = await tauriApi.getAccounts();
    accounts.value = loaded;
    // 限流并发拉取用量，避免账号多时 N 次请求同时打到后端
    if (quotaLimits.value) {
      await mapWithConcurrency(loaded, 4, (account) => loadAccountUsage(account.id));
    }
  } catch (e) {
    accountListError.value = errorDetail(e);
    message.error(t("加载账号失败: {error}", { error: accountListError.value }));
  } finally {
    accountListLoading.value = false;
  }
}

async function loadQuotaLimits(): Promise<boolean> {
  quotaLimitsLoading.value = true;
  quotaLimitsError.value = "";
  try {
    quotaLimits.value = (await tauriApi.getPricing()).limits;
    return true;
  } catch (error) {
    quotaLimits.value = null;
    quotaLimitsError.value = errorDetail(error);
    return false;
  } finally {
    quotaLimitsLoading.value = false;
  }
}

async function initializeAccounts() {
  await loadQuotaLimits();
  await loadAccounts();
}

async function retryQuotaLimits() {
  if (!await loadQuotaLimits()) return;
  await mapWithConcurrency(accounts.value, 4, (account) => loadAccountUsage(account.id));
}

async function loadAccountUsage(accountId: string) {
  usageLoading.value[accountId] = true;
  usageLoadErrors.value[accountId] = null;
  try {
    const usage = await tauriApi.getAccountUsage(accountId);
    usageMap.value[accountId] = usage;
    syncUsageEdits(accountId, usage);
  } catch (error) {
    usageLoadErrors.value[accountId] = errorDetail(error);
  } finally {
    usageLoading.value[accountId] = false;
  }
}

async function onFormSave(payload: { name: string; username: string; key?: string; purchase_date?: string }) {
  if (editingAccount.value) {
    const update: AccountUpdate = {
      name: payload.name,
      username: payload.username,
      purchase_date: payload.purchase_date,
    };
    if (payload.key !== undefined) update.key = payload.key;
    busy.value = true;
    try {
      const saved = await tauriApi.updateAccount(editingAccount.value.id, update);
      replaceAccount(saved);
      message.success(t("账号已更新"));
      showModal.value = false;
    } catch (e) {
      message.error(t("保存失败: {error}", { error: errorDetail(e) }));
    } finally {
      busy.value = false;
    }
  } else {
    const input: AccountInput = {
      name: payload.name,
      username: payload.username,
      key: payload.key || "",
      purchase_date: payload.purchase_date,
    };
    busy.value = true;
    try {
      const created = await tauriApi.createAccount(input);
      message.success(t("账号已添加"));
      addAccount(created);
      await loadAccountUsage(created.id);
      showModal.value = false;
    } catch (e) {
      message.error(t("保存失败: {error}", { error: errorDetail(e) }));
    } finally {
      busy.value = false;
    }
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
      : t("Ping 失败: {error}", { error: errorDetail(e) }));
  } finally {
    pinging.value[id] = false;
    try {
      await refreshAccountState(id);
    } catch (e) {
      message.error(t("加载账号失败: {error}", { error: errorDetail(e) }));
    }
  }
}

async function toggleAccount(id: string) {
  try {
    const updated = await tauriApi.toggleAccount(id);
    replaceAccount(updated);
  } catch (e) {
    message.error(t("切换失败: {error}", { error: errorDetail(e) }));
  }
}

async function deleteAccount(id: string) {
  try {
    await tauriApi.deleteAccount(id);
    message.success(t("账号已删除"));
    removeAccountState(id);
  } catch (e) {
    message.error(t("删除失败: {error}", { error: errorDetail(e) }));
  }
}

async function resetCooldown(id: string) {
  try {
    const updated = await tauriApi.resetAccountCooldown(id);
    replaceAccount(updated);
    message.success(t("已重置冷却"));
  } catch (e) {
    message.error(t("重置失败: {error}", { error: errorDetail(e) }));
  }
}

let clock: number | undefined;
onMounted(() => {
  clock = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
  void initializeAccounts();
});
onUnmounted(() => {
  window.clearInterval(clock);
  if (accountDrag) {
    accounts.value = accountDrag.previous;
    clearAccountDrag(accountDrag);
  }
});
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

.account-list {
  display: grid;
  gap: 12px;
}
.account-list-state {
  min-height: 160px;
  display: grid;
  place-items: center;
}

.account-card {
  border-radius: 14px;
  box-shadow: var(--ocg-shadow-sm);
  transition: border-color 0.16s ease, box-shadow 0.16s ease, opacity 0.16s ease;
}

.account-card--cooling {
  border-color: rgba(208, 48, 80, 0.45);
}

.account-card--dragging {
  border-color: var(--ocg-primary);
  box-shadow: 0 10px 28px color-mix(in srgb, var(--ocg-primary) 18%, transparent);
  opacity: 0.72;
}

.account-title {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  width: 100%;
}

.account-order-handle {
  flex: 0 0 auto;
  cursor: grab;
  touch-action: none;
  user-select: none;
}

.account-order-handle--dragging {
  cursor: grabbing;
}

.account-heading {
  display: flex;
  align-items: center;
  flex: 1 1 auto;
  min-width: 0;
}

.account-name-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 6px;
  min-width: 0;
}

.account-name {
  overflow: hidden;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-md);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.account-name-row :deep(.n-tag) {
  flex: 0 0 auto;
}

.usage-load-error {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 42px;
  color: var(--ocg-error);
  font-size: var(--ocg-font-sm);
}

.usage-strip {
  min-width: 0;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
}

.usage-segment {
  display: grid;
  gap: 6px;
  min-width: 0;
}

.usage-meta {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: var(--ocg-font-sm);
  color: var(--ocg-muted);
}

.usage-meta strong {
  color: var(--ocg-ink);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-weight: 600;
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

.usage-reset-countdown {
  min-height: 1.4em;
}

.usage-resets-row {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--ocg-font-xs);
  color: var(--ocg-muted);
}

.usage-resets-row :deep(.n-input-number) {
  width: 72px;
}

.usage-resets-hint {
  white-space: nowrap;
}

.usage-save-error {
  color: var(--ocg-error);
  font-size: var(--ocg-font-xs);
}

@media (max-width: 900px) {
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

@media (max-width: 640px) {
  .account-card :deep(.n-card-header) {
    flex-wrap: wrap;
    gap: 8px;
  }

  .account-card :deep(.n-card-header__main),
  .account-card :deep(.n-card-header__extra) {
    width: 100%;
  }

  .account-card :deep(.n-card-header__extra) {
    display: flex;
    justify-content: flex-end;
    margin-left: 0;
  }
}
</style>
