<template>
  <n-modal
    :show="show"
    preset="card"
    :title="t('新增账号')"
    class="account-add-modal"
    style="width: 760px; max-width: calc(100vw - 32px)"
    @update:show="$emit('update:show', $event)"
  >
    <div v-if="catalogLoading" class="account-add-loading">
      <n-spin size="large" :description="t('加载中…')" />
    </div>

    <div v-else class="account-add-layout">
      <div class="account-add-mobile">
        <n-select
          :value="selectedPlanId || null"
          :options="selectOptions"
          :aria-label="t('选择要添加的方案')"
          :consistent-menu-width="false"
          @update:value="selectPlanId"
        />
      </div>

      <aside
        class="account-add-rail"
        :aria-label="t('选择要添加的方案')"
        @keydown="onRailKeydown"
      >
        <section v-for="group in groups" :key="group.id" class="account-add-group">
          <h3 class="account-add-group__label">{{ t(group.label) }}</h3>
          <button
            v-for="option in group.options"
            :id="`account-add-option-${option.optionId}`"
            :key="option.optionId"
            type="button"
            class="account-add-item"
            :class="{
              'account-add-item--active': option.optionId === selectedPlanId,
              'account-add-item--disabled': option.disabled,
            }"
            :aria-pressed="option.optionId === selectedPlanId"
            :aria-current="option.optionId === selectedPlanId ? 'true' : undefined"
            @click="selectPlanId(option.optionId)"
          >
            <n-icon :component="planIcon(option.plan.id)" size="16" aria-hidden="true" />
            <span class="account-add-item__label">{{ option.label }}</span>
          </button>
        </section>
      </aside>

      <div v-if="selectedOption" class="account-add-detail">
        <header class="account-add-detail__header">
          <n-icon :component="planIcon(selectedOption.plan.id)" size="22" aria-hidden="true" />
          <div class="account-add-detail__titles">
            <h2>{{ selectedOption.label }}</h2>
            <n-tag
              v-if="selectedOption.source === 'user-defined'"
              size="small"
              :bordered="false"
            >
              {{ t("用户定义") }}
            </n-tag>
            <n-tag
              v-else-if="planKindTag(selectedOption.plan)"
              size="small"
              :bordered="false"
              :type="planKindTag(selectedOption.plan)!.type"
            >
              {{ planKindTag(selectedOption.plan)!.label }}
            </n-tag>
          </div>
        </header>

        <p v-if="planDescription(selectedOption.plan)" class="account-add-detail__copy">
          {{ planDescription(selectedOption.plan) }}
        </p>
        <p
          v-if="selectedOption.managed"
          class="account-add-detail__copy account-add-detail__copy--secondary"
        >
          {{ t("独立 Profile：登录 → 邀请 → 支付 → 验证 Key。") }}
        </p>

        <n-alert
          v-if="selectedOption.disabled"
          type="warning"
          :title="selectedOption.disabledReason ? t(selectedOption.disabledReason) : ''"
        />
        <n-alert
          v-else-if="selectedOption.creationHint"
          type="default"
          :title="t(selectedOption.creationHint)"
        />
        <n-alert
          v-if="selectedOption.managed && !managedAvailable"
          type="warning"
          class="account-add-hint"
        >
          <div class="account-add-hint__content">
            <span>{{ managedReason }}</span>
            <n-button v-if="inviteMissing" text type="primary" @click="$emit('openSettings')">
              {{ t("前往设置邀请链接") }}
            </n-button>
          </div>
        </n-alert>

        <n-space v-if="selectedOption.managed" :size="8" class="account-add-detail__actions">
          <n-button secondary @click="$emit('importKey')">
            {{ t("导入已有 Key") }}
          </n-button>
          <n-tooltip :disabled="managedAvailable">
            <template #trigger>
              <n-button
                type="primary"
                :disabled="!managedAvailable"
                @click="managedAvailable && $emit('registerManaged')"
              >
                {{ t("注册新账号（Beta）") }}
              </n-button>
            </template>
            {{ managedReason }}
          </n-tooltip>
        </n-space>

        <n-space
          v-else-if="!selectedOption.disabled"
          :size="8"
          class="account-add-detail__actions"
        >
          <n-button
            :type="selectedOption.plan.id === 'custom-endpoint' ? 'primary' : 'default'"
            @click="handleSelect(selectedOption)"
          >
            {{ t(planActionLabel(selectedOption)) }}
          </n-button>
        </n-space>
      </div>
    </div>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref, toRef, watch } from "vue";
