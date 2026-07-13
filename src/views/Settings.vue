<template>
  <div class="settings-grid">
    <section class="settings-card" aria-labelledby="forwarding-title">
      <div class="settings-head">
        <div>
          <h2 id="forwarding-title">↗ 转发</h2>
          <p>上游连接与访问凭据</p>
        </div>
      </div>
      <n-form :model="config" label-placement="top" :show-feedback="false">
        <n-form-item
          label="下游访问根地址（可选）"
          :show-feedback="true"
          :validation-status="clientRootPreview.status"
          :feedback="clientRootPreview.feedback"
        >
          <div class="client-root-field">
            <n-input
              v-model:value="config.client_root_url"
              clearable
              class="mono"
              :input-props="{
                'aria-label': '下游访问根地址（可选）',
                'aria-describedby': 'client-root-help',
              }"
              placeholder="https://ocg.example.com"
              @blur="normalizeClientRootInput"
            />
            <p id="client-root-help">
              仅用于下游教程、展示和复制；不会修改 Gateway 监听、DNS 或反向代理。
            </p>
          </div>
        </n-form-item>
        <n-form-item label="上游地址">
          <n-input
            v-model:value="config.upstream_base_url"
            :input-props="{ 'aria-label': '上游地址' }"
            placeholder="https://opencode.ai/zen/go"
          />
        </n-form-item>
        <n-form-item label="Key">
          <div class="key-stack">
            <div class="key-field">
              <div class="key-display" aria-label="已脱敏 Key">
                <code>{{ maskedSettingsKey }}</code>
              </div>
              <n-tooltip trigger="hover">
                <template #trigger>
                  <n-button
                    circle
                    quaternary
                    aria-label="复制 Key"
                    :disabled="!config.gateway_key"
                    @click="copyKey"
                  >
                    <template #icon>
                      <n-icon :component="keyCopied ? CheckOutlined : CopyOutlined" />
                    </template>
                  </n-button>
                </template>
                复制 Key
              </n-tooltip>
              <n-tooltip trigger="hover">
                <template #trigger>
                  <n-button
                    circle
                    quaternary
                    aria-label="设置自定义 Key"
                    :disabled="saving || regenerating"
                    @click="startGatewayKeyEdit"
                  >
                    <template #icon><n-icon :component="EditOutlined" /></template>
                  </n-button>
                </template>
                设置自定义 Key
              </n-tooltip>
              <n-popconfirm
                positive-text="生成新 Key"
                negative-text="取消"
                @positive-click="regenerateKey"
              >
                <template #trigger>
                  <n-tooltip trigger="hover">
                    <template #trigger>
                      <n-button
                        circle
                        quaternary
                        aria-label="刷新 Key"
                        :loading="regenerating"
                        :disabled="saving"
                      >
                        <template #icon><n-icon :component="ReloadOutlined" /></template>
                      </n-button>
                    </template>
                    刷新 Key
                  </n-tooltip>
                </template>
                旧 Key 将立即失效，继续生成新 Key？
              </n-popconfirm>
            </div>
            <div v-if="editingGatewayKey" class="key-editor">
              <n-input
                v-model:value="gatewayKeyDraft"
                type="password"
                class="mono"
                :input-props="{ 'aria-label': '新 Key' }"
                placeholder="输入新 Key，然后保存设置"
              />
              <n-button size="small" secondary @click="cancelGatewayKeyEdit">取消</n-button>
            </div>
            <p>已保存的 Key 只脱敏显示；复制或复制教程配置时才会使用完整值。</p>
          </div>
        </n-form-item>
        <div
          v-if="config.auto_start_supported"
          class="settings-subsection"
          aria-labelledby="startup-title"
        >
          <h3 id="startup-title">开机启动</h3>
          <p id="startup-help">登录 Windows 后在托盘后台启动，不自动打开 Dashboard。</p>
          <n-switch
            v-model:value="config.auto_start"
            aria-label="随 Windows 登录自动启动 OCG Manager"
            aria-describedby="startup-help"
            :disabled="!loaded || saving || regenerating"
            :loading="saving"
          >
            <template #checked>开启</template>
            <template #unchecked>关闭</template>
          </n-switch>
        </div>
        <div class="settings-subsection" aria-labelledby="request-timeout-title">
          <h3 id="request-timeout-title">请求超时</h3>
          <p>分别控制连接建立、非流式响应完成和流式数据停顿的等待上限。</p>
          <n-form-item label="连接超时">
            <n-input-number
              v-model:value="config.connect_timeout_secs"
              :min="1"
              :max="300"
              :precision="0"
              :input-props="{ 'aria-label': '连接超时（秒）' }"
            >
              <template #suffix>秒</template>
            </n-input-number>
          </n-form-item>
          <n-form-item label="非流式总超时">
            <n-input-number
              v-model:value="config.non_stream_timeout_secs"
              :min="1"
              :max="3600"
              :precision="0"
              :input-props="{ 'aria-label': '非流式总超时（秒）' }"
            >
              <template #suffix>秒</template>
            </n-input-number>
          </n-form-item>
          <n-form-item label="流式空闲超时">
            <n-input-number
              v-model:value="config.stream_idle_timeout_secs"
              :min="1"
              :max="3600"
              :precision="0"
              :input-props="{ 'aria-label': '流式空闲超时（秒）' }"
            >
              <template #suffix>秒</template>
            </n-input-number>
          </n-form-item>
        </div>
      </n-form>
      <n-button
        type="primary"
        :loading="saving"
        :disabled="!loaded || regenerating || clientRootPreview.status === 'error'"
        @click="saveSettings"
      >保存设置</n-button>
    </section>

    <section class="settings-card appearance-card" aria-labelledby="appearance-title">
      <div class="settings-head">
        <div>
          <h2 id="appearance-title">◐ 外观</h2>
          <p>当前：{{ themeLabel }}</p>
        </div>
      </div>
      <div class="theme-grid" role="group" aria-label="选择主题">
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
          <span>{{ option.label }}</span>
          <n-icon
            v-if="themeName === option.value"
            class="theme-check"
            :component="CheckOutlined"
            aria-hidden="true"
          />
        </button>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
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
} from "@vicons/antd";
import { tauriApi } from "../api/tauri";
import type { AppConfig } from "../api/tauri";
import { THEME_OPTIONS } from "../theme";
import type { ResolvedTheme, ThemeName } from "../theme";
import {
  maskConnectionKey,
  normalizeClientRootUrl,
  resolveConnectionUrls,
  writeConnectionValue,
} from "./dashboard-connection";

