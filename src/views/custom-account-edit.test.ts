import assert from "node:assert/strict";
import test from "node:test";
import type { Account } from "../api/dashboard.ts";
import { applyCustomAccountEditPlan, planCustomAccountEdit } from "../domain/custom-account.ts";

function customAccount(): Account {
  return {
    id: "custom-1", name: "Custom", username: "", password: "", key: "", enabled: false,
    account_type: "key", setup_step: "ready", provider_id: "custom",
    credential_kind: "api_key", quota_scope: "key", purchase_date: "", expires_on: "",
    cooldown_until: null, cooldown_generic_until: null, cooldown_5h_until: null,
    cooldown_week_until: null, cooldown_month_until: null, cooldown_free_until: null,
    last_error: null, auth_error: null, notes: "", usage_sync_last_success_at: null,
    usage_sync_next_allowed_at: null, created_at: "2026-08-21T00:00:00Z",
    updated_at: "2026-08-21T00:00:00Z", verification_status: "verified",
    connection_verified_at: null, verification_error: null, plan_routable: true,
    custom_config: {
      account_id: "custom-1", endpoint_url: "https://api.example.com/v1/responses",
      upstream_protocol: "responses", created_at: "2026-08-21T00:00:00Z", updated_at: "2026-08-21T00:00:00Z",
    },
    model_capabilities: [{ public_model: "model-a", upstream_model: "provider/model-a", protocol: "responses", verified_at: null, source: "manual" }],
  };
}

test("Custom config and capabilities use one atomic config write", async () => {
  const account = customAccount();
  const plan = planCustomAccountEdit(account, {
    name: "Custom", endpoint_url: "https://api.example.com/v1/messages", upstream_protocol: "messages",
    model_capabilities: [{ public_model: "model-a", upstream_model: "provider/model-a", protocol: "messages" }],
  });
  assert.deepEqual(plan.customConfig, {
    endpoint_url: "https://api.example.com/v1/messages", upstream_protocol: "messages",
    model_capabilities: [{ public_model: "model-a", upstream_model: "provider/model-a", protocol: "messages", source: "manual" }],
  });
  const calls: string[] = [];
  await applyCustomAccountEditPlan(plan, {
    account: async () => { calls.push("account"); },
    customConfig: async () => { calls.push("custom-config"); },
  });
  assert.deepEqual(calls, ["custom-config"]);
});

test("metadata-only edits do not rewrite Custom config", () => {
  const plan = planCustomAccountEdit(customAccount(), {
    name: "Renamed", endpoint_url: "https://api.example.com/v1/responses",
    upstream_protocol: "responses", model_capabilities: [{ public_model: "model-a", upstream_model: "provider/model-a", protocol: "responses" }],
  });
  assert.ok(plan.account);
  assert.equal(plan.customConfig, undefined);
});
