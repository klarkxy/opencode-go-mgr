<template>
  <div class="settings-grid">
    <section class="settings-card" aria-labelledby="forwarding-title">
      <div class="settings-head">
        <div>
          <h2 id="forwarding-title"><n-icon class="section-icon" :component="SwapOutlined" aria-hidden="true" /> {{ t("转发") }}</h2>
        </div>
      </div>
      <n-form :model="config" label-placement="top" :show-feedback="false">
        <n-form-item :label="t('上游地址')">
          <n-input
            v-model:value="config.upstream_base_url"
            :input-props="{ 'aria-label': t('上游地址') }"
            placeholder="https://opencode.ai/zen/go"
          />
        </n-form-item>
        <div class="downstream-grid">
          <n-form-item
            :label="t('下游访问根地址（可选）')"
            :show-feedback="true"
            :validation-status="clientRootPreview.status"
            :feedback="clientRootPreview.feedback"
          >
            <div class="client-root-field">
              <n-input
                v-model:value="clientRootInputValue"
                :readonly="config.client_root_url_from_env"
                :clearable="!config.client_root_url_from_env && !!config.client_root_url"
                :placeholder="config.client_root_url_from_env ? '' : automaticClientRootUrls.rootUrl"
                class="mono"
                :input-props="{
                  'aria-label': t('下游访问根地址（可选）'),
                  'aria-describedby': 'client-root-help',
                }"
                @blur="normalizeClientRootInput"
              />
              <p id="client-root-help">
                <template v-if="config.client_root_url_from_env">
                  {{ t("由环境变量 OCG_CLIENT_ROOT_URL 管理；修改环境变量并重启后生效。") }}<br />
                </template>
                <span v-else-if="!config.client_root_url.trim()" class="sr-only">
                  {{ automaticClientRootFeedback }}
                </span>
              </p>
            </div>
          </n-form-item>
          <n-form-item label="Key">
            <div class="key-stack">
              <div class="key-field">
                <div class="key-display" :aria-label="t('已脱敏 Key')">
                  <code>{{ maskedSettingsKey }}</code>
                </div>
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button
                      circle
                      quaternary
                      :aria-label="t('复制 Key')"
                      :disabled="!config.gateway_key"
                      @click="copyKey"
                    >
                      <template #icon>
                        <n-icon :component="keyCopied ? CheckOutlined : CopyOutlined" />
                      </template>
                    </n-button>
                  </template>
                  {{ t("复制 Key") }}
                </n-tooltip>
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button
                      circle
                      quaternary
                      :aria-label="t('设置自定义 Key')"
                      :disabled="saving || regenerating"
                      @click="startGatewayKeyEdit"
                    >
                      <template #icon><n-icon :component="EditOutlined" /></template>
                    </n-button>
                  </template>
                  {{ t("设置自定义 Key") }}
                </n-tooltip>
                <n-popconfirm
                  :positive-text="t('生成新 Key')"
                  :negative-text="t('取消')"
                  @positive-click="regenerateKey"
                >
                  <template #trigger>
                    <n-tooltip trigger="hover">
                      <template #trigger>
                        <n-button
                          circle
                          quaternary
                          :aria-label="t('刷新 Key')"
                          :loading="regenerating"
                          :disabled="saving"
                        >
                          <template #icon><n-icon :component="ReloadOutlined" /></template>
                        </n-button>
                      </template>
                      {{ t("刷新 Key") }}
                    </n-tooltip>
                  </template>
                  {{ t("旧 Key 将立即失效，继续生成新 Key？") }}
                </n-popconfirm>
              </div>
              <div v-if="editingGatewayKey" class="key-editor">
                <n-input
                  v-model:value="gatewayKeyDraft"
                  type="password"
                  class="mono"
                  :input-props="{ 'aria-label': t('新 Key') }"
                  :placeholder="t('输入新 Key')"
                />
                <n-button size="small" secondary @click="cancelGatewayKeyEdit">{{ t("取消") }}</n-button>
                <n-button size="small" type="primary" :loading="saving" @click="saveGatewayKey">{{ t("保存 Key") }}</n-button>
              </div>
            </div>
          </n-form-item>
        </div>
        <div
          v-if="config.auto_start_supported"
          class="settings-subsection"
          aria-labelledby="startup-title"
        >
          <h3 id="startup-title">{{ t("开机启动") }}</h3>
          <n-switch
            :value="config.auto_start"
            @update:value="handleAutoStartToggle"
            :aria-label="t('随 Windows 登录自动启动 OCG Manager')"
            :disabled="!loaded || saving || regenerating"
            :loading="saving"
          >
            <template #checked>{{ t("开启") }}</template>
            <template #unchecked>{{ t("关闭") }}</template>
          </n-switch>
        </div>
        <div class="settings-subsection" aria-labelledby="request-timeout-title">
          <h3 id="request-timeout-title">{{ t("请求超时") }}</h3>
          <n-form-item :label="t('连接超时')">
            <div class="timeout-field">
              <n-input-number
                v-model:value="config.connect_timeout_secs"
                :min="1"
                :max="300"
                :precision="0"
                :input-props="{ 'aria-label': t('连接超时（秒）') }"
              >
                <template #suffix>{{ t("秒") }}</template>
              </n-input-number>
              <span class="field-caption">{{ t("建立上游连接的初始超时（秒）") }}</span>
            </div>
          </n-form-item>
          <n-form-item :label="t('非流式总超时')">
            <div class="timeout-field">
              <n-input-number
                v-model:value="config.non_stream_timeout_secs"
                :min="1"
                :max="3600"
                :precision="0"
                :input-props="{ 'aria-label': t('非流式总超时（秒）') }"
              >
                <template #suffix>{{ t("秒") }}</template>
              </n-input-number>
              <span class="field-caption">{{ t("非流式请求从发起到完整响应的总超时（秒）") }}</span>
            </div>
          </n-form-item>
          <n-form-item :label="t('流式空闲超时')">
            <div class="timeout-field">
              <n-input-number
                v-model:value="config.stream_idle_timeout_secs"
                :min="1"
                :max="3600"
                :precision="0"
                :input-props="{ 'aria-label': t('流式空闲超时（秒）') }"
              >
                <template #suffix>{{ t("秒") }}</template>
              </n-input-number>
              <span class="field-caption">{{ t("流式响应两次数据块之间的最大空闲时间（秒）") }}</span>
            </div>
          </n-form-item>
        </div>
      </n-form>
      <n-button
        type="primary"
        :loading="saving"
        :disabled="!loaded || regenerating || clientRootPreview.status === 'error' || editingGatewayKey"
        @click="saveSettings"
      >{{ t("保存设置") }}</n-button>
    </section>

    <div class="settings-side">
      <section class="settings-card" aria-labelledby="appearance-title">
        <div class="settings-head">
          <div>
            <h2 id="appearance-title"><n-icon class="section-icon" :component="BgColorsOutlined" aria-hidden="true" /> {{ t("外观") }}</h2>
            <p>{{ t("当前：{theme}", { theme: themeLabel }) }}</p>
          </div>
        </div>
        <div class="theme-grid" role="group" :aria-label="t('选择主题')">
          <button
            v-for="option in THEME_OPTIONS"
            :key="option.value"
            type="button"
            class="theme-option"
            :class="{ 'theme-option--selected': themeName === option.value }"
            :aria-pressed="themeName === option.value"
            @click="emit('update:themeName', option.value)"
          >
            <span
              class="theme-swatch"
              :class="{
                'theme-swatch--default': option.value === 'default',
                'theme-swatch--white': option.value === 'white',
              }"
              :style="{ background: option.swatch }"
              aria-hidden="true"
            />
            <span>{{ t(option.label as MessageKey) }}</span>
            <n-icon
              v-if="themeName === option.value"
              class="theme-check"
              :component="CheckOutlined"
              aria-hidden="true"
            />
          </button>
        </div>
      </section>

      <section class="settings-card" aria-labelledby="update-title">
        <div class="settings-head">
          <div>
            <h2 id="update-title"><n-icon class="section-icon" :component="CloudSyncOutlined" aria-hidden="true" /> {{ t("检查更新") }}</h2>
          </div>
        </div>
        <n-button
          type="primary"
          :loading="checkingUpdate"
          :disabled="checkingUpdate"
          @click="checkForUpdate"
        >{{ checkingUpdate ? t("正在检查更新…") : t("检查更新") }}</n-button>
        <div class="update-result" aria-live="polite" aria-atomic="true">
          <n-alert
            v-if="updateResult"
            :type="updateResult.update_available ? 'warning' : 'success'"
            :title="t(updateResult.update_available ? '发现新版本' : '已是最新版本')"
          >
            <div class="update-result-content">
              <dl class="update-versions">
                <div>
                  <dt>{{ t("当前版本") }}</dt>
                  <dd><code>v{{ updateResult.current_version }}</code></dd>
                </div>
                <div>
                  <dt>{{ t("最新版本") }}</dt>
                  <dd><code>v{{ updateResult.latest_version }}</code></dd>
                </div>
              </dl>
              <n-button
                tag="a"
                type="primary"
                size="small"
                :href="updateResult.release_url"
                target="_blank"
                rel="noopener noreferrer"
              >{{ t("查看发布页") }}</n-button>
            </div>
          </n-alert>
          <n-alert v-else-if="updateError" type="error" :title="t('检查更新失败')">
            {{ updateError }}
          </n-alert>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  NAlert,
  NButton,
  NForm,
  NFormItem,
  NIcon,
  NInput,
  NInputNumber,
  NPopconfirm,
  NSwitch,
  NTooltip,
  useMessage,
} from "naive-ui";
import {
  CheckOutlined,
  CopyOutlined,
  EditOutlined,
  ReloadOutlined,
  SwapOutlined,
  BgColorsOutlined,
  CloudSyncOutlined,
} from "@vicons/antd";
import { tauriApi } from "../api/tauri";
import type { AppConfig, UpdateCheckResult } from "../api/tauri";
import { THEME_OPTIONS } from "../theme";
import type { ResolvedTheme, ThemeName } from "../theme";
import { t } from "../i18n/index.ts";
import type { MessageKey } from "../i18n/index.ts";
import { useClipboard } from "../utils/format.ts";
import {
  maskConnectionKey,
  normalizeClientRootUrl,
  resolveConnectionUrls,
} from "./dashboard-connection";

