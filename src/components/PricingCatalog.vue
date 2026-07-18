<template>
  <section class="pricing-card" aria-labelledby="pricing-title">
    <div class="pricing-head">
      <div>
        <h2 id="pricing-title">$ {{ t("OpenCode Go 额度价格表") }}</h2>
        <p>{{ t("只在你主动刷新时访问官方文档；刷新失败会继续使用当前快照。") }}</p>
      </div>
      <div class="pricing-actions">
        <n-button
          v-if="snapshot"
          tag="a"
          text
          :href="snapshot.source_url"
          target="_blank"
          rel="noopener noreferrer"
        >{{ t("官方来源") }}</n-button>
        <n-button
          type="primary"
          :loading="refreshing"
          :disabled="loading || refreshing || savingModelId !== null || confirmationOpen"
          @click="requestPricingRefresh"
        >{{ refreshing ? t("正在刷新…") : t("刷新价格表") }}</n-button>
      </div>
    </div>

    <n-alert v-if="loadError && !snapshot" type="error" :title="t('加载额度价格表失败: {error}', { error: loadError })">
      <n-button size="small" secondary @click="loadPricing">{{ t("重试") }}</n-button>
    </n-alert>
    <n-alert v-else-if="refreshError" type="warning" :title="t('刷新额度价格表失败: {error}', { error: refreshError })" />

    <n-spin :show="loading">
      <template v-if="snapshot">
        <dl class="pricing-ledger">
          <div class="pricing-ledger__revision">
            <dt>{{ t("修订版本") }}</dt>
            <dd><code>{{ snapshot.revision }}</code></dd>
          </div>
          <div>
            <dt>{{ t("启用时间") }}</dt>
            <dd>{{ formatTimestamp(snapshot.activated_at) }}</dd>
          </div>
          <div>
            <dt>{{ t("文档更新时间") }}</dt>
            <dd>{{ snapshot.document_updated_at ? formatTimestamp(snapshot.document_updated_at) : "—" }}</dd>
          </div>
          <div>
            <dt>{{ t("5 小时额度") }}</dt>
            <dd>{{ formatRate(snapshot.limits.window_5h).label }}</dd>
          </div>
          <div>
            <dt>{{ t("周额度") }}</dt>
            <dd>{{ formatRate(snapshot.limits.window_week).label }}</dd>
          </div>
          <div>
            <dt>{{ t("月额度") }}</dt>
            <dd>{{ formatRate(snapshot.limits.window_month).label }}</dd>
          </div>
        </dl>

        <p class="pricing-note">
          {{ t("模型价格为 OpenCode Go 表中的美元/百万 tokens；官方倍率用于换算额度消耗，可按活动手动调整。") }}
        </p>
        <n-data-table
          :columns="columns"
          :data="tableRows"
          :pagination="false"
          :row-key="rowKey"
          :expanded-row-keys="expandedRowKeys"
          :scroll-x="1310"
          size="small"
          @update:expanded-row-keys="updateExpandedRowKeys"
        />
      </template>
    </n-spin>
  </section>
</template>

<script setup lang="ts">
import { computed, h, onMounted, ref } from "vue";
import {
  NAlert,
  NButton,
  NDataTable,
  NIcon,
  NInputNumber,
  NSpin,
  NTooltip,
  useDialog,
  useMessage,
} from "naive-ui";
import type { DataTableColumns, DataTableRowKey } from "naive-ui";
import { CheckOutlined, CloseOutlined, RightOutlined } from "@vicons/antd";
import { DashboardRequestError, tauriApi } from "../api/tauri";
import type {
  PricingMultiplierChange,
  PricingSnapshot,
} from "../api/tauri";
import { locale, t } from "../i18n/index.ts";
import {
  buildPricingTableRows,
  effectivePricingRate,
  formatPricingMultiplier,
  formatPricingRate,
} from "../views/pricing-view";
import type { PricingTableRow } from "../views/pricing-view";

const message = useMessage();
const dialog = useDialog();
const snapshot = ref<PricingSnapshot | null>(null);
const loading = ref(false);
const refreshing = ref(false);
const confirmationOpen = ref(false);
const savingModelId = ref<string | null>(null);
const multiplierDrafts = ref<Partial<Record<string, number | null>>>({});
const expandedRowKeys = ref<DataTableRowKey[]>([]);
const loadError = ref("");
const refreshError = ref("");

