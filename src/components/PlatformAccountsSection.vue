<template>
  <n-card class="platform-section" size="small">
    <template #header>
      <span class="platform-section-title">{{ t("平台账号") }}</span>
      <n-tag v-if="view" size="small" :bordered="false" class="platform-count-tag">
        {{ view.accounts.length }}
      </n-tag>
    </template>
    <template #header-extra>
      <n-dropdown
        trigger="click"
        :options="addOptions"
        @select="openCreate"
      >
        <n-button size="small" type="primary" secondary>
          <template #icon>
            <n-icon :component="PlusOutlined" />
          </template>
          {{ t("添加平台账号") }}
        </n-button>
      </n-dropdown>
    </template>

    <div v-if="loading" class="platform-state" role="status" :aria-label="t('加载中…')">
      <n-spin size="small" />
    </div>
    <n-alert
      v-else-if="loadError"
      type="error"
      :title="t('加载平台账号失败: {error}', { error: loadError })"
    >
      <n-button size="small" secondary @click="load">{{ t("重试") }}</n-button>
    </n-alert>
    <n-empty
      v-else-if="view && view.accounts.length === 0"
      :description="t('暂无平台账号')"
    />

    <n-collapse v-else-if="view" v-model:expanded-names="expanded">
      <n-collapse-item
        v-for="parent in view.accounts"
        :key="parent.id"
        :name="parent.id"
      >
        <template #header>
          <span class="platform-parent-title">
            <n-tag size="small" :bordered="false">{{ kindLabel(parent.kind) }}</n-tag>
            <span class="platform-parent-name">{{ parent.name }}</span>
            <span class="mono platform-parent-url">{{ parent.baseUrl }}</span>
            <n-tag v-if="parent.snapshot?.stale" size="small" type="warning" :bordered="false">
              {{ t("快照已过期") }}
            </n-tag>
          </span>
        </template>
        <template #header-extra>
          <n-space size="small" @click.stop>
            <n-button
              size="tiny"
              quaternary
              :loading="!!refreshing[parent.id]"
              :disabled="mutating"
              @click="refreshParent(parent)"
            >{{ t("刷新") }}</n-button>
            <n-button size="tiny" quaternary :disabled="mutating" @click="openEdit(parent)">
              {{ t("编辑") }}
            </n-button>
            <n-tooltip v-if="linksFor(parent).length > 0" trigger="hover">
              <template #trigger>
                <span>
                  <n-button size="tiny" quaternary disabled>{{ t("删除") }}</n-button>
                </span>
              </template>
              {{ t("已关联 {count} 个 Key，先取消关联后再删除", { count: linksFor(parent).length }) }}
            </n-tooltip>
            <n-button
              v-else
              size="tiny"
              quaternary
              :disabled="mutating"
              @click="confirmDelete(parent)"
            >{{ t("删除") }}</n-button>
          </n-space>
        </template>

        <div class="platform-parent-body">
          <n-alert
            v-if="parent.snapshot && parent.snapshot.errors.length > 0"
            type="warning"
            :show-icon="false"
            class="platform-block"
          >
            {{ t("刷新错误") }}: {{ parent.snapshot.errors.join(", ") }}
          </n-alert>

          <div v-if="parent.snapshot" class="platform-observed">
            {{ t("观测时间：{time}", { time: observedText(parent.snapshot) }) }}
          </div>
          <div v-else class="platform-observed">
            {{ t("尚无平台数据，请手动刷新。") }}
          </div>

          <template v-if="parent.snapshot">
            <div
              v-for="kind in PARENT_QUOTA_KINDS"
              :key="kind"
              class="platform-block"
            >
              <div class="platform-block-title">{{ t(quotaKindKeys[kind] as MessageKey) }}</div>
              <div v-if="quotasOf(parent.snapshot, kind).length === 0" class="platform-unknown">
                {{ t("未知") }}
              </div>
              <div v-for="(quota, index) in quotasOf(parent.snapshot, kind)" :key="index" class="quota-row">
                <span class="quota-values">
                  <template v-if="quota.unlimited">{{ t("不限") }}</template>
                  <template v-else>
                    {{ t("已用 {value}", { value: quotaAmount(quota.used, quota.unit) }) }}
                    · {{ t("剩余 {value}", { value: quotaAmount(quota.remaining, quota.unit) }) }}
                    · {{ t("限额 {value}", { value: quotaAmount(quota.limit, quota.unit) }) }}
                  </template>
                </span>
                <span class="quota-meta">
                  <span v-if="quota.scopeId" class="mono">{{ quota.scopeId }}</span>
                  <span v-if="quota.period">{{ t("周期：{period}", { period: quota.period }) }}</span>
                  <span v-if="quota.resetsAt">{{ t("重置时间：{time}", { time: timeText(quota.resetsAt) }) }}</span>
                  <span v-if="quota.expiresAt">{{ t("到期时间：{time}", { time: timeText(quota.expiresAt) }) }}</span>
                  <span>{{ t("来源：{source}", { source: quota.source }) }}</span>
                </span>
              </div>
            </div>

            <div v-if="parent.snapshot.groups.length > 0" class="platform-block">
              <div class="platform-block-title">{{ t("分组") }}</div>
              <n-space size="small" wrap>
                <n-tag
                  v-for="(group, index) in parent.snapshot.groups"
                  :key="index"
                  size="small"
                  :bordered="false"
                >{{ groupLabel(group) || t("无分组") }}</n-tag>
              </n-space>
              <div class="platform-hint">{{ t("分组归属不代表该 Key 拥有对应模型的调用权限。") }}</div>
            </div>

            <PlatformPriceTable :snapshot="parent.snapshot" />
          </template>

          <div class="platform-block">
            <div class="platform-children-head">
              <span class="platform-block-title">
                {{ t("已关联 Key（{count}）", { count: linksFor(parent).length }) }}
              </span>
              <n-button
                size="tiny"
                secondary
                :disabled="mutating"
                @click="openLink(parent)"
              >{{ t("关联 Key") }}</n-button>
            </div>
            <div v-if="linksFor(parent).length === 0" class="platform-hint">
              {{ t("关联为手动操作：在新增账号流程中粘贴 Key 创建 Custom API 账号后，再回到这里关联。") }}
            </div>
            <div
              v-for="link in linksFor(parent)"
              :key="link.accountId"
              class="platform-child"
            >
              <template v-if="accountOf(link.accountId)">
                <div class="platform-child-head">
                  <span class="platform-child-name">{{ accountOf(link.accountId)!.name }}</span>
                  <n-tag v-if="groupLabel(link.group)" size="small" :bordered="false">
                    {{ groupLabel(link.group) }}
                  </n-tag>
                  <n-tag
                    v-for="autoGroup in link.group.autoGroups"
                    :key="autoGroup"
                    size="small"
                    :bordered="false"
                    type="info"
                  >{{ autoGroup }}</n-tag>
                  <n-space size="small" class="platform-child-actions">
                    <n-button
                      size="tiny"
                      quaternary
                      :loading="!!refreshing[`${parent.id}:${link.accountId}`]"
                      :disabled="mutating"
                      @click="refreshChild(parent, link)"
                    >{{ t("刷新") }}</n-button>
                    <n-button
                      size="tiny"
                      quaternary
                      :disabled="mutating"
                      @click="openImport(accountOf(link.accountId)!, link)"
                    >{{ t("导入模型") }}</n-button>
                    <n-button
                      size="tiny"
                      quaternary
                      :disabled="mutating"
                      @click="confirmUnlink(accountOf(link.accountId)!, link)"
                    >{{ t("取消关联") }}</n-button>
                  </n-space>
                </div>
                <div class="platform-hint">
                  {{ t("关联期间 Endpoint 由平台账号托管") }}<template v-if="accountOf(link.accountId)!.custom_config">
                    ：<span class="mono">{{ accountOf(link.accountId)!.custom_config!.endpoint_url }}</span>
                  </template>
                </div>
                <div v-if="link.snapshot">
                  <div
                    v-for="kind in CHILD_QUOTA_KINDS"
                    :key="kind"
                  >
                    <template v-if="kind === 'key_limit' || quotasOf(link.snapshot, kind).length > 0">
                      <div class="platform-block-title">
                        {{ t(quotaKindKeys[kind] as MessageKey) }}
                        <n-tag v-if="kind !== 'key_limit'" size="small" type="info" :bordered="false">
                          {{ t("经此 Key 观测") }}
                        </n-tag>
                      </div>
                      <div v-if="quotasOf(link.snapshot, kind).length === 0" class="platform-unknown">
                        {{ t("未知") }}
                      </div>
                      <div
                        v-for="(quota, index) in quotasOf(link.snapshot, kind)"
                        :key="index"
                        class="quota-row"
                      >
                        <span class="quota-values">
                          <template v-if="quota.unlimited">{{ t("不限") }}</template>
                          <template v-else>
                            {{ t("已用 {value}", { value: quotaAmount(quota.used, quota.unit) }) }}
                            · {{ t("剩余 {value}", { value: quotaAmount(quota.remaining, quota.unit) }) }}
                            · {{ t("限额 {value}", { value: quotaAmount(quota.limit, quota.unit) }) }}
                          </template>
                        </span>
                        <span class="quota-meta">
                          <span v-if="quota.scopeId" class="mono">{{ quota.scopeId }}</span>
                          <span v-if="quota.period">{{ t("周期：{period}", { period: quota.period }) }}</span>
                          <span v-if="quota.resetsAt">{{ t("重置时间：{time}", { time: timeText(quota.resetsAt) }) }}</span>
                          <span v-if="quota.expiresAt">{{ t("到期时间：{time}", { time: timeText(quota.expiresAt) }) }}</span>
                          <span>{{ t("来源：{source}", { source: quota.source }) }}</span>
                        </span>
                      </div>
                    </template>
                  </div>
                  <PlatformPriceTable :snapshot="link.snapshot" />
                  <div class="platform-observed">
                    {{ t("观测时间：{time}", { time: observedText(link.snapshot) }) }}
                    <n-tag v-if="link.snapshot.stale" size="small" type="warning" :bordered="false">
                      {{ t("快照已过期") }}
                    </n-tag>
                  </div>
                  <n-alert
                    v-if="link.snapshot.errors.length > 0"
                    type="warning"
                    :show-icon="false"
                  >
                    {{ t("刷新错误") }}: {{ link.snapshot.errors.join(", ") }}
                  </n-alert>
                </div>
                <div v-else class="platform-hint">{{ t("尚无 Key 快照，请手动刷新。") }}</div>
              </template>
            </div>
          </div>
        </div>
      </n-collapse-item>
    </n-collapse>

    <PlatformAccountFormModal
      :show="showForm"
      :editing="editingPlatform"
      :preset-kind="presetKind"
      :busy="mutating"
      @update:show="showForm = $event"
      @save="onFormSave"
    />
    <PlatformLinkModal
      :show="showLink"
      :parent="linkParent"
      :candidates="linkCandidates"
      :busy="mutating"
      @update:show="showLink = $event"
      @submit="onLinkSubmit"
    />
    <PlatformModelImportModal
      :show="!!importTarget"
      :account="importTarget?.account ?? null"
      :link="importTarget?.link ?? null"
      :busy="mutating"
      @update:show="setImportVisible"
      @submit="onImportSubmit"
    />
  </n-card>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NCard,
  NCollapse,
  NCollapseItem,
  NDropdown,
  NEmpty,
  NIcon,
  NSpace,
  NSpin,
  NTag,
  NTooltip,
  useDialog,
  useMessage,
} from "naive-ui";
import { PlusOutlined } from "@vicons/antd";
import { dashboardApi, type Account } from "../api/dashboard.ts";
import { isRevisionConflict } from "../api/dashboard-v3.ts";
import {
  platformAccountsApi,
  platformGroupWrite,
  type PlatformAccount,
  type PlatformAccountsView,
  type PlatformKind,
  type PlatformLink,
  type PlatformQuota,
  type PlatformQuotaKind,
  type PlatformSnapshot,
} from "../api/platform-accounts.ts";
import {
  PLATFORM_KIND_LABELS,
  PLATFORM_QUOTA_KIND_KEYS,
  formatPlatformTime,
  formatQuotaAmount,
  importCandidateCapabilities,
  linkedAccountIdSet,
  platformGroupLabel,
  platformModelCandidates,
} from "../domain/platform-accounts.ts";
import { isCustomApiAccount } from "../domain/custom-account.ts";
import { locale, t, type MessageKey } from "../i18n/index.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";
import PlatformAccountFormModal, {
  type PlatformAccountFormPayload,
} from "./PlatformAccountFormModal.vue";
import PlatformLinkModal from "./PlatformLinkModal.vue";
import PlatformModelImportModal from "./PlatformModelImportModal.vue";
import PlatformPriceTable from "./PlatformPriceTable.vue";

