[简体中文](architecture.zh-CN.md)

# Architecture

## Four-layer crates

Provider extension is **static and sealed**. There is no plugin slot, JSON
DSL, or user-defined adapter implementation. Custom API is
`ProviderAdapterKind::ConfigurableHttp`, not a base class other adapters
inherit from.

```text
ocg-gateway -> ocg-domain
ocg-core    -> ocg-domain + ocg-gateway + ocg-infra
ocg-cli     -> ocg-core
src-tauri   -> ocg-core
```

```text
  ocg-domain                      ocg-infra
  IDs, BUILTIN_PLANS,             crypto, proxy HTTP,
  MODEL_PROTOCOLS, Zen            inference HTTP, log SQL
       ^                               ^
       |                               |
  ocg-gateway                          |
  alias, AttemptSpec,                  |
  classify, selector,                  |
  JSON convert (no I/O)                |
       ^                               |
       |                               |
       +---------------+---------------+
                       |
                    ocg-core
           SQLite, CoreState, Dashboard V3,
           GatewayExecutor, adapters, host_router
                       |
             +---------+----------+
             |                    |
          ocg-cli             src-tauri
       ocg-manager-cli        ocg-manager (tray)

  aside: ocg-browser-worker   separate process, no ocg-* deps
         Vue SPA in src/      static assets; talks HTTP V3 only
```

`ocg-domain` and `ocg-infra` have no internal `ocg-*` dependencies.
`ocg-browser-worker` is a separate process with no internal crate dependency.

| Crate | Owns | Must not own |
| --- | --- | --- |
| `ocg-domain` | IDs, `BUILTIN_PLANS`, `ProviderAdapterKind`, protocol tables, Zen ID normalize, account/setup enums | DB, `CoreState`, reqwest, rusqlite, tokio, axum, filesystem, clocks |
| `ocg-gateway` | Alias registry, `AttemptSpec`, classify policy, secret-free selector, whole-document JSON convert | DB, `CoreState`, raw reqwest, rusqlite, axum, plaintext credentials |
| `ocg-infra` | Key obfuscation, catalog-stripped proxy clients, inference HTTP helpers, one-statement log SQL | Product catalogs, `AppConfig`, Dashboard DTOs |
| `ocg-core` | SQLite, `CoreState`, Dashboard V3, provider adapters, `GatewayExecutor`, `forward_once`, usage sync, host composition | Plugin registries; adapters still must not own DB/`CoreState`/raw clients |

`ocg-core` keeps historical public paths as **explicit compatibility
facades** (`alias.rs`, `provider.rs`, `crypto.rs`, `http_client.rs`,
`kernel/{ids,catalog,protocol,zen}.rs`, `gateway/{attempt,classify,protocol,selector}.rs`).
Do not glob-reexport `ocg_domain` / `ocg_gateway` / `ocg_infra`. Production
graph guards in `kernel/mod.rs` require a DAG with **no multi-node SCC**.
`redaction.rs` is a crate-level leaf. `db` does not depend on `pricing` or
`gateway_keys`. `dashboard_v3` does not import `gateway` or `dashboard`.
`account_control`, `gateway_keys`, and `usage_sync` do not name `CoreState`.

`ocg-gateway` production dependencies are exactly `anyhow`, `base64`,
`ocg-domain`, `serde_json`. `ocg-domain` production dependencies are
exactly `chrono` (serde+std only, no clock feature), `serde`, and
`serde_json`.

## ocg-core as composition / control plane

`ocg-core` wires the other crates. It is the only crate that opens SQLite,
holds `CoreStateInner`, mounts HTTP, and talks to upstreams.

- `host_router.rs` is the HTTP composition root: inference router +
  `/dashboard/api/v3` + the retired V2 REST tombstone + dashboard assets.
  `gateway` does not import dashboard mounts.
- `host_gateway.rs` implements `GatewayRebindHost` so `state` can rebind a
  listener without importing `gateway`.
- `gateway_runtime.rs` / `routing_runtime.rs` are DAG leaves that hold
  `GatewayHandle` and routing slots outside both `gateway` and `state`.
