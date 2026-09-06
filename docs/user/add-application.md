[简体中文](add-application.zh-CN.md)

# Add an Application

Use this guide when a client is missing from **Applications**. If the app accepts a custom base URL, API Key, and model ID for one supported protocol, connect it manually first. A first-class guide card and a Desktop automatic connector are separate, optional contributions.

| Goal | Integration level |
| --- | --- |
| Use an unlisted app now | Manual Gateway configuration |
| Publish copy-ready instructions in **Applications** | Add an `ApplicationGuide` |
| Let the installed Desktop app edit that client's local configuration | Add a static Desktop connector after the manual guide works |

## Connect an unlisted client

Copy the **Key** and URLs from **Connection Center**. Choose the interface the client already supports:

| Client protocol | Typical base value | OCG Manager request path | Authentication |
| --- | --- | --- | --- |
| OpenAI Chat Completions | `http://127.0.0.1:9042/v1` | `POST /v1/chat/completions` | `Authorization: Bearer <key>` |
| OpenAI Responses | `http://127.0.0.1:9042/v1` | `POST /v1/responses` | `Authorization: Bearer <key>` |
| Anthropic Messages | `http://127.0.0.1:9042` when the client appends `/v1/messages` | `POST /v1/messages` | `x-api-key: <key>` |
| Gemini | `http://127.0.0.1:9042` with API version `v1beta` | `POST /v1beta/models/{model}:generateContent` or `:streamGenerateContent` | `x-goog-api-key: <key>` |
| Claude Desktop Gateway | `http://127.0.0.1:9042/claude-desktop` | `POST /claude-desktop/v1/messages` | Static API key / Bearer |

If a client asks for a **complete endpoint** instead of a base URL, use the request path shown above. If it automatically adds `/v1`, give it the root; if it expects an OpenAI API base, usually give it the `/v1` base. The client's official documentation decides which form is correct.

Use an exact model returned by authenticated local discovery:

```bash
curl http://127.0.0.1:9042/v1/models \
  -H "Authorization: Bearer <key>"
```

This list is a local read of currently routeable code-owned Aliases and eligible Custom IDs. The dashboard application picker is the Go-routeable intersection in [Application guides](applications.md). Minimal request bodies for all five interfaces are in [Connect your first client](first-client.md).

After configuration, send one real request and check **Logs**.

## Add an Applications guide

The guide registry lives in `src/views/application-guides.ts`. A guide is presentation and copy-ready configuration.

1. Verify the client's current official documentation and choose one `endpointKind`: `chat`, `responses`, `messages`, or `gemini`.
2. Add a stable kebab-case `id`, display name, category, protocol, official URL, short summary, ordered steps, operational notes, and one or more generated snippets.
3. Build snippets from `GuideContext` URLs and model selections. Use `displayKey` for the visible preview and `actualKey` only for the copied value through the existing keyed-snippet helper; never place a complete Key in logs, tests, labels, or static source.
4. Use `modelFields` only for client-owned model settings. Use `multipleModels` only when the client genuinely consumes several selected models. Add a quick action only when it is safe and supported by the current UI.
5. Update `src/views/dashboard-connection.test.ts`: the expected catalog size, unique ID check, official documentation URL, redaction assertions, and client-specific output assertions. Add or update translations used by the guide.
6. Run `pnpm run test:web` and `pnpm run build:web`. If model capability metadata changed, also synchronize the capability table in [Application guides and model capabilities](applications.md).

A good guide tells the user exactly which URL form the client expects, where the Key is stored, which protocol is used, how to select a model, whether restart is required, and how to prove the first request in OCG logs.

## Add an automatic Desktop connector

Automatic configuration is a local installed-Desktop capability for a small static set of clients. CLI, Docker, and remote dashboards use the manual guides. Add one only when the target has stable, documented configuration ownership and connect/restore can preserve every unrelated field.

The local dashboard uses these session-protected V3 endpoints:

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/dashboard/api/v3/applications/connectors` | Inspect the static connector set without returning Keys |
| `POST` | `/dashboard/api/v3/applications/connectors/{id}/preview` | Produce a redacted connect/restore plan and file fingerprint |
| `POST` | `/dashboard/api/v3/applications/connectors/{id}/commit` | Apply the exact preview under CAS and fingerprint checks |

Preview accepts only `action`, optional `keyId`, and `modelValues`. Callers cannot supply target paths, Gateway URLs, config text, or plaintext Key material. Commit additionally requires `expectedRevision`, `processGeneration`, and `previewFingerprint`. A stale revision or changed target file must be shown to the user; it must not be auto-replayed.

Implementation ownership is split deliberately:

- Add the static connector identity and secret-free DTO behavior in `crates/ocg-core/src/application_connectors.rs`.
- Implement fixed target detection, field-level merge, preview, atomic commit, restore, permissions, and failure recovery in `src-tauri/src/host/application_connectors.rs`, then register the Host capability in `CoreState`.
- Keep the explicit connector sets and UI state handling in `src/views/Applications.vue` synchronized. Native Pi/DSH packages belong under `integrations/` and follow their client-native credential rules.
- Add Core/V3, Desktop Host, and frontend tests. Run `cargo test -p ocg-core`, `cargo test -p ocg-manager --lib`, `pnpm run test:web`, and `pnpm run build:web`; run `pnpm run contract:v3:check` if the frozen DTO contract changes.

The supported path is a static Desktop connector over those session-protected V3 preview and commit endpoints. The manual guide remains available whenever automatic detection or writing is unsupported. Repository constraints live in [Extending OCG Manager](../maintainer/extending.md) and [Coding conventions](../maintainer/conventions.md).

---

[User guide index](../USER.md) · [简体中文](add-application.zh-CN.md) · [Add a provider](add-provider.md) · [Docs index](../README.md)