const props = defineProps<{
  accounts: Account[];
}>();

const emit = defineEmits<{
  /** Server mutated account state (link/unlink rewrote the endpoint); reload the ordered list. */
  changed: [];
  /** A capabilities import returned the updated account; replace it in place. */
  accountUpdated: [account: Account];
  /** Latest link set plus parent names, for the parent-owned-endpoint lock. */
  linksChange: [links: PlatformLink[], parents: { id: string; name: string }[]];
}>();

const PARENT_QUOTA_KINDS: readonly PlatformQuotaKind[] = ["wallet", "subscription"];
// Key-authenticated observations (source sub2api.v1.usage) stay on that Key:
// manual association cannot prove shared wallet ownership.
const CHILD_QUOTA_KINDS: readonly PlatformQuotaKind[] = ["wallet", "subscription", "key_limit"];

const dialog = useDialog();
const message = useMessage();

const view = ref<PlatformAccountsView | null>(null);
const loading = ref(true);
const loadError = ref("");
const expanded = ref<string[]>([]);
const mutating = ref(false);
const refreshing = ref<Record<string, boolean>>({});

const showForm = ref(false);
const editingPlatform = ref<PlatformAccount | null>(null);
const presetKind = ref<PlatformKind>("new_api");

const showLink = ref(false);
const linkParent = ref<PlatformAccount | null>(null);