- `account_control.rs` is the HTTP-neutral account mutation service.
  Dashboard V3 wraps it with CAS; the CLI calls the same functions without
  an argv CAS token. Both bump `settings_revision` after a successful
  persist.
- `gateway_keys.rs` owns the `access_keys` table and the in-memory
  credential snapshot. Concrete `KeyStore` / `KeyHost` impls live in
  `state`.
- `control/observability.rs` is HTTP-neutral local read logic shared by
  leftover V2 adapters and V3. It never issues outbound HTTP.

```text
                    127.0.0.1:9042
                            |
              host_router.rs (HTTP composition root)
                            |
      +----------+----------+----------+----------+
      |          |          |          |          |
      v          v          v          v          v
  inference   Dash V3    V2 REST    preserved   SPA
  /v1 ...     /dashboard tombstone  V2 auth +   /dashboard
              /api/v3    /dashboard browser WS  /assets
                         /api
                             |
                     anon -> 401
                     session -> 410 dashboardV2Removed

  inference entries
    POST /v1/chat/completions
    POST /v1/responses
    POST /v1/messages
    GET  /v1/models                  local; no upstream
    POST /v1beta|/v1/models/{model}:*
    POST /claude-desktop/v1/messages
    GET  /claude-desktop/v1/models   three role aliases only

  preserved unversioned /dashboard/api
    auth/status | register | login | logout
    browser/sessions/{token}/ws
  SPA auth uses /dashboard/api/v3/auth/...
```

User-facing maps of the same node: [Architecture diagrams](../user/architecture.md).
Route tables: [HTTP routes](http-routes.md).

## Gateway execution

Client inference lives under `crates/ocg-core/src/gateway/`. Axum + Tokio +
reqwest, default bind `127.0.0.1:9042`. Body cap is 16 MiB before auth.

Split of responsibility:

1. **`handler.rs`** — request id (`x-ocg-request-id`), credential auth,
   client parse/format validation, Claude Desktop rewrite, Alias
   resolution. Then it hands a parsed, resolved request to the executor.
2. **`GatewayExecutor`** — frozen request-entry snapshot, candidate
   selection, same-account retry, account fallback. One logical client
   request uses one immutable pricing revision, one `ForwardRouteSet`, one
   contract set, and one Alias resolution from start to finish. Each
   fallback iteration **re-reads** accounts, eligible Custom runtimes, and
   Zen Free cooldown.
3. **`provider_adapter.rs`** — exhaustive match on sealed
   `ProviderAdapterKind`. Returns a data-only `AttemptSpec` (URL, path,
   upstream protocol, auth scheme, redirect policy, opaque
   `CredentialHandle`, `ProxyRoutingModel`). Adapters take an account,
   config, and request plan. They do **not** decrypt keys, open databases,
   or build HTTP clients.
4. **`forwarder.rs` / `forward_once`** — exactly one upstream `.send()`
   per call. Owns transport selection and timeouts only. No policy, no
   retry, no fallback inside `forward_once`.
5. **Host `CredentialResolver`** — decrypts the handle after the outer
   loop has already selected the account.

```text
  handler.rs
    1. x-ocg-request-id ; body cap 16 MiB (before auth)
    2. Key vs credential_snapshot
       Bearer / x-api-key / x-goog-api-key  (first header hit wins)
    3. parse client protocol
    4. Claude Desktop role rewrite (that entry only)
    5. Alias resolve (kebab / raw ID / Custom overlay)
         unknown -> 400
         overlap -> 400 ambiguous_model_id   (no upstream)
                    |
                    v
  GatewayExecutor     frozen at entry:
                      pricing revision, ForwardRouteSet,
                      contracts, Alias resolution
    6. materialize candidates
    7. filter cards + ocg-gateway::selector
       StrictPriority / StickyGlobal / RoundRobin
       fallback iteration re-reads accounts, Custom, Zen cooldown
    8. provider_adapter -> AttemptSpec
       (no decrypt, no DB, no HTTP client)
    9. CredentialResolver decrypts the selected handle
   10. forward_once = exactly one upstream .send()
   11. classify  (not inside forward_once)
         pre-send connect fail -> retry same account once
         403 / Go 429          -> next card
         Free 429              -> cool shared free channel
         Go CreditsError 401   -> rotate + persist auth_error
         other OpenCode 401    -> return as-is
         Custom 401            -> rotate + persist auth_error
         408 / 5xx / body timeout / stream interrupt -> never replay
   12. convert response ; write forward_logs
       requested_model, resolved_alias, upstream_model
       (no requested_alias field)
```

