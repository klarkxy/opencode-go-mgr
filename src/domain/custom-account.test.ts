import assert from "node:assert/strict";
import test from "node:test";
import {
  CUSTOM_ENDPOINT_URL_ISSUE_KEYS,
  customAccountNeedsVerification,
  customEndpointUrlIssue,
  customApiUrlPlaceholder,
  customApiUrlSupportsModelDiscovery,
  customApiUrlNeedsManualModels,
  expandCustomModelCapabilities,
  normalizeCustomCapabilities,
  CustomCapabilityError,
} from "./custom-account.ts";
import type { Account } from "../api/dashboard.ts";

test("trusted Endpoint validation permits LAN, localhost, and HTTP", () => {
  for (const endpoint of [
    "http://192.168.1.10:8080/v1/chat/completions",
    "http://localhost:3000/responses",
    "http://[::1]:8080/messages",
    "https://api.example.com/v1/messages",
  ]) assert.equal(customEndpointUrlIssue(endpoint), null, endpoint);
  assert.equal(customEndpointUrlIssue("ftp://api.example.com"), "not_http");
  assert.equal(customEndpointUrlIssue("https://user:pass@api.example.com"), "with_credentials");
  assert.equal(CUSTOM_ENDPOINT_URL_ISSUE_KEYS.empty, "请填写 API 地址");
});

test("common API bases and legacy standard paths enable model discovery", () => {
  assert.equal(customApiUrlPlaceholder(), "https://api.example.com");
  assert.ok(customApiUrlSupportsModelDiscovery("https://api.example.com", "chat_completions"));
  assert.ok(customApiUrlSupportsModelDiscovery("https://api.example.com/v1", "chat_completions"));
  assert.ok(customApiUrlSupportsModelDiscovery("https://api.example.com/openai/v1/", "responses"));
  assert.ok(customApiUrlSupportsModelDiscovery("https://api.example.com/v1/chat/completions", "chat_completions"));
  assert.ok(customApiUrlSupportsModelDiscovery("https://api.example.com/v1/responses/", "responses"));
  assert.ok(customApiUrlSupportsModelDiscovery("https://api.example.com/v1/messages", "messages"));
  assert.ok(!customApiUrlSupportsModelDiscovery("https://api.example.com/custom/infer", "messages"));
  assert.ok(!customApiUrlSupportsModelDiscovery("https://api.example.com/v1/messages", "responses"));
  assert.ok(!customApiUrlNeedsManualModels("", "messages"));
  assert.ok(!customApiUrlNeedsManualModels("not a url", "messages"));
  assert.ok(!customApiUrlNeedsManualModels("https://api.example.com", "messages"));
  assert.ok(customApiUrlNeedsManualModels("https://api.example.com/custom/infer", "messages"));
});

test("one protocol expands each model once and rejects mismatched rows", () => {
  const rows = expandCustomModelCapabilities(["m1", "m2"], "messages");
  assert.deepEqual(rows, [
    { public_model: "m1", upstream_model: "m1", protocol: "messages" },
    { public_model: "m2", upstream_model: "m2", protocol: "messages" },
  ]);
  assert.deepEqual(normalizeCustomCapabilities(rows, "messages"), [
    { public_model: "m1", upstream_model: "m1", protocol: "messages", source: "manual" },
    { public_model: "m2", upstream_model: "m2", protocol: "messages", source: "manual" },
  ]);
  assert.throws(
    () => normalizeCustomCapabilities([{ public_model: "m", upstream_model: "m", protocol: "responses" }], "messages"),
    (error) => error instanceof CustomCapabilityError && error.issue === "protocol_mismatch",
  );
});

test("public models are case-insensitively unique while upstream IDs are reusable", () => {
  assert.deepEqual(normalizeCustomCapabilities([
    { public_model: "chat", upstream_model: "vendor/shared", protocol: "messages" },
    { public_model: "reasoning", upstream_model: "vendor/shared", protocol: "messages" },
  ], "messages").map(({ public_model, upstream_model }) => ({ public_model, upstream_model })), [
    { public_model: "chat", upstream_model: "vendor/shared" },
    { public_model: "reasoning", upstream_model: "vendor/shared" },
  ]);
  assert.throws(
    () => normalizeCustomCapabilities([
      { public_model: "Chat", upstream_model: "vendor/a", protocol: "messages" },
      { public_model: "chat", upstream_model: "vendor/b", protocol: "messages" },
    ], "messages"),
    (error) => error instanceof CustomCapabilityError && error.issue === "duplicate_public_model",
  );
});

test("verification state only applies to Custom accounts", () => {
  const account = { provider_id: "custom", verification_status: "pending" } as Pick<
    Account,
    "provider_id" | "verification_status"
  >;
  assert.ok(customAccountNeedsVerification(account));
  assert.ok(!customAccountNeedsVerification({ ...account, verification_status: "verified" }));
});