const { themeName, resolvedTheme } = defineProps<{
  themeName: ThemeName;
  resolvedTheme: ResolvedTheme;
}>();
const emit = defineEmits<{ "update:themeName": [value: ThemeName] }>();

const message = useMessage();
const saving = ref(false);
const regenerating = ref(false);
const { copiedTarget: keyCopied, copy, cleanup } = useClipboard();
const loaded = ref(false);
const editingGatewayKey = ref(false);
const gatewayKeyDraft = ref("");
const checkingUpdate = ref(false);
const updateResult = ref<UpdateCheckResult | null>(null);
const updateError = ref("");
const savedConfig = ref<AppConfig | null>(null);

// ponytail: keep this pre-load fallback in sync with AppConfig::default().
const config = ref<AppConfig>({
  gateway_port: 9042,
  gateway_key: "",
  upstream_base_url: "https://opencode.ai/zen/go",
  client_root_url: "",
  client_root_url_from_env: false,
  auto_start: false,
  auto_start_supported: false,
  connect_timeout_secs: 30,
  non_stream_timeout_secs: 120,
  stream_idle_timeout_secs: 300,
});

const themeLabel = computed(() => {
  const selected = t((THEME_OPTIONS.find((option) => option.value === themeName)?.label ?? "默认") as MessageKey);
  if (themeName !== "default") return selected;
  const resolved = t((THEME_OPTIONS.find((option) => option.value === resolvedTheme)?.label ?? "皓白") as MessageKey);
  return t("默认 · {theme}", { theme: resolved });
});
const maskedSettingsKey = computed(() => maskConnectionKey(config.value.gateway_key));

