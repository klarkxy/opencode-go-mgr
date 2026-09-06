<template>
  <div class="applications">
    <header class="page-header">
      <h1>{{ t("应用接入") }}</h1>
      <p>{{ t("在本机安全接入客户端，或查看可复制的手动配置") }}</p>
    </header>

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
            <h2 id="connection-panel-title" class="connection-panel-title">
              {{ t("接入信息") }}
            </h2>

            <div class="access-fields">
              <div class="access-field">
                <span>{{ t("请求地址") }}</span>
                <div class="access-value">
                  <code>{{ activeEndpoint.url }}</code>
                  <n-button
                    circle
                    quaternary
                    :aria-label="t('复制 {label}', { label: t('请求地址') })"
                    :disabled="!settingsLoaded"
                    @click="copyValue('endpoint', activeEndpoint.url, t('请求地址'))"
                  >
                    <template #icon>
                      <n-icon :component="copiedTarget === 'endpoint' ? CheckOutlined : CopyOutlined" />
                    </template>
                  </n-button>
                </div>
              </div>
              <div class="access-field">
                <span>{{ t("Key") }}</span>
                <div class="access-value">
                  <n-select
                    :value="selectedKeyId"
                    :options="keySelectOptions"
                    :loading="settingsLoading"
                    :disabled="!settingsLoaded || enabledGatewayKeys.length === 0"
                    :placeholder="t('选择 Key')"
                    :aria-label="t('选择 Key')"
                    @update:value="selectGatewayKey"
                  />
                  <n-button
                    circle
                    quaternary
                    :aria-label="t('复制 Key')"
                    :disabled="!settingsLoaded || !selectedKey?.value"
                    @click="copyValue('key', selectedKey?.value ?? '', t('Key'))"
                  >
                    <template #icon>
                      <n-icon :component="copiedTarget === 'key' ? CheckOutlined : CopyOutlined" />
                    </template>
                  </n-button>
                </div>
              </div>
              <div class="access-field access-field--alias">
                <span>{{ t("模型别名") }}</span>
                <div class="access-value">
                  <code>{{ effectiveModelAlias || "<MODEL_ALIAS>" }}</code>
                  <n-button
                    circle
                    quaternary
                    :aria-label="t('复制 {label}', { label: t('模型别名') })"
                    :disabled="!settingsLoaded || !effectiveModelAlias"
                    @click="copyValue('model-alias', effectiveModelAlias, t('模型别名'))"
                  >
                    <template #icon>
                      <n-icon :component="copiedTarget === 'model-alias' ? CheckOutlined : CopyOutlined" />
                    </template>
                  </n-button>
                </div>
                <p class="alias-explainer">
                  {{ t("客户端以别名请求模型；上游模型 ID 仅在技术详情中出现。") }}
                </p>
              </div>
            </div>

            <div class="model-row">
                  <div class="model-row-head">
                    <span class="model-label">{{ t("模型") }}</span>
                    <n-button
                      text
                      size="small"
                      :loading="modelsLoading"
                      :disabled="!settingsLoaded
                        || (activeGuide.id === 'claude-desktop'
                          ? !claudeDesktopModelsLoaded
                          : applicationModelIds.length === 0)"
                      @click="restoreApplicationDefaults"
                    >
                      {{ t("恢复推荐模型") }}
                    </n-button>
                    <n-button
                      v-if="activeGuide.id === 'claude-desktop'"
                      type="primary"
                      size="small"
                      :loading="claudeDesktopModelsSaving"
                      :disabled="!claudeDesktopModelsDirty || modelsLoading"
                      @click="saveClaudeDesktopModels"
                    >
                      {{ t("保存") }}
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
                          :value="modelValues[field]"
                          :options="activeModelOptions"
                          :loading="modelsLoading"
                          :disabled="!settingsLoaded || (activeGuide.id === 'claude-desktop'
                            && (!claudeDesktopModelsLoaded || claudeDesktopModelsSaving))"
                          :placeholder="t('选择 Alias（模型 ID）')"
                          filterable
                          @update:value="updateModelField(field, $event)"
                        />
                      </label>
                    </template>
                    <template v-else>
                      <label v-if="activeGuide.multipleModels" class="model-field">
                        <span>{{ t("模型（多选）") }}</span>
                        <n-select
                          v-model:value="selectedModels"
                          :options="activeModelOptions"
                          :loading="modelsLoading"
                          :disabled="!settingsLoaded"
                          :placeholder="t('选择 Alias（模型 ID）')"
                          max-tag-count="responsive"
                          multiple
                          filterable
                        />
                      </label>
                      <label class="model-field">
                        <span>{{ t("模型") }}</span>
                        <n-select
                          v-model:value="selectedModel"
                          :options="primaryModelOptions"
                          :loading="modelsLoading"
                          :disabled="!settingsLoaded"
                          :placeholder="t('选择 Alias（模型 ID）')"
                          filterable
                        />
                      </label>
                    </template>
                  </div>
                </div>

            <p v-if="copyDisabledHint" class="copy-disabled-hint">
              {{ copyDisabledHint }}
            </p>
            <n-alert
              v-if="usesMuseContributor"
              type="warning"
              :title="t('Contributor 模型的数据使用')"
            >
              {{ t('Muse Spark 1.2 Contributor 不是 ZDR；提示词和补全内容可能用于训练。仅在你有权这样使用的数据上选择它。') }}
            </n-alert>
          </section>

          <section
            v-if="connectorEligible"
            class="native-connector-panel"
            aria-labelledby="native-connector-title"
          >
            <div class="native-connector-head">
              <div>
                <div class="native-connector-title-row">
                  <h2 id="native-connector-title">{{ connectorPanelTitle }}</h2>
                  <n-tag :type="connectorTagType" :bordered="false">
                    {{ connectorStatusLabel }}
                  </n-tag>
                </div>
                <p>{{ connectorDetail }}</p>
              </div>
              <n-button
                quaternary
                size="small"
                :loading="connectorsLoading"
                @click="loadConnectors"
              >
                {{ t("刷新检测") }}
              </n-button>
            </div>

            <div v-if="activeConnector?.targetPaths.length" class="connector-paths">
              <span>{{ connectorTargetLabel }}</span>
              <code v-for="path in activeConnector.targetPaths" :key="path">{{ path }}</code>
            </div>

            <n-alert
              v-if="connectorError"
              type="error"
              :title="t('本机接入失败')"
            >
              {{ connectorError }}
            </n-alert>

            <n-alert
              v-if="activeConnector && ['conflict', 'partial'].includes(activeConnector.status)"
              type="warning"
              :title="t('需要人工处理')"
            >
              {{ t("为避免覆盖其他程序的修改，当前不能继续接入或恢复。请关闭目标客户端，检查上方受管文件，并在处理后刷新检测。") }}
            </n-alert>

            <div v-if="connectorPreview" class="connector-preview">
              <div class="connector-preview-head">
                <strong>{{ connectorPreviewTitle }}</strong>
                <span>{{ t("仅显示字段级变化，Key 始终脱敏。") }}</span>
              </div>
              <ul v-if="connectorPreview.changes.length">
                <li v-for="change in connectorPreview.changes" :key="change.field">
                  <code>{{ change.field }}</code>
                  <span>{{ change.before ?? t("未设置") }}</span>
                  <span aria-hidden="true">→</span>
                  <span>{{ change.after ?? t("删除") }}</span>
                </li>
              </ul>
              <p v-else>{{ t("当前配置已经符合所选方案，不需要写入。") }}</p>
            </div>

            <div class="connector-actions">
              <n-button
                secondary
                :disabled="!connectorCanConnect"
                :loading="connectorPreviewing === 'connect'"
                @click="previewConnector('connect')"
              >
                {{ connectorConnectPreviewLabel }}
              </n-button>
              <n-button
                secondary
                :disabled="!connectorCanRestore"
                :loading="connectorPreviewing === 'restore'"
                @click="previewConnector('restore')"
              >
                {{ connectorRestorePreviewLabel }}
              </n-button>
              <n-popconfirm
                v-if="connectorPreview"
                :negative-text="t('取消')"
                @positive-click="commitConnector"
              >
                <template #trigger>
                  <n-button type="primary" :loading="connectorCommitting">
                    {{ connectorCommitLabel }}
                  </n-button>
                </template>
                {{ connectorCommitConfirmation }}
              </n-popconfirm>
            </div>
          </section>

          <n-alert v-if="settingsError" type="error" :title="t('节点设置加载失败')">
            <div class="models-error-content">
              <span>{{ t("{error}。教程正文仍可阅读，但为避免复制错误地址，动态配置复制已禁用。", { error: settingsError }) }}</span>
              <n-button size="small" secondary :loading="settingsLoading" @click="loadSettings()">
                {{ t("重试") }}
              </n-button>
            </div>
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
            {{ t("Key 与请求内容会以明文传输。仅在可信局域网内使用，公网接入请配置 HTTPS。") }}
          </n-alert>
          <n-alert
            v-if="activeGuide.id === 'gemini-cli' && !geminiCliBaseUrlAllowed"
            type="error"
            :title="t('Gemini CLI 的远程 Base URL 必须使用 HTTPS；仅 localhost、127.0.0.1 和 [::1] 可使用 HTTP。')"
          />

          <article class="guide-body" :aria-labelledby="`${activeGuide.id}-title`">
            <header class="guide-head">
              <div>
                <div class="guide-title-row">
                  <h1 :id="`${activeGuide.id}-title`">{{ activeGuide.name }}</h1>
                  <n-tag type="info" :bordered="false">{{ activeGuide.protocol }}</n-tag>
                  <n-tag v-if="activeGuide.badge" :bordered="false">{{ activeGuide.badge }}</n-tag>
                  <n-tag v-if="activeGuide.popular" type="success" :bordered="false">{{ t("常用") }}</n-tag>
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
                  <div>{{ t("即将把当前 Key 交给 {app}。", { app: activeGuide.name }) }}</div>
                  <div>{{ t("如未安装客户端，一键导入不会有反应") }}</div>
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
              <p class="snippet-backup-warning">{{ t("覆盖现有配置文件前，请先备份原文件。") }}</p>
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
                      :disabled="!canGenerateConfig
                        || (activeGuide.id === 'claude-desktop' && claudeDesktopModelsSaving)"
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
import type { MenuOption, SelectGroupOption, SelectOption } from "naive-ui";
import { CheckOutlined, CopyOutlined, ExportOutlined } from "@vicons/antd";
import logoUrl from "../../assets/logo/ocg_logo_final_transparent.png";
import { PRIMARY_KEY_ID, dashboardApi, type ClaudeDesktopModels, type ConnectionSubKey } from "../api/dashboard";
import type {
  ApplicationConnectorAction,
  ApplicationConnectorItem,
  ApplicationConnectorPreview,
  ApplicationConnectorStatus,
} from "../api/generated/dashboard-v3.ts";
import { useConnectionStore } from "../stores/connection.ts";
import { useSettingsStore } from "../stores/settings.ts";
import { selectedApplicationAlias } from "./application-alias.ts";
import { useClipboard } from "../utils/format.ts";
import {
  isGeminiCliBaseUrlAllowed,
  maskConnectionKey,
  reconcileConnectionDrafts,
  resolveConnectionUrls,
  restoreMaskedConnectionKey,
} from "./dashboard-connection";
import {
  APPLICATION_GUIDES,
  isApplicationId,
  recommendClaudeCodeModel,
  reconcileApplicationModelSelection,
} from "./application-guides";
import type { ApplicationGuide, ApplicationId, GuideAction, GuideContext } from "./application-guides";
import { t, type MessageKey } from "../i18n/index.ts";

