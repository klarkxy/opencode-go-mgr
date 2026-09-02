import { dashboardV3, isRevisionConflict, type WithoutExpectation } from "./dashboard-v3.ts";
import { useControlPlaneStore } from "../stores/controlPlane.ts";
import type {
  AccountCredentialKind,
  AccountQuotaScope,
  ModelProtocolOverridesUpdate,
  ProviderCatalogEntry as V3ProviderCatalogEntry,
  ProviderContracts as V3ProviderContracts,
  ProviderPricing as V3ProviderPricing,
  ProviderPricingSnapshot as V3ProviderPricingSnapshot,
  ProviderModels as V3ProviderModels,
  ProviderUsage as V3ProviderUsage,
  OllamaUsageStatus as V3OllamaUsage,
  ProtocolOverrideState as V3ProtocolOverrideState,
  ProtocolProbeResponse as V3ProtocolProbeResponse,
  ZenFreeModels as V3ZenFreeModels,
  ZenFreeSettings,
} from "./generated/dashboard-v3.ts";
import { presentAccount, presentPricing, type Account, type AccountProtocol, type PricingSnapshot } from "./dashboard-presenters.ts";

export { isRevisionConflict };

/**
 * Typed wrappers for the provider-scoped dashboard endpoints. These live
 * outside the page layer so provider catalog/pricing/usage/settings calls share
 * the `http.ts` transport without growing the legacy account surface; Zen
 * provider settings must go through `updateProviderSettings`, never the
 * generic account PATCH.
 */

export interface ProviderCatalogFormField {
  id: string;
  kind: "text" | "secret" | "date" | "url" | "select" | "models";
  required: boolean;
  immutable_after_create: boolean;
}

export interface ProviderCatalogEntry {
  provider_id: string;
  offering_id: string;
  display_name: string;
  display_family: string;
  credential_kind: AccountCredentialKind;
  quota_scope: AccountQuotaScope;
  singleton: boolean;
  creation_availability: "available" | "unavailable";
  creation_unavailable_reason?: string | null;
  verification_policy: "not_required" | "required";
  verification_runtime_availability: "optional" | "unavailable" | "not_applicable" | "available";
  routable: boolean;
  managed_registration: boolean;
  pricing_availability: "available" | "unavailable" | "not_applicable" | "unpriced";
  usage_availability: "available" | "unavailable" | "local_state";
  manual_usage_calibration: boolean;
  quota_unit: string;
  model_source: string;
  key_prefix?: string | null;
  auth_schemes: ("bearer" | "x-api-key")[];
  upstream_protocols: ("chat_completions" | "responses" | "messages")[];
  form_fields: ProviderCatalogFormField[];
  model_aliases: string[];
}

export type ProviderProtocol = AccountProtocol;

export interface ProviderModelCapability {
  model_id: string;
  provider_id: string;
  offering_id: string;
  preferred_protocol: ProviderProtocol;
  supported_protocols: ProviderProtocol[];
}

export interface StoredProviderPricingValue {
  model_id: string;
  display_name: string;
  input_per_million: number | null;
  output_per_million: number | null;
  cache_read_per_million: number | null;
  cache_write_per_million: number | null;
  plan_limit: number | null;
  model_allowance: number | null;
  quota_multiplier: number | null;
  paid_plan_price: number | null;
  currency: string | null;
}

export interface ProviderNeutralPricingSnapshot {
  revision: string;
  activated_at: string;
  document_updated_at: string | null;
  source_url: string;
  content_hash: string;
  evidence: string;
  values: StoredProviderPricingValue[];
}

export type StoredProviderPricingSnapshot = PricingSnapshot | ProviderNeutralPricingSnapshot | {
  provider_id: string;
  offering_id: string;
  revision: string;
  activated_at: string;
  document_updated_at: string | null;
  source_url: string;
  content_hash: string;
  snapshot_json: string;
};

export interface ProviderPricingResponse {
  provider_id: string;
  offering_id: string;
  availability: "available" | "unavailable" | "not_applicable" | "unpriced";
  snapshot?: StoredProviderPricingSnapshot;
  revision: number;
  process_generation: number;
  pricing_revision: string;
  provider_pricing_revision: string;
}

