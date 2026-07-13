<template>
  <div class="applications">
    <header class="page-head">
      <div>
        <h1>{{ t("应用接入教程") }}</h1>
        <p class="page-copy">{{ t("选择常用客户端，复制当前节点配置并完成一次请求验证。") }}</p>
      </div>
      <n-tag type="info" :bordered="false" round>{{ t("客户端数：{count}", { count: applicationGuides.length }) }}</n-tag>
    </header>

    <section class="connection-panel" aria-labelledby="connection-panel-title">
      <div class="connection-head">
        <div>
          <h2 id="connection-panel-title">{{ t("当前节点接入轨") }}</h2>
          <p>{{ t("地址随设置实时生成；Key 始终脱敏展示。") }}</p>
        </div>
        <n-tag v-if="settingsLoaded" type="success" :bordered="false" size="small">{{ t("已同步设置") }}</n-tag>
        <n-tag v-else-if="settingsLoading" :bordered="false" size="small">{{ t("正在读取设置") }}</n-tag>
        <n-tag v-else type="error" :bordered="false" size="small">{{ t("读取失败") }}</n-tag>
      </div>

      <div class="connection-track">
        <article class="connection-stage">
          <span>ROOT</span>
          <div class="connection-value">
            <code>{{ connectionUrls.rootUrl }}</code>
            <n-button
              circle
              quaternary
              size="small"
              :aria-label="t('复制根地址')"
              :disabled="!settingsLoaded"
              @click="copyValue('root', connectionUrls.rootUrl, t('根地址'))"
            >
              <template #icon>
                <n-icon :component="copiedTarget === 'root' ? CheckOutlined : CopyOutlined" />
              </template>
            </n-button>
          </div>
        </article>
        <article class="connection-stage">
          <span>API BASE</span>
          <div class="connection-value">
            <code>{{ connectionUrls.apiBaseUrl }}</code>
            <n-button
              circle
              quaternary
              size="small"
              :aria-label="t('复制 API Base URL')"
              :disabled="!settingsLoaded"
              @click="copyValue('api', connectionUrls.apiBaseUrl, 'API Base URL')"
            >
              <template #icon>
                <n-icon :component="copiedTarget === 'api' ? CheckOutlined : CopyOutlined" />
              </template>
            </n-button>
          </div>
        </article>
        <article class="connection-stage connection-stage--endpoint">
          <span>{{ activeEndpoint.label }}</span>
          <div class="connection-value">
            <code>{{ activeEndpoint.url }}</code>
            <n-button
              circle
              quaternary
              size="small"
              :aria-label="t('复制 {label}', { label: activeEndpoint.label })"
              :disabled="!settingsLoaded"
              @click="copyValue('endpoint', activeEndpoint.url, activeEndpoint.label)"
            >
              <template #icon>
                <n-icon :component="copiedTarget === 'endpoint' ? CheckOutlined : CopyOutlined" />
              </template>
            </n-button>
          </div>
        </article>
      </div>

      <div class="key-row">
        <span>GATEWAY KEY</span>
        <code>{{ maskedKey }}</code>
        <n-button
          circle
          quaternary
          size="small"
          :aria-label="t('复制 Gateway Key')"
          :disabled="!settingsLoaded || !serviceConfig.gateway_key"
          @click="copyValue('key', serviceConfig.gateway_key, 'Gateway Key')"
        >
          <template #icon>
            <n-icon :component="copiedTarget === 'key' ? CheckOutlined : CopyOutlined" />
          </template>
        </n-button>
      </div>
    </section>

    <n-alert v-if="settingsError" type="error" :title="t('节点设置加载失败')">
      {{ t("{error}。教程正文仍可阅读，但为避免复制错误地址，动态配置复制已禁用。", { error: settingsError }) }}
    </n-alert>
    <n-alert
      v-if="connectionUrls.insecureHttp"
      type="warning"
      :title="t('当前使用非本机 HTTP 地址')"
    >
      {{ t("Gateway Key 与请求内容会以明文传输。仅在可信局域网内使用，公网接入请配置 HTTPS。") }}
    </n-alert>

    <section class="guide-card" :aria-label="t('下游应用教程')">
      <n-tabs
        class="guide-tabs"
        type="line"
        size="small"
        role="tablist"
        :aria-label="t('选择下游应用')"
        :value="currentApplication"
        @update:value="selectApplication"
      >
        <n-tab-pane
          v-for="guide in applicationGuides"
          :key="guide.id"
          :name="guide.id"
          :tab="guide.name"
          :tab-props="applicationTabProps(guide.id)"
          display-directive="if"
        >
          <article
            :id="`${guide.id}-panel`"
            class="guide-body"
            role="tabpanel"
            :aria-labelledby="`${guide.id}-tab`"
            tabindex="0"
          >
            <header class="guide-head">
              <div>
                <div class="guide-title-row">
                  <h2>{{ guide.name }}</h2>
                  <n-tag size="small" type="info" :bordered="false">{{ guide.protocol }}</n-tag>
                  <n-tag
                    v-if="guide.badge"
                    size="small"
                    :type="guide.id === 'trae' ? 'warning' : 'default'"
                    :bordered="false"
                  >
                    {{ guide.badge }}
                  </n-tag>
                </div>
                <p>{{ guide.summary }}</p>
              </div>
              <a :href="guide.officialUrl" target="_blank" rel="noopener noreferrer">
                {{ t("官方文档") }}
                <n-icon :component="ExportOutlined" aria-hidden="true" />
              </a>
            </header>

            <section class="guide-section" :aria-labelledby="`${guide.id}-steps`">
              <h3 :id="`${guide.id}-steps`">{{ t("配置步骤") }}</h3>
              <ol>
                <li v-for="step in guide.steps" :key="step">{{ step }}</li>
              </ol>
            </section>

            <section class="guide-section" :aria-labelledby="`${guide.id}-snippets`">
              <h3 :id="`${guide.id}-snippets`">{{ t("配置示例") }}</h3>
              <div class="snippet-grid">
                <article
                  v-for="(snippet, index) in guide.snippets(guideContext)"
                  :key="snippet.label"
                  class="snippet-card"
                >
                  <header>
                    <strong>{{ snippet.label }}</strong>
                    <span>{{ snippet.language }}</span>
                    <n-button
                      size="small"
                      secondary
                      :disabled="!settingsLoaded || !serviceConfig.gateway_key"
                      :aria-label="t('复制 {label}', { label: snippet.label })"
                      @click="copyValue(`${guide.id}:${index}`, snippet.copy, snippet.label)"
                    >
                      <template #icon>
                        <n-icon
                          :component="copiedTarget === `${guide.id}:${index}` ? CheckOutlined : CopyOutlined"
                        />
                      </template>
                      {{ copiedTarget === `${guide.id}:${index}` ? t("已复制") : t("复制配置") }}
                    </n-button>
                  </header>
                  <pre><code>{{ snippet.display }}</code></pre>
                </article>
              </div>
            </section>

            <section class="guide-section verification" :aria-labelledby="`${guide.id}-verify`">
              <h3 :id="`${guide.id}-verify`">{{ t("验证方法") }}</h3>
              <p>{{ t("在客户端发送一次测试消息，再到 OCG Manager 的“请求日志”确认模型、账号和成功状态。") }}</p>
            </section>

            <section class="guide-section" :aria-labelledby="`${guide.id}-notes`">
              <h3 :id="`${guide.id}-notes`">{{ t("注意事项") }}</h3>
              <ul>
                <li v-for="note in guide.notes" :key="note">{{ note }}</li>
              </ul>
            </section>
          </article>
        </n-tab-pane>
      </n-tabs>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NIcon,
  NTabPane,
  NTabs,
  NTag,
  useMessage,
} from "naive-ui";
import { CheckOutlined, CopyOutlined, ExportOutlined } from "@vicons/antd";
import { tauriApi } from "../api/tauri";
import {
  maskConnectionKey,
  resolveConnectionUrls,
  writeConnectionValue,
} from "./dashboard-connection";
import {
  APPLICATION_GUIDES,
  isApplicationId,
} from "./application-guides";
import type { ApplicationId, GuideContext } from "./application-guides";
import { t } from "../i18n/index.ts";