const DEFAULT_APPLICATION: ApplicationId = "claude-code";
const CLAUDE_DESKTOP_FIELDS = ["sonnet", "opus", "haiku"] as const;
const allowedImportProtocols = new Set(["chatbox:"]);
const message = useMessage();
const connectionStore = useConnectionStore();
const settingsStore = useSettingsStore();
const { copiedTarget, copy, cleanup } = useClipboard();
const currentApplication = ref<ApplicationId>(readApplication());
const settingsLoading = ref(true);
const settingsLoaded = ref(false);
const settingsError = ref("");
const modelsLoading = ref(false);
const modelsError = ref("");
const claudeDesktopModelsLoaded = ref(false);
const claudeDesktopModelsSaving = ref(false);
const claudeDesktopDefaults = ref<ClaudeDesktopModels>({ sonnet: "", opus: "", haiku: "" });
const applicationModelIds = ref<string[]>([]);
const modelOptions = ref<SelectOption[]>([]);
const selectedModelsByApplication = ref<Partial<Record<ApplicationId, string[]>>>({});
const selectedModelByApplication = ref<Partial<Record<ApplicationId, string | null>>>({});
const selectedModels = computed<string[]>({
  get: () => selectedModelsByApplication.value[currentApplication.value] ?? [],
  set: (value) => {
    const applicationId = currentApplication.value;
    if (sameStringArray(selectedModelsByApplication.value[applicationId], value)) return;
    selectedModelsByApplication.value[applicationId] = [...value];
    clearApplicationDrafts(applicationId);
    connectorPreview.value = null;
    const primary = selectedModelByApplication.value[applicationId];
    if (!primary || !value.includes(primary)) {
      selectedModelByApplication.value[applicationId] = value[0] ?? null;
    }
  },
});
const selectedModel = computed<string | null>({
  get: () => selectedModelByApplication.value[currentApplication.value] ?? null,
  set: (value) => {
    const applicationId = currentApplication.value;
    if ((selectedModelByApplication.value[applicationId] ?? null) === value) return;
    selectedModelByApplication.value[applicationId] = value;
    clearApplicationDrafts(applicationId);
    connectorPreview.value = null;
  },
});
const modelValues = ref<Record<string, string>>({});
const snippetDrafts = ref<Record<string, string>>({});
const connectors = ref<ApplicationConnectorItem[]>([]);
const connectorsLoading = ref(false);
const connectorError = ref("");
const connectorPreview = ref<ApplicationConnectorPreview | null>(null);
const connectorPreviewing = ref<ApplicationConnectorAction | null>(null);
const connectorCommitting = ref(false);
const claudeDesktopModelsDirty = computed(() => (
  claudeDesktopModelsLoaded.value
  && CLAUDE_DESKTOP_FIELDS.some((field) => (
    modelValues.value[field] !== claudeDesktopDefaults.value[field]
  ))
));

