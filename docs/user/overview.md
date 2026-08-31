[简体中文](overview.zh-CN.md)

# What OCG Manager Does

OCG Manager is a local gateway that stores provider API keys in a SQLite
database — including supported provider-plan keys and trusted Custom API
destinations — and exposes a loopback gateway at `http://127.0.0.1:9042/v1`.
Each account card is one **Plan** (provider + offering). Clients send
**aliases** from the local registry or eligible Custom model IDs; live routing
is OpenCode Go, Zen Free, Command Code GOAT, MiniMax CN Token Plan, Kimi Code
CN, and Custom API. The Vue 3 dashboard is at
`/dashboard/` and the current SPA talks JSON at `/dashboard/api/v3`. Every node
is independent: no remote sync, no Admin API, no telemetry.

The gateway does four jobs, in roughly the order you would expect:

1. Authenticate the client with the **Key** issued by the dashboard.
2. Resolve the requested model against the local Alias registry (and eligible
   Custom declared IDs), then pick a usable account card after capability
   filtering, the adapter ceiling, the saved provider contract, and the
   per-model protocol effective state.
3. Convert the request to the selected Plan's effective upstream protocol,
   and the response back to the client protocol. Client requests never
   discover or probe.
4. Log the request (`requested_model`, `resolved_alias`, `upstream_model`),
   write usage and any cooldown to SQLite, and surface everything in the
   dashboard.

## Shape of a node

Desktop, CLI, and Docker all run one `ocg-core` process on `127.0.0.1:9042`.
The dashboard opens in your system browser; clients hit `/v1` in OpenAI,
Anthropic, Gemini, or Claude Desktop format.

```text
   Desktop tray / CLI `serve` / Docker
                    |
                    v
              ocg-core @ 127.0.0.1:9042
               /                    \
    /dashboard/  Vue SPA          /v1  inference
    (system browser)              clients + Key
               \                    /
                v                  v
              SQLite schema v34 (local only)
```

Text diagrams for request flow, Plans, the seven dashboard views, and protocol
conversion: [Architecture diagrams](architecture.md).

---

[User guide index](../USER.md) · [简体中文](overview.zh-CN.md) · [Docs index](../README.md)
