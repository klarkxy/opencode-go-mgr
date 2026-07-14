<template>
  <div class="applications">
    <div class="application-layout">
      <aside class="application-sider">
        <nav class="application-nav" :aria-label="t('选择下游应用')">
          <n-menu
            :value="currentApplication"
            :options="applicationMenuOptions"
            :root-indent="16"
            @update:value="selectApplication"
          />
        </nav>
      </aside>

      <div class="application-content">
        <div class="application-page">
          <div class="application-picker">
            <n-select
              :value="currentApplication"
              :options="applicationSelectOptions"
              :aria-label="t('选择下游应用')"
              @update:value="selectApplication"
            />
          </div>

          <section class="connection-panel" aria-labelledby="connection-panel-title">
            <div class="connection-head">
              <div>
                <h2 id="connection-panel-title">{{ t("当前节点接入轨") }}</h2>
              </div>
              <n-tag v-if="settingsLoaded" type="success" :bordered="false">{{ t("已同步设置") }}</n-tag>
              <n-tag v-else-if="settingsLoading" :bordered="false">{{ t("正在读取设置") }}</n-tag>
              <n-tag v-else type="error" :bordered="false">{{ t("读取失败") }}</n-tag>
            </div>

            <div class="connection-track">
              <article class="connection-stage">
                <span>{{ t("ROOT") }}</span>
                <div class="connection-value">
                  <code>{{ connectionUrls.rootUrl }}</code>
                  <n-button
                    circle
                    quaternary
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
                <span>{{ t("API BASE") }}</span>
                <div class="connection-value">
                  <code>{{ connectionUrls.apiBaseUrl }}</code>
                  <n-button
                    circle
                    quaternary
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
              <article class="connection-stage">
                <span>{{ activeEndpoint.label }}</span>
                <div class="connection-value">
                  <code>{{ activeEndpoint.url }}</code>
                  <n-button
                    circle
                    quaternary
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
              <span>{{ t("GATEWAY KEY") }}</span>
              <code>{{ maskedKey }}</code>
              <n-button
                circle
                quaternary
                :aria-label="t('复制 Gateway Key')"
                :disabled="!settingsLoaded || !serviceConfig.gateway_key"
                @click="copyValue('key', serviceConfig.gateway_key, 'Gateway Key')"
              >
                <template #icon>
                  <n-icon :component="copiedTarget === 'key' ? CheckOutlined : CopyOutlined" />
                </template>
              </n-button>
            </div>

            <div class="model-row">
              <div class="model-row-head">
                <strong>{{ t("模型") }}</strong>
                <n-button
                  size="small"
                  secondary
                  :loading="modelsLoading"
                  :disabled="!settingsLoaded
                    || (activeGuide.id === 'claude-desktop'
                      ? !claudeDesktopModelsLoaded
                      : applicationModelIds.length === 0)"
                  @click="restoreApplicationDefaults"
                >
                  {{ t("恢复默认") }}
                </n-button>
              </div>
              <div
                class="model-controls"
                :class="{ 'model-controls--single': !activeGuide.multipleModels && !activeGuide.modelFields }"
              >
                <template v-if="activeGuide.modelFields">
                  <label v-for="field in activeGuide.modelFields" :key="field" class="model-field">
                    <span>{{ field }}</span>
                    <n-select
                      v-model:value="modelValues[field]"
                      :options="modelOptions"
                      :loading="modelsLoading"
                      :disabled="!settingsLoaded || (activeGuide.id === 'claude-desktop' && !claudeDesktopModelsLoaded)"
                      :placeholder="t('选择模型 ID')"
                      filterable
                    />
                  </label>
                </template>
                <template v-else>
                  <label v-if="activeGuide.multipleModels" class="model-field">
                    <span>models</span>
                    <n-select
                      v-model:value="selectedModels"
                      :options="modelOptions"
                      :loading="modelsLoading"
                      :disabled="!settingsLoaded"
                      :placeholder="t('选择模型 ID')"
                      max-tag-count="responsive"
                      multiple
                      filterable
                    />
                  </label>
                  <label class="model-field">
                    <span>model</span>
                    <n-select
                      v-model:value="selectedModel"
                      :options="primaryModelOptions"
                      :loading="modelsLoading"
                      :disabled="!settingsLoaded"
                      :placeholder="t('选择模型 ID')"
                      filterable
                    />
                  </label>
                </template>
              </div>
            </div>
          </section>

          <n-alert v-if="settingsError" type="error" :title="t('节点设置加载失败')">
            {{ t("{error}。教程正文仍可阅读，但为避免复制错误地址，动态配置复制已禁用。", { error: settingsError }) }}
          </n-alert>
          <n-alert v-if="modelsError" type="warning" :title="t('读取失败')">
            <div class="models-error-content">
              <span>{{ modelsError }}</span>
              <n-button size="small" secondary :loading="modelsLoading" @click="loadModels">
                {{ t("重试") }}
              </n-button>
            </div>
          </n-alert>
          <n-alert
            v-if="connectionUrls.insecureHttp"
            type="warning"
            :title="t('当前使用非本机 HTTP 地址')"
          >
            {{ t("Gateway Key 与请求内容会以明文传输。仅在可信局域网内使用，公网接入请配置 HTTPS。") }}
          </n-alert>

          <article class="guide-body" :aria-labelledby="`${activeGuide.id}-title`">
            <header class="guide-head">
              <div>
                <div class="guide-title-row">
                  <h1 :id="`${activeGuide.id}-title`">{{ activeGuide.name }}</h1>
                  <n-tag type="info" :bordered="false">{{ activeGuide.protocol }}</n-tag>
                  <n-tag v-if="activeGuide.badge" :bordered="false">{{ activeGuide.badge }}</n-tag>
                </div>
                <p>{{ t(activeGuide.summary) }}</p>
              </div>
              <a :href="activeGuide.officialUrl" target="_blank" rel="noopener noreferrer">
                {{ t("官方文档") }}
                <n-icon :component="ExportOutlined" aria-hidden="true" />
              </a>
            </header>

            <div v-if="activeGuide.quickActions?.length" class="quick-actions">
              <template v-for="action in activeGuide.quickActions" :key="action.id">
                <n-button
                  v-if="action.kind === 'copy'"
                  secondary
                  :disabled="!canGenerateConfig"
                  @click="copyGuideAction(action)"
                >
                  <template #icon><n-icon :component="CopyOutlined" /></template>
                  {{ t(action.label) }}
                </n-button>
                <n-popconfirm
                  v-else
                  :negative-text="t('取消')"
                  @positive-click="launchGuideAction(action)"
                >
                  <template #trigger>
                    <n-button type="primary" :disabled="!canGenerateConfig">
                      <template #icon><n-icon :component="ExportOutlined" /></template>
                      {{ t(action.label) }}
                    </n-button>
                  </template>
                  {{ t("即将把当前 Gateway Key 交给 {app}。", { app: activeGuide.name }) }}
                </n-popconfirm>
              </template>
            </div>

            <section class="guide-section" :aria-labelledby="`${activeGuide.id}-steps`">
              <h2 :id="`${activeGuide.id}-steps`">{{ t("配置步骤") }}</h2>
              <ol>
                <li v-for="step in activeGuide.steps" :key="step">{{ t(step) }}</li>
              </ol>
            </section>

            <section class="guide-section" :aria-labelledby="`${activeGuide.id}-snippets`">
              <h2 :id="`${activeGuide.id}-snippets`">{{ t("配置示例") }}</h2>
              <div class="snippet-grid">
                <article
                  v-for="(snippet, index) in currentSnippets"
                  :key="snippet.label"
                  class="snippet-card"
                >
                  <header>
                    <strong>{{ snippet.label }}</strong>
                    <span>{{ snippet.language }}</span>
                    <n-button
                      secondary
                      :disabled="!canGenerateConfig"
                      :aria-label="t('复制 {label}', { label: snippet.label })"
                      @click="copySnippet(index, snippet)"
                    >
                      <template #icon>
                        <n-icon
                          :component="copiedTarget === `${activeGuide.id}:${index}` ? CheckOutlined : CopyOutlined"
                        />
                      </template>
                      {{ copiedTarget === `${activeGuide.id}:${index}` ? t("已复制") : t("复制配置") }}
                    </n-button>
                  </header>
                  <n-input
                    type="textarea"
                    class="snippet-editor"
                    :value="snippetDraft(index, snippet)"
                    :autosize="{ minRows: 5, maxRows: 24 }"
                    :input-props="{ 'aria-label': snippet.label, spellcheck: 'false' }"
                    @update:value="updateSnippetDraft(index, $event)"
                  />
                </article>
              </div>
            </section>

            <section class="guide-section" :aria-labelledby="`${activeGuide.id}-notes`">
              <h2 :id="`${activeGuide.id}-notes`">{{ t("注意事项") }}</h2>
              <ul>
                <li v-for="note in activeGuide.notes" :key="note">{{ t(note) }}</li>
              </ul>
            </section>
          </article>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onMounted, onUnmounted, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NIcon,
  NInput,
  NMenu,
  NPopconfirm,
  NSelect,
  NTag,
  useMessage,
} from "naive-ui";
import type { MenuOption, SelectOption } from "naive-ui";
import { CheckOutlined, CopyOutlined, ExportOutlined } from "@vicons/antd";
import logoUrl from "../../assets/logo/ocg_logo_final_transparent.png";
import { tauriApi, type ClaudeDesktopModels } from "../api/tauri";
import { useClipboard } from "../utils/format.ts";
import {
  maskConnectionKey,
  resolveConnectionUrls,
  restoreMaskedConnectionKey,
} from "./dashboard-connection";
import {
  APPLICATION_GUIDES,
  isApplicationId,
  recommendClaudeCodeModel,
} from "./application-guides";
import type { ApplicationGuide, ApplicationId, GuideAction, GuideContext } from "./application-guides";
import { t } from "../i18n/index.ts";

