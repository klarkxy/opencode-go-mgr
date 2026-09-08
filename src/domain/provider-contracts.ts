import type { Account } from "../api/dashboard.ts";
import type {
  CardCapabilitySummary,
  CapabilitySummary,
  ContractScopeKind,
  CustomEndpointContract,
  EffectiveCatalog,
  EffectiveModelContract,
  ProviderAccountChoice,
  ProviderCatalogEntry,
  ProviderContractsResponse,
  ProviderProtocol,
} from "../api/providers.ts";
import {
  planFamilyLabel,
  planForAccount,
  PLAN_DEFINITIONS,
} from "./plans.ts";

export const PROVIDER_PROTOCOLS: readonly ProviderProtocol[] = [
  "chat_completions",
  "responses",
  "messages",
];

export function modelProtocolOverrideKey(
  scopeKind: ContractScopeKind,
  scopeId: string,
  modelId: string,
  protocol: ProviderProtocol,
): string {
  return JSON.stringify([scopeKind, scopeId, modelId, protocol]);
}

/**
 * The single protocol a built-in row connection test submits: the preferred
 * protocol when its effective state is enabled, otherwise the first enabled
 * fallback. Null when nothing is enabled — no blind multi-protocol scan.
 */
export function effectiveModelTestProtocol(
  model: Pick<EffectiveModelContract, "preferred_protocol" | "protocols"> | undefined,
): ProviderProtocol | null {
  if (!model) return null;
  if (model.protocols[model.preferred_protocol]?.enabled) return model.preferred_protocol;
  for (const protocol of PROVIDER_PROTOCOLS) {
    if (model.protocols[protocol]?.enabled) return protocol;
  }
  return null;
}

export const CATALOG_SOURCE_STATIC = "static";
export const CATALOG_SOURCE_OFFICIAL_ZEN = "official_zen";
export const CATALOG_SOURCE_CUSTOM_DISCOVERY = "custom_discovery";
export const CATALOG_SOURCE_DECLARED = "account_declared";
export const CATALOG_SOURCE_OPENCODE_MODELS = "opencode_get_models";
export const CATALOG_SOURCE_COMMAND_CODE_MODELS = "command_code_get_models";

export interface ProviderScopeRef {
  scope_kind: ContractScopeKind;
  scope_id: string;
}

export interface ProviderScopeView {
  key: string;
  scope_kind: ContractScopeKind;
  scope_id: string;
  provider_id: string;
  static_protocol_snapshot_date: string | null;
  label: string;
  accounts: ProviderAccountChoice[];
  catalog: EffectiveCatalog;
  models: ProviderModelContract[];
  pricing: CapabilitySummary;
  usage: CapabilitySummary;
  card: CardCapabilitySummary;
  catalog_routable: boolean;
  production_inference: boolean;
  disabled_reasons: string[];
  revision: number;
}

/** Provider rows may publish a stable client Alias alongside their raw upstream id. */
export type ProviderModelContract = EffectiveModelContract & {
  alias?: string;
};

export function providerScopeKey(scopeKind: string, scopeId: string): string {
  return `${scopeKind}:${scopeId}`;
}

/** Match an account to the backend-owned exact contract scope. */
export function findAccountScopeView(
  scopes: readonly ProviderScopeView[],
  account: Pick<Account, "id" | "provider_id">,
): ProviderScopeView | undefined {
  const plan = planForAccount(account);
  if (plan?.kind === "custom") {
    return scopes.find((scope) => (
      scope.scope_kind === "custom_endpoint" && scope.scope_id === account.id
    ));
  }
  return scopes.find((scope) => (
    scope.scope_kind === "provider"
    && scope.provider_id === account.provider_id
  ));
}

export function protocolDisplayName(protocol: ProviderProtocol): string {
  if (protocol === "chat_completions") return "Chat Completions";
  if (protocol === "responses") return "Responses";
  return "Messages";
}

export function isSafeSourceUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return false;
  try {
    const url = new URL(trimmed);
    if (url.protocol !== "https:" && url.protocol !== "http:") return false;
    if (url.username || url.password) return false;
    return Boolean(url.hostname);
  } catch {
    return false;
  }
}

export function normalizeProviderContractsResponse(
  raw: ProviderContractsResponse | null | undefined,
): ProviderContractsResponse {
  if (raw == null) {
    throw new Error("provider contracts response is missing");
  }
  return raw;
}

