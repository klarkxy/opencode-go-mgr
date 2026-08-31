import type { Account } from "../api/dashboard.ts";
import type {
  ProviderCatalogEntry,
  ProviderContractsResponse,
  ProviderProtocol,
} from "../api/providers.ts";
import {
  findAccountScopeView,
  flattenProviderScopes,
  normalizeProviderContractsResponse,
} from "./provider-contracts.ts";

export interface AccountTestModel {
  modelId: string;
  alias: string;
  protocol: ProviderProtocol;
}

/** Resolve only the current account's routable models from the loaded contract snapshot. */
export function accountTestModels(
  account: Pick<Account, "id" | "provider_id" | "offering_id">,
  response: ProviderContractsResponse | null | undefined,
  catalog: readonly ProviderCatalogEntry[] | null | undefined = null,
): AccountTestModel[] {
  if (!response) return [];
  const scopes = flattenProviderScopes(normalizeProviderContractsResponse(response), catalog);
  const scope = findAccountScopeView(scopes, account);
  if (!scope) return [];

  const seen = new Set<string>();
  return scope.models
    .filter((model) => model.routable)
    .flatMap((model) => {
      const modelId = model.model_id.trim();
      const identity = modelId.toLowerCase();
      if (!modelId || seen.has(identity)) return [];
      seen.add(identity);
      return [{
        modelId,
        alias: model.alias.trim(),
        protocol: model.preferred_protocol,
      }];
    })
    .sort((left, right) => left.modelId.localeCompare(right.modelId, undefined, {
      numeric: true,
      sensitivity: "base",
    }));
}

export function filterAccountTestModels(
  models: readonly AccountTestModel[],
  query: string,
): AccountTestModel[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [...models];
  return models.filter((model) => (
    model.modelId.toLowerCase().includes(needle)
    || model.alias.toLowerCase().includes(needle)
  ));
}