const DEFAULT_APPLICATION: ApplicationId = "claude-code";
const CLAUDE_DESKTOP_FIELDS = ["sonnet", "opus", "haiku"] as const;
const allowedImportProtocols = new Set(["chatbox:"]);
const message = useMessage();
const { copiedTarget, copy, cleanup } = useClipboard();
const currentApplication = ref<ApplicationId>(readApplication());
const settingsLoading = ref(true);
const settingsLoaded = ref(false);
const settingsError = ref("");
const modelsLoading = ref(false);
const modelsError = ref("");
const modelsInitialized = ref(false);
const claudeDesktopModelsLoaded = ref(false);
const claudeDesktopDefaults = ref<ClaudeDesktopModels>({ sonnet: "", opus: "", haiku: "" });
const applicationModelIds = ref<string[]>([]);
const modelOptions = ref<SelectOption[]>([]);
const selectedModelsByApplication = ref<Partial<Record<ApplicationId, string[]>>>({});
const selectedModelByApplication = ref<Partial<Record<ApplicationId, string | null>>>({});
const selectedModels = computed<string[]>({
  get: () => selectedModelsByApplication.value[currentApplication.value] ?? [],
  set: (value) => { selectedModelsByApplication.value[currentApplication.value] = value; },
});
const selectedModel = computed<string | null>({
  get: () => selectedModelByApplication.value[currentApplication.value] ?? null,
  set: (value) => { selectedModelByApplication.value[currentApplication.value] = value; },
});
const modelValues = ref<Record<string, string>>({});
const snippetDrafts = ref<Record<string, string>>({});

