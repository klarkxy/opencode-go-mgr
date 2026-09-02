# AGENTS.md — ocg-manager

This file is for AI coding assistants. Treat the current code as the source of truth; do not fill in non-existent things based on old READMEs, stale user/maintainer docs, or V2 requirements.

User guides live under `docs/user/` (landing: `docs/USER.md` / `docs/USER.zh-CN.md`); the root `README.md` is only the repo entry point. Maintainer guides are indexed at `docs/MAINTAINER.md`. Detailed runtime semantics (gateway routing, aliases, Zen Free, plan catalog, access keys, proxy, usage sync, CI/container) live in `docs/maintainer/runtime-invariants.md` — read it before touching those areas.

## Project facts

- Product: OCG Manager, a local multi-Plan operations console. Routable: OpenCode Go, Zen Free, Command Code GOAT, MiniMax CN Token Plan, Kimi Code CN, Custom API, and user-defined Providers created on Providers; CPA is a separately routable static external integration when configured and enabled. Command Code uses one Provider-level model/protocol semantic: GOAT preset rows default on, additional discovered rows default off, the public `/models` catalog is not Key verification, and there is no account GOAT/All or Max mode.
- Workspace crates: `ocg-domain` (identity/catalog/protocol tables, sealed adapter implementations with typed Provider definitions), `ocg-infra` (key obfuscation, outbound/inference HTTP, SQLite log statements), `ocg-gateway` (no-I/O alias, selector, protocol conversion), `ocg-core` (process host: SQLite, gateway execution, Dashboard V3, V2 tombstones, usage sync, Custom, dynamic Provider persistence, and static external-integration adapters). Host binaries: `crates/ocg-cli`, `crates/ocg-browser-worker`, `src-tauri` (package `ocg-manager`). Provider and Plan are one product identity keyed by `provider_id`; unknown provider IDs fail closed. **Adapter Registry is static/sealed; typed Provider definitions may be dynamically extended.** Every user-defined Provider binds the code-owned Configurable HTTP adapter. Do not load user scripts, plugins, or binaries. Custom API remains a distinct account-owned path. Static external integrations are a separate product path; they are not dynamic Provider plugins.
- Frontend: Vue 3 + TypeScript + naive-ui in `src/`; the production SPA talks HTTP `/dashboard/api/v3` only. The dashboard data path is `src/api/dashboard-v3.ts` (transport) + `src/api/dashboard.ts` / `src/api/providers.ts` (presentation) + `src/api/generated/dashboard-v3.ts` (contract types); shared view/component logic lives in `src/domain/`.
- Dashboard navigation has eight fixed core views in this order: Dashboard / Access Keys / Accounts / Providers / Aliases / Applications / Logs / Settings. A divider below Settings starts the optional **Extensions** group; CPA is its local-only entry. `browser` is a managed-session overlay page. Access credentials are shown as **Key** (never “Gateway Key”); the design system is governed by `DESIGN.md` + `src/theme.ts`.
- One default port `127.0.0.1:9042` (composition root `crates/ocg-core/src/host_router.rs`) merges inference entrypoints (OpenAI Chat Completions / Responses, Anthropic Messages, Gemini `generateContent`, Claude Desktop aliases), Dashboard V3, V2 REST tombstones, preserved V2 auth + browser WebSocket, and `/dashboard` static assets.
- Dashboard V3 control-plane writes require CAS (`expectedRevision` / `processGeneration`); the frozen contract is `schema/dashboard-api-v3.schema.json`. Tombstoned V2 REST returns `410` after auth; never add handlers there.
- Access credentials are SQLite `access_keys` rows (current schema v35); plaintext leaves the process only via the session-protected `GET /dashboard/api/v3/connection`. Persistence details: `docs/maintainer/storage-migration.md`.
- Desktop: Tauri v2 tray app; Host capabilities are registered into `CoreState`, never as `#[tauri::command]`. No remote sync or Admin API; each node is managed by its own dashboard.
- Sources of truth: protocol tables `ocg-domain/src/protocol.rs`, capability table `src/views/application-guides.ts`, aliases `ocg-gateway/src/alias.rs`, Plan catalog `ocg-domain/src/provider.rs`. USER guides mirror them; do not pad the README with those tables.

## Key files

