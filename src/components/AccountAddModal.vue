<template>
  <n-modal
    :show="show"
    preset="card"
    :title="t('新增账号')"
    class="account-add-modal"
    style="width: 920px; max-width: calc(100vw - 32px)"
    :mask-closable="false"
    :close-on-esc="!interactionLocked"
    @update:show="onOuterUpdateShow"
  >
    <div v-if="showCatalogLoading" class="account-add-loading">
      <n-spin size="large" :description="t('加载中…')" />
    </div>

    <div v-else class="account-add-layout">
      <div class="account-add-mobile">
        <n-select
          :value="selectedOptionId || null"
          :options="selectOptions"
          filterable
          :disabled="interactionLocked"
          :aria-label="t('选择要添加的方案')"
          :consistent-menu-width="false"
          @update:value="selectOption"
        />
      </div>

      <aside
        class="account-add-rail"
        :aria-label="t('选择要添加的方案')"
        @keydown="onRailKeydown"
      >
        <div class="account-add-search">
          <n-input
            v-model:value="presetQuery"
            size="small"
            clearable
            :placeholder="t('搜索全部选项')"
            :input-props="{ 'aria-label': t('搜索全部选项') }"
          />
        </div>
        <div class="account-add-list">
          <section v-for="group in chooserGroups" :key="group.id" class="account-add-group">
            <h3 class="account-add-group__label">{{ group.label }}</h3>
            <button
              v-for="option in group.options"
              :id="`account-add-option-${option.optionId}`"
              :key="option.optionId"
              type="button"
              class="account-add-item"
              :class="{
                'account-add-item--active': option.optionId === selectedOptionId,
                'account-add-item--disabled': isChooserOptionDisabled(option),
              }"
              :aria-pressed="option.optionId === selectedOptionId"
              :aria-current="option.optionId === selectedOptionId ? 'true' : undefined"
              @click="selectOption(option.optionId)"
            >
              <n-icon :component="iconFor(chooserOptionIconKey(option))" size="16" aria-hidden="true" />
              <span class="account-add-item__label">{{ option.label }}</span>
            </button>
          </section>
          <p v-if="presetSearchMiss" class="account-add-empty" role="status">{{ t("无匹配选项") }}</p>
        </div>
      </aside>

      <div v-if="selected && detail" class="account-add-detail">
        <header class="account-add-detail__header">
          <n-icon :component="iconFor(detail.iconKey)" size="22" aria-hidden="true" />
          <div class="account-add-detail__titles">
            <h2>{{ detail.title }}</h2>
            <n-tag
              v-if="detail.tag"
              size="small"
              :bordered="false"
              :type="detail.tag.type"
            >
              {{ t(detail.tag.label) }}
            </n-tag>
            <span v-if="detail.links" class="account-add-detail__links">
              <a :href="detail.links.docsUrl" target="_blank" rel="noopener noreferrer">{{ t("官方文档") }}</a>
              <a :href="detail.links.websiteUrl" target="_blank" rel="noopener noreferrer">{{ t("控制台") }}</a>
            </span>
          </div>
        </header>

        <template v-if="selectedPlanOption">
          <n-alert
            v-if="selectedPlanOption.disabled"
            type="warning"
            :title="selectedPlanOption.disabledReason ? t(selectedPlanOption.disabledReason) : ''"
          />

          <template v-else>
            <AccountFormModal
              v-if="show"
              embedded
              :show="true"
              :account="null"
              :busy="createBusy"
              :plan="selectedPlanOption.plan"
              :catalog="catalog ?? null"
              @save="(payload) => emit('saveAccount', payload)"
            />

            <template v-if="selectedPlanOption.managed">
              <n-alert
                v-if="!managedAvailable"
                type="warning"
                class="account-add-hint"
              >
                <div class="account-add-hint__content">
                  <span>{{ dashboardErrorDetail(managedReason) }}</span>
                  <n-button v-if="inviteMissing" text type="primary" @click="emit('openInviteUrl')">
                    {{ t("前往 OpenCode Go 填写邀请链接") }}
                  </n-button>
                </div>
              </n-alert>
              <div class="account-add-detail__actions">
                <n-tooltip :disabled="managedAvailable">
                  <template #trigger>
                    <n-button
                      secondary
                      :disabled="!managedAvailable || interactionLocked"
                      @click="managedAvailable && !interactionLocked && emit('registerManaged')"
                    >
                      {{ t("注册新账号（Beta）") }}
                    </n-button>
                  </template>
                  {{ dashboardErrorDetail(managedReason) }}
                </n-tooltip>
              </div>
            </template>
          </template>
        </template>

        <DynamicProviderModal
          v-else-if="selectedPresetOption && show"
          embedded
          :show="true"
          :provider="null"
          :initial-preset-id="selectedPresetOption.preset.id"
          preset-selection-locked
          context="account"
          @saved="(providerId) => emit('presetSaved', providerId)"
          @conflict="emit('presetConflict')"
          @busy-change="embeddedFormBusy = $event"
        />

        <PlatformAccountFormModal
          v-else-if="selectedPlatformOption && show"
          embedded
          :show="true"
          :editing="null"
          :preset-kind="selectedPlatformOption.kind"
          :busy="platformBusy"
          @save="(payload) => emit('createPlatform', payload)"
        />
      </div>
    </div>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, toRef, watch } from "vue";
