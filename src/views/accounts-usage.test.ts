import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  isUsageLimitReached,
  mergeUsageEdit,
  normalizeUsagePercent,
  usagePercentFromCost,
  usageProgressStatus,
} from "./accounts-usage.ts";
import type { UsageKey } from "./accounts-usage.ts";

test("fills only the active 5-hour, weekly, or monthly limit", () => {
  const cases: Array<[UsageKey, string]> = [
    ["window_5h", "5-hour usage limit reached. Resets in 13min."],
    ["window_week", "Weekly usage limit reached. Resets in 4 days."],
    ["window_month", "Monthly usage limit reached. Resets in 13 days."],
  ];

  for (const [key, last_error] of cases) {
    assert.equal(
      isUsageLimitReached({ cooldown_until: "2099-01-01T00:00:00Z", last_error }, key),
      true,
    );
  }
  assert.equal(
    isUsageLimitReached(
      {
        cooldown_until: "2099-01-01T00:00:00Z",
        last_error: "Weekly usage limit reached. Resets in 4 days.",
      },
      "window_month",
    ),
    false,
  );
  assert.equal(
    isUsageLimitReached(
      {
        cooldown_until: "2000-01-01T00:00:00Z",
        last_error: "Weekly usage limit reached. Resets in 4 days.",
      },
      "window_week",
    ),
    false,
  );
});

test("shows local estimated saturation as a warning, not a real breaker", () => {
  assert.equal(
    usageProgressStatus({ cooldown_until: null, last_error: null }, "window_week", 100),
    "warning",
  );
  assert.equal(
    usageProgressStatus(
      {
        cooldown_until: "2099-01-01T00:00:00Z",
        last_error: "Weekly usage limit reached. Resets in 4 days.",
      },
      "window_week",
      100,
    ),
    "error",
  );
});

test("normalizes manually entered percentages to the supported range and precision", () => {
  assert.equal(normalizeUsagePercent(-1), 0);
  assert.equal(normalizeUsagePercent(42.56), 42.6);
  assert.equal(normalizeUsagePercent(101), 100);
  assert.equal(usagePercentFromCost(6, 12), 50);
});

test("usage refresh preserves dirty drafts unless a real 429 reset that window", () => {
  const dirty = { draft: 75, saved: 20, saving: false, error: "save failed" };

  assert.deepEqual(mergeUsageEdit(dirty, 35, false), {
    draft: 75,
    saved: 35,
    saving: false,
    error: "save failed",
  });
  assert.deepEqual(mergeUsageEdit(dirty, 0, true), {
    draft: 0,
    saved: 0,
    saving: false,
    error: null,
  });
});

test("accounts render before per-account usage and expose failed loads for retry", async () => {
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");
  const load = source.slice(source.indexOf("async function loadAccounts"), source.indexOf("async function createAccount"));

  assert.ok(load.indexOf("accounts.value = loaded") < load.indexOf("getAccountUsage"));
  assert.match(load, /loadAccountUsage\(account\.id\)/);
  assert.match(load, /usageLoadErrors\.value\[accountId\] = String\(error\)/);

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
});

test("usage API sends the selected window and percent with PATCH", async () => {
  const source = await readFile(new URL("../api/tauri.ts", import.meta.url), "utf8");
  const update = source.slice(source.indexOf("updateAccountUsage"), source.indexOf("resetAccountCooldown"));

  assert.match(update, /method: "PATCH"/);
  assert.match(update, /jsonBody\(\{ window, percent \}\)/);
});
