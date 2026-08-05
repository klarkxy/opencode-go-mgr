import assert from "node:assert/strict";
import test from "node:test";
import type { AppConfig } from "../api/tauri.ts";
import { mergeUnsavedSettings } from "./settings-merge.ts";

function config(overrides: Partial<AppConfig> = {}): AppConfig {
  return {
    revision: 1,
    gateway_port: 9042,
    gateway_key: "server-key-1",
    upstream_base_url: "https://opencode.ai/zen/go",
    opencode_invite_url: "https://opencode.ai/go?ref=68XPB6NP8V",
    client_root_url: "",
    client_root_url_from_env: false,
    auto_start: false,
    auto_start_supported: true,
    show_dock_icon: true,
    dock_visibility_supported: false,
    connect_timeout_secs: 30,
    non_stream_timeout_secs: 900,
    stream_idle_timeout_secs: 300,
    routing_mode: "strict-priority",
    conversation_sticky: true,
    ...overrides,
  };
}

test("settings conflict merge preserves local edits and accepts unrelated remote edits", () => {
  const saved = config();
  const current = config({
    opencode_invite_url: "https://opencode.ai/invite/local",
    connect_timeout_secs: 45,
  });
  const latest = config({
    revision: 2,
    gateway_port: 9142,
    non_stream_timeout_secs: 1_200,
  });

  const merged = mergeUnsavedSettings(latest, current, saved);

  assert.equal(merged.revision, 2);
  assert.equal(merged.opencode_invite_url, "https://opencode.ai/invite/local");
  assert.equal(merged.connect_timeout_secs, 45);
  assert.equal(merged.gateway_port, 9142);
  assert.equal(merged.non_stream_timeout_secs, 1_200);
});

test("settings conflict merge never restores stale secrets or capability flags", () => {
  const saved = config();
  const current = config({
    gateway_key: "unpersisted-key",
    auto_start: true,
    auto_start_supported: true,
  });
  const latest = config({
    revision: 3,
    gateway_key: "server-key-3",
    auto_start_supported: false,
    client_root_url_from_env: true,
  });

  const merged = mergeUnsavedSettings(latest, current, saved);

  assert.equal(merged.gateway_key, "server-key-3");
  assert.equal(merged.auto_start, true);
  assert.equal(merged.auto_start_supported, false);
  assert.equal(merged.client_root_url_from_env, true);
});
