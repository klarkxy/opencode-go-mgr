[简体中文](http-routes.zh-CN.md)

# HTTP Routes

All routes share one port: inference, Dashboard V3, V2 tombstone, and SPA. See [Architecture](architecture.md).

## Inference (unchanged paths)

| Method | Path | Notes |
| --- | --- | --- |
| POST | `/v1/chat/completions` | OpenAI Chat |
| POST | `/v1/responses` | OpenAI Responses (stateless; `store` / `previous_response_id` / `conversation` / `background` → 400) |
| POST | `/v1/messages` | Anthropic Messages |
| GET | `/v1/models` | Local list; auth required |
| POST | `/claude-desktop/v1/messages` | Role alias rewrite then Messages |
| GET | `/claude-desktop/v1/models` | Three role aliases |
| POST | `/v1beta/models/{model}:*` and `/v1/models/{model}:*` | Gemini client format |

## Dashboard V3 (`/dashboard/api/v3`)

Public: `/auth/status`, `/auth/register`, `/auth/login`, `/auth/logout`.

Session-protected (non-exhaustive; see `dashboard_v3/mod.rs`):
`/contract`, `/connection`, `/settings`, `/settings/test-proxy`,
`/claude-desktop/models`, `/settings/check-update`,
`/settings/update-status`, `/settings/install-update`,
`/providers/{provider_id}/pricing`,
`/providers/{provider_id}/pricing/refresh`,
`/providers/{provider_id}/pricing/multipliers`, `/keys`,
`/keys/primary/regenerate`, `/keys/{id}`, `/keys/{id}/regenerate`,
`/accounts`, `/accounts/managed`, `/accounts/order`, `/accounts/{id}`,
`/accounts/{id}/toggle`, `/accounts/{id}/browser`,
`/accounts/{id}/browser-profile`, `/accounts/{id}/setup`,
`/accounts/{id}/setup/verify-key`, `/accounts/{id}/reset-cooldown`,
`/accounts/{id}/custom-config`, `/accounts/{id}/model-capabilities`,
`/accounts/{id}/acknowledgements`, `/accounts/{id}/usage`,
`/accounts/{id}/usage/refresh`, `/accounts/{id}/provider-usage`,
`/accounts/{id}/verify`, `/providers`, `/providers/{provider_id}`,
`/providers/models/discover`, `/providers/test`, `/providers/model-capabilities`,
`/providers/zen-free`, `/providers/zen-free/models`,
`/providers/zen-free/models/refresh`,
`/providers/{provider_id}/models/refresh`, `/provider-contracts`,
`/provider-contracts/provider/{scope_id}/model-protocol-overrides`,
`/provider-contracts/custom-endpoint/{scope_id}/model-protocol-overrides`,
`/providers/{provider_id}/protocol-probes`, `/browser/capabilities`,
`/browser/sessions/{token}/ws`, `/gateway/status`,
`/application-models`, `/dashboard/summary`,
`/dashboard/daily-tokens-by-model`, `/logs/gateway`, `/logs/forward`,
`/logs/forward/models`, `/logs/forward/keys`,
`/custom/models/discover`.

Go/Zen protocol probes are `POST /providers/{provider_id}/protocol-probes`.
Custom is rejected there (`protocol probes for Custom API are account-owned`).
The historical V2 `POST /accounts/{id}/protocol-probes` is 410. Custom
connection verify is `POST /accounts/{id}/verify`; model discovery is the
operational `POST /custom/models/discover`. User-defined Providers use
`POST /providers`, `GET|PATCH|DELETE /providers/{provider_id}`,
`POST /providers/models/discover`, and `POST /providers/test`. Discovery and
test never gate save; a real test may consume upstream quota.

## Static dashboard

`GET /dashboard`, `GET /dashboard/`, `GET /dashboard/assets/{*path}`.
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](http-routes.zh-CN.md) · [Docs index](../README.md)
