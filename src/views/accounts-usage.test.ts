import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  isCooling,
  isUsageLimitReached,
  mergeUsageEdit,
  normalizeUsagePercent,
  resetTimeForWindow,
  resetsInMinutesForSave,
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

test("keeps generic and overlapping window cooldowns visible", () => {
  assert.equal(isCooling({
    cooldown_until: "2099-01-01T00:00:00Z",
    cooldown_5h_until: null,
    cooldown_week_until: null,
    cooldown_month_until: null,
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
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");
  const progress = source.indexOf(":percentage=\"usageProgressPercentage(");
  const countdown = source.indexOf("<span class=\"usage-reset-countdown\">");

  assert.ok(progress >= 0);
  assert.ok(countdown > progress);
  assert.match(source, /accountUsageLimitReached\(account, limit\.key\)[\s\S]*formatWindowRemaining\(account, limit\.key\)/);
  assert.match(source, /\.usage-reset-countdown \{[\s\S]*color: var\(--ocg-error\);/);
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

test("keeps account cards compact with metadata tags and top-level usage calibration", async () => {
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");
  const header = source.slice(
    source.indexOf("<template #header>"),
    source.indexOf('<div v-if="!quotaLimitsError'),
  );
  const usageStart = source.indexOf('class="usage-strip-body" role="group"');
  const usage = source.slice(
    usageStart,
    source.indexOf("</n-card>"),
  );

  assert.ok(header.indexOf("accountStatusLabel(account)") < header.indexOf('t("购买于 {date}"'));
  assert.match(header, /<n-tag size="small" :bordered="false">\s+\{\{ t\("购买于 \{date\}"/);
  assert.match(header, /<n-tag size="small" :bordered="false">\s+\{\{ t\("到期于 \{date\}"/);
  assert.match(header, /:aria-label="t\('校准用量'\)"/);
  assert.doesNotMatch(usage, /usage-strip-title|\{\{ t\("用量"\) \}\}/);
  assert.match(usage, /class="usage-strip-body" role="group" :aria-label="t\('用量'\)"/);
  assert.doesNotMatch(source, /class="account-lifecycle"|\.account-lifecycle\s*\{/);
  assert.match(source, /key: "edit", label: t\("编辑账号"\)/);
  assert.match(source, /v-if="quotaLimitsError"[\s\S]*?@click="retryQuotaLimits"/);
  assert.equal(source.match(/v-if="quotaLimitsError"/g)?.length, 1);
});

test("normalizes manually entered percentages to the supported range and precision", () => {
  assert.equal(normalizeUsagePercent(-1), 0);
  assert.equal(normalizeUsagePercent(42.56), 42.6);
  assert.equal(normalizeUsagePercent(101), 100);
  assert.equal(usagePercentFromCost(6, 12), 50);
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
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");
  const fields = source.slice(source.indexOf("function resetsFirstField"), source.indexOf("function fieldsToMinutes"));

  assert.equal(fields.match(/resetsInMinutesForSave\(edit, key, now\.value\)/g)?.length, 2);
});

test("calibration shortcut is disabled when every usage window is cooling", async () => {
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");

  assert.match(source, /:disabled="!hasAvailableUsageEditor\(account\)"/);
  assert.match(source, /usageLoading\.value\[account\.id\] \|\| usageLoadErrors\.value\[account\.id\]/);
  assert.match(source, /usageLimits\.value\.some\(\(\{ key \}\) => !accountUsageLimitReached\(account, key\)\)/);
});

test("usage refresh initializes windows missing after an earlier quota load failure", async () => {
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");
  const sync = source.slice(source.indexOf("function syncUsageEdits"), source.indexOf("function updateUsageDraft"));

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
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");
  const load = source.slice(source.indexOf("async function loadAccounts"), source.indexOf("async function onFormSave"));

  assert.ok(load.indexOf("accounts.value = loaded") < load.indexOf("getAccountUsage"));
  assert.match(load, /loadAccountUsage\(account\.id\)/);
  assert.match(load, /usageLoadErrors\.value\[accountId\] = errorDetail\(error\)/);
  assert.match(source, /v-if="accountListLoading"[\s\S]*?v-else-if="accountListError"[\s\S]*?@click="loadAccounts"/);

  const ping = source.slice(source.indexOf("async function pingAccount"), source.indexOf("async function toggleAccount"));
  assert.match(ping, /try \{\s+await refreshAccountState\(id\);\s+\} catch \(e\) \{/);
});

test("manual editor writes on commit events instead of each value update", async () => {
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");

  assert.match(source, /@update:value="updateUsageDraft\(account\.id, limit\.key, \$event\)"/);
  assert.match(source, /@dragend="saveUsage\(account\.id, limit\.key\)"/);
  assert.match(source, /@blur="saveUsage\(account\.id, limit\.key\)"/);
  assert.match(source, /@keydown\.enter\.prevent="saveUsage\(account\.id, limit\.key\)"/);
  assert.match(source, /if \(!edit \|\| edit\.saving\) return;/);
  assert.equal(source.match(/edit\.resets_dirty = true;/g)?.length, 2);
  assert.match(source, /const resetsInMin = resetsInMinutesForSave\(edit, key\)/);
});

test("account drag keeps receiving touch pointers after keyed cards move", async () => {
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");

  assert.match(source, /window\.addEventListener\("pointermove", previewAccountDrag, \{ passive: false \}\)/);
  assert.match(source, /window\.addEventListener\("pointerup", finishAccountDrag\)/);
  assert.match(source, /window\.addEventListener\("pointercancel", cancelAccountDrag\)/);
  assert.match(source, /window\.removeEventListener\("pointermove", previewAccountDrag\)/);
  assert.doesNotMatch(source, /@lostpointercapture|@pointermove="previewAccountDrag"/);
});

test("usage API sends the selected window and percent with PATCH", async () => {
  const source = await readFile(new URL("../api/tauri.ts", import.meta.url), "utf8");
  const update = source.slice(source.indexOf("updateAccountUsage"), source.indexOf("resetAccountCooldown"));

  assert.match(update, /method: "PATCH"/);
  assert.match(update, /jsonBody\(\{ window, percent, resets_in_minutes/);
});
