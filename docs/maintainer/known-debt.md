[简体中文](known-debt.zh-CN.md)

# Known Debt And Non-Goals

## Known Debt

- Auto-start is capability-gated: only Windows release/installed Tauri
  processes inject the registry sync hook. Development builds, the CLI,
  Docker, macOS, and Linux dashboards do not expose the switch. Dock
  visibility is macOS Tauri only.
- Existing generated Tauri schema files are noisy in diffs; avoid touching
  them unless the Tauri config actually changed.
- Streaming cost is exact only when upstream emits usage chunks. Chat streams
  request `stream_options.include_usage`. Without a chunk, Go rows end as
  `success_no_usage`; Zen success without usage stays `success` / `free`.
- Legacy `profiles/<account_id>` WebView profiles are not migrated to
  external Chromium, so users sign in again after upgrading. The old path is
  retained only for safe reset/delete cleanup; never attempt cross-engine
  reuse.
- The Responses endpoint is stateless. `previous_response_id`, `conversation`,
  `store: true`, and `background: true` return `400` rather than being
  silently ignored. This is intentional — see `protocol.rs` and the [User guide](../USER.md).
- Gemini is a client compatibility format, not a native upstream. Only
  `generateContent` and `streamGenerateContent` forward; `countTokens` and
  `embedContent` return `501`. Non-empty `safetySettings`, `cachedContent`,
  file-backed media, Google-hosted tools, and other unconvertible semantics
  return `400`. `topK` and `thinkingConfig` are accepted for compatibility
  but not guaranteed to behave equivalently on Chat Completions or Messages
  upstreams. Every other non-null `generationConfig` field must be mapped or
  rejected; no silent pass-through.
- Claude Desktop only advertises three fixed Claude aliases, mapped to the
  supported actual models; it does not mean OCG Manager provides native
  Claude 4.6 models or the full Anthropic Models API.
- Command Code GOAT has no machine-readable usage endpoint. Its public model
  directory cannot validate a stored Key, so authentication failure is only
  known from real inference 401/403. Custom API remains a distinct live route
  under the trusted-administrator boundary (`custom.rs` + `custom_http.rs`).
- Per-model/per-protocol overrides are on V3; Custom account-level
  per-protocol probing has no V3 counterpart, and the historical V2
  account-owned probe path is 410. Custom verify and model discovery are the
  live Custom operational paths.

## Deliberate Non-Goals

- Dynamic adapter/plugin loading, user-defined adapter implementations, or
  adapters that own SQLite, `CoreState`, or a raw `reqwest::Client`. Typed
  user-defined Provider definitions remain supported data bound to the sealed
  Configurable HTTP adapter.
- Remote node sync, an Admin API, or a multi-tenant control plane.
- Tauri `invoke` as a dashboard data path; WebView commands stay removed.
- Request-time upstream discovery on `GET /v1/models` or
  `GET /dashboard/api/v3/application-models`.
- An authoritative GOAT usage API or treating its public directory as Key
  verification.
- `/embeddings`, Gemini `embedContent` (501), or Gemini `countTokens` as a
  real upstream count (501 so Gemini CLI can fall back locally).
- Gemini as an upstream protocol.
- Automatic pricing or Zen catalog polling.
- Cross-engine reuse of legacy WebView profiles.
- Database downgrade support or letting an older binary open a newer schema.
- Windows/Linux ARM64, 32-bit x86, RPM, Snap, app-store packages, Windows
  Authenticode, or Apple notarization.
- A second Cosign image signature on top of GitHub provenance.
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](known-debt.zh-CN.md) · [Docs index](../README.md)