export interface ProviderQuotaWindow {
  account_id: string;
  window_kind: string;
  used: number;
  limit_value: number | null;
  started_at: string | null;
  resets_at: string | null;
  calibration_offset: number;
  unit: string;
  source: string;
  observed_at: string | null;
  updated_at: string;
}

export interface ProviderCreditBalance {
  account_id: string;
  balance_kind: string;
  amount: number;
  unit: string;
  source: string;
  observed_at: string | null;
  updated_at: string;
}

export interface ProviderUsageSyncState {
  last_success_at: string | null;
  last_attempt_at: string | null;
  next_eligible_at: string | null;
  failure_streak: number;
  last_expedited_at: string | null;
}

/** Sanitized Ollama Cloud Cookie-usage status (never carries the Cookie). */
export interface OllamaUsageResponse {
  account_id: string;
  cookie_configured: boolean;
  status: "unconfigured" | "ok" | "unauthorized" | "failed";
  snapshot: {
    windows: { window: string; used_percent: number | null; reset_at: string | null }[];
    models: { model: string; requests_5h: number | null; requests_7d: number | null }[];
    plan: string | null;
    balance: string | null;
  } | null;
  /** Sanitized failure reason (≤256 chars, no HTML/query strings) or null. */
  last_error: string | null;
  last_success_at: string | null;
  last_attempt_at: string | null;
  next_eligible_at: string | null;
  failure_streak: number;
}

export interface ProviderUsageResponse {
  account_id: string;
  provider_id: string;
  offering_id: string;
  availability: string;
  quota_windows: ProviderQuotaWindow[];
  credit_balances: ProviderCreditBalance[];
  sync_state: ProviderUsageSyncState | null;
}

export interface ProviderSettingsUpdate {
  enabled: boolean;
  /** Settings revision guard; omit only when no revision has been loaded. */
  expected_revision?: number;
}

export interface ProviderSettingsResponse {
  account: Account;
  revision: number;
}

export interface ZenFreeModelEntry {
  model_id: string;
  alias: string;
}

export interface ZenFreeModelsResponse {
  account_id: string;
  models: ZenFreeModelEntry[];
  refreshed_at: string | null;
  source_url: string;
}

export type ContractScopeKind = "provider" | "custom_endpoint";
export type ContractEvidenceSource = "static" | "preset" | "probe_confirmed" | "probe_observed";
export type ProbeResultKind = "success" | "failure";
export type ConnectionVerificationStatus = "not_required" | "pending" | "verified" | "failed";
export type ProtocolOverrideState = V3ProtocolOverrideState;

export interface EffectiveCatalog {
  source: string;
  source_url: string;
  refreshed_at: string | null;
  models: string[];
  refresh_supported: boolean;
}

export interface EffectiveProtocolEvidence {
  protocol: ProviderProtocol;
  available: boolean;
  enabled: boolean;
  source: ContractEvidenceSource;
  verified_at: string | null;
  observed_at: string | null;
  last_probe_result: ProbeResultKind | null;
  last_probe_at: string | null;
  last_probe_error: string | null;
  override: ProtocolOverrideState;
}

export interface EffectiveModelContract {
  alias: string;
  model_id: string;
  preferred_protocol: ProviderProtocol;
  protocols: Record<string, EffectiveProtocolEvidence>;
  routable: boolean;
  disabled_reasons: string[];
}

export interface ProviderAccountChoice {
  id: string;
  name: string;
  enabled: boolean;
  verification_status: ConnectionVerificationStatus;
}

export interface ProviderOfferingChoice {
  offering_id: string;
  display_name: string;
  routable: boolean;
  accounts: ProviderAccountChoice[];
}

export interface CapabilitySummary {
  availability: string;
}

export interface CardCapabilitySummary {
  fetch_zen_models: boolean;
  discover_models: boolean;
  protocol_probe: boolean;
  catalog_refresh: boolean;
}