interface SwitcherKey {
  id: string;
  name: string;
  value: string;
}

const EMPTY_SERVICE_CONFIG = {
  gateway_port: 9042,
  client_root_url: "",
  primary_key: "",
  sub_keys: [] as ConnectionSubKey[],
};
const serviceConfig = computed(() => {
  const connection = connectionStore.info;
  if (!connection) return EMPTY_SERVICE_CONFIG;
  return {
    gateway_port: connection.gateway_port,
    client_root_url: connection.client_root_url,
    primary_key: connection.primary_key,
    sub_keys: connection.sub_keys,
  };
});
const selectedKeyId = ref(PRIMARY_KEY_ID);
const enabledGatewayKeys = computed<SwitcherKey[]>(() => [
  { id: PRIMARY_KEY_ID, name: t("主 Key"), value: serviceConfig.value.primary_key },
  ...serviceConfig.value.sub_keys
    .filter((entry) => entry.enabled && entry.value)
    .map((entry) => ({ id: entry.id, name: entry.name, value: entry.value })),
]);
const selectedKey = computed<SwitcherKey | null>(() => {
  const keys = enabledGatewayKeys.value.filter((entry) => entry.value);
  if (!keys.length) return null;
  return keys.find((entry) => entry.id === selectedKeyId.value) ?? keys[0];
});
const keySelectOptions = computed(() =>
  enabledGatewayKeys.value
    .filter((entry) => entry.value)
    .map((entry) => ({
      label: `${entry.id === PRIMARY_KEY_ID ? t("主 Key") : entry.name} · ${maskConnectionKey(entry.value)}`,
      value: entry.id,
    })),
);
watch(enabledGatewayKeys, (keys) => {
  if (keys.length > 0 && !keys.some((entry) => entry.id === selectedKeyId.value && entry.value)) {
    selectedKeyId.value = keys.find((entry) => entry.value)?.id ?? PRIMARY_KEY_ID;
  }
});
watch(selectedKeyId, () => {
  snippetDrafts.value = {};
  connectorPreview.value = null;
});
function selectGatewayKey(value: string | number | null) {
  if (typeof value !== "string" || value === selectedKeyId.value) return;
  selectedKeyId.value = value;
}

