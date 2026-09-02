<template>
  <div class="ollama-usage" data-testid="ollama-usage">
    <template v-if="!usage || !usage.cookie_configured">
      <p class="ollama-usage__hint">{{ t("未配置网页会话 Cookie，暂无法查询用量") }}</p>
    </template>
    <template v-else>
      <p
        v-if="usage.status === 'unauthorized'"
        class="ollama-usage__hint ollama-usage__hint--warn"
        role="alert"
      >
        {{ t("网页会话已过期，请重新粘贴 Cookie") }}
      </p>
      <p
        v-else-if="usage.status === 'failed'"
        class="ollama-usage__hint ollama-usage__hint--warn"
        role="alert"
      >
        {{ usage.last_error || t("用量查询失败，稍后可重试") }}
      </p>
      <template v-if="usage.snapshot">
        <div class="ollama-usage__windows">
          <div
            v-for="window in usage.snapshot.windows"
            :key="window.window"
            class="ollama-usage__window"
          >
            <div class="ollama-usage__window-heading">
              <span>{{ windowLabel(window.window) }}</span>
              <strong>{{ usedLabel(window.used_percent) }}</strong>
            </div>
            <n-progress
              type="line"
              :percentage="window.used_percent ?? 0"
              :status="(window.used_percent ?? 0) >= 100 ? 'error' : 'default'"
              :show-indicator="false"
              :height="8"
              :border-radius="4"
            />
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
      <p
        v-else-if="usage.status !== 'unauthorized'"
        class="ollama-usage__hint"
      >{{ t("暂无用量快照，点击刷新查询") }}</p>
    </template>
  </div>
</template>

<script setup lang="ts">
import { NProgress } from "naive-ui";
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

<style scoped>
.ollama-usage__windows {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 12px;
}

.ollama-usage__window {
  display: grid;
  gap: 6px;
  min-width: 0;
}

.ollama-usage__window-heading {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

.ollama-usage__window-heading strong {
  color: var(--ocg-ink);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: var(--ocg-font-md);
  font-variant-numeric: tabular-nums;
  font-weight: 600;
}

.ollama-usage__window-reset {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
  font-variant-numeric: tabular-nums;
}

@media (max-width: 640px) {
  .ollama-usage__windows {
    grid-template-columns: 1fr;
  }
}
</style>
