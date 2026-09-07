import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { dashboardApi } from "../api/dashboard.ts";
import type { Account } from "../api/dashboard.ts";

/**
 * Account list cache for pages that need `byId` after a load.
 * Mutations stay on `dashboardApi` in the views.
 */
export const useAccountsStore = defineStore("accounts", () => {
  const accounts = ref<Account[]>([]);
  const loaded = ref(false);
  const loading = ref(false);
  const error = ref("");

  const byId = computed(() => {
    const map = new Map<string, Account>();
    for (const account of accounts.value) map.set(account.id, account);
    return map;
  });

  // Overlapping loads resolve out of order; only the latest request commits
  // state. Stale calls still return/throw to their own caller unchanged.
  let loadGeneration = 0;

  async function loadPresented(): Promise<Account[]> {
    const generation = ++loadGeneration;
    loading.value = true;
    try {
      const list = await dashboardApi.getAccounts();
      if (generation !== loadGeneration) return list;
      accounts.value = list;
      loaded.value = true;
      error.value = "";
      return list;
    } catch (e) {
      if (generation === loadGeneration) {
        error.value = e instanceof Error ? e.message : String(e);
      }
      throw e;
    } finally {
      if (generation === loadGeneration) loading.value = false;
    }
  }

  return {
    accounts: computed(() => accounts.value),
    loaded: computed(() => loaded.value),
    loading: computed(() => loading.value),
    error: computed(() => error.value),
    byId,
    loadPresented,
  };
});