const DEFAULT_APPLICATION: ApplicationId = "claude-code";
const message = useMessage();
const currentApplication = ref<ApplicationId>(readApplication());
const settingsLoading = ref(true);
const settingsLoaded = ref(false);
const settingsError = ref("");
const copiedTarget = ref("");
let copyTimer: ReturnType<typeof setTimeout> | undefined;

const serviceConfig = ref({
  gateway_port: 9042,
  gateway_key: "",
  client_root_url: "",
});

const applicationGuides = computed(() => APPLICATION_GUIDES.map((guide) => ({
  ...guide,
  badge: guide.badge === "版本相关" ? t("版本相关") : guide.badge,
  summary: t(guide.summary),
  steps: guide.steps.map((step) => t(step)),
  notes: guide.notes.map((note) => t(note)),
})));

const connectionUrls = computed(() => resolveConnectionUrls(
  serviceConfig.value.client_root_url,
  window.location.origin,
  serviceConfig.value.gateway_port,
  import.meta.env.DEV,
));
const maskedKey = computed(() => maskConnectionKey(serviceConfig.value.gateway_key));
const guideContext = computed<GuideContext>(() => ({
  rootUrl: connectionUrls.value.rootUrl,
  apiBaseUrl: connectionUrls.value.apiBaseUrl,
  chatCompletionsUrl: connectionUrls.value.chatCompletionsUrl,
  responsesUrl: connectionUrls.value.responsesUrl,
  messagesUrl: connectionUrls.value.messagesUrl,
  displayKey: maskedKey.value,
  actualKey: serviceConfig.value.gateway_key,
}));
const activeEndpoint = computed(() => {
  if (currentApplication.value === "claude-code") {
    return { label: "MESSAGES ENDPOINT", url: connectionUrls.value.messagesUrl };
  }
  if (currentApplication.value === "codex") {
    return { label: "RESPONSES ENDPOINT", url: connectionUrls.value.responsesUrl };
  }
  return { label: "CHAT ENDPOINT", url: connectionUrls.value.chatCompletionsUrl };
});

