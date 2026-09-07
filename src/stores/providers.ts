import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { isRevisionConflict } from "../api/dashboard.ts";
import { providerApi } from "../api/providers.ts";
import type {
  ContractScopeKind,
  ModelProtocolOverrideUpdate,
  ProviderCatalogEntry,
  ProviderContractsResponse,
} from "../api/providers.ts";

/**
 * Provider catalog and contract fetches used by Providers and Aliases.
 * Probe progress and pricing refresh stay page-local.
 */
export const useProvidersStore = defineStore("providers", () => {
  const catalog = ref<ProviderCatalogEntry[] | null>(null);
  const contracts = ref<ProviderContractsResponse | null>(null);
  const loading = ref(false);
  const error = ref("");

  // Overlapping loads resolve out of order; only the latest request commits
  // state. Mutation responses bump the contracts generation so a slow pending
  // load cannot clobber fresher post-mutation state. Stale calls still
  // return/throw to their own caller unchanged.
  let catalogGeneration = 0;
  let contractsGeneration = 0;

  async function loadCatalog(): Promise<ProviderCatalogEntry[]> {
    const generation = ++catalogGeneration;
    const result = await providerApi.getProviderCatalog();
    if (generation !== catalogGeneration) return result;
    catalog.value = result;
    return result;
  }

  async function loadContracts(): Promise<ProviderContractsResponse> {
    const generation = ++contractsGeneration;
    loading.value = true;
    try {
      const result = await providerApi.getProviderContracts();
      if (generation !== contractsGeneration) return result;
      contracts.value = result;
      error.value = "";
      return result;
    } catch (e) {
      if (generation === contractsGeneration) {
        error.value = e instanceof Error ? e.message : String(e);
      }
      throw e;
    } finally {
      if (generation === contractsGeneration) loading.value = false;
    }
  }

  async function refreshContractCatalog(
    scopeKind: ContractScopeKind,
    scopeId: string,
  ): Promise<ProviderContractsResponse> {
    const result = await providerApi.refreshContractCatalog(scopeKind, scopeId);
    contractsGeneration += 1;
    contracts.value = result;
    // Release the flag of any superseded in-flight `loadContracts`.
    loading.value = false;
    return result;
  }

  async function resetStaticModelProtocols(
    scopeId: string,
  ): Promise<ProviderContractsResponse> {
    try {
      const result = await providerApi.resetStaticModelProtocols(scopeId);
      contractsGeneration += 1;
      contracts.value = result;
      loading.value = false;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadContracts();
      throw cause;
    }
  }

  async function putModelProtocolOverrides(
    scopeKind: ContractScopeKind,
    scopeId: string,
    overrides: ModelProtocolOverrideUpdate[],
  ): Promise<ProviderContractsResponse> {
    try {
      const result = await providerApi.updateModelProtocolOverrides(scopeKind, scopeId, overrides);
      contractsGeneration += 1;
      contracts.value = result;
      loading.value = false;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadContracts();
      throw cause;
    }
  }

  return {
    catalog: computed(() => catalog.value),
    contracts: computed(() => contracts.value),
    loading: computed(() => loading.value),
    error: computed(() => error.value),
    loadCatalog,
    loadContracts,
    refreshContractCatalog,
    resetStaticModelProtocols,
    putModelProtocolOverrides,
  };
});
