<template>
  <n-modal
    :show="show"
    preset="card"
    style="width: 520px"
    :mask-closable="false"
    :closable="false"
    title="首次配置"
  >
    <n-steps :current="step" size="small" style="margin-bottom: 16px">
      <n-step title="模式" />
      <n-step title="连接" />
    </n-steps>

    <div v-if="step === 0">
      <n-radio-group v-model:value="mode">
        <n-space vertical>
          <n-radio value="local">本地模式（单机使用，数据仅保存在本机）</n-radio>
          <n-radio value="cloud">
            云端模式（启用远端同步，把 key 推送到远程 daemon）
          </n-radio>
        </n-space>
      </n-radio-group>
    </div>

    <n-form
      v-else
      :model="form"
      label-placement="left"
      label-width="100"
    >
      <n-form-item label="远程 URL">
        <n-input
          v-model:value="form.url"
          placeholder="https://ocg.example.com"
          @update:value="onTokenOrUrlEdit"
        />
      </n-form-item>
      <n-form-item label="Token">
        <n-input
          v-model:value="form.token"
          type="password"
          show-password-on="click"
          placeholder="daemon 启动时打印的 bearer token"
          @update:value="onTokenOrUrlEdit"
        />
      </n-form-item>
      <n-space>
        <n-button :loading="testing" @click="onTest">测试连接</n-button>
        <n-button type="primary" :disabled="!testedOk" @click="onSave">
          保存
        </n-button>
      </n-space>
      <p v-if="testMessage" style="margin-top: 12px; color: #888">
        {{ testMessage }}
      </p>
    </n-form>

    <template #footer>
      <n-space justify="end">
        <n-button v-if="step === 1" @click="step = 0">上一步</n-button>
        <n-button
          v-if="step === 0"
          type="primary"
          @click="onModeContinue"
        >
          下一步
        </n-button>
      </n-space>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import {
  NModal,
  NSteps,
  NStep,
  NRadioGroup,
  NRadio,
  NSpace,
  NForm,
  NFormItem,
  NInput,
  NButton,
  useMessage,
} from "naive-ui";
import { tauriApi, AppConfig } from "../api/tauri";

const props = defineProps<{ show: boolean; config: AppConfig }>();
const emit = defineEmits<{ done: [] }>();
const message = useMessage();

const step = ref(0);
const mode = ref<"local" | "cloud">("local");
const form = ref({ url: "", token: "" });
const testing = ref(false);
const testedOk = ref(false);
const testMessage = ref("");

function onModeContinue() {
  if (mode.value === "local") {
    void onSave();
  } else {
    step.value = 1;
  }
}

watch(mode, (m) => {
  if (m === "local") {
    testedOk.value = true;
  } else {
    testedOk.value = false;
    testMessage.value = "";
  }
});

async function onTest() {
  testing.value = true;
  testedOk.value = false;
  testMessage.value = "";
  try {
    const r = await tauriApi.testRemote(form.value.url, form.value.token);
    testedOk.value = r.ok;
    testMessage.value = r.message;
    if (r.ok) {
      message.success("连接成功");
    } else {
      message.error(r.message);
    }
  } catch (e) {
    testMessage.value = String(e);
    message.error(String(e));
  } finally {
    testing.value = false;
  }
}

function onTokenOrUrlEdit() {
  // ponytail: any edit to URL or token invalidates the prior test. Don't
  // allow saving a half-tested config — force the user to re-test.
  testedOk.value = false;
  testMessage.value = "";
}

async function onSave() {
  const next: AppConfig = {
    ...props.config,
    remote:
      mode.value === "cloud"
        ? { url: form.value.url.trim(), token: form.value.token.trim() }
        : { url: "", token: "" },
  };
  try {
    await tauriApi.updateSettings(next);
    emit("done");
  } catch (e) {
    message.error(`保存失败: ${e}`);
  }
}
</script>
