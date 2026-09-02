<template>
  <div class="cpa-page">
    <header class="cpa-header">
      <div>
        <h1>{{ t("CPA 本机接入") }}</h1>
        <p>{{ modeDescription }}</p>
      </div>
      <n-button secondary :loading="loading" @click="load">
        {{ t("重试") }}
      </n-button>
    </header>

    <n-alert v-if="loadError" type="error" :title="t('加载 CPA 失败: {error}', { error: loadError })">
      <n-button size="small" secondary :loading="loading" @click="load">{{ t("重试") }}</n-button>
    </n-alert>

    <template v-else-if="integration">
      <section class="cpa-section" aria-labelledby="cpa-overview-title">
        <h2 id="cpa-overview-title" class="cpa-section-title">{{ t("概览") }}</h2>
        <n-space class="cpa-mode-choice" align="center" wrap>
          <span class="cpa-muted">{{ t("选择 CPA 使用方式") }}</span>
          <n-button size="small" :type="mode === 'external' ? 'primary' : 'default'" @click="selectMode('external')">
            {{ t("外部连接") }}
          </n-button>
          <n-button
            size="small"
            :type="mode === 'managed' ? 'primary' : 'default'"
            :disabled="!managedRuntimeAvailable"
            @click="selectMode('managed')"
          >{{ t("托管安装") }}</n-button>
        </n-space>

        <n-alert v-if="runtimeError" type="error" :title="t('加载 CPA 运行时失败: {error}', { error: runtimeError })">
          <n-button size="small" secondary :loading="loading" @click="load">{{ t("重试") }}</n-button>
        </n-alert>

        <template v-if="mode === 'external'">
          <n-alert type="info" :show-icon="false">
            <strong>{{ t("安装 CPA") }}</strong>
            <p>{{ t("连接到你自行启动的本机 CPA；OCG 不会接管、停止或更改该进程。") }}</p>
          </n-alert>

          <n-card size="small" :title="t('基础地址')" class="cpa-card">
            <n-form label-placement="top" @submit.prevent="save">
              <n-form-item :label="t('基础地址')">
                <n-input
                  v-model:value="draft.baseUrl"
                  class="mono"
                  :readonly="integration.baseUrlReadOnly"
                  :disabled="saving"
                  :placeholder="integration.baseUrl"
                  :input-props="{ 'aria-label': t('基础地址') }"
                />
              </n-form-item>
              <n-form-item :label="t('Inference Key')">
                <n-input
                  v-model:value="draft.inferenceKey"
                  type="password"
                  show-password-on="click"
                  :disabled="saving"
                  :placeholder="integration.inferenceKeyConfigured ? t('已设置') : t('未设置')"
                  :input-props="{ 'aria-label': t('Inference Key'), autocomplete: 'new-password' }"
                />
              </n-form-item>
              <n-form-item :label="t('Management Key')">
                <n-input
                  v-model:value="draft.managementKey"
                  type="password"
                  show-password-on="click"
                  :disabled="saving"
                  :placeholder="integration.managementKeyConfigured ? t('已设置') : t('未设置')"
                  :input-props="{ 'aria-label': t('Management Key'), autocomplete: 'new-password' }"
                />
              </n-form-item>
              <p class="cpa-help">{{ t("Key 只会在保存或测试时发送，不会重新显示。") }}</p>
              <n-space wrap>
                <n-button type="primary" attr-type="submit" :loading="saving">
                  {{ t("保存 CPA 配置") }}
                </n-button>
                <n-button :loading="testing" @click="testConnection">{{ t("测试连接") }}</n-button>
                <n-switch
                  :value="integration.enabled"
                  :disabled="!integration.configured || saving"
                  @update:value="setRoutingEnabled"
                >
                  <template #checked>{{ t("路由启用") }}</template>
                  <template #unchecked>{{ t("停用") }}</template>
                </n-switch>
              </n-space>
            </n-form>
          </n-card>

          <n-card size="small" :title="t('版本兼容')" class="cpa-card">
            <div class="cpa-status-grid">
              <StatusCell :label="t('可达性')" :ready="report?.reachable ?? null" :detail="report?.version ?? t('未测试')" />
              <StatusCell :label="t('Management 鉴权')" :ready="report?.managementReady ?? null" :detail="report?.managementError" />
              <StatusCell :label="t('Inference 鉴权')" :ready="report?.inferenceReady ?? null" :detail="report?.inferenceError" />
            </div>
          </n-card>

          <n-card v-if="integration.configured" size="small" class="cpa-card cpa-danger" :title="t('断开并清除')">
            <p>{{ t("确定断开 CPA 并清除 OCG 保存的地址、两把 Key、路由账号和模型目录吗？CPA 自己的 OAuth 数据不会被删除。") }}</p>
            <n-button type="error" secondary :loading="disconnecting" @click="confirmDisconnect">{{ t("断开并清除") }}</n-button>
          </n-card>
        </template>

        <template v-else>
          <n-alert v-if="mode === 'unsupported'" type="warning" :title="t('当前环境不支持托管 CPA 运行时')">
            {{ integration.runtimeUnavailableReason || runtime?.unavailableReason || t("仅已安装的 Windows x64 桌面端可以托管 CPA；请改用外部连接。") }}
          </n-alert>

          <template v-else>
            <n-card size="small" :title="t('托管运行时')" class="cpa-card">
              <div class="cpa-status-grid">
                <div class="cpa-status-cell">
                  <span class="cpa-muted">{{ t("运行状态") }}</span>
                  <n-tag :type="integration.runtimeRunning ? 'success' : 'default'" size="small">{{ runtimeStateDetail }}</n-tag>
                </div>
                <div class="cpa-status-cell">
                  <span class="cpa-muted">{{ t("当前版本") }}</span>
                  <span>{{ integration.installedVersion ?? runtime?.currentVersion ?? "—" }}</span>
                </div>
                <div class="cpa-status-cell">
                  <span class="cpa-muted">{{ t("最新版本") }}</span>
                  <span>{{ runtimeCheck?.latestVersion ?? integration.latestVersion ?? runtime?.latestVersion ?? t("未检查") }}</span>
                  <span v-if="runtimeCheck?.updateAvailable ?? integration.updateAvailable" class="cpa-status-detail">
                    {{ t("发现新版本 {version}", { version: runtimeCheck?.latestVersion ?? integration.latestVersion ?? t("未检查") }) }}
                  </span>
                  <span v-else-if="runtimeCheck" class="cpa-status-detail">{{ t("已是最新版本") }}</span>
                </div>
                <div class="cpa-status-cell">
                  <span class="cpa-muted">{{ t("上一版本") }}</span>
                  <span>{{ runtime?.previousVersion ?? "—" }}</span>
                </div>
              </div>

              <div v-if="runtime && (runtime.phase !== 'idle' || runtimeOperationDetail)" class="cpa-phase">
                <n-tag :type="runtime.phase === 'failed' ? 'error' : 'warning'" size="small">
                  {{ runtimePhaseLabel(runtime.phase) }}
                </n-tag>
                <span v-if="runtimeOperationDetail" class="cpa-status-detail">{{ runtimeOperationDetail }}</span>
                <span v-if="runtime.phase === 'failed' && runtime.error" class="cpa-status-detail">{{ runtime.error }}</span>
              </div>

              <n-alert v-if="runtimePollError" type="error" :title="t('CPA 运行时状态刷新失败: {error}', { error: runtimePollError })">
                <n-button size="small" secondary @click="retryRuntimePoll">{{ t("重试") }}</n-button>
              </n-alert>

              <n-space v-if="runtime" wrap class="cpa-runtime-actions">
                <n-button
                  type="primary"
                  size="small"
                  :disabled="!controls.install"
                  :loading="runtimeAction === 'install'"
                  @click="installRuntime"
                >{{ t("安装") }}</n-button>
                <n-button
                  size="small"
                  :disabled="!controls.start"
                  :loading="runtimeAction === 'start'"
                  @click="startRuntime"
                >{{ t("启动") }}</n-button>
                <n-button
                  size="small"
                  :disabled="!controls.stop"
                  :loading="runtimeAction === 'stop'"
                  @click="stopRuntime"
                >{{ t("停止") }}</n-button>
                <n-button
                  size="small"
                  :disabled="!controls.checkUpdate"
                  :loading="runtimeAction === 'checkUpdate'"
                  @click="checkUpdate"
                >{{ t("检查更新") }}</n-button>
                <n-button
                  size="small"
                  :disabled="!controls.update"
                  :loading="runtimeAction === 'update'"
                  @click="updateRuntime"
                >{{ updateLabel }}</n-button>
                <n-button
                  size="small"
                  :disabled="!controls.rollback"
                  :loading="runtimeAction === 'rollback'"
                  @click="rollbackRuntime"
                >{{ rollbackLabel }}</n-button>
                <n-button
                  size="small"
                  type="error"
                  secondary
                  :disabled="!controls.remove"
                  :loading="runtimeAction === 'remove'"
                  @click="confirmRemoveRuntime"
                >{{ t("移除") }}</n-button>
              </n-space>

              <div class="cpa-logs">
                <n-button size="small" secondary :aria-expanded="logsExpanded" @click="toggleLogs">
                  {{ logsExpanded ? t("收起日志") : t("查看日志") }}
                </n-button>
                <template v-if="logsExpanded">
                  <div v-if="logsLoading" class="cpa-state"><n-spin size="small" /></div>
                  <n-alert v-else-if="logsError" type="error" :title="t('加载 CPA 运行时日志失败: {error}', { error: logsError })">
                    <n-button size="small" secondary @click="refreshLogs">{{ t("重试") }}</n-button>
                  </n-alert>
                  <template v-else-if="logs">
                    <div class="cpa-log-actions">
                      <n-button size="tiny" quaternary :loading="logsLoading" @click="refreshLogs">{{ t("刷新日志") }}</n-button>
                    </div>
                    <template v-if="stdoutTail || stderrTail">
                      <h3 class="cpa-log-title">{{ t("标准输出") }}</h3>
                      <pre class="cpa-log mono">{{ stdoutTail || t("暂无日志") }}</pre>
                      <h3 class="cpa-log-title">{{ t("标准错误") }}</h3>
                      <pre class="cpa-log mono">{{ stderrTail || t("暂无日志") }}</pre>
                    </template>
                    <n-empty v-else :description="t('暂无日志')" />
                  </template>
                </template>
              </div>
            </n-card>
          </template>
        </template>
      </section>

      <section class="cpa-section" aria-labelledby="cpa-accounts-title">
        <h2 id="cpa-accounts-title" class="cpa-section-title">{{ t("账号") }}</h2>

        <n-card size="small" :title="t('模型目录')" class="cpa-card">
          <n-space align="center" wrap>
            <n-button
              :disabled="!integration.configured"
              :loading="refreshingModels"
              @click="refreshModels"
            >{{ t("刷新模型目录") }}</n-button>
            <span class="cpa-muted">{{ modelCatalogDetail }}</span>
          </n-space>
          <p v-if="integration.modelCount === 0" class="cpa-help">{{ t("尚未刷新模型目录；启用路由前请先刷新。") }}</p>
        </n-card>

        <n-card size="small" :title="t('CPA OAuth 账号')" class="cpa-card">
          <template #header-extra>
            <span class="cpa-muted">{{ t("由 CPA 管理；OCG 不读取或保存 OAuth Token。") }}</span>
          </template>
          <n-space wrap class="oauth-providers">
            <n-button
              v-for="provider in CPA_OAUTH_PROVIDERS"
              :key="provider.id"
              secondary
              :disabled="!integration.configured || !!oauth || !!oauthStartingProvider"
              :loading="oauthStartingProvider === provider.id"
              @click="startOAuth(provider.id)"
            >{{ t("登录 {provider}", { provider: provider.label }) }}</n-button>
          </n-space>
          <n-alert v-if="oauth" type="info" class="cpa-oauth-status" :show-icon="false">
            <p>{{ t("正在等待 CPA 完成授权…") }}</p>
            <n-space align="center" wrap>
              <n-button v-if="oauth.url" size="small" type="primary" tag="a" :href="oauth.url" target="_blank" rel="noopener noreferrer">
                {{ t("打开授权页面") }}
              </n-button>
              <n-tag v-if="oauth.userCode" type="warning">{{ t("设备码：{code}", { code: oauth.userCode }) }}</n-tag>
              <n-button size="small" secondary :loading="oauthCancelling" @click="cancelOAuth">{{ t("取消当前授权") }}</n-button>
            </n-space>
          </n-alert>

          <div v-if="accountsLoading" class="cpa-state"><n-spin size="small" /></div>
          <n-alert v-else-if="accountsError" type="error" :title="t('CPA 账号操作失败: {error}', { error: accountsError })">
            <n-button size="small" secondary @click="loadAccounts">{{ t("重试") }}</n-button>
          </n-alert>
          <n-empty v-else-if="cpaAccounts.length === 0" :description="t('暂无账号')" />
          <div v-else class="cpa-account-list">
            <article v-for="account in cpaAccounts" :key="cpaAccountKey(account)" class="cpa-account-row">
              <div class="cpa-account-main">
                <div class="cpa-account-title">
                  <strong>{{ account.label || account.name }}</strong>
                  <n-tag v-if="account.runtimeOnly" type="warning" size="small">{{ t("运行时插件账号，仅供查看") }}</n-tag>
                  <n-tag v-else :type="account.disabled || account.unavailable ? 'default' : 'success'" size="small">
                    {{ account.status || (account.disabled ? t("已禁用") : account.unavailable ? t("不可用") : t("可用")) }}
                  </n-tag>
                </div>
                <span class="cpa-muted">{{ account.provider }}<template v-if="account.email"> · {{ account.email }}</template></span>
                <span v-if="account.statusMessage" class="cpa-muted">{{ account.statusMessage }}</span>
                <span v-if="account.quota !== null" class="cpa-muted">{{ t("配额") }} · {{ formatCpaQuota(account.quota) }}</span>
              </div>
              <n-space v-if="account.mutable && !account.runtimeOnly && account.authIndex" wrap>
                <n-button size="small" :loading="accountAction === cpaAccountKey(account)" @click="setAccountStatus(account, !account.disabled)">
                  {{ account.disabled ? t("启用") : t("停用") }}
                </n-button>
                <n-button v-if="account.authIndex" size="small" :loading="accountAction === cpaAccountKey(account)" @click="resetQuota(account)">
                  {{ t("重置配额") }}
                </n-button>
                <n-button size="small" type="error" secondary :loading="accountAction === cpaAccountKey(account)" @click="confirmDeleteAccount(account)">
                  {{ t("删除") }}
                </n-button>
              </n-space>
            </article>
          </div>
        </n-card>
      </section>

      <section v-if="showClientKeys" class="cpa-section" aria-labelledby="cpa-keys-title">
        <h2 id="cpa-keys-title" class="cpa-section-title">{{ t("客户端 Key") }}</h2>

        <n-card size="small" class="cpa-card">
          <template #header-extra>
            <n-space wrap>
              <n-button
                size="small"
                type="primary"
                :disabled="!!keyAction"
                :loading="keyAction === 'create'"
                @click="addClientKey"
              >{{ t("添加客户端 Key") }}</n-button>
            </n-space>
          </template>

          <n-alert v-if="revealedSecret" type="success" class="cpa-secret" :title="t('新 Key 仅显示这一次')">
            <p>{{ t("请立即复制并妥善保存；关闭后将无法再次查看。") }}</p>
            <div class="cpa-secret-row">
              <code class="mono cpa-secret-value">{{ revealedSecret.secret }}</code>
              <n-space wrap>
                <n-button size="small" @click="copyRevealedSecret">
                  {{ copiedTarget === "cpa-runtime-secret" ? t("已复制 Key") : t("复制 Key") }}
                </n-button>
                <n-button size="small" secondary @click="dismissRevealedSecret">{{ t("我已保存，关闭") }}</n-button>
              </n-space>
            </div>
          </n-alert>

          <div v-if="keysLoading" class="cpa-state"><n-spin size="small" /></div>
          <n-alert v-else-if="keysError" type="error" :title="t('加载客户端 Key 失败: {error}', { error: keysError })">
            <n-button size="small" secondary @click="loadRuntimeKeys">{{ t("重试") }}</n-button>
          </n-alert>
          <template v-else>
            <div v-if="runtimeKeys.length" class="cpa-key-list">
              <article v-for="key in keyPartition.protectedKeys" :key="key.fingerprint" class="cpa-key-row">
                <div class="cpa-account-main">
                  <div class="cpa-account-title">
                    <strong class="mono">{{ key.hint }}</strong>
                    <n-tag type="info" size="small">{{ t("OCG 路由 Key") }}</n-tag>
                  </div>
                  <span class="cpa-muted">{{ t("指纹") }} · {{ key.fingerprint }}</span>
                  <span class="cpa-muted">{{ t("由 OCG 管理的路由 Key，不能删除。") }}</span>
                </div>
                <n-button
                  size="small"
                  secondary
                  :disabled="!!keyAction"
                  :loading="keyAction === `rotate:${key.fingerprint}`"
                  @click="rotateClientKey(key)"
                >{{ t("轮换 OCG 路由 Key") }}</n-button>
              </article>
              <article v-for="key in keyPartition.directKeys" :key="key.fingerprint" class="cpa-key-row">
                <div class="cpa-account-main">
                  <strong class="mono">{{ key.hint }}</strong>
                  <span class="cpa-muted">{{ t("指纹") }} · {{ key.fingerprint }}</span>
                </div>
                <n-space wrap>
                  <n-button
                    size="small"
                    secondary
                    :disabled="!!keyAction"
                    :loading="keyAction === `rotate:${key.fingerprint}`"
                    @click="rotateClientKey(key)"
                  >{{ t("轮换 Key") }}</n-button>
                  <n-button
                    size="small"
                    type="error"
                    secondary
                    :disabled="!!keyAction"
                    :loading="keyAction === `delete:${key.fingerprint}`"
                    @click="confirmDeleteClientKey(key)"
                  >{{ t("删除") }}</n-button>
                </n-space>
              </article>
            </div>
            <n-empty v-else :description="t('暂无客户端 Key')" />
          </template>
        </n-card>
      </section>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, h, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from "vue";