const importTarget = ref<{ account: Account; link: PlatformLink } | null>(null);

const quotaKindKeys = PLATFORM_QUOTA_KIND_KEYS;

const addOptions = computed(() => (
  (Object.entries(PLATFORM_KIND_LABELS) as [PlatformKind, string][]).map(([kind, label]) => ({
    key: kind,
    label: `${t("添加平台账号")} · ${label}`,
  }))
));

const linkCandidates = computed(() => {
  const linked = linkedAccountIdSet(view.value?.links ?? []);
  return props.accounts.filter((account) => isCustomApiAccount(account) && !linked.has(account.id));
});

watch(() => view.value?.links, (links) => {
  emit(
    "linksChange",
    links ?? [],
    (view.value?.accounts ?? []).map((parent) => ({ id: parent.id, name: parent.name })),
  );
});

function kindLabel(kind: PlatformKind): string {
  return PLATFORM_KIND_LABELS[kind];
}

function groupLabel(group: PlatformLink["group"]): string {
  return platformGroupLabel(group);
}

function linksFor(parent: PlatformAccount): PlatformLink[] {
  return (view.value?.links ?? []).filter((link) => link.platformAccountId === parent.id);
}

function accountOf(accountId: string): Account | undefined {
  return props.accounts.find((account) => account.id === accountId);
}