- `crates/ocg-domain/src/provider.rs`: sealed adapter implementations, `BUILTIN_PROVIDERS`, and interactive-use limits. Provider/Plan identity is `provider_id` only.
- `crates/ocg-domain/src/dynamic.rs` and `crates/ocg-core/src/dynamic.rs`: typed user-defined Provider definitions bound to Configurable HTTP.
- `crates/ocg-core/src/dashboard_v3/dynamic_providers.rs`: V3 create/update/delete/discover/test for user-defined Providers.
- `crates/ocg-domain/src/protocol.rs`: `MODEL_PROTOCOLS` and client/upstream protocol identities.
- `crates/ocg-domain/src/ids.rs`: `PRIMARY_KEY_ID` and Plan/account identity constants.
- `crates/ocg-gateway/src/alias.rs`: client Alias registry and raw-ID resolution (`ocg-core` `alias.rs` is the facade; it re-exports `resolve_with_catalogs`, `resolve_with_all_catalogs`, and `published_routeable_aliases_with_all_catalogs`).
- `crates/ocg-gateway/src/protocol.rs` / `selector.rs`: no-I/O whole-document conversion and selector state machines.
- `crates/ocg-infra/src/http.rs`: `ForwardRouteSet`, default + exception segments, `client_for` routing.
- `crates/ocg-infra/src/crypto.rs`: Key obfuscation implementation (`ocg-core` `crypto.rs` is the facade).
- `crates/ocg-core/src/host_router.rs`: HTTP composition root for inference + V3 + V2 tombstones + static assets.
- `crates/ocg-core/src/dashboard_v3/`: `/dashboard/api/v3` used by the current Vue dashboard.
- `crates/ocg-core/src/dashboard.rs`: SPA static assets, preserved V2 auth/browser WS. The tombstoned V2 REST middleware lives in `host_router.rs`.
- `crates/ocg-core/src/gateway/`: OpenAI / Anthropic / Gemini client protocol routes and conversions, Claude Desktop alias rewriting, forwarding, cooldown, and cost accounting. `materialize.rs` parses the client protocol first, then materializes candidates by Alias mapping; adapters must not use billable paths to probe protocols.
- `crates/ocg-core/src/provider.rs`: domain catalog compatibility facade.
- `crates/ocg-core/src/provider_contracts.rs`: provider contract scopes, per-model/per-protocol overrides, effective contract derivation, and model protocol evidence.
- `crates/ocg-core/src/goat.rs`: Command Code public catalog refresh and account runtime types.
- `crates/ocg-core/src/custom.rs` / `custom_http.rs`: Custom eligible runtime, API URL validation/resolution, verification probe, declared-model matching, and outbound boundaries.
- `crates/ocg-core/src/gateway_keys.rs`: `access_keys` lifecycle implementation, credential snapshot, `PRIMARY_KEY_ID`, cross-layer value-uniqueness gate.
- `crates/ocg-core/src/http_client.rs`: maps `AppConfig` to `ocg_infra::http`.
- `crates/ocg-core/src/go_usage.rs`: official Go usage client (`/zen/go/v1/usage`).
- `crates/ocg-core/src/usage_sync.rs`: adaptive official usage sync. Background loop starts/stops with `CoreState` (spawned on Gateway start, exits when CoreState drops).
- `crates/ocg-core/src/db.rs`: SQLite schema, migrations, queries; `CURRENT_SCHEMA_VERSION = 35`.
- `crates/ocg-core/src/models.rs`: shared serde types and `AppConfig` (includes `DEFAULT_OPENCODE_INVITE_URL`).
- `crates/ocg-core/src/pricing.rs` + `kernel/pricing.rs`: OpenCode Go price snapshot, multipliers, and quota estimation.
- `crates/ocg-cli/src/main.rs`: CLI `serve`, `key`, `status` (writes Database directly, does not go through dashboard CAS).
- `src-tauri/src/lib.rs`: Tauri startup, Gateway startup, tray; no invoke commands.
- `src-tauri/src/host/`: desktop Host capabilities (gateway lifecycle, native browser, desktop settings).
- `src-tauri/src/updater.rs`: signed desktop updater bridge; triggered by protected dashboard HTTP API.
- `src-tauri/src/tray.rs`: tray menu and dashboard-open logic.
- `src/api/dashboard-v3.ts` / `dashboard.ts` / `providers.ts` / `generated/dashboard-v3.ts`: current dashboard HTTP client (transport), presenters, and contract types. `dashboardApi` covers auth/keys/acknowledgements; `providerApi` covers Zen-free/pricing/model-protocol-override methods.
- `src/domain/`: shared domain helpers used by both views and components (plans, account-*, pricing-*, provider-contracts, accounts-usage, custom-account, dynamic-provider, managed-account, etc.).
- `src/components/DynamicProviderModal.vue`: Providers-page wizard for user-defined Providers.
- `src/views/`: Dashboard / Keys / Accounts / Providers / Aliases / Applications / Logs / Settings.
- `src/components/ManagedAccountWizard.vue`: managed registration wizard (step back, Google/GitHub).
- `src/views/application-guides.ts`: application tutorial registry and `APPLICATION_MODEL_METADATA` capability table (when changing entries/protocol/redaction/capabilities, sync tests and the USER capability table in `docs/user/applications.md`; the README only keeps recommended-protocol groups).
- `src/theme.ts` + `DESIGN.md`: theme tokens and design spec; change colors/font sizes in both.
- `schema/dashboard-api-v3.schema.json` + `scripts/dashboard-v3-contract.mjs`: V3 contract generation/validation (`pnpm run contract:v3:check`).
- `vite.config.ts`: `build.target`/`esbuild` must support top-level await (`@novnc/novnc`).
- `docs/`: user guides (`docs/user/`), maintainer guides (`docs/maintainer/`), anti-abuse statements, CONTRIBUTORS, and documentation index pages. The root `README.md` is the landing page, not the authoritative copy of capability/protocol tables.

