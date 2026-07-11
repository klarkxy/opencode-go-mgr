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
        <n-form-item label="上游地址">
          <n-input
            v-model:value="config.upstream_base_url"
            :input-props="{ 'aria-label': '上游地址' }"
            placeholder="https://opencode.ai/zen/go"
          />
        </n-form-item>
        <n-form-item label="Key">
          <div class="key-field">
            <n-input
              v-model:value="config.gateway_key"
              :input-props="{ 'aria-label': 'Key' }"
              type="password"
              show-password-on="click"
              class="mono"
            />
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
        </n-form-item>
      </n-form>
      <n-button type="primary" :loading="saving" @click="saveSettings">保存设置</n-button>
    </section>

    <section class="settings-card appearance-card" aria-labelledby="appearance-title">
      <div class="settings-head">
        <div>
          <h2 id="appearance-title">◐ 外观</h2>
          <p>当前：{{ themeLabel }}</p>
        </div>
      </div>
      <n-button-group>
        <n-tooltip v-for="option in themeOptions" :key="option.value" trigger="hover">
          <template #trigger>
            <n-button
              :type="themeMode === option.value ? 'primary' : 'default'"
              :secondary="themeMode === option.value"
              :aria-label="option.label"
              :aria-pressed="themeMode === option.value"
              @click="$emit('update:themeMode', option.value)"
            >
              <template #icon><n-icon :component="option.icon" /></template>
            </n-button>
          </template>
          {{ option.label }}
        </n-tooltip>
      </n-button-group>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import type { Component } from "vue";
import {
  NButton,
  NButtonGroup,
  NForm,
  NFormItem,
  NIcon,
  NInput,
  NPopconfirm,
  NTooltip,
  useMessage,
} from "naive-ui";
import {
  BulbOutlined,
  CheckOutlined,
  CopyOutlined,
  DesktopOutlined,
  ReloadOutlined,
  StarOutlined,
} from "@vicons/antd";
import { tauriApi } from "../api/tauri";
import type { AppConfig } from "../api/tauri";
import type { ThemeMode } from "../theme";
import { writeConnectionValue } from "./dashboard-connection";

const props = defineProps<{ themeMode: ThemeMode }>();
defineEmits<{ (event: "update:themeMode", value: ThemeMode): void }>();

const message = useMessage();
const saving = ref(false);
const regenerating = ref(false);
const keyCopied = ref(false);
let copyTimer: ReturnType<typeof setTimeout> | undefined;

// ponytail: keep this pre-load fallback in sync with AppConfig::default().
const config = ref<AppConfig>({
  gateway_port: 9042,
  gateway_key: "",
  upstream_base_url: "https://opencode.ai/zen/go",
  auto_start: false,
});

const themeOptions: Array<{ value: ThemeMode; label: string; icon: Component }> = [
  { value: "system", label: "跟随系统", icon: DesktopOutlined },
  { value: "light", label: "浅色", icon: BulbOutlined },
  { value: "dark", label: "深色", icon: StarOutlined },
];
const themeLabel = computed(() => themeOptions.find((option) => option.value === props.themeMode)?.label ?? "跟随系统");

async function loadSettings() {
  try {
    config.value = await tauriApi.getSettings();
  } catch (e) {
    message.error(`加载设置失败: ${e}`);
  }
}

async function saveSettings() {
  saving.value = true;
  try {
    await tauriApi.updateSettings(config.value);
    message.success("设置已保存");
  } catch (e) {
    message.error(`保存失败: ${e}`);
  } finally {
    saving.value = false;
  }
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
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 4px;
  width: 100%;
}
.appearance-card {
  align-self: start;
}

@media (max-width: 800px) {
  .settings-grid {
    grid-template-columns: 1fr;
  }
}
</style>