function readApplication(): ApplicationId {
  const value = new URLSearchParams(window.location.search).get("app");
  return isApplicationId(value) ? value : DEFAULT_APPLICATION;
}

function selectApplication(value: string | number) {
  if (typeof value === "string" && isApplicationId(value)) currentApplication.value = value;
}

function applicationTabProps(id: ApplicationId) {
  const selected = currentApplication.value === id;
  return {
    id: `${id}-tab`,
    role: "tab",
    tabindex: selected ? 0 : -1,
    "aria-selected": selected,
    "aria-controls": `${id}-panel`,
    onKeydown: (event: KeyboardEvent) => handleApplicationTabKeydown(event, id),
  };
}

function handleApplicationTabKeydown(event: KeyboardEvent, id: ApplicationId) {
  const index = APPLICATION_GUIDES.findIndex((guide) => guide.id === id);
  let nextIndex: number | undefined;
  if (event.key === "ArrowRight") nextIndex = (index + 1) % APPLICATION_GUIDES.length;
  if (event.key === "ArrowLeft") nextIndex = (index - 1 + APPLICATION_GUIDES.length) % APPLICATION_GUIDES.length;
  if (event.key === "Home") nextIndex = 0;
  if (event.key === "End") nextIndex = APPLICATION_GUIDES.length - 1;
  if (nextIndex === undefined) return;

  event.preventDefault();
  const next = APPLICATION_GUIDES[nextIndex].id;
  currentApplication.value = next;
  requestAnimationFrame(() => document.getElementById(`${next}-tab`)?.focus());
}

