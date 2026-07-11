<template>
  <n-config-provider
    class="app-provider"
    :theme="naiveTheme"
    :theme-overrides="themeOverrides"
  >
    <n-global-style />

    <main v-if="authState !== 'ready'" class="auth-page">
      <section class="auth-panel">
        <div class="auth-brand"><span>OCG</span> Co-pilot</div>
        <p class="auth-kicker">OpenCode-Go Console</p>
        <h1>{{ authState === "register" ? "创建管理员" : "管理员登录" }}</h1>
        <p v-if="authState === 'checking'" class="auth-copy">正在连接管理服务…</p>
        <n-form
          v-else
          class="auth-form"
          :model="authFormModel"
          label-placement="top"
          :show-feedback="false"
          @submit.prevent="submitAuth"
        >
          <n-form-item label="用户名">
            <n-input
              v-model:value="authUsername"
              :input-props="{ 'aria-label': '用户名' }"
              autocomplete="username"
              placeholder="admin"
              autofocus
            />
          </n-form-item>
          <n-form-item label="密码">
            <n-input
              v-model:value="authPassword"
              :input-props="{ 'aria-label': '密码' }"
              type="password"
              :autocomplete="authState === 'register' ? 'new-password' : 'current-password'"
              placeholder="至少 8 个字符"
              show-password-on="click"
            />
          </n-form-item>
          <n-form-item v-if="authState === 'register'" label="确认密码">
            <n-input
              v-model:value="authPasswordConfirm"
              :input-props="{ 'aria-label': '确认密码' }"
              type="password"
              autocomplete="new-password"
              placeholder="再次输入密码"
              show-password-on="click"
            />
          </n-form-item>
          <p v-if="authError" class="auth-error" role="alert">{{ authError }}</p>
          <n-button
            attr-type="submit"
            type="primary"
            block
            :disabled="!authUsername.trim() || !authPassword"
          >
            {{ authState === "register" ? "创建并进入" : "登录" }}
          </n-button>
        </n-form>
      </section>
      <img :src="characterImage" alt="" class="auth-character" aria-hidden="true" />
    </main>

    <n-message-provider v-else>
      <n-layout has-sider class="app-shell">
        <n-layout-sider
          bordered
          collapse-mode="width"
          :collapsed-width="64"
          :width="208"
          :collapsed="collapsed"
          show-trigger
          class="app-sider"
          :class="{ 'app-sider--collapsed': collapsed }"
          @collapse="collapsed = true"
          @expand="collapsed = false"
        >
          <div class="brand" :class="{ collapsed }">
            <span class="brand-mark">OCG</span>
            <span v-if="!collapsed" class="brand-name">Co-pilot</span>
          </div>
          <n-menu
            :collapsed="collapsed"
            :collapsed-width="64"
            :collapsed-icon-size="22"
            :options="menuOptions"
            :value="activeKey"
            @update:value="selectView"
          />
        </n-layout-sider>

        <n-layout class="app-main">
          <n-layout-header bordered class="app-header">
            <div class="desktop-title">{{ currentTitle }}</div>
            <div class="mobile-nav">
              <span class="brand-mark">OCG</span>
              <n-menu
                mode="horizontal"
                responsive
                :options="menuOptions"
                :value="activeKey"
                @update:value="selectView"
              />
            </div>
            <div class="header-actions">
              <n-tooltip trigger="hover">
                <template #trigger>
                  <n-button
                    circle
                    quaternary
                    :aria-label="`主题：${themeModeLabel}`"
                    @click="cycleThemeMode"
                  >
                    <template #icon><n-icon :component="themeIcon" /></template>
                  </n-button>
                </template>
                主题：{{ themeModeLabel }} · 点击切换
              </n-tooltip>
              <n-tooltip v-if="!localMode" trigger="hover">
                <template #trigger>
                  <n-button circle quaternary aria-label="退出登录" @click="logout">
                    <template #icon><n-icon :component="LogoutOutlined" /></template>
                  </n-button>
                </template>
                退出登录
              </n-tooltip>
            </div>
          </n-layout-header>

          <n-layout-content class="app-content">
            <Dashboard v-if="activeKey === 'dashboard'" />
            <Accounts v-else-if="activeKey === 'accounts'" />
            <Logs v-else-if="activeKey === 'logs'" />
            <Settings
              v-else-if="activeKey === 'settings'"
              :theme-mode="themeMode"
              @update:theme-mode="themeMode = $event"
            />
          </n-layout-content>
        </n-layout>
      </n-layout>
    </n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed, h, onMounted, onUnmounted, ref, watch } from "vue";
