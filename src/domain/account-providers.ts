import type {
  Account,
  AccountCredentialKind,
  AccountQuotaScope,
} from "../api/dashboard";
import { PLAN_DEFINITIONS } from "./plans.ts";

/**
 * Built-in provider registry. The backend owns the DTO fields
 * (`provider_id`, `credential_kind`, `quota_scope`); this module only
 * holds the frontend's static knowledge of the built-in providers so
 * forms and cards can branch without inventing new endpoints.
 */

export type ProviderOffering = {
  provider_id: string;
  /** Display name shown in the account form and cards. */
  label: string;
  credential_kind: AccountCredentialKind;
  quota_scope: AccountQuotaScope;
  /** Managed registration wizard is only available for this provider. */
  managed_registration: boolean;
};

/** Existing and migrated accounts default to OpenCode Go. */
export const DEFAULT_PROVIDER_ID = "opencode";

export const COMMAND_CODE_PROVIDER_ID = "command-code";
export const MINIMAX_PROVIDER_ID = "minimax";
export const KIMI_PROVIDER_ID = "kimi";

/** Built-in singleton Zen Free account; created and owned by the backend. */
export const ZEN_FREE_ACCOUNT_ID = "00000000-0000-0000-0000-000000000002";
export const ZEN_FREE_PROVIDER_ID = "opencode-zen-free";

/** Static external-integration singleton; it is routable but not a Provider Plan. */
export const CPA_ACCOUNT_ID = "00000000-0000-0000-0000-000000000003";
export const CPA_PROVIDER_ID = "cpa";

const ALL_PROVIDER_OFFERINGS: readonly ProviderOffering[] = PLAN_DEFINITIONS.map((plan) => ({
  provider_id: plan.provider_id,
  label: plan.label,
  credential_kind: plan.credential_kind,
  quota_scope: plan.quota_scope,
  managed_registration: plan.managed_registration,
}));

export const ZEN_FREE_OFFERING: ProviderOffering = ALL_PROVIDER_OFFERINGS.find((offering) => (
  offering.provider_id === ZEN_FREE_PROVIDER_ID
))!;

export const PROVIDER_OFFERINGS: readonly ProviderOffering[] = ALL_PROVIDER_OFFERINGS.filter(
  (offering) => offering !== ZEN_FREE_OFFERING,
);

export function isZenFreeAccount(
  account: Pick<Account, "id" | "provider_id">,
): boolean {
  return account.id === ZEN_FREE_ACCOUNT_ID
    || account.provider_id === ZEN_FREE_PROVIDER_ID;
}

export function isCpaIntegrationAccount(
  account: Pick<Account, "id" | "provider_id">,
): boolean {
  return account.id === CPA_ACCOUNT_ID
    || account.provider_id === CPA_PROVIDER_ID;
}

export function isCommandCodeGoatAccount(
  account: Pick<Account, "provider_id">,
): boolean {
  return account.provider_id === COMMAND_CODE_PROVIDER_ID;
}

export function isOfficialCnPlanAccount(
  account: Pick<Account, "provider_id">,
): boolean {
  return account.provider_id === MINIMAX_PROVIDER_ID || account.provider_id === KIMI_PROVIDER_ID;
}

export function findProviderOffering(providerId: string): ProviderOffering | undefined {
  return ALL_PROVIDER_OFFERINGS.find((offering) => offering.provider_id === providerId);
}

export function providerOfferingLabel(
  account: Pick<Account, "provider_id">,
): string {
  return findProviderOffering(account.provider_id)?.label
    ?? account.provider_id;
}