const applicationGuides: readonly ApplicationGuide[] = APPLICATION_GUIDES;
const activeGuide = computed<ApplicationGuide>(() => (
  applicationGuides.find((guide) => guide.id === currentApplication.value)
  ?? applicationGuides[0]
));
const CONNECTOR_IDS = new Set<ApplicationId>([
  "claude-code",
  "codex",
  "dsh",
  "gemini-cli",
  "opencode",
  "openclaw",
  "pi",
  "hermes",
]);
const NATIVE_PLUGIN_CONNECTOR_IDS = new Set<ApplicationId>(["dsh", "pi"]);
const CLIENT_NATIVE_CREDENTIAL_CONNECTOR_IDS = new Set<ApplicationId>(["pi"]);
const connectorEligible = computed(() => (
  isApplicationId(activeGuide.value.id) && CONNECTOR_IDS.has(activeGuide.value.id)
));
const nativePluginConnector = computed(() => (
  isApplicationId(activeGuide.value.id) && NATIVE_PLUGIN_CONNECTOR_IDS.has(activeGuide.value.id)
));
const clientNativeCredentialConnector = computed(() => (
  isApplicationId(activeGuide.value.id)
  && CLIENT_NATIVE_CREDENTIAL_CONNECTOR_IDS.has(activeGuide.value.id)
));
const connectorPanelTitle = computed(() => nativePluginConnector.value
  ? t("本机插件接入")
  : t("本机自动接入"));
const connectorTargetLabel = computed(() => nativePluginConnector.value
  ? t("插件安装目标")
  : t("受管配置"));
const connectorPreviewTitle = computed(() => connectorPreview.value?.action === "restore"
  ? (nativePluginConnector.value ? t("卸载预览") : t("恢复预览"))
  : (nativePluginConnector.value ? t("安装预览") : t("连接预览")));
const connectorConnectPreviewLabel = computed(() => nativePluginConnector.value
  ? t("预览安装")
  : t("预览接入"));
const connectorRestorePreviewLabel = computed(() => nativePluginConnector.value
  ? t("预览卸载")
  : t("预览恢复"));
const connectorCommitLabel = computed(() => connectorPreview.value?.action === "restore"
  ? (nativePluginConnector.value ? t("确认卸载") : t("确认恢复"))
  : (nativePluginConnector.value ? t("确认安装") : t("确认接入")));
const connectorCommitConfirmation = computed(() => {
  if (!nativePluginConnector.value) {
    return t("提交前会重新核对配置文件；发生外部修改时会停止，不会覆盖。写入完成后请重新打开客户端。");
  }
  if (activeGuide.value.id === "dsh") {
    return connectorPreview.value?.action === "restore"
      ? t("提交前会重新核对 DSH 插件和专属 Key 变量；发生外部修改时会停止。卸载后请重新打开 DSH。")
      : t("提交前会重新核对 DSH 插件和专属 Key 变量；只安装 OCG 插件并管理 DSH .env 中的 OCG_MANAGER_API_KEY。完成后请重新打开 DSH。");
  }
  return connectorPreview.value?.action === "restore"
    ? t("提交前会重新核对客户端、插件包与安装状态；发生外部修改时会停止。卸载完成后请重新打开客户端。")
    : t("提交前会重新核对客户端、插件包与安装状态；发生外部修改时会停止。安装完成后请重新打开客户端并使用其原生凭据入口保存 Key。");
});
const activeConnector = computed(() => (
  connectors.value.find((connector) => connector.id === activeGuide.value.id) ?? null
));
const connectorStatusLabel = computed(() => nativePluginConnector.value
  ? nativePluginStatusText(activeConnector.value?.status)
  : connectorStatusText(activeConnector.value?.status));
