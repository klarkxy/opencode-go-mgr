<template>
  <div class="cpa-page">
    <header class="cpa-header">
      <div>
        <h1>{{ t("CPA 本机接入") }}</h1>
        <p>{{ t("将 OCG 连接到你自行安装的本机 CLI Proxy API。OAuth 凭据始终由 CPA 保存。") }}</p>
      </div>
      <n-button secondary :loading="loading" @click="load">
        {{ t("重试") }}
      </n-button>
    </header>

    <n-alert type="info" :show-icon="false">
      <strong>{{ t("安装 CPA") }}</strong>
      <p>{{ t("桌面与 CLI 请先在本机启动 CPA；Docker 可启用 cpa profile，使两个容器在同一 Compose 网络中通信。OCG 不会安装、启动或升级 CPA。") }}</p>
    </n-alert>

    <n-alert v-if="loadError" type="error" :title="t('加载 CPA 失败: {error}', { error: loadError })">
      <n-button size="small" secondary :loading="loading" @click="load">{{ t("重试") }}</n-button>
    </n-alert>

    <template v-else-if="integration">
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
          <StatusCell :label="t('模型目录')" :ready="integration.modelCount > 0" :detail="modelCatalogDetail" />
        </div>
        <n-button
          class="cpa-refresh-models"
          :disabled="!integration.configured"
          :loading="refreshingModels"
          @click="refreshModels"
        >{{ t("刷新模型目录") }}</n-button>
        <p v-if="integration.modelCount === 0" class="cpa-help">{{ t("尚未刷新模型目录；启用路由前请先刷新。") }}</p>
      </n-card>

      <n-card size="small" :title="t('CPA OAuth 账号')" class="cpa-card">
        <template #header-extra>
          <span class="cpa-muted">{{ t("由 CPA 管理；OCG 不读取或保存 OAuth Token。") }}</span>
        </template>
        <n-space wrap class="oauth-providers">
          <n-button
            v-for="provider in oauthProviders"
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
          <article v-for="account in cpaAccounts" :key="accountKey(account)" class="cpa-account-row">
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
              <span v-if="account.quota !== null" class="cpa-muted">{{ t("配额") }} · {{ formatQuota(account.quota) }}</span>
            </div>
            <n-space v-if="account.mutable && !account.runtimeOnly && account.authIndex" wrap>
              <n-button size="small" :loading="accountAction === accountKey(account)" @click="setAccountStatus(account, !account.disabled)">
                {{ account.disabled ? t("启用") : t("停用") }}
              </n-button>
              <n-button v-if="account.authIndex" size="small" :loading="accountAction === accountKey(account)" @click="resetQuota(account)">
                {{ t("重置配额") }}
              </n-button>
              <n-button size="small" type="error" secondary :loading="accountAction === accountKey(account)" @click="confirmDeleteAccount(account)">
                {{ t("删除") }}
              </n-button>
            </n-space>
          </article>
        </div>
      </n-card>

      <n-card v-if="integration.configured" size="small" class="cpa-card cpa-danger" :title="t('断开并清除')">
        <p>{{ t("确定断开 CPA 并清除 OCG 保存的地址、两把 Key、路由账号和模型目录吗？CPA 自己的 OAuth 数据不会被删除。") }}</p>
        <n-button type="error" secondary :loading="disconnecting" @click="confirmDisconnect">{{ t("断开并清除") }}</n-button>
      </n-card>
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
import type { CpaAccount, CpaConnectionReport, CpaIntegration, CpaOAuthProvider, CpaOAuthStart } from "../api/generated/dashboard-v3.ts";
import { dashboardV3 } from "../api/dashboard-v3.ts";
import { useControlPlaneStore } from "../stores/controlPlane.ts";
import { t } from "../i18n/index.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";

const dialog = useDialog();
const message = useMessage();
const controlPlane = useControlPlaneStore();
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

