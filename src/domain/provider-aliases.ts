import type { Account } from "../api/dashboard.ts";
import type { DynamicProviderView } from "../api/providers.ts";
import type { ProviderScopeView } from "./provider-contracts.ts";

export interface ProviderAliasRow {
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
  providers: readonly DynamicProviderView[],
): ProviderAliasRow[] {
  return providers.flatMap((provider) => provider.models.map((model) => ({
    key: `dynamic:${provider.id}:${model.public_model}:${model.upstream_model}`,
    public_model: model.public_model,
    provider_plan: provider.name,
    custom_account: null,
    upstream_model: model.upstream_model,
    routable: true,
    custom_account_id: null,
  })));
}
