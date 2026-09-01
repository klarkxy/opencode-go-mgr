[简体中文](development.zh-CN.md)

# Development

## Prerequisites

Use Node.js 22 (the CI baseline), pnpm 10.29.2 (`packageManager` in
`package.json`), and Rust 1.85 or newer. Native build dependencies vary by
runner; treat `.github/workflows/release.yml` as the source of truth. The
current Linux runner installs `libwebkit2gtk-4.1-dev
libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf
libfuse2 xvfb xauth xdg-utils dbus-x11`.

## Development

Quit the release tray app to free the single-instance lock and port `9042`,
then run:

```bash
pnpm install
pnpm run dev
```

`pnpm run dev` runs `tauri dev`. On Windows, `predev`
(`scripts/free-dev-port.mjs`) inspects `127.0.0.1:30001` and stops stale Vite
processes. Tauri starts Vite, waits for the gateway, then opens
`http://127.0.0.1:30001/dashboard/`. Vite proxies `/dashboard/api` (including
WebSockets) to the development Gateway port. `pnpm run dev` defaults that port
to `19042` so Windows HNS/WSL/Docker excluded ranges around `9042` do not block
the development stack. Installed builds still default to `9042`.

To use another development port, set one runtime override for both Tauri and
Vite before starting:

```powershell
$env:OCG_GATEWAY_PORT = "19042"
pnpm run dev
```

The Settings view shows the effective port as read-only while the variable is
set. Remove it with `Remove-Item Env:OCG_GATEWAY_PORT` before starting a later
session if you want to return to the saved port.

- Frontend (Vue, CSS, TypeScript) changes use Vite HMR.
- Rust changes use Tauri's watcher plus Cargo's incremental compiler, then
  restart the process. Rust code is **not** replaced inside a running
  process — expect a restart.

Enable shared git hooks once after cloning (`pnpm install` runs this via
`prepare`):

```bash
pnpm run hooks:install
# equivalent: git config core.hooksPath .githooks
```

When a commit stages `*.rs` files, `.githooks/pre-commit` runs
`cargo fmt --all` and re-stages them. CI checks the same with
`cargo fmt --all -- --check`.

## Checks And Builds

During development, run the smallest check that covers the changed ownership
boundary:

| Change scope | Local check |
| --- | --- |
| One frontend or script behavior | `node --experimental-strip-types --test <test-file>` |
| Vue/dashboard change | focused adjacent test, then `pnpm run build:web` |
| One Rust crate | `cargo test -p <package>`; add a test-name filter when useful |
| Core or Dashboard V3 behavior | `cargo test -p ocg-core <filter>` |
| Desktop Host behavior | `cargo test -p ocg-manager --lib` |
| Dashboard V3 schema or generated types | `pnpm run contract:v3:check` |

Run `pnpm install --frozen-lockfile` after cloning or when the pnpm lockfile
changes, not before every test. Use the full `pnpm run test` only for a
cross-frontend/Rust change, shared manifest or test-infrastructure change, or
an integration/release gate. Run `pnpm run design:lint` when `DESIGN.md` or
theme rules change. Run `pnpm run build` only when native release artifacts
are actually required; the complete pre-tag sequence remains in
`releasing.md`.

- `pnpm run build:web` is the **frontend-only** production build
  (`vue-tsc && vite build`). Use it when you only need to validate the
  dashboard. Do not run it again immediately after `pnpm run test`: that
  command already performs the same TypeScript check and Vite build.
- `pnpm run test` runs `pnpm run test:web` (Node `--experimental-strip-types`
  over `scripts/*.test.mjs` and `src/**/*.test.ts`), `vue-tsc --noEmit`,
  `vite build`, then `cargo test --workspace --locked`.
- `pnpm run test:rust` is the locked workspace Rust suite by itself.
- `pnpm run contract:v3:check` regenerates the Dashboard V3 JSON Schema from
  `ocg-core`'s `export_dashboard_v3_schema` example and fails if
  `schema/dashboard-api-v3.schema.json` or
  `src/api/generated/dashboard-v3.ts` drifted. Write with
  `pnpm run contract:v3:generate`.
- `pnpm run design:lint` runs the `@google/design.md` linter against
  `DESIGN.md`.
- `pnpm run build` is for **release validation** only. It runs
  `scripts/release.mjs`, builds the current native platform, and atomically
  replaces `release/` only after every expected file passes validation. The
  previous `release/` is kept on failure. Cargo's incremental cache is **not**
  erased. Release binaries use thin LTO (`[profile.release]` in the workspace
  `Cargo.toml`) to keep native CI linking bounded.