import type { Component } from "vue";
import {
  NButton,
  NConfigProvider,
  NForm,
  NFormItem,
  NGlobalStyle,
  NIcon,
  NInput,
  NLayout,
  NLayoutContent,
  NLayoutHeader,
  NLayoutSider,
  NMenu,
  NMessageProvider,
  NTooltip,
  darkTheme,
  useOsTheme,
} from "naive-ui";
import type { GlobalThemeOverrides } from "naive-ui";
import {
  BulbOutlined,
  DashboardOutlined,
  DesktopOutlined,
  FileTextOutlined,
  KeyOutlined,
  LogoutOutlined,
  SettingOutlined,
  StarOutlined,
} from "@vicons/antd";
import Dashboard from "./views/Dashboard.vue";
import Accounts from "./views/Accounts.vue";
import Logs from "./views/Logs.vue";
import Settings from "./views/Settings.vue";
import { DASHBOARD_AUTH_REQUIRED_EVENT, tauriApi } from "./api/tauri";
import { readThemeMode, THEME_STORAGE_KEY } from "./theme";
import type { ThemeMode } from "./theme";

type ViewKey = "dashboard" | "accounts" | "logs" | "settings";

const views = new Set<ViewKey>(["dashboard", "accounts", "logs", "settings"]);
const osTheme = useOsTheme();
const collapsed = ref(false);
const activeKey = ref<ViewKey>(readView());
const themeMode = ref<ThemeMode>(readThemeMode(window.localStorage));
const characterImage = new URL("../assets/opencode-mascot.png", import.meta.url).href;
const authUsername = ref("");
const authPassword = ref("");
const authPasswordConfirm = ref("");
const authError = ref("");
const authState = ref<"checking" | "login" | "register" | "ready">("checking");
const localMode = ref(false);

const authFormModel = computed(() => ({
  username: authUsername.value,
  password: authPassword.value,
  passwordConfirm: authPasswordConfirm.value,
}));

const resolvedTheme = computed<"light" | "dark">(() => {
  if (themeMode.value !== "system") return themeMode.value;
  return osTheme.value === "dark" ? "dark" : "light";
});
const naiveTheme = computed(() => resolvedTheme.value === "dark" ? darkTheme : null);

const lightOverrides: GlobalThemeOverrides = {
  common: {
    bodyColor: "#F6F6FA",
    cardColor: "#FFFFFF",
    modalColor: "#FFFFFF",
    popoverColor: "#FFFFFF",
    tableColor: "#FFFFFF",
    primaryColor: "#6257C8",
    primaryColorHover: "#756ADB",
    primaryColorPressed: "#4F45AD",
    primaryColorSuppl: "#756ADB",
    textColorBase: "#181820",
    textColor1: "#181820",
    textColor2: "#5E5D6A",
    textColor3: "#7B7987",
    successColor: "#16845B",
    warningColor: "#A85F00",
    errorColor: "#C33B55",
    infoColor: "#2F6FD4",
    borderColor: "#E3E1EA",
    dividerColor: "#E9E7EE",
    borderRadius: "10px",
    fontFamily: '"Segoe UI Variable Text", "Noto Sans SC", "Microsoft YaHei UI", sans-serif',
    fontFamilyMono: '"Cascadia Mono", Consolas, monospace',
  },
};
const darkOverrides: GlobalThemeOverrides = {
  common: {
    bodyColor: "#111116",
    cardColor: "#1A1A22",
    modalColor: "#1A1A22",
    popoverColor: "#23232D",
    tableColor: "#1A1A22",
    primaryColor: "#A99CFF",
    primaryColorHover: "#BCB2FF",
    primaryColorPressed: "#8F81E8",
    primaryColorSuppl: "#BCB2FF",
    textColorBase: "#F4F2FA",
    textColor1: "#F4F2FA",
    textColor2: "#C7C3D0",
    textColor3: "#9994A6",
    successColor: "#56C596",
    warningColor: "#E7AE55",
    errorColor: "#F08095",
    infoColor: "#74A6F6",
    borderColor: "#32313C",
    dividerColor: "#2D2C36",
    borderRadius: "10px",
    fontFamily: '"Segoe UI Variable Text", "Noto Sans SC", "Microsoft YaHei UI", sans-serif',
    fontFamilyMono: '"Cascadia Mono", Consolas, monospace',
  },
};
const themeOverrides = computed(() => resolvedTheme.value === "dark" ? darkOverrides : lightOverrides);

function renderIcon(icon: Component) {
  return () => h(icon);
}

const menuOptions = [
  { label: "仪表盘", key: "dashboard", icon: renderIcon(DashboardOutlined) },
  { label: "账号", key: "accounts", icon: renderIcon(KeyOutlined) },
  { label: "日志", key: "logs", icon: renderIcon(FileTextOutlined) },
  { label: "设置", key: "settings", icon: renderIcon(SettingOutlined) },
];
const titleMap: Record<ViewKey, string> = {
  dashboard: "仪表盘",
  accounts: "账号管理",
  logs: "日志",
  settings: "设置",
};
const currentTitle = computed(() => titleMap[activeKey.value]);
const themeModeLabel = computed(() => ({ system: "跟随系统", light: "浅色", dark: "深色" })[themeMode.value]);
const themeIcon = computed<Component>(() => ({
  system: DesktopOutlined,
  light: BulbOutlined,
  dark: StarOutlined,
})[themeMode.value]);

