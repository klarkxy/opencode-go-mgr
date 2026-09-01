[简体中文](README.zh-CN.md)

# Documentation index

OCG Manager documentation is split by audience: open the guide that matches your role. When you edit a paired page, keep the English and Chinese twins in sync; when a page and the code disagree, code at HEAD wins the argument.

## Catalog

| Audience | English | 简体中文 | Scope |
| --- | --- | --- | --- |
| Product overview | [../README.md](../README.md) | [../README.zh-CN.md](../README.zh-CN.md) | What it is, download matrix, 3-step start, pointers into USER |
| End users | [USER.md](USER.md) | [USER.zh-CN.md](USER.zh-CN.md) | 20 chapters in [`user/`](user/), including dedicated [Provider](user/add-provider.md) and [application](user/add-application.md) integration guides |
| Maintainers | [MAINTAINER.md](MAINTAINER.md) | [MAINTAINER.zh-CN.md](MAINTAINER.zh-CN.md) | 13 chapters in [`maintainer/`](maintainer/): layout, dev loop, architecture, release matrix, CI, validation |
| Anti-abuse | [OPENCODE_GO_ANTI_ABUSE.md](OPENCODE_GO_ANTI_ABUSE.md) | [OPENCODE_GO_ANTI_ABUSE.zh-CN.md](OPENCODE_GO_ANTI_ABUSE.zh-CN.md) | Allowed use boundary for OpenCode-Go |
| Contributors | [CONTRIBUTORS.md](CONTRIBUTORS.md) | bilingual / 中英同页 | Community credits |
| Design system | [../DESIGN.md](../DESIGN.md) | English source of truth | Themes, type scale, Key naming, layout rules |
| AI coding agents | [../AGENTS.md](../AGENTS.md) | English only | Project facts and coding constraints for assistants |

Root stubs redirect old top-level paths into this directory:

- [../CONTRIBUTORS.md](../CONTRIBUTORS.md) → [CONTRIBUTORS.md](CONTRIBUTORS.md)
- [../OPENCODE_GO_ANTI_ABUSE.md](../OPENCODE_GO_ANTI_ABUSE.md) → [OPENCODE_GO_ANTI_ABUSE.md](OPENCODE_GO_ANTI_ABUSE.md)
- [../OPENCODE_GO_ANTI_ABUSE.zh-CN.md](../OPENCODE_GO_ANTI_ABUSE.zh-CN.md) → [OPENCODE_GO_ANTI_ABUSE.zh-CN.md](OPENCODE_GO_ANTI_ABUSE.zh-CN.md)

## Fact ownership

When docs disagree, prefer the source below and fix the other side.

| Topic | Source of truth |
| --- | --- |
| User-visible product behavior | Code + chapters under [`user/`](user/) (for example [`user/accounts.md`](user/accounts.md) / [`user/accounts.zh-CN.md`](user/accounts.zh-CN.md), [`user/providers.md`](user/providers.md) / [`user/providers.zh-CN.md`](user/providers.zh-CN.md)) |
| Plan catalog | `crates/ocg-domain/src/provider.rs` (`BUILTIN_PLANS`, sealed `ProviderRegistry`); `crates/ocg-core/src/provider.rs` is the compatibility facade plus Custom URL inspection; [`user/accounts.md`](user/accounts.md) / [`user/accounts.zh-CN.md`](user/accounts.zh-CN.md) mirrors live vs pending families; [`user/providers.md`](user/providers.md) / [`user/providers.zh-CN.md`](user/providers.zh-CN.md) mirrors the control plane. Custom is `ConfigurableHttpAdapter`, not a base class or a dynamic plugin |
| Provider contracts | `crates/ocg-core/src/provider_contracts.rs`; [`user/providers.md`](user/providers.md) / [`user/providers.zh-CN.md`](user/providers.zh-CN.md) mirrors scopes, local catalogs, switches, probes, and request-time selection |
| Client aliases | `crates/ocg-gateway/src/alias.rs`; `crates/ocg-core/src/alias.rs` is the compatibility facade; [`user/gateway.md`](user/gateway.md) / [`user/gateway.zh-CN.md`](user/gateway.zh-CN.md) mirrors the contract |
| Local `GET /v1/models` | `crates/ocg-core/src/gateway/handler.rs`; authenticated Go aliases ∪ saved Zen Free aliases ∪ user-defined Provider public models ∪ eligible Custom IDs that have an effective enabled protocol; the GET itself makes no upstream request. Mirrored in [`user/gateway.md`](user/gateway.md) |
| Applications picker list | `crates/ocg-core/src/dashboard_v3/` (`GET /dashboard/api/v3/application-models`) via `control/observability.rs`; Go routeable aliases ∩ active pricing; no Custom. Mirrored in [`user/applications.md`](user/applications.md) |
| Custom API HTTP | `crates/ocg-core/src/custom.rs` + `custom_http.rs`; trusted-admin destinations, Direct/Manual/Auto, no redirects, isolated auth. Mirrored in [`user/accounts.md`](user/accounts.md) |
| Model preferred/supported protocols | `crates/ocg-domain/src/protocol.rs` (`MODEL_PROTOCOLS`); conversion kernel `crates/ocg-gateway/src/protocol.rs`; host parse/stream `crates/ocg-core/src/gateway/protocol.rs`; [`user/protocol-conversion.md`](user/protocol-conversion.md) / [`user/protocol-conversion.zh-CN.md`](user/protocol-conversion.zh-CN.md) mirrors the table |
| Model context/input/reasoning capabilities | `src/views/application-guides.ts` (`APPLICATION_MODEL_METADATA`); [`user/applications.md`](user/applications.md) / [`user/applications.zh-CN.md`](user/applications.zh-CN.md) mirrors the table |
| Dashboard HTTP API | `crates/ocg-core/src/dashboard_v3/` mounted at `/dashboard/api/v3`; frozen contract `schema/dashboard-api-v3.schema.json`; SPA client `src/api/dashboard-v3.ts` + presenters `src/api/dashboard.ts` / `src/api/providers.ts` + contract types `src/api/generated/dashboard-v3.ts`. Composition root `crates/ocg-core/src/host_router.rs`. Protected unversioned `/dashboard/api` REST is a structured `410` tombstone; auth/session, browser WebSocket, and inference stay distinct. |
| Access keys | SQLite `access_keys` (current schema v35; introduced in v27) via `crates/ocg-core/src/gateway_keys.rs` and `dashboard_v3/keys.rs`. Primary id is `PRIMARY_KEY_ID`. Historical `sub_gateway_keys` is not the live authority |
| SQLite schema | `crates/ocg-core/src/db.rs` (`CURRENT_SCHEMA_VERSION = 35`); upgrade/backup/rollback contract: [`maintainer/storage-migration.md`](maintainer/storage-migration.md) |
| Release artifacts, CI, signing | [`maintainer/release-artifacts.md`](maintainer/release-artifacts.md) / [`maintainer/release-artifacts.zh-CN.md`](maintainer/release-artifacts.zh-CN.md), [`maintainer/ci.md`](maintainer/ci.md) / [`maintainer/ci.zh-CN.md`](maintainer/ci.zh-CN.md), and [`maintainer/releasing.md`](maintainer/releasing.md) / [`maintainer/releasing.zh-CN.md`](maintainer/releasing.zh-CN.md) |
| Current package version pins | `package.json` / workspace `Cargo.toml` / `src-tauri/tauri.conf.json` / `compose.example.yaml` |
| UI copy for the access credential | Panel shows **Key** (`DESIGN.md`, `src/theme.ts`); never “Gateway Key” |
| Design tokens | [../DESIGN.md](../DESIGN.md) + `src/theme.ts` |
| Agent coding constraints | [../AGENTS.md](../AGENTS.md) |