function syncApplication(value: ApplicationId) {
  const url = new URL(window.location.href);
  url.searchParams.set("app", value);
  window.history.replaceState(null, "", url);
}

function onPopState() {
  const params = new URLSearchParams(window.location.search);
  if (params.get("view") !== "apps") return;
  const value = params.get("app");
  const next = isApplicationId(value) ? value : DEFAULT_APPLICATION;
  if (currentApplication.value === next) syncApplication(next);
  else currentApplication.value = next;
}

async function loadSettings() {
  try {
    const settings = await tauriApi.getSettings();
    serviceConfig.value = {
      gateway_port: settings.gateway_port,
      gateway_key: settings.gateway_key,
      client_root_url: settings.client_root_url,
    };
    settingsLoaded.value = true;
  } catch (error) {
    settingsError.value = error instanceof Error ? error.message : String(error);
  } finally {
    settingsLoading.value = false;
  }
}

async function copyValue(target: string, value: string, label: string) {
  try {
    const writeText = navigator.clipboard?.writeText?.bind(navigator.clipboard);
    await writeConnectionValue(writeText, value);
    copiedTarget.value = target;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copiedTarget.value = ""; }, 1500);
    message.success(t("已复制 {label}", { label }));
  } catch (error) {
    message.error(error instanceof Error ? error.message : t("复制失败"));
  }
}

watch(currentApplication, syncApplication);

onMounted(() => {
  syncApplication(currentApplication.value);
  window.addEventListener("popstate", onPopState);
  void loadSettings();
});

onUnmounted(() => {
  window.removeEventListener("popstate", onPopState);
  clearTimeout(copyTimer);
});
</script>

<style scoped>
.applications {
  display: grid;
  gap: 16px;
  width: min(1180px, 100%);
  min-width: 0;
  margin: 0 auto;
}

.page-head,
.connection-head,
.guide-head,
.guide-title-row,
.connection-value,
.key-row,
.snippet-card > header {
  display: flex;
  align-items: center;
}

.page-head {
  justify-content: space-between;
  gap: 16px;
}
.page-kicker {
  margin: 0 0 4px;
  color: var(--ocg-primary);
  font: 700 10px/1.2 "Cascadia Mono", Consolas, monospace;
  letter-spacing: 0.12em;
}
.page-head h1 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 24px/1.25 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.page-copy {
  margin: 6px 0 0;
  color: var(--ocg-subtle);
  font-size: 12px;
}

