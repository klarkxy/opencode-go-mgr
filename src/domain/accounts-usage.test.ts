import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  isCooling,
  isFreeCooling,
  isUsageLimitReached,
  mergeUsageEdit,
  normalizeUsagePercent,
  resetTimeForWindow,
  resetsFieldsToMinutes,
  resetsFirstFieldMax,
  resetsFirstFieldValue,
  resetsInMinutesForSave,
  resetsSecondFieldMax,
  resetsSecondFieldValue,
  usagePercentFromCost,
  usageProgressPercentage,
  usageProgressStatus,
} from "./accounts-usage.ts";
import type { UsageEditState, UsageKey } from "./accounts-usage.ts";
import { mapWithConcurrency } from "../utils/async.ts";

test("fills every active 5-hour, weekly, or monthly limit", () => {
  const cases: Array<[UsageKey, "cooldown_5h_until" | "cooldown_week_until" | "cooldown_month_until"]> = [
    ["window_5h", "cooldown_5h_until"],
    ["window_week", "cooldown_week_until"],
    ["window_month", "cooldown_month_until"],
  ];

  for (const [key, field] of cases) {
    assert.equal(
      isUsageLimitReached({
        cooldown_5h_until: field === "cooldown_5h_until" ? "2099-01-01T00:00:00Z" : null,
        cooldown_week_until: field === "cooldown_week_until" ? "2099-01-01T00:00:00Z" : null,
        cooldown_month_until: field === "cooldown_month_until" ? "2099-01-01T00:00:00Z" : null,
      }, key),
      true,
    );
  }
  assert.equal(
    isUsageLimitReached(
      {
        cooldown_5h_until: null,
        cooldown_week_until: "2099-01-01T00:00:00Z",
        cooldown_month_until: null,
      },
      "window_month",
    ),
    false,
  );
  assert.equal(
    isUsageLimitReached(
      {
        cooldown_5h_until: null,
        cooldown_week_until: "2000-01-01T00:00:00Z",
        cooldown_month_until: null,
      },
      "window_week",
    ),
    false,
  );
});


test("treats free promo cooldown as cooling without Go usage windows", () => {
  assert.equal(isFreeCooling({
    cooldown_free_until: "2099-01-01T00:00:00Z",
  }), true);
  assert.equal(isFreeCooling({
    cooldown_free_until: null,
  }), false);
  assert.equal(isCooling({
    cooldown_until: null,
    cooldown_5h_until: null,
    cooldown_week_until: null,
    cooldown_month_until: null,
    cooldown_free_until: "2099-01-01T00:00:00Z",
  }), true);
  assert.equal(isCooling({
    cooldown_until: null,
    cooldown_5h_until: null,
    cooldown_week_until: null,
    cooldown_month_until: null,
    cooldown_free_until: null,
  }), false);
});

test("keeps generic and overlapping window cooldowns visible", () => {
  assert.equal(isCooling({
    cooldown_until: "2099-01-01T00:00:00Z",
    cooldown_5h_until: null,
    cooldown_week_until: null,
    cooldown_month_until: null,
    cooldown_free_until: null,
  }), true);

  const overlapping = {
    cooldown_5h_until: "2099-01-01T00:00:00Z",
    cooldown_week_until: "2099-01-02T00:00:00Z",
    cooldown_month_until: null,
  };
  assert.equal(isUsageLimitReached(overlapping, "window_5h"), true);
  assert.equal(isUsageLimitReached(overlapping, "window_week"), true);
});

test("shows local estimated saturation as a warning, not a real breaker", () => {
  const available = {
    cooldown_5h_until: null,
    cooldown_week_until: null,
    cooldown_month_until: null,
  };
  const realWeeklyBreaker = {
    cooldown_5h_until: null,
    cooldown_week_until: "2099-01-01T00:00:00Z",
    cooldown_month_until: null,
  };

  assert.equal(
    usageProgressStatus(
      available,
      "window_week",
      100,
    ),
    "warning",
  );
  assert.equal(
    usageProgressStatus(
      realWeeklyBreaker,
      "window_week",
      0,
    ),
    "error",
  );
  assert.equal(usageProgressPercentage(available, "window_week", 100), 100);
  assert.equal(usageProgressPercentage(realWeeklyBreaker, "window_week", 0), 100);
});