const connectorTagType = computed<"default" | "success" | "warning" | "error" | "info">(() => {
  switch (activeConnector.value?.status) {
    case "connected": return "success";
    case "ready": return "info";
    case "conflict":
    case "partial": return "error";
    case "manual_only": return "warning";
    default: return "default";
  }
});
const connectorDetail = computed(() => {
  if (connectorsLoading.value && !activeConnector.value) return t("正在检测本机客户端…");
  return activeConnector.value?.detail
    ?? (connectorEligible.value
      ? t("仅安装版 Desktop 可修改本机配置；其他运行方式继续使用下方手动教程。")
      : "");
});
const connectorCanConnect = computed(() => (
  (clientNativeCredentialConnector.value || Boolean(selectedKey.value?.value))
  && Boolean(activeConnector.value?.automatic)
  && ["ready", "connected"].includes(activeConnector.value?.status ?? "")
));
const connectorCanRestore = computed(() => (
  Boolean(activeConnector.value?.automatic)
  && activeConnector.value?.status === "connected"
));
function groupGuidesByCategory(guides: readonly ApplicationGuide[]) {
  const groups = new Map<string, ApplicationGuide[]>();
  for (const guide of guides) {
    const list = groups.get(guide.category) ?? [];
    list.push(guide);
    groups.set(guide.category, list);
  }
  return [...groups.entries()];
}
function guideOptionLabel(guide: ApplicationGuide): string {
  return guide.popular ? `${guide.name} · ${t("常用")}` : guide.name;
}
const applicationMenuOptions = computed<MenuOption[]>(() => {
  const groups = groupGuidesByCategory(applicationGuides);
  return groups.map(([category, guides]) => ({
    type: "group",
    label: t(category as MessageKey),
    key: `group:${category}`,
    children: guides.map((guide) => ({
      key: guide.id,
      label: guideOptionLabel(guide),
    })),
  }));
});
const applicationSelectOptions = computed<(SelectOption | SelectGroupOption)[]>(() => {
  const groups = groupGuidesByCategory(applicationGuides);
  return groups.map(([category, guides]) => ({
    type: "group",
    label: t(category as MessageKey),
    key: `group:${category}`,
    children: guides.map((guide) => ({
      value: guide.id,
      label: guideOptionLabel(guide),
    })),
  }));
});
const activeModelOptions = computed<SelectOption[]>(() => (
  activeGuide.value.id === "claude-desktop"
    ? modelOptions.value
    : applicationModelIds.value.map((modelId) => ({ label: modelId, value: modelId }))
));
const primaryModelOptions = computed<SelectOption[]>(() => (
  activeGuide.value.multipleModels
    ? activeModelOptions.value.filter(({ value }) => typeof value === "string" && selectedModels.value.includes(value))
    : activeModelOptions.value
));
const selectedApplicationModelIds = computed(() => {
  if (activeGuide.value.modelFields?.length) {
    return activeGuide.value.modelFields.map((field) => modelValues.value[field]);
  }
  return activeGuide.value.multipleModels
    ? selectedModels.value
    : [selectedModel.value];
});
const usesMuseContributor = computed(() => (
  selectedApplicationModelIds.value.includes("muse-spark-1.2-contributor")
));

const effectiveModelAlias = computed<string>(() => {
  return selectedApplicationAlias(
    activeGuide.value.modelFields,
    modelValues.value,
    selectedModel.value,
  );
});

const connectionUrls = computed(() => {
  try {
    return resolveConnectionUrls(
      serviceConfig.value.client_root_url,
      window.location.origin,
      serviceConfig.value.gateway_port,
      import.meta.env.DEV,
    );
  } catch {
    return resolveConnectionUrls(
      "",
      window.location.origin,
      serviceConfig.value.gateway_port,
      import.meta.env.DEV,
    );
  }
});
const maskedKey = computed(() => maskConnectionKey(selectedKey.value?.value ?? ""));
const guideContext = computed<GuideContext>(() => ({
  rootUrl: connectionUrls.value.rootUrl,
  apiBaseUrl: connectionUrls.value.apiBaseUrl,
  chatCompletionsUrl: connectionUrls.value.chatCompletionsUrl,
  responsesUrl: connectionUrls.value.responsesUrl,
  messagesUrl: connectionUrls.value.messagesUrl,
  displayKey: maskedKey.value,
  actualKey: selectedKey.value?.value ?? "",
  modelId: activeGuide.value.modelFields?.length
    ? modelValues.value[activeGuide.value.modelFields[0]] || "<MODEL_ID>"
    : selectedModel.value?.trim() || "<MODEL_ID>",
  modelIds: selectedModels.value,
  availableModelIds: applicationModelIds.value,
  modelValues: modelValues.value,
  iconUrl: new URL(logoUrl, window.location.origin).href,
}));
const currentSnippets = computed(() => activeGuide.value.snippets(guideContext.value));
const geminiCliBaseUrlAllowed = computed(() => isGeminiCliBaseUrlAllowed(connectionUrls.value.rootUrl));
const canGenerateConfig = computed(() => (
  settingsLoaded.value
  && Boolean(selectedKey.value?.value)
  && (activeGuide.value.id !== "gemini-cli" || geminiCliBaseUrlAllowed.value)
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
    return { url };
  }
  if (activeGuide.value.endpointKind === "responses") {
    return { url: connectionUrls.value.responsesUrl };
  }
  if (activeGuide.value.endpointKind === "gemini") {
    return {
      url: `${connectionUrls.value.rootUrl}/v1beta/models/${guideContext.value.modelId}:generateContent`,
    };
  }
  return { url: connectionUrls.value.chatCompletionsUrl };
});
const copyDisabledHint = computed(() => {
  if (settingsLoading.value) return t("设置加载完成后可复制");
  if (settingsError.value) return t("设置加载失败，请先重试");
  if (!selectedKey.value?.value) return t("请先在仪表盘设置 Key");
  if (modelsLoading.value) return t("模型加载完成后可复制");
  if (modelsError.value && activeGuide.value.id === "claude-desktop" && !claudeDesktopModelsLoaded.value) {
    return modelsError.value;
  }
  return "";
});

