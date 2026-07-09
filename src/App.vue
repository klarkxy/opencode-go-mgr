<template>
  <n-message-provider>
  <n-layout has-sider style="height: 100%">
    <n-layout-sider
      bordered
      collapse-mode="width"
      :collapsed-width="64"
      :width="200"
      :collapsed="collapsed"
      show-trigger
      @collapse="collapsed = true"
      @expand="collapsed = false"
      class="app-sider"
    >
      <div class="brand" :class="{ collapsed }">
        <span class="brand-mark">OCG</span>
        <span v-if="!collapsed" class="brand-name">Manager</span>
      </div>
      <n-menu
        :collapsed="collapsed"
        :collapsed-width="64"
        :collapsed-icon-size="22"
        :options="menuOptions"
        :value="activeKey"
        @update:value="activeKey = $event"
      />
    </n-layout-sider>
    <n-layout>
      <n-layout-header bordered class="app-header">
        <div class="header-left">
          <span class="header-title">{{ currentTitle }}</span>
        </div>
        <div class="header-right">
          <span class="gw-status" :class="gatewayRunning ? 'on' : 'off'">
            <span class="gw-dot" />
            Gateway {{ gatewayRunning ? "运行中" : "已停止" }}
          </span>
        </div>
      </n-layout-header>
      <n-layout-content class="app-content">
          <Dashboard v-if="activeKey === 'dashboard'" />
          <Accounts v-else-if="activeKey === 'accounts'" />
          <Logs v-else-if="activeKey === 'logs'" />
          <Settings v-else-if="activeKey === 'settings'" />
      </n-layout-content>
    </n-layout>
  </n-layout>
  <RemoteWizard
    v-if="showWizard"
    :show="showWizard"
    :config="wizardConfig"
    @done="showWizard = false"
  />
  </n-message-provider>
</template>

<script setup lang="ts">
import { h, ref, onMounted, computed } from "vue";
import {
  NLayout,
  NLayoutSider,
  NLayoutHeader,
  NLayoutContent,
  NMenu,
  NMessageProvider,
} from "naive-ui";
import {
  DashboardOutlined,
  KeyOutlined,
  FileTextOutlined,
  SettingOutlined,
} from "@vicons/antd";
import { Component } from "vue";
import Dashboard from "./views/Dashboard.vue";
import Accounts from "./views/Accounts.vue";
import Logs from "./views/Logs.vue";
import Settings from "./views/Settings.vue";
import RemoteWizard from "./components/RemoteWizard.vue";
import { tauriApi, AppConfig, GatewayStatus } from "./api/tauri";

const collapsed = ref(false);
const activeKey = ref("dashboard");
const gatewayRunning = ref(false);
const showWizard = ref(false);
// ponytail: wizardConfig is only the FALLBACK for when getSettings()
// fails on boot. Keep it in sync with AppConfig::default() in
// crates/ocg-core/src/models.rs. The Rust Default is the source of truth.
const wizardConfig = ref<AppConfig>({
  gateway_port: 9042,
  gateway_key: "",
  selection_strategy: "sequential",
  upstream_base_url: "https://opencode.ai/zen/go",
  auto_start: false,
  remote: { url: "", token: "" },
});

function renderIcon(icon: Component) {
  return () => h(icon);
}

const menuOptions = [
  {
    label: "仪表盘",
    key: "dashboard",
    icon: renderIcon(DashboardOutlined),
  },
  {
    label: "账号管理",
    key: "accounts",
    icon: renderIcon(KeyOutlined),
  },
  {
    label: "日志",
    key: "logs",
    icon: renderIcon(FileTextOutlined),
  },
  {
    label: "设置",
    key: "settings",
    icon: renderIcon(SettingOutlined),
  },
];

const titleMap: Record<string, string> = {
  dashboard: "仪表盘",
  accounts: "账号管理",
  logs: "日志",
  settings: "设置",
};
const currentTitle = computed(() => titleMap[activeKey.value] ?? "OCG Manager");

onMounted(async () => {
  try {
    const status: GatewayStatus = await tauriApi.getGatewayStatus();
    gatewayRunning.value = status.running;
  } catch (e) {
    console.error("failed to get gateway status", e);
  }
  try {
    const cfg = await tauriApi.getSettings();
    wizardConfig.value = cfg;
    const rs = await tauriApi.getRemoteStatus();
    // ponytail: only show wizard when the user has never completed the
    // bootstrap path. They can still clear remote.url in Settings later
    // and re-run it; clearing the bootstrapped flag is a separate, future
    // maintenance task.
    // ponytail: show wizard whenever the remote URL is empty, regardless
    // of the bootstrapped flag. The flag is a one-way switch from the old
    // design; if the user clears the URL in Settings they should be able
    // to re-run the wizard. The wizard itself is harmless on a no-op save.
    showWizard.value = rs.url === "";
  } catch (e) {
    console.error("failed to load remote status", e);
  }
});
</script>

<style scoped>
.app-sider {
  background: var(--n-color, #fff);
}
.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 56px;
  padding: 0 18px;
  border-bottom: 1px solid var(--n-border-color, rgba(0, 0, 0, 0.06));
  overflow: hidden;
}
.brand.collapsed {
  padding: 0;
  justify-content: center;
}
.brand-mark {
  font-weight: 800;
  font-size: 15px;
  letter-spacing: 0.04em;
  background: linear-gradient(135deg, #18a058 0%, #2080f0 100%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}
.brand-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--n-text-color, #222);
}

.app-header {
  height: 56px;
  padding: 0 20px;
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.header-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--n-text-color, #222);
}
.gw-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  padding: 4px 12px;
  border-radius: 999px;
}
.gw-status.on {
  background: rgba(24, 160, 88, 0.12);
  color: #18a058;
}
.gw-status.off {
  background: rgba(208, 48, 80, 0.12);
  color: #d03050;
}
.gw-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}
.gw-status.on .gw-dot {
  box-shadow: 0 0 0 0 rgba(24, 160, 88, 0.5);
  animation: pulse 1.8s infinite;
}
@keyframes pulse {
  0% { box-shadow: 0 0 0 0 rgba(24, 160, 88, 0.5); }
  70% { box-shadow: 0 0 0 6px rgba(24, 160, 88, 0); }
  100% { box-shadow: 0 0 0 0 rgba(24, 160, 88, 0); }
}

.app-content {
  padding: 20px;
  overflow-y: auto;
}
</style>