import {
  NAlert,
  NButton,
  NCard,
  NEmpty,
  NForm,
  NFormItem,
  NInput,
  NSpin,
  NSpace,
  NSwitch,
  NTag,
  useDialog,
  useMessage,
} from "naive-ui";
import type {
  CpaAccount,
  CpaConnectionReport,
  CpaIntegration,
  CpaOAuthProvider,
  CpaOAuthStart,
  CpaRuntime,
  CpaRuntimeCheck,
  CpaRuntimeKey,
  CpaRuntimeLogs,
  CpaRuntimePhase,
  MutationExpectation,
} from "../api/generated/dashboard-v3.ts";
import { dashboardV3 } from "../api/dashboard-v3.ts";
import { useControlPlaneStore } from "../stores/controlPlane.ts";
import { t } from "../i18n/index.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";
import { useClipboard } from "../utils/format.ts";
import {
  CPA_OAUTH_PROVIDERS,
  cpaAccountKey,
  cpaClientKeysAvailable,
  cpaLogTail,
  cpaManagedRuntimeConfirmed,
  cpaRuntimeControls,
  cpaRuntimeMode,
  formatCpaQuota,
  partitionCpaRuntimeKeys,
  isCpaOAuthSuccessStatus,
  isCpaOAuthTerminalStatus,
  isCpaPhaseBusy,
} from "../domain/cpa-runtime.ts";
import type { CpaRuntimeModePreference } from "../domain/cpa-runtime.ts";

