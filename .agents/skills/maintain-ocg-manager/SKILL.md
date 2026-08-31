---
name: maintain-ocg-manager
description: Maintain OCG Manager safely and minimally. Use for repository changes to gateway routing, Dashboard V3, Vue dashboard views, Tauri Desktop Host capabilities, provider/catalog behavior, application guides/connectors, documentation, validation, or release-readiness evidence.
---

# Maintain OCG Manager

Start with [AGENTS.md](../../../AGENTS.md), then inspect the live execution
path and the affected tests. Treat code as authoritative. Preserve unrelated
working-tree changes and do not commit, publish, tag, deploy, or alter Git
state unless explicitly authorized.

## Route the change

| Change | Start here | Required boundary |
| --- | --- | --- |
| Gateway, alias, protocol, key, proxy, usage, catalog | [runtime invariants](../../../docs/maintainer/runtime-invariants.md), then the named crate source | Preserve auth, no-I/O conversion, snapshots, logging, and fail-closed routing. |
| Dashboard/API contract | [dashboard V3](../../../crates/ocg-core/src/dashboard_v3/), [V3 schema](../../../schema/dashboard-api-v3.schema.json), [V3 client](../../../src/api/dashboard-v3.ts) | Use only `/dashboard/api/v3`; update schema/types and run the contract check. Do not revive retired V2 REST. |
| Vue UI | [views](../../../src/views/), [components](../../../src/components/), [domain](../../../src/domain/), [DESIGN.md](../../../DESIGN.md) | Reuse the V3 client and theme; retain the fixed rail and the **Key** name. |
| Desktop capability | [Desktop Host](../../../src-tauri/src/host/), [Tauri startup](../../../src-tauri/src/lib.rs), [host router](../../../crates/ocg-core/src/host_router.rs) | Register a local, process-owned capability in `CoreState`; never add a Tauri `invoke` command or a second desktop control path. |
| Provider or Plan | [registry](../../../crates/ocg-domain/src/provider.rs), [protocol table](../../../crates/ocg-domain/src/protocol.rs), [alias resolver](../../../crates/ocg-gateway/src/alias.rs) | Keep the registry static and sealed. Unknown pairs fail closed; do not add plugins or runtime discovery. |
| Application guide/connector | [guide registry](../../../src/views/application-guides.ts), [Applications view](../../../src/views/Applications.vue), [Desktop connector Host](../../../src-tauri/src/host/application_connectors.rs), [native packages](../../../integrations/), [user guide](../../../docs/user/applications.md) | Treat it as a local Desktop capability. Use field-owned configuration for supported config clients and client-native packages for Pi/DSH; never add a connector service, daemon, remote sync, or dynamic Provider registry. |
| Static external integration | [runtime invariants](../../../docs/maintainer/runtime-invariants.md), [extending](../../../docs/maintainer/extending.md), [Dashboard V3](../../../crates/ocg-core/src/dashboard_v3/) | Keep it a typed, product-approved local-service adapter. It is neither a Provider/Plan nor a dynamic plugin; OCG may manage it but never hosts, upgrades, or reads its private auth files. |

## Control-plane and connector rules

- Put dashboard mutations behind V3 CAS: send `expectedRevision` and
  `processGeneration` (and `expectedPricingRevision` for price writes). On
  `409 revisionConflict`, refresh the token and affected resource; never
  auto-replay a mutation.
- Keep field ownership singular. The backend owns catalog, account/provider
  state, and persisted configuration; the V3 contract owns wire shapes; the
  frontend owns only presentation and transient form/draft state; `CoreState`
  owns local Desktop Host capabilities. Do not let an application connector
  silently write a field owned by another layer.
- Replace coupled connector/configuration sets atomically in one V3 CAS write.
  Do not split an endpoint, protocol, capability list, or associated mapping
  into independently saved partial state. Respect immutable-after-create
  catalog fields.
- For application-facing configuration, copy from the protected local V3
  source. `application-models` and `/v1/models` remain local reads; never add
  request-time upstream discovery, account selection, or billable probing.
- Keep plaintext Keys on the shortest authorized path: the session-protected
  V3 connection response for copy-ready manual snippets, or Core directly to
  the registered Desktop Host and its fixed client targets for automatic
  connection. Never place a Key in connector preview/response DTOs, logs,
  errors, ownership state, sidecars, journals, or unrelated configuration.