function readView(): ViewKey {
  const value = new URLSearchParams(window.location.search).get("view");
  return value && views.has(value as ViewKey) ? value as ViewKey : "dashboard";
}

function selectView(key: string) {
  if (views.has(key as ViewKey)) activeKey.value = key as ViewKey;
}

function syncView(view: ViewKey) {
  const url = new URL(window.location.href);
  url.searchParams.set("view", view);
  window.history.replaceState(null, "", url);
}

function onPopState() {
  activeKey.value = readView();
}

function cycleThemeMode() {
  const order: ThemeMode[] = ["system", "light", "dark"];
  themeMode.value = order[(order.indexOf(themeMode.value) + 1) % order.length];
}

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
    authState.value = status.authenticated ? "ready" : status.initialized ? "login" : "register";
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
    if (mode === "register") await tauriApi.registerAdmin(username, authPassword.value);
    else await tauriApi.loginAdmin(username, authPassword.value);
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

watch(activeKey, syncView);
watch(themeMode, (value) => window.localStorage.setItem(THEME_STORAGE_KEY, value));
watch(resolvedTheme, (value) => { document.documentElement.dataset.theme = value; }, { immediate: true });

onMounted(() => {
  window.addEventListener(DASHBOARD_AUTH_REQUIRED_EVENT, onAuthRequired);
  window.addEventListener("popstate", onPopState);
  void loadAuthStatus();
});

onUnmounted(() => {
  window.removeEventListener(DASHBOARD_AUTH_REQUIRED_EVENT, onAuthRequired);
  window.removeEventListener("popstate", onPopState);
});
</script>

<style scoped>
.app-provider,
.app-shell,
.app-main {
  height: 100%;
}

.auth-page {
  position: relative;
  min-height: 100%;
  overflow: hidden;
  display: flex;
  align-items: center;
  padding: clamp(24px, 7vw, 96px);
  background:
    radial-gradient(circle at 22% 18%, var(--ocg-primary-soft), transparent 34%),
    var(--ocg-canvas);
}
.auth-panel {
  position: relative;
  z-index: 2;
  width: min(408px, 100%);
  padding: 32px;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-lg);
}
.auth-brand,
.brand-name,
.desktop-title {
  font-family: "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.auth-brand {
  font-size: 19px;
  font-weight: 700;
}
.auth-brand span,
.brand-mark {
  color: var(--ocg-primary);
}
.auth-kicker {
  margin: 20px 0 6px;
  color: var(--ocg-primary);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}
.auth-panel h1 {
  margin: 0 0 22px;
  font-family: "Bahnschrift", "Segoe UI Variable Display", sans-serif;
  font-size: 29px;
}
.auth-copy {
  color: var(--ocg-muted);
}
.auth-form :deep(.n-form-item) {
  margin-bottom: 12px;
}
.auth-error {
  margin: 0 0 12px;
  color: var(--ocg-error);
  font-size: 13px;
}
.auth-character {
  position: absolute;
  right: clamp(-28px, 4vw, 92px);
  bottom: -54px;
  height: min(94vh, 980px);
  max-width: 60vw;
  object-fit: contain;
  filter: drop-shadow(0 24px 30px rgba(27, 23, 52, 0.16));
  pointer-events: none;
  user-select: none;
}

.app-sider {
  background: var(--ocg-surface);
}
.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 58px;
  padding: 0 18px;
  border-bottom: 1px solid var(--ocg-border);
  overflow: hidden;
}
.brand.collapsed {
  justify-content: center;
  padding: 0;
}
.brand-mark {
  flex: 0 0 auto;
  font: 800 15px/1 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
  letter-spacing: 0.04em;
}
.brand-name {
  color: var(--ocg-ink);
  font-size: 14px;
  font-weight: 650;
}
.app-header {
  height: 58px;
  padding: 0 20px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: color-mix(in srgb, var(--ocg-surface) 94%, transparent);
}
.desktop-title {
  color: var(--ocg-ink);
  font-size: 16px;
  font-weight: 650;
}
.header-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}
.mobile-nav {
  display: none;
  min-width: 0;
  align-items: center;
  gap: 8px;
}
.app-content {
  padding: 24px;
  overflow-y: auto;
  background: var(--ocg-canvas);
}

@media (max-width: 1023px) {
  .app-sider {
    display: none;
  }
  .desktop-title {
    display: none;
  }
  .mobile-nav {
    display: flex;
    flex: 1 1 auto;
  }
  .mobile-nav :deep(.n-menu) {
    flex: 1 1 auto;
    min-width: 0;
  }
  .app-header {
    padding: 0 12px;
  }
  .app-content {
    padding: 16px;
  }
}

@media (max-width: 640px) {
  .auth-page {
    padding: 16px;
  }
  .auth-panel {
    padding: 24px 20px;
  }
  .auth-character {
    right: -190px;
    max-width: none;
    opacity: 0.14;
  }
  .mobile-nav > .brand-mark {
    display: none;
  }
  .app-content {
    padding: 12px;
  }
}
</style>
