<template>
  <div class="provider-model-matrix">
    <div v-if="providerDisabled" class="matrix-status" role="status">
      <n-tag type="warning" size="small" :bordered="false">
        {{ t("全部供应商协议已关闭") }}
      </n-tag>
    </div>
    <div class="matrix-toolbar" v-if="allMatrixModels.length > 0">
      <n-input
        v-model:value="modelQuery"
        size="small"
        clearable
        class="matrix-search"
        :placeholder="t('搜索模型名或别名')"
        :input-props="{ 'aria-label': t('搜索模型名或别名') }"
      />
      <label class="matrix-enabled-filter">
        <n-switch size="small" v-model:value="enabledOnly" :aria-label="t('仅看已启用')" />
        <span>{{ t("仅看已启用") }}</span>
      </label>
      <span class="matrix-toolbar__label">{{ t("批量") }}</span>
      <n-button
        text
        size="tiny"
        :disabled="!canBatch || batchSaving || props.actionLocked"
        :loading="batchSaving"
        @click="applyBatch(true)"
      >
        {{ t("全部开启") }}
      </n-button>
      <n-button
        text
        size="tiny"
        :disabled="!canBatch || batchSaving || props.actionLocked"
        :loading="batchSaving"
        @click="applyBatch(false)"
      >
        {{ t("全部关闭") }}
      </n-button>
    </div>
    <p v-if="allMatrixModels.length > 0 && matrixModels.length === 0" class="matrix-empty" role="status">
      {{ t("无匹配模型") }}
    </p>
    <div class="matrix-scroll">
      <table class="matrix-table">
        <thead>
          <tr>
            <th class="matrix-cell matrix-cell--model-header">{{ t("模型") }}</th>
            <th class="matrix-cell matrix-cell--protocol-header">{{ t("上游协议") }}</th>
            <th class="matrix-cell matrix-cell--state-header">{{ t("允许路由") }}</th>
            <th v-if="probeSupported" class="matrix-cell matrix-cell--actions-header">
              {{ t("操作") }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="modelId in matrixModels" :key="modelId">
            <td class="matrix-cell matrix-cell--model">
              <code>{{ modelAlias(modelId) || modelId }}</code>
              <code
                v-if="modelAlias(modelId) && modelAlias(modelId) !== modelId"
                class="matrix-model-id"
              >{{ modelId }}</code>
            </td>
            <td class="matrix-cell matrix-cell--protocol">
              <n-radio-group
                v-if="isTwoProtocolScope && rowTarget(modelId)"
                :value="rowTarget(modelId) ?? undefined"
                size="small"
                type="button"
                :disabled="props.actionLocked || rowProbing(modelId) || rowSaving(modelId)"
                :aria-label="`${modelId} ${t('协议选择')}`"
                @update:value="(protocol: ProviderProtocol) => switchRowProtocol(modelId, protocol)"
              >
                <n-radio-button
                  v-for="choice in protocolChoices"
                  :key="choice"
                  :value="choice"
                >
                  {{ protocolDisplayName(choice) }}
                </n-radio-button>
              </n-radio-group>
              <span
                v-else-if="rowTarget(modelId)"
                class="matrix-protocol-label"
              >{{ protocolDisplayName(rowTarget(modelId)!) }}</span>
              <span v-else class="matrix-protocol-label matrix-protocol-label--muted">
                {{ t("无可用协议") }}
              </span>
            </td>
            <td class="matrix-cell matrix-cell--state">
              <n-switch
                class="matrix-switch"
                size="small"
                :value="rowEnabled(modelId)"
                :loading="rowSaving(modelId) && !rowProbing(modelId)"
                :disabled="props.actionLocked || rowProbing(modelId) || !rowControllable(modelId)"
                :aria-label="`${modelId} ${t('允许路由')}`"
                @update:value="(on: boolean) => toggleRow(modelId, on)"
              />
            </td>
            <td v-if="probeSupported" class="matrix-cell matrix-cell--actions">
              <n-popconfirm
                @positive-click="runRowProbe(modelId)"
              >
                <template #trigger>
                  <n-tooltip trigger="hover">
                    <template #trigger>
                      <n-button
                        text
                        size="tiny"
                        :loading="rowProbing(modelId)"
                        :disabled="props.actionLocked || rowSaving(modelId) || rowProbing(modelId)"
                        :aria-label="t('测试 {model}', { model: modelId })"
                      >
                        <template #icon>
                          <n-icon :component="ApiOutlined" />
                        </template>
                      </n-button>
                    </template>
                    {{ t("测试 {model}", { model: modelId }) }}
                  </n-tooltip>
                </template>
                {{ t("将按当前生效的协议发送一次最小真实请求测试连接，可能消耗额度；测试只作观测，不会开启路由。是否继续？") }}
              </n-popconfirm>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import {
  NButton,
  NIcon,
  NInput,
  NPopconfirm,
  NRadioButton,
  NRadioGroup,
  NSwitch,
  NTag,
  NTooltip,
} from "naive-ui";
import { ApiOutlined } from "@vicons/antd";
import type {
  ContractScopeKind,
  ModelProtocolOverrideUpdate,
  ProviderProtocol,
} from "../api/providers.ts";
import {
  buildModelProtocolSwitchOverrides,
  buildModelToggleOverrides,
  modelEffectiveOn,
  modelProtocolOverrideKey,
  modelTargetProtocol,
  protocolDisplayName,
  PROVIDER_PROTOCOLS,
  scopeProtocolChoices,
  type ProviderScopeView,
} from "../domain/provider-contracts.ts";
import { CPA_PROVIDER_ID } from "../domain/account-providers.ts";
import { t } from "../i18n/index.ts";

const props = defineProps<{
  scope: ProviderScopeView;
  optimisticOverrides?: Map<string, boolean>;
  pendingOverrideKeys?: Set<string>;
  probingModels?: Set<string>;
  actionLocked?: boolean;
}>();

const emit = defineEmits<{
  (
    e: "update:overrides",
    payload: {
      scopeKind: ContractScopeKind;
      scopeId: string;
      overrides: ModelProtocolOverrideUpdate[];
    },
  ): void;
  (e: "probe", payload: { modelId: string }): void;
  (e: "error", message: string): void;
}>();

const modelQuery = ref("");
const enabledOnly = ref(false);

const allMatrixModels = computed(() => {
  return [...new Set(props.scope.catalog.models)].sort();
});

// Search matches the raw model id and the published alias; the enabled
// filter reads the same effective-on state as the row switch. Filtering only
// narrows the visible rows — it never changes configuration.
const matrixModels = computed(() => {
  const needle = modelQuery.value.trim().toLocaleLowerCase();
  return allMatrixModels.value.filter((modelId) => {
    if (enabledOnly.value && !rowEnabled(modelId)) return false;
    if (!needle) return true;
    return modelId.toLocaleLowerCase().includes(needle)
      || modelAlias(modelId).toLocaleLowerCase().includes(needle);
  });
});

const protocolChoices = computed<ProviderProtocol[]>(() => scopeProtocolChoices(props.scope));
const isTwoProtocolScope = computed(() => protocolChoices.value.length === 2);

// CPA is a separate static external integration: it never gets a scan/test
// column here even if a backend card flag claims probe support.
const probeSupported = computed(() => (
  props.scope.card.protocol_probe && props.scope.provider_id !== CPA_PROVIDER_ID
));

function modelContract(modelId: string): ProviderScopeView["models"][number] | undefined {
  return props.scope.models.find((model) => model.model_id === modelId);
}

function modelAlias(modelId: string): string {
  return modelContract(modelId)?.alias?.trim() ?? "";
}

function rowTarget(modelId: string): ProviderProtocol | null {
  const model = modelContract(modelId);
  if (!model) return null;
  return modelTargetProtocol(model, props.scope);
}

function rowEnabled(modelId: string): boolean {
  const model = modelContract(modelId);
  if (!model) return false;
  const target = modelTargetProtocol(model, props.scope);
  if (target === null) return false;
  // The Providers view stores the per-cell optimistic value keyed by
  // (scope, model, protocol); a row's intended state lives at the target
  // protocol's key, so read it first and only fall back to the live
  // contract when no override is in flight.
  const key = cellKey(modelId, target);
  const optimistic = props.optimisticOverrides?.get(key);
  if (optimistic !== undefined) return optimistic;
  return modelEffectiveOn(model, props.scope);
}

function rowControllable(modelId: string): boolean {
  const model = modelContract(modelId);
  if (!model) return false;
  return modelTargetProtocol(model, props.scope) !== null;
}

function cellKey(modelId: string, protocol: ProviderProtocol): string {
  return modelProtocolOverrideKey(
    props.scope.scope_kind,
    props.scope.scope_id,
    modelId,
    protocol,
  );
}

function rowKeys(modelId: string): string[] {
  return PROVIDER_PROTOCOLS.map((protocol) => cellKey(modelId, protocol));
}

function rowProbing(modelId: string): boolean {
  return props.probingModels?.has(modelId) ?? false;
}

function rowSaving(modelId: string): boolean {
  const pending = props.pendingOverrideKeys;
  if (!pending) return false;
  return rowKeys(modelId).some((key) => pending.has(key));
}

const canBatch = computed(() => matrixModels.value.length > 0);
const batchSaving = computed(() => {
  const pending = props.pendingOverrideKeys;
  if (!pending || pending.size === 0) return false;
  // The batch toggle writes to all three protocol slots per model, so any
  // pending override key across the scope's models means a batch is in flight.
  return allMatrixModels.value.some((modelId) => rowSaving(modelId));
});

const providerDisabled = computed(() => {
  if (allMatrixModels.value.length === 0) return false;
  return !allMatrixModels.value.some((modelId) => rowEnabled(modelId));
});

function emitOverrides(overrides: ModelProtocolOverrideUpdate[]): void {
  if (overrides.length === 0) return;
  emit("update:overrides", {
    scopeKind: props.scope.scope_kind,
    scopeId: props.scope.scope_id,
    overrides,
  });
}

function toggleRow(modelId: string, on: boolean): void {
  emitOverrides(buildModelToggleOverrides(props.scope, [modelId], on));
}

function switchRowProtocol(modelId: string, protocol: ProviderProtocol): void {
  emitOverrides(buildModelProtocolSwitchOverrides(props.scope, modelId, protocol));
}

function applyBatch(on: boolean): void {
  if (!canBatch.value) return;
  emitOverrides(buildModelToggleOverrides(props.scope, matrixModels.value, on));
}

function runRowProbe(modelId: string): void {
  if (!probeSupported.value) return;
  emit("probe", { modelId });
}
</script>

<style scoped>
.provider-model-matrix {
  min-width: 0;
}
.matrix-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}
.matrix-search {
  max-width: 240px;
}
.matrix-enabled-filter {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}
.matrix-empty {
  margin: 0 0 8px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}