const tableRows = computed(() => buildPricingTableRows(snapshot.value?.models ?? [], {
  highspeed: t("高速别名"),
  minimaxM3Upper: t("> 512K 输入"),
  priorityService: t("优先服务"),
  minimaxM3UpperPriority: t("> 512K 输入 + 优先服务"),
}));

function formatRate(value: number | null) {
  return formatPricingRate(value, locale.value);
}

function renderRate(value: number | null | undefined) {
  if (value === undefined) return "";
  const formatted = formatRate(value);
  if (!formatted.exact) return formatted.label;
  const exactLabel = t("精确值：{value} / 百万 tokens", { value: formatted.exact });
  return h(NTooltip, { trigger: "focus" }, {
    trigger: () => h("span", {
      class: "tiny-rate",
      tabindex: 0,
      title: exactLabel,
      "aria-label": `${formatted.label}, ${exactLabel}`,
    }, formatted.label),
    default: () => exactLabel,
  });
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale.value, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function rowKey(row: PricingTableRow): string {
  return row.row_key;
}

function updateExpandedRowKeys(keys: DataTableRowKey[]) {
  expandedRowKeys.value = keys;
}

function rowExpanded(row: PricingTableRow): boolean {
  return expandedRowKeys.value.includes(row.row_key);
}

function toggleRow(row: PricingTableRow) {
  expandedRowKeys.value = rowExpanded(row)
    ? expandedRowKeys.value.filter((key) => key !== row.row_key)
    : [...expandedRowKeys.value, row.row_key];
}

function renderModel(row: PricingTableRow) {
  if (row.kind !== "group") {
    return h("span", {
      "aria-label": row.kind === "variant" ? `${row.model_id} ${row.display_name}` : undefined,
    }, row.display_name || row.model_id);
  }
  const count = (row.children?.length ?? 0) + 1;
  const tierCount = count === 1 ? t("1 个价格档位") : t("{count} 个价格档位", { count });
  return h(NButton, {
    text: true,
    class: "pricing-tree-toggle",
    "aria-expanded": rowExpanded(row),
    "aria-label": `${row.display_name}, ${tierCount}`,
    onClick: () => toggleRow(row),
  }, {
    icon: () => h(NIcon, {
      component: RightOutlined,
      class: ["pricing-tree-chevron", rowExpanded(row) && "pricing-tree-chevron--expanded"],
    }),
    default: () => row.display_name || row.model_id,
  });
}

function snapshotMultiplier(modelId: string): number {
  return snapshot.value?.models.find(({ model_id }) => model_id === modelId)?.quota_multiplier ?? 1;
}

function hasMultiplierDraft(modelId: string): boolean {
  return multiplierDrafts.value[modelId] !== undefined;
}

function multiplierValue(modelId: string): number | null {
  return hasMultiplierDraft(modelId)
    ? multiplierDrafts.value[modelId] ?? null
    : snapshotMultiplier(modelId);
}

function validMultiplier(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function previewMultiplier(modelId: string): number {
  const value = multiplierValue(modelId);
  return validMultiplier(value) ? value : snapshotMultiplier(modelId);
}

function updateMultiplierDraft(modelId: string, value: number | null) {
  const current = snapshotMultiplier(modelId);
  if (validMultiplier(value) && Math.abs(value - current) < Number.EPSILON) {
    delete multiplierDrafts.value[modelId];
    return;
  }
  multiplierDrafts.value[modelId] = value;
}

function discardMultiplierDraft(modelId: string) {
  delete multiplierDrafts.value[modelId];
}

async function reloadPricingAfterRevisionChange(): Promise<string | null> {
  try {
    snapshot.value = await tauriApi.getPricing();
    message.warning(t("价格表已在其他位置更新，已重新加载"));
    return null;
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}

async function saveMultiplier(modelId: string) {
  const active = snapshot.value;
  const multiplier = multiplierValue(modelId);
  if (!active || !hasMultiplierDraft(modelId) || !validMultiplier(multiplier) || savingModelId.value) return;
  savingModelId.value = modelId;
  try {
    snapshot.value = await tauriApi.updatePricingMultipliers(active.revision, [{ model_id: modelId, multiplier }]);
    discardMultiplierDraft(modelId);
    message.success(t("官方倍率已保存"));
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    if (
      error instanceof DashboardRequestError
      && error.status === 409
      && detail.includes("pricing revision changed")
    ) {
      const reloadError = await reloadPricingAfterRevisionChange();
      if (reloadError) {
        message.error(t("保存官方倍率失败: {error}", { error: reloadError }));
      }
    } else {
      message.error(t("保存官方倍率失败: {error}", { error: detail }));
    }
  } finally {
    savingModelId.value = null;
  }
}

function renderMultiplierAction(
  row: PricingTableRow,
  label: string,
  icon: typeof CheckOutlined,
  action: () => void,
  primary = false,
  disabled = false,
) {
  return h(NTooltip, { trigger: "hover" }, {
    trigger: () => h(NButton, {
      circle: true,
      quaternary: true,
      size: "tiny",
      type: primary ? "primary" : "default",
      loading: primary && savingModelId.value === row.model_id,
      disabled: disabled || savingModelId.value !== null || refreshing.value,
      "aria-label": `${label}: ${row.display_name}`,
      onClick: action,
    }, { icon: () => h(NIcon, { component: icon }) }),
    default: () => label,
  });
}

function renderMultiplierEditor(row: PricingTableRow) {
  if (!row.editable_multiplier) return "";
  const value = multiplierValue(row.model_id);
  const dirty = hasMultiplierDraft(row.model_id);
  const valid = validMultiplier(value);
  return h("div", { class: ["multiplier-editor", dirty && "multiplier-editor--dirty"] }, [
    h(NInputNumber, {
      value,
      min: 0.0001,
      max: 1000,
      step: 0.1,
      showButton: false,
      updateValueOnInput: true,
      size: "small",
      status: dirty && !valid ? "error" : undefined,
      disabled: savingModelId.value !== null || refreshing.value,
      inputProps: { "aria-label": `${row.display_name} ${t("官方倍率")}` },
      onUpdateValue: (nextValue: number | null) => updateMultiplierDraft(row.model_id, nextValue),
      onKeydown: (event: KeyboardEvent) => {
        if (event.key === "Enter" && dirty && valid) {
          event.preventDefault();
          void saveMultiplier(row.model_id);
        } else if (event.key === "Escape" && dirty) {
          event.preventDefault();
          discardMultiplierDraft(row.model_id);
        }
      },
    }, { prefix: () => "×" }),
    dirty
      ? h("div", { class: "multiplier-editor__actions" }, [
        renderMultiplierAction(
          row,
          t("保存倍率"),
          CheckOutlined,
          () => void saveMultiplier(row.model_id),
          true,
          !valid,
        ),
        renderMultiplierAction(row, t("放弃修改"), CloseOutlined, () => discardMultiplierDraft(row.model_id)),
      ])
      : null,
  ]);
}

function renderEffectiveRates(row: PricingTableRow) {
  const rates = [
    ["I", row.input],
    ["O", row.output],
    ["CR", row.cache_read],
    ["CW", row.cache_write],
  ] as const;
  return h("div", { class: "effective-rates" }, rates.map(([label, rate]) => (
    h("span", { key: label }, [
      h("b", label),
      " ",
      renderRate(effectivePricingRate(rate ?? null, previewMultiplier(row.model_id))),
    ])
  )));
}

const columns = computed<DataTableColumns<PricingTableRow>>(() => [
  {
    title: t("模型"),
    key: "model_id",
    width: 190,
    fixed: "left",
    ellipsis: { tooltip: true },
    render: renderModel,
  },
  { title: t("输入"), key: "input", width: 112, align: "right", render: (row) => renderRate(row.input) },
  { title: t("输出"), key: "output", width: 112, align: "right", render: (row) => renderRate(row.output) },
  { title: t("缓存读"), key: "cache_read", width: 112, align: "right", render: (row) => renderRate(row.cache_read) },
  { title: t("缓存写"), key: "cache_write", width: 112, align: "right", render: (row) => renderRate(row.cache_write) },
  { title: "Usage", key: "usage", width: 100, align: "right", render: (row) => renderRate(row.usage) },
  {
    title: t("官方倍率"),
    key: "quota_multiplier",
    width: 238,
    render: renderMultiplierEditor,
  },
  { title: t("额度有效价格"), key: "effective", width: 330, render: renderEffectiveRates },
]);

async function loadPricing() {
  if (loading.value) return;
  loading.value = true;
  loadError.value = "";
  try {
    snapshot.value = await tauriApi.getPricing();
  } catch (error) {
    loadError.value = error instanceof Error ? error.message : String(error);
  } finally {
    loading.value = false;
  }
}

type MultiplierPolicy = "keep_current" | "use_official";

function multiplierDisplayName(modelId: string): string {
  return snapshot.value?.models.find(({ model_id }) => model_id === modelId)?.display_name || modelId;
}

function renderMultiplierChanges(changes: readonly PricingMultiplierChange[]) {
  return h("div", { class: "pricing-refresh-comparison" }, [
    h("p", t("刷新到的官方倍率与当前设置不同。选择是否覆盖当前倍率后，才会启用新的价格与模型列表。")),
    h("div", { class: "pricing-refresh-comparison__scroll" }, [
      h("table", [
        h("thead", [h("tr", [
          h("th", { scope: "col" }, t("模型")),
          h("th", { scope: "col" }, t("当前倍率")),
          h("th", { scope: "col" }, t("最新官方倍率")),
        ])]),
        h("tbody", changes.map((change) => h("tr", { key: change.model_id }, [
          h("th", { scope: "row" }, multiplierDisplayName(change.model_id)),
          h("td", formatPricingMultiplier(change.current_multiplier)),
          h("td", formatPricingMultiplier(change.official_multiplier)),
        ]))),
      ]),
    ]),
  ]);
}

function showRefreshConfirmation(
  changes: PricingMultiplierChange[],
  expectedRevision: string | undefined,
  officialContentHash: string,
) {
  confirmationOpen.value = true;
  let instance: { destroy: () => void } | null = null;
  const close = () => {
    instance?.destroy();
    confirmationOpen.value = false;
  };
  const apply = (policy: MultiplierPolicy) => {
    close();
    void performPricingRefresh(policy, expectedRevision, officialContentHash);
  };
  instance = dialog.warning({
    title: t("价格表与当前倍率不同"),
    content: () => renderMultiplierChanges(changes),
    closable: false,
    closeOnEsc: false,
    maskClosable: false,
    action: () => h("div", { class: "pricing-refresh-actions" }, [
      h(NButton, { onClick: close }, { default: () => t("取消刷新") }),
      h(NButton, { secondary: true, onClick: () => apply("keep_current") }, {
        default: () => t("保留当前倍率"),
      }),
      h(NButton, { type: "primary", onClick: () => apply("use_official") }, {
        default: () => t("使用最新官方倍率"),
      }),
    ]),
  });
}

function requestPricingRefresh() {
  if (Object.keys(multiplierDrafts.value).length) {
    message.warning(t("请先保存或放弃倍率修改"));
    return;
  }
  void performPricingRefresh();
}

async function performPricingRefresh(
  policy?: MultiplierPolicy,
  expectedRevision = snapshot.value?.revision,
  expectedOfficialContentHash?: string,
) {
  if (refreshing.value) return;
  refreshing.value = true;
  refreshError.value = "";
  try {
    const result = await tauriApi.refreshPricing({
      policy,
      expected_revision: expectedRevision,
      expected_official_content_hash: expectedOfficialContentHash,
    });
    if (result.refresh_status === "needs_confirmation") {
      if (!result.official_content_hash) {
        refreshError.value = t("刷新确认缺少官方内容哈希");
        return;
      }
      showRefreshConfirmation(
        result.multiplier_changes ?? [],
        expectedRevision,
        result.official_content_hash,
      );
    } else if (result.refresh_status === "success") {
      snapshot.value = result;
      message.success(policy === "keep_current"
        ? t("价格表已更新，已保留当前倍率")
        : policy === "use_official"
          ? t("价格表已更新，已采用最新官方倍率")
          : t("价格表已更新"));
    } else if (result.refresh_status === "unchanged") {
      snapshot.value = result;
      message.info(t("价格表没有变化"));
    } else {
      refreshError.value = result.error || t("价格表刷新失败，详见页面提示");
      message.warning(t("价格表刷新失败，详见页面提示"));
    }
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    if (
      error instanceof DashboardRequestError
      && error.status === 409
      && detail.includes("pricing revision changed")
    ) {
      refreshError.value = await reloadPricingAfterRevisionChange() ?? "";
    } else if (!policy && error instanceof DashboardRequestError && error.status === 409) {
      message.warning(t("已有价格表刷新正在进行"));
    } else {
      refreshError.value = detail;
    }
  } finally {
    refreshing.value = false;
  }
}

onMounted(() => void loadPricing());
</script>

<style scoped>
.pricing-card {
  grid-column: 1 / -1;
  padding: 22px;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.pricing-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 18px;
}
.pricing-head h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 18px/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.pricing-head p,
.pricing-note {
  margin: 4px 0 0;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-size);
}
.pricing-actions {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 12px;
}
.pricing-ledger {
  display: grid;
  grid-template-columns: minmax(180px, 1.4fr) repeat(5, minmax(112px, 1fr));
  gap: 1px;
  margin: 0 0 14px;
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-border);
}
.pricing-ledger > div {
  min-width: 0;
  padding: 10px 12px;
  background: var(--ocg-canvas);
}
.pricing-ledger dt {
  margin-bottom: 4px;
  color: var(--ocg-subtle);
  font-size: 12px;
}
.pricing-ledger dd {
  overflow: hidden;
  margin: 0;
  color: var(--ocg-ink);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pricing-ledger code {
  font-family: "Cascadia Mono", Consolas, monospace;
}
.pricing-note {
  margin-bottom: 10px;
}
:deep(.n-data-table-expand-trigger) {
  display: none;
}
:deep(.pricing-tree-toggle) {
  min-width: 0;
}
:deep(.pricing-tree-toggle .n-button__content) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
:deep(.pricing-tree-chevron) {
  transition: transform 160ms ease;
}
:deep(.pricing-tree-chevron--expanded) {
  transform: rotate(90deg);
}
:deep(.multiplier-editor) {
  display: flex;
  align-items: center;
  gap: 6px;
}
:deep(.multiplier-editor .n-input-number) {
  width: 112px;
}
:deep(.multiplier-editor--dirty .n-input-number) {
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--ocg-primary) 62%, transparent);
}
:deep(.multiplier-editor__actions) {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
:deep(.tiny-rate) {
  border-bottom: 1px dotted currentColor;
  cursor: help;
}
:deep(.effective-rates) {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 2px 12px;
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: 12px;
}
:deep(.effective-rates b) {
  color: var(--ocg-subtle);
}
:global(.pricing-refresh-comparison) {
  min-width: min(560px, 76vw);
}
:global(.pricing-refresh-comparison > p) {
  margin: 0 0 12px;
  color: var(--ocg-subtle);
}
:global(.pricing-refresh-comparison__scroll) {
  max-height: 280px;
  overflow: auto;
  border: 1px solid var(--ocg-border);
  border-radius: 8px;
}
:global(.pricing-refresh-comparison table) {
  width: 100%;
  border-collapse: collapse;
}
:global(.pricing-refresh-comparison th),
:global(.pricing-refresh-comparison td) {
  padding: 8px 10px;
  border-bottom: 1px solid var(--ocg-border);
  text-align: left;
}
:global(.pricing-refresh-comparison thead th) {
  position: sticky;
  top: 0;
  background: var(--ocg-canvas);
  color: var(--ocg-subtle);
  font-size: 12px;
}
:global(.pricing-refresh-comparison td) {
  font-family: "Cascadia Mono", Consolas, monospace;
}
:global(.pricing-refresh-actions) {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
}
@media (prefers-reduced-motion: reduce) {
  :deep(.pricing-tree-chevron) {
    transition: none;
  }
}
@media (max-width: 900px) {
  .pricing-ledger {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}
@media (max-width: 640px) {
  .pricing-head {
    align-items: stretch;
    flex-direction: column;
  }
  .pricing-actions {
    justify-content: space-between;
  }
  .pricing-ledger {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  :global(.pricing-refresh-comparison) {
    min-width: 0;
  }
  :global(.pricing-refresh-actions) {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