import type { Component } from "vue";
import {
  NAlert,
  NButton,
  NIcon,
  NInput,
  NModal,
  NSelect,
  NSpin,
  NTag,
  NTooltip,
} from "naive-ui";
import {
  KeyOutlined,
  CloudOutlined,
  ApiOutlined,
  DatabaseOutlined,
  SwapOutlined,
} from "@vicons/antd";
import { t } from "../i18n/index.ts";
import { useLocalizedModalCloseLabel } from "../utils/modal-close-label.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";
import {
  buildChooserGroups,
  chooserOptionIconKey,
  chooserSelectOptions,
  chooserUniverse,
  defaultChooserOptionId,
  describeChooserSelection,
  isChooserOptionDisabled,
  isValidChooserOption,
  visibleChooserOptions,
} from "../domain/account-add-chooser.ts";
import { isDynamicCatalogEntry } from "../domain/dynamic-provider.ts";
import { providerApi } from "../api/providers.ts";
import type { AccountInput } from "../api/dashboard.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import AccountFormModal, { type AccountFormPayload } from "./AccountFormModal.vue";
import DynamicProviderModal from "./DynamicProviderModal.vue";
import PlatformAccountFormModal, {
  type PlatformAccountFormPayload,
} from "./PlatformAccountFormModal.vue";

const props = defineProps<{
  show: boolean;
  catalog: readonly ProviderCatalogEntry[] | null | undefined;
  catalogLoading: boolean;
  managedAvailable: boolean;
  managedReason: string;
  inviteMissing: boolean;
  /** Parent account mutation in flight; embedded account forms bind to it. */
  createBusy: boolean;
  /** Platform section mutation in flight; embedded platform form binds to it. */
  platformBusy: boolean;
  /**
   * One-shot deep-link target (e.g. from the Suppliers Custom API row):
   * preselected only on the closed-to-open transition, never re-applied while
   * the modal is open, so a catalog refresh cannot steal the user's draft.
   */
  initialOptionId?: string | null;
}>();

const emit = defineEmits<{
  (event: "update:show", value: boolean): void;
  (event: "registerManaged"): void;
  (event: "openInviteUrl"): void;
  /** Create path only; the parent owns the account mutation and closes us. */
  (event: "saveAccount", payload: AccountInput | AccountFormPayload): void;
  /** The platform section owns validation, CAS handling, and the write. */
  (event: "createPlatform", payload: PlatformAccountFormPayload): void;
  /** The atomic supplier+first-account create already persisted both lists. */
  (event: "presetSaved", providerId: string): void;
  (event: "presetConflict"): void;
}>();