export interface ProviderContractGroup {
  scope_kind: ContractScopeKind;
  scope_id: string;
  provider_id: string;
  static_protocol_snapshot_date: string | null;
  offerings: ProviderOfferingChoice[];
  catalog: EffectiveCatalog;
  models: EffectiveModelContract[];
  pricing: CapabilitySummary;
  usage: CapabilitySummary;
  card: CardCapabilitySummary;
  catalog_routable: boolean;
  production_inference: boolean;
  disabled_reasons: string[];
  revision: number;
}

export interface CustomEndpointContract {
  scope_kind: ContractScopeKind;
  scope_id: string;
  provider_id: string;
  account: ProviderAccountChoice;
  catalog: EffectiveCatalog;
  models: EffectiveModelContract[];
  pricing: CapabilitySummary;
  usage: CapabilitySummary;
  card: CardCapabilitySummary;
  catalog_routable: boolean;
  production_inference: boolean;
  disabled_reasons: string[];
  revision: number;
}

export interface ProviderContractsResponse {
  /** Shared settings revision for PUT `expected_revision`. Distinct from each scope `revision`. */
  revision: number;
  providers: ProviderContractGroup[];
  custom_endpoints: CustomEndpointContract[];
}

export interface ProtocolProbeRequest {
  model_id: string;
  protocols: ProviderProtocol[];
}

export interface ModelProtocolOverrideUpdate {
  model_id: string;
  protocol: ProviderProtocol;
  state: ProtocolOverrideState;
}

export interface ProtocolProbeResult {
  protocol: ProviderProtocol;
  success: boolean;
  skipped: boolean;
  error: string | null;
}

export interface ProtocolProbeResponse {
  model_id: string;
  results: ProtocolProbeResult[];
  contract: EffectiveModelContract | null;
}

export interface CustomCatalogRefreshResponse {
  scope_kind: ContractScopeKind;
  scope_id: string;
  models: string[];
  truncated: boolean;
  refreshed_at: string;
  source: string;
  declared_capabilities_unchanged: boolean;
}

export type ProviderModelsRefreshResponse = ZenFreeModelsResponse | CustomCatalogRefreshResponse | V3ProviderModels;

export function isCustomCatalogRefreshResponse(
  value: ProviderModelsRefreshResponse,
): value is CustomCatalogRefreshResponse {
  return "scope_kind" in value && "truncated" in value;
}

function creationAvailability(value: string): ProviderCatalogEntry["creation_availability"] {
  return value === "available" ? "available" : "unavailable";
}

function verificationPolicy(value: string): ProviderCatalogEntry["verification_policy"] {
  return value === "required" ? "required" : "not_required";
}

function verificationRuntime(value: string): ProviderCatalogEntry["verification_runtime_availability"] {
  if (value === "available" || value === "unavailable" || value === "optional") return value;
  return "not_applicable";
}

function pricingAvailability(value: string): ProviderCatalogEntry["pricing_availability"] {
  if (value === "available" || value === "unavailable" || value === "unpriced") return value;
  return "not_applicable";
}

function usageAvailability(value: string): ProviderCatalogEntry["usage_availability"] {
  if (value === "available" || value === "local_state") return value;
  return "unavailable";
}

function formFieldKind(value: string): ProviderCatalogFormField["kind"] {
  if (value === "secret" || value === "date"
    || value === "url" || value === "select" || value === "models") return value;
  return "text";
}

export function presentCatalogEntry(value: V3ProviderCatalogEntry): ProviderCatalogEntry {
  return {
    provider_id: value.providerId,
    offering_id: value.offeringId,
    display_name: value.displayName,
    display_family: value.displayFamily,
    credential_kind: value.credentialKind,
    quota_scope: value.quotaScope,
    singleton: value.singleton,
    creation_availability: creationAvailability(value.creationAvailability),
    creation_unavailable_reason: value.creationUnavailableReason,
    verification_policy: verificationPolicy(value.verificationPolicy),
    verification_runtime_availability: verificationRuntime(value.verificationRuntimeAvailability),
    routable: value.routable,
    managed_registration: value.managedRegistration,
    pricing_availability: pricingAvailability(value.pricingAvailability),
    usage_availability: usageAvailability(value.usageAvailability),
    manual_usage_calibration: value.manualUsageCalibration,
    quota_unit: value.quotaUnit,
    model_source: value.modelSource,
    key_prefix: value.keyPrefix,
    auth_schemes: [...value.authSchemes],
    upstream_protocols: [...value.upstreamProtocols],
    form_fields: value.formFields.map((field) => ({
      id: field.id,
      kind: formFieldKind(field.kind),
      required: field.required,
      immutable_after_create: field.immutableAfterCreate,
    })),
    model_aliases: [...value.modelAliases],
  };
}

