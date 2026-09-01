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
      v-else-if="!loading && scopes.length === 0 && dynamicEntries.length === 0"
      :description="t('暂无供应商范围')"
    />

    <div v-else-if="activeScope || selectedDynamic" class="providers-layout">
      <aside class="providers-rail">
        <n-menu
          :value="selectedKey"
          :options="scopeMenuOptions"
          :aria-label="t('选择供应商范围')"
          @update:value="selectScopeKey"
        />
      </aside>

      <div class="providers-main">
        <div class="providers-mobile-nav">
          <n-select
            :value="selectedKey"
            :options="scopeSelectOptions"
            :aria-label="t('选择供应商范围')"
            :disabled="actionLocked"
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
        </section>

        <n-tabs v-else-if="activeScope" v-model:value="activeTab" class="providers-tabs" display-directive="if">
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
                :title="probeSummary.hasFailures ? t('测试完成，部分协议失败') : t('测试完成')"
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
                :title="t('探测失败: {error}', { error: probeError })"
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
        </n-tabs>
      </div>
    </div>

    <DynamicProviderModal
      v-model:show="showDynamicModal"
      :provider="editingDynamic"
      @saved="onDynamicSaved"
      @conflict="onDynamicConflict"
    />
    <span class="sr-only" aria-live="polite" aria-atomic="true">{{ actionLive }}</span>
  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onMounted, onUnmounted, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NEmpty,
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
import { locale, t } from "../i18n/index.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";
import { applyAppViewSearchParams, readProviderScopeQuery } from "./app-navigation.ts";
import {
  applyModelContractToResponse,
  catalogRefreshSupported,
  flattenProviderScopes,
  isSafeSourceUrl,
  modelProtocolOverrideKey,
  normalizeProviderContractsResponse,
  protocolDisplayName,
  PROVIDER_PROTOCOLS,
  selectProviderScope,
} from "../domain/provider-contracts.ts";
import { isDynamicCatalogEntry } from "../domain/dynamic-provider.ts";
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
const activeSelection = computed(() => {
  const query = selectedKey.value?.split(":") ?? [];
  const scopeKind = query[0] ?? null;
  const scopeId = query.length > 1 ? query.slice(1).join(":") : null;
  return selectProviderScope(scopes.value, scopeKind, scopeId);
});
const activeScope = computed(() => activeSelection.value.scope);
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
const scopeMenuOptions = computed<MenuOption[]>(() => {
  const builtin: MenuOption = {
    type: "group",
    label: t("内置"),
    key: "builtin",
    children: scopes.value.map((scope) => ({ key: scope.key, label: `${scope.label}` })),
  };
  const userDefined: MenuOption = {
    type: "group",
    label: t("用户定义"),
    key: "user-defined",
    children: dynamicEntries.value.map((entry) => ({
      key: `dynamic:${entry.provider_id}`,
      label: entry.display_name,
    })),
  };
  return [
    ...(scopes.value.length ? [builtin] : []),
    ...(dynamicEntries.value.length ? [userDefined] : []),
  ];
});
const scopeSelectOptions = computed<SelectOption[]>(() => [
  ...scopes.value.map((scope) => ({ value: scope.key, label: `${scope.label} · ${t("内置")}` })),
  ...dynamicEntries.value.map((entry) => ({
    value: `dynamic:${entry.provider_id}`,
    label: `${entry.display_name} · ${t("用户定义")}`,
  })),
]);
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

function writeScopeToUrl(scopeKind: string, scopeId: string) {
  const url = applyAppViewSearchParams(new URL(window.location.href), "providers", {
    scope_kind: scopeKind,
    scope_id: scopeId,
  });
  window.history.replaceState(null, "", url);
}

function selectDynamicProvider(providerId: string): boolean {
  if (!dynamicEntries.value.some((entry) => entry.provider_id === providerId)) return false;
  selectedKey.value = `dynamic:${providerId}`;
  writeScopeToUrl("dynamic", providerId);
  return true;
}

function applyScopeFromQuery(fellBackNotice = false, preferDynamicId?: string) {
  if (preferDynamicId && selectDynamicProvider(preferDynamicId)) return;
  if (selectedKey.value?.startsWith("dynamic:")) {
    const id = selectedKey.value.slice("dynamic:".length);
    if (selectDynamicProvider(id)) return;
  }
  const query = readProviderScopeQuery(window.location.search);
  if (query.scope_kind === "dynamic" && query.scope_id && selectDynamicProvider(query.scope_id)) {
    return;
  }
  const selected = selectProviderScope(scopes.value, query.scope_kind, query.scope_id);
  if (!selected.scope) {
    selectedKey.value = null;
    return;
  }
  selectedKey.value = selected.scope.key;
  writeScopeToUrl(selected.scope.scope_kind, selected.scope.scope_id);
  if (fellBackNotice && selected.fellBack) {
    actionLive.value = t("已选择过期范围，已回到第一个供应商");
  }
}

function selectScopeKey(key: string | number) {
  const value = String(key);
  if (value.startsWith("dynamic:")) {
    selectDynamicProvider(value.slice("dynamic:".length));
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
  showDynamicModal.value = true;
}

function openEditDynamic(): void {
  if (!selectedDynamic.value) return;
  editingDynamic.value = selectedDynamic.value;
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
    nextOptimistic.set(key, item.state === "force_on");
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
  probingModels.value = new Set(probingModels.value).add(payload.modelId);
  probeError.value = "";
  try {
    const response = await providerApi.runProtocolProbes(scope.provider_id, {
      model_id: payload.modelId,
      protocols: [...PROVIDER_PROTOCOLS],
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
      message.error(t("探测失败: {error}", { error: probeError.value }));
      return;
    }
    const failures = response.results.filter((result) => !result.success);
    if (failures.length > 0) {
      actionLive.value = t("测试完成，部分协议失败");
      message.warning(actionLive.value);
      return;
    }
    actionLive.value = t("探测完成");
    message.success(t("探测完成"));
  } catch (error) {
    probeError.value = dashboardErrorDetail(error);
    message.error(t("探测失败: {error}", { error: probeError.value }));
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
  applyScopeFromQuery();
}

watch(activeScope, (scope, previous) => {
  if (scope?.key !== previous?.key) resetScopeActions();
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
  min-width: 0;
  padding: 8px 0;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-surface);
}
.providers-mobile-nav {
  display: none;
  min-width: 0;
  margin-bottom: 12px;
}
.providers-main {
  display: grid;
  gap: 16px;
  min-width: 0;
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