function quotasOf(snapshot: PlatformSnapshot, kind: PlatformQuotaKind): PlatformQuota[] {
  return snapshot.quotas.filter((quota) => quota.kind === kind);
}

function quotaAmount(value: number | null, unit: string): string {
  return value === null ? t("未知") : formatQuotaAmount(value, unit, locale.value);
}

function timeText(epochSeconds: number): string {
  return formatPlatformTime(epochSeconds, locale.value) || t("未知");
}

function observedText(snapshot: PlatformSnapshot): string {
  return timeText(snapshot.observedAt);
}

async function load(): Promise<void> {
  loading.value = true;
  loadError.value = "";
  try {
    view.value = await platformAccountsApi.list();
  } catch (error) {
    loadError.value = dashboardErrorDetail(error);
    message.error(t("加载平台账号失败: {error}", { error: loadError.value }));
  } finally {
    loading.value = false;
  }
}

/** Revision-conflict recovery: tokens already refreshed by the CAS layer; reload and ask to retry. */
async function recoverConflict(): Promise<void> {
  try {
    view.value = await platformAccountsApi.list();
  } catch {
    // The next explicit action retries; keep the conflict warning meaningful.
  }
  message.warning(t("账号设置已被其他操作修改，已重新加载最新状态，请重试"));
  emit("changed");
}