function presentEvidence(value: V3ProviderContracts["providers"][number]["models"][number]["protocols"]["chat_completions"]): EffectiveProtocolEvidence | undefined {
  if (value === null) return undefined;
  return {
    protocol: value.protocol,
    available: value.available,
    enabled: value.enabled,
    source: value.source,
    verified_at: value.verifiedAt,
    observed_at: value.observedAt,
    last_probe_result: value.lastProbeResult,
    last_probe_at: value.lastProbeAt,
    last_probe_error: value.lastProbeError,
    override: value.override,
  };
}

function presentModel(value: V3ProviderContracts["providers"][number]["models"][number]): EffectiveModelContract {
  const protocols: Record<string, EffectiveProtocolEvidence> = {};
  const chat = presentEvidence(value.protocols.chat_completions);
  const responses = presentEvidence(value.protocols.responses);
  const messages = presentEvidence(value.protocols.messages);
  if (chat) protocols.chat_completions = chat;
  if (responses) protocols.responses = responses;
  if (messages) protocols.messages = messages;
  return {
    alias: value.alias,
    model_id: value.modelId,
    preferred_protocol: value.preferredProtocol,
    protocols,
    routable: value.routable,
    disabled_reasons: [...value.disabledReasons],
  };
}

function presentCard(value: V3ProviderContracts["providers"][number]["card"]): CardCapabilitySummary {
  return {
    fetch_zen_models: value.fetchZenModels,
    discover_models: value.discoverModels,
    protocol_probe: value.protocolProbe,
    catalog_refresh: value.catalogRefresh,
  };
}

function presentCatalog(value: V3ProviderContracts["providers"][number]["catalog"]): EffectiveCatalog {
  return {
    source: value.source,
    source_url: value.sourceUrl,
    refreshed_at: value.refreshedAt,
    models: [...value.models],
    refresh_supported: value.refreshSupported,
  };
}

function presentAccountChoice(value: V3ProviderContracts["providers"][number]["offerings"][number]["accounts"][number]): ProviderAccountChoice {
  return {
    id: value.id,
    name: value.name,
    enabled: value.enabled,
    verification_status: value.verificationStatus,
  };
}

export function presentContracts(value: V3ProviderContracts): ProviderContractsResponse {
  return {
    revision: value.revision,
    providers: value.providers.map((scope) => ({
      scope_kind: scope.scopeKind,
      scope_id: scope.scopeId,
      provider_id: scope.providerId,
      static_protocol_snapshot_date: scope.staticProtocolSnapshotDate,
      offerings: scope.offerings.map((offering) => ({
        offering_id: offering.offeringId,
        display_name: offering.displayName,
        routable: offering.routable,
        accounts: offering.accounts.map(presentAccountChoice),
      })),
      catalog: presentCatalog(scope.catalog),
      models: scope.models.map(presentModel),
      pricing: { availability: scope.pricing.availability },
      usage: { availability: scope.usage.availability },
      card: presentCard(scope.card),
      catalog_routable: scope.catalogRoutable,
      production_inference: scope.productionInference,
      disabled_reasons: [...scope.disabledReasons],
      revision: scope.revision,
    })),
    custom_endpoints: value.customEndpoints.map((scope) => ({
      scope_kind: scope.scopeKind,
      scope_id: scope.scopeId,
      provider_id: scope.providerId,
      account: presentAccountChoice(scope.account),
      catalog: presentCatalog(scope.catalog),
      models: scope.models.map(presentModel),
      pricing: { availability: scope.pricing.availability },
      usage: { availability: scope.usage.availability },
      card: {
        ...presentCard(scope.card),
        // Custom per-protocol probing has no V3 endpoint yet (deferred).
        protocol_probe: false,
      },
      catalog_routable: scope.catalogRoutable,
      production_inference: scope.productionInference,
      disabled_reasons: [...scope.disabledReasons],
      revision: scope.revision,
    })),
  };
}

