<template>
  <n-modal
    :show="show"
    preset="card"
    :title="isEdit ? t('编辑供应商') : t('新建供应商')"
    class="dynamic-provider-modal"
    style="width: 720px; max-width: calc(100vw - 32px)"
    :mask-closable="false"
    @update:show="$emit('update:show', $event)"
  >
    <n-form label-placement="top" @submit.prevent="save">
      <n-alert v-if="formError" type="error" class="form-error" role="alert">
        {{ formError }}
      </n-alert>
      <n-alert v-if="conflictNotice" type="warning" class="form-error" role="alert">
        {{ conflictNotice }}
      </n-alert>
      <n-alert type="default" :show-icon="false" class="form-error">
        {{ t("用户定义供应商绑定内置 Configurable HTTP 适配器；价格和官方用量始终未知。") }}
      </n-alert>

      <div class="modal-grid">
        <n-form-item :label="t('名称')" path="name">
          <n-input
            v-model:value="draft.name"
            :input-props="{ 'aria-label': t('名称') }"
            :placeholder="t('例如：主号')"
          />
        </n-form-item>
        <n-form-item :label="t('鉴权方式')">
          <n-select
            v-model:value="draft.auth_kind"
            :options="authOptions"
            :aria-label="t('鉴权方式')"
          />
        </n-form-item>
        <n-form-item :label="t('API 地址')" class="full-width-field">
          <n-input
            v-model:value="draft.endpoint_url"
            :input-props="{ 'aria-label': t('API 地址') }"
            :placeholder="t('推荐填写不带 /v1 的 API 根地址；OCG 会自动补全 /v1 和协议路径。已带 /v1 时不会重复添加。')"
          />
        </n-form-item>
        <n-form-item :label="t('上游协议')">
          <n-select
            v-model:value="draft.upstream_protocol"
            :options="protocolOptions"
            :aria-label="t('上游协议')"
          />
        </n-form-item>
        <n-form-item v-if="!isEdit" :label="t('第一个账号名称')">
          <n-input
            v-model:value="draft.account_name"
            :input-props="{ 'aria-label': t('第一个账号名称') }"
          />
        </n-form-item>
        <n-form-item
          v-if="showKeyField"
          :label="t('API Key')"
          class="full-width-field"
        >
          <n-input
            v-model:value="draft.key"
            type="password"
            show-password-on="click"
            :input-props="{ 'aria-label': t('API Key') }"
            :placeholder="isEdit ? t('Key 只会在保存或测试时发送，不会重新显示。') : 'sk-...'"
          />
        </n-form-item>
        <n-form-item v-if="!isEdit" :label="t('备注')" class="full-width-field">
          <n-input
            v-model:value="draft.notes"
            type="textarea"
            :autosize="{ minRows: 2, maxRows: 6 }"
            :input-props="{ 'aria-label': t('备注') }"
          />
        </n-form-item>
        <n-form-item :label="t('模型映射')" class="full-width-field">
          <div class="capability-rows">
            <div class="capability-actions">
              <n-button
                attr-type="button"
                size="small"
                secondary
                :loading="discovering"
                :disabled="busy"
                @click="discover"
              >
                {{ t("获取模型") }}
              </n-button>
              <n-button attr-type="button" size="small" secondary :disabled="busy" @click="addMapping">
                {{ t("添加映射") }}
              </n-button>
            </div>
            <p class="field-hint">{{ t("对外模型名不区分大小写且必须唯一；上游模型 ID 可复用。") }}</p>
            <n-alert v-if="discoveryError" type="error" :show-icon="false">{{ discoveryError }}</n-alert>
            <div v-for="(row, index) in draft.models" :key="index" class="mapping-row">
              <n-input
                v-model:value="row.public_model"
                :placeholder="t('对外模型名')"
                :input-props="{ 'aria-label': t('对外模型名') }"
              />
              <n-input
                v-model:value="row.upstream_model"
                :placeholder="t('上游模型 ID')"
                :input-props="{ 'aria-label': t('上游模型 ID') }"
              />
              <n-button attr-type="button" quaternary :disabled="draft.models.length < 2" @click="removeMapping(index)">
                {{ t("删除映射") }}
              </n-button>
            </div>
            <div v-if="discoveredModels.length" class="discovery-import">
              <n-select
                v-model:value="selectedDiscovery"
                multiple
                :options="discoveredModels.map((model) => ({ label: model, value: model }))"
                :placeholder="t('选择要导入的模型')"
                :aria-label="t('选择要导入的模型')"
              />
              <n-button attr-type="button" size="small" @click="importDiscovered">{{ t("导入所选") }}</n-button>
            </div>
          </div>
        </n-form-item>
      </div>
    </n-form>
    <template #footer>
      <div class="modal-footer">
        <n-popconfirm
          v-if="testNeedsConfirm"
          :positive-text="t('测试模型')"
          :negative-text="t('取消')"
          @positive-click="runTest"
        >
          <template #trigger>
            <n-button attr-type="button" secondary :loading="testing" :disabled="busy">
              {{ t("测试模型") }}
            </n-button>
          </template>
          {{ t(paidTestWarningKey) }}
        </n-popconfirm>
        <n-space>
          <n-button attr-type="button" :disabled="busy" @click="$emit('update:show', false)">{{ t("取消") }}</n-button>
          <n-button type="primary" attr-type="submit" :loading="saving" :disabled="busy" @click="save">
            {{ saving ? t("正在保存…") : t("保存供应商") }}
          </n-button>
        </n-space>
      </div>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NForm,
  NFormItem,
  NInput,
  NModal,
  NPopconfirm,
  NSelect,
  NSpace,
} from "naive-ui";
import { isRevisionConflict, providerApi, type DynamicProviderView } from "../api/providers.ts";
import { t, type MessageKey } from "../i18n/index.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";
import { protocolDisplayName } from "../domain/provider-contracts.ts";
import {
  DYNAMIC_AUTH_KINDS,
  DYNAMIC_PAID_TEST_WARNING_KEY,
  DYNAMIC_PROTOCOLS,
  DYNAMIC_PROVIDER_DRAFT_ERROR_KEYS,
  buildDynamicProviderCreateBody,
  buildDynamicProviderUpdateBody,
  dynamicAuthRequiresKey,
  dynamicProviderActionNeedsConfirm,
  emptyDynamicProviderDraft,
  sanitizeDynamicProviderDraft,
  validateDynamicProviderDraft,
  type DynamicAuthKind,
  type DynamicProviderDraft,
  type DynamicUpstreamProtocol,
} from "../domain/dynamic-provider.ts";

