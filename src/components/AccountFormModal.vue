<template>
  <n-modal
    :show="show"
    preset="card"
    :title="title"
    class="account-modal"
    style="width: 600px; max-width: calc(100vw - 32px)"
    :mask-closable="false"
    @update:show="$emit('update:show', $event)"
  >
    <n-form
      ref="formRef"
      :model="form"
      :rules="rules"
      label-placement="top"
    >
      <n-alert v-if="formError" type="error" class="form-error" role="alert">
        {{ formError }}
      </n-alert>
      <n-alert
        v-if="isCustomPlan"
        type="warning"
        :show-icon="false"
        class="form-error"
      >
        {{ t("目标端点由管理员自行选择并负责：使用 http:// 时 Key 将明文传输；测试连接会发送最小真实请求，可能产生服务商费用。") }}
      </n-alert>
      <n-alert
        v-if="isDynamicPlan"
        type="default"
        :show-icon="false"
        class="form-error"
      >
        {{ t("账号不拥有 Endpoint、协议或模型映射。") }}
      </n-alert>
      <div class="modal-grid">
        <n-form-item path="name" :label="t('名称')">
          <n-input
            :value="form.name"
            :input-props="{ 'aria-label': t('名称') }"
            :placeholder="t('例如：主号')"
            @update:value="handleNameUpdate"
          />
        </n-form-item>

        <n-form-item
          v-if="hasField('username')"
          path="username"
          :label="t('账号')"
        >
          <n-input
            :value="form.username"
            :input-props="{ 'aria-label': t('登录账号') }"
            :placeholder="t('OpenCode-Go 账号')"
            @update:value="form.username = $event"
          />
        </n-form-item>

        <n-form-item
          v-if="hasField('purchase_date')"
          path="purchaseDate"
          :label="t('购买日期')"
        >
          <div class="purchase-date-control">
            <n-date-picker
              v-model:value="form.purchaseDate"
              type="date"
              format="yyyy-MM-dd"
              :clearable="!purchaseDateRequired"
              :is-date-disabled="isPurchaseDateDisabled"
              :input-props="{ 'aria-label': t('购买日期') }"
            />
            <n-button
              v-if="isEdit"
              secondary
              :disabled="isPurchaseDateToday"
              @click="setPurchaseDateToday"
            >
              {{ t("今日") }}
            </n-button>
          </div>
        </n-form-item>

        <n-form-item
          v-if="hasField('key')"
          path="key"
          :label="t('API Key')"
          class="full-width-field"
        >
          <n-input
            v-model:value="form.key"
            :input-props="{ 'aria-label': t('API Key') }"
            type="password"
            show-password-on="click"
            :placeholder="keyPlaceholder"
          />
        </n-form-item>

        <n-form-item
          v-if="isCustomPlan"
          path="endpointUrl"
          :label="t('API 地址')"
          class="full-width-field"
        >
          <div class="endpoint-field">
            <n-input
              v-model:value="form.endpointUrl"
              :input-props="{ 'aria-label': t('API 地址') }"
              :placeholder="endpointPlaceholder"
            />
            <p class="field-hint">
              {{ t("推荐填写不带 /v1 的 API 根地址；OCG 会自动补全 /v1 和协议路径。已带 /v1 时不会重复添加。") }}
            </p>
          </div>
        </n-form-item>

        <n-form-item
          v-if="isCustomPlan"
          path="upstreamProtocol"
          :label="t('上游协议')"
        >
          <div class="protocol-field">
            <n-select
              v-model:value="form.upstreamProtocol"
              :options="upstreamProtocolOptions"
              :placeholder="t('上游协议')"
              :aria-label="t('上游协议')"
            />
            <p class="field-hint">{{ t("所选协议对该账号下全部模型统一生效。") }}</p>
          </div>
        </n-form-item>

        <n-form-item
          v-if="isCustomPlan"
          path="modelCapabilities"
          :label="t('模型映射')"
          class="full-width-field"
        >
          <div class="capability-rows">
            <div class="capability-actions">
              <n-button
                size="small"
                secondary
                :loading="discoveringModels"
                :disabled="!canDiscoverModels"
                @click="discoverModels"
              >
                {{ t("获取模型") }}
              </n-button>
              <n-button size="small" secondary @click="addModelMapping">
                {{ t("添加映射") }}
              </n-button>
              <span v-if="discoverySuccess" class="field-hint">{{ discoverySuccess }}</span>
            </div>
            <p v-if="showManualModelHint" class="field-hint">
              {{ t("非标准完整 Endpoint 无法自动推导 /models；请手动添加模型映射。") }}
            </p>
            <n-alert v-if="discoveryError" type="error" :show-icon="false">
              {{ discoveryError }}
            </n-alert>
            <div v-if="discoveryOptions.length > 0" class="discovery-import">
              <n-select
                v-model:value="selectedDiscoveredModels"
                :options="discoveryOptions"
                multiple
                filterable
                :placeholder="t('选择要导入的模型')"
                :aria-label="t('选择要导入的模型')"
                max-tag-count="responsive"
              />
              <n-button
                size="small"
                secondary
                :disabled="selectedDiscoveredModels.length === 0"
                @click="importSelectedModels"
              >{{ t("导入所选") }}</n-button>
            </div>
            <div class="mapping-rows" role="list" :aria-label="t('模型映射')">
              <div
                v-for="(mapping, index) in form.modelCapabilities"
                :key="mapping.row_id"
                class="mapping-row"
                role="listitem"
              >
                <n-input
                  v-model:value="mapping.public_model"
                  :placeholder="t('对外模型名')"
                  :aria-label="t('对外模型名')"
                />
                <n-input
                  v-model:value="mapping.upstream_model"
                  :placeholder="t('上游模型 ID')"
                  :aria-label="t('上游模型 ID')"
                  class="mono"
                />
                <n-button
                  size="small"
                  tertiary
                  :aria-label="t('删除映射')"
                  @click="removeModelMapping(index)"
                >{{ t("删除") }}</n-button>
              </div>
            </div>
            <p class="field-hint capability-count">
              {{ t("{count} 个模型", { count: form.modelCapabilities.length }) }} ·
              {{ t("对外模型名不区分大小写且必须唯一；上游模型 ID 可复用。") }}
            </p>
          </div>
        </n-form-item>

        <n-form-item
          v-if="hasField('notes')"
          path="notes"
          :label="t('备注')"
          class="full-width-field"
        >
          <n-input
            v-model:value="form.notes"
            type="textarea"
            :autosize="{ minRows: 4, maxRows: 10 }"
            :maxlength="4000"
            show-count
            :placeholder="t('可填写任意备注')"
            :input-props="{ 'aria-label': t('备注') }"
          />
        </n-form-item>
      </div>
    </n-form>
    <template #footer>
      <div class="modal-footer">
        <n-button
          v-if="isEdit && isCooling"
          text
          size="small"
          type="warning"
          @click="$emit('resetCooldown')"
        >
          {{ t("重置冷却") }}
        </n-button>
        <n-space>
          <n-button @click="$emit('update:show', false)">{{ t("取消") }}</n-button>
          <n-button type="primary" :loading="busy" @click="handleSave">{{ t("保存") }}</n-button>
        </n-space>
      </div>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref, toRef, watch } from "vue";
