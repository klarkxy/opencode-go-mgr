<template>
  <div class="provider-model-matrix">
    <div v-if="providerDisabled" class="matrix-status" role="status">
      <n-tag type="warning" size="small" :bordered="false">
        {{ t("全部供应商协议已关闭") }}
      </n-tag>
    </div>
    <div class="matrix-scroll">
      <table class="matrix-table">
        <thead>
          <tr>
            <th class="matrix-cell matrix-cell--model-header">{{ t("模型") }}</th>
            <th
              v-for="protocol in matrixProtocols"
              :key="protocol"
              class="matrix-cell matrix-cell--protocol-header"
            >
              <div class="protocol-header">
                <span>{{ protocolDisplayName(protocol) }}</span>
                <n-dropdown
                  :options="columnBatchOptions"
                  trigger="click"
                  @select="(key) => applyColumnBatch(protocol, String(key) as ProtocolOverrideState)"
                >
                  <n-button
                    text
                    size="tiny"
                    :disabled="props.actionLocked || columnSaving(protocol) || !columnControllable(protocol)"
                    :aria-label="t('本列全部')"
                  >
                    <template #icon>
                      <n-icon :component="MoreOutlined" />
                    </template>
                  </n-button>
                </n-dropdown>
              </div>
            </th>
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
            <td
              v-for="protocol in matrixProtocols"
              :key="protocol"
              class="matrix-cell matrix-cell--state"
            >
              <n-switch
                class="matrix-switch"
                size="small"
                :value="cellEnabled(modelId, protocol)"
                :loading="cellSaving(modelId, protocol)"
                :disabled="props.actionLocked || rowProbing(modelId) || !cellControllable(modelId, protocol)"
                :aria-label="`${modelId} ${protocolDisplayName(protocol)}`"
                @update:value="(on) => updateSingle(modelId, protocol, on ? 'force_on' : 'force_off')"
              />
            </td>
            <td v-if="probeSupported" class="matrix-cell matrix-cell--actions">
              <n-popconfirm
                @positive-click="runRowProbe(modelId)"
              >
                <template #trigger>
                  <n-button
                    text
                    size="tiny"
                    :loading="rowProbing(modelId)"
                    :disabled="props.actionLocked || overridesSaving() || rowProbing(modelId)"
                    :aria-label="t('测试')"
                  >
                    <template #icon>
                      <n-icon :component="ReloadOutlined" />
                    </template>
                  </n-button>
                </template>
                {{ t("探测会向上游发送真实最小请求，可能消耗额度。是否继续？") }}
              </n-popconfirm>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  NButton,
  NDropdown,
  NIcon,
  NPopconfirm,
  NSwitch,
  NTag,
} from "naive-ui";
import { MoreOutlined, ReloadOutlined } from "@vicons/antd";
import type { DropdownOption } from "naive-ui";
import type {
  ContractScopeKind,
  EffectiveProtocolEvidence,
  ModelProtocolOverrideUpdate,
  ProviderProtocol,
  ProtocolOverrideState,
} from "../api/providers.ts";
import type { ProviderScopeView } from "../domain/provider-contracts.ts";
import { t } from "../i18n/index.ts";
import {
  modelProtocolOverrideKey,
  protocolDisplayName,
  PROVIDER_PROTOCOLS,
} from "../domain/provider-contracts.ts";

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

const matrixModels = computed(() => {
  return [...new Set(props.scope.catalog.models)].sort();
});

const matrixProtocols = computed<ProviderProtocol[]>(() => {
  if (props.scope.scope_kind !== "custom_endpoint") {
    return [...PROVIDER_PROTOCOLS];
  }
  return PROVIDER_PROTOCOLS.filter((protocol) => (
    props.scope.models.some((model) => model.protocols[protocol]?.available === true)
  ));
});

const probeSupported = computed(() => props.scope.card.protocol_probe);

function modelContract(modelId: string): ProviderScopeView["models"][number] | undefined {
  return props.scope.models.find((model) => model.model_id === modelId);
}

