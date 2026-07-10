import type { Account } from "../api/tauri";

export type UsageKey = "window_5h" | "window_week" | "window_month";

const limitMarkers: Record<UsageKey, string> = {
  window_5h: "5-hour usage limit reached",
  window_week: "weekly usage limit reached",
  window_month: "monthly usage limit reached",
};

export function isCooling(account: Pick<Account, "cooldown_until">): boolean {
  return account.cooldown_until !== null && Date.parse(account.cooldown_until) > Date.now();
}

export function isUsageLimitReached(
  account: Pick<Account, "cooldown_until" | "last_error">,
  key: UsageKey,
): boolean {
  return (
    isCooling(account) &&
    account.last_error?.toLowerCase().includes(limitMarkers[key]) === true
  );
}

export function usageProgressStatus(
  account: Pick<Account, "cooldown_until" | "last_error">,
  key: UsageKey,
  percent: number,
): "success" | "warning" | "error" {
  if (isUsageLimitReached(account, key)) return "error";
  return percent >= 80 ? "warning" : "success";
}