import type { FormInst, FormRules } from "naive-ui";
import {
  NAlert,
  NButton,
  NDatePicker,
  NForm,
  NFormItem,
  NInput,
  NModal,
  NSelect,
  NSpace,
} from "naive-ui";
import { dashboardApi, type Account, type AccountInput, type AccountProtocol } from "../api/dashboard";
import type { ProviderCatalogEntry, ProviderCatalogFormField } from "../api/providers.ts";
import { t } from "../i18n/index.ts";
import { useLocalizedModalCloseLabel } from "../utils/modal-close-label.ts";
import { localDateString } from "../domain/account-lifecycle.ts";
import { findCatalogEntry, planFamilyLabel, planForAccount } from "../domain/plans.ts";
import type { PlanDefinition } from "../domain/plans.ts";
import { resolveAccountFormFields } from "../domain/account-form-fields.ts";
import {
  accountCreatePayloadErrorKey,
  buildCreateAccountPayload,
  type AccountCreateCapability,
  type AccountCreateFormValues,
} from "../domain/account-create-payload.ts";
import {
  CUSTOM_ENDPOINT_URL_ISSUE_KEYS,
  CUSTOM_PROTOCOLS,
  customEndpointUrlIssue,
  customApiUrlPlaceholder,
  customApiUrlNeedsManualModels,
  customApiUrlSupportsModelDiscovery,
} from "../domain/custom-account.ts";
import { protocolDisplayName } from "../domain/provider-contracts.ts";

