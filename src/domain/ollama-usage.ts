import type { OllamaUsageResponse } from "../api/providers.ts";

export function isOllamaUsageRefreshBlocked(
  usage: OllamaUsageResponse | null | undefined,
  now: number,
): boolean {
  if (!usage) return false;
  if (!usage.cookie_configured) return true;
  if (!usage.next_eligible_at) return false;
  const nextEligibleAt = Date.parse(usage.next_eligible_at);
  return Number.isFinite(nextEligibleAt) && nextEligibleAt > now;
}
