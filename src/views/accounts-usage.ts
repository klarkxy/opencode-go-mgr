import type { Account, UsageWindow } from "../api/tauri";

export type UsageKey = "window_5h" | "window_week" | "window_month";

export type UsageEditState = {
  draft: number;
  saved: number;
  saving: boolean;
  error: string | null;
  /// 手动校准的"距上游重置还剩多少分钟"。仅 5h/周窗口使用；月窗口始终为 null。
  resets_in_minutes_draft: number | null;
  resets_in_minutes_saved: number | null;
};

/// 5h/周窗口的满窗分钟数。月窗口无法手动校准时间。
export const WINDOW_FULL_MINUTES: Record<UsageKey, number | null> = {
  window_5h: 5 * 60,
  window_week: 7 * 24 * 60,
  window_month: null,
};

/// 根据当前 `resets_in_*` 推断手动校准的默认剩余分钟数。
/// `resets_in_*` 为 null（窗口未开始）时返回满窗分钟数。
export function defaultResetsInMinutes(usage: Pick<UsageWindow, "resets_in_5h" | "resets_in_week" | "resets_in_month">, key: UsageKey, now = Date.now()): number | null {
  const full = WINDOW_FULL_MINUTES[key];
  if (full === null) return null;
  const until = windowResetsAt(usage, key);
  if (!until) return full;
  const remainingMs = Date.parse(until) - now;
  return Math.max(0, Math.ceil(remainingMs / 60000));
}

const cooldownFields: Record<UsageKey, keyof Pick<Account, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">> = {
  window_5h: "cooldown_5h_until",
  window_week: "cooldown_week_until",
  window_month: "cooldown_month_until",
};

const resetsFields: Record<UsageKey, keyof Pick<UsageWindow, "resets_in_5h" | "resets_in_week" | "resets_in_month">> = {
  window_5h: "resets_in_5h",
  window_week: "resets_in_week",
  window_month: "resets_in_month",
};

export function isWindowCooling(
  account: Pick<Account, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">,
  key: UsageKey,
  now = Date.now(),
): boolean {
  const until = account[cooldownFields[key]];
  return until !== null && Date.parse(until) > now;
}

/// 固定窗口的清零时刻（来自后端 `resets_in_*`）；`null` 表示窗口尚未开始（无成功请求）或月窗口无购买日期。
export function windowResetsAt(
  usage: Pick<UsageWindow, "resets_in_5h" | "resets_in_week" | "resets_in_month">,
  key: UsageKey,
): string | null {
  return usage[resetsFields[key]];
}

export function isCooling(
  account: Pick<Account, "cooldown_until" | "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">,
  now = Date.now(),
): boolean {
  return (
    (account.cooldown_until !== null && Date.parse(account.cooldown_until) > now) ||
    isWindowCooling(account, "window_5h", now) ||
    isWindowCooling(account, "window_week", now) ||
    isWindowCooling(account, "window_month", now)
  );
}

export function isUsageLimitReached(
  account: Pick<Account, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">,
  key: UsageKey,
  now = Date.now(),
): boolean {
  return isWindowCooling(account, key, now);
}

export function normalizeUsagePercent(value: number): number {
  return Math.min(100, Math.max(0, Math.round(value * 10) / 10));
}

export function usagePercentFromCost(cost: number, limit: number): number {
  return normalizeUsagePercent((cost / limit) * 100);
}

export function mergeUsageEdit(
  edit: UsageEditState | undefined,
  saved: number,
  force: boolean,
): UsageEditState {
  if (!edit) {
    return {
      draft: saved,
      saved,
      saving: false,
      error: null,
      resets_in_minutes_draft: null,
      resets_in_minutes_saved: null,
    };
  }
  if (!force && (edit.saving || edit.draft !== edit.saved)) {
    return { ...edit, saved };
  }
  return { ...edit, draft: saved, saved, error: null };
}

export function usageProgressStatus(
  account: Pick<Account, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">,
  key: UsageKey,
  percent: number,
  now = Date.now(),
): "success" | "warning" | "error" {
  if (isUsageLimitReached(account, key, now)) return "error";
  return percent >= 80 ? "warning" : "success";
}

export function usageProgressPercentage(
  account: Pick<Account, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">,
  key: UsageKey,
  percent: number,
  now = Date.now(),
): number {
  return isUsageLimitReached(account, key, now) ? 100 : normalizeUsagePercent(percent);
}
