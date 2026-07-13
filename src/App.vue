<template>
  <n-config-provider
    class="app-provider"
    :theme="naiveTheme"
    :theme-overrides="themeOverrides"
    :locale="naiveLocale"
    :date-locale="naiveDateLocale"
  >
    <n-global-style />

    <main v-if="authState !== 'ready'" class="auth-page">
      <section class="auth-panel">
        <div class="auth-panel-head">
          <div class="auth-brand"><span>OCG</span> Manager</div>
          <LocaleSwitcher />
        </div>
        <p class="auth-kicker">OpenCode-Go Console</p>
        <h1>{{ authState === "register" ? t("创建管理员") : t("管理员登录") }}</h1>
        <p v-if="authState === 'checking'" class="auth-copy">{{ t("正在连接管理服务…") }}</p>
        <n-form
          v-else
          class="auth-form"
          :model="authFormModel"
          label-placement="top"
          :show-feedback="false"
          @submit.prevent="submitAuth"
        >
          <n-form-item :label="t('用户名')">
            <n-input
              v-model:value="authUsername"
              :input-props="{ 'aria-label': t('用户名') }"
              autocomplete="username"
              placeholder="admin"
              autofocus
            />
          </n-form-item>
          <n-form-item :label="t('密码')">
            <n-input
              v-model:value="authPassword"
              :input-props="{ 'aria-label': t('密码') }"
              type="password"
              :autocomplete="authState === 'register' ? 'new-password' : 'current-password'"
              :placeholder="t('至少 8 个字符')"
              show-password-on="click"
            />
          </n-form-item>
          <n-form-item v-if="authState === 'register'" :label="t('确认密码')">
            <n-input
              v-model:value="authPasswordConfirm"
              :input-props="{ 'aria-label': t('确认密码') }"
              type="password"
              autocomplete="new-password"
              :placeholder="t('再次输入密码')"
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
            {{ authState === "register" ? t("创建并进入") : t("登录") }}
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
            <span v-if="!collapsed" class="brand-name">Manager</span>
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
              <LocaleSwitcher />
              <n-tooltip trigger="hover" :disabled="themeMenuShown">
                <template #trigger>
                  <n-dropdown
                    trigger="click"
                    :keyboard="false"
                    :show="themeMenuShown"
                    :options="themeMenuOptions"
                    :menu-props="themeMenuProps"
                    @select="selectTheme"
                    @update:show="updateThemeMenuShown"
                  >
                    <n-button
                      circle
                      quaternary
                      aria-controls="theme-menu"
                      aria-haspopup="menu"
                      :aria-expanded="themeMenuShown"
                      :aria-label="t('主题：{theme}', { theme: themeLabel })"
                    >
                      <template #icon><n-icon :component="BgColorsOutlined" /></template>
                    </n-button>
                  </n-dropdown>
                </template>
                {{ t("主题：{theme}", { theme: themeLabel }) }}
              </n-tooltip>
              <n-tooltip v-if="!localMode" trigger="hover">
                <template #trigger>
                  <n-button circle quaternary :aria-label="t('退出登录')" @click="logout">
                    <template #icon><n-icon :component="LogoutOutlined" /></template>
                  </n-button>
                </template>
                {{ t("退出登录") }}
              </n-tooltip>
            </div>
          </n-layout-header>

          <main class="app-content">
            <Dashboard v-if="activeKey === 'dashboard'" />
            <Accounts v-else-if="activeKey === 'accounts'" />
            <Applications v-else-if="activeKey === 'apps'" />
            <Logs v-else-if="activeKey === 'logs'" />
            <Settings
              v-else-if="activeKey === 'settings'"
              :theme-name="themeName"
              :resolved-theme="resolvedTheme"
              @update:theme-name="themeName = $event"
            />
          </main>
        </n-layout>
      </n-layout>
    </n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed, defineAsyncComponent, h, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import type { Component } from "vue";
