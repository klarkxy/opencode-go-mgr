<template>
  <div class="ollama-pricing-reference">
    <dl v-if="snapshot" class="pricing-ledger">
      <div class="pricing-ledger__revision">
        <dt>{{ t("修订版本") }}</dt>
        <dd><code>{{ snapshot.revision }}</code></dd>
      </div>
      <div>
        <dt>{{ t("启用时间") }}</dt>
        <dd>{{ formatTimestamp(snapshot.activated_at) }}</dd>
      </div>
    </dl>
    <p class="pricing-note">
      USD / 1M tokens · {{ t("未知价格不会参与费用估算") }}
    </p>
    <n-data-table
      :columns="columns"
      :data="rows"
      :pagination="false"
      :row-key="(row: OllamaPricingRow) => row.modelId"
      :scroll-x="720"
      size="small"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, h } from "vue";
import { NDataTable, NTooltip } from "naive-ui";
import type { DataTableColumns } from "naive-ui";
import { locale, t } from "../i18n/index.ts";
import type { ProviderNeutralPricingSnapshot } from "../api/providers.ts";
import { formatPricingRate } from "../domain/pricing-view.ts";

interface OllamaPricingRow {
  modelId: string;
  model: string;
  input: number | null;
  cached: number | null;
  output: number | null;
}

const props = defineProps<{
  snapshot?: ProviderNeutralPricingSnapshot | null;
}>();

const rows = computed<OllamaPricingRow[]>(() => (
  props.snapshot?.values.map((row) => ({
    modelId: row.model_id,
    model: row.display_name,
    input: row.input_per_million,
    cached: row.cache_read_per_million,
    output: row.output_per_million,
  })) ?? []
));

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

function renderRate(value: number | null) {
  if (value === null) return "—";
  const formatted = formatPricingRate(value, locale.value);
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

const columns = computed<DataTableColumns<OllamaPricingRow>>(() => [
  { title: t("模型"), key: "model", ellipsis: { tooltip: true }, width: 220 },
  { title: t("输入"), key: "input", width: 140, render: (row) => renderRate(row.input) },
  { title: t("缓存输入"), key: "cached", width: 140, render: (row) => renderRate(row.cached) },
  { title: t("输出"), key: "output", width: 140, render: (row) => renderRate(row.output) },
]);
</script>

<style scoped>
.ollama-pricing-reference {
  display: grid;
  gap: 12px;
  min-width: 0;
}

.pricing-ledger {
  display: flex;
  flex-wrap: wrap;
  gap: 16px 24px;
  margin: 0;
}

.pricing-ledger div {
  display: grid;
  gap: 2px;
}

.pricing-ledger dt {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}

.pricing-ledger dd {
  margin: 0;
}

.pricing-note {
  margin: 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

.tiny-rate {
  font-variant-numeric: tabular-nums;
}
</style>