- Preserve the local Desktop model: Host capabilities are process-owned and
  compose the one loopback dashboard/gateway. Do not introduce remote sync,
  an admin API, a background connector process, or a second mutation channel.
  A static external integration may instead address a user-deployed loopback
  service (or its explicit Compose sibling) through a narrow V3 adapter; it
  must not become a general remote-service or process-management surface.

## Native application packages

- Keep the Provider/Plan registry sealed. Pi and DSH packages are integrations
  installed by those client applications, not runtime Provider plugins for OCG
  Manager.
- Generate packages only from checked-in templates, the fixed loopback Gateway
  URL, and selected public Alias metadata. Never place a Key, bearer header,
  environment value, or credential in package source, generated catalogs,
  package-manager arguments, output, logs, or fingerprints.
- Let Pi store the Key through its native provider login. For DSH, install the
  companion route and field-manage only `OCG_MANAGER_API_KEY` in the selected
  DSH home's `.env`; preserve every other line and restore the original value.
  OCG manages only `ocg-manager-pi`, the `ocg-manager-dsh` web-profile bundle,
  and that one DSH environment assignment; removal must leave every other
  extension, Provider, profile, and credential untouched.
  DSH bundle patches replace a matching row's whole config and skip absent ids.
  Use an `insert` for the OCG companion plugin, which registers only its fixed
  route; never patch the base `llm-pi-ai` row or mount a second copy of that
  plugin.
- Invoke only the detected client with fixed argument vectors and a bounded
  timeout. On Windows, create `.cmd`/`.bat` launchers suspended, place them in a
  kill-on-close Job Object, and only then resume them; never interpolate a Key
  or untrusted user text into the command-processor line. On Unix, start the
  package manager in a fresh process group and terminate the entire group on
  timeout. Recheck the client's
  package registry after the command and report success only when the exact
  source was installed or removed. For Pi removal, use the exact registered OCG
  source path. For DSH, require the dependency to resolve to the exact
  digest-named OCG source before allowing removal.
- Generated package sources are immutable direct children of the OCG connector
  root. Reject symlinks, junctions/reparse points, non-direct descendants, and
  digest mismatches at every path component before writing, installing, or
  removing a package.
- Keep routine tests isolated with temporary homes, temporary data directories,
  and an injected command runner. They must not discover, edit, start, stop, or
  restart real client applications. The phase-one real-machine gate is Codex,
  Claude Code, OpenCode, Pi, and DSH; run it only with explicit live-test
  authorization and report install/restart/request-log evidence separately.

## Sources and documentation

Read the authoritative source named above before changing its governed fact.
For schema/storage behavior also use
[storage migration](../../../docs/maintainer/storage-migration.md); for
execution/layout and focused test selection use
[development](../../../docs/maintainer/development.md).

When a user-visible fact changes, update the English guide first and its
matching `*.zh-CN.md` page, including reciprocal links and TOCs. Keep detailed
tables in `docs/user/`; do not expand the root README into a duplicate source
of truth. Changes to application guides also require the user guide capability
table to match `application-guides.ts`.

## Verify and hand off

Run the smallest relevant checks, then broaden only when risk warrants it:

| Scope | Minimum evidence |
| --- | --- |
| Domain/gateway/infra Rust | `cargo test -p ocg-domain`, `cargo test -p ocg-gateway`, or `cargo test -p ocg-infra` as applicable |
| Core/V3 behavior | `cargo test -p ocg-core` |
| Desktop Host | `cargo test -p ocg-manager --lib` |
| Pi/DSH package templates | `node --test scripts/application-plugin-packages.test.mjs` |
| Vue/UI | focused adjacent test plus `pnpm run build:web` |
| V3 schema/client | `pnpm run contract:v3:check` |
| Full regression | `pnpm run test` |

For a release claim, follow [releasing.md](../../../docs/maintainer/releasing.md):
record exact commands and outcomes, run the release checks, and perform the
required platform smoke. A build alone is not proof that the installed desktop
experience works. Report files changed, checks run, observed result, and any
unverified live/packaging risk.
