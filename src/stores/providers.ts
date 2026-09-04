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

  async function loadCatalog(): Promise<ProviderCatalogEntry[]> {
    const result = await providerApi.getProviderCatalog();
    catalog.value = result;
    return result;
  }

  async function loadContracts(): Promise<ProviderContractsResponse> {
    loading.value = true;
    try {
      const result = await providerApi.getProviderContracts();
      contracts.value = result;
      error.value = "";
      return result;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function refreshContractCatalog(
    scopeKind: ContractScopeKind,
    scopeId: string,
  ): Promise<ProviderContractsResponse> {
    const result = await providerApi.refreshContractCatalog(scopeKind, scopeId);
    contracts.value = result;
    return result;
  }

  async function resetStaticModelProtocols(
    scopeId: string,
  ): Promise<ProviderContractsResponse> {
    try {
      const result = await providerApi.resetStaticModelProtocols(scopeId);
      contracts.value = result;
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
      contracts.value = result;
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