function connectorStatusText(status: ApplicationConnectorStatus | undefined): string {
  switch (status) {
    case "not_detected": return t("未检测到");
    case "manual_only": return t("仅手动");
    case "ready": return t("可以接入");
    case "connected": return t("已接入");
    case "conflict": return t("配置冲突");
    case "partial": return t("部分完成");
    default: return t("当前运行方式不支持");
  }
}

function nativePluginStatusText(status: ApplicationConnectorStatus | undefined): string {
  switch (status) {
    case "ready": return t("可以安装");
    case "connected": return t("已安装");
    default: return connectorStatusText(status);
  }
}

function connectorModelValues(): Record<string, string> {
  const guide = activeGuide.value;
  if (guide.modelFields?.length) {
    return Object.fromEntries(
      guide.modelFields
        .map((field) => [field, modelValues.value[field]?.trim() ?? ""])
        .filter(([, value]) => Boolean(value)),
    );
  }
  const values: Record<string, string> = {};
  if (selectedModel.value?.trim()) values.model = selectedModel.value.trim();
  if (guide.multipleModels && selectedModels.value.length) {
    values.models = selectedModels.value.join("\n");
  }
  return values;
}

async function loadConnectors() {
  if (!connectorEligible.value || connectorsLoading.value) return;
  connectorsLoading.value = true;
  connectorError.value = "";
  try {
    connectors.value = (await dashboardApi.getApplicationConnectors()).items;
  } catch (error) {
    connectorError.value = error instanceof Error ? error.message : String(error);
  } finally {
    connectorsLoading.value = false;
  }
}

async function previewConnector(action: ApplicationConnectorAction) {
  if (connectorPreviewing.value || connectorCommitting.value) return;
  connectorPreviewing.value = action;
  connectorError.value = "";
  connectorPreview.value = null;
  try {
    connectorPreview.value = await dashboardApi.previewApplicationConnector(
      activeGuide.value.id,
      {
        action,
        keyId: action === "connect" && !clientNativeCredentialConnector.value
          ? selectedKeyId.value
          : null,
        modelValues: connectorModelValues(),
      },
    );
  } catch (error) {
    connectorError.value = error instanceof Error ? error.message : String(error);
  } finally {
    connectorPreviewing.value = null;
  }
}

async function commitConnector() {
  const preview = connectorPreview.value;
  if (!preview || connectorCommitting.value) return;
  connectorCommitting.value = true;
  connectorError.value = "";
  try {
    const result = await dashboardApi.commitApplicationConnector(activeGuide.value.id, {
      action: preview.action,
      keyId: preview.action === "connect" && !clientNativeCredentialConnector.value
        ? selectedKeyId.value
        : null,
      modelValues: connectorModelValues(),
      previewFingerprint: preview.fingerprint,
    });
    connectorPreview.value = null;
    await loadConnectors();
    message.success(result.changed
      ? (nativePluginConnector.value
        ? (preview.action === "connect"
          ? (activeGuide.value.id === "dsh"
            ? t("插件和本机凭据已安装；重新打开 DSH 后生效")
            : t("插件已安装；重新打开客户端并在其凭据入口保存 Key"))
          : t("插件已卸载；重新打开客户端后生效"))
        : (preview.action === "connect"
          ? t("本机配置已接入；重新打开客户端后生效")
          : t("本机配置已恢复；重新打开客户端后生效")))
      : t("当前配置无需更改"));
  } catch (error) {
    connectorError.value = error instanceof Error ? error.message : String(error);
  } finally {
    connectorCommitting.value = false;
  }
}

function readApplication(): ApplicationId {
  const value = new URLSearchParams(window.location.search).get("app");
  return isApplicationId(value) ? value : DEFAULT_APPLICATION;
}

function selectApplication(value: string | number | null) {
  if (typeof value !== "string" || !isApplicationId(value) || value === currentApplication.value) return;
  currentApplication.value = value;
  connectorPreview.value = null;
  connectorError.value = "";
  writeApplicationUrl(value, "push");
  void loadConnectors();
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
  connectorPreview.value = null;
  connectorError.value = "";
  void loadConnectors();
}

function onWindowFocus() {
  void loadConnectors();
}