const serviceConfig = ref({
  gateway_port: 9042,
  gateway_key: "",
  client_root_url: "",
});

const applicationGuides: readonly ApplicationGuide[] = APPLICATION_GUIDES;
const activeGuide = computed<ApplicationGuide>(() => (
  applicationGuides.find((guide) => guide.id === currentApplication.value)
  ?? applicationGuides[0]
));
const applicationMenuOptions = computed<MenuOption[]>(() => applicationGuides.map((guide) => ({
  key: guide.id,
  label: guide.name,
})));
const applicationSelectOptions = computed<SelectOption[]>(() => applicationGuides.map((guide) => ({
  value: guide.id,
  label: guide.name,
})));
const primaryModelOptions = computed<SelectOption[]>(() => (
  activeGuide.value.multipleModels
    ? modelOptions.value.filter(({ value }) => typeof value === "string" && selectedModels.value.includes(value))
    : modelOptions.value
));

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
  modelId: activeGuide.value.modelFields?.length
    ? modelValues.value[activeGuide.value.modelFields[0]] || "<MODEL_ID>"
    : selectedModel.value?.trim() || "<MODEL_ID>",
  modelIds: selectedModels.value,
  modelValues: modelValues.value,
  iconUrl: new URL(logoUrl, window.location.origin).href,
}));
const currentSnippets = computed(() => activeGuide.value.snippets(guideContext.value));
const canGenerateConfig = computed(() => (
  settingsLoaded.value
  && Boolean(serviceConfig.value.gateway_key)
  && (activeGuide.value.id !== "claude-desktop" || claudeDesktopModelsLoaded.value)
  && (activeGuide.value.modelFields?.every((field) => Boolean(modelValues.value[field]))
    ?? Boolean(selectedModel.value?.trim()))
  && (!activeGuide.value.multipleModels || selectedModels.value.length > 0)
));
const activeEndpoint = computed(() => {
  if (activeGuide.value.endpointKind === "messages") {
    const url = activeGuide.value.id === "claude-desktop"
      ? `${connectionUrls.value.rootUrl}/claude-desktop/v1/messages`
      : connectionUrls.value.messagesUrl;
    return { label: t("MESSAGES ENDPOINT"), url };
  }
  if (activeGuide.value.endpointKind === "responses") {
    return { label: t("RESPONSES ENDPOINT"), url: connectionUrls.value.responsesUrl };
  }
  if (activeGuide.value.endpointKind === "gemini") {
    return {
      label: "GENERATE CONTENT",
      url: `${connectionUrls.value.rootUrl}/v1beta/models/${guideContext.value.modelId}:generateContent`,
    };
  }
  return { label: t("CHAT ENDPOINT"), url: connectionUrls.value.chatCompletionsUrl };
});

