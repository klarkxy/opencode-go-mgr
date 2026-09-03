import type { Account } from "../api/dashboard.ts";
import type {
  ProviderCatalogEntry,
  ProviderContractsResponse,
  ProviderProtocol,
} from "../api/providers.ts";
import { isDynamicCatalogEntry } from "./dynamic-provider.ts";
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

/** Resolve the current account's routable models from its contract scope, or the dynamic catalog. */
export function accountTestModels(
  account: Pick<Account, "id" | "provider_id">,
  response: ProviderContractsResponse | null | undefined,
  catalog: readonly ProviderCatalogEntry[] | null | undefined = null,
): AccountTestModel[] {
  const seen = new Set<string>();
  const models = exactContractTestModels(account, response, catalog, seen)
    ?? dynamicCatalogTestModels(account, catalog, seen);
  return models.sort((left, right) => left.modelId.localeCompare(right.modelId, undefined, {
    numeric: true,
    sensitivity: "base",
  }));
}

function exactContractTestModels(
  account: Pick<Account, "id" | "provider_id">,
  response: ProviderContractsResponse | null | undefined,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
  seen: Set<string>,
): AccountTestModel[] | null {
  if (!response) return null;
  const scopes = flattenProviderScopes(normalizeProviderContractsResponse(response), catalog);
  const scope = findAccountScopeView(scopes, account);
  if (!scope) return null;
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
    });
}

function dynamicCatalogTestModels(
  account: Pick<Account, "id" | "provider_id">,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
  seen: Set<string>,
): AccountTestModel[] {
  const entry = catalog?.find((candidate) => (
    candidate.provider_id === account.provider_id && isDynamicCatalogEntry(candidate)
  ));
  if (!entry || entry.upstream_protocols.length !== 1) return [];
  const protocol = entry.upstream_protocols[0];
  return entry.model_aliases.flatMap((alias) => {
    const modelId = alias.trim();
    const identity = modelId.toLowerCase();
    if (!modelId || seen.has(identity)) return [];
    seen.add(identity);
    return [{ modelId, alias: modelId, protocol }];
  });
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