## Rust Checks

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --locked
```

The first command checks formatting without changing files. Run
`cargo fmt --all` to apply formatting. With hooks enabled, staged Rust commits
run that step via `.githooks/pre-commit`.

For focused work:

```bash
cargo test -p ocg-domain
cargo test -p ocg-gateway
cargo test -p ocg-infra
cargo test -p ocg-core
cargo test -p ocg-manager-cli
cargo test -p ocg-browser-worker
cargo test -p ocg-manager --lib
cargo test -p ocg-core gemini
cargo test -p ocg-core claude_desktop
cargo test -p ocg-core dashboard_v3
cargo test -p ocg-core v3_runtime_invariants
```

Layering rules (`ocg-domain` and `ocg-gateway` stay I/O-free; the kernel does
not import host code) are design intent documented in the module headers and
enforced by review and the crate graph, not by source-text assertions.

Rust unit tests live in a sibling child module rather than inline, so source
files stay readable: `src/db.rs` declares `#[cfg(test)] mod tests;` and the
tests live in `src/db/tests.rs`. Add new unit tests there. Two consequences
worth knowing: `include_str!` resolves relative to the file containing it, so
fixture paths in a `tests.rs` need one more `../` than they did inline, and a
few small `#[cfg(test)]` helpers that production code references are still
inline on purpose.

Do not add tests that assert on source text, workflow YAML, or documentation
prose. Reading a `.rs`, `.vue`, `.yml`, or `.md` file to regex-match its
contents, or walking a `syn` AST to police imports, only fails when someone
edits that file, and the fix is always to edit the test in the same commit.
Assert behavior through public APIs instead.

Test real account flows in a sandboxed CLI first:

```bash
ocg-manager-cli --data-dir /tmp/ocg-cli-test key add smoke sk-smoke
ocg-manager-cli --data-dir /tmp/ocg-cli-test key list
ocg-manager-cli --data-dir /tmp/ocg-cli-test serve --port 19042
```

The CLI only exposes `serve`, `key`, and `status`. `key add` creates an
enabled, ready OpenCode Go card via `account_control::create_go_api_key` and
bumps that process's `settings_revision`. It cannot create Custom accounts,
sub keys, or settings. Direct `Database::update_account` still does not bump
revision; this is intentional and not the CLI path.

## Frontend Checks

Frontend unit tests live next to the code (`src/**/*.test.ts`) and run with
Node's `--experimental-strip-types` — no extra test runner is required.
Script-level tests live in `scripts/*.test.mjs` (release helpers, Dashboard V3
contract, container publish). Pair them with `pnpm run build:web` and
`pnpm run contract:v3:check`.

The 17 application guides are driven by
`src/views/application-guides.ts`. When changing that registry, check the
guide count, unique IDs, protocol endpoints, the display/copy masking
difference, and the Claude Desktop three-role persistence behavior.

The side rail is Dashboard / Access Keys / Accounts / Providers / Aliases /
Applications / Logs / Settings. A `pricing` query is a legacy alias for
Providers. `BrowserSession` is a session overlay, not a ninth rail item.

## Local Release Smoke Build (Windows)

Local smoke build steps below. Full release process, CI matrix, and signing
keys: `docs/maintainer/releasing.md` and `docs/maintainer/ci.md`.

1. Make sure `pnpm` is available (`packageManager: pnpm@10.29.2`). If PATH
   has no pnpm, create a shim in your user directory.
2. Quit the installed release version to release the single-instance lock
   and `9042`:

   ```powershell
   Get-NetTCPConnection -LocalPort 9042 -ErrorAction SilentlyContinue |
     Select-Object OwningProcess | Get-Process | Stop-Process -Force
   ```

3. Version alignment: `package.json`, `src-tauri/tauri.conf.json`, workspace
   `Cargo.toml`, `src-tauri/Cargo.toml`, and the title/default image in
   `compose.example.yaml`.
4. Run `pnpm run build` (invokes `scripts/release.mjs`).

Signing-related environment variables (same as CI / MAINTAINER):

- `TAURI_SIGNING_PRIVATE_KEY`: private key content, or a secure path outside
  the repo (the script normalizes it to Tauri's path form).
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: private key password (if any).
- `TAURI_UPDATER_PUBLIC_KEY`: public key content; must match
  `src-tauri/updater-public-key.sha256`.
- `OCG_REQUIRE_UPDATER_ARTIFACTS=1`: forces signed artifacts; fails if keys
  are missing.

**Without `TAURI_SIGNING_PRIVATE_KEY` only plain local packages are
produced, which cannot be used for in-app upgrades and are only for local
smoke tests.**

On Windows, Tauri may convert line endings of `src-tauri/Cargo.toml` and
`src-tauri/gen/schemas/*.json` to CRLF; to get a clean working tree after
the build:

```powershell
git checkout -- src-tauri/Cargo.toml src-tauri/gen/schemas/desktop-schema.json src-tauri/gen/schemas/windows-schema.json
```
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](development.zh-CN.md) · [Docs index](../README.md)