function presentProviderPricing(value: V3ProviderPricing): ProviderPricingResponse {
  return {
    provider_id: value.providerId,
    offering_id: value.offeringId,
    availability: value.availability,
    snapshot: value.providerSnapshot === null
      ? (value.snapshot === null ? undefined : presentPricing(value.snapshot))
      : presentProviderPricingSnapshot(value.providerSnapshot),
    revision: value.revision,
    process_generation: value.processGeneration,
    pricing_revision: value.pricingRevision,
    provider_pricing_revision: value.providerPricingRevision,
  };
}

function presentProviderPricingSnapshot(value: V3ProviderPricingSnapshot): ProviderNeutralPricingSnapshot {
  return {
    revision: value.revision,
    activated_at: value.activatedAt,
    document_updated_at: value.documentUpdatedAt,
    source_url: value.sourceUrl,
    content_hash: value.contentHash,
    evidence: value.evidence,
    values: value.values.map((row) => ({
      model_id: row.modelId,
      display_name: row.displayName,
      input_per_million: row.inputPerMillion,
      output_per_million: row.outputPerMillion,
      cache_read_per_million: row.cacheReadPerMillion,
      cache_write_per_million: row.cacheWritePerMillion,
      plan_limit: row.planLimit,
      model_allowance: row.modelAllowance,
      quota_multiplier: row.quotaMultiplier,
      paid_plan_price: row.paidPlanPrice,
      currency: row.currency,
    })),
  };
}

function finiteOrNull(value: number | null): number | null {
  return value !== null && Number.isFinite(value) ? value : null;
}

export function presentOllamaUsage(value: V3OllamaUsage): OllamaUsageResponse {
  const snapshot = value.snapshot;
  return {
    account_id: value.accountId,
    cookie_configured: value.cookieConfigured,
    status: value.status as OllamaUsageResponse["status"],
    snapshot: snapshot === null ? null : {
      windows: snapshot.windows.map((window) => ({
        window: window.window,
        used_percent: finiteOrNull(window.used_percent),
        reset_at: window.reset_at ?? null,
      })),
      models: snapshot.models.map((model) => ({
        model: model.model,
        requests_5h: finiteOrNull(model.requests_5h),
        requests_7d: finiteOrNull(model.requests_7d),
      })),
      plan: snapshot.plan ?? null,
      balance: snapshot.balance ?? null,
    },
    last_error: value.lastError,
    last_success_at: value.lastSuccessAt,
    last_attempt_at: value.lastAttemptAt,
    next_eligible_at: value.nextEligibleAt,
    failure_streak: value.failureStreak,
  };
}

function presentProviderUsage(value: V3ProviderUsage): ProviderUsageResponse {
  return {
    account_id: value.accountId,
    provider_id: value.providerId,
    offering_id: value.offeringId,
    availability: value.availability,
    quota_windows: value.quotaWindows.map((window) => ({
      account_id: window.accountId,
      window_kind: window.windowKind,
      used: window.used,
      limit_value: window.limitValue,
      started_at: window.startedAt,
      resets_at: window.resetsAt,
      calibration_offset: window.calibrationOffset,
      unit: window.unit,
      source: window.source,
      observed_at: window.observedAt,
      updated_at: window.updatedAt,
    })),
    credit_balances: value.creditBalances.map((balance) => ({
      account_id: balance.accountId,
      balance_kind: balance.balanceKind,
      amount: balance.amount,
      unit: balance.unit,
      source: balance.source,
      observed_at: balance.observedAt,
      updated_at: balance.updatedAt,
    })),
    sync_state: value.syncState === null ? null : {
      last_success_at: value.syncState.lastSuccessAt,
      last_attempt_at: value.syncState.lastAttemptAt,
      next_eligible_at: value.syncState.nextEligibleAt,
      failure_streak: value.syncState.failureStreak,
      last_expedited_at: value.syncState.lastExpeditedAt,
    },
  };
}

