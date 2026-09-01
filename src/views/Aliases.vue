<template>
  <div class="aliases-page">
    <header class="aliases-header">
      <h1>{{ t("别名") }}</h1>
      <p>{{ t("只读汇总当前供应商合同与 Custom 账号映射；点击编辑 Custom 可直接打开对应账号。") }}</p>
    </header>

    <div
      v-if="initialLoading"
      class="aliases-state"
      role="status"
      aria-live="polite"
      :aria-label="t('加载中…')"
    >
      <n-spin size="small" />
    </div>

    <n-alert
      v-else-if="loadError && !contracts"
      type="error"
      :title="t('加载供应商失败: {error}', { error: loadError })"
    >
      <n-button size="small" secondary :loading="loading" @click="loadAliases()">
        {{ t("重试") }}
      </n-button>
    </n-alert>

    <section v-else class="aliases-section" aria-labelledby="alias-table-title">
      <h2 id="alias-table-title" class="sr-only">{{ t("别名") }}</h2>
      <n-alert
        v-if="loadError && contracts"
        type="warning"
        :title="t('加载供应商失败: {error}', { error: loadError })"
      >
        <n-button size="small" secondary :loading="loading" @click="loadAliases({ retain: true })">
          {{ t("重试") }}
        </n-button>
      </n-alert>
      <n-alert
        v-if="accountsLoadError"
        type="warning"
        :title="t('加载 Custom Alias 账号失败: {error}', { error: accountsLoadError })"
      >
        <n-button size="small" secondary :loading="loading" @click="loadAliases({ retain: true })">
          {{ t("重试") }}
        </n-button>
      </n-alert>

      <n-empty v-if="aliasGroups.length === 0" :description="t('暂无 Alias')" />
      <div v-else class="aliases-table-wrap">
        <table class="aliases-table">
          <thead>
            <tr>
              <th>{{ t("对外模型名") }}</th>
              <th>{{ t("供应商 / 方案") }}</th>
              <th>{{ t("Custom 账号") }}</th>
              <th>{{ t("上游模型 ID") }}</th>
              <th>{{ t("可路由") }}</th>
              <th>{{ t("操作") }}</th>
            </tr>
          </thead>
          <tbody v-for="group in aliasGroups" :key="group.public_model">
            <tr v-for="(row, index) in group.rows" :key="row.key">
              <td v-if="index === 0" :rowspan="group.rows.length" class="aliases-name">
                <code>{{ group.public_model }}</code>
              </td>
              <td>{{ row.provider_plan }}</td>
              <td>{{ row.custom_account ?? '—' }}</td>
              <td><code>{{ row.upstream_model }}</code></td>
              <td>{{ row.routable ? t("可用") : t("不可用") }}</td>
              <td>
                <n-button
                  v-if="row.custom_account_id"
                  size="small"
                  tertiary
                  @click="openCustomAccount(row.custom_account_id)"
                >{{ t("编辑 Custom") }}</n-button>
                <span v-else>—</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onMounted, ref } from "vue";
import { NAlert, NButton, NEmpty, NSpin } from "naive-ui";
import type { Account } from "../api/dashboard.ts";
import type { ProviderCatalogEntry, ProviderContractsResponse } from "../api/providers.ts";
import { flattenProviderScopes, normalizeProviderContractsResponse } from "../domain/provider-contracts.ts";
import { providerAliasRows } from "../domain/provider-aliases.ts";
import { t } from "../i18n/index.ts";
import { useAccountsStore } from "../stores/accounts.ts";
import { useProvidersStore } from "../stores/providers.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";
import { applyAccountViewSearchParams } from "./app-navigation.ts";

const accountsStore = useAccountsStore();
const providersStore = useProvidersStore();
const contracts = ref<ProviderContractsResponse | null>(null);
const catalog = ref<ProviderCatalogEntry[] | null>(null);
const accounts = ref<Account[]>([]);
const loading = ref(false);
const loadError = ref("");
const accountsLoadError = ref("");
let activatedOnce = false;

const initialLoading = computed(() => loading.value && !contracts.value);
const aliasRows = computed(() => (
  contracts.value
    ? providerAliasRows(flattenProviderScopes(contracts.value, catalog.value), accounts.value)
    : []
));
const aliasGroups = computed(() => {
  const groups = new Map<string, typeof aliasRows.value>();
  for (const row of aliasRows.value) {
    const key = row.public_model.toLocaleLowerCase();
    const existing = groups.get(key);
    if (existing) existing.push(row);
    else groups.set(key, [row]);
  }
  return [...groups.values()]
    .map((rows) => ({ public_model: rows[0]?.public_model ?? "", rows }))
    .sort((left, right) => left.public_model.localeCompare(right.public_model));
});

async function loadAliases(options: { retain?: boolean } = {}): Promise<void> {
  if (loading.value) return;
  loading.value = true;
  if (!options.retain) loadError.value = "";
  try {
    const [contractsResult, catalogResult, accountsResult] = await Promise.allSettled([
      providersStore.loadContracts(),
      providersStore.loadCatalog(),
      accountsStore.loadPresented(),
    ]);
    if (catalogResult.status === "fulfilled") catalog.value = catalogResult.value;
    if (accountsResult.status === "fulfilled") {
      accounts.value = accountsResult.value;
      accountsLoadError.value = "";
    } else {
      accountsLoadError.value = dashboardErrorDetail(accountsResult.reason);
    }
    if (contractsResult.status === "fulfilled") {
      contracts.value = normalizeProviderContractsResponse(contractsResult.value);
      loadError.value = "";
    } else {
      loadError.value = dashboardErrorDetail(contractsResult.reason);
    }
  } finally {
    loading.value = false;
  }
}

function openCustomAccount(accountId: string): void {
  const url = applyAccountViewSearchParams(new URL(window.location.href), accountId);
  window.history.pushState(null, "", url);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

onMounted(() => void loadAliases());
onActivated(() => {
  if (activatedOnce) void loadAliases({ retain: true });
  else activatedOnce = true;
});
</script>

<style scoped>
.aliases-page {
  min-width: 0;
  max-width: 1440px;
  margin: 0 auto;
  overflow-x: hidden;
}
.aliases-header {
  margin-bottom: 16px;
}
.aliases-header h1 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-xl)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.aliases-header p {
  margin: 4px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}
.aliases-state {
  min-height: 160px;
  display: grid;
  place-items: center;
}
.aliases-section {
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.aliases-section > .n-alert {
  margin-bottom: 12px;
}
.aliases-table-wrap {
  overflow-x: auto;
}
.aliases-table {
  width: 100%;
  min-width: 760px;
  border-collapse: collapse;
  font-size: var(--ocg-font-sm);
}
.aliases-table th,
.aliases-table td {
  padding: 10px 12px;
  border-bottom: 1px solid var(--ocg-border);
  text-align: left;
  vertical-align: middle;
}
.aliases-table th {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
  font-weight: 600;
}
.aliases-table .aliases-name {
  vertical-align: top;
}
</style>