async function loadModels() {
  modelsLoading.value = true;
  modelsError.value = "";
  claudeDesktopModelsLoaded.value = false;
  const errors: string[] = [];
  try {
    const [modelsResult, desktopResult] = await Promise.allSettled([
      dashboardApi.getApplicationModels(),
      settingsStore.loadClaudeDesktop(),
    ]);
    const modelIds = modelsResult.status === "fulfilled"
      ? modelsResult.value
      : applicationModelIds.value;
    if (modelsResult.status === "rejected") {
      errors.push(modelsResult.reason instanceof Error ? modelsResult.reason.message : String(modelsResult.reason));
    } else {
      applicationModelIds.value = modelIds;
      if (!modelIds.length) errors.push(t("未返回可用模型"));
    }
    const claudeDesktopModels = desktopResult.status === "fulfilled" ? {
      sonnet: desktopResult.value.sonnet,
      opus: desktopResult.value.opus,
      haiku: desktopResult.value.haiku,
    } : undefined;
    if (desktopResult.status === "rejected") {
      errors.push(desktopResult.reason instanceof Error ? desktopResult.reason.message : String(desktopResult.reason));
    }
    const availableIds = [...new Set([
      ...modelIds,
      ...Object.values(claudeDesktopModels ?? claudeDesktopDefaults.value).filter(Boolean),
    ])];
    modelOptions.value = availableIds.map((modelId) => ({ label: modelId, value: modelId }));
    const defaultSelectedModels = modelIds;
    const fallbackModel = modelIds[0] ?? "";
    for (const guide of applicationGuides) {
      if (!isApplicationId(guide.id)) continue;
      if (!guide.modelFields?.length) {
        const selection = reconcileApplicationModelSelection(
          selectedModelsByApplication.value[guide.id],
          selectedModelByApplication.value[guide.id],
          modelIds,
          defaultSelectedModels,
          Boolean(guide.multipleModels),
        );
        let changed = false;
        if (
          guide.multipleModels
          && !sameStringArray(selectedModelsByApplication.value[guide.id], selection.selectedModels)
        ) {
          selectedModelsByApplication.value[guide.id] = selection.selectedModels;
          changed = true;
        }
        if ((selectedModelByApplication.value[guide.id] ?? null) !== selection.selectedModel) {
          selectedModelByApplication.value[guide.id] = selection.selectedModel;
          changed = true;
        }
        if (changed) clearApplicationDrafts(guide.id);
        continue;
      }
      if (guide.id === "claude-desktop") continue;
      let changed = false;
      for (const field of guide.modelFields) {
        if (!modelIds.includes(modelValues.value[field])) {
          const nextModel = guide.id === "claude-code"
            ? recommendClaudeCodeModel(field, modelIds) || fallbackModel
            : fallbackModel;
          if (modelValues.value[field] !== nextModel) {
            modelValues.value[field] = nextModel;
            changed = true;
          }
        }
      }
      if (changed) clearApplicationDrafts(guide.id);
    }
    if (claudeDesktopModels) {
      claudeDesktopDefaults.value = { ...claudeDesktopModels };
      let changed = false;
      for (const field of CLAUDE_DESKTOP_FIELDS) {
        const current = modelValues.value[field];
        const nextModel = current && availableIds.includes(current)
          ? current
          : claudeDesktopModels[field];
        if (current !== nextModel) {
          modelValues.value[field] = nextModel;
          changed = true;
        }
      }
      if (changed) clearApplicationDrafts("claude-desktop");
      claudeDesktopModelsLoaded.value = true;
    }
    modelsError.value = errors.join(t("；"));
  } finally {
    modelsLoading.value = false;
  }
}

async function loadSettings(loadApplicationModels = true) {
  settingsLoading.value = true;
  settingsLoaded.value = false;
  settingsError.value = "";
  try {
    // The view only needs connection fields; the lightweight payload keeps
    // it off the full settings shape.
    const previousServiceConfig = serviceConfig.value;
    const connection = await connectionStore.load();
    const nextServiceConfig = {
      gateway_port: connection.gateway_port,
      client_root_url: connection.client_root_url,
      primary_key: connection.primary_key,
      sub_keys: connection.sub_keys,
    };
    snippetDrafts.value = reconcileConnectionDrafts(
      {
        gateway_port: previousServiceConfig.gateway_port,
        gateway_key: selectedKey.value?.value ?? "",
        client_root_url: previousServiceConfig.client_root_url,
      },
      {
        gateway_port: nextServiceConfig.gateway_port,
        gateway_key: (
          nextServiceConfig.primary_key
          && selectedKeyId.value === PRIMARY_KEY_ID
            ? nextServiceConfig.primary_key
            : nextServiceConfig.sub_keys.find((entry) => entry.id === selectedKeyId.value)?.value
        ) ?? nextServiceConfig.primary_key,
        client_root_url: nextServiceConfig.client_root_url,
      },
      snippetDrafts.value,
    );
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

function updateModelField(field: string, value: string | number | null) {
  const nextModel = typeof value === "string" ? value : "";
  if (modelValues.value[field] === nextModel) return;
  modelValues.value[field] = nextModel;
  clearApplicationDrafts(activeGuide.value.id);
  connectorPreview.value = null;
}

function currentClaudeDesktopModels(): ClaudeDesktopModels {
  return {
    sonnet: modelValues.value.sonnet,
    opus: modelValues.value.opus,
    haiku: modelValues.value.haiku,
  };
}

async function saveClaudeDesktopModels(): Promise<boolean> {
  if (!claudeDesktopModelsLoaded.value) return false;
  if (!claudeDesktopModelsDirty.value) return true;
  if (claudeDesktopModelsSaving.value) return false;
  claudeDesktopModelsSaving.value = true;
  try {
    const result = await settingsStore.putClaudeDesktop(currentClaudeDesktopModels());
    const persisted = { sonnet: result.sonnet, opus: result.opus, haiku: result.haiku };
    Object.assign(modelValues.value, persisted);
    claudeDesktopDefaults.value = persisted;
    message.success(t("设置已保存"));
    return true;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    message.error(t("模型映射保存失败: {error}", { error: detail }));
    return false;
  } finally {
    claudeDesktopModelsSaving.value = false;
  }
}

function sameStringArray(left: readonly string[] | undefined, right: readonly string[]): boolean {
  if (!left) return false;
  return left.length === right.length
    && left.every((value, index) => value === right[index]);
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
    if (!(await saveClaudeDesktopModels())) return;
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

onMounted(() => {
  const value = new URLSearchParams(window.location.search).get("app");
  if (!isApplicationId(value)) writeApplicationUrl(currentApplication.value, "replace");
  window.addEventListener("popstate", onPopState);
  window.addEventListener("focus", onWindowFocus);
  void loadSettings().then(() => loadConnectors());
});

onActivated(() => {
  if (!settingsLoading.value) void loadSettings().then(() => loadConnectors());
});

onUnmounted(() => {
  window.removeEventListener("popstate", onPopState);
  window.removeEventListener("focus", onWindowFocus);
  cleanup();
});
</script>

<style scoped>
.applications {
  width: min(1280px, 100%);
  min-width: 0;
  margin: 0 auto;
}

.page-header {
  margin-bottom: 24px;
}

.page-header h1 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-xl)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}

.page-header p {
  margin: 8px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-md);
  line-height: 1.6;
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

.connection-panel {
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}

.connection-panel-title {
  margin: 0 0 16px;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}

.native-connector-panel {
  display: grid;
  gap: 14px;
  min-width: 0;
  padding: 16px;
  border: 1px solid color-mix(in srgb, var(--ocg-primary) 35%, var(--ocg-border));
  border-radius: 14px;
  background: color-mix(in srgb, var(--ocg-primary) 5%, var(--ocg-surface));
  box-shadow: var(--ocg-shadow-sm);
}

.native-connector-head,
.native-connector-title-row,
.connector-actions,
.connector-preview-head {
  display: flex;
  align-items: center;
}

.native-connector-head {
  justify-content: space-between;
  gap: 16px;
}

.native-connector-title-row {
  flex-wrap: wrap;
  gap: 8px;
}

.native-connector-title-row h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}