const dialog = useDialog();
const message = useMessage();
const controlPlane = useControlPlaneStore();
const { copiedTarget, copy, cleanup: cleanupClipboard } = useClipboard();

const integration = ref<CpaIntegration | null>(null);
const report = ref<CpaConnectionReport | null>(null);
const cpaAccounts = ref<CpaAccount[]>([]);
const loading = ref(false);
const loadError = ref("");
const saving = ref(false);
const testing = ref(false);
const refreshingModels = ref(false);
const accountsLoading = ref(false);
const accountsError = ref("");
const accountAction = ref("");
const disconnecting = ref(false);
const oauth = ref<CpaOAuthStart | null>(null);
const oauthStartingProvider = ref<CpaOAuthProvider | null>(null);
const oauthCancelling = ref(false);
let oauthTimer: number | null = null;

const runtime = ref<CpaRuntime | null>(null);
const runtimeError = ref("");
const runtimePollError = ref("");
const runtimeCheck = ref<CpaRuntimeCheck | null>(null);
const runtimeAction = ref("");
let runtimeTimer: number | null = null;
let runtimePollGeneration = 0;

const logs = ref<CpaRuntimeLogs | null>(null);
const logsExpanded = ref(false);
const logsLoading = ref(false);
const logsError = ref("");

const runtimeKeys = ref<CpaRuntimeKey[]>([]);
const keysLoading = ref(false);
const keysError = ref("");
const keyAction = ref("");
// One-time reveal area: the only component state that ever holds a client-key
// secret. Dismiss or any page refresh clears it; list rows stay secret-free.
const revealedSecret = ref<{ fingerprint: string; hint: string; secret: string } | null>(null);