export type AccountFormPayload = {
  name: string;
  username: string;
  key?: string;
  provider_id?: string;
  purchase_date?: string;
  notes: string;
  /** Custom API edit only; persisted via the dedicated custom-config route. */
  endpoint_url?: string;
  /** Custom API edit only; persisted via the dedicated custom-config route. */
  upstream_protocol?: AccountProtocol;
  /** Custom API edit only; atomically persisted with the dedicated custom-config route. */
  model_capabilities?: Array<{
    public_model: string;
    upstream_model: string;
    protocol: AccountProtocol;
  }>;
};

type FormModel = {
  name: string;
  username: string;
  key: string;
  purchaseDate: number | null;
  notes: string;
  endpointUrl: string;
  upstreamProtocol: AccountProtocol | null;
  modelCapabilities: EditableModelCapability[];
};

type EditableModelCapability = AccountCreateCapability & { row_id: number };

type ModelDiscoveryContext = {
  show: boolean;
  accountId: string;
  endpointUrl: string;
  upstreamProtocol: FormModel["upstreamProtocol"];
  key: string;
};

const props = withDefaults(defineProps<{
  show: boolean;
  account: Account | null;
  isCooling?: boolean;
  busy?: boolean;
  /** The selected plan family when creating an account. */
  plan: PlanDefinition | null;
  /** Provider catalog; when null, only the legacy OpenCode Go path is supported. */
  catalog: readonly ProviderCatalogEntry[] | null;
}>(), {
  account: null,
  isCooling: false,
  busy: false,
  plan: null,
  catalog: null,
});

const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "save", payload: AccountInput | AccountFormPayload): void;
  (e: "resetCooldown"): void;
}>();

useLocalizedModalCloseLabel(toRef(props, "show"), "account-modal");

const formRef = ref<FormInst | null>(null);
const form = ref<FormModel>(blankForm());
const nameWasEdited = ref(false);
const formError = ref("");
const discoveringModels = ref(false);
const discoveryError = ref("");
const discoverySuccess = ref("");
const discoveredModels = ref<string[]>([]);
const selectedDiscoveredModels = ref<string[]>([]);
let discoveryGeneration = 0;
let nextModelMappingRowId = 1;

const isEdit = computed(() => !!props.account);
const title = computed(() => {
  if (isEdit.value) return t("编辑账号");
  const plan = effectivePlan.value;
  return plan
    ? t("添加 {plan} 账号", { plan: planFamilyLabel(plan, props.catalog) })
    : t("导入已有 Key");
});

const effectivePlan = computed<PlanDefinition | null>(() => {
  if (isEdit.value) {
    const account = props.account!;
    return planForAccount(account, props.catalog);
  }
  return props.plan;
});

const isCustomPlan = computed(() => effectivePlan.value?.id === "custom-endpoint");
const isDynamicPlan = computed(() => effectivePlan.value?.id === "dynamic-http");

const catalogEntry = computed<ProviderCatalogEntry | undefined>(() => {
  const plan = effectivePlan.value;
  if (!plan) return undefined;
  return findCatalogEntry(props.catalog, plan.provider_id);
});

const formFields = computed<ProviderCatalogFormField[]>(() => {
  return resolveAccountFormFields(effectivePlan.value, catalogEntry.value);
});

const fieldMap = computed(() => new Map(formFields.value.map((field) => [field.id, field])));

function hasField(id: string): boolean {
  return fieldMap.value.has(id);
}

function fieldRequired(id: string): boolean {
  return fieldMap.value.get(id)?.required ?? false;
}