function mutationError(error: unknown, fallbackKey: MessageKey): void {
  message.error(t(fallbackKey, { error: dashboardErrorDetail(error) }));
}

function openCreate(kind: PlatformKind): void {
  editingPlatform.value = null;
  presetKind.value = kind;
  showForm.value = true;
}

function openEdit(parent: PlatformAccount): void {
  editingPlatform.value = parent;
  showForm.value = true;
}

async function onFormSave(payload: PlatformAccountFormPayload): Promise<void> {
  if (mutating.value) return;
  mutating.value = true;
  try {
    const editing = editingPlatform.value;
    view.value = editing
      ? await platformAccountsApi.update(editing.id, {
        name: payload.name,
        ...(payload.userCredential !== undefined ? { userCredential: payload.userCredential } : {}),
      })
      : await platformAccountsApi.create({
        kind: payload.kind,
        name: payload.name,
        baseUrl: payload.baseUrl,
        ...(payload.userCredential !== undefined ? { userCredential: payload.userCredential } : {}),
      });
    showForm.value = false;
    message.success(editing ? t("平台账号已更新") : t("平台账号已创建"));
  } catch (error) {
    if (isRevisionConflict(error)) {
      showForm.value = false;
      await recoverConflict();
    } else {
      mutationError(error, "保存失败: {error}");
    }
  } finally {
    mutating.value = false;
  }
}

function confirmDelete(parent: PlatformAccount): void {
  dialog.warning({
    title: t("删除平台账号"),
    content: t("确定删除平台账号 {name} 吗？其快照数据会一并删除，已保存的凭证不可恢复。", { name: parent.name }),
    positiveText: t("删除"),
    negativeText: t("取消"),
    onPositiveClick: () => deletePlatform(parent),
  });
}

async function deletePlatform(parent: PlatformAccount): Promise<void> {
  if (mutating.value) return;
  mutating.value = true;
  try {
    await platformAccountsApi.remove(parent.id);
    await load();
    message.success(t("平台账号已删除"));
  } catch (error) {
    if (isRevisionConflict(error)) await recoverConflict();
    else mutationError(error, "删除失败: {error}");
  } finally {
    mutating.value = false;
  }
}

async function refreshParent(parent: PlatformAccount): Promise<void> {
  if (refreshing.value[parent.id]) return;
  refreshing.value[parent.id] = true;
  try {
    view.value = await platformAccountsApi.refresh(parent.id);
    message.success(t("已刷新"));
  } catch (error) {
    if (isRevisionConflict(error)) await recoverConflict();
    else mutationError(error, "刷新失败: {error}");
  } finally {
    refreshing.value[parent.id] = false;
  }
}

async function refreshChild(parent: PlatformAccount, link: PlatformLink): Promise<void> {
  const key = `${parent.id}:${link.accountId}`;
  if (refreshing.value[key]) return;
  refreshing.value[key] = true;
  try {
    view.value = await platformAccountsApi.refresh(parent.id, link.accountId);
    message.success(t("已刷新"));
  } catch (error) {
    if (isRevisionConflict(error)) await recoverConflict();
    else mutationError(error, "刷新失败: {error}");
  } finally {
    refreshing.value[key] = false;
  }
}

function openLink(parent: PlatformAccount): void {
  linkParent.value = parent;
  showLink.value = true;
}

async function onLinkSubmit(
  selection: { accountId: string; group: { id: string | null; platform: string | null } },
): Promise<void> {
  const parent = linkParent.value;
  if (!parent || mutating.value) return;
  mutating.value = true;
  try {
    view.value = await platformAccountsApi.link(
      selection.accountId,
      parent.id,
      platformGroupWrite(selection.group),
    );
    showLink.value = false;
    message.success(t("已关联"));
    // Linking rewrites the Key's endpoint to the parent-owned inference URL.
    emit("changed");
  } catch (error) {
    if (isRevisionConflict(error)) {
      showLink.value = false;
      await recoverConflict();
    } else {
      mutationError(error, "操作失败: {error}");
    }
  } finally {
    mutating.value = false;
  }
}