.connection-panel,
.guide-card {
  min-width: 0;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.connection-panel {
  padding: 16px;
}
.connection-head {
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}
.connection-head h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 15px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.connection-head p {
  margin: 3px 0 0;
  color: var(--ocg-subtle);
  font-size: 11px;
}
.connection-track {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 22px;
}
.connection-stage {
  position: relative;
  min-width: 0;
  padding: 10px 8px 10px 12px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: color-mix(in srgb, var(--ocg-canvas) 72%, var(--ocg-surface));
}
.connection-stage:not(:last-child)::after {
  content: "→";
  position: absolute;
  top: 50%;
  right: -17px;
  color: var(--ocg-subtle);
  transform: translateY(-50%);
}
.connection-stage > span,
.key-row > span {
  display: block;
  margin-bottom: 5px;
  color: var(--ocg-subtle);
  font: 700 9px/1.2 "Cascadia Mono", Consolas, monospace;
  letter-spacing: 0.08em;
}
.connection-value {
  min-width: 0;
  gap: 4px;
}
.connection-value code,
.key-row code {
  min-width: 0;
  overflow: hidden;
  color: var(--ocg-ink);
  font: 11px/1.45 "Cascadia Mono", Consolas, monospace;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.connection-value code {
  flex: 1 1 auto;
}
.key-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 10px;
  margin-top: 10px;
  padding: 8px 8px 8px 12px;
  border-radius: 10px;
  background: var(--ocg-primary-soft);
}
.key-row > span {
  margin: 0;
  color: var(--ocg-primary);
}

.guide-card {
  padding: 0 18px 18px;
}
.guide-tabs {
  min-width: 0;
}
.guide-body {
  display: grid;
  gap: 22px;
  min-width: 0;
  padding-top: 8px;
}
.guide-head {
  align-items: flex-start;
  justify-content: space-between;
  gap: 18px;
}
.guide-title-row {
  flex-wrap: wrap;
  gap: 8px;
}
.guide-head h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 21px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.guide-head p {
  margin: 7px 0 0;
  color: var(--ocg-muted);
  font-size: 13px;
  line-height: 1.65;
}
.guide-head > a {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 5px;
  padding: 7px 10px;
  border: 1px solid var(--ocg-border);
  border-radius: 8px;
  color: var(--ocg-primary);
  font-size: 12px;
  font-weight: 650;
  text-decoration: none;
}
.guide-head > a:hover {
  border-color: var(--ocg-primary);
}
.guide-section {
  min-width: 0;
  padding-top: 18px;
  border-top: 1px solid var(--ocg-divider);
}
.guide-section h3 {
  margin: 0 0 10px;
  color: var(--ocg-ink);
  font: 700 14px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.guide-section ol,
.guide-section ul {
  display: grid;
  gap: 8px;
  margin: 0;
  padding-left: 22px;
  color: var(--ocg-muted);
  font-size: 13px;
  line-height: 1.65;
}
.verification {
  padding: 14px 16px;
  border: 1px solid color-mix(in srgb, var(--ocg-success) 28%, var(--ocg-border));
  border-radius: 10px;
  background: var(--ocg-success-soft);
}
.verification h3 {
  color: var(--ocg-success);
}
.verification p {
  margin: 0;
  color: var(--ocg-ink);
  font-size: 13px;
  line-height: 1.6;
}
.snippet-grid {
  display: grid;
  gap: 12px;
}
.snippet-card {
  min-width: 0;
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: color-mix(in srgb, var(--ocg-canvas) 82%, var(--ocg-surface));
}
.snippet-card > header {
  gap: 8px;
  min-height: 42px;
  padding: 6px 8px 6px 12px;
  border-bottom: 1px solid var(--ocg-border);
}
.snippet-card strong {
  min-width: 0;
  overflow: hidden;
  color: var(--ocg-ink);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.snippet-card header > span {
  margin-right: auto;
  color: var(--ocg-subtle);
  font: 10px/1 "Cascadia Mono", Consolas, monospace;
  text-transform: uppercase;
}
.snippet-card pre {
  max-width: 100%;
  margin: 0;
  overflow: auto;
  padding: 14px;
  color: var(--ocg-ink);
  font: 12px/1.6 "Cascadia Mono", Consolas, monospace;
  tab-size: 2;
}
.snippet-card code {
  display: block;
  width: max-content;
  min-width: 100%;
  white-space: pre;
}

@media (max-width: 800px) {
  .connection-track {
    grid-template-columns: 1fr;
    gap: 18px;
  }
  .connection-stage:not(:last-child)::after {
    content: "↓";
    top: auto;
    right: 50%;
    bottom: -17px;
    transform: translateX(50%);
  }
}

@media (max-width: 640px) {
  .applications {
    gap: 12px;
  }
  .page-head,
  .guide-head {
    align-items: flex-start;
    flex-direction: column;
  }
  .page-head h1 {
    font-size: 21px;
  }
  .connection-panel {
    padding: 12px;
  }
  .guide-card {
    padding: 0 12px 14px;
  }
  .guide-body {
    gap: 18px;
  }
  .guide-head > a {
    width: 100%;
    justify-content: center;
  }
  .snippet-card > header {
    flex-wrap: wrap;
  }
  .snippet-card header > span {
    margin-right: 0;
  }
  .snippet-card header > .n-button {
    margin-left: auto;
  }
}
</style>
