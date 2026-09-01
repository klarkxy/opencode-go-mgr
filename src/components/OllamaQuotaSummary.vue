<template>
  <div class="ollama-usage" data-testid="ollama-usage">
    <template v-if="!usage || !usage.cookie_configured">
      <p class="ollama-usage__hint">{{ t("未配置网页会话 Cookie，暂无法查询用量") }}</p>
    </template>
    <template v-else-if="usage.status === 'unauthorized'">
      <p class="ollama-usage__hint ollama-usage__hint--warn" role="alert">
        {{ t("网页会话已过期，请重新粘贴 Cookie") }}
      </p>
    </template>
    <template v-else-if="usage.status === 'failed'">
      <p class="ollama-usage__hint ollama-usage__hint--warn" role="alert">
        {{ usage.last_error || t("用量查询失败，稍后可重试") }}
      </p>
    </template>
    <template v-else-if="usage.snapshot">
      <div class="ollama-usage__windows">
        <div
          v-for="window in usage.snapshot.windows"
          :key="window.window"
          class="ollama-usage__window"
        >
          <span class="ollama-usage__window-label">{{ windowLabel(window.window) }}</span>
          <span class="ollama-usage__window-value">{{ usedLabel(window.used_percent) }}</span>
          <span
            v-if="window.reset_at"
            class="ollama-usage__window-reset"
          >{{ t("{time}后重置", { time: formatCooldownRemainingUntil(window.reset_at, now) }) }}</span>
        </div>
      </div>
      <p v-if="planLabel" class="ollama-usage__meta">{{ planLabel }}</p>
      <p v-if="usage.snapshot.models.length" class="ollama-usage__meta">
        {{ modelRequestsLabel(usage.snapshot.models) }}
      </p>
    </template>
    <p v-else class="ollama-usage__hint">{{ t("暂无用量快照，点击刷新查询") }}</p>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { t } from "../i18n/index.ts";
import type { OllamaUsageResponse } from "../api/providers.ts";
import { formatCooldownRemainingUntil } from "../domain/account-display.ts";

const props = defineProps<{
  usage: OllamaUsageResponse | null;
  now: number;
}>();

const planLabel = computed(() => {
  const snapshot = props.usage?.snapshot;
  if (!snapshot) return "";
  const parts = [
    snapshot.plan ? t("套餐: {plan}", { plan: snapshot.plan }) : "",
    snapshot.balance ? t("余额: {balance}", { balance: snapshot.balance }) : "",
  ].filter(Boolean);
  return parts.join(" · ");
});

function windowLabel(window: string): string {
  if (window === "5h") return t("5 小时");
  if (window === "7d") return t("本周");
  return window;
}

function usedLabel(usedPercent: number | null): string {
  if (usedPercent === null || Number.isNaN(usedPercent)) return "—";
  return `${Math.round(usedPercent)}%`;
}

function modelRequestsLabel(
  models: { model: string; requests_5h: number | null; requests_7d: number | null }[],
): string {
  const parts = models.slice(0, 3).map((model) => {
    const total = model.requests_7d ?? model.requests_5h;
    return total === null || total === undefined
      ? model.model
      : `${model.model} ${total}`;
  });
  const suffix = models.length > 3 ? ` +${models.length - 3}` : "";
  return `${t("按模型请求")}: ${parts.join(" · ")}${suffix}`;
}
</script>
