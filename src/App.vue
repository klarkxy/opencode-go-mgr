<template>
  <n-layout has-sider style="height: 100%">
    <n-layout-sider
      bordered
      collapse-mode="width"
      :collapsed-width="64"
      :width="180"
      :collapsed="collapsed"
      show-trigger
      @collapse="collapsed = true"
      @expand="collapsed = false"
    >
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
      <n-layout-header bordered style="padding: 16px; display: flex; align-items: center; justify-content: space-between">
        <n-h3 style="margin: 0">OCG Manager</n-h3>
        <n-tag :type="gatewayRunning ? 'success' : 'error'">
          Gateway {{ gatewayRunning ? "运行中" : "已停止" }}
        </n-tag>
      </n-layout-header>
      <n-layout-content style="padding: 16px; overflow-y: auto">
        <n-message-provider>
          <Dashboard v-if="activeKey === 'dashboard'" />
          <Accounts v-else-if="activeKey === 'accounts'" />
          <Logs v-else-if="activeKey === 'logs'" />
          <Settings v-else-if="activeKey === 'settings'" />
        </n-message-provider>
      </n-layout-content>
    </n-layout>
  </n-layout>
  <RemoteWizard
    v-if="showWizard"
    :show="showWizard"
    :config="wizardConfig"
    @done="showWizard = false"
  />
</template>

<script setup lang="ts">
import { h, ref, onMounted } from "vue";
import {
  NLayout,
  NLayoutSider,
  NLayoutHeader,
  NLayoutContent,
  NMenu,
  NH3,
  NTag,
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
