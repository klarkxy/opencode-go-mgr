<template>
  <div class="aliases-page">
    <header class="aliases-header">
      <h1>{{ t("别名") }}</h1>
      <p>{{ t("只读汇总当前供应商合同与 Custom 账号映射。") }}</p>
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
      <p class="aliases-help">{{ t('此页展示模型与账号配置，不保证当前可调用；实际调用还取决于账号状态、名称冲突和上游服务。') }}</p>
      <n-input v-model:value="search" clearable :input-props="{ 'aria-label': t('搜索模型或供应商') }" :placeholder="t('搜索模型或供应商')" class="aliases-search" />
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
      <n-alert
        v-if="dynamicLoadError"
        type="warning"
        :title="t('加载供应商失败: {error}', { error: dynamicLoadError })"
      >
        <n-button size="small" secondary :loading="loading" @click="loadAliases({ retain: true })">
          {{ t("重试") }}
        </n-button>
      </n-alert>

      <n-empty v-if="aliasGroups.length === 0" :description="search.trim() ? t('无匹配模型') : t('暂无 Alias')" />
      <div v-else class="aliases-table-wrap" tabindex="0" role="region" :aria-label="t('模型映射')">
        <table class="aliases-table">
          <thead>
            <tr>
              <th>{{ t("对外模型名") }}</th>
              <th>{{ t("供应商 / 方案") }}</th>
              <th>{{ t("上游模型 ID") }}</th>
              <th>{{ t("配置状态") }}</th>
              <th>{{ t("账号配置") }}</th>
              <th>{{ t("操作") }}</th>
            </tr>
          </thead>
          <tbody v-for="group in aliasGroups" :key="group.public_model">
            <tr v-for="(row, index) in group.rows" :key="row.key">
              <td v-if="index === 0" :rowspan="group.rows.length" class="aliases-name">
                <code>{{ group.public_model }}</code>
              </td>
              <td>{{ row.provider_plan }}</td>
              <td><code>{{ row.upstream_model }}</code></td>
              <td>
                {{ row.routable ? t('已启用') : t('未启用') }}
                <p v-if="aliasNameOverlaps(row, aliasRows)" class="alias-warning">{{ t('名称与其他上游 ID 重叠，请检查调用名称。') }}</p>
              </td>
              <td>{{ accountConfiguration(row) }}</td>
              <td><a v-if="row.custom_account_id" :href="`?view=accounts&account_id=${encodeURIComponent(row.custom_account_id)}`">{{ t('编辑映射') }}</a></td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onMounted, ref } from "vue";
import { NAlert, NButton, NEmpty, NInput, NSpin } from "naive-ui";
import type { Account } from "../api/dashboard.ts";
import type {
  ProviderDefinitionView,
  ProviderCatalogEntry,
  ProviderContractsResponse,
} from "../api/providers.ts";
import { providerApi } from "../api/providers.ts";
import { isDynamicCatalogEntry } from "../domain/dynamic-provider.ts";
import { flattenProviderScopes, normalizeProviderContractsResponse } from "../domain/provider-contracts.ts";
import { aliasAccountCounts, aliasNameOverlaps, mergeProviderAliasRows, type ProviderAliasRow } from "../domain/provider-aliases.ts";
import { t } from "../i18n/index.ts";
import { useAccountsStore } from "../stores/accounts.ts";
import { useProvidersStore } from "../stores/providers.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";

const accountsStore = useAccountsStore();
const providersStore = useProvidersStore();
const contracts = ref<ProviderContractsResponse | null>(null);
const catalog = ref<ProviderCatalogEntry[] | null>(null);
const accounts = ref<Account[]>([]);
const dynamicProviders = ref<ProviderDefinitionView[]>([]);
const loading = ref(false);
const search = ref("");
const loadError = ref("");
const accountsLoadError = ref("");
const dynamicLoadError = ref("");
let activatedOnce = false;

const initialLoading = computed(() => loading.value && !contracts.value);
const aliasRows = computed(() => (
  contracts.value
    ? mergeProviderAliasRows(
      flattenProviderScopes(contracts.value, catalog.value),
      accounts.value,
      dynamicProviders.value,
    )
    : []
));
const aliasGroups = computed(() => {
  const groups = new Map<string, typeof aliasRows.value>();
  const query = search.value.trim().toLocaleLowerCase();
  for (const row of aliasRows.value) {
    if (query && ![row.public_model, row.upstream_model, row.provider_plan, row.custom_account ?? ""].some((value) => value.toLocaleLowerCase().includes(query))) continue;
    const key = row.public_model.toLocaleLowerCase();
    const existing = groups.get(key);
    if (existing) existing.push(row);
    else groups.set(key, [row]);
  }
  return [...groups.values()]
    .map((rows) => ({ public_model: rows[0]?.public_model ?? "", rows }))
    .sort((left, right) => left.public_model.localeCompare(right.public_model));
});

function accountConfiguration(row: ProviderAliasRow): string {
  if (accountsLoadError.value) return t('账号状态未知');
  const count = aliasAccountCounts(row, accounts.value);
  if (!count.total) return t('未添加账号');
  return count.enabled ? t('{count} 个启用账号', { count: count.enabled }) : t('无启用账号');
}

async function loadAliases(options: { retain?: boolean } = {}): Promise<void> {
  if (loading.value) return;
  loading.value = true;
  if (!options.retain) {
    loadError.value = "";
    dynamicLoadError.value = "";
  }
  try {
    const [contractsResult, catalogResult, accountsResult] = await Promise.allSettled([
      providersStore.loadContracts(),
      providersStore.loadCatalog(),
      accountsStore.loadPresented(),
    ]);
    if (catalogResult.status === "fulfilled") {
      catalog.value = catalogResult.value;
      const entries = catalogResult.value.filter(isDynamicCatalogEntry);
      if (entries.length === 0) {
        dynamicProviders.value = [];
        dynamicLoadError.value = "";
      } else {
        const details = await Promise.allSettled(
          entries.map((entry) => providerApi.getProviderDefinition(entry.provider_id)),
        );
        const previous = new Map(dynamicProviders.value.map((provider) => [provider.id, provider]));
        const next: ProviderDefinitionView[] = [];
        const failures: string[] = [];
        details.forEach((result, index) => {
          if (result.status === "fulfilled") {
            next.push(result.value);
            return;
          }
          failures.push(dashboardErrorDetail(result.reason));
          if (options.retain) {
            const kept = previous.get(entries[index]?.provider_id ?? "");
            if (kept) next.push(kept);
          }
        });
        dynamicProviders.value = next;
        dynamicLoadError.value = failures[0] ?? "";
      }
    }
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
.aliases-help {
  margin: 0 0 12px;
  color: var(--ocg-muted);
}
.aliases-search { margin-bottom: 16px; }
.alias-warning { color: var(--ocg-warning); margin: 4px 0 0; }
.aliases-table a { color: var(--ocg-primary); }
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
@media (max-width: 720px) {
  .aliases-table th:first-child,
  .aliases-name {
    position: sticky;
    left: 0;
    z-index: 1;
    background: var(--ocg-surface);
    max-width: 140px;
    overflow-wrap: anywhere;
    box-shadow: 1px 0 var(--ocg-border);
  }
}
</style>
