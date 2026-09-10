import type { Account } from "../api/dashboard.ts";
import type {
  CardCapabilitySummary,
  CapabilitySummary,
  ContractScopeKind,
  CustomEndpointContract,
  EffectiveCatalog,
  EffectiveModelContract,
  ModelProtocolOverrideUpdate,
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

export const CN_PROTOCOL_CHOICES: readonly ProviderProtocol[] = [
  "chat_completions",
  "messages",
];

/**
 * Provider ids of the official CN plans whose models are published with two
 * selectable upstream protocols (Chat Completions + Messages). The frontend
 * uses this to decide whether to render a single static protocol label or a
 * two-way radio group per row.
 */
const TWO_PROTOCOL_SCOPE_PROVIDER_IDS: ReadonlySet<string> = new Set([
  "minimax",
  "kimi",
]);

/**
 * The set of upstream protocols a scope allows an operator to choose from.
 * - CN two-protocol scopes (MiniMax CN / Kimi CN) → `[chat_completions, messages]`
 * - Custom endpoints → the union of protocols any model in the scope has
 *   evidence of being available for (matches the old column-filter behavior)
 * - All other built-in scopes → a single-element marker meaning "no protocol
 *   choice"; each model's target protocol is then derived per model by
 *   {@link modelTargetProtocol} (OpenCode Go mixes Chat/Responses/Messages
 *   rows, so a scope-level answer would be wrong)
 * Returns `[chat_completions]` as a last-resort fallback when the scope has
 * no model evidence at all.
 */
export function scopeProtocolChoices(scope: ProviderScopeView): ProviderProtocol[] {
  if (TWO_PROTOCOL_SCOPE_PROVIDER_IDS.has(scope.provider_id)) {
    return [...CN_PROTOCOL_CHOICES];
  }
  if (scope.scope_kind === "custom_endpoint") {
    return PROVIDER_PROTOCOLS.filter((protocol) => (
      scope.models.some((model) => model.protocols[protocol]?.available === true)
    ));
  }
  for (const model of scope.models) {
    for (const protocol of PROVIDER_PROTOCOLS) {
      if (model.protocols[protocol]) {
        return [protocol];
      }
    }
  }
  return ["chat_completions"];
}

/**
 * The single protocol a model row advertises under this scope. Used to render
 * the static badge (single-protocol scope) or seed the radio group (two-protocol
 * scope). Returns null when the model has no protocol evidence under the
 * scope (e.g. a Custom endpoint row whose declared capabilities carry no
 * protocol evidence). Built-in refresh-discovered models unknown to the
 * static preset materialize with the provider-level default protocol on the
 * backend, so they still resolve a target here.
 */
/**
 * The protocol whose row switch should read ON: the enabled preferred
 * protocol, then the first enabled fallback. When any protocol is actually
 * enabled the UI must never present the row as off, so this always wins over
 * available-but-disabled evidence.
 */
function modelEnabledTarget(model: ProviderModelContract): ProviderProtocol | null {
  const preferred = model.preferred_protocol;
  if (preferred && model.protocols[preferred]?.enabled) return preferred;
  for (const protocol of PROVIDER_PROTOCOLS) {
    if (model.protocols[protocol]?.enabled) return protocol;
  }
  return null;
}

/**
 * The protocol shown while the row is off: the available preferred protocol,
 * then the first available fallback. Null when the model has no protocol
 * evidence at all (the row stays disabled).
 */
function modelAvailableTarget(model: ProviderModelContract): ProviderProtocol | null {
  const preferred = model.preferred_protocol;
  if (preferred && model.protocols[preferred]?.available) return preferred;
  for (const protocol of PROVIDER_PROTOCOLS) {
    if (model.protocols[protocol]?.available) return protocol;
  }
  return null;
}

export function modelTargetProtocol(
  model: ProviderModelContract,
  scope: ProviderScopeView,
): ProviderProtocol | null {
  const choices = scopeProtocolChoices(scope);
  if (choices.length === 0) return null;
  if (choices.length === 1) {
    // Single-protocol scopes still mix per-model static protocols (OpenCode
    // Go has Chat, Responses and Messages rows), so the target comes from
    // the model's own evidence: an enabled protocol always wins so the row
    // can never display off while an alternate is actually enabled;
    // otherwise the available preferred protocol, then the first available
    // one. Models with nothing available (e.g. `NO_PROTOCOLS` rows) have no
    // operable target.
    return modelEnabledTarget(model) ?? modelAvailableTarget(model);
  }
  // Two-protocol scope: preferred wins when enabled, otherwise the other
  // enabled one, otherwise preferred (so the UI always reflects intent).
  if (choices.includes(model.preferred_protocol)
    && model.protocols[model.preferred_protocol]?.enabled) {
    return model.preferred_protocol;
  }
  for (const choice of choices) {
    if (model.protocols[choice]?.enabled) return choice;
  }
  return model.preferred_protocol && choices.includes(model.preferred_protocol)
    ? model.preferred_protocol
    : choices[0]!;
}

/**
 * Whether the row's current target protocol is enabled by the contract.
 * The "启用" column of the matrix is bound to this.
 */
export function modelEffectiveOn(
  model: ProviderModelContract,
  scope: ProviderScopeView,
): boolean {
  const target = modelTargetProtocol(model, scope);
  if (target === null) return false;
  return model.protocols[target]?.enabled === true;
}

/**
 * The protocols an override batch may legally write for one model. Built-in
 * provider scopes accept exactly the model's non-null protocol evidence rows
 * (a CN model's Responses slot is null and absent, so it is never written);
 * Custom endpoint contracts retain all three rows but only the declared
 * (available) ones are writable. The backend validator rejects anything
 * outside this ceiling — even `force_off` — so batches must never exceed it.
 * Filtering is by writability, never by enabled state, so a fully disabled
 * row stays re-enableable.
 */
function modelWritableProtocols(
  model: ProviderModelContract,
  scope: ProviderScopeView,
): ProviderProtocol[] {
  return PROVIDER_PROTOCOLS.filter((protocol) => (
    scope.scope_kind === "custom_endpoint"
      ? model.protocols[protocol]?.available === true
      : model.protocols[protocol] !== undefined
  ));
}

/**
 * Build the override batch that toggles each model in `modelIds` fully on or
 * fully off, touching only the model's legal writable protocols (see
 * {@link modelWritableProtocols}).
 *
 * Design choice (documented for reviewers): on=true explicitly force-enables
 * the model's chosen available target protocol and force-disables every other
 * writable protocol — never `auto`, because under `auto` the static default
 * keeps extra models (e.g. GOAT's) off and the operator's "on" would be a
 * no-op. The target is the persisted preferred protocol when it is available,
 * otherwise the first available fallback; models with no available protocol
 * are skipped so the write never exceeds the model's evidence ceiling.
 * on=false force-disables every writable protocol. In a two-protocol (CN)
 * scope the off batch also marks the row's current active choice with
 * `preferred: true` so the saved selection survives the full-off write and a
 * later enable returns to it; non-CN scopes omit the flag. A model with no
 * writable protocol at all is skipped as a no-op since there is nothing to
 * enable or disable.
 */
export function buildModelToggleOverrides(
  scope: ProviderScopeView,
  modelIds: readonly string[],
  on: boolean,
): ModelProtocolOverrideUpdate[] {
  const overrides: ModelProtocolOverrideUpdate[] = [];
  const choices = scopeProtocolChoices(scope);
  const isTwoProtocol = choices.length === 2;
  for (const modelId of modelIds) {
    const model = scope.models.find((entry) => entry.model_id === modelId);
    if (!model) continue;
    const writable = modelWritableProtocols(model, scope);
    if (writable.length === 0) continue;
    if (!on) {
      const active = isTwoProtocol ? modelTargetProtocol(model, scope) : null;
      for (const protocol of writable) {
        overrides.push({
          model_id: modelId,
          protocol,
          state: "force_off",
          ...(active === protocol ? { preferred: true } : {}),
        });
      }
      continue;
    }
    const target = isTwoProtocol
      ? (model.protocols[model.preferred_protocol]?.available
        ? model.preferred_protocol
        : choices.find((choice) => model.protocols[choice]?.available) ?? null)
      : modelAvailableTarget(model);
    if (!target || !writable.includes(target)) continue;
    for (const protocol of writable) {
      overrides.push({
        model_id: modelId,
        protocol,
        state: protocol === target ? "force_on" : "force_off",
      });
    }
  }
  return overrides;
}

/**
 * Build the override batch that switches a single model between the two
 * protocols a two-protocol scope offers. The chosen item always carries
 * `preferred: true` so the selection is persisted as the saved choice
 * regardless of the row's on/off state: switching while the row is enabled
 * enables only the chosen protocol; switching while it is disabled keeps
 * every protocol off and stores just the choice. Returns an empty array when
 * the scope isn't a two-protocol scope, the model is unknown, or the
 * requested protocol isn't one of its choices (so the call is safe to wire
 * into a single, unconditional emit).
 */
export function buildModelProtocolSwitchOverrides(
  scope: ProviderScopeView,
  modelId: string,
  protocol: ProviderProtocol,
): ModelProtocolOverrideUpdate[] {
  const choices = scopeProtocolChoices(scope);
  if (choices.length !== 2) return [];
  if (!choices.includes(protocol)) return [];
  const model = scope.models.find((entry) => entry.model_id === modelId);
  if (!model) return [];
  const rowOn = modelEffectiveOn(model, scope);
  return choices.map((choice) => ({
    model_id: modelId,
    protocol: choice,
    state: rowOn && choice === protocol ? "force_on" : "force_off",
    ...(choice === protocol ? { preferred: true } : {}),
  }));
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