.matrix-toolbar__label {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}
.matrix-scroll {
  overflow-x: auto;
}
.matrix-table {
  width: 100%;
  min-width: 560px;
  border-collapse: collapse;
  font-size: var(--ocg-font-sm);
}
.matrix-cell {
  padding: 10px 12px;
  border-bottom: 1px solid var(--ocg-divider);
  text-align: left;
  vertical-align: middle;
}
.matrix-cell--model-header,
.matrix-cell--protocol-header,
.matrix-cell--state-header,
.matrix-cell--actions-header {
  position: sticky;
  top: 0;
  z-index: 1;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
  font-weight: 600;
  background: var(--ocg-surface);
}
.matrix-cell--model {
  min-width: 200px;
  max-width: 320px;
}
.matrix-cell--model code {
  display: block;
  overflow-wrap: anywhere;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
}
.matrix-cell--model .matrix-model-id {
  margin-top: 2px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}
.matrix-cell--protocol {
  min-width: 200px;
}
.matrix-protocol-label {
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
}
.matrix-protocol-label--muted {
  color: var(--ocg-muted);
}
.matrix-cell--state {
  width: 88px;
}
.matrix-cell--actions {
  width: 64px;
  white-space: nowrap;
}
.matrix-switch {
  --n-rail-color-active: var(--ocg-primary);
}
.matrix-status {
  margin-bottom: 8px;
}
</style>