import {
  NButton,
  NConfigProvider,
  NDropdown,
  NForm,
  NFormItem,
  NGlobalStyle,
  NIcon,
  NInput,
  NLayout,
  NLayoutHeader,
  NLayoutSider,
  NMenu,
  NMessageProvider,
  NTooltip,
  darkTheme,
  useOsTheme,
} from "naive-ui";
import type { DropdownMenuProps, DropdownOption } from "naive-ui";
import {
  AppstoreOutlined,
  BgColorsOutlined,
  CheckOutlined,
  DashboardOutlined,
  FileTextOutlined,
  KeyOutlined,
  LogoutOutlined,
  SettingOutlined,
} from "@vicons/antd";
import LocaleSwitcher from "./components/LocaleSwitcher.vue";
import { locale, naiveDateLocale, naiveLocale, t } from "./i18n/index.ts";
import type { MessageKey } from "./i18n/index.ts";
import { DASHBOARD_AUTH_REQUIRED_EVENT, DashboardRequestError, tauriApi } from "./api/tauri";
import {
  applyTheme,
  getThemeStorage,
  getThemeTokens,
  readTheme,
  resolveTheme,
  THEME_OPTIONS,
  toNaiveThemeOverrides,
  writeTheme,
} from "./theme";
import type { ThemeName } from "./theme";

type ViewKey = "dashboard" | "accounts" | "apps" | "logs" | "settings";

const Dashboard = defineAsyncComponent(() => import("./views/Dashboard.vue"));
const Accounts = defineAsyncComponent(() => import("./views/Accounts.vue"));
const Applications = defineAsyncComponent(() => import("./views/Applications.vue"));
const Logs = defineAsyncComponent(() => import("./views/Logs.vue"));
const Settings = defineAsyncComponent(() => import("./views/Settings.vue"));

const views = new Set<ViewKey>(["dashboard", "accounts", "apps", "logs", "settings"]);
const osTheme = useOsTheme();
const collapsed = ref(false);
const activeKey = ref<ViewKey>(readView());
const themeStorage = getThemeStorage();
const themeName = ref<ThemeName>(readTheme(themeStorage));
const themeMenuShown = ref(false);
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

const resolvedTheme = computed(() => resolveTheme(themeName.value, osTheme.value));
const themeTokens = computed(() => getThemeTokens(themeName.value, osTheme.value));
const naiveTheme = computed(() => resolvedTheme.value === "black" ? darkTheme : null);
const themeOverrides = computed(() => toNaiveThemeOverrides(themeTokens.value));

function renderIcon(icon: Component) {
  return () => h(icon);
}

const menuOptions = computed(() => [
  { label: t("仪表盘"), key: "dashboard", icon: renderIcon(DashboardOutlined) },
  { label: t("账号"), key: "accounts", icon: renderIcon(KeyOutlined) },
  { label: t("应用"), key: "apps", icon: renderIcon(AppstoreOutlined) },
  { label: t("日志"), key: "logs", icon: renderIcon(FileTextOutlined) },
  { label: t("设置"), key: "settings", icon: renderIcon(SettingOutlined) },
]);
const titleMap: Record<ViewKey, MessageKey> = {
  dashboard: "仪表盘",
  accounts: "账号管理",
  apps: "应用教程",
  logs: "日志",
  settings: "设置",
};
const currentTitle = computed(() => t(titleMap[activeKey.value]));
const themeNames = new Set<ThemeName>(THEME_OPTIONS.map(({ value }) => value));
const themeLabel = computed(() => {
  const selected = t((THEME_OPTIONS.find(({ value }) => value === themeName.value)?.label ?? "默认") as MessageKey);
  if (themeName.value !== "default") return selected;
  const resolved = t((THEME_OPTIONS.find(({ value }) => value === resolvedTheme.value)?.label ?? "皓白") as MessageKey);
  return t("默认 · {theme}", { theme: resolved });
});
const themeMenuOptions = computed<DropdownOption[]>(() => THEME_OPTIONS.map((option) => ({
  key: option.value,
  label: t(option.label as MessageKey),
  icon: () => h("span", {
    "aria-hidden": "true",
    style: {
      display: "inline-block",
      width: "16px",
      height: "16px",
      borderRadius: "50%",
      background: option.swatch,
      boxShadow: "inset 0 0 0 1px rgba(128, 128, 140, 0.45)",
    },
  }),
  extra: themeName.value === option.value
    ? () => h(NIcon, { component: CheckOutlined, size: 14, "aria-hidden": true })
    : undefined,
  props: {
    id: `theme-menu-option-${option.value}`,
    role: "menuitemradio",
    tabindex: -1,
    "aria-checked": themeName.value === option.value ? "true" : "false",
    onKeydown: (event: KeyboardEvent) => handleThemeMenuKeydown(event, option.value),
  },
})));
const themeMenuProps: DropdownMenuProps = () => ({
  id: "theme-menu",
  role: "menu",
  "aria-label": t("选择主题"),
});

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

