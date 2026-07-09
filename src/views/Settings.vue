<template>
  <n-space vertical :size="16">
    <n-card title="Gateway 设置">
      <n-form :model="config" label-placement="left" label-width="120">
        <n-form-item label="监听端口">
          <n-input-number v-model:value="config.gateway_port" :min="1024" :max="65535" />
        </n-form-item>
        <n-form-item label="上游地址">
          <n-input v-model:value="config.upstream_base_url" placeholder="https://api.opencode.ai" />
        </n-form-item>
        <n-form-item label="Gateway Key">
          <n-space>
            <n-input v-model:value="config.gateway_key" type="password" show-password-on="click" style="width: 320px" />
            <n-button @click="regenerateKey">重新生成</n-button>
          </n-space>
        </n-form-item>
        <n-form-item label="开机自启">
          <n-switch v-model:value="config.auto_start" />
        </n-form-item>
      </n-form>
      <n-space>
        <n-button type="primary" @click="saveSettings">保存设置</n-button>
        <n-button @click="restartGateway">重启 Gateway</n-button>
      </n-space>
    </n-card>

    <n-card title="数据目录">
      <p>数据文件保存在：<code>{{ dataDir }}</code></p>
      <p class="text-muted">卸载应用时可选择是否删除此目录。</p>
    </n-card>
  </n-space>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import {
  NSpace,
  NCard,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSwitch,
  NButton,
  useMessage,
} from "naive-ui";
import { tauriApi, AppConfig, GatewayStatus } from "../api/tauri";

const message = useMessage();
// ponytail: keep in sync with AppConfig::default() in
// crates/ocg-core/src/models.rs. This is only the pre-load fallback.
const config = ref<AppConfig>({
  gateway_port: 9042,
  gateway_key: "",
  upstream_base_url: "https://opencode.ai/zen/go",
  auto_start: false,
  remote: { url: "", token: "" },
});
const dataDir = ref("%USERPROFILE%\\.ocg-mgr");
const savedPort = ref(9042);

async function loadSettings() {
  try {
    config.value = await tauriApi.getSettings();
    savedPort.value = config.value.gateway_port;
  } catch (e) {
    message.error(`加载设置失败: ${e}`);
  }
}

async function saveSettings() {
  const oldPort = savedPort.value;
  try {
    const status: GatewayStatus = await tauriApi.updateSettings(config.value);
    savedPort.value = status.port;
    if (status.running && oldPort !== status.port) {
      message.success(`设置已保存，Gateway 已切换到端口 ${status.port}`);
    } else {
      message.success("设置已保存");
    }
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

async function restartGateway() {
  try {
    const status: GatewayStatus = await tauriApi.restartGateway();
    message.success(`Gateway 已重启，端口 ${status.port}`);
  } catch (e) {
    message.error(`重启失败: ${e}`);
  }
}

onMounted(loadSettings);
</script>
