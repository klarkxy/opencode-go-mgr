import assert from "node:assert/strict";
import test from "node:test";
import type { AppConfig } from "./dashboard-presenters.ts";
import { settingsUpdateInput } from "./dashboard-presenters.ts";

function appConfig(overrides: Partial<AppConfig> = {}): AppConfig {
  return {
    revision: 1,
    gateway_port: 9042,
    gateway_port_from_env: false,
    proxy_mode: "auto",
    proxy_url: "",
    proxy_list_direction: "whitelist",
    proxy_list_models: [],
    proxy_supported_models: [],
    opencode_invite_url: "https://invite.example.test",
    client_root_url: "https://client.example.test",
    client_root_url_from_env: false,
    auto_start: false,
    auto_start_supported: false,
    show_dock_icon: false,
    dock_visibility_supported: false,
    connect_timeout_secs: 10,
    non_stream_timeout_secs: 60,
    stream_idle_timeout_secs: 300,
    routing_mode: "strict-priority",
    conversation_sticky: true,
    ...overrides,
  };
}

function assertAlwaysSentFields(input: ReturnType<typeof settingsUpdateInput>, value: AppConfig): void {
  assert.equal(input.clientRootUrl, value.client_root_url);
  assert.equal(input.connectTimeoutSecs, value.connect_timeout_secs);
  assert.equal(input.conversationSticky, value.conversation_sticky);
  assert.equal(input.nonStreamTimeoutSecs, value.non_stream_timeout_secs);
  assert.equal(input.opencodeInviteUrl, value.opencode_invite_url);
  assert.equal(input.proxyListDirection, value.proxy_list_direction);
  assert.equal(input.proxyListModels, value.proxy_list_models);
  assert.equal(input.proxyMode, value.proxy_mode);
  assert.equal(input.proxyUrl, value.proxy_url);
  assert.equal(input.routingMode, value.routing_mode);
  assert.equal(input.streamIdleTimeoutSecs, value.stream_idle_timeout_secs);
}

test("settingsUpdateInput omits showDockIcon when dock visibility is unsupported (Windows)", () => {
  const value = appConfig({ auto_start: true, auto_start_supported: true, dock_visibility_supported: false });
  const input = settingsUpdateInput(value);
  assert.equal(input.autoStart, true);
  assert.equal("showDockIcon" in input, false);
  assert.equal(Object.hasOwn(input, "showDockIcon"), false);
  assertAlwaysSentFields(input, value);
});

test("settingsUpdateInput sends autoStart false when auto start is supported but disabled (Windows)", () => {
  const value = appConfig({ auto_start: false, auto_start_supported: true, dock_visibility_supported: false });
  const input = settingsUpdateInput(value);
  assert.equal("autoStart" in input, true);
  assert.equal(input.autoStart, false);
  assert.equal("showDockIcon" in input, false);
  assertAlwaysSentFields(input, value);
});

test("settingsUpdateInput omits autoStart and showDockIcon when neither capability is supported", () => {
  const value = appConfig({ auto_start: true, auto_start_supported: false, show_dock_icon: true, dock_visibility_supported: false });
  const input = settingsUpdateInput(value);
  assert.equal("autoStart" in input, false);
  assert.equal("showDockIcon" in input, false);
  assert.equal(Object.hasOwn(input, "autoStart"), false);
  assert.equal(Object.hasOwn(input, "showDockIcon"), false);
  assertAlwaysSentFields(input, value);
});

test("settingsUpdateInput sends both autoStart and showDockIcon when both capabilities are supported", () => {
  const value = appConfig({
    auto_start: true,
    auto_start_supported: true,
    show_dock_icon: true,
    dock_visibility_supported: true,
  });
  const input = settingsUpdateInput(value);
  assert.equal(input.autoStart, true);
  assert.equal(input.showDockIcon, true);
  assertAlwaysSentFields(input, value);
});

test("settingsUpdateInput sends supported capabilities set to false instead of omitting them", () => {
  const value = appConfig({
    auto_start: false,
    auto_start_supported: true,
    show_dock_icon: false,
    dock_visibility_supported: true,
  });
  const input = settingsUpdateInput(value);
  assert.equal("autoStart" in input, true);
  assert.equal("showDockIcon" in input, true);
  assert.equal(input.autoStart, false);
  assert.equal(input.showDockIcon, false);
});

test("settingsUpdateInput omits autoStart and sends showDockIcon when only dock visibility is supported", () => {
  const on = appConfig({ auto_start_supported: false, show_dock_icon: true, dock_visibility_supported: true });
  const inputOn = settingsUpdateInput(on);
  assert.equal("autoStart" in inputOn, false);
  assert.equal(inputOn.showDockIcon, true);
  assertAlwaysSentFields(inputOn, on);

  const off = appConfig({ auto_start_supported: false, show_dock_icon: false, dock_visibility_supported: true });
  const inputOff = settingsUpdateInput(off);
  assert.equal("autoStart" in inputOff, false);
  assert.equal("showDockIcon" in inputOff, true);
  assert.equal(inputOff.showDockIcon, false);
});

test("settingsUpdateInput omits gatewayPort when gateway port comes from the environment", () => {
  const value = appConfig({ gateway_port_from_env: true });
  const input = settingsUpdateInput(value);
  assert.equal("gatewayPort" in input, false);
  assertAlwaysSentFields(input, value);
});

test("settingsUpdateInput sends the exact gatewayPort when it is not from the environment", () => {
  const value = appConfig({ gateway_port_from_env: false, gateway_port: 19042 });
  const input = settingsUpdateInput(value);
  assert.equal("gatewayPort" in input, true);
  assert.equal(input.gatewayPort, 19042);
});