function readApplication(): ApplicationId {
  const value = new URLSearchParams(window.location.search).get("app");
  return isApplicationId(value) ? value : DEFAULT_APPLICATION;
}

function selectApplication(value: string | number | null) {
  if (typeof value !== "string" || !isApplicationId(value) || value === currentApplication.value) return;
  currentApplication.value = value;
  writeApplicationUrl(value, "push");
}

function writeApplicationUrl(value: ApplicationId, mode: "push" | "replace") {
  const url = new URL(window.location.href);
  url.searchParams.set("app", value);
  if (mode === "push") window.history.pushState(null, "", url);
  else window.history.replaceState(null, "", url);
}

function onPopState() {
  const params = new URLSearchParams(window.location.search);
  if (params.get("view") !== "apps") return;
  currentApplication.value = readApplication();
}

async function loadModels() {
  modelsLoading.value = true;
  modelsError.value = "";
  claudeDesktopModelsLoaded.value = false;
  applicationModelIds.value = [];
  for (const field of CLAUDE_DESKTOP_FIELDS) delete modelValues.value[field];
  const errors: string[] = [];
  try {
    const [modelsResult, desktopResult] = await Promise.allSettled([
      tauriApi.getApplicationModels(),
      tauriApi.getClaudeDesktopModels(),
    ]);
    const modelIds = modelsResult.status === "fulfilled" ? modelsResult.value : [];
    applicationModelIds.value = modelIds;
    if (modelsResult.status === "rejected") {
      errors.push(modelsResult.reason instanceof Error ? modelsResult.reason.message : String(modelsResult.reason));
    } else if (!modelIds.length) {
      errors.push(t("未返回可用模型"));
    }
    const claudeDesktopModels = desktopResult.status === "fulfilled" ? desktopResult.value : undefined;
    if (desktopResult.status === "rejected") {
      errors.push(desktopResult.reason instanceof Error ? desktopResult.reason.message : String(desktopResult.reason));
    }
    const availableIds = [...new Set([
      ...modelIds,
      ...Object.values(claudeDesktopModels ?? {}).filter(Boolean),
    ])];
    modelOptions.value = availableIds.map((modelId) => ({ label: modelId, value: modelId }));
    const defaultSelectedModels = modelIds.length ? modelIds : availableIds;
    const fallbackModel = availableIds[0] ?? "";
    for (const guide of applicationGuides) {
      if (!isApplicationId(guide.id)) continue;
      if (!guide.modelFields?.length) {
        if (guide.multipleModels) {
          selectedModelsByApplication.value[guide.id] = [...defaultSelectedModels];
        }
        const selected = selectedModelByApplication.value[guide.id];
        if (!selected || !availableIds.includes(selected)) {
          selectedModelByApplication.value[guide.id] = availableIds[0] ?? null;
        }
        continue;
      }
      if (guide.id === "claude-desktop") continue;
      for (const field of guide.modelFields) {
        if (!availableIds.includes(modelValues.value[field])) {
          modelValues.value[field] = guide.id === "claude-code"
            ? recommendClaudeCodeModel(field, modelIds) || fallbackModel
            : fallbackModel;
        }
      }
    }
    if (claudeDesktopModels) {
      claudeDesktopDefaults.value = { ...claudeDesktopModels };
      Object.assign(modelValues.value, claudeDesktopModels);
      claudeDesktopModelsLoaded.value = true;
    }
    modelsError.value = errors.join("；");
  } finally {
    modelsInitialized.value = true;
    modelsLoading.value = false;
  }
}