import type { Component } from "vue";
import {
  NAlert,
  NButton,
  NIcon,
  NModal,
  NSelect,
  NSpace,
  NSpin,
  NTag,
  NTooltip,
  type SelectGroupOption,
  type SelectOption,
} from "naive-ui";
import {
  KeyOutlined,
  CloudOutlined,
  ApiOutlined,
  SwapOutlined,
} from "@vicons/antd";
import { t, type MessageKey } from "../i18n/index.ts";
import { useLocalizedModalCloseLabel } from "../utils/modal-close-label.ts";
import {
  buildPlanChooserGroups,
  planChooserGroupId,
  type PlanOption,
} from "../domain/account-plan-options.ts";
import type { PlanDefinition } from "../domain/plans.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";

const props = defineProps<{
  show: boolean;
  catalog: readonly ProviderCatalogEntry[] | null | undefined;
  catalogLoading: boolean;
  managedAvailable: boolean;
  managedReason: string;
  inviteMissing: boolean;
}>();

const emit = defineEmits<{
  (event: "update:show", value: boolean): void;
  (event: "importKey"): void;
  (event: "registerManaged"): void;
  (event: "openSettings"): void;
  (event: "selectPlan", plan: PlanDefinition): void;
}>();

useLocalizedModalCloseLabel(toRef(props, "show"), "account-add-modal");

const selectedPlanId = ref<string>("");

const groups = computed(() => buildPlanChooserGroups(props.catalog));
const flatOptions = computed(() => groups.value.flatMap((group) => group.options));
const selectedOption = computed(() => (
  flatOptions.value.find((option) => option.optionId === selectedPlanId.value) ?? null
));

const selectOptions = computed<Array<SelectOption | SelectGroupOption>>(() => (
  groups.value.map((group) => ({
    type: "group" as const,
    key: group.id,
    label: t(group.label),
    children: group.options.map((option) => ({
      label: option.source === "user-defined" ? `${option.label} · ${t("用户定义")}` : option.label,
      value: option.optionId,
    })),
  }))
));

function defaultPlanId(): string {
  return flatOptions.value.find((option) => !option.disabled)?.optionId
    ?? flatOptions.value[0]?.optionId
    ?? "";
}

function selectPlanId(value: string): void {
  if (flatOptions.value.some((option) => option.optionId === value)) {
    selectedPlanId.value = value;
  }
}

watch(
  () => [props.show, flatOptions.value] as const,
  ([visible, options]) => {
    if (!visible) return;
    if (!options.some((option) => option.optionId === selectedPlanId.value)) {
      selectedPlanId.value = defaultPlanId();
    }
  },
  { immediate: true },
);

function onRailKeydown(event: KeyboardEvent): void {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  const ids = flatOptions.value.map((option) => option.optionId);
  if (ids.length === 0) return;
  event.preventDefault();
  const current = ids.indexOf(selectedPlanId.value);
  const delta = event.key === "ArrowDown" ? 1 : -1;
  const next = ids[(current + delta + ids.length) % ids.length];
  if (next) selectedPlanId.value = next;
}

const ICONS: Record<string, Component> = {
  "opencode-go": CloudOutlined,
  "command-code-goat": ApiOutlined,
  "minimax-cn": ApiOutlined,
  "kimi-cn": ApiOutlined,
  "custom-endpoint": SwapOutlined,
};

