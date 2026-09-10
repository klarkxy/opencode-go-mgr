<template>
  <div class="providers-page">
    <header class="providers-header">
      <h1>{{ t("供应商") }}</h1>
      <n-button type="primary" :disabled="actionLocked" @click="openCreateDynamic">
        {{ t("新建供应商") }}
      </n-button>
    </header>

    <div
      v-if="initialLoading"
      class="providers-state"
      role="status"
      aria-live="polite"
      :aria-label="t('加载中…')"
    >
      <n-spin size="small" />
    </div>

    <n-alert
      v-else-if="loadError && !contracts"
      type="error"
      :title="t('加载供应商失败: {error}', { error: loadError })"
    >
      <n-button size="small" secondary :loading="loading" @click="loadContracts()">
        {{ t("重试") }}
      </n-button>
    </n-alert>

    <n-empty
      v-else-if="!loading && scopes.length === 0 && dynamicEntries.length === 0 && !selectedPreset"
      :description="t('暂无供应商范围')"
    />

    <div v-else-if="activeScope || selectedDynamic || selectedPreset" class="providers-layout">
      <aside class="providers-rail">
        <div class="providers-rail-search">
          <n-input
            v-model:value="presetQuery"
            size="small"
            clearable
            :placeholder="browsingPresets ? t('搜索预设') : t('搜索供应商')"
            :input-props="{ 'aria-label': browsingPresets ? t('搜索预设') : t('搜索供应商') }"
          />
        </div>
        <div class="providers-rail-list">
          <section v-for="pane in scopeMenuPanes" :key="pane.id" class="providers-rail-pane">
            <h3 class="providers-rail-pane__label">{{ pane.label }}</h3>
            <n-menu
              :value="selectedKey"
              :options="pane.options"
              :default-expanded-keys="railDefaultExpandedKeys"
              :aria-label="`${t('选择供应商范围')} · ${pane.label}`"
              @update:value="selectScopeKey"
            />
          </section>
          <p v-if="railFilteredOut" class="providers-rail-empty">
            {{ browsingPresets ? t("无匹配预设") : t("无匹配供应商") }}
          </p>
        </div>
        <div class="providers-rail-footer">
          <n-button
            secondary
            size="small"
            block
            :disabled="inlineFormBusy"
            @click="togglePresetBrowsing"
          >
            {{ browsingPresets ? t("已有连接") : t("添加新服务") }}
          </n-button>
        </div>
      </aside>

      <div class="providers-main">
        <div class="providers-mobile-nav">
          <n-select
            :value="selectedKey"
            :options="scopeSelectOptions"
            filterable
            :aria-label="t('选择供应商范围')"
            :disabled="actionLocked || inlineFormBusy"
            :consistent-menu-width="false"
            @update:value="selectScopeKey"
          />
        </div>

        <n-alert
          v-if="loadError && contracts"
          type="warning"
          :title="t('加载供应商失败: {error}', { error: loadError })"
        >
          <n-button size="small" secondary :loading="loading" @click="loadContracts({ retain: true })">
            {{ t("重试") }}
          </n-button>
        </n-alert>

        <section v-if="selectedDynamic" class="providers-section" aria-labelledby="dynamic-provider-title">
          <div class="providers-catalog-head">
            <div class="providers-catalog-heading">
              <h2 id="dynamic-provider-title">{{ selectedDynamic.name }}</h2>
              <div class="providers-catalog-meta">
                <n-tag size="small" :bordered="false">{{ t("用户定义") }}</n-tag>
                <span>{{ t("该供应商没有价格或官方用量。") }}</span>
              </div>
            </div>
            <n-space>
              <n-button secondary :disabled="actionLocked" @click="openEditDynamic">{{ t("编辑供应商") }}</n-button>
              <n-popconfirm
                :positive-text="t('删除')"
                :negative-text="t('取消')"
                @positive-click="deleteSelectedDynamic"
              >
                <template #trigger>
                  <n-button type="error" secondary :disabled="actionLocked">{{ t("删除供应商") }}</n-button>
                </template>
                {{ t("请先删除引用该供应商的账号，再删除供应商。不会级联删除账号。") }}
              </n-popconfirm>
            </n-space>
          </div>
          <dl class="dynamic-provider-facts">
            <div><dt>{{ t("API 地址") }}</dt><dd><code>{{ selectedDynamic.endpoint_url }}</code></dd></div>
            <div><dt>{{ t("上游协议") }}</dt><dd>{{ protocolDisplayName(selectedDynamic.upstream_protocol) }}</dd></div>
            <div><dt>{{ t("鉴权方式") }}</dt><dd>{{ authDisplayName(selectedDynamic.auth_kind) }}</dd></div>
          </dl>
          <div class="providers-alias-table-wrap">
            <table class="providers-alias-table">
              <thead>
                <tr>
                  <th>{{ t("对外模型名") }}</th>
                  <th>{{ t("上游模型 ID") }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="model in selectedDynamic.models" :key="model.public_model">
                  <td><code>{{ model.public_model }}</code></td>
                  <td><code>{{ model.upstream_model }}</code></td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>

        <section v-else-if="selectedPreset" class="providers-section" aria-labelledby="preset-provider-title">
          <div class="providers-catalog-head">
            <div class="providers-catalog-heading">
              <h2 id="preset-provider-title">{{ selectedPreset.name }}</h2>
              <div class="providers-catalog-meta">
                <n-tag size="small" :bordered="false">
                  {{ providerPresetOffering(selectedPreset) === "plan" ? "Plan" : "API" }}
                </n-tag>
                <n-tag size="small" :bordered="false">{{ t("供应商预设") }}</n-tag>
                <a :href="selectedPreset.docsUrl" target="_blank" rel="noopener noreferrer">{{ t("官方文档") }}</a>
                <a :href="selectedPreset.websiteUrl" target="_blank" rel="noopener noreferrer">{{ t("控制台") }}</a>
              </div>
            </div>
          </div>
          <DynamicProviderModal
            :key="`preset-form:${selectedPreset.id}`"
            embedded
            :show="true"
            :provider="null"
            :initial-preset-id="selectedPreset.id"
            preset-selection-locked
            @saved="onDynamicSaved"
            @conflict="onDynamicConflict"
            @busy-change="inlineFormBusy = $event"
          />
        </section>

        <template v-else-if="activeScope">
          <n-tabs v-model:value="activeTab" class="providers-tabs" display-directive="if">
          <n-tab-pane name="catalog" :tab="t('模型目录')">
            <section class="providers-section" aria-labelledby="provider-catalog-title">
              <div class="providers-catalog-head">
                <div class="providers-catalog-heading">
                  <h2 id="provider-catalog-title">{{ t("模型目录") }}</h2>
                  <div class="providers-catalog-meta">
                    <span>{{ catalogSourceLabel(activeScope.catalog.source) }}</span>
                    <a
                      v-if="safeSourceUrl"
                      :href="safeSourceUrl"
                      target="_blank"
                      rel="noopener noreferrer"
                    >{{ t("官方来源") }}</a>
                    <span v-if="activeScope.catalog.refreshed_at">
                      {{ t("刷新时间") }} · {{ formatTimestamp(activeScope.catalog.refreshed_at) }}
                    </span>
                    <span v-if="activeScope.static_protocol_snapshot_date">
                      {{ t("官方协议基线 {date}；未列出的协议默认关闭", { date: activeScope.static_protocol_snapshot_date }) }}
                    </span>
                  </div>
                </div>
                <div class="providers-catalog-actions">
                  <n-button
                    v-if="catalogRefreshVisible"
                    type="primary"
                    :loading="catalogRefreshing"
                    :disabled="actionLocked"
                    @click="refreshCatalog"
                  >
                    {{ catalogRefreshing ? t("正在刷新模型目录…") : t("刷新模型目录") }}
                  </n-button>
                  <n-popconfirm
                    v-if="staticProtocolResetVisible"
                    @positive-click="resetStaticProtocols"
                  >
                    <template #trigger>
                      <n-button
                        secondary
                        :loading="staticProtocolResetting"
                        :disabled="actionLocked"
                      >
                        {{ t("恢复官方协议基线") }}
                      </n-button>
                    </template>
                    {{ staticProtocolResetConfirmation }}
                  </n-popconfirm>
                </div>
              </div>
              <n-alert
                v-if="catalogRefreshError"
                type="error"
                :title="t('刷新模型目录失败: {error}', { error: catalogRefreshError })"
              />
              <n-alert
                v-if="probeSummary"
                :type="probeSummary.hasFailures ? 'warning' : 'success'"
                :title="probeSummary.hasFailures ? t('连接测试失败') : t('连接测试成功')"
                class="providers-probe-summary"
              >
                <div v-for="result in probeSummary.results" :key="result.protocol" class="providers-probe-result">
                  <strong>{{ protocolDisplayName(result.protocol) }}</strong>
                  <span>{{ probeResultStatus(result) }}</span>
                  <span v-if="probeResultHttpStatus(result.error)">HTTP {{ probeResultHttpStatus(result.error) }}</span>
                  <span v-if="probeResultMessage(result.error)">{{ probeResultMessage(result.error) }}</span>
                  <a
                    v-if="probeResultUrl(result.error)"
                    :href="probeResultUrl(result.error)"
                    target="_blank"
                    rel="noopener noreferrer"
                  >{{ t("帮助链接") }}</a>
                </div>
              </n-alert>
              <n-alert
                v-if="matrixError"
                type="error"
                :title="t('保存协议覆盖失败: {error}', { error: matrixError })"
              />
              <n-alert
                v-if="probeError"
                type="error"
                :title="t('连接测试失败: {error}', { error: probeError })"
              />
              <ProviderModelMatrix
                :scope="activeScope"
                :optimistic-overrides="optimisticOverrides"
                :pending-override-keys="pendingOverrideKeys"
                :probing-models="probingModels"
                :action-locked="matrixActionLocked"
                @update:overrides="updateOverrides"
                @probe="runModelProbe"
                @error="matrixError = $event"
              />
            </section>
          </n-tab-pane>

          <n-tab-pane name="pricing" :tab="t('模型价格')">
            <section class="providers-section" aria-labelledby="provider-pricing-title">
              <h2 id="provider-pricing-title" class="sr-only">{{ t("模型价格") }}</h2>
              <PricingCatalog :provider-id="activeScope.provider_id" />
            </section>
          </n-tab-pane>
          <n-tab-pane v-if="isOpenCodeGoScope" name="other" :tab="t('其他')">
            <OpenCodeInviteUrlField />
          </n-tab-pane>
          </n-tabs>
        </template>
      </div>
    </div>

    <DynamicProviderModal
      v-model:show="showDynamicModal"
      :provider="editingDynamic"
      :initial-preset-id="createPresetId"
      @saved="onDynamicSaved"
      @conflict="onDynamicConflict"
    />
    <span class="sr-only" aria-live="polite" aria-atomic="true">{{ actionLive }}</span>
  </div>
</template>

<script setup lang="ts">
import { computed, h, onActivated, onMounted, onUnmounted, ref, watch } from "vue";
import type { VNodeChild } from "vue";
import {
  NAlert,
  NButton,
  NEmpty,
  NInput,
  NMenu,
  NPopconfirm,
  NSelect,
  NSpace,
  NSpin,
  NTabPane,
  NTabs,
  NTag,
  useMessage,
} from "naive-ui";
import type { MenuOption, SelectOption } from "naive-ui";
import { DashboardRequestError } from "../api/dashboard";
import { isRevisionConflict, providerApi } from "../api/providers.ts";
import { useProvidersStore } from "../stores/providers.ts";
import type {
  DynamicProviderView,
  ModelProtocolOverrideUpdate,
  ProviderCatalogEntry,
  ProviderContractsResponse,
  ProtocolProbeResponse,
  ProtocolProbeResult,
} from "../api/providers.ts";
import ProviderModelMatrix from "../components/ProviderModelMatrix.vue";
import PricingCatalog from "../components/PricingCatalog.vue";
import DynamicProviderModal from "../components/DynamicProviderModal.vue";
import OpenCodeInviteUrlField from "../components/OpenCodeInviteUrlField.vue";
import ProviderBrandMark from "../components/ProviderBrandMark.vue";
import { locale, t } from "../i18n/index.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";
import { applyAppViewSearchParams, PROVIDER_OTHER_TAB, readProviderScopeQuery, resolveAppViewKey } from "./app-navigation.ts";
import {
  applyModelContractToResponse,
  catalogRefreshSupported,
  effectiveModelTestProtocol,
  flattenProviderScopes,
  isSafeSourceUrl,
  modelProtocolOverrideKey,
  normalizeProviderContractsResponse,
  protocolDisplayName,
  selectProviderScope,
  type ProviderScopeView,
} from "../domain/provider-contracts.ts";
import { DEFAULT_PROVIDER_ID } from "../domain/account-providers.ts";
import { isDynamicCatalogEntry } from "../domain/dynamic-provider.ts";
import {
  PROVIDER_PRESETS,
  filterProviderPresets,
  groupProviderPresetsByOffering,
  providerPresetOffering,
  providerPresetOfferingForId,
  type ProviderPreset,
} from "../domain/provider-presets.ts";
import { providerScopeOffering } from "../domain/plans.ts";
import {
  familyOf,
  groupPresetsByFamily,
  type ProviderFamily,
} from "../domain/provider-families.ts";
import {
  CATALOG_SOURCE_CUSTOM_DISCOVERY,
  CATALOG_SOURCE_DECLARED,
  CATALOG_SOURCE_OPENCODE_MODELS,
  CATALOG_SOURCE_COMMAND_CODE_MODELS,
  CATALOG_SOURCE_OFFICIAL_ZEN,
  CATALOG_SOURCE_STATIC,
} from "../domain/provider-contracts.ts";

const message = useMessage();
const providersStore = useProvidersStore();
const contracts = ref<ProviderContractsResponse | null>(null);
const catalog = ref<ProviderCatalogEntry[] | null>(null);
const dynamicDetails = ref<DynamicProviderView[]>([]);
const showDynamicModal = ref(false);
const editingDynamic = ref<DynamicProviderView | null>(null);
const createPresetId = ref<string | null>(null);
/** In-flight save/test/discovery inside the inline preset create form. */
const inlineFormBusy = ref(false);
/**
 * Rail mode: saved configured scopes (default) vs local preset browsing.
 * Browsing is a pure UI mode — preset entries never hit the backend and
 * never imply a configured Provider.
 */
const browsingPresets = ref(false);
const presetQuery = ref("");
const loading = ref(false);
const loadError = ref("");
const selectedKey = ref<string | null>(null);
const activeTab = ref("catalog");
const catalogRefreshing = ref(false);
const staticProtocolResetting = ref(false);
const catalogRefreshError = ref("");
const matrixError = ref("");
const probeError = ref("");
const probeSummary = ref<{ results: ProtocolProbeResult[]; hasFailures: boolean } | null>(null);
const probingModels = ref<Set<string>>(new Set());
const optimisticOverrides = ref<Map<string, boolean>>(new Map());
const pendingOverrideKeys = ref<Set<string>>(new Set());
const actionLive = ref("");
let activatedOnce = false;
let overrideSequence = 0;
let overrideQueue: Promise<void> = Promise.resolve();
const latestOverrideSequence = new Map<string, number>();

const scopes = computed(() => (
  contracts.value
    ? flattenProviderScopes(contracts.value, catalog.value)
      .filter((scope) => scope.scope_kind === "provider")
    : []
));
const dynamicEntries = computed(() => (catalog.value ?? []).filter(isDynamicCatalogEntry));
const selectedDynamic = computed(() => {
  if (!selectedKey.value?.startsWith("dynamic:")) return null;
  const id = selectedKey.value.slice("dynamic:".length);
  return dynamicDetails.value.find((item) => item.id === id) ?? null;
});
// Preset entries are a local UI scope only (scope_kind=preset): they never hit
// the backend and never imply a configured Provider. Matching is the single
// filterProviderPresets pass (family label, name, id, variant, endpoint host).
const presetGroups = computed(() => (
  groupProviderPresetsByOffering(filterProviderPresets(PROVIDER_PRESETS, presetQuery.value))
));
const railFilteredOut = computed(() => {
  if (!presetQuery.value.trim()) return false;
  if (browsingPresets.value) {
    return presetGroups.value.plan.length === 0 && presetGroups.value.api.length === 0;
  }
  return scopeMenuPanes.value.every((pane) => pane.options.length === 0);
});
const selectedPreset = computed(() => {
  if (!selectedKey.value?.startsWith("preset:")) return null;
  const id = selectedKey.value.slice("preset:".length);
  return PROVIDER_PRESETS.find((preset) => preset.id === id) ?? null;
});
const activeSelection = computed(() => {
  const query = selectedKey.value?.split(":") ?? [];
  const scopeKind = query[0] ?? null;
  const scopeId = query.length > 1 ? query.slice(1).join(":") : null;
  return selectProviderScope(scopes.value, scopeKind, scopeId);
});
const activeScope = computed(() => activeSelection.value.scope);
const isOpenCodeGoScope = computed(() => activeScope.value?.provider_id === DEFAULT_PROVIDER_ID);
const initialLoading = computed(() => loading.value && !contracts.value);
const actionLocked = computed(() => (
  catalogRefreshing.value
  || staticProtocolResetting.value
  || probingModels.value.size > 0
  || pendingOverrideKeys.value.size > 0
));
const matrixActionLocked = computed(() => (
  catalogRefreshing.value
  || staticProtocolResetting.value
  || probingModels.value.size > 0
));
/**
 * Rail key for the account-owned Custom API action. It is a navigation entry,
 * never a scope: Custom API accounts are created on the Accounts view and no
 * Provider row exists for them here.
 */
const CUSTOM_API_MENU_KEY = "custom-api";

function scopeOffering(scope: ProviderScopeView): "plan" | "api" {
  // Only the explicit built-in paid families are Plan; Zen Free, Custom, and
  // unknown providers are API.
  return providerScopeOffering(scope.provider_id);
}

/**
 * Offering of a saved user-defined Provider from its persisted preset ID
 * (already loaded in dynamicDetails); unknown or manual rows are API.
 */
function dynamicOffering(providerId: string): "plan" | "api" {
  const detail = dynamicDetails.value.find((item) => item.id === providerId);
  return providerPresetOfferingForId(detail?.preset_id);
}

const dynamicPlanEntries = computed(() => (
  dynamicEntries.value.filter((entry) => dynamicOffering(entry.provider_id) === "plan")
));
const dynamicApiEntries = computed(() => (
  dynamicEntries.value.filter((entry) => dynamicOffering(entry.provider_id) === "api")
));
const planScopes = computed(() => scopes.value.filter((scope) => scopeOffering(scope) === "plan"));
const apiScopes = computed(() => scopes.value.filter((scope) => scopeOffering(scope) === "api"));

const RAIL_BRAND_SIZE = 18;

function presetVariantHost(preset: ProviderPreset): string {
  const raw = preset.endpointUrl || preset.endpointPlaceholder || "";
  if (!raw) return "";
  try {
    return new URL(raw).host;
  } catch {
    return raw;
  }
}

function brandIcon(family: ProviderFamily): () => VNodeChild {
  return () => h(ProviderBrandMark, { family, size: RAIL_BRAND_SIZE });
}

/**
 * Brand for a saved user-defined Provider, resolved only from its persisted
 * preset id (already loaded in dynamicDetails). A manual Provider or an
 * unknown preset id gets no brand — identity is never inferred from the
 * display name or a custom endpoint URL.
 */
function dynamicBrandIcon(providerId: string): (() => VNodeChild) | undefined {
  const detail = dynamicDetails.value.find((item) => item.id === providerId);
  const preset = detail?.preset_id
    ? PROVIDER_PRESETS.find((entry) => entry.id === detail.preset_id) ?? null
    : null;
  return preset ? brandIcon(familyOf(preset)) : undefined;
}

function presetFamilyMenuOptions(
  presets: readonly ProviderPreset[],
  offering: "plan" | "api",
): MenuOption[] {
  return groupPresetsByFamily(presets).map(({ family, presets: familyPresets }) => {
    if (familyPresets.length === 1) {
      const preset = familyPresets[0]!;
      return {
        key: `preset:${preset.id}`,
        label: family.label,
        icon: brandIcon(family),
      };
    }
    return {
      // Family ids carry the offering so the same vendor can appear once per
      // group without a key collision.
      key: `family:${offering}:${family.id}`,
      label: family.label,
      icon: brandIcon(family),
      extra: String(familyPresets.length),
      children: familyPresets.map((preset) => ({
        key: `preset:${preset.id}`,
        label: () => h("span", { class: "providers-rail-preset" }, [
          h("span", { class: "providers-rail-preset__variant" }, preset.variant ?? preset.name),
          presetVariantHost(preset)
            ? h("span", { class: "providers-rail-preset__host mono" }, presetVariantHost(preset))
            : null,
        ]),
        icon: brandIcon(family),
      })),
    };
  });
}

const scopeMenuPanes = computed<Array<{ id: "plan" | "api"; label: "Plan" | "API"; options: MenuOption[] }>>(() => {
  const query = presetQuery.value.trim().toLocaleLowerCase();
  const matches = (label: string) => !query || label.toLocaleLowerCase().includes(query);
  const panes: Array<{ id: "plan" | "api"; label: "Plan" | "API"; options: MenuOption[] }> = [];
  if (browsingPresets.value) {
    const planOptions = presetFamilyMenuOptions(presetGroups.value.plan, "plan");
    const apiOptions = presetFamilyMenuOptions(presetGroups.value.api, "api");
    if (planOptions.length) panes.push({ id: "plan", label: "Plan", options: planOptions });
    panes.push({ id: "api", label: "API", options: apiOptions });
    return panes;
  }
  // Default rail: built-in and saved configured scopes only. Preset browsing
  // is a deliberate local mode entered from the rail footer.
  const scopeItems = (list: readonly ProviderScopeView[]) => (
    list
      .filter((scope) => matches(scope.label))
      .map((scope) => ({ key: scope.key, label: `${scope.label}` }))
  );
  const dynamicItems = (list: readonly ProviderCatalogEntry[]) => (
    list
      .filter((entry) => matches(entry.display_name))
      .map((entry) => ({
        key: `dynamic:${entry.provider_id}`,
        label: entry.display_name,
        icon: dynamicBrandIcon(entry.provider_id),
      }))
  );
  const planOptions: MenuOption[] = [
    ...scopeItems(planScopes.value),
    ...dynamicItems(dynamicPlanEntries.value),
  ];
  const apiOptions: MenuOption[] = [
    ...(matches("custom api") ? [{ key: CUSTOM_API_MENU_KEY, label: "Custom API" }] : []),
    ...scopeItems(apiScopes.value),
    ...dynamicItems(dynamicApiEntries.value),
  ];
  if (planOptions.length) panes.push({ id: "plan", label: "Plan", options: planOptions });
  panes.push({ id: "api", label: "API", options: apiOptions });
  return panes;
});

/**
 * Pre-expand every multi-variant family so the variant picker rows are
 * visible without an extra click. Single-variant families stay flat and
 * need no expansion. The rail menu is uncontrolled after the first render;
 * once the user collapses a family it stays collapsed across navigations.
 */
const railDefaultExpandedKeys = computed<string[]>(() => (
  scopeMenuPanes.value.flatMap((pane) => (
    pane.options.flatMap((option) => (
      option.children && option.key ? [String(option.key)] : []
    ))
  ))
));
const scopeSelectOptions = computed<SelectOption[]>(() => {
  // The mobile selector has its own built-in filter; the rail search query
  // must not shrink these options when the rail itself is hidden.
  const allPresetGroups = groupProviderPresetsByOffering(PROVIDER_PRESETS);
  return [
    ...planScopes.value.map((scope) => ({ value: scope.key, label: `${scope.label} · Plan` })),
    ...dynamicPlanEntries.value.map((entry) => ({
      value: `dynamic:${entry.provider_id}`,
      label: `${entry.display_name} · Plan`,
    })),
    ...allPresetGroups.plan.map((preset) => ({
      value: `preset:${preset.id}`,
      label: `${preset.name} · Plan`,
    })),
    { value: CUSTOM_API_MENU_KEY, label: "Custom API · API" },
    ...apiScopes.value.map((scope) => ({ value: scope.key, label: `${scope.label} · API` })),
    ...dynamicApiEntries.value.map((entry) => ({
      value: `dynamic:${entry.provider_id}`,
      label: `${entry.display_name} · API`,
    })),
    ...allPresetGroups.api.map((preset) => ({
      value: `preset:${preset.id}`,
      label: `${preset.name} · API`,
    })),
  ];
});
const catalogRefreshVisible = computed(() => {
  const scope = activeScope.value;
  return Boolean(scope && catalogRefreshSupported(scope));
});
const staticProtocolResetVisible = computed(() => (
  activeScope.value?.scope_kind === "provider"
  && Boolean(activeScope.value.static_protocol_snapshot_date)
));
const staticProtocolResetConfirmation = computed(() => {
  const scope = activeScope.value;
  return t("不会请求上游；将清除手动开关和探测判断，保留当前目录，恢复 {date} 开发时官方协议基线，并关闭基线中没有的协议。是否继续？", {
    date: scope?.static_protocol_snapshot_date ?? "",
  });
});
const safeSourceUrl = computed(() => {
  const url = activeScope.value?.catalog.source_url ?? "";
  return isSafeSourceUrl(url) ? url : "";
});

function catalogSourceLabel(source: string): string {
  if (source === CATALOG_SOURCE_STATIC) return t("静态目录");
  if (source === CATALOG_SOURCE_OFFICIAL_ZEN) return t("官方 Zen 目录");
  if (source === CATALOG_SOURCE_CUSTOM_DISCOVERY) return t("自定义发现");
  if (source === CATALOG_SOURCE_DECLARED) return t("账号声明");
  if (source === CATALOG_SOURCE_OPENCODE_MODELS) return `OpenCode · ${t("官方来源")}`;
  if (source === CATALOG_SOURCE_COMMAND_CODE_MODELS) return `Command Code · ${t("官方来源")}`;
  return source;
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale.value, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

/**
 * This view stays mounted under KeepAlive after the user leaves it; only
 * touch scope state or the URL when the current URL actually targets it.
 * Legacy "pricing" resolves to providers, so bookmarks keep working.
 */
function currentUrlIsProvidersView(): boolean {
  const view = new URL(window.location.href).searchParams.get("view");
  return resolveAppViewKey(view) === "providers";
}

function writeScopeToUrl(scopeKind: string, scopeId: string) {
  // An in-flight load finishing after navigation must not rewrite the URL
  // (e.g. strip the one-shot Accounts `add` deep link) for another view.
  if (!currentUrlIsProvidersView()) return;
  const url = applyAppViewSearchParams(new URL(window.location.href), "providers", {
    scope_kind: scopeKind,
    scope_id: scopeId,
    ...(isOpenCodeGoScope.value && activeTab.value === PROVIDER_OTHER_TAB
      ? { tab: PROVIDER_OTHER_TAB }
      : {}),
  });
  window.history.replaceState(null, "", url);
}

function applyProviderTabFromQuery(tab: string | null) {
  if (isOpenCodeGoScope.value && tab === PROVIDER_OTHER_TAB) {
    activeTab.value = PROVIDER_OTHER_TAB;
    return;
  }
  if (activeTab.value === PROVIDER_OTHER_TAB) activeTab.value = "catalog";
}

function selectDynamicProvider(providerId: string): boolean {
  if (!dynamicEntries.value.some((entry) => entry.provider_id === providerId)) return false;
  selectedKey.value = `dynamic:${providerId}`;
  writeScopeToUrl("dynamic", providerId);
  return true;
}

function selectPresetScope(presetId: string): boolean {
  if (!PROVIDER_PRESETS.some((preset) => preset.id === presetId)) return false;
  // A preset selection only exists inside the local browsing mode.
  browsingPresets.value = true;
  selectedKey.value = `preset:${presetId}`;
  writeScopeToUrl("preset", presetId);
  return true;
}

function togglePresetBrowsing(): void {
  // A preset form with in-flight save/test/discovery must not be swapped out.
  if (inlineFormBusy.value) return;
  browsingPresets.value = !browsingPresets.value;
}

function applyScopeFromQuery(fellBackNotice = false, preferDynamicId?: string) {
  if (preferDynamicId && selectDynamicProvider(preferDynamicId)) return;
  if (selectedKey.value?.startsWith("dynamic:")) {
    const id = selectedKey.value.slice("dynamic:".length);
    if (selectDynamicProvider(id)) return;
  }
  // A preset selection is valid on its own and must survive reloads instead of
  // being treated as a stale builtin scope.
  if (selectedKey.value?.startsWith("preset:")) {
    const id = selectedKey.value.slice("preset:".length);
    if (selectPresetScope(id)) return;
  }
  const query = readProviderScopeQuery(window.location.search);
  if (query.scope_kind === "dynamic" && query.scope_id && selectDynamicProvider(query.scope_id)) {
    return;
  }
  if (query.scope_kind === "preset" && query.scope_id && selectPresetScope(query.scope_id)) {
    return;
  }
  const selected = selectProviderScope(scopes.value, query.scope_kind, query.scope_id);
  if (!selected.scope) {
    // No saved provider scope: keep the rail usable through preset choices.
    const firstPreset = PROVIDER_PRESETS[0];
    if (firstPreset && selectPresetScope(firstPreset.id)) return;
    selectedKey.value = null;
    return;
  }
  selectedKey.value = selected.scope.key;
  applyProviderTabFromQuery(query.tab);
  writeScopeToUrl(selected.scope.scope_kind, selected.scope.scope_id);
  if (fellBackNotice && selected.fellBack) {
    actionLive.value = t("已选择过期范围，已回到第一个供应商");
  }
}

function selectScopeKey(key: string | number) {
  // A preset form with in-flight save/test/discovery must not be swapped out;
  // its stale-generation guards only cover responses, not dismissal.
  if (inlineFormBusy.value) return;
  const value = String(key);
  if (value === CUSTOM_API_MENU_KEY) {
    // Custom API accounts are account-owned: deep-link straight into Add
    // Account with custom-endpoint preselected. Accounts consumes and
    // deletes the one-shot `add` parameter when it opens the modal.
    const url = applyAppViewSearchParams(new URL(window.location.href), "accounts");
    url.searchParams.set("add", "custom-endpoint");
    window.history.pushState(null, "", url);
    window.dispatchEvent(new PopStateEvent("popstate"));
    return;
  }
  if (value.startsWith("dynamic:")) {
    selectDynamicProvider(value.slice("dynamic:".length));
    return;
  }
  if (value.startsWith("family:")) {
    // Family rows are submenu parents: their native click toggles expansion,
    // they do not select. Treat any stray selection event as a no-op so the
    // rail never blanks the active scope on a parent click.
    return;
  }
  if (value.startsWith("preset:")) {
    selectPresetScope(value.slice("preset:".length));
    return;
  }
  const scope = scopes.value.find((item) => item.key === value);
  if (!scope) return;
  selectedKey.value = value;
  writeScopeToUrl(scope.scope_kind, scope.scope_id);
}

function authDisplayName(kind: string): string {
  if (kind === "none") return t("无鉴权");
  if (kind === "bearer") return "Bearer";
  if (kind === "x-api-key") return "x-api-key";
  return kind;
}

function resetScopeActions() {
  catalogRefreshError.value = "";
  matrixError.value = "";
  probeError.value = "";
  probeSummary.value = null;
}

async function loadContracts(options: { retain?: boolean; preferDynamicId?: string } = {}): Promise<{ ok: boolean; error: string }> {
  if (loading.value) {
    return { ok: false, error: loadError.value };
  }
  loading.value = true;
  if (!options.retain) loadError.value = "";
  try {
    const [contractsResult, catalogResult] = await Promise.allSettled([
      providersStore.loadContracts(),
      providersStore.loadCatalog(),
    ]);
    if (catalogResult.status === "fulfilled") {
      catalog.value = catalogResult.value;
      const dynamicIds = catalogResult.value.filter(isDynamicCatalogEntry).map((entry) => entry.provider_id);
      const loaded = await Promise.allSettled(dynamicIds.map((id) => providerApi.getDynamicProvider(id)));
      dynamicDetails.value = loaded.flatMap((item) => item.status === "fulfilled" ? [item.value] : []);
    }
    if (contractsResult.status === "fulfilled") {
      contracts.value = normalizeProviderContractsResponse(contractsResult.value);
      loadError.value = "";
      applyScopeFromQuery(true, options.preferDynamicId);
      return { ok: true, error: "" };
    }
    applyScopeFromQuery(true, options.preferDynamicId);
    const error = dashboardErrorDetail(contractsResult.reason);
    loadError.value = error;
    return { ok: false, error };
  } finally {
    loading.value = false;
  }
}

function openCreateDynamic(): void {
  editingDynamic.value = null;
  createPresetId.value = null;
  showDynamicModal.value = true;
}

function openEditDynamic(): void {
  if (!selectedDynamic.value) return;
  editingDynamic.value = selectedDynamic.value;
  createPresetId.value = null;
  showDynamicModal.value = true;
}

async function onDynamicSaved(providerId: string): Promise<void> {
  const created = !editingDynamic.value;
  await loadContracts({ retain: true, preferDynamicId: providerId });
  message.success(created ? t("供应商已创建") : t("供应商已更新"));
}

async function onDynamicConflict(): Promise<void> {
  await loadContracts({ retain: true });
}

async function deleteSelectedDynamic(): Promise<void> {
  const current = selectedDynamic.value;
  if (!current) return;
  try {
    await providerApi.deleteDynamicProvider(current.id);
    message.success(t("供应商已删除"));
    selectedKey.value = scopes.value[0]?.key ?? null;
    await loadContracts({ retain: true });
  } catch (error) {
    if (isRevisionConflict(error) || (error instanceof DashboardRequestError && error.status === 409)) {
      await loadContracts({ retain: true });
      message.warning(t("数据已更新，请检查后重新保存。不会自动重试。"));
      return;
    }
    message.error(t("删除供应商失败: {error}", { error: dashboardErrorDetail(error) }));
  }
}

async function refreshCatalog() {
  const scope = activeScope.value;
  if (!scope || !catalogRefreshVisible.value || catalogRefreshing.value) return;
  catalogRefreshing.value = true;
  catalogRefreshError.value = "";
  try {
    const refreshed = await providersStore.refreshContractCatalog(scope.scope_kind, scope.scope_id);
    contracts.value = normalizeProviderContractsResponse(refreshed);
    applyScopeFromQuery();
    actionLive.value = t("已刷新模型目录");
    message.success(t("已刷新模型目录"));
  } catch (error) {
    catalogRefreshError.value = dashboardErrorDetail(error);
    message.error(t("刷新模型目录失败: {error}", { error: catalogRefreshError.value }));
  } finally {
    catalogRefreshing.value = false;
  }
}

async function resetStaticProtocols() {
  const scope = activeScope.value;
  if (!scope || !staticProtocolResetVisible.value || actionLocked.value) return;
  staticProtocolResetting.value = true;
  matrixError.value = "";
  probeError.value = "";
  try {
    const response = await providersStore.resetStaticModelProtocols(scope.scope_id);
    contracts.value = normalizeProviderContractsResponse(response);
    applyScopeFromQuery();
    actionLive.value = t("已恢复官方协议基线");
    message.success(t("已恢复官方协议基线"));
  } catch (error) {
    matrixError.value = dashboardErrorDetail(error);
    message.error(t("恢复官方协议基线失败: {error}", { error: matrixError.value }));
  } finally {
    staticProtocolResetting.value = false;
  }
}

type OverridePayload = {
  scopeKind: "provider" | "custom_endpoint";
  scopeId: string;
  overrides: ModelProtocolOverrideUpdate[];
};

function overrideKey(payload: OverridePayload, item: ModelProtocolOverrideUpdate): string {
  return modelProtocolOverrideKey(
    payload.scopeKind,
    payload.scopeId,
    item.model_id,
    item.protocol,
  );
}

function showOptimisticOverrides(payload: OverridePayload, sequence: number) {
  const nextOptimistic = new Map(optimisticOverrides.value);
  const nextPending = new Set(pendingOverrideKeys.value);
  for (const item of payload.overrides) {
    const key = overrideKey(payload, item);
    latestOverrideSequence.set(key, sequence);
    // Map the override state to the cell the operator will see before the
    // response lands: `force_on` flips the cell on, `force_off` flips it off.
    // The override builders only emit these two states.
    const optimisticValue = item.state === "force_on";
    nextOptimistic.set(key, optimisticValue);
    nextPending.add(key);
  }
  optimisticOverrides.value = nextOptimistic;
  pendingOverrideKeys.value = nextPending;
}

function settleOptimisticOverrides(payload: OverridePayload, sequence: number) {
  const nextOptimistic = new Map(optimisticOverrides.value);
  const nextPending = new Set(pendingOverrideKeys.value);
  for (const item of payload.overrides) {
    const key = overrideKey(payload, item);
    if (latestOverrideSequence.get(key) !== sequence) continue;
    latestOverrideSequence.delete(key);
    nextOptimistic.delete(key);
    nextPending.delete(key);
  }
  optimisticOverrides.value = nextOptimistic;
  pendingOverrideKeys.value = nextPending;
}

function updateOverrides(payload: OverridePayload) {
  const sequence = ++overrideSequence;
  showOptimisticOverrides(payload, sequence);
  matrixError.value = "";
  overrideQueue = overrideQueue.then(() => persistOverrides(payload, sequence));
}

async function persistOverrides(payload: OverridePayload, sequence: number) {
  try {
    const response = await providersStore.putModelProtocolOverrides(
      payload.scopeKind,
      payload.scopeId,
      payload.overrides,
    );
    contracts.value = normalizeProviderContractsResponse(response);
    actionLive.value = t("协议覆盖已保存");
  } catch (error) {
    if (error instanceof DashboardRequestError && error.status === 409) {
      await loadContracts({ retain: true });
      actionLive.value = t("供应商设置已在其他位置更新，已重新加载，请重试");
      message.warning(t("供应商设置已在其他位置更新，已重新加载，请重试"));
    } else {
      matrixError.value = dashboardErrorDetail(error);
      message.error(t("保存协议覆盖失败: {error}", { error: matrixError.value }));
    }
  } finally {
    settleOptimisticOverrides(payload, sequence);
  }
}

async function runModelProbe(payload: { modelId: string }) {
  const scope = activeScope.value;
  if (!scope || actionLocked.value || probingModels.value.has(payload.modelId)) return;
  // Configured-route test only: the effective preferred protocol, or the first
  // enabled fallback when the preferred one is disabled. Never a blind scan.
  const model = scope.models.find((item) => item.model_id === payload.modelId);
  const protocol = effectiveModelTestProtocol(model);
  if (!protocol) {
    probeError.value = t("该模型没有已开启的协议；请先在矩阵中开启后再测试");
    message.warning(probeError.value);
    return;
  }
  probingModels.value = new Set(probingModels.value).add(payload.modelId);
  probeError.value = "";
  try {
    const response = await providerApi.runProtocolProbes(scope.provider_id, {
      model_id: payload.modelId,
      protocols: [protocol],
    });
    probeSummary.value = probeSummaryFromResponse(response);
    if (response.contract && contracts.value) {
      contracts.value = applyModelContractToResponse(contracts.value, {
        scope_kind: scope.scope_kind,
        scope_id: scope.scope_id,
      }, response.contract);
    }
    const loaded = await loadContracts({ retain: true });
    if (!loaded.ok) {
      probeError.value = loaded.error;
      message.error(t("连接测试失败: {error}", { error: probeError.value }));
      return;
    }
    const failures = response.results.filter((result) => !result.success);
    if (failures.length > 0) {
      actionLive.value = t("连接测试失败");
      message.warning(actionLive.value);
      return;
    }
    actionLive.value = t("连接测试成功");
    message.success(t("连接测试成功"));
  } catch (error) {
    probeError.value = dashboardErrorDetail(error);
    message.error(t("连接测试失败: {error}", { error: probeError.value }));
  } finally {
    const next = new Set(probingModels.value);
    next.delete(payload.modelId);
    probingModels.value = next;
  }
}

function probeSummaryFromResponse(response: ProtocolProbeResponse) {
  return {
    results: response.results,
    hasFailures: response.results.some((result) => !result.success),
  };
}

function probeResultStatus(result: ProtocolProbeResult): string {
  if (result.success) return t("成功");
  if (result.skipped) return t("已跳过");
  return t("失败");
}

function probeErrorValue(error: string | null): { raw: string; parsed: unknown } | null {
  if (!error?.trim()) return null;
  const raw = error.trim();
  const objectStart = raw.indexOf("{");
  try {
    return { raw, parsed: JSON.parse(objectStart >= 0 ? raw.slice(objectStart) : raw) as unknown };
  } catch {
    return { raw, parsed: null };
  }
}

function nestedErrorMessage(value: unknown): string | null {
  if (typeof value === "string") return value.trim() || null;
  if (!value || typeof value !== "object") return null;
  const record = value as Record<string, unknown>;
  for (const candidate of [record.message, record.error]) {
    const message = nestedErrorMessage(candidate);
    if (message) return message;
  }
  return null;
}

function probeResultMessage(error: string | null): string {
  const value = probeErrorValue(error);
  return nestedErrorMessage(value?.parsed) ?? value?.raw ?? "";
}

function probeResultHttpStatus(error: string | null): string {
  const match = error?.match(/\b(?:HTTP\s+|returned\s+)(\d{3})\b/i);
  return match?.[1] ?? "";
}

function findSafeHttpUrl(value: unknown): string | null {
  if (typeof value === "string") {
    const match = value.match(/https?:\/\/[^\s"'<>]+/i);
    return match && isSafeSourceUrl(match[0]) ? match[0] : null;
  }
  if (!value || typeof value !== "object") return null;
  for (const item of Object.values(value as Record<string, unknown>)) {
    const url = findSafeHttpUrl(item);
    if (url) return url;
  }
  return null;
}

function probeResultUrl(error: string | null): string {
  const value = probeErrorValue(error);
  return findSafeHttpUrl(value?.parsed) ?? findSafeHttpUrl(value?.raw) ?? "";
}

function onPopState() {
  // KeepAlive keeps this view mounted; a popstate for another view (e.g. the
  // Accounts add deep link) is not ours to apply.
  if (!currentUrlIsProvidersView()) return;
  applyScopeFromQuery();
}

watch(selectedKey, () => {
  // The inline preset form unmounts on selection change; its busy flags die
  // with it, so the navigation lock must not outlive the form.
  inlineFormBusy.value = false;
});

watch(activeScope, (scope, previous) => {
  if (scope?.key !== previous?.key) {
    resetScopeActions();
    if (scope?.provider_id !== DEFAULT_PROVIDER_ID && activeTab.value === PROVIDER_OTHER_TAB) {
      activeTab.value = "catalog";
    }
  }
});

watch(activeTab, () => {
  const scope = activeScope.value;
  if (!scope) return;
  writeScopeToUrl(scope.scope_kind, scope.scope_id);
});

onMounted(() => {
  window.addEventListener("popstate", onPopState);
  void loadContracts();
});
onActivated(() => {
  if (activatedOnce) void loadContracts({ retain: true });
  else activatedOnce = true;
});
onUnmounted(() => {
  window.removeEventListener("popstate", onPopState);
});
</script>

<style scoped>
.providers-page {
  min-width: 0;
  max-width: 1440px;
  margin: 0 auto;
  overflow-x: hidden;
}
.providers-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
}
.providers-header h1 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-xl)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.dynamic-provider-facts {
  display: grid;
  gap: 8px 16px;
  margin: 0 0 16px;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
}
.dynamic-provider-facts dt {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}
.dynamic-provider-facts dd {
  margin: 0;
}
.dynamic-provider-facts dd code {
  overflow-wrap: anywhere;
}

.providers-alias-hint {
  margin: 4px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

.providers-alias-table-wrap {
  overflow-x: auto;
}

.providers-alias-table {
  width: 100%;
  min-width: 760px;
  border-collapse: collapse;
  font-size: var(--ocg-font-sm);
}

.providers-alias-table th,
.providers-alias-table td {
  padding: 10px 12px;
  border-bottom: 1px solid var(--ocg-border);
  text-align: left;
  vertical-align: middle;
}

.providers-alias-table th {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
  font-weight: 600;
}

.providers-alias-table .providers-alias-name {
  vertical-align: top;
}
.providers-state {
  min-height: 160px;
  display: grid;
  place-items: center;
}
.providers-layout {
  display: grid;
  grid-template-columns: 208px minmax(0, 1fr);
  gap: 16px;
  min-width: 0;
}
.providers-probe-summary {
  margin: 12px 0;
}
.providers-probe-result {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: baseline;
  margin-top: 4px;
}
.providers-catalog-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
}
.providers-rail {
  display: flex;
  flex-direction: column;
  min-width: 0;
  max-height: calc(100vh - 140px);
  padding: 8px 0;
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-surface);
}
.providers-rail-search {
  flex: none;
  padding: 0 8px 8px;
}
/* One list scrolls; the Plan / API group labels stick to its top edge instead
   of splitting the rail into two independently scrolling half-height panes. */
.providers-rail-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.providers-rail-pane + .providers-rail-pane {
  border-top: 1px solid var(--ocg-border);
}
.providers-rail-pane__label {
  position: sticky;
  top: 0;
  z-index: 1;
  margin: 0;
  padding: 4px 12px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
  font-weight: 600;
  line-height: 1.3;
  background: var(--ocg-surface);
}
.providers-rail-footer {
  flex: none;
  padding: 8px;
  border-top: 1px solid var(--ocg-border);
}
.providers-rail-empty {
  margin: 0;
  padding: 8px 12px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}
.providers-mobile-nav {
  display: none;
  min-width: 0;
  margin-bottom: 12px;
}
.providers-main {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 16px;
  min-width: 0;
}
.providers-tabs {
  min-width: 0;
  max-width: 100%;
}
.providers-tabs :deep(.n-tabs-nav) {
  margin-bottom: 12px;
}
.providers-section {
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.providers-section h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.providers-catalog-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--ocg-border);
}
.providers-catalog-heading {
  min-width: 0;
}
.providers-catalog-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  margin-top: 4px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
}

/* Vendor family row label (parent) and variant row label (child). The
   family label keeps the same single-line look as the flat preset row did
   so single-variant families blend in. The child variant stacks a short
   label and a muted monospaced endpoint host, matching the Accounts dialog
   variant picker. */
.providers-rail-preset {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  min-width: 0;
  line-height: 1.25;
}
.providers-rail-preset__variant {
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
  font-weight: 500;
}
.providers-rail-preset__host {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-variant-numeric: tabular-nums;
  word-break: break-all;
}
/* n-menu's own item content sets an inherited color; the deep selector
   wins over the n-menu color when both apply so the host stays muted. */
.providers-rail-list :deep(.n-menu-item-content .providers-rail-preset__host) {
  color: var(--ocg-muted);
}

@media (max-width: 720px) {
  .providers-layout {
    grid-template-columns: minmax(0, 1fr);
  }
  .providers-rail {
    display: none;
  }
  .providers-mobile-nav {
    display: block;
  }
  .providers-catalog-head {
    align-items: stretch;
    flex-direction: column;
  }
}

@media (max-width: 390px) {
  .providers-page,
  .providers-layout,
  .providers-main,
  .providers-section {
    min-width: 0;
  }
}
</style>
