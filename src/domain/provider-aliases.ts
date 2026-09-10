import type { Account } from "../api/dashboard.ts";
import type { ProviderDefinitionView } from "../api/providers.ts";
import type { ProviderScopeView } from "./provider-contracts.ts";

export interface ProviderAliasRow {
  provider_id: string;
  key: string;
  public_model: string;
  provider_plan: string;
  custom_account: string | null;
  upstream_model: string;
  routable: boolean;
  custom_account_id: string | null;
}

function providerPlanLabel(scope: ProviderScopeView): string {
  return scope.label;
}

/**
 * This is a read-only cross-reference. Provider contracts describe built-in
 * Alias resolution; account capabilities describe Custom mappings. No new
 * catalog or state is introduced for the table.
 */
export function providerAliasRows(
  scopes: readonly ProviderScopeView[],
  accounts: readonly Account[],
): ProviderAliasRow[] {
  const rows: ProviderAliasRow[] = [];
  const providerRawModels = new Set(
    scopes
      .filter((scope) => scope.scope_kind === "provider")
      .flatMap((scope) => scope.models.map((model) => model.model_id)),
  );
  for (const scope of scopes) {
    if (scope.scope_kind !== "provider") continue;
    for (const model of scope.models) {
      if (!model.alias) continue;
      rows.push({
        provider_id: scope.provider_id || scope.scope_id,
        key: `${scope.key}:${model.alias}:${model.model_id}`,
        public_model: model.alias,
        provider_plan: providerPlanLabel(scope),
        custom_account: null,
        upstream_model: model.model_id,
        routable: model.routable,
        custom_account_id: null,
      });
    }
  }

  for (const account of accounts) {
    if (account.provider_id !== "custom") continue;
    const scope = scopes.find((candidate) => (
      candidate.scope_kind === "custom_endpoint" && candidate.scope_id === account.id
    ));
    for (const capability of account.model_capabilities) {
      const contract = scope?.models.find((model) => (
        (model.alias || model.model_id).toLocaleLowerCase()
          === capability.public_model.toLocaleLowerCase()
      ));
      const conflictsWithProviderRaw = providerRawModels.has(capability.public_model);
      rows.push({
        provider_id: account.provider_id,
        key: `custom:${account.id}:${capability.public_model}:${capability.upstream_model}`,
        public_model: capability.public_model,
        provider_plan: scope?.label || "Custom API",
        custom_account: account.name,
        upstream_model: capability.upstream_model,
        routable: !conflictsWithProviderRaw
          && account.enabled
          && account.setup_step === "ready"
          && account.plan_routable
          && Boolean(contract?.routable),
        custom_account_id: account.id,
      });
    }
  }
  return rows;
}

export function dynamicProviderAliasRows(
  providers: readonly ProviderDefinitionView[],
): ProviderAliasRow[] {
  return providers.flatMap((provider) => provider.models.map((model) => ({
    provider_id: provider.id,
    key: `dynamic:${provider.id}:${model.public_model}:${model.upstream_model}`,
    public_model: model.public_model,
    provider_plan: provider.name,
    custom_account: null,
    upstream_model: model.upstream_model,
    routable: true,
    custom_account_id: null,
  })));
}

/** Production Alias table: built-in/Custom rows, then definition-level dynamic rows. */
export function mergeProviderAliasRows(
  scopes: readonly ProviderScopeView[],
  accounts: readonly Account[],
  providers: readonly ProviderDefinitionView[],
): ProviderAliasRow[] {
  return [...providerAliasRows(scopes, accounts), ...dynamicProviderAliasRows(providers)];
}

/** Configuration inventory only; these counts do not predict request-time eligibility. */
export function aliasAccountCounts(row: ProviderAliasRow, accounts: readonly Account[]) {
  const matching = accounts.filter((account) => row.custom_account_id
    ? account.id === row.custom_account_id
    : account.provider_id === row.provider_id);
  return { total: matching.length, enabled: matching.filter((account) => account.enabled).length };
}

/** Flag cross-provider names that can be interpreted as another exact upstream ID. */
export function aliasNameOverlaps(row: ProviderAliasRow, rows: readonly ProviderAliasRow[]): boolean {
  return rows.some((other) => other.provider_id !== row.provider_id
    && other.upstream_model === row.public_model
    && other.public_model.toLocaleLowerCase() !== row.public_model.toLocaleLowerCase());
}
