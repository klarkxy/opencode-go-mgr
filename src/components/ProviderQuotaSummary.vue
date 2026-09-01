<template>
  <div class="provider-quota-summary">
    <div
      v-if="displayedWindows.length === 0"
      class="provider-quota-row provider-quota-row--empty"
      role="status"
    >
      <div class="provider-quota-row__heading">
        <span>{{ t("尚未刷新") }}</span>
        <strong>—</strong>
      </div>
      <n-progress
        type="line"
        :percentage="0"
        status="default"
        :show-indicator="false"
        :height="8"
        :border-radius="4"
      />
    </div>
    <template v-else>
      <div v-for="window in displayedWindows" :key="window.window_kind" class="provider-quota-row">
        <div class="provider-quota-row__heading">
          <span>{{ windowLabel(window) }}</span>
          <strong>{{ usedLabel(window) }}</strong>
        </div>
        <n-progress
          type="line"
          :percentage="usedPercent(window)"
          :status="usedPercent(window) >= 100 ? 'error' : 'default'"
          :show-indicator="false"
          :height="8"
          :border-radius="4"
        />
        <time v-if="window.resets_at" class="provider-quota-row__reset">
          {{ t("{time}后重置", { time: formatCooldownRemainingUntil(window.resets_at, now) }) }}
        </time>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { NProgress } from "naive-ui";
import { computed } from "vue";
import type { ProviderQuotaWindow, ProviderUsageResponse } from "../api/providers.ts";
import { formatCooldownRemainingUntil } from "../domain/account-display.ts";
import { t } from "../i18n/index.ts";

const props = defineProps<{ usage: ProviderUsageResponse | null; now: number }>();

const displayedWindows = computed(() => (
  props.usage?.quota_windows.filter((window) => {
    const kind = window.window_kind.toLowerCase();
    return !(kind.startsWith("minimax_") && kind.endsWith(":video"));
  }) ?? []
));

function scopeLabel(value: string): string {
  const normalized = value.replaceAll("_", " ").trim();
  return normalized ? normalized[0].toUpperCase() + normalized.slice(1) : normalized;
}

function windowLabel(window: ProviderQuotaWindow): string {
  const kind = window.window_kind;
  if (kind.startsWith("minimax_current:")) {
    const started = window.started_at ? Date.parse(window.started_at) : Number.NaN;
    const ended = window.resets_at ? Date.parse(window.resets_at) : Number.NaN;
    const hours = Math.round((ended - started) / 3_600_000);
    const period = Number.isFinite(hours) && hours > 0 ? `${hours}${t("小时")}` : "";
    const scope = scopeLabel(kind.slice(16));
    return period ? `${period} · ${scope}` : scope;
  }
  if (kind.startsWith("minimax_weekly:")) return `${t("本周")} · ${scopeLabel(kind.slice(15))}`;
  if (kind === "kimi_usage") return t("本周");
  if (kind === "kimi_5h") return t("5小时");
  return scopeLabel(kind);
}

function usedPercent(window: ProviderQuotaWindow): number {
  if (window.limit_value === null || window.limit_value <= 0) return 0;
  return Math.max(0, Math.min(100, (window.used / window.limit_value) * 100));
}

function usedLabel(window: ProviderQuotaWindow): string {
  if (window.limit_value === null) return "∞";
  if (window.unit === "percent" || window.window_kind.startsWith("kimi_")) {
    return `${usedPercent(window).toLocaleString(undefined, { maximumFractionDigits: 1 })}%`;
  }
  return `${window.used.toLocaleString()} / ${window.limit_value.toLocaleString()}`;
}
</script>

<style scoped>
.provider-quota-summary {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 12px;
}

.provider-quota-row {
  display: grid;
  gap: 6px;
  min-width: 0;
}

.provider-quota-row__heading {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

.provider-quota-row__heading strong {
  color: var(--ocg-ink);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: var(--ocg-font-md);
  font-variant-numeric: tabular-nums;
  font-weight: 600;
}

.provider-quota-row__reset {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
  font-variant-numeric: tabular-nums;
}

@media (max-width: 640px) {
  .provider-quota-summary {
    grid-template-columns: 1fr;
  }
}
</style>
