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
      </n-form>
      <n-space>
        <n-button type="primary" @click="saveSettings()">保存设置</n-button>
      </n-space>
    </n-card>

    <n-card title="远端节点">
      <n-form :model="config" label-placement="left" label-width="120">
        <n-form-item label="远端 URL">
          <n-input v-model:value="config.remote.url" placeholder="https://ocg.example.com 或 http://127.0.0.1:PORT" />
        </n-form-item>
        <n-form-item label="Admin Token">
          <n-input v-model:value="config.remote.token" type="password" show-password-on="click" placeholder="远端 serve --admin-token" />
        </n-form-item>
      </n-form>
      <n-alert v-if="remoteDirty" type="warning" :bordered="false" class="remote-alert">
        远端节点设置有未保存修改，刷新和推送会使用已保存配置。
      </n-alert>
      <n-space>
        <n-button @click="saveSettings()">保存远端设置</n-button>
        <n-button :loading="remoteBusy" @click="testRemote">测试连接</n-button>
        <n-button :disabled="!remoteConfigured || remoteDirty" :loading="remoteBusy" @click="refreshRemoteStatus">刷新远端状态</n-button>
        <n-button :disabled="!remoteConfigured || remoteDirty" :loading="remoteBusy" @click="pushLocalToRemote">推送本地到远端</n-button>
      </n-space>
      <div v-if="remoteStatus" class="remote-status">
        <div class="remote-status-head">
          <span class="remote-url">{{ remoteStatus.url }}</span>
          <n-tag :type="remoteStatus.gateway.running ? 'success' : 'error'" size="small">
            Gateway {{ remoteStatus.gateway.running ? "运行中" : "已停止" }}
          </n-tag>
        </div>
        <div class="remote-grid">
          <span>版本</span><strong>{{ remoteStatus.version }}</strong>
          <span>端口</span><strong>{{ remoteStatus.gateway.port }}</strong>
          <span>账号</span><strong>{{ remoteStatus.accounts.available }}/{{ remoteStatus.accounts.total }} 可用</strong>
          <span>禁用 / 冷却</span><strong>{{ remoteStatus.accounts.disabled }} / {{ remoteStatus.accounts.cooldown }}</strong>
          <span>今日 / 本周 / 本月</span><strong>${{ formatCost(remoteStatus.usage.today_cost) }} / ${{ formatCost(remoteStatus.usage.week_cost) }} / ${{ formatCost(remoteStatus.usage.month_cost) }}</strong>
          <span>上游</span><strong class="mono">{{ remoteStatus.gateway.upstream_base_url }}</strong>
        </div>
        <p v-if="remoteStatus.last_error || remoteStatus.gateway.last_error" class="remote-error">
          {{ remoteStatus.gateway.last_error || remoteStatus.last_error }}
        </p>
      </div>
      <p v-else class="text-muted remote-empty">远端状态尚未刷新。</p>
    </n-card>

    <n-card title="数据目录">
      <p>数据文件保存在：<code>{{ dataDir }}</code></p>
      <p class="text-muted">卸载应用时可选择是否删除此目录。</p>
    </n-card>
  </n-space>
</template>

<script setup lang="ts">
import { computed, ref, onMounted } from "vue";
import {
  NSpace,
  NCard,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NButton,
  NAlert,
  NTag,
  useMessage,
} from "naive-ui";
import { tauriApi } from "../api/tauri";
import type { AppConfig, GatewayStatus, RemoteNodeStatus } from "../api/tauri";

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
const savedRemoteUrl = ref("");
const savedRemoteToken = ref("");
const remoteStatus = ref<RemoteNodeStatus | null>(null);
const remoteBusy = ref(false);
const remoteConfigured = computed(() =>
  config.value.remote.url.trim() !== "" && config.value.remote.token.trim() !== ""
);
const remoteDirty = computed(() =>
  config.value.remote.url !== savedRemoteUrl.value ||
  config.value.remote.token !== savedRemoteToken.value
);

async function loadSettings() {
  try {
    config.value = await tauriApi.getSettings();
    savedPort.value = config.value.gateway_port;
    savedRemoteUrl.value = config.value.remote.url;
    savedRemoteToken.value = config.value.remote.token;
  } catch (e) {
    message.error(`加载设置失败: ${e}`);
  }
}

async function saveSettings(showMessage = true): Promise<boolean> {
  const oldPort = savedPort.value;
  try {
    const status: GatewayStatus = await tauriApi.updateSettings(config.value);
    savedPort.value = status.port;
    savedRemoteUrl.value = config.value.remote.url;
    savedRemoteToken.value = config.value.remote.token;
    window.dispatchEvent(new CustomEvent("ocg-gateway-status", { detail: status }));
    if (status.running && oldPort !== status.port) {
      if (showMessage) message.success(`设置已保存，Gateway 已切换到端口 ${status.port}`);
    } else if (showMessage) {
      message.success("设置已保存");
    }
    return true;
  } catch (e) {
    message.error(`保存失败: ${e}`);
    return false;
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

async function testRemote() {
  remoteBusy.value = true;
  try {
    const result = await tauriApi.testRemote(config.value.remote.url, config.value.remote.token);
    if (result.ok) {
      message.success(`连接成功: ${result.message}`);
    } else {
      message.error(`连接失败: ${result.message}`);
    }
  } catch (e) {
    message.error(`连接失败: ${e}`);
  } finally {
    remoteBusy.value = false;
  }
}

function ensureRemoteReady(): boolean {
  if (!remoteConfigured.value) {
    message.warning("请先填写并保存远端 URL/token");
    return false;
  }
  if (remoteDirty.value) {
    message.warning("远端设置有未保存修改，请先保存");
    return false;
  }
  return true;
}

async function refreshRemoteStatus() {
  if (!ensureRemoteReady()) return;
  remoteBusy.value = true;
  try {
    remoteStatus.value = await tauriApi.getRemoteNodeStatus();
    message.success("远端状态已刷新");
  } catch (e) {
    message.error(`刷新失败: ${e}`);
  } finally {
    remoteBusy.value = false;
  }
}

async function pushLocalToRemote() {
  if (!ensureRemoteReady()) return;
  remoteBusy.value = true;
  try {
    const result = await tauriApi.pushLocalToRemote();
    message.success(result.message);
  } catch (e) {
    message.error(`推送失败: ${e}`);
  } finally {
    remoteBusy.value = false;
  }
}

function formatCost(v: number): string {
  if (v === 0) return "0.00";
  if (v < 0.01) return v.toFixed(4);
  return v.toFixed(2);
}

onMounted(loadSettings);
</script>

<style scoped>
.remote-alert {
  margin-bottom: 12px;
}
.remote-status {
  margin-top: 14px;
  padding: 12px 14px;
  border: 1px solid var(--n-border-color, rgba(0, 0, 0, 0.06));
  border-radius: 8px;
}
.remote-status-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 10px;
}
.remote-url {
  font-weight: 600;
  word-break: break-all;
}
.remote-grid {
  display: grid;
  grid-template-columns: 120px minmax(0, 1fr);
  gap: 8px 14px;
  font-size: 13px;
}
.remote-grid span {
  color: var(--n-text-color-3, #888);
}
.remote-grid strong {
  font-weight: 600;
  word-break: break-word;
}
.mono {
  font-family: v-mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}
.remote-error {
  margin: 10px 0 0;
  color: #d03050;
  font-size: 13px;
}
.remote-empty {
  margin-bottom: 0;
}
</style>