Auth accepts Bearer, `x-api-key`, and `x-goog-api-key`. The first header
that hits `CoreStateInner.credential_snapshot` (primary or enabled sub
key) determines attribution and supplies the forward-log name. Client
credentials are stripped before upstream; only the selected account's
scheme is injected. Gemini or Anthropic client credentials never pass
through. Command Code models may share a client Alias with an OpenCode Go
model, but every mapping retains its own Provider identity and GOAT keys never
reach OpenCode endpoints.

Standard entries are `/v1/chat/completions`, `/v1/responses`,
`/v1/messages`, and `/v1/models`. Claude Desktop uses
`/claude-desktop/v1/messages` and `/claude-desktop/v1/models`. Gemini
accepts `/v1beta/models/{model}:*` and `/v1/models/{model}:*`;
`generateContent` and `streamGenerateContent` convert, `countTokens` and
`embedContent` return `501`, unknown actions return `404`. Authenticated
`GET /v1/models` reads local routeable code-owned Aliases, then eligible
Custom declared IDs. The original OpenCode Go table authorizes Go names;
sealed exact MiniMax CN, Kimi CN, and selected GOAT long-name maps authorize
provider-only names without creating Go routes. Saved Zen catalogs may join Go
Aliases, Command catalogs may join any code-owned Alias, and CN catalogs
activate only their sealed mappings; unmatched built-in catalog IDs
remain exact raw pins.
`GET /dashboard/api/v3/application-models` is a separate local list: Go
routeable aliases ∩ active Go pricing snapshot (highspeed variants
inherit the base row). It excludes Custom IDs. Claude Desktop
`/claude-desktop/v1/models` advertises only the three role aliases.

The Alias registry lives in `ocg-gateway::alias` (facade `ocg_core::alias`).
Preferred aliases are lowercase kebab-case and come from the original Go table
or sealed CN/GOAT maps. Command removes Provider namespaces, strips known plan
suffixes only when the shorter Alias is already code-owned, and maps
`nvidia/nemotron-3-ultra-550b-a55b` to `nemotron-3-ultra`. Kebab Alias spellings are case-folded; built-in raw IDs are exact and
case-sensitive, including names with `/`, `_`, or whitespace. A raw ID with
exactly one registry mapping pins to that mapping;
routability is checked afterward, so an unroutable mapping is recognized
but cannot produce a production route. Overlapping raw IDs return `400`
`ambiguous_model_id` without calling upstream. Unknown names return `400`
on Chat Completions, Responses, Messages, and Gemini generate /
streamGenerate. Eligible Custom IDs overlay resolution and `/v1/models`
but do not steal published Go/Zen aliases. The published kebab
`deepseek-v4-flash` can contain Go, Zen, and Command Code mappings; raw
`deepseek/deepseek-v4-flash` pins to Command Code. Forward logs persist `requested_model`,
`resolved_alias`, `upstream_model`, `provider_id`, and `offering_id`;
there is no `requested_alias` field.

JSON conversion lives in `ocg-gateway::protocol`; the host
`gateway/protocol.rs` keeps parse, usage, stream, and route-identity
types. Gemini is client-only. Known models use `ocg-domain`'s hardcoded
`MODEL_PROTOCOLS`: client protocol in `supported` passes through, otherwise
converts to `preferred`. Unknown models return `400` on every supported
client format; the request path never trials protocols. Non-empty
`safetySettings` return `400`; an empty array is acceptable. `topK` and
`thinkingConfig` are compatibility hints, not Gemini-equivalent behavior
guarantees.