const automaticClientRootUrls = computed(() => resolveConnectionUrls(
  "",
  window.location.origin,
  config.value.gateway_port,
  import.meta.env.DEV,
));
const automaticClientRootFeedback = computed(() => t(
  "未配置时自动使用：{root}（API Base URL：{api}）；自动值不会写入设置。",
  {
    root: automaticClientRootUrls.value.rootUrl,
    api: automaticClientRootUrls.value.apiBaseUrl,
  },
));

const clientRootInputValue = computed({
  get: () => config.value.client_root_url,
  set: (value: string) => {
    if (!config.value.client_root_url_from_env) config.value.client_root_url = value;
  },
});

const clientRootPreview = computed<{
  status?: "error" | "warning";
  feedback: string;
}>(() => {
  try {
    const urls = resolveConnectionUrls(
      config.value.client_root_url,
      window.location.origin,
      config.value.gateway_port,
      import.meta.env.DEV,
    );
    if (urls.insecureHttp) {
      return {
        status: "warning",
        feedback: t("API Base URL：{url}。警告：非本机 HTTP 会明文传输 Key 与请求内容。", { url: urls.apiBaseUrl }),
      };
    }
    if (!config.value.client_root_url.trim()) {
      return { feedback: automaticClientRootFeedback.value };
    }
    return { feedback: t("API Base URL：{url}", { url: urls.apiBaseUrl }) };
  } catch (error) {
    return {
      status: "error",
      feedback: error instanceof Error ? error.message : t("地址格式无效"),
    };
  }
});

