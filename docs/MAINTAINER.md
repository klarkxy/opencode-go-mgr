# Maintainer Guide

This guide is for people changing code, building releases, debugging the
gateway, and validating the desktop bundle. It describes the repository
layout, the development loop, the test/build pipeline, the architecture,
the release matrix, the CI smoke flow, and the things that are explicitly
out of scope.

## Table Of Contents

- [Layout](#layout)
- [Prerequisites](#prerequisites)
- [Development](#development)
- [Checks And Builds](#checks-and-builds)
- [Rust Checks](#rust-checks)
- [Frontend Checks](#frontend-checks)
- [Architecture Notes](#architecture-notes)
- [Upgrades And Database Migrations](#upgrades-and-database-migrations)
- [Release Artifacts](#release-artifacts)
- [CI Workflow](#ci-workflow)
- [Release Procedure](#release-procedure)
- [Release Validation Checklist](#release-validation-checklist)
- [Known Debt](#known-debt)
- [Coding Conventions](#coding-conventions)

## Layout

```
ocg-manager/
├── crates/
│   ├── ocg-core/      Gateway, dashboard HTTP API, SQLite, models, crypto, selector, cooldown, cost accounting
│   └── ocg-cli/       Headless CLI and gateway entrypoint
├── src/               Vue 3 dashboard (TypeScript, naive-ui, Vite)
│   ├── App.vue        Top-level shell, auth page, side rail, header
│   ├── api/tauri.ts   Historical name; HTTP wrapper for /dashboard/api (not Tauri invoke)
│   ├── components/    LocaleSwitcher, StackedBarChart
│   ├── i18n/          i18n setup + per-locale message tables + tests
│   ├── styles/        Theme tokens, design-system overrides
│   └── views/         Dashboard, Accounts, Applications, Logs, Settings (+ unit tests)
├── src-tauri/         Cross-platform tray app, single-instance behavior, Tauri commands, native packaging
├── docs/              USER.md, MAINTAINER.md (English + Chinese)
├── scripts/           free-dev-port.mjs, release.mjs
├── DESIGN.md          Design system source of truth (linted in CI)
├── .github/workflows/ Cross-platform release workflow
├── Dockerfile         Multi-stage headless gateway image
└── compose.yaml       Compose service definition
```

`src/api/tauri.ts` is a historical name; it wraps HTTP `/dashboard/api`, not
Tauri `invoke()`. Tauri commands still register in
`src-tauri/src/commands/`, but they are not the main Vue data path — the
HTTP dashboard is.

## Prerequisites

Use Node.js 22 (the CI baseline), pnpm 10.29.2, and Rust 1.85 or newer.
Native build dependencies vary by runner; treat
`.github/workflows/release.yml` as the source of truth. The current Linux
runner installs `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
librsvg2-dev libxdo-dev libssl-dev patchelf libfuse2 xvfb xauth xdg-utils
dbus-x11`.

## Development

Exit any running release tray app so the single‑instance lock and port
`9042` are free, then start the full development stack:

```bash
pnpm install
pnpm run dev
```

`pnpm run dev` runs `tauri dev`. On Windows the `predev` script
(`scripts/free-dev-port.mjs`) inspects `127.0.0.1:30001` and stops any stale
Vite process from a previous run. Tauri starts Vite and waits for the
Gateway to be ready, then opens `http://127.0.0.1:30001/dashboard/`.

- Frontend (Vue, CSS, TypeScript) changes use Vite HMR.
- Rust changes use Tauri's watcher plus Cargo's incremental compiler, then
  restart the process. Rust code is **not** replaced inside a running
  process — expect a restart.

## Checks And Builds

```bash
pnpm install
pnpm run test
pnpm run build:web
pnpm run design:lint
pnpm run build
```

- `pnpm run build:web` is the **frontend‑only** production build
  (`vue-tsc && vite build`). Use it when you only need to validate the
  dashboard.
- `pnpm run test` runs `cargo test --workspace`, the frontend unit tests
  (`src/i18n.test.ts`, `src/views/*.test.ts`, `src/theme.test.ts`), and a
  `vue-tsc --noEmit` typecheck.
- `pnpm run design:lint` runs the `@google/design.md` linter against
  `DESIGN.md` so the design system stays in sync with the code.
- `pnpm run build` is reserved for **release validation**. It runs
  `scripts/release.mjs`, which builds the current supported native
  platform and atomically replaces `release/` only after every expected
  file passes validation. The previous `release/` is preserved on
  failure. Cargo's incremental build cache is **not** erased.

## Rust Checks

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
```

The first command checks formatting without changing files. Run
`cargo fmt --all` to apply formatting.

For focused work:

```bash
cargo test -p ocg-core
cargo test -p ocg-manager-cli
```

Run the CLI in a sandbox first when testing real account flows:

```bash
ocg-manager-cli --data-dir /tmp/ocg-cli-test key add smoke sk-smoke
ocg-manager-cli --data-dir /tmp/ocg-cli-test key list
ocg-manager-cli --data-dir /tmp/ocg-cli-test serve --port 19042
```

## Frontend Checks

The frontend unit tests live next to the code they cover
(`src/i18n.test.ts`, `src/views/accounts-usage.test.ts`,
`src/views/dashboard-connection.test.ts`, `src/views/logs.test.ts`,
`src/theme.test.ts`). They run with Node's experimental
`--experimental-strip-types` flag — no extra test runner is required.
Pair them with `pnpm run build:web` for a final smoke test.

## Architecture Notes

### Gateway

- The gateway is `crates/ocg-core/src/gateway/`: Axum + Tokio + reqwest,
  bound to `127.0.0.1:9042` by default.
- The handler parses the client's `Authorization: Bearer <gateway-key>`,
  validates it against the configured key, selects an enabled account,
  rewrites auth for upstream, and records logs, usage, cooldown, and
  errors in SQLite.
- `protocol.rs` and `protocol_stream.rs` convert between Chat Completions,
  Responses, and Anthropic Messages. `selector.rs` picks the next account
  and skips disabled / cooled‑down / already‑failed accounts. `limit.rs`
  parses the upstream 429 reset phrase. `cost.rs` aggregates token counts
  into the local 5‑hour / weekly / monthly windows.

### Dashboard

- The dashboard is served by the gateway under `/dashboard` and uses
  `/dashboard/api` for its JSON. Tauri still registers command handlers,
  but those are not the main Vue data path.
- Dashboard authentication is **skipped for direct requests** when the
  gateway binds a loopback address. Requests carrying standard
  reverse‑proxy forwarding headers still require login. Non‑loopback binds
  use a single administrator stored as an Argon2 password hash in SQLite
  and an HttpOnly session cookie.
- Docker may bootstrap the first administrator with
  `OCG_ADMIN_USERNAME` and `OCG_ADMIN_PASSWORD`; otherwise the first
  registration wins.

### Persistence

- `crates/ocg-core/src/db.rs` defines the SQLite schema, migrations, and
  queries. `crates/ocg-core/src/models.rs` defines the shared serde types
  and `AppConfig`. `crates/ocg-core/src/crypto.rs` provides key
  obfuscation and `.encryption-key` management.
- `crates/ocg-core/src/state.rs` is the `CoreStateInner` shared by the
  gateway, dashboard, and CLI.

### Per‑Node Boundaries

Each node owns its account data and is managed through its own dashboard.
There is no cross‑node sync and no Admin API. Do not add one.

## Upgrades And Database Migrations

SQLite migrations run in place when the GUI or CLI starts. Stop the process
and back up the complete data directory before upgrading, including the
database and `.encryption-key` when present. Downgrades are not guaranteed;
to roll back, restore the data backup made by the matching older version
instead of opening a migrated database with an older binary.

## Release Artifacts

The supported matrix is intentionally small:

| Runner | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | NSIS current‑user setup | x64 ZIP |
| macOS 11+ | Universal DMG (x64 + ARM64) | Universal tar.gz |
| Linux x64 | AppImage + deb | x64 tar.gz |

Stable delivery names are:

```text
ocg-manager_<version>_windows-x64-setup.exe
ocg-manager-cli_<version>_windows-x64.zip
ocg-manager_<version>_macos-universal.dmg
ocg-manager-cli_<version>_macos-universal.tar.gz
ocg-manager_<version>_linux-x64.AppImage
ocg-manager_<version>_linux-x64.deb
ocg-manager-cli_<version>_linux-x64.tar.gz
SHA256SUMS
```

Each CLI archive contains its executable, `dist/`, and `LICENSE`. Do not
ship the CLI executable alone: `serve` needs the sibling dashboard
assets. Windows has no portable GUI artifact.

`scripts/release.mjs` does the heavy lifting:

1. Validates that `package.json`, `src-tauri/tauri.conf.json`, the
   workspace `Cargo.toml`, and `src-tauri/Cargo.toml` all agree on the
   version. It also checks the Git tag, if any, against that version.
2. Rejects unsupported host/architecture pairs
   (`process.platform`/`process.arch`).
3. Invokes `@tauri-apps/cli` with the exact bundle path for the platform
   (`nsis` on Windows, `appimage,deb` on Linux, `dmg` with
   `--target universal-apple-darwin` on macOS).
4. Builds the CLI binary, packages it with `dist/` and `LICENSE` into
   the per‑platform archive, and on macOS uses `lipo` + `codesign -` to
   create the universal CLI.
5. Writes `SHA256SUMS` over every payload in the staged `release/`
   directory.
6. Atomically replaces `release/`. On any error, the previous `release/`
   is preserved and the staged tree is removed.

`scripts/release.mjs` does **not** erase Cargo's incremental build
caches — repeated release builds reuse the same `target/` tree.

## CI Workflow

`.github/workflows/release.yml` runs on `workflow_dispatch` and on `v*`
tags, with a 3‑runner matrix: Windows x64, macOS Universal, and Linux x64
(Ubuntu 22.04). Each runner:

1. Installs the matching Rust targets on macOS, and `libwebkit2gtk-4.1-dev
   libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev
   patchelf libfuse2 xvfb xauth xdg-utils dbus-x11` on Linux.
2. Runs `pnpm install --frozen-lockfile`, `pnpm run build:web`,
   `pnpm run test`, `pnpm run design:lint`, and `pnpm run build`.
3. Uploads the per‑runner `release/` directory as a
   `release-<platform>` Actions artifact.

Each runner also runs a smoke flow on the freshly built bundle:

- **Windows CLI** — verifies `SHA256SUMS`, expands the ZIP, runs
  `key add` / `key list` / `key disable` / `key enable` / `status` /
  `key remove` against a temp data dir, then starts `serve --port=19042`
  and waits for `id="app"` to appear in the dashboard HTML.
- **macOS / Linux CLI** — the same `key` and `serve` flow plus a `lipo
  -archs` check that the macOS CLI is a universal binary.
- **Windows GUI** — silent NSIS install into a temp dir, launch with
  `--startup`, wait for the dashboard on `127.0.0.1:9042`, flip
  `auto_start` on, verify the
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OCG Manager` value,
  flip it off, verify cleanup, then silently uninstall and confirm the
  user data dir survives.
- **macOS GUI** — mount the DMG, `codesign --verify --deep --strict`,
  check the binary is universal with `lipo -archs`, launch with
  `--startup`, wait for the dashboard.
- **Linux GUI** — `dpkg-deb --info` / `dpkg-deb --contents` on the deb,
  `file` on the AppImage, then launch under
  `dbus-run-session -- xvfb-run -a env APPIMAGE_EXTRACT_AND_RUN=1
  WEBKIT_DISABLE_COMPOSITING_MODE=1` and wait for the dashboard.

When a `v*` tag is pushed, a downstream `draft-release` job downloads the
three per-runner Actions artifacts, assembles the seven platform payloads in
`release/`, regenerates `SHA256SUMS` over all seven, and creates or updates a
**draft** GitHub Release. It never publishes the release. After reviewing the
draft and the native smoke results, publish the release in GitHub or run
`gh release edit vX.Y.Z --draft=false`.

Current Windows installers are unsigned and macOS uses ad‑hoc signing
(`-`), not Developer ID notarization. Keep releases in draft until
native smoke checks and platform warnings are reviewed. Windows/Linux
ARM64, 32‑bit x86, RPM, Snap, app stores, and automatic updates remain
unsupported.

### CI Coverage Boundaries

The repository has no `pull_request` workflow, so these checks do not run
automatically on PRs. The release workflow also does not build or smoke-test
the Docker image, drive real desktop UI interactions, or test backup/restore,
database downgrade, or migration rollback. Run the relevant checks manually
when changing those paths.

## Release Procedure

1. Choose `X.Y.Z` and set it in `package.json`,
   `src-tauri/tauri.conf.json`, the workspace `Cargo.toml`, and
   `src-tauri/Cargo.toml`.
2. Run `cargo check --workspace --all-targets` to refresh `Cargo.lock`, then
   run `pnpm install --frozen-lockfile`, `cargo fmt --all -- --check`,
   `pnpm run test`, `pnpm run design:lint`, and `pnpm run build`. Commit the
   intended lockfile changes; never hand-edit them.
3. Review the diff and current-platform `release/` payloads, then commit the
   version and lockfile changes.
4. Create an annotated tag on that commit with
   `git tag -a vX.Y.Z -m "OCG Manager vX.Y.Z"`, then push the branch and tag.
5. Wait for every `release.yml` matrix job and `draft-release` to pass. Review
   the draft's seven payloads, `SHA256SUMS`, smoke logs, and platform warnings.
6. Publish the draft in GitHub or run
   `gh release edit vX.Y.Z --draft=false`, then verify the public release.

Treat published assets and tags as immutable. If a published payload is wrong,
ship a new patch version; do not replace the asset or retarget the tag.

## Release Validation Checklist

Run these checks **before** publishing a `v*` tag. The CI smoke flow
covers most of them; the manual parts need a real desktop.

- [ ] `pnpm run test`, `pnpm run design:lint`, `pnpm run build` are
      green on the three runners.
- [ ] Each runner's `release/SHA256SUMS` matches every payload in that
      directory; the aggregated release checksum matches all seven platform
      payloads.
- [ ] On Windows, run the installer once, confirm SmartScreen warning
      text, open the dashboard, add an account, send one request.
- [ ] On macOS, mount the DMG, confirm the **Open Anyway** flow works,
      open the dashboard, add an account, send one request.
- [ ] On Linux, install the `.deb`, launch the AppImage, confirm the
      dashboard opens under Xvfb on CI and under a real Wayland or X11
      session locally.
- [ ] On Windows, verify `auto_start` toggles the
      `HKCU\...\Run\OCG Manager` value and that the value is removed
      on uninstall.
- [ ] Confirm `scripts/release.mjs` reported a successful atomic
      replacement of `release/` and that the previous `release/` is
      gone.
- [ ] Review the draft GitHub Release notes and the unsigned/ad‑hoc
      warnings before flipping `--draft=false`.

## Known Debt

- The HTTP dashboard and the Tauri command layer overlap. Do not delete
  Tauri commands until browser and startup behavior are either migrated
  or intentionally removed.
- Auto‑start is capability‑gated: only Windows release/installed Tauri
  processes inject the registry sync hook. Development builds, the CLI,
  Docker, macOS, and Linux dashboards do not expose the switch.
- Existing generated Tauri schema files are noisy in diffs; avoid
  touching them unless the Tauri config actually changed.
- Streaming cost is exact only when upstream emits usage chunks. Without
  one, the row ends as `success_no_usage`.
- The HTTP dashboard does not expose the older isolated WebView browser
  command. The Tauri command layer still has it.
- The Responses endpoint is stateless. `previous_response_id`,
  `conversation`, `store: true`, and `background: true` return `400`
  rather than being silently ignored. This is intentional — see
  `protocol.rs` and the User guide.

## Coding Conventions

- **Ponytail principle.** Prefer deleting code over adding code; reuse
  existing helpers before adding new abstractions. The codebase favors
  flat call sites over speculative indirection.
- **No new Tauri `invoke()` paths on the frontend.** The main Vue data
  path is HTTP `/dashboard/api`. Only re‑introduce `invoke()` calls when
  you are explicitly restoring a desktop WebView feature.
- **Do not weaken security boundaries.** Gateway authentication, key
  obfuscation, the URL allowlist, cooldown writes, and SSE pass‑through
  are not simplification candidates.
- **Do not add remote sync.** Each node is managed through its own
  dashboard.
- **Capability‑gate `auto_start`.** Only the Windows release/installed
  Tauri process injects the registry sync hook; development builds, the
  CLI, Docker, macOS, and Linux dashboards must keep hiding the switch.
- **Don't re‑invent `cargo test` ergonomics.** The CLI uses
  `parking_lot::Mutex`, which is not re‑entrant. When a function needs
  to call another lock holder, `drop` the guard first.
- **Match the surrounding style.** When you change code in a file, the
  new code should look like the old code: same comment density, naming,
  and idiom.

---

[中文维护者指南](MAINTAINER.zh-CN.md) · [User guide](USER.md) ·
[用户指南](USER.zh-CN.md) · [Security policy](../SECURITY.md) ·
[Back to README](../README.md)