function presentZenModels(value: V3ZenFreeModels): ZenFreeModelsResponse {
  return {
    account_id: value.accountId,
    models: value.models.map((model) => ({ model_id: model.modelId, alias: model.alias })),
    refreshed_at: value.refreshedAt,
    source_url: value.sourceUrl,
  };
}

function presentProbe(value: V3ProtocolProbeResponse): ProtocolProbeResponse {
  return {
    model_id: value.modelId,
    results: value.results.map((result) => ({
      protocol: result.protocol,
      success: result.success,
      skipped: result.skipped,
      error: result.error,
    })),
    contract: value.contract === null ? null : presentModel(value.contract),
  };
}

export const providerApi = {
  getProviderCatalog: async () => (await dashboardV3.getProviders()).entries.map(presentCatalogEntry),
  getProviderModelCapabilities: async (): Promise<ProviderModelCapability[]> =>
    (await dashboardV3.getProviderModelCapabilities()).map((model) => ({
      model_id: model.modelId,
      provider_id: model.providerId,
      offering_id: model.offeringId,
      preferred_protocol: model.preferredProtocol,
      supported_protocols: [...model.supportedProtocols],
    })),
  getProviderPricing: async (providerId: string, offeringId: string) =>
    presentProviderPricing(await dashboardV3.getProviderPricing(providerId, offeringId)),
  updateProviderPricingMultipliers: async (
    providerId: string,
    offeringId: string,
    expectedPricingRevision: string,
    multipliers: Array<{ model_id: string; multiplier: number }>,
  ) => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    return presentProviderPricing(await control.runMutation((expectation) => (
      dashboardV3.putProviderPricingMultipliers(providerId, offeringId, {
        expectedPricingRevision,
        multipliers: multipliers.map((multiplier) => ({
          modelId: multiplier.model_id,
          multiplier: multiplier.multiplier,
        })),
      }, expectation)
    )));
  },
  getGoPricing: async (): Promise<PricingSnapshot> => {
    const result = await dashboardV3.getProviderPricing("opencode", "go");
    if (!result.snapshot) throw new Error("OpenCode Go pricing is not available");
    return presentPricing(result.snapshot);
  },
  getZenFreeSettings: async (): Promise<ZenFreeSettings> => dashboardV3.getZenFreeSettings(),
  getZenFreeModels: async (): Promise<ZenFreeModelsResponse> => presentZenModels(await dashboardV3.getZenFreeModels()),
  setZenFreeEnabled: async (enabled: boolean): Promise<ZenFreeSettings> => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    try {
      return await control.runMutation((expectation) => dashboardV3.patchZenFreeSettings(enabled, expectation));
    } catch (cause) {
      if (isRevisionConflict(cause)) await dashboardV3.getZenFreeSettings();
      throw cause;
    }
  },
  refreshZenFreeModels: async (): Promise<ZenFreeModelsResponse> => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    try {
      return presentZenModels(await control.runMutation((expectation) => dashboardV3.refreshZenFreeModels(expectation)));
    } catch (cause) {
      if (isRevisionConflict(cause)) await dashboardV3.getZenFreeModels();
      throw cause;
    }
  },
  getProviderUsage: async (accountId: string) =>
    presentProviderUsage(await dashboardV3.getProviderUsage(accountId)),
  getOllamaUsage: async (accountId: string): Promise<OllamaUsageResponse> =>
    presentOllamaUsage(await dashboardV3.getOllamaUsage(accountId)),
  refreshOllamaUsage: async (accountId: string): Promise<OllamaUsageResponse> => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    return presentOllamaUsage(
      await control.runMutation((expectation) =>
        dashboardV3.refreshOllamaUsage(accountId, expectation)),
    );
  },
  setOllamaCookie: async (
    accountId: string,
    cookie: string | null,
  ): Promise<OllamaUsageResponse> => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    return presentOllamaUsage(
      await control.runMutation((expectation) =>
        dashboardV3.setOllamaCookie(accountId, cookie, expectation)),
    );
  },
  refreshProviderUsage: async (accountId: string) => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    return presentProviderUsage(await control.runMutation((expectation) =>
      dashboardV3.refreshProviderUsage(accountId, expectation)
    ));
  },
  updateProviderSettings: async (accountId: string, update: ProviderSettingsUpdate) => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    const account = await dashboardV3.getAccount(accountId);
    // Catalog provider_id is `opencode-zen-free`; `/providers/zen-free` is only the V3 route slug.
    if (account.providerId !== "opencode-zen-free") throw new Error("only Zen Free has provider settings");
    try {
      await control.runMutation((expectation) => dashboardV3.patchZenFreeSettings(update.enabled, expectation));
    } catch (cause) {
      if (isRevisionConflict(cause)) await dashboardV3.getZenFreeSettings();
      throw cause;
    }
    const refreshed = await dashboardV3.getAccount(accountId);
    return { account: presentAccount(refreshed), revision: refreshed.revision };
  },
  getProviderModels: async (accountId: string) => {
    const account = await dashboardV3.getAccount(accountId);
    if (account.providerId !== "custom") return presentZenModels(await dashboardV3.getZenFreeModels());
    const config = account.customConfig;
    if (!config) throw new Error("Custom account has no configured destination");
    const discovered = await dashboardV3.discoverCustomModels({
      accountId,
      endpointUrl: config.endpointUrl,
      upstreamProtocol: config.upstreamProtocol,
    });
    return {
      scope_kind: "custom_endpoint",
      scope_id: accountId,
      models: discovered.models,
      truncated: discovered.truncated,
      refreshed_at: "",
      source: "discovered",
      declared_capabilities_unchanged: true,
    } satisfies CustomCatalogRefreshResponse;
  },
  refreshProviderModels: async (accountId: string) => {
    const account = await dashboardV3.getAccount(accountId);
    if (account.providerId === "custom") {
      return providerApi.getProviderModels(accountId);
    }
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    if (account.providerId === "opencode-zen-free") {
      return presentZenModels(await control.runMutation((expectation) => dashboardV3.refreshZenFreeModels(expectation)));
    }
    return control.runMutation((expectation) => dashboardV3.refreshProviderModels(
      account.providerId,
      accountId,
      expectation,
    ));
  },
  refreshContractCatalog: async (scopeKind: ContractScopeKind, scopeId: string) => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    return presentContracts(await control.runMutation((expectation) =>
      dashboardV3.refreshContractCatalog(scopeKind, scopeId, expectation)
    ));
  },
  resetStaticModelProtocols: async (scopeId: string) => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    return presentContracts(await control.runMutation((expectation) =>
      dashboardV3.resetStaticModelProtocols(scopeId, expectation)
    ));
  },
  getProviderContracts: async () => presentContracts(await dashboardV3.getProviderContracts()),
  updateModelProtocolOverrides: async (
    scopeKind: ContractScopeKind,
    scopeId: string,
    overrides: { model_id: string; protocol: ProviderProtocol; state: ProtocolOverrideState }[],
  ): Promise<ProviderContractsResponse> => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    try {
      return presentContracts(await control.runMutation((expectation) =>
        dashboardV3.putModelProtocolOverrides(
          scopeKind,
          scopeId,
          { overrides: overrides.map((item) => ({
            modelId: item.model_id,
            protocol: item.protocol,
            state: item.state,
          })) } satisfies WithoutExpectation<ModelProtocolOverridesUpdate>,
          expectation,
        )));
    } catch (cause) {
      if (isRevisionConflict(cause)) await dashboardV3.getProviderContracts();
      throw cause;
    }
  },
  runProtocolProbes: async (providerId: string, input: ProtocolProbeRequest) => {
    const control = useControlPlaneStore();
    if (!control.hasTokens()) await control.refresh();
    if (providerId === "custom") {
      throw new Error("Custom API 协议探测尚未纳入 Dashboard V3 合同");
    }
    return presentProbe(await control.runMutation((expectation) =>
      dashboardV3.runProviderProtocolProbes(providerId, {
        modelId: input.model_id,
        protocols: input.protocols,
      }, expectation)));
  },
};
