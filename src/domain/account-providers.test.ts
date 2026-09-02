import assert from "node:assert/strict";
import { test } from "node:test";

import {
  OLLAMA_CLOUD_OFFERING_ID,
  OLLAMA_PROVIDER_ID,
  isOllamaCloudAccount,
} from "./account-providers.ts";
import { findPlanDefinition } from "./plans.ts";

test("ollama cloud account predicate matches the sealed family exactly", () => {
  assert.ok(
    isOllamaCloudAccount({
      provider_id: OLLAMA_PROVIDER_ID,
      offering_id: OLLAMA_CLOUD_OFFERING_ID,
    }),
  );
  assert.ok(
    !isOllamaCloudAccount({ provider_id: "opencode", offering_id: "go" }),
  );
  assert.ok(
    !isOllamaCloudAccount({ provider_id: "ollama", offering_id: "go" }),
    "wrong offering must not match",
  );
  assert.ok(
    !isOllamaCloudAccount({ provider_id: "kimi", offering_id: "cloud" }),
    "wrong provider must not match",
  );
});

test("ollama cloud plan definition follows the sealed registry identities", () => {
  const plan = findPlanDefinition(OLLAMA_PROVIDER_ID, OLLAMA_CLOUD_OFFERING_ID);
  assert.ok(plan, "the plan definition must exist");
  assert.equal(plan.id, "ollama-cloud");
  assert.equal(plan.kind, "api-key");
  assert.equal(plan.credential_kind, "api_key");
  assert.equal(plan.quota_scope, "key");
  assert.equal(plan.singleton, false);
  assert.equal(plan.managed_registration, false);
});
