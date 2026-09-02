import assert from "node:assert/strict";
import test from "node:test";
import type { Account } from "../api/dashboard.ts";
import {
  accountMenuOptions,
  accountRoutingDraftDescription,
  accountRoutingDraftLabel,
  accountStatusLabel,
  accountStatusTagType,
} from "./account-display.ts";

function draftAccount(overrides: Partial<Account> = {}): Account {
  return {
    id: "draft",
    name: "Draft",
    username: "",
    password: "",
    key: "key",
    enabled: false,
    account_type: "key",
    setup_step: "ready",
    provider_id: "custom",
    offering_id: "api",
    credential_kind: "api_key",
    quota_scope: "key",
    purchase_date: "2026-08-21",
    expires_on: "2026-09-21",
    cooldown_until: null,
    cooldown_generic_until: null,
    cooldown_5h_until: null,
    cooldown_week_until: null,
    cooldown_month_until: null,
    cooldown_free_until: null,
    last_error: null,
    auth_error: null,
    notes: "",
    usage_sync_last_success_at: null,
    usage_sync_next_allowed_at: null,
    created_at: "2026-08-21T00:00:00Z",
    updated_at: "2026-08-21T00:00:00Z",
    verification_status: "pending",
    connection_verified_at: null,
    verification_error: null,
    plan_routable: false,
    model_capabilities: [],
    ...overrides,
  };
}

test("unroutable account drafts use verification status rather than provider-specific branches", () => {
  const base = { setup_step: "ready" as const, plan_routable: false };
  assert.equal(
    accountRoutingDraftLabel({ ...base, verification_status: "pending" }),
    "待验证",
  );
  assert.equal(
    accountRoutingDraftDescription({ ...base, verification_status: "pending" }),
    "该方案验证功能暂不可用，创建后保持禁用草稿。",
  );
  assert.equal(
    accountRoutingDraftLabel({ ...base, verification_status: "failed" }),
    "验证失败",
  );
  assert.equal(
    accountRoutingDraftLabel({ ...base, verification_status: "not_required" }),
    "等待支持",
  );
  assert.equal(
    accountRoutingDraftLabel({ ...base, plan_routable: true, verification_status: "verified" }),
    null,
  );
});

test("draft status replaces disabled instead of rendering a second competing state", () => {
  const pending = draftAccount();
  assert.equal(accountStatusLabel(pending), "待验证");
  assert.equal(accountStatusTagType(pending), "warning");

  const unsupported = draftAccount({ verification_status: "not_required" });
  assert.equal(accountStatusLabel(unsupported), "等待支持");
  assert.equal(accountStatusTagType(unsupported), "warning");

  const failed = draftAccount({ verification_status: "failed" });
  assert.equal(accountStatusLabel(failed), "验证失败");
  assert.equal(accountStatusTagType(failed), "error");

  const ordinaryDisabled = draftAccount({ plan_routable: true, verification_status: "verified" });
  assert.equal(accountStatusLabel(ordinaryDisabled), "已禁用");
  assert.equal(accountStatusTagType(ordinaryDisabled), "default");
});

test("account menu keeps OpenCode-only actions off other sealed families", () => {
  const now = Date.now();
  const base = {
    setup_step: "ready" as const,
    plan_routable: true,
    verification_status: "not_required" as const,
  };
  const ollama = {
    ...base,
    id: "ollama-1",
    name: "s",
    provider_id: "ollama",
    offering_id: "cloud",
    enabled: true,
  } as unknown as Parameters<typeof accountMenuOptions>[0];
  const keys = accountMenuOptions(ollama, now).map((option) => option.key);
  assert.deepEqual(keys, ["open-site", "edit", "delete"]);

  const opencodeGo = {
    ...base,
    id: "go-1",
    name: "go",
    provider_id: "opencode",
    offering_id: "go",
    enabled: true,
  } as unknown as Parameters<typeof accountMenuOptions>[0];
  const goKeys = accountMenuOptions(opencodeGo, now).map((option) => option.key);
  assert.ok(goKeys.includes("open-console"));
  assert.ok(goKeys.includes("reset-profile"));
  assert.ok(goKeys.includes("edit"));
  assert.ok(goKeys.includes("delete"));
});
