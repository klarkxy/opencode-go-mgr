import type { Account } from "../api/dashboard.ts";
import type { PlanDefinition } from "../domain/plans.ts";
import { planForAccount } from "../domain/plans.ts";
import { isCooling, isFreeCooling } from "../domain/accounts-usage.ts";
import { isZenFreeAccount } from "../domain/account-providers.ts";

/**
 * Plan/status filters for the Accounts workbench. Both filters are pure and
 * client-side: the account list is already fully loaded, and filtering must
 * never change the manually ordered priority sequence.
 */

export type AccountStatusKey =
  | "available"
  | "cooling"
  | "auth-error"
  | "disabled"
  | "registering";

export type AccountPlanFilter = "all" | string;
export type AccountStatusFilter = "all" | AccountStatusKey;

/** The single status bucket an account belongs to right now. */
export function accountStatusKey(account: Account, now: number = Date.now()): AccountStatusKey {
  if (isZenFreeAccount(account)) {
    if (!account.enabled) return "disabled";
    return isFreeCooling(account, now) ? "cooling" : "available";
  }
  if (account.setup_step !== "ready") return "registering";
  if (account.auth_error) return "auth-error";
  if (!account.enabled) return "disabled";
  return isCooling(account, now) ? "cooling" : "available";
}

/** The plan family id an account belongs to, or the raw provider id for unknown plans. */
export function accountPlanKey(account: Pick<Account, "provider_id">): string {
  const plan: PlanDefinition | null = planForAccount(account);
  if (plan?.id === "dynamic-http") return account.provider_id;
  return plan ? plan.id : account.provider_id;
}

export function filterAccounts(
  accounts: readonly Account[],
  planFilter: AccountPlanFilter,
  statusFilter: AccountStatusFilter,
  now: number = Date.now(),
): Account[] {
  return accounts.filter((account) => {
    if (planFilter !== "all" && accountPlanKey(account) !== planFilter) return false;
    if (statusFilter !== "all" && accountStatusKey(account, now) !== statusFilter) return false;
    return true;
  });
}

/** Plans that have at least one account, in stable registry order. */
export function plansInUse(
  accounts: readonly Account[],
  registry: readonly PlanDefinition[],
  extras: readonly PlanDefinition[] = [],
): PlanDefinition[] {
  const used = new Set(accounts.map((account) => accountPlanKey(account)));
  const builtin = registry.filter((plan) => used.has(plan.id));
  const extra = extras.filter((plan) => used.has(plan.provider_id) || used.has(plan.id));
  return [...builtin, ...extra];
}