test("shows a live reset countdown below a quota progress bar during cooldown", async () => {
  // The strip lives in UsageStrip.vue with its own 1s clock so a tick only
  // re-renders the strip instead of the whole account card list.
  const source = await readFile(new URL("../components/UsageStrip.vue", import.meta.url), "utf8");
  const progress = source.indexOf(':percentage="usageProgressPercentage(');
  const countdown = source.indexOf('class="usage-reset-countdown"');

  assert.ok(progress >= 0);
  assert.ok(countdown > progress);
  assert.match(source, /v-if="isUsageLimitReached\(account, limit\.key, now\)"[\s\S]*class="usage-reset-countdown"[\s\S]*formatWindowRemaining\(limit\.key\)/);
  assert.match(source, /\.usage-reset-countdown \{[\s\S]*color: var\(--ocg-error\);/);
  assert.doesNotMatch(source, /\.usage-reset-countdown \{[\s\S]*?min-height:/);
});

test("shows a distinct upstream account breaker instead of disguising it as cooldown", async () => {
  const card = await readFile(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  const display = await readFile(new URL("./account-display.ts", import.meta.url), "utf8");
  assert.match(card, /account\.auth_error \|\| isCooling\(account, now\)/);
  assert.match(display, /account\.enabled[\s\S]*t\("不可用"\)[\s\S]*t\("已禁用"\)/);
  assert.match(display, /if \(account\.auth_error\) return "error"/);
});

test("maps each usage window to its cooldown reset deadline", () => {
  const account = {
    cooldown_5h_until: "2026-07-20T01:00:00Z",
    cooldown_week_until: "2026-07-21T01:00:00Z",
    cooldown_month_until: null,
  };
  assert.equal(resetTimeForWindow(account, "window_5h"), account.cooldown_5h_until);
  assert.equal(resetTimeForWindow(account, "window_week"), account.cooldown_week_until);
  assert.equal(resetTimeForWindow(account, "window_month"), null);
});

test("keeps account cards compact with metadata tags and popover calibration", async () => {
  const accounts = await readFile(new URL("../views/Accounts.vue", import.meta.url), "utf8");
  const card = await readFile(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  const editor = await readFile(new URL("../components/AccountUsageEditor.vue", import.meta.url), "utf8");
  const usage = await readFile(new URL("./useAccountUsage.ts", import.meta.url), "utf8");
  const display = await readFile(new URL("./account-display.ts", import.meta.url), "utf8");
  const strip = await readFile(new URL("../components/UsageStrip.vue", import.meta.url), "utf8");
  const header = card.slice(
    card.indexOf("<template #header>"),
    card.indexOf('<div v-if="!accountIsReady(account)"'),
  );
  const stripBody = strip.slice(
    strip.indexOf('class="usage-strip-body" role="group"'),
    strip.indexOf("</template>"),
  );

  assert.ok(header.indexOf("accountStatusLabel(account, now)") < header.indexOf('v-if="hasValidityPeriod"'));
  // Subscription dates collapse into one clickable status tag whose popover
  // supports a selected date or a one-click update to today.
  assert.match(header, /<n-popover[\s\S]*?v-if="hasValidityPeriod"[\s\S]*?trigger="click"/);
  assert.match(header, /accountExpiryLabel\(account, now\) \}\} ·/);
  assert.match(header, /t\("到期于 \{date\}"/);
  assert.match(header, /:aria-label="`\$\{accountExpiryLabel/);
  assert.match(header, /<n-date-picker[\s\S]*?v-model:formatted-value="purchaseDateDraft"/);
  assert.match(header, /@click="commitPurchaseDate\(today\)"/);
  assert.doesNotMatch(header, /<n-tag v-if="isGo && accountIsReady\(account\)"/);
  assert.match(card, /<n-popover[\s\S]*?trigger="click"[\s\S]*?placement="bottom-end"[\s\S]*?:width="320"[\s\S]*?@update:show="\(show: boolean\) => show && emit\('usage-editor-open'\)"/);
  assert.match(editor, /class="usage-editor-popover"/);
  assert.doesNotMatch(card, /:flip="false"/);
  assert.ok(card.indexOf("@update:value=\"emit('toggle')\"") < card.indexOf('placement="bottom-end"'));
  assert.ok(card.indexOf("<n-popover") < card.indexOf("<n-dropdown"));
  assert.match(editor, /class="usage-editor-popover"[\s\S]*?class="usage-resets-row"/);
  assert.match(usage, /async function focusUsageEditor\(accountId: string\)[\s\S]*?requestAnimationFrame[\s\S]*?\.n-input-number input[\s\S]*?\.focus\(\)/);
  assert.match(card, /v-if="\(isGo \|\| isOfficialCn\) && accountIsReady\(account\)"[\s\S]*?刷新额度/);
  assert.doesNotMatch(
    card,
    /accountIsReady\(account\) && account\.account_type === 'managed'/,
  );
  assert.match(usage, /async function refreshAccountUsage/);
  assert.match(usage, /dashboardApi\.refreshAccountUsage/);
  assert.match(usage, /额度已从 OpenCode 官方用量刷新/);
  assert.doesNotMatch(usage, /refreshManagedUsage|refreshManagedAccountUsage|额度已从 OpenCode 控制台刷新/);
  assert.match(card, /:aria-label="t\('校准用量'\)"/);
  assert.doesNotMatch(card, /根据 OCG 内已定价请求估算/);
  assert.match(
    card,
    /manualUsageCalibration && accountIsReady\(account\) && edits"[\s\S]*?class="account-action account-action--secondary"/,
  );
  assert.doesNotMatch(card, /account-action--edit/);
  assert.doesNotMatch(stripBody, /usage-strip-title|\{\{ t\("用量"\) \}\}/);
  assert.match(stripBody, /class="usage-strip-body" role="group" :aria-label="t\('用量'\)"/);
  assert.match(stripBody, /<n-progress[\s\S]*?:percentage="usageProgressPercentage\(/);
  assert.doesNotMatch(stripBody, /<n-input-number|<n-slider|class="usage-resets-row"/);
  assert.match(
    strip,
    /\.usage-strip\s*\{\s*min-width:\s*0;\s*\}\s*\.usage-strip-body\s*\{[\s\S]*?grid-template-columns:\s*repeat\(3,\s*minmax\(0,\s*1fr\)\)/,
  );
  assert.match(strip, /@media \(max-width: 900px\) \{\s*\.usage-strip-body\s*\{\s*grid-template-columns: 1fr;/);
  assert.doesNotMatch(card, /class="account-lifecycle"|\.account-lifecycle\s*\{/);
  assert.match(display, /key: "edit", label: t\("编辑账号"\)/);
  assert.match(accounts, /v-if="quotaLimitsError"[\s\S]*?@click="retryQuotaLimits"/);
  assert.equal(accounts.match(/v-if="quotaLimitsError"/g)?.length, 1);
});

test("normalizes manually entered percentages to the supported range and precision", () => {
  assert.equal(normalizeUsagePercent(-1), 0);
  assert.equal(normalizeUsagePercent(42.56), 42.6);
  assert.equal(normalizeUsagePercent(101), 100);
  assert.equal(usagePercentFromCost(6, 12), 50);
});

test("accounts page surfaces official sync last-success and retry state beyond button loading", async () => {
  const card = await readFile(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  const usage = await readFile(new URL("./useAccountUsage.ts", import.meta.url), "utf8");
  const display = await readFile(new URL("./account-display.ts", import.meta.url), "utf8");
  assert.match(usage, /usage_sync_last_success_at/);
  assert.match(usage, /usage_sync_next_allowed_at/);
  assert.match(display, /isUsageRefreshBlocked/);
  assert.match(display, /usageSyncCaption/);
  assert.match(card, /class="usage-sync-meta"/);
  assert.match(usage, /error\.status === 429/);
  assert.match(usage, /请稍后再试（约 \{seconds\} 秒）/);
  assert.match(display, /上次官方同步: \{time\}/);
  assert.match(display, /尚未官方同步/);
  assert.match(display, /刷新额度冷却中，请于 \{time\} 后重试/);
  assert.match(card, /:disabled="isUsageRefreshBlocked\(account, now\)/);
});

test("usage refresh preserves dirty drafts unless a real 429 reset that window", () => {
  const dirty: UsageEditState = {
    draft: 75,
    saved: 20,
    saving: false,
    error: "save failed",
    resets_in_minutes_draft: 240,
    resets_at_saved: "2099-01-01T00:00:00Z",
    resets_dirty: true,
  };

  assert.deepEqual(mergeUsageEdit(dirty, 35, false), {
    draft: 75,
    saved: 35,
    saving: false,
    error: "save failed",
    resets_in_minutes_draft: 240,
    resets_at_saved: "2099-01-01T00:00:00Z",
    resets_dirty: true,
  });
  assert.deepEqual(mergeUsageEdit(dirty, 0, true), {
    draft: 0,
    saved: 0,
    saving: false,
    error: null,
    resets_in_minutes_draft: 240,
    resets_at_saved: "2099-01-01T00:00:00Z",
    resets_dirty: true,
  });
  assert.deepEqual(mergeUsageEdit(undefined, 35, false), {
    draft: 35,
    saved: 35,
    saving: false,
    error: null,
    resets_in_minutes_draft: null,
    resets_at_saved: null,
    resets_dirty: false,
  });
});

test("percent-only usage saves keep counting down from the backend deadline", () => {
  const resetAt = "2026-07-19T12:05:30Z";
  const clean: UsageEditState = {
    draft: 50,
    saved: 40,
    saving: false,
    error: null,
    resets_in_minutes_draft: 6,
    resets_at_saved: resetAt,
    resets_dirty: false,
  };

  assert.equal(
    resetsInMinutesForSave(clean, "window_5h", Date.parse("2026-07-19T12:00:00Z")),
    5,
  );
  assert.equal(
    resetsInMinutesForSave(clean, "window_5h", Date.parse("2026-07-19T12:02:00Z")),
    3,
  );
  assert.equal(
    resetsInMinutesForSave({ ...clean, resets_in_minutes_draft: 240, resets_dirty: true }, "window_5h"),
    240,
  );
  assert.equal(
    resetsInMinutesForSave(clean, "window_5h", Date.parse("2026-07-19T12:05:00Z")),
    1,
  );
  assert.equal(
    resetsInMinutesForSave(clean, "window_5h", Date.parse("2026-07-19T12:06:00Z")),
    300,
  );
  assert.equal(
    resetsInMinutesForSave({ ...clean, resets_at_saved: "invalid" }, "window_5h"),
    300,
  );
  assert.equal(resetsInMinutesForSave(clean, "window_month"), null);
});

test("reset editor derives untouched fields from the live absolute deadline", async () => {
  const helpers = await readFile(new URL("./accounts-usage.ts", import.meta.url), "utf8");
  const fields = helpers.slice(
    helpers.indexOf("export function resetsFirstFieldValue"),
    helpers.indexOf("export function resetsFieldsToMinutes"),
  );

  assert.equal(fields.match(/resetsInMinutesForSave\(edit, key, now\)/g)?.length, 2);
});

test("reset editor splits minutes into hour/minute or day/hour field pairs", () => {
  assert.equal(resetsFirstFieldMax("window_5h"), 5);
  assert.equal(resetsSecondFieldMax("window_5h"), 59);
  assert.equal(resetsFirstFieldMax("window_week"), 7);
  assert.equal(resetsSecondFieldMax("window_week"), 23);
  assert.equal(resetsFirstFieldMax("window_month"), 0);
  assert.equal(resetsSecondFieldMax("window_month"), 0);

  assert.equal(resetsFieldsToMinutes(1, 30, "window_5h"), 90);
  assert.equal(resetsFieldsToMinutes(1, 2, "window_week"), 1 * 24 * 60 + 2 * 60);
  assert.equal(resetsFieldsToMinutes(3, 4, "window_month"), 0);

  const dirty = { resets_in_minutes_draft: 90, resets_at_saved: null, resets_dirty: true };
  assert.equal(resetsFirstFieldValue(dirty, "window_5h"), 1);
  assert.equal(resetsSecondFieldValue(dirty, "window_5h"), 30);
  const weekly = { ...dirty, resets_in_minutes_draft: 1 * 24 * 60 + 2 * 60 };
  assert.equal(resetsFirstFieldValue(weekly, "window_week"), 1);
  assert.equal(resetsSecondFieldValue(weekly, "window_week"), 2);
  assert.equal(resetsFirstFieldValue(undefined, "window_5h"), 0);
  assert.equal(resetsSecondFieldValue(undefined, "window_week"), 0);
  assert.equal(resetsFirstFieldValue(dirty, "window_month"), 0);
});

test("calibration shortcut is disabled when every usage window is cooling", async () => {
  const card = await readFile(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  const usage = await readFile(new URL("./useAccountUsage.ts", import.meta.url), "utf8");

  assert.match(card, /:disabled="!usageEditorAvailable"/);
  assert.match(usage, /usageLoading\.value\[account\.id\] \|\| usageLoadErrors\.value\[account\.id\]/);
  assert.match(usage, /usageLimitsFor\(account\)\.some\(\(\{ key \}\) => !accountUsageLimitReached\(account, key\)\)/);
});

test("usage refresh initializes windows missing after an earlier quota load failure", async () => {
  const usage = await readFile(new URL("./useAccountUsage.ts", import.meta.url), "utf8");
  const sync = usage.slice(usage.indexOf("function syncUsageEdits"), usage.indexOf("function updateUsageDraft"));

  assert.match(
    sync,
    /if \(!edit\) \{\s+const created = mergeUsageEdit\(undefined, saved, Boolean\(wasActuallyReset\)\);/,
  );
  assert.ok(sync.indexOf("if (!edit)") < sync.indexOf("Object.assign(edit"));
});

test("bounded concurrency rejects invalid limits instead of dropping work", async () => {
  const worker = async (value: number) => value * 2;

  await assert.rejects(mapWithConcurrency([1], 0, worker), RangeError);
  await assert.rejects(mapWithConcurrency([1], -1, worker), RangeError);
  await assert.rejects(mapWithConcurrency([1], Number.NaN, worker), RangeError);
  await assert.rejects(mapWithConcurrency([1], 0.5, worker), RangeError);
});

test("accounts render before per-account usage and expose failed loads for retry", async () => {
  const accounts = await readFile(new URL("../views/Accounts.vue", import.meta.url), "utf8");
  const usage = await readFile(new URL("./useAccountUsage.ts", import.meta.url), "utf8");
  const load = accounts.slice(accounts.indexOf("async function loadAccounts"), accounts.indexOf("async function onFormSave"));

  assert.ok(load.indexOf("accounts.value = loaded") < load.indexOf("loadAccountUsage(account.id)"));
  assert.match(usage, /usageLoadErrors\.value\[accountId\] = dashboardErrorDetail\(error\)/);
  assert.match(accounts, /v-if="accountListLoading"[\s\S]*?v-else-if="accountListError"[\s\S]*?@click="loadAccounts"/);

  assert.match(accounts, /async function refreshAccountState/);
});

test("editing an account refreshes usage after purchase-date window changes", async () => {
  const source = await readFile(new URL("../views/Accounts.vue", import.meta.url), "utf8");
  const save = source.slice(source.indexOf("async function onFormSave"), source.indexOf("function openAccountTest"));

  const update = save.indexOf("const saved = await runWithFreshSettingsRevision");
  const replace = save.indexOf("replaceAccount(saved);");
  const refresh = save.indexOf("if (accountHasUsageDisplay(saved)) await loadAccountUsage(saved.id);");
  assert.ok(update >= 0 && replace > update && refresh > replace);
});

test("manual editor writes on commit events instead of each value update", async () => {
  const editor = await readFile(new URL("../components/AccountUsageEditor.vue", import.meta.url), "utf8");
  const usage = await readFile(new URL("./useAccountUsage.ts", import.meta.url), "utf8");

  assert.match(editor, /@update:value="emit\('update-draft', limit\.key, \$event\)"/);
  assert.match(editor, /@dragend="emit\('save', limit\.key\)"/);
  assert.match(editor, /@blur="emit\('save', limit\.key\)"/);
  assert.match(editor, /@keydown\.enter\.prevent="emit\('save', limit\.key\)"/);
  assert.match(usage, /if \(!edit \|\| edit\.saving\) return;/);
  assert.equal(usage.match(/edit\.resets_dirty = true;/g)?.length, 2);
  assert.match(usage, /const resetsInMin = resetsInMinutesForSave\(edit, key\)/);
  assert.match(usage, /message\.error\(t\("用量保存失败: \{error\}"/);
});

test("account drag keeps receiving touch pointers after keyed cards move", async () => {
  const order = await readFile(new URL("../views/useAccountOrder.ts", import.meta.url), "utf8");

  assert.match(order, /window\.addEventListener\("pointermove", previewAccountDrag, \{ passive: false \}\)/);
  assert.match(order, /window\.addEventListener\("pointerup", finishAccountDrag\)/);
  assert.match(order, /window\.addEventListener\("pointercancel", cancelAccountDrag\)/);
  assert.match(order, /window\.removeEventListener\("pointermove", previewAccountDrag\)/);
  assert.doesNotMatch(order, /@lostpointercapture|@pointermove="previewAccountDrag"/);
});

test("usage API sends the selected window and percent with PATCH", async () => {
  const dashboardSource = await readFile(new URL("../api/dashboard.ts", import.meta.url), "utf8");
  const v3Source = await readFile(new URL("../api/dashboard-v3.ts", import.meta.url), "utf8");
  const update = dashboardSource.slice(
    dashboardSource.indexOf("updateAccountUsage"),
    dashboardSource.indexOf("refreshAccountUsage"),
  );

  assert.match(update, /patchAccountUsage/);
  assert.match(update, /resetsInMinutes/);
  assert.match(v3Source, /patchAccountUsage: \(id: string, update: WithoutExpectation<AccountUsageUpdate>, expectation: MutationExpectation\)/);
  assert.match(v3Source, /method: "PATCH"/);
  assert.match(v3Source, /refreshAccountUsage: \(id: string, expectation: MutationExpectation\)/);
  assert.match(v3Source, /`\/accounts\/\$\{encode\(id\)\}\/usage\/refresh`/);
  assert.match(v3Source, /method: "POST"/);
  assert.doesNotMatch(dashboardSource, /refreshManagedAccountUsage/);
});