const props = defineProps<{
  show: boolean;
  provider: DynamicProviderView | null;
}>();

const emit = defineEmits<{
  (event: "update:show", value: boolean): void;
  (event: "saved", providerId: string): void;
  (event: "conflict"): void;
}>();

const draft = ref<DynamicProviderDraft>(emptyDynamicProviderDraft());
const formError = ref("");
const conflictNotice = ref("");
const discoveryError = ref("");
const discoveredModels = ref<string[]>([]);
const selectedDiscovery = ref<string[]>([]);
const saving = ref(false);
const discovering = ref(false);
const testing = ref(false);

const isEdit = computed(() => Boolean(props.provider));
const busy = computed(() => saving.value || discovering.value || testing.value);
const testNeedsConfirm = dynamicProviderActionNeedsConfirm("test");
const paidTestWarningKey = DYNAMIC_PAID_TEST_WARNING_KEY;
const showKeyField = computed(() => (
  dynamicAuthRequiresKey(draft.value.auth_kind) || (isEdit.value && props.provider?.auth_kind === "none")
));
const protocolOptions = computed(() => DYNAMIC_PROTOCOLS.map((value) => ({
  value,
  label: protocolDisplayName(value),
})));
const authOptions = computed(() => DYNAMIC_AUTH_KINDS.map((value) => ({
  value,
  label: value === "none" ? t("无鉴权") : value === "bearer" ? "Bearer" : "x-api-key",
})));

watch(
  () => [props.show, props.provider] as const,
  ([visible, provider]) => {
    if (!visible) return;
    formError.value = "";
    conflictNotice.value = "";
    discoveryError.value = "";
    discoveredModels.value = [];
    selectedDiscovery.value = [];
    if (provider) {
      draft.value = {
        name: provider.name,
        endpoint_url: provider.endpoint_url,
        upstream_protocol: provider.upstream_protocol,
        auth_kind: provider.auth_kind,
        models: provider.models.map((model) => ({ ...model })),
        account_name: "",
        notes: "",
        key: "",
      };
    } else {
      draft.value = emptyDynamicProviderDraft();
    }
  },
  { immediate: true },
);