function selectTheme(key: string | number) {
  if (typeof key === "string" && themeNames.has(key as ThemeName)) {
    themeName.value = key as ThemeName;
    if (themeMenuShown.value) {
      themeMenuShown.value = false;
      void nextTick(focusThemeTrigger);
    }
  }
}

async function updateThemeMenuShown(show: boolean) {
  themeMenuShown.value = show;
  if (!show) return;
  await nextTick();
  focusThemeMenuOption(themeName.value);
}

function focusThemeMenuOption(theme: ThemeName) {
  document.querySelector<HTMLElement>(`#theme-menu-option-${theme}`)?.focus();
}

function focusThemeTrigger() {
  document.querySelector<HTMLElement>('[aria-controls="theme-menu"]')?.focus();
}

function handleThemeMenuKeydown(event: KeyboardEvent, current: ThemeName) {
  const index = THEME_OPTIONS.findIndex(({ value }) => value === current);
  let nextIndex: number | undefined;
  if (event.key === "ArrowDown" || event.key === "ArrowRight") {
    nextIndex = (index + 1) % THEME_OPTIONS.length;
  } else if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
    nextIndex = (index - 1 + THEME_OPTIONS.length) % THEME_OPTIONS.length;
  } else if (event.key === "Home") {
    nextIndex = 0;
  } else if (event.key === "End") {
    nextIndex = THEME_OPTIONS.length - 1;
  } else if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    event.stopPropagation();
    selectTheme(current);
    return;
  } else if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    themeMenuShown.value = false;
    void nextTick(focusThemeTrigger);
    return;
  } else {
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  focusThemeMenuOption(THEME_OPTIONS[nextIndex].value);
}

function onAuthRequired(event: Event) {
  authState.value = "login";
  authPassword.value = "";
  authPasswordConfirm.value = "";
  authError.value = (event as CustomEvent<string>).detail || t("请重新登录");
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
    authError.value = t("连接失败: {error}", { error: String(e) });
  }
}

async function submitAuth() {
  const mode = authState.value;
  const username = authUsername.value.trim();
  if (!username || !authPassword.value) return;
  if (mode === "register" && [...username].length > 64) {
    authError.value = t("用户名需为 1 至 64 个字符");
    return;
  }
  const passwordLength = [...authPassword.value].length;
  if (mode === "register" && (passwordLength < 8 || passwordLength > 256)) {
    authError.value = t("密码需为 8 至 256 个字符");
    return;
  }
  if (mode === "register" && authPassword.value !== authPasswordConfirm.value) {
    authError.value = t("两次输入的密码不一致");
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
    let error = e instanceof Error ? e.message : String(e);
    if (e instanceof DashboardRequestError) {
      if (mode === "login" && e.status === 401) error = t("用户名或密码错误");
      if (mode === "register" && e.status === 409) error = t("管理员已经创建，请直接登录");
    }
    if (mode === "register") {
      const status = await tauriApi.getAuthStatus().catch(() => null);
      if (status?.initialized) {
        localMode.value = status.local;
        authError.value = error;
        authState.value = status.authenticated ? "ready" : "login";
        return;
      }
    }
    authError.value = error;
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
watch(locale, () => { authError.value = ""; });
watch(themeName, (value) => writeTheme(themeStorage, value));
watch([resolvedTheme, themeTokens], ([resolved, tokens]) => {
  applyTheme(document.documentElement, resolved, tokens);
}, { immediate: true });

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
    radial-gradient(circle at 82% 52%, var(--ocg-mascot-halo), transparent 34%),
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
.auth-panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.auth-brand span,
.brand-mark {
  color: var(--ocg-primary);
}
.auth-kicker {
  margin: 20px 0 6px;
  color: var(--ocg-primary);
  font-size: 16px;
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
  font-size: 16px;
}
.auth-character {
  position: absolute;
  right: clamp(-28px, 4vw, 92px);
  bottom: -54px;
  height: min(94vh, 980px);
  max-width: 60vw;
  object-fit: contain;
  filter:
    drop-shadow(0 0 1px var(--ocg-mascot-rim))
    drop-shadow(0 24px 30px rgba(27, 23, 52, 0.16));
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
  font: 800 16px/1 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
  letter-spacing: 0.04em;
}
.brand-name {
  color: var(--ocg-ink);
  font-size: 16px;
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
  height: calc(100% - 58px);
  min-width: 0;
  min-height: 0;
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