const { themeName, resolvedTheme } = defineProps<{
  themeName: ThemeName;
  resolvedTheme: ResolvedTheme;
}>();
const emit = defineEmits<{ "update:themeName": [value: ThemeName] }>();

const message = useMessage();
const saving = ref(false);
const regenerating = ref(false);
const keyCopied = ref(false);
const loaded = ref(false);
const editingGatewayKey = ref(false);
const gatewayKeyDraft = ref("");
let copyTimer: ReturnType<typeof setTimeout> | undefined;
let persistedAutoStart = false;

// ponytail: keep this pre-load fallback in sync with AppConfig::default().
const config = ref<AppConfig>({
  gateway_port: 9042,
  gateway_key: "",
  upstream_base_url: "https://opencode.ai/zen/go",
  client_root_url: "",
  auto_start: false,
  auto_start_supported: false,
  connect_timeout_secs: 30,
  non_stream_timeout_secs: 120,
  stream_idle_timeout_secs: 300,
});

const themeLabel = computed(() => {
  const selected = THEME_OPTIONS.find((option) => option.value === themeName)?.label ?? "默认";
  if (themeName !== "default") return selected;
  const resolved = THEME_OPTIONS.find((option) => option.value === resolvedTheme)?.label;
  return `默认 · ${resolved ?? "皓白"}`;
});
const maskedSettingsKey = computed(() => maskConnectionKey(config.value.gateway_key));

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
        feedback: `API Base URL：${urls.apiBaseUrl}。警告：非本机 HTTP 会明文传输 Gateway Key 与请求内容。`,
      };
    }
    if (!config.value.client_root_url.trim()) {
      return { feedback: `留空时自动使用：${urls.rootUrl}（API Base URL：${urls.apiBaseUrl}）` };
    }
    return { feedback: `API Base URL：${urls.apiBaseUrl}` };
  } catch (error) {
    return {
      status: "error",
      feedback: error instanceof Error ? error.message : "地址格式无效",
    };
  }
});

