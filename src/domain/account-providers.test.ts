import assert from "node:assert/strict";
import { test } from "node:test";

import { OLLAMA_PROVIDER_ID, isOllamaCloudAccount } from "./account-providers.ts";
import { findPlanDefinition } from "./plans.ts";

test("ollama cloud account predicate matches the sealed family exactly", () => {
  assert.ok(isOllamaCloudAccount({ provider_id: OLLAMA_PROVIDER_ID }));
  assert.ok(!isOllamaCloudAccount({ provider_id: "opencode" }));
  assert.ok(!isOllamaCloudAccount({ provider_id: "kimi" }));
});

test("ollama cloud plan definition follows the sealed registry identities", () => {
  const plan = findPlanDefinition(OLLAMA_PROVIDER_ID);
  assert.ok(plan, "the plan definition must exist");
  assert.equal(plan.id, "ollama-cloud");
  assert.equal(plan.kind, "api-key");
  assert.equal(plan.credential_kind, "api_key");
  assert.equal(plan.quota_scope, "key");
  assert.equal(plan.singleton, false);
  assert.equal(plan.managed_registration, false);
});