function confirmUnlink(account: Account, link: PlatformLink): void {
  dialog.warning({
    title: t("取消关联"),
    content: t("确定取消 Key {name} 与该平台账号的关联吗？关联期间写入的平台 Endpoint 会保留为普通 Custom Endpoint。", { name: account.name }),
    positiveText: t("取消关联"),
    negativeText: t("取消"),
    onPositiveClick: () => unlink(link.accountId),
  });
}

async function unlink(accountId: string): Promise<void> {
  if (mutating.value) return;
  mutating.value = true;
  try {
    view.value = await platformAccountsApi.unlink(accountId);
    message.success(t("已取消关联"));
    emit("changed");
  } catch (error) {
    if (isRevisionConflict(error)) await recoverConflict();
    else mutationError(error, "操作失败: {error}");
  } finally {
    mutating.value = false;
  }
}

function openImport(account: Account, link: PlatformLink): void {
  importTarget.value = { account, link };
}

function setImportVisible(show: boolean): void {
  if (!show) importTarget.value = null;
}

async function onImportSubmit(modelIds: string[]): Promise<void> {
  const target = importTarget.value;
  const protocol = target?.account.custom_config?.upstream_protocol;
  if (!target || !protocol || mutating.value) return;
  const selected = platformModelCandidates(target.link.snapshot, target.account.model_capabilities)
    .filter((candidate) => modelIds.includes(candidate.id) && !candidate.alreadyMapped);
  if (selected.length === 0) return;
  mutating.value = true;
  try {
    // Explicit confirmation writes through the existing account-capabilities
    // HTTP route only; refreshing prices never adds models on its own.
    const updated = await dashboardApi.updateAccountModelCapabilities(target.account.id, [
      ...target.account.model_capabilities.map((capability) => ({
        public_model: capability.public_model,
        upstream_model: capability.upstream_model,
        protocol: capability.protocol,
        source: capability.source,
      })),
      ...importCandidateCapabilities(selected, protocol),
    ]);
    emit("accountUpdated", updated);
    importTarget.value = null;
    message.success(t("导入完成：新增 {count} 个模型映射", { count: selected.length }));
  } catch (error) {
    if (isRevisionConflict(error)) {
      importTarget.value = null;
      await recoverConflict();
    } else {
      mutationError(error, "操作失败: {error}");
    }
  } finally {
    mutating.value = false;
  }
}

onMounted(load);

defineExpose({ reload: load });
</script>

<style scoped>
.platform-section-title {
  font-size: var(--ocg-font-lg);
  font-weight: 600;
}

.platform-count-tag {
  margin-left: 8px;
}

.platform-state {
  min-height: 80px;
  display: grid;
  place-items: center;
}

.platform-parent-title {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.platform-parent-name {
  font-weight: 600;
}

.platform-parent-url {
  font-size: var(--ocg-font-xs);
  color: var(--ocg-subtle);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 40ch;
}

.platform-parent-body {
  display: grid;
  gap: 12px;
}

.platform-block {
  display: grid;
  gap: 6px;
}

.platform-block-title {
  font-size: var(--ocg-font-xs);
  font-weight: 600;
  color: var(--ocg-muted);
}

.platform-observed,
.platform-hint {
  font-size: var(--ocg-font-xs);
  color: var(--ocg-subtle);
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

.platform-unknown {
  font-size: var(--ocg-font-sm);
  color: var(--ocg-subtle);
}

.quota-row {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 4px 12px;
  font-size: var(--ocg-font-sm);
}

.quota-meta {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 4px 10px;
  font-size: var(--ocg-font-xs);
  color: var(--ocg-subtle);
}

.platform-children-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.platform-child {
  display: grid;
  gap: 6px;
  padding: 8px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
}

.platform-child-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.platform-child-name {
  font-weight: 600;
}

.platform-child-actions {
  margin-left: auto;
}
</style>