function planIcon(planId: string): Component {
  return ICONS[planId] ?? KeyOutlined;
}

function planKindTag(plan: PlanDefinition): { label: string; type: "warning" | "default" } | null {
  if (plan.kind === "custom") return { label: t("自定义端点"), type: "default" };
  if (plan.id === "dynamic-http") return { label: t("用户定义"), type: "default" };
  return null;
}

function planDescription(plan: PlanDefinition): string {
  switch (plan.id) {
    case "opencode-go":
      return t("已有 OpenCode Go Key，直接添加并参与账号路由。");
    case "minimax-cn":
      return `${t("API Key")} · ${t("刷新模型目录")}`;
    case "kimi-cn":
      return `${t("API Key")} · ${t("刷新模型目录")}`;
    case "custom-endpoint":
      return t("自定义端点由你自行维护，Gateway 无法验证其价格、额度与协议兼容性。");
    case "dynamic-http":
      return t("账号不拥有 Endpoint、协议或模型映射。");
    default:
      return "";
  }
}

function planActionLabel(option: PlanOption): MessageKey {
  if (option.plan.id === "custom-endpoint") return "添加账号";
  if (planChooserGroupId(option, props.catalog) === "draft") return "创建草稿";
  return "添加账号";
}

function handleSelect(option: PlanOption): void {
  if (option.disabled || option.managed) return;
  emit("selectPlan", option.plan);
}
</script>

<style scoped>
.account-add-loading {
  display: grid;
  place-items: center;
  min-height: 220px;
}

.account-add-layout {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  min-height: 280px;
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
}

.account-add-mobile {
  display: none;
}

.account-add-rail {
  min-width: 0;
  padding: 8px 0 12px;
  overflow: auto;
  border-right: 1px solid var(--ocg-border);
  background: var(--ocg-canvas);
}

.account-add-group + .account-add-group {
  margin-top: 8px;
}

.account-add-group__label {
  margin: 0;
  padding: 8px 12px 4px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
  font-weight: 600;
  line-height: 1.3;
}

.account-add-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  margin: 0;
  padding: 8px 12px;
  border: 0;
  border-radius: 0;
  color: var(--ocg-ink);
  font: inherit;
  font-size: var(--ocg-font-sm);
  text-align: left;
  background: transparent;
  cursor: pointer;
}

.account-add-item :deep(.n-icon) {
  flex: none;
  color: var(--ocg-muted);
}

.account-add-item__label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.account-add-item:hover,
.account-add-item:focus-visible {
  background: var(--ocg-primary-soft);
  outline: none;
}

.account-add-item--active {
  background: var(--ocg-primary-soft);
  font-weight: 600;
}

.account-add-item--active :deep(.n-icon) {
  color: var(--ocg-primary);
}

.account-add-item--disabled {
  color: var(--ocg-muted);
}

.account-add-detail {
  display: grid;
  align-content: start;
  gap: 12px;
  min-width: 0;
  padding: 20px;
}

.account-add-detail__header {
  display: flex;
  align-items: center;
  gap: 12px;
}

.account-add-detail__header :deep(.n-icon) {
  color: var(--ocg-primary);
}

.account-add-detail__titles {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.account-add-detail__titles h2 {
  margin: 0;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-lg);
  font-weight: 700;
  line-height: 1.3;
}

.account-add-detail__copy {
  margin: 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
  line-height: 1.5;
}

.account-add-detail__copy--secondary {
  font-size: var(--ocg-font-xs);
}

.account-add-detail__actions {
  margin-top: 4px;
}

.account-add-hint__content {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

@media (max-width: 640px) {
  .account-add-layout {
    grid-template-columns: minmax(0, 1fr);
  }

  .account-add-rail {
    display: none;
  }

  .account-add-mobile {
    display: block;
    padding: 12px 12px 0;
  }
}
</style>
