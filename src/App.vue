<template>
  <main v-if="authState !== 'ready'" class="auth-page">
    <section class="auth-panel">
      <div class="auth-brand"><span>OCG</span> Manager</div>
      <p class="auth-kicker">OpenCode-Go 多账号管理器</p>
      <h1>{{ authState === "register" ? "创建管理员" : "管理员登录" }}</h1>
      <p class="auth-copy">
        {{
          authState === "checking"
            ? "正在连接管理服务…"
            : authState === "register"
              ? "首次使用，请创建管理员账号。创建后将关闭注册入口。"
              : "使用管理员账号进入管理面板。"
        }}
      </p>
      <form v-if="authState !== 'checking'" class="auth-form" @submit.prevent="submitAuth">
        <label for="admin-username">用户名</label>
        <input
          id="admin-username"
          v-model="authUsername"
          type="text"
          autocomplete="username"
          placeholder="admin"
          autofocus
        />
        <label for="admin-password">密码</label>
        <input
          id="admin-password"
          v-model="authPassword"
          type="password"
          :autocomplete="authState === 'register' ? 'new-password' : 'current-password'"
          placeholder="至少 8 个字符"
        />
        <template v-if="authState === 'register'">
          <label for="admin-password-confirm">确认密码</label>
          <input
            id="admin-password-confirm"
            v-model="authPasswordConfirm"
            type="password"
            autocomplete="new-password"
            placeholder="再次输入密码"
          />
        </template>
        <p v-if="authError" class="auth-error" role="alert">{{ authError }}</p>
        <button type="submit" :disabled="!authUsername.trim() || !authPassword">
          {{ authState === "register" ? "创建并进入" : "登录" }}
        </button>
      </form>
    </section>
    <img :src="characterImage" alt="" class="auth-character" />
  </main>

  <n-message-provider v-else>
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
      :class="{ 'app-sider--collapsed': collapsed }"
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
        <button v-if="!localMode" class="logout-button" type="button" @click="logout">
          退出登录
        </button>
      </n-layout-header>
      <n-layout-content class="app-content">
          <Dashboard v-if="activeKey === 'dashboard'" />
          <Accounts v-else-if="activeKey === 'accounts'" />
          <Logs v-else-if="activeKey === 'logs'" />
          <Settings v-else-if="activeKey === 'settings'" />
      </n-layout-content>
    </n-layout>
  </n-layout>
  </n-message-provider>
</template>

<script setup lang="ts">
import { h, ref, onMounted, onUnmounted, computed } from "vue";
import type { Component } from "vue";
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
import Dashboard from "./views/Dashboard.vue";
import Accounts from "./views/Accounts.vue";
import Logs from "./views/Logs.vue";
import Settings from "./views/Settings.vue";
import {
  DASHBOARD_AUTH_REQUIRED_EVENT,
  tauriApi,
} from "./api/tauri";

const collapsed = ref(false);
const activeKey = ref("dashboard");
const characterImage = new URL("../assets/opencode娘.png", import.meta.url).href;
const authUsername = ref("");
const authPassword = ref("");
const authPasswordConfirm = ref("");
const authError = ref("");
const authState = ref<"checking" | "login" | "register" | "ready">("checking");
const localMode = ref(false);

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

function onAuthRequired(event: Event) {
  authState.value = "login";
  authPassword.value = "";
  authPasswordConfirm.value = "";
  authError.value = (event as CustomEvent<string>).detail || "请重新登录";
}

async function loadAuthStatus() {
  authState.value = "checking";
  try {
    const status = await tauriApi.getAuthStatus();
    localMode.value = status.local;
    authError.value = "";
    authState.value = status.authenticated
      ? "ready"
      : status.initialized
        ? "login"
        : "register";
  } catch (e) {
    authState.value = "login";
    authError.value = `连接失败: ${e}`;
  }
}

