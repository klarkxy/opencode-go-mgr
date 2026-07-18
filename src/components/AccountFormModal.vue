<template>
  <n-modal
    :show="show"
    preset="card"
    :title="title"
    class="account-modal"
    style="width: 560px; max-width: calc(100vw - 32px)"
    :mask-closable="false"
    @update:show="$emit('update:show', $event)"
  >
    <n-form
      ref="formRef"
      :model="form"
      :rules="rules"
      label-placement="top"
    >
      <div class="modal-grid">
        <n-form-item path="username" :label="t('账号')">
          <n-input
            :value="form.username"
            :input-props="{ 'aria-label': t('登录账号') }"
            :placeholder="t('OpenCode-Go 账号')"
            @update:value="handleUsernameUpdate"
          />
        </n-form-item>
        <n-form-item path="name" :label="t('名称')">
          <n-input
            :value="form.name"
            :input-props="{ 'aria-label': t('名称') }"
            :placeholder="t('主号')"
            @update:value="handleNameUpdate"
          />
        </n-form-item>
        <n-form-item path="purchaseDate" :label="t('购买日期')">
          <n-date-picker
            v-model:value="form.purchaseDate"
            type="date"
            format="yyyy-MM-dd"
            :actions="['now']"
            :clearable="false"
            :is-date-disabled="isPurchaseDateDisabled"
            :input-props="{ 'aria-label': t('购买日期') }"
          />
        </n-form-item>
        <n-form-item path="key" :label="t('API Key')">
          <n-input
            v-model:value="form.key"
            :input-props="{ 'aria-label': t('API Key') }"
            type="password"
            show-password-on="click"
            :placeholder="isEdit ? t('留空不修改') : 'sk-...'"
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
import { computed, ref, watch } from "vue";
import type { FormInst, FormRules } from "naive-ui";
import {
  NButton,
  NDatePicker,
  NForm,
  NFormItem,
  NInput,
  NModal,
  NSpace,
} from "naive-ui";
import type { Account } from "../api/tauri";
import { t } from "../i18n/index.ts";
import { localDateString } from "../views/account-lifecycle";

type AccountFormPayload = {
  name: string;
  username: string;
  key?: string;
  purchase_date?: string;
};

type AccountDraft = {
  name: string;
  username: string;
  key: string;
  purchaseDate: number | null;
};

const props = withDefaults(defineProps<{
  show: boolean;
  account: Account | null;
  isCooling?: boolean;
  busy?: boolean;
}>(), {
  account: null,
  isCooling: false,
  busy: false,
});

const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "save", payload: AccountFormPayload): void;
  (e: "resetCooldown"): void;
}>();

const formRef = ref<FormInst | null>(null);
const form = ref<AccountDraft>(blankAccountDraft());
const nameWasEdited = ref(false);

const isEdit = computed(() => !!props.account);
const title = computed(() => (isEdit.value ? t("编辑账号") : t("新增账号")));

const rules = computed<FormRules>(() => {
  const base: FormRules = {
    name: {
      required: true,
      whitespace: true,
      message: t("名称不能为空"),
      trigger: ["input", "blur"],
    },
    purchaseDate: [
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
    ],
  };
  if (!isEdit.value) {
    base.key = {
      required: true,
      whitespace: true,
      message: t("请填写 API Key"),
      trigger: ["input", "blur"],
    };
  }
  return base;
});

watch(() => props.show, (show) => {
  if (show) {
    form.value = props.account ? draftFromAccount(props.account) : blankAccountDraft();
    nameWasEdited.value = isEdit.value;
    formRef.value?.restoreValidation();
  }
});

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

function blankAccountDraft(): AccountDraft {
  return {
    name: "",
    username: "",
    key: "",
    purchaseDate: timestampFromLocalDate(localDateString()) ?? Date.now(),
  };
}

function draftFromAccount(account: Account): AccountDraft {
  return {
    name: account.name,
    username: account.username,
    key: "",
    purchaseDate: timestampFromLocalDate(account.purchase_date)
      ?? timestampFromLocalDate(localDateString())
      ?? Date.now(),
  };
}

function handleUsernameUpdate(value: string) {
  form.value.username = value;
  if (!isEdit.value && !nameWasEdited.value) {
    form.value.name = value;
  }
}

function handleNameUpdate(value: string) {
  form.value.name = value;
  if (!isEdit.value) {
    nameWasEdited.value = true;
  }
}

function isPurchaseDateDisabled(timestamp: number): boolean {
  return localDateString(timestamp) > localDateString();
}

async function handleSave() {
  try {
    await formRef.value?.validate();
  } catch {
    return;
  }
  const payload: AccountFormPayload = {
    name: form.value.name.trim(),
    username: form.value.username.trim(),
    purchase_date: form.value.purchaseDate === null ? undefined : localDateString(form.value.purchaseDate),
  };
  if (isEdit.value) {
    if (form.value.key.trim()) {
      payload.key = form.value.key.trim();
    }
  } else {
    payload.key = form.value.key.trim();
  }
  emit("save", payload);
}
</script>

<style scoped>
.modal-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
  align-items: start;
}

.modal-grid :deep(.n-date-picker) {
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
}
</style>