const draft = ref({ baseUrl: "", inferenceKey: "", managementKey: "" });
const oauthProviders: Array<{ id: CpaOAuthProvider; label: string }> = [
  { id: "codex", label: "Codex" },
  { id: "anthropic", label: "Claude" },
  { id: "antigravity", label: "Antigravity" },
  { id: "kimi", label: "Kimi" },
  { id: "xai", label: "xAI" },
];

const modelCatalogDetail = computed(() => {
  if (!integration.value) return "";
  if (integration.value.modelCount === 0) return t("未测试");
  const refreshed = integration.value.modelsRefreshedAt
    ? new Date(integration.value.modelsRefreshedAt).toLocaleString()
    : "";
  return refreshed ? `${integration.value.modelCount} · ${refreshed}` : String(integration.value.modelCount);
});

async function runMutation<T>(run: (expectation: { expectedRevision: number; processGeneration: number }) => Promise<T>): Promise<T> {
  if (!controlPlane.hasTokens()) await controlPlane.refresh();
  return controlPlane.runMutation(run);
}

async function load(): Promise<void> {
  loading.value = true;
  loadError.value = "";
  try {
    const value = await dashboardV3.getCpaIntegration();
    integration.value = value;
    draft.value.baseUrl = value.baseUrl;
    // Secrets are write-only. Refreshing state must never repopulate either input.
    draft.value.inferenceKey = "";
    draft.value.managementKey = "";
    if (value.configured) await loadAccounts();
    else cpaAccounts.value = [];
  } catch (error) {
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

function accountKey(account: CpaAccount): string {
  return `${account.name}:${account.authIndex ?? ""}`;
}

function formatQuota(value: unknown): string {
  if (typeof value === "string" || typeof value === "number") return String(value);
  try { return JSON.stringify(value); } catch { return "—"; }
}

async function setAccountStatus(account: CpaAccount, disabled: boolean): Promise<void> {
  accountAction.value = accountKey(account);
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
  accountAction.value = accountKey(account);
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
  accountAction.value = accountKey(account);
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
    if (["ok", "completed", "success", "cancelled", "failed", "expired", "error"].includes(status.status.toLowerCase())) {
      stopOAuthPoll();
      oauth.value = null;
      if (["ok", "success", "completed"].includes(status.status.toLowerCase())) await loadAccounts();
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
onDeactivated(cancelOAuthOnLeave);
onBeforeUnmount(() => {
  window.removeEventListener("pagehide", cancelOAuthOnLeave);
  cancelOAuthOnLeave();
});
</script>

<style scoped>
.cpa-page { display: grid; gap: 16px; max-width: 1060px; }
.cpa-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
.cpa-header h1 { margin: 0; color: var(--ocg-ink); font-size: var(--ocg-font-xl); }
.cpa-header p, .cpa-help, .cpa-danger p { margin: 6px 0 0; color: var(--ocg-muted); line-height: 1.6; }
.cpa-card { box-shadow: var(--ocg-shadow-sm); }
.cpa-status-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; }
.cpa-status-cell { display: grid; gap: 6px; min-width: 0; }
.cpa-status-detail, .cpa-muted { overflow-wrap: anywhere; color: var(--ocg-muted); font-size: var(--ocg-font-sm); }
.cpa-refresh-models { margin-top: 14px; }
.cpa-oauth-status { margin-top: 14px; }
.cpa-oauth-status p { margin-top: 0; }
.cpa-state { display: grid; justify-content: center; padding: 20px; }
.cpa-account-list { display: grid; gap: 8px; margin-top: 14px; }
.cpa-account-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 12px; border: 1px solid var(--ocg-divider); border-radius: 10px; }
.cpa-account-main { display: grid; gap: 4px; min-width: 0; }
.cpa-account-title { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; color: var(--ocg-ink); }
.cpa-danger { border-color: color-mix(in srgb, var(--ocg-error) 34%, var(--ocg-divider)); }
@media (max-width: 760px) {
  .cpa-header, .cpa-account-row { align-items: stretch; flex-direction: column; }
  .cpa-status-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
