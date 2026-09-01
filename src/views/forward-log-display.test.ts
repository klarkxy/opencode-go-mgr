import assert from "node:assert/strict";
import test from "node:test";
import {
  forwardLogAlias,
  forwardLogRequestedModel,
  forwardLogResolvedAlias,
  forwardLogTotalTokens,
  forwardLogUpstreamModel,
} from "./forward-log-display.ts";

test("forward log Alias columns keep requested, resolved, and upstream meanings distinct", () => {
  const row = {
    model: "legacy-model",
    requested_model: "sonnet",
    resolved_alias: "claude-sonnet",
    upstream_model: "anthropic/claude-sonnet-4",
  };
  assert.equal(forwardLogAlias(row), "claude-sonnet");
  assert.equal(forwardLogRequestedModel(row), "sonnet");
  assert.equal(forwardLogResolvedAlias(row), "claude-sonnet");
  assert.equal(forwardLogUpstreamModel(row), "anthropic/claude-sonnet-4");
});

test("legacy forward logs still expose their stored model without inventing an Alias", () => {
  assert.equal(forwardLogAlias({ model: "legacy", requested_model: null, resolved_alias: null }), "legacy");
  assert.equal(forwardLogRequestedModel({ requested_model: null }), null);
  assert.equal(forwardLogResolvedAlias({ resolved_alias: "  " }), null);
});

test("forward log total tokens do not double count cached or cache-written input", () => {
  // prompt_tokens already includes the cached-read and cache-write portions
  // (31,331 = 99 fresh + 31,232 cache read), so the total is input + output.
  const row = {
    prompt_tokens: 31_331,
    completion_tokens: 4_505,
    cached_tokens: 31_232,
    cache_creation_tokens: 0,
  };
  assert.equal(forwardLogTotalTokens(row), 35_836);
});

test("forward log total tokens stay input + output when cache write is nonzero", () => {
  // Anthropic-style row: input already includes cache read and cache write
  // (100 = 50 fresh + 30 cache read + 20 cache write).
  const row = {
    prompt_tokens: 100,
    completion_tokens: 50,
    cached_tokens: 30,
    cache_creation_tokens: 20,
  };
  assert.equal(forwardLogTotalTokens(row), 150);
});