async function loadSettings(loadApplicationModels = true) {
  settingsLoading.value = true;
  settingsLoaded.value = false;
  settingsError.value = "";
  try {
    const settings = await tauriApi.getSettings();
    serviceConfig.value = {
      gateway_port: settings.gateway_port,
      gateway_key: settings.gateway_key,
      client_root_url: settings.client_root_url,
    };
    settingsLoaded.value = true;
    if (loadApplicationModels) await loadModels();
  } catch (error) {
    settingsError.value = error instanceof Error ? error.message : String(error);
  } finally {
    settingsLoading.value = false;
  }
}

async function copyValue(target: string, value: string, label: string) {
  try {
    await copy(target, value, label);
    message.success(t("已复制 {label}", { label }));
  } catch (error) {
    message.error(error instanceof Error ? error.message : t("复制失败"));
  }
}

async function copyGuideAction(action: GuideAction) {
  await copyValue(`action:${activeGuide.value.id}:${action.id}`, action.build(guideContext.value), t(action.label));
}

function snippetKey(index: number): string {
  return `${activeGuide.value.id}:${index}`;
}

function snippetDraft(index: number, snippet: { display: string }): string {
  return snippetDrafts.value[snippetKey(index)] ?? snippet.display;
}

function updateSnippetDraft(index: number, value: string) {
  snippetDrafts.value[snippetKey(index)] = value;
}

function clearApplicationDrafts(applicationId: string) {
  const prefix = `${applicationId}:`;
  for (const key of Object.keys(snippetDrafts.value)) {
    if (key.startsWith(prefix)) delete snippetDrafts.value[key];
  }
}

function restoreApplicationDefaults() {
  const guide = activeGuide.value;
  const models = applicationModelIds.value;
  if (guide.id !== "claude-desktop" && !models.length) return;

  if (guide.id === "claude-desktop") {
    Object.assign(modelValues.value, claudeDesktopDefaults.value);
  } else if (guide.modelFields) {
    for (const field of guide.modelFields) {
      modelValues.value[field] = guide.id === "claude-code"
        ? recommendClaudeCodeModel(field, models)
        : models[0] ?? "";
    }
  } else {
    if (guide.multipleModels) selectedModels.value = [...models];
    selectedModel.value = models[0] ?? null;
  }

  clearApplicationDrafts(guide.id);
}

async function copySnippet(index: number, snippet: { label: string; display: string; copy: string }) {
  if (activeGuide.value.id === "claude-desktop") {
    if (!claudeDesktopModelsLoaded.value) {
      message.error(modelsError.value || t("读取失败"));
      return;
    }
    try {
      const persisted = await tauriApi.updateClaudeDesktopModels({
        sonnet: modelValues.value.sonnet,
        opus: modelValues.value.opus,
        haiku: modelValues.value.haiku,
      });
      Object.assign(modelValues.value, persisted);
      claudeDesktopDefaults.value = { ...persisted };
    } catch (error) {
      message.error(error instanceof Error ? error.message : String(error));
      return;
    }
  }
  const draft = snippetDraft(index, snippet);
  const value = draft === snippet.display
    ? snippet.copy
    : restoreMaskedConnectionKey(draft, guideContext.value.displayKey, guideContext.value.actualKey);
  await copyValue(snippetKey(index), value, snippet.label);
}

function launchGuideAction(action: GuideAction) {
  try {
    const value = action.build(guideContext.value);
    if (!allowedImportProtocols.has(new URL(value).protocol)) {
      throw new Error(t("客户端导入链接无效"));
    }
    window.location.assign(value);
  } catch (error) {
    message.error(error instanceof Error ? error.message : t("客户端导入链接无效"));
  }
}

watch([selectedModelByApplication, selectedModelsByApplication, modelValues], () => {
  clearApplicationDrafts(activeGuide.value.id);
  if (
    activeGuide.value.multipleModels
    && (!selectedModel.value || !selectedModels.value.includes(selectedModel.value))
  ) {
    selectedModel.value = selectedModels.value[0] ?? null;
  }
}, { deep: true });

onMounted(() => {
  const value = new URLSearchParams(window.location.search).get("app");
  if (!isApplicationId(value)) writeApplicationUrl(currentApplication.value, "replace");
  window.addEventListener("popstate", onPopState);
  void loadSettings();
});

onActivated(() => {
  if (!settingsLoading.value) void loadSettings(!modelsInitialized.value);
});

onUnmounted(() => {
  window.removeEventListener("popstate", onPopState);
  cleanup();
});
</script>