Example version in Docker snippets should match the current release line (currently **v2.0.0**). Do not leave older patch pins in [`user/docker.md`](user/docker.md) / [`.env.example`](../.env.example) / [`compose.example.yaml`](../compose.example.yaml) after a version bump. The product README no longer pins a clone tag.

## Reading order

1. **New user** — README quick start → User guide `overview` → `architecture` → `install` → `first-client` → `accounts` (Key import vs managed Beta) → `providers` (catalogs, per-model overrides, probes, scoped pricing) → `gateway` → `routing` → `applications` → `troubleshooting`.
2. **Docker / CLI operator** — User guide `overview` → `architecture` → `docker` and `cli` → `accounts` → `providers` → `routing` → `logs-settings`; enable the browser profile when managed onboarding needs noVNC.
3. **Integration author** — User guide [`add-provider`](user/add-provider.md) for an upstream or [`add-application`](user/add-application.md) for a downstream client, then Maintainer guide `extending` for repository mechanics.
4. **Contributor** — Maintainer guide `layout` → `development` → `architecture` → `state-and-lifecycle` → `http-routes` → `conventions`; keep `AGENTS.md` for project facts (V3 crate split, `/dashboard/api/v3`, current schema v35, `access_keys`, managed wizard, quota refresh, protocol table, Key naming, typed user-defined Providers). Do not treat unversioned `/dashboard/api` REST or Tauri `invoke` as the live dashboard path.
5. **Release owner** — Maintainer guide `release-artifacts` → `ci` → `releasing` → `known-debt`; validation checklist includes managed rewind and refresh-quota paths.
6. **UI / theme work** — `DESIGN.md` first, then `src/theme.ts` and the Vue surface you are changing.

## Editing rules

- Keep EN/ZH heading structure and TOC anchors aligned for paired guides.
- Prefer short absolute facts over marketing language.
- Do not invent remote sync, Admin API, embeddings, or unsupported Gemini fields; known gaps live in [`user/limits.md`](user/limits.md), [`maintainer/known-debt.md`](maintainer/known-debt.md), and `AGENTS.md`. Command Code GOAT is a live fixed-origin route: its public catalog is not Key verification, the Provider matrix owns supply, and there is no account GOAT/All or Max mode. Custom API is live under the trusted-administrator boundary in [`user/accounts.md`](user/accounts.md); do not revive Phase-1 / SSRF-denylist wording. Do not invent a `requested_alias` log field. Do not equate `GET /v1/models` with `application-models` (the latter is Go ∩ pricing only, excluding Custom and user-defined Providers). Do not claim there is no supplier page, or that Zen catalog refresh lives on the account card. Do not claim the current schema is v26, that `sub_gateway_keys` is the live key table, that Tauri `invoke` commands are live, or that unversioned `/dashboard/api` REST is the dashboard primary path. The SPA uses `/dashboard/api/v3`. Protected V2 REST returns structured `410`; auth/session, browser WebSocket, and inference stay distinct. Adapter Registry is static and sealed; typed Provider definitions may persist as data bound to Configurable HTTP. Do not claim browser, billable live inference, or installed-desktop proof unless those checks were actually run.
- After release version bumps, update Docker clone tags and image pins in [`user/docker.md`](user/docker.md), [`.env.example`](../.env.example), and [`compose.example.yaml`](../compose.example.yaml) together (`pnpm run release:check` covers compose/package version alignment).
- Keep the product README a landing page: identity, download, three-step start, one curl, a Docker pointer, the preferred-protocol grouping, and links into USER. Do not copy the passthrough matrix, capability table, or circuit-breaker essay back into README.