async function loadSettings() {
  try {
    config.value = await tauriApi.getSettings();
    savedConfig.value = { ...config.value };
    loaded.value = true;
  } catch (e) {
    message.error(t("加载设置失败: {error}", { error: String(e) }));
  }
}

async function saveSettings() {
  if (!loaded.value) return;
  if (!normalizeClientRootInput()) return;
  if (!timeoutsValid()) {
    message.error(t("请求超时必须为整数：连接 1–300 秒，其余 1–3600 秒"));
    return;
  }
  saving.value = true;
  try {
    await tauriApi.updateSettings(config.value);
    savedConfig.value = { ...config.value };
    message.success(t("设置已保存"));
  } catch (e) {
    message.error(t("保存失败: {error}", { error: String(e) }));
  } finally {
    saving.value = false;
  }
}

async function handleAutoStartToggle(newValue: boolean) {
  if (!loaded.value || !savedConfig.value) return;
  const next = { ...savedConfig.value, auto_start: newValue };
  saving.value = true;
  try {
    await tauriApi.updateSettings(next);
    savedConfig.value = next;
    config.value.auto_start = newValue;
    message.success(t("设置已保存"));
  } catch (e) {
    config.value.auto_start = savedConfig.value.auto_start;
    message.error(t("自动启动设置失败: {error}", { error: String(e) }));
  } finally {
    saving.value = false;
  }
}

async function saveGatewayKey() {
  if (!loaded.value || !savedConfig.value) return;
  const key = gatewayKeyDraft.value.trim();
  if (!key) {
    message.error(t("新 Key 不能为空"));
    return;
  }
  const payload = { ...savedConfig.value, gateway_key: key };
  saving.value = true;
  try {
    await tauriApi.updateSettings(payload);
    savedConfig.value = payload;
    config.value.gateway_key = key;
    gatewayKeyDraft.value = "";
    editingGatewayKey.value = false;
    message.success(t("Key 已保存"));
  } catch (e) {
    message.error(t("Key 保存失败: {error}", { error: String(e) }));
  } finally {
    saving.value = false;
  }
}

function startGatewayKeyEdit() {
  gatewayKeyDraft.value = "";
  editingGatewayKey.value = true;
}

function cancelGatewayKeyEdit() {
  gatewayKeyDraft.value = "";
  editingGatewayKey.value = false;
}

function normalizeClientRootInput(): boolean {
  if (config.value.client_root_url_from_env) return true;
  try {
    config.value.client_root_url = normalizeClientRootUrl(config.value.client_root_url);
    return true;
  } catch (error) {
    message.error(error instanceof Error ? error.message : t("下游访问根地址无效"));
    return false;
  }
}

function timeoutsValid(): boolean {
  return [
    [config.value.connect_timeout_secs, 300],
    [config.value.non_stream_timeout_secs, 3600],
    [config.value.stream_idle_timeout_secs, 3600],
  ].every(([value, max]) => Number.isInteger(value) && value >= 1 && value <= max);
}

async function copyKey() {
  try {
    await copy("settings-key", config.value.gateway_key, "Key");
    message.success(t("已复制 Key"));
  } catch (e) {
    message.error(e instanceof Error ? e.message : t("复制失败"));
  }
}

async function regenerateKey() {
  regenerating.value = true;
  try {
    config.value.gateway_key = await tauriApi.regenerateGatewayKey();
    cancelGatewayKeyEdit();
    message.success(t("Key 已重新生成"));
  } catch (e) {
    message.error(t("生成失败: {error}", { error: String(e) }));
  } finally {
    regenerating.value = false;
  }
}

