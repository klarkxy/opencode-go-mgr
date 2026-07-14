import { ref } from "vue";
import { locale, t } from "../i18n/index.ts";

/**
 * Shared formatting helpers used across Dashboard, Accounts, Logs, and
 * StackedBarChart. Centralising them here avoids duplication and keeps
 * locale-aware formatting consistent.
 */

/** Format a number as USD currency with adaptive or caller-specified decimal places. */
export function formatCost(value: number, digits?: number): string {
  const fractionDigits = digits ?? (value !== 0 && value < 0.01 ? 4 : 2);
  return new Intl.NumberFormat(locale.value, {
    style: "currency",
    currency: "USD",
    currencyDisplay: "narrowSymbol",
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  }).format(value);
}

/** Format a number with locale-aware grouping. */
export function formatNumber(value: number): string {
  return new Intl.NumberFormat(locale.value).format(value);
}

/**
 * Composable-style clipboard helper with visual feedback state.
 *
 * Usage in `<script setup>`:
 * ```ts
 * const { copiedTarget, copy, cleanup } = useClipboard();
 * await copy('key', someValue, 'Key');
 * // …onUnmounted(() => cleanup());
 * ```
 */
export function useClipboard(timeout = 1500) {
  const copiedTarget = ref<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copy(target: string, value: string, label: string) {
    const writeText = navigator.clipboard?.writeText?.bind(navigator.clipboard);
    if (!value) throw new Error(t("没有可复制的内容"));
    if (!writeText) throw new Error(t("当前环境不支持剪贴板"));
    await writeText(value);
    copiedTarget.value = target;
    clearTimeout(timer);
    timer = setTimeout(() => { copiedTarget.value = null; }, timeout);
    return { target, label };
  }

  function cleanup() {
    clearTimeout(timer);
  }

  return { copiedTarget, copy, cleanup };
}