const draft = ref({ baseUrl: "", inferenceKey: "", managementKey: "" });
const modePreference = ref<CpaRuntimeModePreference>(null);

const mode = computed(() => (
  integration.value ? cpaRuntimeMode(integration.value, runtime.value, modePreference.value) : "external"
));
const managedRuntimeAvailable = computed(() => (
  integration.value ? cpaManagedRuntimeConfirmed(integration.value, runtime.value) : false
));
const showClientKeys = computed(() => cpaClientKeysAvailable(runtime.value));
const controls = computed(() => cpaRuntimeControls({
  runtime: runtime.value,
  busy: runtimeAction.value !== "",
  updateCheck: runtimeCheck.value,
}));
const keyPartition = computed(() => partitionCpaRuntimeKeys(runtimeKeys.value));
const stdoutTail = computed(() => cpaLogTail(logs.value?.stdout ?? ""));
const stderrTail = computed(() => cpaLogTail(logs.value?.stderr ?? ""));

const modeDescription = computed(() => (
  mode.value === "managed"
    ? t("CPA 由 OCG 在本机托管安装与运行。OAuth 凭据始终由 CPA 保存。")
    : t("将 OCG 连接到你自行安装的本机 CLI Proxy API。OAuth 凭据始终由 CPA 保存。")
));

const runtimeStateDetail = computed(() => {
  if (!runtime.value || !runtime.value.installed) return t("未安装");
  return integration.value?.runtimeRunning ? t("运行中") : t("已停止");
});

const runtimeOperationDetail = computed(() => (
  integration.value?.currentOperation ?? runtime.value?.currentOperation ?? ""
));

