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
import { tauriApi, GatewayStatus } from "./api/tauri";

const collapsed = ref(false);
const activeKey = ref("dashboard");
const gatewayRunning = ref(false);

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
});
</script>