## Common commands

```powershell
pnpm install
pnpm run hooks:install   # once per clone; enables pre-commit cargo fmt
pnpm run dev
pnpm run build:web
pnpm run contract:v3:check
pnpm run test
pnpm run design:lint
pnpm run release:check
pnpm run build
```

Before developing, quit the release tray app to release the single-instance lock and free port `9042`, then run `pnpm run dev`. Tauri starts Vite and opens `http://127.0.0.1:30001/dashboard/` once the Gateway is ready; the frontend is hot-reloaded by Vite, Rust is incrementally compiled and restarted by Cargo. Dashboard JSON goes through `/dashboard/api/v3`.

`pnpm run build` is only for the final release build of the current native platform, and atomically replaces `release/` on success; use `pnpm run build:web` when only validating the frontend. Windows ships only x64 NSIS installer, macOS ships Universal DMG, Linux x64 ships AppImage and deb; the CLI archive must include sibling `dist/` and `LICENSE`. Local smoke-build steps and signing env vars: `docs/maintainer/development.md`; full release process: `docs/maintainer/releasing.md`.

## Development constraints

- The workspace may be a dirty tree. Run `git status --short` first; do not revert changes that are not yours.
- Complexity control is subordinate to complete delivery: prefer reusing existing code and simple architecture, but do not omit processes, states, error handling, or UX explicitly required by the requirements.
- Do not add new Tauri `invoke` frontend paths, and do not restore `src-tauri/src/commands/`. The current desktop Host does not register WebView commands; the dashboard's main path is HTTP `/dashboard/api/v3`.
- Do not add new handlers to retired `/dashboard/api` REST. When changing the dashboard contract, modify `dashboard_v3` + `schema/dashboard-api-v3.schema.json`, and run `pnpm run contract:v3:check`.
- Do not skip security boundaries: Gateway auth, key-storage obfuscation, HTTP URL validation, cooldown state writes, and SSE pass-through must not be removed for simplification.
- Do not reintroduce remote sync; remote nodes are managed through their own dashboards.
- `auto_start` is only available in Windows release/installed Tauri desktop processes; the HTTP dashboard shows the toggle based on runtime capability; dev builds, CLI, Docker, macOS, and Linux do not expose this setting.
- `show_dock_icon` is only available in macOS Tauri desktop processes; when turned off the menu-bar tray icon remains. Windows, Linux, CLI, and Docker do not expose this setting.
- When editing docs, paired guide pages are English-first with a `.zh-CN.md` twin per page under `docs/user/` and `docs/maintainer/`; keep paths and TOCs consistent across each EN/ZH pair. User-visible facts are governed by code and the user guides in `docs/user/` (index: `docs/USER.md` / `docs/USER.zh-CN.md`). Do not write pass-through matrices, capability tables, or long circuit-breaker essays back into the README. Custom API is a live trusted-admin route with one API URL and one account-level upstream protocol; a root URL gets `/v1` plus the selected protocol path, an existing `/v1` is not duplicated, and legacy complete Endpoints remain compatible. That protocol is the preferred conversion target, new valid accounts default enabled, and auth is derived automatically (Chat/Responses Bearer, Messages `x-api-key`). Protocol and API URL remain editable after create. Do not re-describe it as multi-protocol, configurable-auth, verification-gated, enable-blocking, Phase-1 dormant, non-loopback HTTPS-only, public-DNS/private-denylist, connect-time DNS pinning, Direct/Manual-only, no production caller, or verify `501`.
- Do not make `GET /v1/models` or dashboard `application-models` hit upstream at request time; the former reads Go built-in Aliases, saved Zen Free snapshots, user-defined Provider public models, and eligible Custom declared IDs, the latter is Go-routable Aliases ∩ current pricing snapshot (excluding Custom and user-defined Providers). The explicit Zen Free “获取模型” (Fetch Models) action is the only catalog-refresh exception; the control is on the Providers page, may only access the fixed official endpoint, and must persist a successful snapshot before switching runtime.
- When changing UI appearance follow `DESIGN.md`: six font sizes, seven themes, Access Center first screen, Key naming; theme implementation is governed by `src/theme.ts`.
- Keep the Adapter Registry static and sealed. Typed Provider definitions may be created at runtime and persist as data; they never load user adapter code. Unknown `provider_id` values fail closed unless they match a persisted dynamic definition bound to Configurable HTTP. A user-deployed local service may be managed only through an approved static external-integration V3 adapter. CPA remains that adapter: connect/OAuth/accounts/models stay compatible, and only the installed Windows x64 Tauri Host may install, start, update, or stop an OCG-owned CPA child (kill-on-close Job Object, `managed.json` owner marker, no PID/port killing, never an external process). Other runtimes fail closed.