useLocalizedModalCloseLabel(toRef(props, "show"), "account-add-modal");

const selectedOptionId = ref<string>("");
const presetQuery = ref("");
/**
 * provider_id → persisted preset_id for saved user-defined Providers, loaded
 * once per catalog revision so their preset offering can group them. Entries
 * that fail to load stay absent (API). Selection is by optionId, so a late
 * completion only re-slots rail items and never unmounts an open form.
 */
const dynamicPresetIds = ref<ReadonlyMap<string, string | null>>(new Map());
let dynamicPresetGeneration = 0;
/** In-flight save/test/discovery inside the embedded dynamic-provider form. */
const embeddedFormBusy = ref(false);

/**
 * The full-screen spinner is only for the first open with no catalog at all.
 * Once the layout has rendered, a background catalog reconciliation (e.g. the
 * reload after a CAS conflict on save) must not swap back to the spinner:
 * doing so unmounts the embedded form and destroys the user's draft, Key,
 * and the conflict notice they need for an explicit retry.
 */
const layoutRendered = ref(false);
const showCatalogLoading = computed(() => (
  props.catalogLoading && !layoutRendered.value && !props.catalog
));

watch(
  () => [props.show, props.catalogLoading] as const,
  ([visible, loading]) => {
    if (visible && !loading) layoutRendered.value = true;
  },
  { immediate: true },
);

watch(
  () => [props.show, props.catalog] as const,
  ([visible, catalog]) => {
    const generation = ++dynamicPresetGeneration;
    if (!visible) return;
    const ids = (catalog ?? [])
      .filter(isDynamicCatalogEntry)
      .map((entry) => entry.provider_id);
    if (ids.length === 0) {
      dynamicPresetIds.value = new Map();
      return;
    }
    void Promise.allSettled(ids.map((id) => providerApi.getDynamicProvider(id))).then((results) => {
      if (generation !== dynamicPresetGeneration) return;
      const next = new Map<string, string | null>();
      results.forEach((result, index) => {
        next.set(ids[index]!, result.status === "fulfilled" ? result.value.preset_id : null);
      });
      dynamicPresetIds.value = next;
    });
  },
  { immediate: true },
);

const chooserGroups = computed(() => (
  buildChooserGroups(props.catalog, dynamicPresetIds.value, presetQuery.value)
));
const universe = computed(() => chooserUniverse(props.catalog, dynamicPresetIds.value));
const navOptions = computed(() => visibleChooserOptions(chooserGroups.value));
const selectOptions = computed(() => chooserSelectOptions(chooserGroups.value, t("用户定义")));
const presetSearchMiss = computed(() => Boolean(presetQuery.value.trim()) && navOptions.value.length === 0);

const selected = computed(() => (
  universe.value.find((option) => option.optionId === selectedOptionId.value) ?? null
));
const detail = computed(() => (
  selected.value ? describeChooserSelection(selected.value) : null
));
const selectedPlanOption = computed(() => (
  selected.value && "plan" in selected.value ? selected.value : null
));
const selectedPresetOption = computed(() => (
  selected.value && "preset" in selected.value ? selected.value : null
));
const selectedPlatformOption = computed(() => (
  selected.value && !("plan" in selected.value) && !("preset" in selected.value)
    ? selected.value
    : null
));

/**
 * Any in-flight create (parent account save, platform save, or embedded
 * supplier save/test/discovery) blocks closing and switching, so a late
 * success can never land in a different form or duplicate a write.
 */
const interactionLocked = computed(() => (
  props.createBusy || props.platformBusy || embeddedFormBusy.value
));

function selectOption(value: string): void {
  if (interactionLocked.value) return;
  if (isValidChooserOption(universe.value, value)) {
    selectedOptionId.value = value;
  }
}

watch(selectedOptionId, () => {
  // A selection swap unmounts the previous embedded form; its in-flight flags
  // die with it, so the close guard must not outlive the form.
  embeddedFormBusy.value = false;
});