async function loadSettings() {
  try {
    config.value = await tauriApi.getSettings();
    persistedAutoStart = config.value.auto_start;
    loaded.value = true;
  } catch (e) {
    message.error(`加载设置失败: ${e}`);
  }
}

async function saveSettings() {
  if (!loaded.value) return;
  if (!normalizeClientRootInput()) return;
  const nextGatewayKey = gatewayKeyDraft.value.trim();
  if (editingGatewayKey.value && !nextGatewayKey) {
    message.error("新 Key 不能为空");
    return;
  }
  if (!timeoutsValid()) {
    message.error("请求超时必须为整数：连接 1–300 秒，其余 1–3600 秒");
    return;
  }
  const previousGatewayKey = config.value.gateway_key;
  if (editingGatewayKey.value) config.value.gateway_key = nextGatewayKey;
  saving.value = true;
  try {
    await tauriApi.updateSettings(config.value);
    persistedAutoStart = config.value.auto_start;
    editingGatewayKey.value = false;
    gatewayKeyDraft.value = "";
    message.success("设置已保存");
  } catch (e) {
    config.value.auto_start = persistedAutoStart;
    config.value.gateway_key = previousGatewayKey;
    message.error(`保存失败: ${e}`);
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
  try {
    config.value.client_root_url = normalizeClientRootUrl(config.value.client_root_url);
    return true;
  } catch (error) {
    message.error(error instanceof Error ? error.message : "下游访问根地址无效");
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
    const writeText = navigator.clipboard?.writeText?.bind(navigator.clipboard);
    await writeConnectionValue(writeText, config.value.gateway_key);
    keyCopied.value = true;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { keyCopied.value = false; }, 1500);
    message.success("已复制 Key");
  } catch (e) {
    message.error(e instanceof Error ? e.message : "复制失败");
  }
}

async function regenerateKey() {
  regenerating.value = true;
  try {
    config.value.gateway_key = await tauriApi.regenerateGatewayKey();
    cancelGatewayKeyEdit();
    message.success("Key 已重新生成");
  } catch (e) {
    message.error(`生成失败: ${e}`);
  } finally {
    regenerating.value = false;
  }
}

onMounted(loadSettings);
onUnmounted(() => clearTimeout(copyTimer));
</script>

<style scoped>
.settings-grid {
  display: grid;
  grid-template-columns: minmax(0, 2fr) minmax(260px, 1fr);
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
.settings-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 18px;
}
.settings-head h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 18px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.settings-head p {
  margin: 4px 0 0;
  color: var(--ocg-subtle);
  font-size: 11px;
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
.key-stack > p {
  margin: 0;
  color: var(--ocg-subtle);
  font-size: 11px;
  line-height: 1.5;
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
  font: 12px/1.4 "Cascadia Mono", Consolas, monospace;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.key-editor {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 6px;
}
.client-root-field {
  width: 100%;
}
.client-root-field > p {
  margin: 6px 0 0;
  color: var(--ocg-subtle);
  font-size: 11px;
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
  font: 700 15px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.settings-subsection > p {
  margin: 4px 0 14px;
  color: var(--ocg-subtle);
  font-size: 11px;
}
.appearance-card {
  align-self: start;
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
  font: 600 13px/1 "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif;
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
  box-shadow: 0 0 0 1px var(--ocg-primary);
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
  font-size: 12px;
}

@media (max-width: 800px) {
  .settings-grid {
    grid-template-columns: 1fr;
  }
}
</style>