const keyPlaceholder = computed(() => {
  const prefix = catalogEntry.value?.key_prefix;
  if (prefix) return prefix + "...";
  return "sk-...";
});

const purchaseDateRequired = computed(() => fieldRequired("purchase_date"));
const isPurchaseDateToday = computed(() => (
  form.value.purchaseDate !== null
  && localDateString(form.value.purchaseDate) === localDateString()
));

const upstreamProtocolOptions = computed(() => {
  return CUSTOM_PROTOCOLS.map((value) => ({
    value,
    label: protocolDisplayName(value),
  }));
});

const endpointPlaceholder = computed(() => customApiUrlPlaceholder());
const canInferModelEndpoint = computed(() => isCustomPlan.value
  && customApiUrlSupportsModelDiscovery(form.value.endpointUrl, form.value.upstreamProtocol));
const showManualModelHint = computed(() => isCustomPlan.value
  && customApiUrlNeedsManualModels(form.value.endpointUrl, form.value.upstreamProtocol));
const canDiscoverModels = computed(() => canInferModelEndpoint.value
  && (!!form.value.key.trim() || !!props.account?.id));
const discoveryOptions = computed(() => {
  const existing = new Set(
    form.value.modelCapabilities.map((capability) => capability.public_model.trim().toLocaleLowerCase()),
  );
  return discoveredModels.value
    .filter((model) => !existing.has(model.toLocaleLowerCase()))
    .map((model) => ({ value: model, label: model }));
});

const rules = computed<FormRules>(() => {
  const base: FormRules = {
    name: {
      required: true,
      whitespace: true,
      message: t("名称不能为空"),
      trigger: ["input", "blur"],
    },
  };

  if (fieldRequired("purchase_date")) {
    base.purchaseDate = [
      {
        required: true,
        type: "number",
        message: t("请选择购买日期"),
        trigger: ["change", "blur"],
      },
      {
        validator: (_rule: unknown, value: number | null) => {
          if (value === null) return true;
          return localDateString(value) <= localDateString();
        },
        message: t("购买日期不能晚于今天"),
        trigger: ["change", "blur"],
      },
    ];
  }

  if (hasField("key") && !isEdit.value) {
    base.key = {
      required: true,
      whitespace: true,
      message: t("请填写 API Key"),
      trigger: ["input", "blur"],
    };
  }

  if (isCustomPlan.value) {
    base.endpointUrl = {
      required: true,
      validator: (_rule: unknown, value: string) => {
        const issue = customEndpointUrlIssue(value ?? "");
        return issue ? new Error(t(CUSTOM_ENDPOINT_URL_ISSUE_KEYS[issue])) : true;
      },
      trigger: ["input", "blur"],
    };
    base.upstreamProtocol = {
      required: true,
      type: "string",
      validator: (_rule: unknown, value: AccountProtocol | null) => !!value,
      message: t("请选择上游协议"),
      trigger: ["change", "blur"],
    };
    base.modelCapabilities = {
      required: true,
      type: "array",
      validator: (_rule: unknown, value: AccountCreateCapability[]) =>
        Array.isArray(value) && value.length > 0 && value.every((cap) => (
          cap.public_model.trim() && cap.upstream_model.trim()
        )),
      message: t("请至少添加一个完整模型映射"),
      trigger: ["change"],
    };
  }

  return base;
});

watch(() => props.show, (show) => {
  if (show) {
    form.value = props.account ? formFromAccount(props.account) : blankForm();
    nameWasEdited.value = isEdit.value;
    formRef.value?.restoreValidation();
    formError.value = "";
    discoveryError.value = "";
    discoverySuccess.value = "";
    discoveredModels.value = [];
    selectedDiscoveredModels.value = [];
  }
});

function currentModelDiscoveryContext(): ModelDiscoveryContext {
  return {
    show: props.show,
    accountId: props.account?.id ?? "",
    endpointUrl: form.value.endpointUrl,
    upstreamProtocol: form.value.upstreamProtocol,
    key: form.value.key,
  };
}

function modelDiscoveryContextMatches(expected: ModelDiscoveryContext): boolean {
  const current = currentModelDiscoveryContext();
  return current.show === expected.show
    && current.accountId === expected.accountId
    && current.endpointUrl === expected.endpointUrl
    && current.upstreamProtocol === expected.upstreamProtocol
    && current.key === expected.key;
}

