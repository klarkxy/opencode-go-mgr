import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { isUsageLimitReached, usageProgressStatus } from "./accounts-usage.ts";
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

test("accounts render before per-account usage and tolerate usage failures", async () => {
  const source = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");
  const load = source.slice(source.indexOf("async function loadAccounts"), source.indexOf("async function createAccount"));

  assert.ok(load.indexOf("accounts.value = loaded") < load.indexOf("getAccountUsage"));
  assert.match(load, /catch \{[\s\S]*blankUsage\(account\.id\)/);
});