function providerLabel(
  providerId: string,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): string {
  const plan = PLAN_DEFINITIONS.find((item) => item.provider_id === providerId);
  if (plan) return planFamilyLabel(plan, catalog);
  return providerId;
}

function customEndpointLabel(
  endpoint: CustomEndpointContract,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): string {
  const name = endpoint.account.name.trim();
  if (name) return name;
  const plan = PLAN_DEFINITIONS.find((item) => item.kind === "custom");
  return plan ? planFamilyLabel(plan, catalog) : endpoint.scope_id;
}

export function flattenProviderScopes(
  response: ProviderContractsResponse,
  catalog: readonly ProviderCatalogEntry[] | null | undefined = null,
): ProviderScopeView[] {
  const providers = response.providers.map((group) => ({
    key: providerScopeKey(group.scope_kind, group.scope_id),
    scope_kind: group.scope_kind,
    scope_id: group.scope_id,
    provider_id: group.provider_id,
    static_protocol_snapshot_date: group.static_protocol_snapshot_date,
    label: providerLabel(group.provider_id, catalog),
    accounts: group.accounts,
    catalog: group.catalog,
    models: group.models,
    pricing: group.pricing,
    usage: group.usage,
    card: group.card,
    catalog_routable: group.catalog_routable,
    production_inference: group.production_inference,
    disabled_reasons: group.disabled_reasons,
    revision: group.revision,
  }));
  const custom = response.custom_endpoints.map((endpoint) => ({
    key: providerScopeKey(endpoint.scope_kind, endpoint.scope_id),
    scope_kind: endpoint.scope_kind,
    scope_id: endpoint.scope_id,
    provider_id: endpoint.provider_id,
    static_protocol_snapshot_date: null,
    label: customEndpointLabel(endpoint, catalog),
    accounts: [endpoint.account],
    catalog: endpoint.catalog,
    models: endpoint.models,
    pricing: endpoint.pricing,
    usage: endpoint.usage,
    card: endpoint.card,
    catalog_routable: endpoint.catalog_routable,
    production_inference: endpoint.production_inference,
    disabled_reasons: endpoint.disabled_reasons,
    revision: endpoint.revision,
  }));
  return [...providers, ...custom];
}

export function selectProviderScope(
  scopes: readonly ProviderScopeView[],
  scopeKind: string | null | undefined,
  scopeId: string | null | undefined,
): { scope: ProviderScopeView | null; fellBack: boolean } {
  if (scopes.length === 0) return { scope: null, fellBack: false };
  const match = scopes.find((scope) => (
    scope.scope_kind === scopeKind && scope.scope_id === scopeId
  ));
  if (match) return { scope: match, fellBack: false };
  return { scope: scopes[0] ?? null, fellBack: Boolean(scopeKind || scopeId) };
}

export function catalogRefreshSupported(scope: Pick<ProviderScopeView, "card" | "catalog">): boolean {
  return scope.card.catalog_refresh || scope.catalog.refresh_supported;
}

export function enabledProtocols(scope: Pick<ProviderScopeView, "models">): ProviderProtocol[] {
  return PROVIDER_PROTOCOLS.filter((protocol) => (
    scope.models.some((model) => model.protocols[protocol]?.enabled)
  ));
}

function mergeModelContract(
  models: readonly ProviderModelContract[],
  next: ProviderModelContract,
): ProviderModelContract[] {
  const index = models.findIndex((model) => model.model_id === next.model_id);
  if (index < 0) return [...models, next];
  return models.map((model, itemIndex) => (itemIndex === index ? next : model));
}

export function applyModelContractToResponse(
  response: ProviderContractsResponse,
  scope: ProviderScopeRef,
  contract: EffectiveModelContract,
): ProviderContractsResponse {
  const normalized = normalizeProviderContractsResponse(response);
  if (scope.scope_kind === "custom_endpoint") {
    return {
      ...normalized,
      custom_endpoints: normalized.custom_endpoints.map((endpoint) => (
        endpoint.scope_id === scope.scope_id
          ? { ...endpoint, models: mergeModelContract(endpoint.models, contract) }
          : endpoint
      )),
    };
  }
  return {
    ...normalized,
    providers: normalized.providers.map((group) => (
      group.scope_id === scope.scope_id
        ? { ...group, models: mergeModelContract(group.models, contract) }
        : group
    )),
  };
}