watch(
  () => currentModelDiscoveryContext(),
  () => {
    discoveryGeneration += 1;
    discoveringModels.value = false;
    discoveryError.value = "";
    discoverySuccess.value = "";
    discoveredModels.value = [];
    selectedDiscoveredModels.value = [];
  },
  { flush: "sync" },
);

function timestampFromLocalDate(value: string): number | null {
  const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!parts) return null;
  const year = Number(parts[1]);
  const month = Number(parts[2]);
  const day = Number(parts[3]);
  const date = new Date(year, month - 1, day);
  return date.getFullYear() === year && date.getMonth() === month - 1 && date.getDate() === day
    ? date.getTime()
    : null;
}

function blankForm(): FormModel {
  return {
    name: "",
    username: "",
    key: "",
    purchaseDate: timestampFromLocalDate(localDateString()) ?? Date.now(),
    notes: "",
    endpointUrl: "",
    upstreamProtocol: "chat_completions",
    modelCapabilities: [],
  };
}

function formFromAccount(account: Account): FormModel {
  const modelCapabilities: EditableModelCapability[] = [];
  for (const capability of account.model_capabilities) {
    modelCapabilities.push(modelMapping({
      public_model: capability.public_model,
      upstream_model: capability.upstream_model,
    }));
  }
  return {
    name: account.name,
    username: account.username,
    key: "",
    purchaseDate: timestampFromLocalDate(account.purchase_date),
    notes: account.notes ?? "",
    endpointUrl: account.custom_config?.endpoint_url ?? "",
    upstreamProtocol: account.custom_config?.upstream_protocol ?? "chat_completions",
    modelCapabilities,
  };
}

function handleNameUpdate(value: string) {
  form.value.name = value;
  if (!isEdit.value && !nameWasEdited.value) {
    form.value.name = value;
  }
}

function isPurchaseDateDisabled(timestamp: number): boolean {
  return localDateString(timestamp) > localDateString();
}

function setPurchaseDateToday() {
  form.value.purchaseDate = timestampFromLocalDate(localDateString()) ?? Date.now();
}

function addModelMapping(): void {
  form.value.modelCapabilities.push(modelMapping({ public_model: "", upstream_model: "" }));
}

function removeModelMapping(index: number): void {
  form.value.modelCapabilities.splice(index, 1);
}

function modelMapping(capability: AccountCreateCapability): EditableModelCapability {
  return { ...capability, row_id: nextModelMappingRowId++ };
}

function importSelectedModels(): void {
  const existing = new Set(
    form.value.modelCapabilities.map((capability) => capability.public_model.trim().toLocaleLowerCase()),
  );
  let imported = 0;
  for (const model of selectedDiscoveredModels.value) {
    const identity = model.toLocaleLowerCase();
    if (existing.has(identity)) continue;
    existing.add(identity);
    form.value.modelCapabilities.push(modelMapping({ public_model: model, upstream_model: model }));
    imported += 1;
  }
  selectedDiscoveredModels.value = [];
  discoverySuccess.value = t("已导入 {count} 个模型", { count: imported });
}

async function discoverModels() {
  if (!canDiscoverModels.value || !form.value.upstreamProtocol) return;
  const context = currentModelDiscoveryContext();
  const generation = ++discoveryGeneration;
  discoveringModels.value = true;
  discoveryError.value = "";
  discoverySuccess.value = "";
  try {
    const result = await dashboardApi.discoverCustomModels({
      endpoint_url: form.value.endpointUrl.trim(),
      upstream_protocol: form.value.upstreamProtocol,
      ...(form.value.key.trim() ? { api_key: form.value.key.trim() } : {}),
      ...(props.account?.id ? { account_id: props.account.id } : {}),
    });
    if (generation !== discoveryGeneration || !modelDiscoveryContextMatches(context)) return;
    discoveredModels.value = result.models;
    selectedDiscoveredModels.value = [];
    if (result.models.length === 0) {
      discoverySuccess.value = t("未获取到模型，请手动添加模型 ID");
    } else if (result.truncated) {
      discoverySuccess.value = t("已获取 {count} 个模型（结果已截断）", { count: result.models.length });
    } else {
      discoverySuccess.value = t("已获取 {count} 个模型", { count: result.models.length });
    }
  } catch (error) {
    if (generation !== discoveryGeneration || !modelDiscoveryContextMatches(context)) return;
    discoveryError.value = error instanceof Error
      ? error.message
      : t("获取模型失败，请检查配置后重试");
  } finally {
    if (generation === discoveryGeneration && modelDiscoveryContextMatches(context)) {
      discoveringModels.value = false;
    }
  }
}

