[English](http-routes.md)

# HTTP 路由

所有路由共享一个端口：推理、Dashboard V3、V2 墓碑与 SPA。详见[架构](architecture.zh-CN.md)。

## 推理（路径未改）

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `/v1/chat/completions` | OpenAI Chat |
| POST | `/v1/responses` | OpenAI Responses（无状态；`store` / `previous_response_id` / `conversation` / `background` → 400） |
| POST | `/v1/messages` | Anthropic Messages |
| GET | `/v1/models` | 本地列表；需要鉴权 |
| POST | `/claude-desktop/v1/messages` | 角色别名改写后走 Messages |
| GET | `/claude-desktop/v1/models` | 三个角色别名 |
| POST | `/v1beta/models/{model}:*` 与 `/v1/models/{model}:*` | Gemini 客户端格式 |

## Dashboard V3（`/dashboard/api/v3`）

公开：`/auth/status`、`/auth/register`、`/auth/login`、`/auth/logout`。

会话保护（非穷尽；见 `dashboard_v3/mod.rs`）：`/contract`、`/connection`、 `/settings`、`/settings/test-proxy`、`/claude-desktop/models`、 `/settings/check-update`、`/settings/update-status`、 `/settings/install-update`、`/providers/{provider_id}/pricing`、 `/providers/{provider_id}/pricing/refresh`、 `/providers/{provider_id}/pricing/multipliers`、`/keys`、`/keys/primary/regenerate`、`/keys/{id}`、`/keys/{id}/regenerate`、 `/accounts`、`/accounts/managed`、`/accounts/order`、`/accounts/{id}`、 `/accounts/{id}/toggle`、`/accounts/{id}/browser`、 `/accounts/{id}/browser-profile`、`/accounts/{id}/setup`、 `/accounts/{id}/setup/verify-key`、`/accounts/{id}/reset-cooldown`、 `/accounts/{id}/custom-config`、`/accounts/{id}/model-capabilities`、 `/accounts/{id}/acknowledgements`、`/accounts/{id}/usage`、 `/accounts/{id}/usage/refresh`、`/accounts/{id}/provider-usage`、 `/accounts/{id}/verify`、`/providers`、`/providers/{provider_id}`、`/providers/models/discover`、`/providers/test`、`/providers/model-capabilities`、 `/providers/zen-free`、`/providers/zen-free/models`、 `/providers/zen-free/models/refresh`、 `/providers/{provider_id}/models/refresh`、`/provider-contracts`、 `/provider-contracts/provider/{scope_id}/model-protocol-overrides`、 `/provider-contracts/custom-endpoint/{scope_id}/model-protocol-overrides`、 `/providers/{provider_id}/protocol-probes`、`/browser/capabilities`、 `/browser/sessions/{token}/ws`、`/gateway/status`、 `/application-models`、`/dashboard/summary`、 `/dashboard/daily-tokens-by-model`、`/logs/gateway`、`/logs/forward`、 `/logs/forward/models`、`/logs/forward/keys`、 `/custom/models/discover`。

Go/Zen 协议探测是 `POST /providers/{provider_id}/protocol-probes`。Custom 在该路径被拒绝（`protocol probes for Custom API are account-owned`）。历史 V2 `POST /accounts/{id}/protocol-probes` 为 410。Custom 连接验证是 `POST /accounts/{id}/verify`；模型发现是操作探测 `POST /custom/models/discover`。用户定义供应商使用 `POST /providers`、`GET|PATCH|DELETE /providers/{provider_id}`、`POST /providers/models/discover` 与 `POST /providers/test`。发现与测试从不阻挡保存；真实测试可能消耗上游额度。

## 静态面板

`GET /dashboard`、`GET /dashboard/`、`GET /dashboard/assets/{*path}`。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](http-routes.md) · [文档索引](../README.zh-CN.md)