<style scoped>
.applications {
  width: min(1280px, 100%);
  min-width: 0;
  margin: 0 auto;
}

.application-layout {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  gap: 24px;
  align-items: start;
}

.application-content {
  min-width: 0;
}

.application-sider {
  position: sticky;
  top: 16px;
  max-height: calc(100vh - 128px);
  overflow-y: auto;
  border: 1px solid var(--ocg-border);
  border-radius: 12px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}

.application-nav {
  padding: 8px;
}

.application-page {
  display: grid;
  gap: 16px;
  min-width: 0;
}

.application-picker {
  display: none;
}

.connection-head,
.guide-head,
.guide-title-row,
.connection-value,
.key-row,
.snippet-card > header,
.model-row {
  display: flex;
  align-items: center;
}

.connection-panel {
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}

.connection-head {
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.connection-head h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 18px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}

.connection-track {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 28px;
}

.connection-stage {
  position: relative;
  min-width: 0;
  padding: 12px;
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
  font: 700 16px/1.2 "Cascadia Mono", Consolas, monospace;
  letter-spacing: 0.04em;
}

.connection-value {
  min-width: 0;
  gap: 6px;
}

.connection-value code,
.key-row code {
  min-width: 0;
  overflow: hidden;
  color: var(--ocg-ink);
  font: 16px/1.5 "Cascadia Mono", Consolas, monospace;
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
  padding: 10px 12px;
  border-radius: 10px;
  background: var(--ocg-primary-soft);
}

.key-row > span {
  margin: 0;
  color: var(--ocg-primary);
}

.models-error-content {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.model-row {
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--ocg-divider);
}

.model-row strong {
  color: var(--ocg-ink);
}

.model-row-head {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 8px;
}

.model-controls {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  min-width: 0;
  width: min(760px, 100%);
}

.model-controls--single {
  grid-template-columns: minmax(0, 380px);
  justify-content: end;
}

.model-field {
  display: grid;
  min-width: 0;
  gap: 4px;
}

.model-field > span {
  color: var(--ocg-subtle);
  font: 14px/1.2 "Cascadia Mono", Consolas, monospace;
}

.model-field :deep(.n-select) {
  width: 100%;
  min-width: 0;
}

.guide-body {
  display: grid;
  gap: 22px;
  min-width: 0;
  padding: 8px 0 32px;
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

.guide-head h1 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 24px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}

.guide-head p {
  margin: 8px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-size);
  line-height: 1.65;
}

.guide-head > a {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border: 1px solid var(--ocg-border);
  border-radius: 8px;
  color: var(--ocg-primary);
  font-size: var(--ocg-font-size);
  font-weight: 650;
  text-decoration: none;
}

.guide-head > a:hover {
  border-color: var(--ocg-primary);
}

.quick-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.guide-section {
  min-width: 0;
  padding-top: 18px;
  border-top: 1px solid var(--ocg-divider);
}

.guide-section h2 {
  margin: 0 0 10px;
  color: var(--ocg-ink);
  font: 700 18px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}

.guide-section ol,
.guide-section ul {
  display: grid;
  gap: 8px;
  margin: 0;
  padding-left: 24px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-size);
  line-height: 1.65;
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
  min-height: 48px;
  padding: 8px 10px 8px 12px;
  border-bottom: 1px solid var(--ocg-border);
}

.snippet-card strong {
  min-width: 0;
  overflow: hidden;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-size);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.snippet-card header > span {
  margin-right: auto;
  color: var(--ocg-subtle);
  font: 16px/1 "Cascadia Mono", Consolas, monospace;
  text-transform: uppercase;
}

.snippet-editor {
  padding: 12px;
}

.snippet-editor :deep(.n-input__textarea-el) {
  font: 16px/1.6 "Cascadia Mono", Consolas, monospace;
  tab-size: 2;
  white-space: pre;
}

@media (max-width: 1023px) {
  .application-layout {
    grid-template-columns: minmax(0, 1fr);
  }

  .application-sider {
    display: none;
  }

  .application-picker {
    display: block;
  }
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
  .application-page {
    gap: 12px;
  }

  .connection-panel {
    padding: 12px;
  }

  .model-row,
  .guide-head {
    align-items: stretch;
    flex-direction: column;
  }

  .model-row-head {
    justify-content: space-between;
  }

  .model-controls,
  .guide-head > a {
    width: 100%;
  }

  .model-controls,
  .model-controls--single {
    grid-template-columns: 1fr;
  }

  .guide-head > a {
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
