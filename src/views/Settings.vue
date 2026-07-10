<template>
  <n-card title="转发设置">
      <n-form :model="config" label-placement="left" label-width="120">
        <n-form-item label="上游地址">
          <n-input v-model:value="config.upstream_base_url" placeholder="https://api.opencode.ai" />
        </n-form-item>
        <n-form-item label="Gateway Key">
          <n-space>
            <n-input v-model:value="config.gateway_key" type="password" show-password-on="click" style="width: 320px" />
            <n-button @click="regenerateKey">重新生成</n-button>
          </n-space>
        </n-form-item>
      </n-form>
      <n-space>
        <n-button type="primary" @click="saveSettings()">保存设置</n-button>
      </n-space>
  </n-card>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import {
  NSpace,
  NCard,
  NForm,
  NFormItem,
  NInput,
  NButton,
  useMessage,
} from "naive-ui";
import { tauriApi } from "../api/tauri";
import type { AppConfig } from "../api/tauri";

const message = useMessage();
// ponytail: keep in sync with AppConfig::default() in
// crates/ocg-core/src/models.rs. This is only the pre-load fallback.
const config = ref<AppConfig>({
  gateway_port: 9042,
  gateway_key: "",
  upstream_base_url: "https://opencode.ai/zen/go",
  auto_start: false,
});
async function loadSettings() {
  try {
    config.value = await tauriApi.getSettings();
  } catch (e) {
    message.error(`加载设置失败: ${e}`);
  }
}

async function saveSettings() {
  try {
    await tauriApi.updateSettings(config.value);
    message.success("设置已保存");
  } catch (e) {
    message.error(`保存失败: ${e}`);
  }
}

async function regenerateKey() {
  try {
    const newKey = await tauriApi.regenerateGatewayKey();
    config.value.gateway_key = newKey;
    message.success("Gateway Key 已重新生成");
  } catch (e) {
    message.error(`生成失败: ${e}`);
  }
}

onMounted(loadSettings);
</script>
