import type {
  Account,
  AccountCredentialKind,
  AccountQuotaScope,
} from "../api/dashboard.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import type { MessageKey } from "../i18n/index.ts";

/**
 * The hardcoded plan families of the console. A family maps to one or
 * more backend provider/offering entries. The backend owns the DTO fields; this
 * module owns the stable ordering, family ids, and fallback metadata.
 *
 * Availability is never hardcoded: creation/routing/pricing semantics are
 * resolved from `/dashboard/api/providers/catalog`. Legacy families
 * (OpenCode Go, Zen Free) keep pre-catalog fallback behavior when the catalog
 * is unreachable; every other family fails closed in that case.
 */

export type PlanId =
  | "opencode-go"
  | "zen-free"
  | "command-code-goat"
  | "minimax-cn"
  | "kimi-cn"
  | "custom-endpoint";

export type PlanKind = "quota" | "free" | "api-key" | "custom";

export interface PlanDefinition {
  id: PlanId;
  provider_id: string;
  offering_ids: string[];
  /** Brand label; shown only when the catalog is absent. */
  label: string;
  kind: PlanKind;
  credential_kind: AccountCredentialKind;
  quota_scope: AccountQuotaScope;
  /** Singleton families (Zen Free) are created and owned by the backend. */
  singleton: boolean;
  /** Managed registration wizard availability (OpenCode Go only). */
  managed_registration: boolean;
  /** Legacy families keep their pre-catalog behavior when the catalog fails. */
  legacy: boolean;
}

export const PLAN_DEFINITIONS: readonly PlanDefinition[] = [
  {
    id: "opencode-go",
    provider_id: "opencode",
    offering_ids: ["go"],
    label: "OpenCode Go",
    kind: "quota",
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    managed_registration: true,
    legacy: true,
  },
  {
    id: "zen-free",
    provider_id: "opencode-zen-free",
    offering_ids: ["anonymous-free"],
    label: "Zen Free",
    kind: "free",
    credential_kind: "none",
    quota_scope: "egress-ip",
    singleton: true,
    managed_registration: false,
    legacy: true,
  },
  {
    id: "command-code-goat",
    provider_id: "command-code",
    offering_ids: ["goat"],
    label: "Command Code GOAT",
    kind: "api-key",
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    managed_registration: false,
    legacy: false,
  },
  {
    id: "minimax-cn",
    provider_id: "minimax",
    offering_ids: ["cn"],
    label: "MiniMax CN Token Plan",
    kind: "api-key",
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    managed_registration: false,
    legacy: false,
  },
  {
    id: "kimi-cn",
    provider_id: "kimi",
    offering_ids: ["cn"],
    label: "Kimi Code CN",
    kind: "api-key",
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    managed_registration: false,
    legacy: false,
  },
  {
    id: "custom-endpoint",
    provider_id: "custom",
    offering_ids: ["api"],
    label: "Custom API",
    kind: "custom",
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    managed_registration: false,
    legacy: false,
  },
];

/** Stable legacy import target; the chooser must never open without a plan. */
export const OPENCODE_GO_PLAN = PLAN_DEFINITIONS.find((plan) => plan.id === "opencode-go")!;

export function findCatalogEntry(
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
  providerId: string,
  offeringId: string,
): ProviderCatalogEntry | undefined {
  return catalog?.find(
    (entry) => entry.provider_id === providerId && entry.offering_id === offeringId,
  );
}

/**
 * Find a family definition by the exact backend provider/offering pair.
 * Custom/api maps to "custom-endpoint".
 */
export function findPlanDefinition(
  providerId: string,
  offeringId: string,
): PlanDefinition | undefined {
  return PLAN_DEFINITIONS.find((plan) =>
    plan.provider_id === providerId && plan.offering_ids.includes(offeringId),
  );
}

/** The family an account belongs to; unknown pairs return null (render raw). */
export function planForAccount(
  account: Pick<Account, "provider_id" | "offering_id">,
): PlanDefinition | null {
  return findPlanDefinition(account.provider_id, account.offering_id) ?? null;
}

/**
 * Display label for a provider/offering pair. Prefers the catalog's
 * display_name for the exact offering, then the static family label, then the
 * raw "provider/offering" string.
 */
export function planLabel(
  account: Pick<Account, "provider_id" | "offering_id">,
  catalog?: readonly ProviderCatalogEntry[] | null,
): string {
  const entry = findCatalogEntry(catalog, account.provider_id, account.offering_id);
  if (entry?.display_name) return entry.display_name;
  const definition = findPlanDefinition(account.provider_id, account.offering_id);
  if (definition) return definition.label;
  return `${account.provider_id}/${account.offering_id}`;
}

/**
 * Label for a plan-family control. Single-offering families use the catalog's
 * exact display name; multi-offering families keep their shared family label
 * because no tier has been selected yet.
 */
export function planFamilyLabel(
  plan: PlanDefinition,
  catalog?: readonly ProviderCatalogEntry[] | null,
): string {
  if (plan.offering_ids.length === 1) {
    const offeringId = plan.offering_ids[0];
    if (offeringId) {
      const entry = findCatalogEntry(catalog, plan.provider_id, offeringId);
      if (entry?.display_name.trim()) return entry.display_name.trim();
    }
  }
  return plan.label;
}

/**
 * A family may be chosen in Add Account when the catalog says creation is
 * available and the family is not a backend-owned singleton.
 *
 * Legacy families keep their pre-catalog behavior: OpenCode Go stays creatable
 * even when the catalog is unreachable; non-legacy families fail closed.
 */
export function planCanCreateAccount(
  plan: PlanDefinition,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): boolean {
  if (plan.singleton) return false;
  if (!catalog?.length) return plan.legacy;
  return plan.offering_ids.some((offeringId) => {
    const entry = findCatalogEntry(catalog, plan.provider_id, offeringId);
    return entry?.creation_availability === "available";
  });
}

/** Reason the family cannot be created, or null when it is creatable. */
export function planCreateDisabledReason(
  plan: PlanDefinition,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): MessageKey | null {
  if (plan.singleton) return "单例方案由系统自动管理";
  if (!catalog?.length) return plan.legacy ? null : "服务商目录加载失败";
  const anyEntry = plan.offering_ids
    .map((offeringId) => findCatalogEntry(catalog, plan.provider_id, offeringId))
    .find(Boolean);
  if (!anyEntry) return "服务商目录未提供该方案";
  if (anyEntry.creation_availability !== "available") {
    return "该方案暂不可用";
  }
  return null;
}

/** True when the exact offering is routable according to the catalog. */
export function planRoutable(
  providerId: string,
  offeringId: string,
  catalog?: readonly ProviderCatalogEntry[] | null,
): boolean {
  const entry = findCatalogEntry(catalog, providerId, offeringId);
  return entry?.routable ?? false;
}