let chooserWasVisible = false;
watch(
  () => [props.show, universe.value, props.initialOptionId] as const,
  ([visible, options, initialOptionId]) => {
    const justOpened = visible && !chooserWasVisible;
    chooserWasVisible = visible;
    if (!visible) {
      embeddedFormBusy.value = false;
      return;
    }
    // The search resets only on a fresh open; a background catalog or
    // preset-id reload must not clear what the user is typing.
    if (justOpened) {
      presetQuery.value = "";
      if (initialOptionId && isValidChooserOption(options, initialOptionId)) {
        selectedOptionId.value = initialOptionId;
        return;
      }
    }
    if (!isValidChooserOption(options, selectedOptionId.value)) {
      selectedOptionId.value = defaultChooserOptionId(options);
    }
  },
  { immediate: true },
);

function onRailKeydown(event: KeyboardEvent): void {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  const ids = navOptions.value.map((option) => option.optionId);
  if (ids.length === 0) return;
  event.preventDefault();
  const current = ids.indexOf(selectedOptionId.value);
  const delta = event.key === "ArrowDown" ? 1 : -1;
  const next = ids[(current + delta + ids.length) % ids.length];
  if (!next) return;
  selectOption(next);
  void nextTick(() => {
    document.getElementById(`account-add-option-${next}`)?.scrollIntoView({ block: "nearest" });
  });
}

function onOuterUpdateShow(value: boolean): void {
  if (value) {
    emit("update:show", true);
    return;
  }
  // Failed saves keep the draft; in-flight work keeps the modal open.
  if (interactionLocked.value) return;
  emit("update:show", false);
}

const ICONS: Record<string, Component> = {
  "opencode-go": CloudOutlined,
  "command-code-goat": ApiOutlined,
  "minimax-cn": ApiOutlined,
  "kimi-cn": ApiOutlined,
  "custom-endpoint": SwapOutlined,
  api: ApiOutlined,
  database: DatabaseOutlined,
};

function iconFor(iconKey: string): Component {
  return ICONS[iconKey] ?? KeyOutlined;
}
</script>

<style scoped>
.account-add-loading {
  display: grid;
  place-items: center;
  min-height: 220px;
}

/* Fixed-height shell: the rail list and the embedded form body are the only
   scroll regions; the detail header and the form footer stay put. */
.account-add-layout {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
  height: min(620px, calc(100vh - 96px));
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
}

.account-add-mobile {
  display: none;
}

.account-add-rail {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  border-right: 1px solid var(--ocg-border);
  background: var(--ocg-canvas);
}

.account-add-search {
  flex: none;
  padding: 8px 12px;
  background: var(--ocg-canvas);
}

/* One list scrolls; the Plan / API group labels stick to its top edge. */
.account-add-list {
  flex: 1;
  min-height: 0;
  padding-bottom: 12px;
  overflow: auto;
}

.account-add-group + .account-add-group {
  border-top: 1px solid var(--ocg-border);
}

.account-add-group__label {
  position: sticky;
  top: 0;
  z-index: 1;
  margin: 0;
  padding: 8px 12px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
  font-weight: 600;
  line-height: 1.3;
  background: var(--ocg-canvas);
}

.account-add-empty {
  margin: 0;
  padding: 8px 12px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
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
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
  min-height: 0;
  padding: 16px 20px;
  overflow: hidden;
}

.account-add-detail__header {
  display: flex;
  flex: none;
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

.account-add-detail__links {
  display: flex;
  gap: 12px;
  font-size: var(--ocg-font-xs);
}

.account-add-detail__actions {
  display: flex;
  flex: none;
  gap: 8px;
}

.account-add-hint {
  flex: none;
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
    height: auto;
  }

  .account-add-rail {
    display: none;
  }

  .account-add-mobile {
    display: block;
    padding: 12px 12px 0;
  }

  .account-add-detail {
    overflow: visible;
  }
}
</style>