`materialize.rs` parses the client protocol once, resolves the Alias, then
materializes model, protocol, endpoint, and auth per candidate. Adapters
do not probe billable inference paths to discover protocol support. The
OpenCode `MODEL_PROTOCOLS` table is Go-specific. Dynamic Zen `-free` IDs
unknown to the table default to Chat. Custom rematerializes per account to
that card's single declared protocol and resolved inference Endpoint, deriving
the standard path and auth from the API URL plus protocol.

`zen_models.rs` owns the only Zen Free model-discovery path. A protected
Providers-page refresh calls the fixed keyless
`https://opencode.ai/zen/v1/models` endpoint through the global proxy,
follows no redirects, keeps only valid IDs ending in `-free`, and persists
the complete snapshot before swapping runtime state. Each model remains
available by its exact raw ID; it joins the suffix-stripped Alias only when
that Alias is authorized by the original Go table. Failed or empty refreshes
preserve the previous snapshot; `/v1/models` only reads it. Go-owned
`ox-alpha-free` is reserved and excluded.

Selector policy: host `gateway/selector.rs` filters cards by capability,
enabled/ready state, credential validity, cooldown, and request-local
failures, then the secret-free `ocg-gateway::selector` state machine walks
the surviving order using `StrictPriority`, `StickyGlobal`, or
`RoundRobin`. There is no model-routing page or per-model quota pool. Zen
Free quota is shared per egress IP: any active `cooldown_free_until`
exhausts the whole free channel; no key rotation.

Pricing snapshots are immutable and provider-scoped. Refresh is manual.
The Provider path fetches and activates only that Provider's priced
offerings; OpenCode and Command Code keep distinct revision tokens and
last-known-good state so one source failure cannot veto the other. For
OpenCode Go, the allowance only derives the account quota-debit multiplier
(`monthly limit / Usage`); it is not a routable quota pool. Official Go
rows whose Input/Output/Usage cells are all dashes (currently Ox Alpha
Free / `ox-alpha-free`) are skipped as unpriced promos. When official
multipliers differ from active values, the first refresh returns a
non-activating preview; the follow-up is bound to both the active revision
and the previewed official content hash. The fetcher is restricted to the
OpenCode Go HTTPS host, same-host redirects, a 20-second deadline, and a
2 MiB body. MiniMax context, priority, and high-speed adjustments are
local policy.

Command Code GOAT request accounting reads only its latest verified
provider-scoped snapshot. A uniquely matched model row produces raw USD cost,
quota debit (`raw cost × multiplier`), and paid-plan equivalent; missing,
ambiguous, or unverified rows remain unpriced and never inherit Go pricing.
The applied multiplier is editable through the provider-scoped pricing write
and is persisted as a new snapshot revision. GOAT reuses the provider-agnostic
account window projector: priced OCG logs accumulate against `$14 / $35 / $70`,
with an optional manual baseline correction. Unpriced and external requests
remain absent, and old log rows are not retroactively repriced.

Fallback / retry (executor + classify, **not** `forward_once`):

- Only a pre-send DNS/TCP/TLS connection failure can retry **once** on the
  same account, and only before any downstream bytes.
- Partial SSE never falls back. Ambiguous stream results log
  `outcome_unknown`. `StreamOutcomeGuard` finalizes on drop.
- OpenCode Go inference `401` rotates and persists `auth_error` only for an
  exact structured `CreditsError`; `ModelError` and unknown/malformed 401s,
  plus every Zen Free 401, are returned as-is.
  Ordinary Custom `401` rotates and persists `auth_error`. Dashboard ping
  / key verification still record `auth_error` on 401.
- `403` and Go-channel `429` can select another account. Free-channel
  `429` cools the IP-shared free pool and does not rotate keys; routing
  continues with later compatible cards. Generic Custom/GOAT `429` does
  not parse Go windows.
- `408`, `5xx`, post-connect failures, body timeouts, and stream
  interruptions are never replayed.