async function checkForUpdate() {
  if (checkingUpdate.value) return;
  checkingUpdate.value = true;
  updateResult.value = null;
  updateError.value = "";
  try {
    updateResult.value = await tauriApi.checkForUpdate();
  } catch (error) {
    updateError.value = error instanceof Error ? error.message : String(error);
  } finally {
    checkingUpdate.value = false;
  }
}

onMounted(loadSettings);
onUnmounted(cleanup);
</script>

<style scoped>
.settings-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 16px;
  max-width: 1080px;
  margin: 0 auto;
}
.settings-card {
  padding: 22px;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.settings-side {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-self: start;
  gap: 16px;
}
.downstream-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-items: start;
  gap: 16px;
  padding-top: 18px;
  border-top: 1px solid var(--ocg-border);
}
.settings-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 18px;
}
.settings-head h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.settings-head p {
  margin: 4px 0 0;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
}
.section-icon {
  margin-right: 6px;
  vertical-align: -0.15em;
}
.key-field {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto auto;
  align-items: center;
  gap: 4px;
  width: 100%;
}
.key-stack {
  display: grid;
  gap: 6px;
  width: 100%;
}
.key-display {
  display: flex;
  min-width: 0;
  min-height: 34px;
  align-items: center;
  padding: 0 10px;
  border: 1px solid var(--ocg-border);
  border-radius: 3px;
  background: var(--ocg-canvas);
}
.key-display code {
  overflow: hidden;
  color: var(--ocg-ink);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: var(--ocg-font-md);
  line-height: 1.4;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.key-editor {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 6px;
}
.client-root-field {
  width: 100%;
}
.client-root-field > p {
  margin: 6px 0 0;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
  line-height: 1.5;
}
.settings-subsection {
  margin-top: 8px;
  padding-top: 18px;
  border-top: 1px solid var(--ocg-border);
}
.settings-subsection h3 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.timeout-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
}
.field-caption {
  font-size: var(--ocg-font-xs);
  color: var(--ocg-subtle);
  line-height: 1.4;
}
.theme-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(72px, 1fr));
  gap: 8px;
}
.theme-option {
  position: relative;
  display: flex;
  min-width: 0;
  min-height: 64px;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 8px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  color: var(--ocg-muted);
  background: var(--ocg-canvas);
  font: 600 var(--ocg-font-sm)/1 "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif;
  cursor: pointer;
  transition: border-color 0.16s ease, box-shadow 0.16s ease, color 0.16s ease;
}
.theme-option:hover {
  border-color: var(--ocg-primary);
  color: var(--ocg-ink);
}
.theme-option:focus-visible {
  outline: 2px solid var(--ocg-primary);
  outline-offset: 2px;
}
.theme-option--selected {
  border-color: var(--ocg-primary);
  color: var(--ocg-primary);
  box-shadow: 0 0 0 2px var(--ocg-primary);
}
.theme-swatch {
  width: 20px;
  height: 20px;
  flex: 0 0 20px;
  border-radius: 50%;
  box-shadow: inset 0 0 0 1px rgb(0 0 0 / 12%);
}
.theme-swatch--default {
  background: linear-gradient(135deg, #fff 0 50%, #000 50%) !important;
  box-shadow: inset 0 0 0 1px #8c8994;
}
.theme-swatch--white {
  box-shadow: inset 0 0 0 1px #8c8994;
}
.theme-check {
  position: absolute;
  top: 5px;
  right: 5px;
  font-size: var(--ocg-font-xs);
}
.update-result {
  margin-top: 14px;
}
.update-result:empty {
  margin-top: 0;
}
.update-result-content {
  display: grid;
  justify-items: start;
  gap: 12px;
}
.update-versions {
  display: grid;
  gap: 6px;
  margin: 0;
}
.update-versions > div {
  display: grid;
  grid-template-columns: auto 1fr;
  align-items: baseline;
  gap: 10px;
}
.update-versions dt {
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
}
.update-versions dd {
  margin: 0;
}
.update-result-content code {
  color: var(--ocg-ink);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: var(--ocg-font-md);
  font-weight: 600;
  line-height: 1.4;
}

@media (max-width: 800px) {
  .settings-side,
  .downstream-grid {
    grid-template-columns: 1fr;
  }
}
</style>
