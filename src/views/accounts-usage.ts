import type { Account } from "../api/tauri";

export type UsageKey = "window_5h" | "window_week" | "window_month";

export type UsageEditState = {
  draft: number;
  saved: number;
  saving: boolean;
  error: string | null;
};

const cooldownFields: Record<UsageKey, keyof Pick<Account, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">> = {
  window_5h: "cooldown_5h_until",
  window_week: "cooldown_week_until",
  window_month: "cooldown_month_until",
};

export function isWindowCooling(
  account: Pick<Account, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">,
  key: UsageKey,
  now = Date.now(),
): boolean {
  const until = account[cooldownFields[key]];
  return until !== null && Date.parse(until) > now;
}

export function resetTimeForWindow(
  account: Pick<Account, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">,
  key: UsageKey,
): string | null {
  return account[cooldownFields[key]];
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
  edit: UsageEditState,
  saved: number,
  force: boolean,
): UsageEditState {
  if (!force && (edit.saving || edit.draft !== edit.saved)) {
    return { ...edit, saved };
  }
  return { ...edit, draft: saved, saved, error: null };
}

export function usageProgressStatus(
  account: Pick<Account, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until">,
  key: UsageKey,
  percent: number,
): "success" | "warning" | "error" {
  if (isUsageLimitReached(account, key)) return "error";
  return percent >= 80 ? "warning" : "success";
}
