import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const providers = readFileSync(new URL("./Providers.vue", import.meta.url), "utf8");
const matrix = readFileSync(new URL("../components/ProviderModelMatrix.vue", import.meta.url), "utf8");
const catalog = readFileSync(new URL("../components/PricingCatalog.vue", import.meta.url), "utf8");

test("Providers reads and mutates contracts through its store while explicit actions stay page-local", () => {
  assert.match(providers, /useProvidersStore\(\)/);
  assert.match(providers, /providersStore\.loadContracts\(\)/);
  assert.match(providers, /providersStore\.loadCatalog\(\)/);
  assert.match(providers, /providersStore\.putModelProtocolOverrides\(/);
  assert.match(providers, /providersStore\.refreshContractCatalog\(/);
  assert.match(providers, /providerApi\.runProtocolProbes\(/);
  assert.match(providers, /error\.status === 409/);
  assert.match(providers, /<ProviderModelMatrix/);
  assert.match(providers, /<PricingCatalog/);
  assert.doesNotMatch(providers, /ProviderProtocolSwitches/);
  assert.doesNotMatch(providers, /ProviderProbePanel/);
  assert.doesNotMatch(providers, /ProviderModelList/);
});

test("Providers excludes account-scoped Custom API contracts from its supplier navigation", () => {
  const scopes = providers.slice(
    providers.indexOf("const scopes = computed"),
    providers.indexOf("const activeSelection = computed"),
  );
  assert.match(scopes, /filter\(\(scope\) => scope\.scope_kind === "provider"\)/);
});

test("Providers exposes a read-only Alias tab from existing contracts and accounts", () => {
  assert.match(providers, /<n-tab-pane name="aliases"/);
  assert.match(providers, /providerAliasRows\(flattenProviderScopes\(contracts\.value, catalog\.value\), aliasAccounts\.value\)/);
  assert.match(providers, /dynamicProviderAliasRows\(dynamicDetails\.value\)/);
  assert.match(providers, /const aliasGroups = computed/);
  assert.match(providers, /v-for="group in aliasGroups"/);
  assert.match(providers, /dashboardApi\.getAccounts\(\)/);
  assert.match(providers, /public_model/);
  assert.match(providers, /upstream_model/);
  assert.match(providers, /openCustomAccount\(row\.custom_account_id\)/);
  const open = providers.slice(providers.indexOf("function openCustomAccount"), providers.indexOf("async function refreshCatalog"));
  assert.match(open, /applyAppViewSearchParams\(new URL\(window\.location\.href\), "accounts"\)/);
  assert.match(open, /searchParams\.set\("account_id", accountId\)/);
  assert.doesNotMatch(open, /dashboardApi\.(?:update|create|delete)/);
  assert.match(providers, /v-if="accountsLoadError"/);
  assert.match(providers, /accountsLoadError\.value = dashboardErrorDetail\(accountsResult\.reason\)/);
  assert.match(providers, /aliasAccounts\.value = accountsResult\.value;[\s\S]*?accountsLoadError\.value = ""/);
});

test("Providers keeps last-good contracts while actions fail and distinguishes page vs action errors", () => {
  assert.match(providers, /v-else-if="loadError && !contracts"/);
  assert.match(providers, /v-if="loadError && contracts"/);
  assert.match(providers, /catalogRefreshError/);
  assert.match(providers, /matrixError/);
  assert.match(providers, /probeError/);
  assert.match(providers, /await loadContracts\(\{ retain: true \}\)/);
  assert.match(providers, /aria-live="polite"/);
});

test("catalog refresh is scope-based, account-free, and adopts the returned contract", () => {
  const refresh = providers.slice(
    providers.indexOf("async function refreshCatalog"),
    providers.indexOf("type OverridePayload"),
  );
  assert.match(refresh, /refreshContractCatalog\(scope\.scope_kind, scope\.scope_id\)/);
  assert.match(refresh, /contracts\.value = normalizeProviderContractsResponse\(refreshed\)/);
  assert.match(refresh, /applyScopeFromQuery\(\)/);
  assert.doesNotMatch(refresh, /refreshProviderModels|refreshAccount|loadContracts/);
  assert.match(refresh, /message\.error\(t\("刷新模型目录失败: \{error\}"/);
  assert.match(refresh, /message\.success\(t\("已刷新模型目录"\)\)/);
  const adoptIdx = refresh.indexOf("contracts.value = normalizeProviderContractsResponse");
  const successIdx = refresh.indexOf('message.success(t("已刷新模型目录")');
  assert.ok(adoptIdx >= 0 && successIdx > adoptIdx);
  assert.doesNotMatch(refresh, /onMounted|onActivated|setInterval/);
});

test("Model catalog uses one content panel and one capability-driven refresh action", () => {
  assert.match(providers, /class="providers-catalog-head"/);
  assert.match(providers, /catalogRefreshSupported\(scope\)/);
  assert.match(providers, /v-if="catalogRefreshVisible"/);
  assert.match(providers, /t\("刷新模型目录"\)/);
  assert.doesNotMatch(providers, /refreshAccountId|refreshAccountOptions/);
  assert.doesNotMatch(providers, /选择用于刷新的账号|NFormItem|providers-overview/);
  assert.doesNotMatch(providers, /该供应商不支持刷新模型目录/);
});

test("override mutations render optimistically, serialize CAS writes, and remain conflict-aware", () => {
  const update = providers.slice(
    providers.indexOf("type OverridePayload"),
    providers.indexOf("async function runModelProbe"),
  );
  assert.match(update, /putModelProtocolOverrides\(/);
  assert.match(update, /overrides: ModelProtocolOverrideUpdate\[\]/);
  assert.match(update, /showOptimisticOverrides\(payload, sequence\)/);
  assert.match(update, /overrideQueue = overrideQueue\.then\(\(\) => persistOverrides\(payload, sequence\)\)/);
  assert.match(update, /settleOptimisticOverrides\(payload, sequence\)/);
  assert.match(update, /latestOverrideSequence\.get\(key\) !== sequence/);
  assert.match(update, /contracts\.value = normalizeProviderContractsResponse\(response\)/);
  assert.match(update, /error instanceof DashboardRequestError && error\.status === 409/);
  assert.match(update, /await loadContracts\(\{ retain: true \}\)/);
  assert.match(update, /message\.warning\(t\("供应商设置已在其他位置更新，已重新加载，请重试"\)\)/);
  assert.match(update, /message\.error\(t\("保存协议覆盖失败: \{error\}"/);
  assert.doesNotMatch(update, /onMounted|onActivated|setInterval/);
  assert.match(providers, /:action-locked="matrixActionLocked"/);
  const matrixLock = providers.slice(
    providers.indexOf("const matrixActionLocked"),
    providers.indexOf("const scopeMenuOptions"),
  );
  assert.doesNotMatch(matrixLock, /pendingOverrideKeys/);
});

test("row-level probes send all three protocols and merge the returned contract", () => {
  const probe = providers.slice(
    providers.indexOf("async function runModelProbe"),
    providers.indexOf("function onPopState"),
  );
  assert.match(probe, /protocols: \[\.\.\.PROVIDER_PROTOCOLS\]/);
  assert.match(probe, /runProtocolProbes\(scope\.provider_id/);
  assert.doesNotMatch(probe, /accountId/);
  assert.match(probe, /applyModelContractToResponse\(/);
  assert.match(probe, /await loadContracts\(\{ retain: true \}\)/);
  assert.match(probe, /probingModels\.value = new Set/);
  assert.match(probe, /probingModels\.value\.has\(payload\.modelId\)/);
  assert.match(probe, /response\.results\.filter\(\(result\) => !result\.success\)/);
  assert.match(probe, /message\.warning\(actionLive\.value\)/);
  assert.match(probe, /message\.success\(t\("探测完成"\)\)/);
  assert.match(probe, /message\.error\(t\("探测失败: \{error\}"/);
});

test("Providers presents per-protocol probe results above the matrix without a raw failure aggregate", () => {
  const summary = providers.slice(
    providers.indexOf('v-if="probeSummary"'),
    providers.indexOf("<ProviderModelMatrix"),
  );
  assert.match(summary, /probeResultStatus\(result\)/);
  assert.match(summary, /probeResultHttpStatus\(result\.error\)/);
  assert.match(summary, /probeResultMessage\(result\.error\)/);
  assert.match(summary, /probeResultUrl\(result\.error\)/);
  assert.match(providers, /\(\?:HTTP\\s\+\|returned\\s\+\)\(\\d\{3\}\)/);
  assert.match(providers, /raw\.indexOf\("\{"\)/);
  const failure = providers.slice(
    providers.indexOf("const failures = response.results.filter"),
    providers.indexOf('actionLive.value = t("探测完成")'),
  );
  assert.doesNotMatch(failure, /probeError\.value = failures/);
});

test("every provider with a dated static snapshot can expose the restore action", () => {
  assert.match(providers, /staticProtocolResetVisible/);
  assert.doesNotMatch(providers, /provider_id === "opencode"/);
  assert.match(providers, /static_protocol_snapshot_date/);
  assert.match(providers, /resetStaticModelProtocols/);
  assert.match(providers, /不会请求上游；将清除手动和探测判断/);
  assert.match(providers, /未出现的协议默认关闭/);
});

test("Providers pricing is filtered to the active provider and 390px layout does not require horizontal scrolling", () => {
  assert.match(providers, /<PricingCatalog :provider-id="activeScope\.provider_id" \/>/);
  assert.match(catalog, /buildScopedPlanPricingGroups\(props\.providerId/);
  assert.match(providers, /providers-mobile-nav/);
  assert.match(providers, /@media \(max-width: 390px\)/);
  assert.match(providers, /overflow-x: hidden/);
  assert.match(providers, /min-width: 0/);
  assert.match(providers, /@media \(max-width: 720px\)/);
});

test("Providers shows catalog, pricing, and the read-only Alias aggregate in three tabs", () => {
  assert.match(providers, /<n-tabs[^>]*v-model:value="activeTab"/);
  assert.match(providers, /<n-tab-pane[^>]*name="catalog"/);
  assert.match(providers, /<n-tab-pane[^>]*name="pricing"/);
  assert.match(providers, /<n-tab-pane[^>]*name="aliases"/);
  assert.match(providers, /:tab="t\('模型目录'\)"/);
  assert.match(providers, /:tab="t\('模型价格'\)"/);
  assert.match(providers, /:tab="t\('别名'\)"/);
  assert.doesNotMatch(providers, /id="provider-overview-title"/);
  assert.doesNotMatch(providers, /id="provider-protocol-title"/);
});

test("ProviderModelMatrix uses a scrollable table with sticky headers", () => {
  assert.match(matrix, /<table class="matrix-table">/);
  assert.match(matrix, /overflow-x: auto/);
  assert.match(matrix, /position:\s*sticky/);
  assert.match(matrix, /matrix-cell--protocol-header/);
});

test("ProviderModelMatrix binds one switch per model-protocol cell to the enabled state", () => {
  assert.match(matrix, /<n-switch/);
  assert.match(matrix, /:value="cellEnabled\(modelId, protocol\)"/);
  assert.match(matrix, /cellEvidence\(modelId, protocol\)\?\.enabled === true/);
  assert.match(matrix, /props\.optimisticOverrides\?\.get\(cellKey\(modelId, protocol\)\)/);
  assert.match(matrix, /:loading="cellSaving\(modelId, protocol\)"/);
  assert.match(matrix, /:disabled="props\.actionLocked \|\| rowProbing\(modelId\)"/);
  assert.match(matrix, /\.matrix-switch \{\s*--n-rail-color-active: var\(--ocg-success\)/);
});

test("ProviderModelMatrix scopes pending override state to the affected cells", () => {
  assert.match(matrix, /modelProtocolOverrideKey\(/);
  assert.match(matrix, /pendingOverrideKeys\?\.has\(cellKey\(modelId, protocol\)\)/);
  assert.match(matrix, /columnSaving\(protocol\)/);
  assert.doesNotMatch(matrix, /loading\?: boolean/);
});

test("ProviderModelMatrix renders and probes only current catalog models", () => {
  assert.match(matrix, /new Set\(props\.scope\.catalog\.models\)/);
  assert.doesNotMatch(matrix, /for \(const model of props\.scope\.models\) ids\.add/);
  assert.match(matrix, /overridesSaving\(\) \|\| rowProbing\(modelId\)/);
});

test("ProviderModelMatrix presents canonical aliases and derives an all-disabled provider state", () => {
  assert.match(matrix, /modelContract\(modelId\)\?\.alias\?\.trim\(\)/);
  assert.match(matrix, /modelAlias\(modelId\) \|\| modelId/);
  assert.match(matrix, /modelAlias\(modelId\) !== modelId/);
  assert.match(matrix, /const providerDisabled = computed/);
  assert.match(matrix, /matrixModels\.value\.length > 0/);
  assert.match(matrix, /cellEnabled\(modelId, protocol\)/);
  assert.match(matrix, /t\("全部供应商协议已关闭"\)/);
});

test("ProviderModelMatrix shows all provider protocols while limiting Custom to declared evidence", () => {
  assert.match(matrix, /v-for="protocol in matrixProtocols"/);
  assert.match(matrix, /scope\.scope_kind !== "custom_endpoint"/);
  assert.doesNotMatch(matrix, /scope\.provider_id !== "command-code"/);
  assert.match(matrix, /model\.protocols\[protocol\]\?\.available === true/);
  assert.doesNotMatch(matrix, /v-for="protocol in PROVIDER_PROTOCOLS"/);
});

test("ProviderModelMatrix emits force_on or force_off on switch toggle, never auto", () => {
  assert.match(matrix, /updateSingle\(modelId, protocol, on \? 'force_on' : 'force_off'\)/);
  assert.match(matrix, /emit\("update:overrides"/);
  const singleStates = matrix.match(/updateSingle\(modelId, protocol, [^)]+\)/g) ?? [];
  assert.ok(singleStates.length > 0);
  for (const call of singleStates) assert.ok(!call.includes("'auto'"), `unexpected auto state in ${call}`);
  assert.doesNotMatch(matrix, /canForceOn/);
});

test("ProviderModelMatrix batch actions set whole columns on or off", () => {
  assert.match(matrix, /makeOverrides\(/);
  assert.match(matrix, /applyColumnBatch\(/);
  assert.match(matrix, /columnBatchOptions/);
  assert.match(matrix, /\{ key: "force_on", label: t\("全部开启"\) \}/);
  assert.match(matrix, /\{ key: "force_off", label: t\("全部关闭"\) \}/);
  assert.doesNotMatch(matrix, /rowBatchOptions/);
  assert.doesNotMatch(matrix, /applyRowBatch/);
  assert.doesNotMatch(matrix, /\{ key: "auto"/);
});

test("ProviderModelMatrix renders probe controls only when the Provider supports probes", () => {
  assert.match(matrix, /ReloadOutlined/);
  assert.match(matrix, /:aria-label="t\('测试'\)"/);
  assert.match(matrix, /:loading="rowProbing\(modelId\)"/);
  assert.match(matrix, /const probeSupported = computed\(\(\) => props\.scope\.card\.protocol_probe\)/);
  assert.match(matrix, /v-if="probeSupported"/);
  assert.match(matrix, /if \(!probeSupported\.value\) return/);
  assert.match(matrix, /n-popconfirm/);
  assert.match(matrix, /runRowProbe/);
  assert.match(matrix, /emit\("probe", \{ modelId \}\)/);
  assert.doesNotMatch(matrix, /probeAccounts|accountId/);
});

test("ProviderModelMatrix has no expand rows, dots, dropdown editors, or hint remnants", () => {
  assert.doesNotMatch(matrix, /expandedModel/);
  assert.doesNotMatch(matrix, /toggleRow/);
  assert.doesNotMatch(matrix, /status-dot/);
  assert.doesNotMatch(matrix, /overrideOptions/);
  assert.doesNotMatch(matrix, /overrideStateLabel/);
  assert.doesNotMatch(matrix, /rowHintNeeded/);
  assert.doesNotMatch(matrix, /cellHintNeeded/);
  assert.doesNotMatch(matrix, /status-label/);
  assert.doesNotMatch(matrix, /override-badge/);
  assert.doesNotMatch(matrix, /n-radio-group/);
  assert.doesNotMatch(matrix, /无可用证据/);
});
