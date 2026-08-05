<template>
  <n-modal
    :show="show"
    preset="card"
    :title="t('新增账号')"
    class="account-add-modal"
    style="width: 720px; max-width: calc(100vw - 32px)"
    @update:show="$emit('update:show', $event)"
  >
    <div class="account-add-grid">
      <button type="button" class="account-add-option" @click="$emit('importKey')">
        <n-icon :component="KeyOutlined" size="28" aria-hidden="true" />
        <span class="account-add-option__title">{{ t("导入已有 Key") }}</span>
        <span>{{ t("已有 OpenCode Go Key，直接添加并参与账号路由。") }}</span>
      </button>
      <n-tooltip :disabled="managedAvailable">
        <template #trigger>
          <button
            type="button"
            class="account-add-option"
            :class="{ 'account-add-option--disabled': !managedAvailable }"
            :disabled="!managedAvailable"
            @click="$emit('registerManaged')"
          >
            <n-icon :component="UserAddOutlined" size="28" aria-hidden="true" />
            <span class="account-add-option__title">{{ t("注册新账号（Beta）") }}</span>
            <span>{{ t("独立 Profile：登录 → 邀请 → 支付 → 验证 Key。") }}</span>
          </button>
        </template>
        {{ managedReason }}
      </n-tooltip>
    </div>
    <n-alert v-if="!managedAvailable" type="warning" class="account-add-hint">
      <div class="account-add-hint__content">
        <span>{{ managedReason }}</span>
        <n-button v-if="inviteMissing" text type="primary" @click="$emit('openSettings')">
          {{ t("前往设置邀请链接") }}
        </n-button>
      </div>
    </n-alert>
  </n-modal>
</template>

<script setup lang="ts">
import { KeyOutlined, UserAddOutlined } from "@vicons/antd";
import { NAlert, NButton, NIcon, NModal, NTooltip } from "naive-ui";
import { t } from "../i18n/index.ts";

defineProps<{
  show: boolean;
  managedAvailable: boolean;
  managedReason: string;
  inviteMissing: boolean;
}>();

defineEmits<{
  (event: "update:show", value: boolean): void;
  (event: "importKey"): void;
  (event: "registerManaged"): void;
  (event: "openSettings"): void;
}>();
</script>

<style scoped>
.account-add-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
}

.account-add-option {
  display: grid;
  justify-items: start;
  gap: 10px;
  min-height: 180px;
  padding: 22px;
  border: 1px solid var(--ocg-divider);
  border-radius: 14px;
  color: var(--ocg-muted);
  font: inherit;
  text-align: left;
  background: var(--ocg-surface);
  cursor: pointer;
  transition: border-color 0.16s ease, box-shadow 0.16s ease, transform 0.16s ease;
}

.account-add-option:hover:not(:disabled),
.account-add-option:focus-visible:not(:disabled) {
  border-color: var(--ocg-primary);
  box-shadow: 0 8px 24px color-mix(in srgb, var(--ocg-primary) 14%, transparent);
  transform: translateY(-1px);
  outline: none;
}

.account-add-option :deep(.n-icon) {
  color: var(--ocg-primary);
}

.account-add-option__title {
  color: var(--ocg-ink);
  font-size: var(--ocg-font-lg);
  font-weight: 700;
}

.account-add-option--disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.account-add-hint {
  margin-top: 14px;
}

.account-add-hint__content {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

@media (max-width: 640px) {
  .account-add-grid {
    grid-template-columns: 1fr;
  }

  .account-add-option {
    min-height: 140px;
  }
}
</style>