function modelAlias(modelId: string): string {
  return modelContract(modelId)?.alias?.trim() ?? "";
}

function cellEvidence(modelId: string, protocol: ProviderProtocol): EffectiveProtocolEvidence | undefined {
  return modelContract(modelId)?.protocols[protocol];
}

function cellEnabled(modelId: string, protocol: ProviderProtocol): boolean {
  const optimistic = props.optimisticOverrides?.get(cellKey(modelId, protocol));
  if (optimistic !== undefined) return optimistic;
  return cellEvidence(modelId, protocol)?.enabled === true;
}

function cellControllable(modelId: string, protocol: ProviderProtocol): boolean {
  return cellEvidence(modelId, protocol) !== undefined;
}

function cellKey(modelId: string, protocol: ProviderProtocol): string {
  return modelProtocolOverrideKey(
    props.scope.scope_kind,
    props.scope.scope_id,
    modelId,
    protocol,
  );
}

function cellSaving(modelId: string, protocol: ProviderProtocol): boolean {
  return props.pendingOverrideKeys?.has(cellKey(modelId, protocol)) ?? false;
}

function columnSaving(protocol: ProviderProtocol): boolean {
  return matrixModels.value.some((modelId) => cellSaving(modelId, protocol));
}

function columnControllable(protocol: ProviderProtocol): boolean {
  return matrixModels.value.some((modelId) => cellControllable(modelId, protocol));
}

function overridesSaving(): boolean {
  return (props.pendingOverrideKeys?.size ?? 0) > 0;
}

function rowProbing(modelId: string): boolean {
  return props.probingModels?.has(modelId) ?? false;
}

const providerDisabled = computed(() => (
  matrixModels.value.length > 0
  && !matrixModels.value.some((modelId) => (
    matrixProtocols.value.some((protocol) => cellEnabled(modelId, protocol))
  ))
));

function makeOverrides(
  modelIds: string[],
  protocols: ProviderProtocol[],
  state: ProtocolOverrideState,
): ModelProtocolOverrideUpdate[] {
  const overrides: ModelProtocolOverrideUpdate[] = [];
  for (const modelId of modelIds) {
    for (const protocol of protocols) {
      if (!cellControllable(modelId, protocol)) continue;
      overrides.push({ model_id: modelId, protocol, state });
    }
  }
  return overrides;
}

function emitOverrides(overrides: ModelProtocolOverrideUpdate[]): void {
  if (overrides.length === 0) return;
  emit("update:overrides", {
    scopeKind: props.scope.scope_kind,
    scopeId: props.scope.scope_id,
    overrides,
  });
}

function updateSingle(
  modelId: string,
  protocol: ProviderProtocol,
  state: ProtocolOverrideState,
): void {
  emitOverrides([{ model_id: modelId, protocol, state }]);
}

function applyColumnBatch(protocol: ProviderProtocol, state: ProtocolOverrideState): void {
  emitOverrides(makeOverrides(matrixModels.value, [protocol], state));
}

function runRowProbe(modelId: string): void {
  if (!probeSupported.value) return;
  emit("probe", { modelId });
}

const columnBatchOptions: DropdownOption[] = [
  { key: "force_on", label: t("全部开启") },
  { key: "force_off", label: t("全部关闭") },
];
</script>

<style scoped>
.provider-model-matrix {
  min-width: 0;
}
.matrix-scroll {
  overflow-x: auto;
}
.matrix-table {
  width: 100%;
  min-width: 720px;
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
  min-width: 180px;
  max-width: 260px;
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
.matrix-cell--state {
  min-width: 120px;
}
.matrix-cell--actions {
  width: 64px;
  white-space: nowrap;
}
.matrix-switch {
  --n-rail-color-active: var(--ocg-success);
}
.protocol-header {
  display: flex;
  align-items: center;
  gap: 8px;
}
.matrix-status {
  margin-bottom: 8px;
}
</style>