const updateLabel = computed(() => (
  (runtimeCheck.value?.updateAvailable ?? integration.value?.updateAvailable)
    ? t("更新到 {version}", { version: runtimeCheck.value?.latestVersion ?? integration.value?.latestVersion ?? t("未检查") })
    : t("更新")
));

const rollbackLabel = computed(() => (
  runtime.value?.previousVersion
    ? t("回滚到 {version}", { version: runtime.value.previousVersion })
    : t("回滚")
));

const modelCatalogDetail = computed(() => {
  if (!integration.value) return "";
  if (integration.value.modelCount === 0) return t("未测试");
  const refreshed = integration.value.modelsRefreshedAt
    ? new Date(integration.value.modelsRefreshedAt).toLocaleString()
    : "";
  return refreshed ? `${integration.value.modelCount} · ${refreshed}` : String(integration.value.modelCount);
});

function runtimePhaseLabel(phase: CpaRuntimePhase): string {
  switch (phase) {
    case "checking": return t("检查中");
    case "downloading": return t("下载中");
    case "installing": return t("安装中");
    case "starting": return t("启动中");
    case "failed": return t("失败");
    default: return t("空闲");
  }
}

function selectMode(next: Exclude<CpaRuntimeModePreference, null>): void {
  if (next === "managed" && !managedRuntimeAvailable.value) return;
  modePreference.value = next;
}

async function runMutation<T>(run: (expectation: MutationExpectation) => Promise<T>): Promise<T> {
  if (!controlPlane.hasTokens()) await controlPlane.refresh();
  return controlPlane.runMutation(run);
}

async function load(): Promise<void> {
  bumpRuntimePollGeneration();
  const generation = runtimePollGeneration;
  loading.value = true;
  loadError.value = "";
  runtimePollError.value = "";
  // A full refresh never keeps a previously revealed secret around.
  revealedSecret.value = null;
  try {
    const value = await dashboardV3.getCpaIntegration();
    if (generation !== runtimePollGeneration) return;
    integration.value = value;
    draft.value.baseUrl = value.baseUrl;
    // Secrets are write-only. Refreshing state must never repopulate either input.
    draft.value.inferenceKey = "";
    draft.value.managementKey = "";
    runtimeError.value = "";
    try {
      const snapshot = await dashboardV3.getCpaRuntime();
      if (generation !== runtimePollGeneration) return;
      runtime.value = snapshot;
      applyRuntimeSnapshot(snapshot);
    } catch (error) {
      if (generation !== runtimePollGeneration) return;
      runtime.value = null;
      runtimeError.value = dashboardErrorDetail(error);
    }
    if (value.configured) await loadAccounts();
    else cpaAccounts.value = [];
    if (generation !== runtimePollGeneration) return;
    if (cpaClientKeysAvailable(runtime.value)) await loadRuntimeKeys();
    else runtimeKeys.value = [];
    if (generation !== runtimePollGeneration) return;
    syncRuntimePolling();
  } catch (error) {
    if (generation !== runtimePollGeneration) return;
    loadError.value = dashboardErrorDetail(error);
  } finally {
    loading.value = false;
  }
}

