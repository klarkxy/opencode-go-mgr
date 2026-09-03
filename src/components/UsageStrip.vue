<template>
  <div class="usage-strip">
    <div class="usage-strip-body" role="group" :aria-label="t('用量')">
      <div v-for="limit in limits" :key="limit.key" class="usage-segment">
        <div class="usage-meta">
          <span>{{ limit.label }}</span>
          <strong>
            {{ formatCost(usage[limit.key]) }} / {{ formatCost(limit.limit) }}
            <template v-if="usage[limit.key] > limit.limit">
              · {{ t("超出 {amount}", { amount: formatCost(usage[limit.key] - limit.limit) }) }}
            </template>
          </strong>
        </div>
        <n-progress
          type="line"
          :height="8"
          :percentage="usageProgressPercentage(
            account,
            limit.key,
            usagePercentFromCost(usage[limit.key], limit.limit),
            now,
          )"
          :status="usageProgressStatus(
            account,
            limit.key,
            usagePercentFromCost(usage[limit.key], limit.limit),
            now,
          )"
          :processing="!editing"
          :show-indicator="false"
        />
        <span
          v-if="isUsageLimitReached(account, limit.key, now)"
          class="usage-reset-countdown"
        >
          {{ t("{time}后重置", { time: formatWindowRemaining(limit.key) }) }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
// The strip is the only part of the account card whose display changes every
// second (progress status flips and the reset countdown). Keeping the clock
// here means a tick re-renders this small subtree instead of the whole card
// list in Accounts.vue.
import { onActivated, onDeactivated, onMounted, onUnmounted, ref } from "vue";
import { NProgress } from "naive-ui";
import type { Account, UsageWindow } from "../api/dashboard";
import {
  isUsageLimitReached,
  resetTimeForWindow,
  usagePercentFromCost,
  usageProgressPercentage,
  usageProgressStatus,
} from "../domain/accounts-usage.ts";
import type { UsageKey } from "../domain/accounts-usage.ts";
import { t } from "../i18n/index.ts";
import { formatCost } from "../utils/format.ts";

const props = defineProps<{
  account: Account;
  usage: UsageWindow;
  limits: Array<{ key: UsageKey; label: string; limit: number }>;
  editing: boolean;
}>();

const now = ref(Date.now());
let clock: number | undefined;

function startClock() {
  if (clock === undefined) {
    clock = window.setInterval(() => {
      now.value = Date.now();
    }, 1000);
  }
}

function stopClock() {
  if (clock !== undefined) {
    window.clearInterval(clock);
    clock = undefined;
  }
}

onMounted(startClock);
onActivated(startClock);
onDeactivated(stopClock);
onUnmounted(stopClock);

function formatWindowRemaining(key: UsageKey): string {
  const until = resetTimeForWindow(props.account, key);
  if (!until) return "";
  const ms = Date.parse(until) - now.value;
  if (ms <= 0) return "";
  const seconds = Math.ceil(ms / 1000);
  if (seconds < 60) return t("{seconds}秒", { seconds });
  const minutes = Math.floor(ms / 60000);
  if (minutes < 60) return t("{minutes}分钟", { minutes });
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return t("{hours}小时{minutes}分钟", { hours, minutes: minutes % 60 });
  const days = Math.floor(hours / 24);
  return t("{days}天{hours}小时", { days, hours: hours % 24 });
}
</script>

<style scoped>
.usage-strip {
  min-width: 0;
}

.usage-strip-body {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 12px;
}

.usage-segment {
  display: grid;
  gap: 6px;
  min-width: 0;
}

.usage-meta {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: var(--ocg-font-sm);
  color: var(--ocg-muted);
}

.usage-meta strong {
  color: var(--ocg-ink);
  font-size: var(--ocg-font-md);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-weight: 600;
}

.usage-reset-countdown {
  color: var(--ocg-error);
  font-size: var(--ocg-font-xs);
  line-height: 1.4;
}

@media (max-width: 900px) {
  .usage-strip-body {
    grid-template-columns: 1fr;
  }
}
</style>