function addMapping(): void {
  draft.value.models.push({ public_model: "", upstream_model: "" });
}

function removeMapping(index: number): void {
  if (draft.value.models.length < 2) return;
  draft.value.models.splice(index, 1);
}

function importDiscovered(): void {
  const existing = new Set(draft.value.models.map((row) => row.public_model.trim().toLocaleLowerCase()));
  for (const model of selectedDiscovery.value) {
    if (existing.has(model.toLocaleLowerCase())) continue;
    if (draft.value.models.length === 1 && !draft.value.models[0]?.public_model && !draft.value.models[0]?.upstream_model) {
      draft.value.models[0] = { public_model: model, upstream_model: model };
    } else {
      draft.value.models.push({ public_model: model, upstream_model: model });
    }
    existing.add(model.toLocaleLowerCase());
  }
}

async function discover(): Promise<void> {
  discoveryError.value = "";
  discovering.value = true;
  try {
    const result = await providerApi.discoverDynamicProviderModels({
      endpoint_url: draft.value.endpoint_url,
      upstream_protocol: draft.value.upstream_protocol as DynamicUpstreamProtocol,
      auth_kind: draft.value.auth_kind as DynamicAuthKind,
      key: draft.value.key || undefined,
    });
    discoveredModels.value = result.models;
  } catch (error) {
    discoveryError.value = dashboardErrorDetail(error);
  } finally {
    discovering.value = false;
  }
}

async function runTest(): Promise<void> {
  const mapping = draft.value.models.find((row) => row.public_model.trim() && row.upstream_model.trim());
  if (!mapping) {
    formError.value = t("请至少添加一个完整模型映射");
    return;
  }
  testing.value = true;
  formError.value = "";
  try {
    const result = await providerApi.testDynamicProvider({
      endpoint_url: draft.value.endpoint_url,
      upstream_protocol: draft.value.upstream_protocol as DynamicUpstreamProtocol,
      auth_kind: draft.value.auth_kind as DynamicAuthKind,
      public_model: mapping.public_model.trim(),
      upstream_model: mapping.upstream_model.trim(),
      key: draft.value.key || undefined,
    });
    if (result.ok) formError.value = t("测试成功");
    else formError.value = t("测试失败: {error}", { error: result.error || "" });
  } catch (error) {
    formError.value = t("测试失败: {error}", { error: dashboardErrorDetail(error) });
  } finally {
    testing.value = false;
  }
}

async function save(): Promise<void> {
  const error = validateDynamicProviderDraft(draft.value, {
    mode: isEdit.value ? "edit" : "create",
    previousAuthKind: props.provider?.auth_kind ?? "",
  });
  if (error) {
    formError.value = t(DYNAMIC_PROVIDER_DRAFT_ERROR_KEYS[error] as MessageKey);
    return;
  }
  saving.value = true;
  formError.value = "";
  conflictNotice.value = "";
  try {
    const saved = isEdit.value && props.provider
      ? await providerApi.updateDynamicProvider(
        props.provider.id,
        buildDynamicProviderUpdateBody(draft.value, props.provider.auth_kind),
      )
      : await providerApi.createDynamicProvider(buildDynamicProviderCreateBody(draft.value));
    draft.value = sanitizeDynamicProviderDraft(draft.value);
    emit("saved", saved.id);
    emit("update:show", false);
  } catch (cause) {
    if (isRevisionConflict(cause)) {
      conflictNotice.value = t("数据已更新，请检查后重新保存。不会自动重试。");
      emit("conflict");
    } else {
      formError.value = dashboardErrorDetail(cause);
    }
  } finally {
    saving.value = false;
  }
}
</script>

<style scoped>
.modal-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
.form-error { margin-bottom: 12px; }
.full-width-field { grid-column: 1 / -1; }
.field-hint {
  margin: 6px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}
.capability-rows, .mapping-row { display: grid; gap: 8px; }
.capability-actions, .modal-footer, .discovery-import {
  display: flex;
  align-items: center;
  gap: 8px;
  justify-content: space-between;
}
.mapping-row {
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
}
.discovery-import { grid-column: 1 / -1; }
</style>