- Shared reqwest client: 30-second connect timeout; non-stream 900-second
  total deadline; streams 300-second idle per chunk.

`ProxyRoutingModel` on `AttemptSpec`:

- `RequestEntrySnapshot` — frozen dual-leg `ForwardRouteSet` (Go / Zen).
  Follows redirects. Restricted URL (https or loopback http).
- `ProcessWideNoRedirect` — Command Code's fixed-origin public catalog and
  inference transport; redirects are disabled.
- `IsolatedTrustedAdmin` — Custom: process-wide proxy, no redirects, no
  client-header forwarding, administrator-trusted URL.

Global outbound proxy is process-wide (`AppConfig`): Auto, Manual HTTP,
Direct, or List. List mode uses `proxy_list_direction` and
`proxy_list_models`. Listed models take the exception leg (whitelist →
proxy, blacklist → direct); unlisted models and non-model outbound
(verify, Zen refresh, usage, pricing, updater) take the default leg.
Membership is validated only on dashboard `update_settings` (non-empty,
exact known id, de-duplicated); load tolerates old values. Construction
lives in `ocg-infra::http`; `ocg-core::http_client` folds catalog aliases
before exact match. A request keeps its entry `ForwardRouteSet`;
concurrent settings changes affect only later requests.

```text
  AppConfig  (process-wide)
    Auto | Manual HTTP | Direct | List

  List mode
    listed model id  -> exception leg
      whitelist: proxy
      blacklist: direct
    unlisted model, and non-model outbound
      (verify, Zen refresh, usage, pricing, updater)
      -> default leg
      whitelist: direct
      blacklist: proxy

  membership validated only on dashboard PUT /settings
  (non-empty, exact known id, de-duplicated); load tolerates old values

  in-flight request keeps the entry ForwardRouteSet

  AttemptSpec.proxy_routing
    RequestEntrySnapshot     Go / Zen ; follows redirects
    IsolatedTrustedAdmin     Custom ; no redirects ; no client-header forward
    ProcessWideNoRedirect    Command Code fixed origin ; no redirects
```

## Plan catalog

`BUILTIN_PLANS` and `ProviderAdapterKind` live in `ocg-domain::provider`
(facade `ocg_core::provider`). Six families:

| Family | IDs | Routable | Notes |
| --- | --- | --- | --- |
| OpenCode Go | `opencode` / `go` | yes | Official keys only |
| Zen Free | `opencode-zen-free` / `anonymous-free` | yes | Credentialless singleton, DB-owned |
| Command Code GOAT | `command-code` / `goat` | yes | Fixed official origin; public Provider catalog; model supply controlled by the Provider matrix |
| MiniMax CN Token Plan | `minimax` / `cn-token-plan` | yes | Fixed CN Chat Completions and Anthropic Messages origins |
| Kimi Code CN | `kimi-code` / `cn` | yes | Fixed CN Chat Completions and Anthropic Messages paths |
| Custom API | `custom` / `api` | yes | Trusted-admin destination |

Every persistent mutation path rejects `enabled=true` for a
`routable=false` offering before touching the row, revision, or timestamps.
On every `Database::open`, historical Command Code verification states are
normalized to `not_required`, because its public catalog is not Key
verification. Enabled state is preserved. Go, Zen Free, GOAT, MiniMax, Kimi,
Custom, and unknown
pairs are otherwise untouched.

Custom API (`custom.rs` + `custom_http.rs`) accepts one syntactically valid
HTTP/HTTPS API URL. A root gains `/v1` plus the selected protocol path, a base
already ending in `/v1` does not duplicate it, and compatible complete
Endpoints remain valid. URL-embedded credentials, query, fragment, and
redirects are rejected. It does not forward dashboard/client auth.
Chat/Responses derive Bearer auth and Messages derives `x-api-key`.
The account declares one protocol, uniform across all models and used as the
effective preferred protocol; other supported client formats convert to it.
Config and the complete capability list update atomically. Verification is
optional and sends one minimal request to the resolved inference URL. Standard
protocol suffixes alone may derive `/models`; other paths require manual model
entry. Editing the Key, Endpoint, declared capabilities, or protocol resets
`verification_status` to `pending` but keeps the account enabled. Custom
costs and usage are unpriced/unknown with no provider quota debit.


