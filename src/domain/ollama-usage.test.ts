import assert from "node:assert/strict";
import test from "node:test";
import type { OllamaUsageResponse } from "../api/providers.ts";
import { isOllamaUsageRefreshBlocked } from "./ollama-usage.ts";

function usage(overrides: Partial<OllamaUsageResponse> = {}): OllamaUsageResponse {
  return {
    account_id: "ollama-1",
    cookie_configured: true,
    status: "ok",
    snapshot: null,
    last_error: null,
    last_success_at: null,
    last_attempt_at: null,
    next_eligible_at: null,
    failure_streak: 0,
    ...overrides,
  };
}

test("Ollama usage refresh follows Cookie configuration and its own throttle", () => {
  const now = Date.parse("2026-09-03T00:00:00Z");
  assert.equal(isOllamaUsageRefreshBlocked(null, now), false);
  assert.equal(isOllamaUsageRefreshBlocked(usage({ cookie_configured: false }), now), true);
  assert.equal(isOllamaUsageRefreshBlocked(usage({ next_eligible_at: "2026-09-03T00:00:30Z" }), now), true);
  assert.equal(isOllamaUsageRefreshBlocked(usage({ next_eligible_at: "2026-09-02T23:59:59Z" }), now), false);
  assert.equal(isOllamaUsageRefreshBlocked(usage({ next_eligible_at: "not-a-date" }), now), false);
});