async function handleSave() {
  try {
    await formRef.value?.validate();
  } catch {
    return;
  }

  if (isEdit.value) {
    const payload: AccountFormPayload = {
      name: form.value.name.trim(),
      username: form.value.username.trim(),
      notes: form.value.notes,
    };
    if (hasField("purchase_date")) {
      // Only catalog-declared subscription plans own a monthly purchase date.
      // Hidden form defaults must never reset other account types.
      payload.purchase_date = form.value.purchaseDate === null ? undefined : localDateString(form.value.purchaseDate);
    }
    if (form.value.key.trim()) {
      payload.key = form.value.key.trim();
    }
    if (isCustomPlan.value) {
      payload.endpoint_url = form.value.endpointUrl.trim();
      payload.upstream_protocol = form.value.upstreamProtocol ?? undefined;
      payload.model_capabilities = form.value.modelCapabilities.map((capability) => ({
        public_model: capability.public_model,
        upstream_model: capability.upstream_model,
        protocol: form.value.upstreamProtocol ?? "chat_completions",
      }));
    }
    emit("save", payload);
    return;
  }

  const plan = effectivePlan.value;
  if (!plan) {
    formError.value = t("无法确定账号方案，请关闭后重试");
    return;
  }

  const values: AccountCreateFormValues = {
    name: form.value.name,
    username: form.value.username,
    key: form.value.key,
    notes: form.value.notes,
  };
  if (hasField("purchase_date")) {
    values.purchase_date = form.value.purchaseDate === null
      ? undefined
      : localDateString(form.value.purchaseDate);
  }
  // The form model keeps a default Custom protocol so that opening the Custom
  // plan is convenient. Do not leak that hidden field into sealed built-in
  // plans: the payload builder correctly rejects Custom-only fields there.
  if (isCustomPlan.value) {
    values.endpoint_url = form.value.endpointUrl;
    values.upstream_protocol = form.value.upstreamProtocol ?? undefined;
    values.model_capabilities = form.value.modelCapabilities.length > 0
      ? form.value.modelCapabilities
      : undefined;
  }

  try {
    const payload = buildCreateAccountPayload(plan, values);
    emit("save", payload);
  } catch (error) {
    // Never submit a degraded payload: the backend rejects incomplete Custom
    // plans, so keep the draft editable instead.
    formError.value = t(accountCreatePayloadErrorKey(error));
  }
}
</script>

<style scoped>
.modal-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
  align-items: start;
}

.form-error {
  margin-bottom: 12px;
}

.full-width-field,
.notes-field {
  grid-column: 1 / -1;
}

.field-hint {
  margin: 6px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}

.capability-rows {
  display: grid;
  gap: 8px;
}

.capability-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.discovery-import {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  align-items: center;
}

.mapping-rows {
  display: grid;
  gap: 8px;
}

.mapping-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
  gap: 8px;
  align-items: center;
}

.capability-count {
  margin-top: 0;
  font-variant-numeric: tabular-nums;
}

.endpoint-field,
.protocol-field {
  display: grid;
  gap: 4px;
  width: 100%;
}

.modal-grid :deep(.n-date-picker) {
  width: 100%;
}

.purchase-date-control {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  width: 100%;
}

.modal-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
}

@media (max-width: 640px) {
  .modal-grid {
    grid-template-columns: 1fr;
  }

  .mapping-row {
    grid-template-columns: 1fr;
  }

  .discovery-import {
    grid-template-columns: 1fr;
  }

}
</style>