## Control plane

The Vue SPA is the only live dashboard client. It talks HTTP Dashboard V3.
The CLI calls the same mutation services without an argv CAS token. There
is no Tauri `invoke` path.

```text
  Vue 3  (eight views, KeepAlive)
    Pinia: session / controlPlane / connection
           accounts / providers / settings
           |
           |  src/api/dashboard-v3.ts
           |  src/api/dashboard.ts
           |  src/api/providers.ts
           v
  /dashboard/api/v3
    public:  /auth/status|register|login|logout
    else:    dashboard session
             loopback skips login unless forwarding headers
           |
           |  CAS expectedRevision + processGeneration
           |  pricing writes also expectedPricingRevision
           |  GET /contract = live tokens, not schema export
           |  GET /connection = only V3 DTO with plaintext Key
           v
  account_control / gateway_keys / settings / ...
           |
           v
  SQLite schema v35
           ^
           |
  ocg-manager-cli  same services, no argv CAS
```

409 `revisionConflict` refreshes tokens; the SPA does not auto-replay the
mutation. CAS details: [Dashboard API](dashboard-api.md).

## Persistence map

Authoritative schema is v34. `sub_gateway_keys` exists only in pre-v27
databases and is dropped by the migration. GUI data dir is
`%USERPROFILE%\.ocg-mgr` on Windows and `~/.ocg-mgr` elsewhere; CLI
defaults to `~/.ocg-mgr-cli`.

```text
  data.sqlite                         CURRENT_SCHEMA_VERSION = 35
    access_keys                       Primary id PRIMARY_KEY_ID
                                      cannot disable/delete Primary
                                      64 active sub-key cap
    accounts                          one card = one Plan
    settings                          AppConfig (gateway_key stored "")
    forward_logs                      requested_model, resolved_alias,
                                      upstream_model, route, provider_id
    gateway_logs
    provider_pricing_snapshots
    provider_usage_sync_state         official Go usage metadata
    provider_model_catalogs
    provider_contract_scopes          deprecated scope-level switch columns;
                                      no longer read by effective derivation
    provider_contract_model_protocols model-protocol evidence
    provider_contract_model_protocol_overrides
                                      per-model/per-protocol override state
    account_custom_configs
    account_model_capabilities
    account_acknowledgements

  existing non-empty DB: non-overwriting
    data.sqlite.pre-v3.<UTC>.bak + .sha256
    before any v27 write
  empty new DB: v31 directly, no that copy

  keys obfuscated; ConnectionInfo is the only V3 plaintext-Key DTO
```

Upgrade, backup hash, rollback: [Storage and migrations](storage-migration.md).

## Usage calibration

Official Go usage is a periodic baseline. Local `forward_logs` remain the
real-time estimator after the last successful calibration. Quota bars do
not stop traffic.

```text
  official  GET https://opencode.ai/zen/go/v1/usage
            calibration baseline (never auto-polled from the SPA)

  local     forward_logs after last success
            live estimate on the account card

  background (Gateway start spawns; CoreState drop stops)
    ready+enabled + local activity in 24h  ~ hourly
    ready+enabled, idle                   ~ daily
    disabled / not ready / empty key      no auto refresh
    local Go usage >= 80%                 expedite, min 15 min
    inference 429                         schedule official in 1-2 min
                                          (not inline; official failure
                                           never writes cooldown)
    failure backoff  5m -> 15m -> 1h -> 6h
    global concurrency 1; startup spread, no stampede

  manual  POST /dashboard/api/v3/accounts/{id}/usage/refresh
          15 s throttle (success and failure)
```

Locks, clocks, and credential snapshot: [State and lifecycle](state-and-lifecycle.md).

---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](architecture.zh-CN.md) · [Docs index](../README.md)