async function save(): Promise<void> {
  if (saving.value || !integration.value) return;
  saving.value = true;
  try {
    const updated = await runMutation((expectation) => dashboardV3.putCpaIntegration({
      ...(integration.value!.baseUrlReadOnly ? {} : { baseUrl: draft.value.baseUrl.trim() || null }),
      ...(draft.value.inferenceKey.trim() ? { inferenceKey: draft.value.inferenceKey.trim() } : {}),
      ...(draft.value.managementKey.trim() ? { managementKey: draft.value.managementKey.trim() } : {}),
      enabled: integration.value!.enabled,
    }, expectation));
    integration.value = updated;
    draft.value.baseUrl = updated.baseUrl;
    draft.value.inferenceKey = "";
    draft.value.managementKey = "";
    message.success(t("CPA 配置已保存"));
    await loadAccounts();
  } catch (error) {
    message.error(t("CPA 配置失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    saving.value = false;
  }
}

async function testConnection(): Promise<void> {
  if (testing.value || !integration.value) return;
  testing.value = true;
  try {
    report.value = await dashboardV3.testCpaIntegration({
      ...(integration.value.baseUrlReadOnly ? {} : { baseUrl: draft.value.baseUrl.trim() || null }),
      ...(draft.value.inferenceKey.trim() ? { inferenceKey: draft.value.inferenceKey.trim() } : {}),
      ...(draft.value.managementKey.trim() ? { managementKey: draft.value.managementKey.trim() } : {}),
    });
  } catch (error) {
    message.error(t("CPA 连接测试失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    testing.value = false;
  }
}

async function setRoutingEnabled(enabled: boolean): Promise<void> {
  if (!integration.value || saving.value) return;
  saving.value = true;
  try {
    integration.value = await runMutation((expectation) => dashboardV3.putCpaIntegration({ enabled }, expectation));
  } catch (error) {
    message.error(t("CPA 配置失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    saving.value = false;
  }
}

async function refreshModels(): Promise<void> {
  if (refreshingModels.value) return;
  refreshingModels.value = true;
  try {
    const models = await runMutation((expectation) => dashboardV3.refreshCpaModels(expectation));
    if (integration.value) {
      integration.value = { ...integration.value, modelCount: models.models.length, modelsRefreshedAt: models.refreshedAt };
    }
    message.success(t("模型目录已刷新，共 {count} 个模型", { count: models.models.length }));
  } catch (error) {
    message.error(t("CPA 模型刷新失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    refreshingModels.value = false;
  }
}

async function loadAccounts(): Promise<void> {
  accountsLoading.value = true;
  accountsError.value = "";
  try {
    cpaAccounts.value = (await dashboardV3.getCpaAccounts()).accounts;
  } catch (error) {
    accountsError.value = dashboardErrorDetail(error);
  } finally {
    accountsLoading.value = false;
  }
}

async function setAccountStatus(account: CpaAccount, disabled: boolean): Promise<void> {
  accountAction.value = cpaAccountKey(account);
  try {
    await runMutation((expectation) => dashboardV3.setCpaAccountStatus({
      name: account.name,
      authIndex: account.authIndex!,
      disabled,
    }, expectation));
    await loadAccounts();
  } catch (error) {
    message.error(t("CPA 账号操作失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    accountAction.value = "";
  }
}

async function resetQuota(account: CpaAccount): Promise<void> {
  if (!account.authIndex) return;
  accountAction.value = cpaAccountKey(account);
  try {
    await runMutation((expectation) => dashboardV3.resetCpaQuota({
      name: account.name,
      authIndex: account.authIndex!,
    }, expectation));
    await loadAccounts();
  } catch (error) {
    message.error(t("CPA 账号操作失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    accountAction.value = "";
  }
}

function confirmDeleteAccount(account: CpaAccount): void {
  dialog.warning({
    title: t("删除 CPA 账号"),
    content: t("确定删除 CPA 中的账号 {name} 吗？这会删除 CPA 保存的 OAuth 凭据。", { name: account.label || account.name }),
    positiveText: t("删除"),
    negativeText: t("取消"),
    onPositiveClick: () => deleteAccount(account),
  });
}

async function deleteAccount(account: CpaAccount): Promise<void> {
  accountAction.value = cpaAccountKey(account);
  try {
    await runMutation((expectation) => dashboardV3.deleteCpaAccount({
      name: account.name,
      authIndex: account.authIndex!,
    }, expectation));
    await loadAccounts();
  } catch (error) {
    message.error(t("CPA 账号操作失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    accountAction.value = "";
  }
}

async function startOAuth(provider: CpaOAuthProvider): Promise<void> {
  if (oauth.value || oauthStartingProvider.value) return;
  oauthStartingProvider.value = provider;
  try {
    const started = await runMutation((expectation) => dashboardV3.startCpaOAuth({ provider }, expectation));
    oauth.value = started;
    if (started.url) window.open(started.url, "_blank", "noopener,noreferrer");
    scheduleOAuthPoll();
  } catch (error) {
    message.error(t("CPA 账号操作失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    oauthStartingProvider.value = null;
  }
}

function scheduleOAuthPoll(): void {
  stopOAuthPoll();
  oauthTimer = window.setInterval(() => void pollOAuth(), 3000);
}

function stopOAuthPoll(): void {
  if (oauthTimer !== null) window.clearInterval(oauthTimer);
  oauthTimer = null;
}

async function pollOAuth(): Promise<void> {
  const active = oauth.value;
  if (!active) return;
  try {
    const status = await dashboardV3.getCpaOAuthStatus(active.state);
    if (isCpaOAuthTerminalStatus(status.status)) {
      stopOAuthPoll();
      oauth.value = null;
      if (isCpaOAuthSuccessStatus(status.status)) await loadAccounts();
      else if (status.error) message.warning(status.error);
    }
  } catch (error) {
    stopOAuthPoll();
    oauth.value = null;
    message.error(t("CPA 账号操作失败: {error}", { error: dashboardErrorDetail(error) }));
  }
}

async function cancelOAuth(): Promise<void> {
  const active = oauth.value;
  if (!active) return;
  oauthCancelling.value = true;
  try {
    await runMutation((expectation) => dashboardV3.cancelCpaOAuth({ state: active.state }, expectation));
  } catch {
    // A page close must not surface a second error over the original OAuth result.
  } finally {
    stopOAuthPoll();
    oauth.value = null;
    oauthCancelling.value = false;
  }
}

function cancelOAuthOnLeave(): void {
  if (!oauth.value) return;
  void cancelOAuth();
}

function confirmDisconnect(): void {
  dialog.warning({
    title: t("断开并清除"),
    content: t("确定断开 CPA 并清除 OCG 保存的地址、两把 Key、路由账号和模型目录吗？CPA 自己的 OAuth 数据不会被删除。"),
    positiveText: t("断开并清除"),
    negativeText: t("取消"),
    onPositiveClick: () => disconnect(),
  });
}

async function disconnect(): Promise<void> {
  disconnecting.value = true;
  try {
    await runMutation((expectation) => dashboardV3.deleteCpaIntegration(expectation));
    integration.value = await dashboardV3.getCpaIntegration();
    cpaAccounts.value = [];
    report.value = null;
    draft.value = { baseUrl: integration.value.baseUrl, inferenceKey: "", managementKey: "" };
    message.success(t("CPA 已断开"));
  } catch (error) {
    message.error(t("CPA 配置失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    disconnecting.value = false;
  }
}

// --- managed runtime lifecycle ---

function bumpRuntimePollGeneration(): void {
  runtimePollGeneration += 1;
  stopRuntimePoll();
}

function syncRuntimePolling(): void {
  stopRuntimePoll();
  if (runtime.value && isCpaPhaseBusy(runtime.value.phase)) {
    runtimeTimer = window.setTimeout(() => void pollRuntime(), 2000);
  }
}

function stopRuntimePoll(): void {
  if (runtimeTimer !== null) window.clearTimeout(runtimeTimer);
  runtimeTimer = null;
}

async function pollRuntime(): Promise<void> {
  const generation = runtimePollGeneration;
  runtimePollError.value = "";
  try {
    const next = await dashboardV3.getCpaRuntime();
    if (generation !== runtimePollGeneration) return;
    runtime.value = next;
    applyRuntimeSnapshot(next);
    if (!isCpaPhaseBusy(next.phase)) {
      stopRuntimePoll();
      await refreshAfterRuntimeSettled();
    } else {
      syncRuntimePolling();
    }
  } catch (error) {
    if (generation !== runtimePollGeneration) return;
    stopRuntimePoll();
    runtimePollError.value = dashboardErrorDetail(error);
  }
}

function retryRuntimePoll(): Promise<void> {
  return pollRuntime();
}

/** Keep the Overview truthful while the full integration refresh is in flight. */
function applyRuntimeSnapshot(snapshot: CpaRuntime): void {
  if (!integration.value) return;
  integration.value = {
    ...integration.value,
    currentOperation: snapshot.currentOperation,
    installedVersion: snapshot.currentVersion,
    latestVersion: snapshot.latestVersion,
    runtimeOwned: snapshot.owned,
    runtimeRunning: snapshot.running,
    runtimeSupported: snapshot.supported,
    runtimeUnavailableReason: snapshot.unavailableReason,
    updateAvailable: snapshot.updateAvailable,
  };
}

/** A settled lifecycle operation can change routing config, accounts, models, and key eligibility. */
async function refreshAfterRuntimeSettled(): Promise<void> {
  const generation = runtimePollGeneration;
  try {
    const next = await dashboardV3.getCpaIntegration();
    if (generation !== runtimePollGeneration) return;
    integration.value = next;
  } catch {
    // The runtime snapshot remains visible and the header retry can recover the integration read.
    return;
  }
  if (integration.value.configured) await loadAccounts();
  else cpaAccounts.value = [];
  if (generation !== runtimePollGeneration) return;
  if (cpaClientKeysAvailable(runtime.value)) await loadRuntimeKeys();
  else runtimeKeys.value = [];
}

async function runRuntimeAction(
  name: string,
  run: (expectation: MutationExpectation) => Promise<CpaRuntime>,
): Promise<void> {
  if (runtimeAction.value) return;
  bumpRuntimePollGeneration();
  const generation = runtimePollGeneration;
  runtimeAction.value = name;
  runtimePollError.value = "";
  // A lifecycle change invalidates the previous update check.
  runtimeCheck.value = null;
  try {
    const next = await runMutation(run);
    if (generation !== runtimePollGeneration) return;
    runtime.value = next;
    applyRuntimeSnapshot(next);
    if (isCpaPhaseBusy(next.phase)) syncRuntimePolling();
    else {
      stopRuntimePoll();
      await refreshAfterRuntimeSettled();
    }
  } catch (error) {
    if (generation !== runtimePollGeneration) return;
    message.error(t("CPA 运行时操作失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    runtimeAction.value = "";
  }
}

function installRuntime(): Promise<void> {
  return runRuntimeAction("install", (expectation) => dashboardV3.installCpaRuntime({}, expectation));
}

function startRuntime(): Promise<void> {
  return runRuntimeAction("start", (expectation) => dashboardV3.startCpaRuntime(expectation));
}

function stopRuntime(): Promise<void> {
  return runRuntimeAction("stop", (expectation) => dashboardV3.stopCpaRuntime(expectation));
}

async function checkUpdate(): Promise<void> {
  if (runtimeAction.value) return;
  bumpRuntimePollGeneration();
  const generation = runtimePollGeneration;
  runtimeAction.value = "checkUpdate";
  try {
    runtimeCheck.value = await runMutation((expectation) => dashboardV3.checkCpaRuntimeUpdate(expectation));
    if (generation !== runtimePollGeneration) return;
    runtime.value = await dashboardV3.getCpaRuntime();
    if (generation !== runtimePollGeneration) return;
    applyRuntimeSnapshot(runtime.value);
    syncRuntimePolling();
  } catch (error) {
    if (generation !== runtimePollGeneration) return;
    message.error(t("CPA 运行时操作失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    runtimeAction.value = "";
  }
}

function updateRuntime(): Promise<void> {
  const check = runtimeCheck.value;
  if (!check?.updateAvailable) return Promise.resolve();
  return runRuntimeAction("update", (expectation) => dashboardV3.updateCpaRuntime({
    expectedVersion: check.latestVersion,
  }, expectation));
}

function rollbackRuntime(): Promise<void> {
  return runRuntimeAction("rollback", (expectation) => dashboardV3.rollbackCpaRuntime(expectation));
}

function confirmRemoveRuntime(): void {
  dialog.warning({
    title: t("移除 CPA 运行时"),
    content: t("确定移除 OCG 管理的 CPA 运行时吗？本机安装文件、CPA OAuth 凭据和本地运行时配置将被删除。"),
    positiveText: t("移除"),
    negativeText: t("取消"),
    onPositiveClick: () => runRuntimeAction("remove", (expectation) => dashboardV3.removeCpaRuntime(expectation)),
  });
}

// --- runtime logs ---

function toggleLogs(): void {
  logsExpanded.value = !logsExpanded.value;
  if (logsExpanded.value && !logs.value) void refreshLogs();
}

async function refreshLogs(): Promise<void> {
  if (logsLoading.value) return;
  logsLoading.value = true;
  logsError.value = "";
  try {
    logs.value = await dashboardV3.getCpaRuntimeLogs();
  } catch (error) {
    logsError.value = dashboardErrorDetail(error);
  } finally {
    logsLoading.value = false;
  }
}

// --- client keys ---

async function loadRuntimeKeys(): Promise<void> {
  keysLoading.value = true;
  keysError.value = "";
  try {
    runtimeKeys.value = (await dashboardV3.getCpaRuntimeKeys()).keys;
  } catch (error) {
    keysError.value = dashboardErrorDetail(error);
  } finally {
    keysLoading.value = false;
  }
}

async function addClientKey(): Promise<void> {
  if (keyAction.value) return;
  keyAction.value = "create";
  try {
    const created = await runMutation((expectation) => dashboardV3.createCpaRuntimeKey(expectation));
    revealedSecret.value = { fingerprint: created.fingerprint, hint: created.hint, secret: created.secret };
    await loadRuntimeKeys();
  } catch (error) {
    message.error(t("客户端 Key 操作失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    keyAction.value = "";
  }
}

async function rotateClientKey(key: CpaRuntimeKey): Promise<void> {
  if (keyAction.value) return;
  keyAction.value = `rotate:${key.fingerprint}`;
  try {
    const rotated = await runMutation((expectation) => dashboardV3.rotateCpaRuntimeKey(key.fingerprint, expectation));
    revealedSecret.value = { fingerprint: rotated.fingerprint, hint: rotated.hint, secret: rotated.secret };
    await loadRuntimeKeys();
  } catch (error) {
    message.error(t("客户端 Key 操作失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    keyAction.value = "";
  }
}

function confirmDeleteClientKey(key: CpaRuntimeKey): void {
  if (key.protected) return;
  dialog.warning({
    title: t("删除客户端 Key"),
    content: t("确定删除客户端 Key {hint} 吗？使用该 Key 的客户端将立即失效。", { hint: key.hint }),
    positiveText: t("删除"),
    negativeText: t("取消"),
    onPositiveClick: () => deleteClientKey(key),
  });
}

async function deleteClientKey(key: CpaRuntimeKey): Promise<void> {
  keyAction.value = `delete:${key.fingerprint}`;
  try {
    await runMutation((expectation) => dashboardV3.deleteCpaRuntimeKey(key.fingerprint, expectation));
    if (revealedSecret.value?.fingerprint === key.fingerprint) revealedSecret.value = null;
    await loadRuntimeKeys();
  } catch (error) {
    message.error(t("客户端 Key 操作失败: {error}", { error: dashboardErrorDetail(error) }));
  } finally {
    keyAction.value = "";
  }
}

async function copyRevealedSecret(): Promise<void> {
  if (!revealedSecret.value) return;
  try {
    await copy("cpa-runtime-secret", revealedSecret.value.secret, "Key");
    message.success(t("已复制 Key"));
  } catch (error) {
    message.error(error instanceof Error ? error.message : t("复制失败"));
  }
}

function dismissRevealedSecret(): void {
  revealedSecret.value = null;
}

const StatusCell = (props: { label: string; ready: boolean | null; detail?: string | null }) => h("div", { class: "cpa-status-cell" }, [
  h("span", { class: "cpa-muted" }, props.label),
  h(NTag, { type: props.ready === true ? "success" : props.ready === false ? "error" : "default", size: "small" }, {
    default: () => props.ready === true ? t("已就绪") : props.ready === false ? t("未就绪") : t("未测试"),
  }),
  props.detail ? h("span", { class: "cpa-status-detail" }, props.detail) : null,
]);

onMounted(() => {
  window.addEventListener("pagehide", cancelOAuthOnLeave);
  void load();
});
onActivated(() => { if (!loading.value) void load(); });
onDeactivated(() => {
  cancelOAuthOnLeave();
  bumpRuntimePollGeneration();
});
onBeforeUnmount(() => {
  window.removeEventListener("pagehide", cancelOAuthOnLeave);
  cancelOAuthOnLeave();
  bumpRuntimePollGeneration();
  cleanupClipboard();
});
</script>

<style scoped>
.cpa-page { display: grid; gap: 16px; max-width: 1060px; }
.cpa-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
.cpa-header h1 { margin: 0; color: var(--ocg-ink); font-size: var(--ocg-font-xl); }
.cpa-header p, .cpa-help, .cpa-danger p { margin: 6px 0 0; color: var(--ocg-muted); line-height: 1.6; }
.cpa-section { display: grid; gap: 12px; }
.cpa-section-title { margin: 8px 0 0; color: var(--ocg-ink); font-size: var(--ocg-font-lg); }
.cpa-card { box-shadow: var(--ocg-shadow-sm); }
.cpa-status-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 12px; }
.cpa-status-cell { display: grid; gap: 6px; min-width: 0; align-content: start; }
.cpa-status-detail, .cpa-muted { overflow-wrap: anywhere; color: var(--ocg-muted); font-size: var(--ocg-font-sm); }
.cpa-phase { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-top: 14px; }
.cpa-runtime-actions { margin-top: 14px; }
.cpa-logs { display: grid; gap: 10px; margin-top: 14px; justify-items: start; }
.cpa-log-actions { display: flex; justify-content: flex-end; width: 100%; }
.cpa-log-title { margin: 0; color: var(--ocg-muted); font-size: var(--ocg-font-sm); font-weight: 600; }
.cpa-log {
  box-sizing: border-box;
  width: 100%;
  max-height: 240px;
  margin: 0;
  padding: 10px 12px;
  overflow: auto;
  border: 1px solid var(--ocg-divider);
  border-radius: 8px;
  background: var(--ocg-surface);
  color: var(--ocg-ink);
  font-size: var(--ocg-font-xs);
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.cpa-oauth-status { margin-top: 14px; }
.cpa-oauth-status p { margin-top: 0; }
.cpa-state { display: grid; justify-content: center; padding: 20px; }
.cpa-account-list, .cpa-key-list { display: grid; gap: 8px; margin-top: 14px; }
.cpa-account-row, .cpa-key-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 12px; border: 1px solid var(--ocg-divider); border-radius: 10px; }
.cpa-account-main { display: grid; gap: 4px; min-width: 0; }
.cpa-account-title { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; color: var(--ocg-ink); }
.cpa-secret { margin-bottom: 14px; }
.cpa-secret p { margin: 0 0 10px; }
.cpa-secret-row { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; }
.cpa-secret-value {
  padding: 6px 10px;
  border: 1px solid var(--ocg-divider);
  border-radius: 6px;
  background: var(--ocg-surface);
  color: var(--ocg-ink);
  overflow-wrap: anywhere;
}
.cpa-danger { border-color: color-mix(in srgb, var(--ocg-error) 34%, var(--ocg-divider)); }
@media (max-width: 760px) {
  .cpa-header, .cpa-account-row, .cpa-key-row { align-items: stretch; flex-direction: column; }
  .cpa-status-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