.native-connector-head p,
.connector-preview p {
  margin: 6px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
  line-height: 1.55;
}

.connector-paths {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.connector-paths > span {
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
  font-weight: 700;
}

.connector-paths code,
.connector-preview code {
  padding: 2px 6px;
  border-radius: 5px;
  background: var(--ocg-surface);
  color: var(--ocg-ink);
  font: var(--ocg-font-sm)/1.5 "Cascadia Mono", Consolas, monospace;
}

.connector-preview {
  padding: 12px;
  border: 1px solid var(--ocg-divider);
  border-radius: 10px;
  background: var(--ocg-surface);
}

.connector-preview-head {
  justify-content: space-between;
  gap: 12px;
}

.connector-preview-head span {
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
}

.connector-preview ul {
  display: grid;
  gap: 8px;
  margin: 12px 0 0;
  padding: 0;
  list-style: none;
}

.connector-preview li {
  display: grid;
  grid-template-columns: minmax(120px, 1fr) minmax(80px, 1fr) auto minmax(80px, 1fr);
  gap: 8px;
  align-items: center;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

.connector-preview li span {
  overflow-wrap: anywhere;
}

.connector-actions {
  flex-wrap: wrap;
  gap: 8px;
}

.access-fields {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(220px, 280px);
  gap: 12px;
}

.access-field {
  display: grid;
  gap: 6px;
  min-width: 0;
}

.access-field > span {
  color: var(--ocg-subtle);
  font: 700 var(--ocg-font-sm)/1.2 "Cascadia Mono", Consolas, monospace;
  letter-spacing: 0.04em;
}

.access-value {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.access-value :deep(.n-select) {
  flex: 1 1 140px;
  min-width: 0;
}

.access-value code {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  color: var(--ocg-ink);
  font: var(--ocg-font-md)/1.5 "Cascadia Mono", Consolas, monospace;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.access-field--alias {
  grid-column: 1 / -1;
}

.alias-explainer {
  margin: 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
  line-height: 1.5;
}

.guide-head,
.guide-title-row,
.snippet-card > header,
.model-row {
  display: flex;
  align-items: center;
}

.copy-disabled-hint {
  margin: 12px 0 0;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
  line-height: 1.5;
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
  padding-top: 12px;
  border-top: 1px solid var(--ocg-divider);
}

.model-label {
  color: var(--ocg-ink);
  font-size: var(--ocg-font-md);
  font-weight: 700;
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
  font: var(--ocg-font-sm)/1.2 "Cascadia Mono", Consolas, monospace;
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
  font: 700 var(--ocg-font-2xl)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}

.guide-head p {
  margin: 8px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-md);
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
  font-size: var(--ocg-font-md);
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
  font: 700 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}

.snippet-backup-warning {
  margin: -2px 0 12px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
  line-height: 1.5;
}

.guide-section ol,
.guide-section ul {
  display: grid;
  gap: 8px;
  margin: 0;
  padding-left: 24px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-md);
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
  font-size: var(--ocg-font-md);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.snippet-card header > span {
  margin-right: auto;
  color: var(--ocg-subtle);
  font: var(--ocg-font-sm)/1 "Cascadia Mono", Consolas, monospace;
  text-transform: uppercase;
}

.snippet-editor {
  padding: 12px;
}

.snippet-editor :deep(.n-input__textarea-el) {
  font: var(--ocg-font-md)/1.6 "Cascadia Mono", Consolas, monospace;
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
  .access-fields {
    grid-template-columns: 1fr;
  }

  .model-controls {
    width: 100%;
  }
}

@media (max-width: 640px) {
  .application-page {
    gap: 12px;
  }

  .page-header {
    margin-bottom: 16px;
  }

  .page-header h1 {
    font-size: var(--ocg-font-lg);
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
