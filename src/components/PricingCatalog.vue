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
          :disabled="loading || refreshing"
          @click="refreshPricing"
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
          {{ t("模型价格为 OpenCode Go 表中的美元/百万 tokens；“表价已含”表示官方单价已包含的额度倍率，“Go 倍率”表示结算时仍需额外乘的倍率，本地条件倍率最后叠加。") }}
        </p>
        <n-data-table
          :columns="columns"
          :data="snapshot.models"
          :pagination="false"
          :scroll-x="1615"
          size="small"
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
  NSpin,
  NTag,
  NTooltip,
  useMessage,
} from "naive-ui";
import type { DataTableColumns } from "naive-ui";
import { DashboardRequestError, tauriApi } from "../api/tauri";
import type { PricingModel, PricingSnapshot } from "../api/tauri";
import { locale, t } from "../i18n/index.ts";
import {
  effectivePricingRate,
  formatPricingMultiplier,
  formatPricingRate,
  officialPriceMultiplier,
} from "../views/pricing-view";

const message = useMessage();
const snapshot = ref<PricingSnapshot | null>(null);
const loading = ref(false);
const refreshing = ref(false);
const loadError = ref("");
const refreshError = ref("");

function formatRate(value: number | null) {
  return formatPricingRate(value, locale.value);
}

function renderRate(value: number | null) {
  const formatted = formatRate(value);
  if (!formatted.exact) return formatted.label;
  return h(NTooltip, { trigger: "hover" }, {
    trigger: () => h("span", { class: "tiny-rate", tabindex: 0 }, formatted.label),
    default: () => t("精确值：{value} / 百万 tokens", { value: formatted.exact ?? "" }),
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

function renderAdjustments(row: PricingModel) {
  if (!row.adjustments.length) return t("无");
  return h("div", { class: "adjustment-list" }, row.adjustments.map((adjustment) => (
    h(NTooltip, { key: `${adjustment.label}:${adjustment.applies_to}`, trigger: "hover" }, {
      trigger: () => h(NTag, { size: "small", bordered: false, type: "warning" }, {
        default: () => `${adjustment.label} ${formatPricingMultiplier(adjustment.multiplier)}`,
      }),
      default: () => t("适用于：{scope}", { scope: adjustment.applies_to }),
    })
  )));
}

function renderOfficialPriceMultiplier(row: PricingModel) {
  const multiplier = officialPriceMultiplier(row.official_price_multiplier);
  if (multiplier === 1) return formatPricingMultiplier(multiplier);
  return h(NTooltip, { trigger: "hover" }, {
    trigger: () => h(NTag, { size: "small", bordered: false, type: "info" }, {
      default: () => formatPricingMultiplier(multiplier),
    }),
    default: () => t("官方表格中的 token 单价相对供应商基准已包含此倍率；Go Usage 额度仍需单独换算。"),
  });
}

function renderEffectiveRates(row: PricingModel) {
  const rates = [
    ["I", row.input],
    ["O", row.output],
    ["CR", row.cache_read],
    ["CW", row.cache_write],
  ] as const;
  return h("div", { class: "effective-rates" }, rates.map(([label, rate]) => (
    h("span", { key: label }, [h("b", label), " ", renderRate(effectivePricingRate(rate, row.quota_multiplier))])
  )));
}

const columns = computed<DataTableColumns<PricingModel>>(() => [
  {
    title: t("模型"),
    key: "model_id",
    width: 190,
    fixed: "left",
    ellipsis: { tooltip: true },
    render: (row) => row.display_name || row.model_id,
  },
  { title: t("输入"), key: "input", width: 112, align: "right", render: (row) => renderRate(row.input) },
  { title: t("输出"), key: "output", width: 112, align: "right", render: (row) => renderRate(row.output) },
  { title: t("缓存读"), key: "cache_read", width: 112, align: "right", render: (row) => renderRate(row.cache_read) },
  { title: t("缓存写"), key: "cache_write", width: 112, align: "right", render: (row) => renderRate(row.cache_write) },
  { title: "Usage", key: "usage", width: 100, align: "right", render: (row) => renderRate(row.usage) },
  {
    title: t("表价已含"),
    key: "official_price_multiplier",
    width: 104,
    align: "right",
    render: renderOfficialPriceMultiplier,
  },
  {
    title: t("Go 倍率"),
    key: "quota_multiplier",
    width: 94,
    align: "right",
    render: (row) => formatPricingMultiplier(row.quota_multiplier),
  },
  { title: t("本地调整"), key: "adjustments", width: 250, render: renderAdjustments },
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

async function refreshPricing() {
  if (refreshing.value) return;
  refreshing.value = true;
  refreshError.value = "";
  try {
    const result = await tauriApi.refreshPricing();
    snapshot.value = result;
    if (result.refresh_status === "success") {
      message.success(t("价格表已更新"));
    } else {
      refreshError.value = result.error ?? "";
      message.warning(t("价格表未变化"));
    }
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    if (error instanceof DashboardRequestError && error.status === 409) {
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
:deep(.tiny-rate) {
  border-bottom: 1px dotted currentColor;
  cursor: help;
}
:deep(.adjustment-list) {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
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
}
</style>