async function submitAuth() {
  const mode = authState.value;
  const username = authUsername.value.trim();
  if (!username || !authPassword.value) return;
  if (mode === "register" && authPassword.value !== authPasswordConfirm.value) {
    authError.value = "两次输入的密码不一致";
    return;
  }
  authState.value = "checking";
  try {
    if (mode === "register") {
      await tauriApi.registerAdmin(username, authPassword.value);
    } else {
      await tauriApi.loginAdmin(username, authPassword.value);
    }
    authPassword.value = "";
    authPasswordConfirm.value = "";
    authError.value = "";
    authState.value = "ready";
  } catch (e) {
    authPassword.value = "";
    authPasswordConfirm.value = "";
    authError.value = e instanceof Error ? e.message : String(e);
    authState.value = mode;
  }
}

async function logout() {
  await tauriApi.logoutAdmin();
  authPassword.value = "";
  authPasswordConfirm.value = "";
  authError.value = "";
  authState.value = "login";
}

onMounted(() => {
  window.addEventListener(DASHBOARD_AUTH_REQUIRED_EVENT, onAuthRequired);
  void loadAuthStatus();
});

onUnmounted(() => {
  window.removeEventListener(DASHBOARD_AUTH_REQUIRED_EVENT, onAuthRequired);
});
</script>

<style scoped>
.auth-page {
  position: relative;
  min-height: 100%;
  overflow: hidden;
  display: flex;
  align-items: center;
  padding: clamp(28px, 7vw, 96px);
  background:
    radial-gradient(circle at 24% 24%, rgba(24, 160, 88, 0.12), transparent 34%),
    linear-gradient(120deg, #f7fbfa 0%, #f7f8fc 48%, #eef1f7 100%);
}
.auth-panel {
  position: relative;
  z-index: 1;
  width: min(420px, 100%);
  padding: 36px;
  border: 1px solid rgba(255, 255, 255, 0.9);
  border-radius: 20px;
  background: rgba(255, 255, 255, 0.84);
  box-shadow: 0 24px 80px rgba(24, 38, 56, 0.12);
  backdrop-filter: blur(16px);
}
.auth-brand {
  font-size: 18px;
  font-weight: 700;
}
.auth-brand span {
  color: #18a058;
}
.auth-kicker {
  margin: 22px 0 6px;
  color: #18a058;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}
.auth-panel h1 {
  margin: 0;
  font-size: 30px;
  line-height: 1.25;
}
.auth-copy {
  margin: 12px 0 24px;
  color: #667085;
  line-height: 1.7;
}
.auth-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.auth-form label {
  font-size: 13px;
  font-weight: 600;
}
.auth-form input {
  width: 100%;
  height: 44px;
  padding: 0 14px;
  border: 1px solid #d9dde5;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.94);
  font: inherit;
  outline: none;
}
.auth-form input:focus {
  border-color: #18a058;
  box-shadow: 0 0 0 3px rgba(24, 160, 88, 0.12);
}
.auth-form button {
  height: 44px;
  margin-top: 4px;
  border: 0;
  border-radius: 10px;
  background: #18a058;
  color: #fff;
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}
.auth-form button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}
.auth-error {
  margin: 0;
  color: #d03050;
  font-size: 13px;
}
.auth-character {
  position: absolute;
  right: clamp(-80px, -2vw, -12px);
  bottom: -18px;
  height: min(96vh, 1040px);
  max-width: 68vw;
  object-fit: contain;
  opacity: 0.94;
  mix-blend-mode: multiply;
  pointer-events: none;
  user-select: none;
}
@media (max-width: 760px) {
  .auth-page {
    padding: 20px;
  }
  .auth-panel {
    padding: 28px 22px;
  }
  .auth-character {
    right: -180px;
    max-width: none;
    opacity: 0.18;
  }
}

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
.app-content {
  padding: 20px;
  overflow-y: auto;
  background:
    linear-gradient(90deg, rgba(255, 255, 255, 0.96) 0%, rgba(255, 255, 255, 0.76) 52%, rgba(255, 255, 255, 0.24) 100%),
    url("../assets/opencode娘.png") right -24px bottom -56px / 520px auto no-repeat,
    var(--n-color, #fff);
}
.logout-button {
  padding: 6px 12px;
  border: 1px solid #d9dde5;
  border-radius: 8px;
  background: #fff;
  color: #475467;
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}
.logout-button:hover {
  border-color: #18a058;
  color: #18a058;
}
</style>