## Test strategy

- Domain / no-I/O kernel: `cargo test -p ocg-domain`, `cargo test -p ocg-gateway`, `cargo test -p ocg-infra`.
- For Rust host logic prefer `cargo test -p ocg-core` (includes `dashboard_v3_*`, `dashboard_v2_rest_retirement`, V3 runtime invariants).
- For CLI changes run `cargo test -p ocg-manager-cli`; when needed use a temporary data dir for real `key add/list`, `status`.
- For desktop Host changes run `cargo test -p ocg-manager --lib`.
- For frontend changes run `pnpm run build:web`; for contract changes run `pnpm run contract:v3:check`.
- `pnpm run test:web` covers only product tests under `src/` and finishes in seconds. Release-tooling tests (`scripts/*.test.mjs`) live behind `pnpm run test:tooling`, are slow because they spawn a fake container toolchain, and are gated on release/tooling changes rather than every run.
- Rust and frontend regression runs `pnpm run test` (web tests + typecheck + vite build + `cargo test --workspace --locked`); GUI/packaging changes run the current platform's `pnpm run build`. To claim real desktop availability, actually launch the installer, DMG, or AppImage and verify dashboard/gateway behavior.
- Rust unit tests live in a sibling `tests.rs` child module, not inline: `src/db.rs` declares `#[cfg(test)] mod tests;` and the tests live in `src/db/tests.rs`. Keep new unit tests there so source files stay readable. `include_str!` in a `tests.rs` resolves relative to that file, so fixture paths need the extra `../`. A few small `#[cfg(test)]` helpers that production code references stay inline on purpose.
- Do not add tests that assert on source text, workflow YAML, or documentation prose: no `include_str!`/`read_to_string` of `.rs`, `.vue`, `.yml`, or `.md` files to regex-match their contents, and no `syn`/AST scanning to police imports or module boundaries. Such tests only fail when someone edits the file and the fix is always to edit the test, so they cost tokens without protecting runtime behavior. Assert behavior through public APIs instead.

## Known gaps

- `/embeddings` and Gemini `embedContent` are not implemented; Gemini `countTokens` returns `501`, allowing Gemini CLI to fall back to local estimation.
- Gemini `generateContent` / `streamGenerateContent` are implemented, but non-empty `safetySettings`, `cachedContent`, `fileData`, Google Search, `urlContext`, and unsupported non-empty `generationConfig` fields return `400`. `topK` and `thinkingConfig` can only be treated as cross-protocol compatibility hints; do not promise semantic equivalence with the Gemini native backend.
- Streaming usage depends on the upstream usage chunk; Chat streaming requests set `stream_options.include_usage`. When no chunk is present it is recorded as `success_no_usage`.
- Account-page operational model tests use V3 `POST /dashboard/api/v3/accounts/{id}/model-tests`: one exact account, one admitted model, no selector/fallback, and no Provider evidence or account-state mutation. This is separate from low-frequency Provider model/protocol probes. Historical V2 `POST /dashboard/api/accounts/{id}/protocol-probes` and `POST /dashboard/api/accounts/{id}/test` remain removed and return `410` via the tombstone middleware.
- Windows/Linux ARM64, 32-bit x86, RPM, Snap, and app-store packages are not currently shipped; there is no Windows Authenticode formal signing or Apple notarization. Installed desktop versions can complete signed upgrades from the Settings page. See `docs/maintainer/release-artifacts.md` for artifact details.
- Command Code GOAT is a live fixed-origin route with Provider-level catalog controls; Custom API is a separate live trusted-admin route. Do not mix their trust, verification, or catalog semantics.
