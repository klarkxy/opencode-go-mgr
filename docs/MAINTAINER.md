# Maintainer Guide

This guide is for people changing code, building releases, or debugging the project.

## Layout

- `src`: Vue 3 dashboard. `src/api/tauri.ts` is a historical name; it wraps HTTP `/dashboard/api`, not Tauri `invoke()`.
- `crates/ocg-core`: Gateway, dashboard HTTP API, SQLite, models, crypto, selectors, cooldown, and cost accounting.
- `crates/ocg-cli`: Headless CLI and gateway entrypoint.
- `src-tauri`: Windows tray app, single-instance behavior, Tauri commands, and NSIS packaging.

## Commands

```bash
pnpm install
pnpm run dev
pnpm run typecheck
pnpm run build:web
pnpm run test
pnpm run build:cli
pnpm run build:gui
pnpm run build
```

`pnpm run build:web` is the frontend build. `pnpm run build` is the full release path and copies artifacts to `release/`.

## Rust Checks

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo test --workspace
```

For focused work:

```bash
cargo test -p ocg-core
cargo test -p ocg-manager-cli
```

## Architecture Notes

The dashboard is served by the gateway under `/dashboard` and uses `/dashboard/api`. Tauri still registers command handlers, but those are not the main Vue data path.

Dashboard authentication is skipped for direct requests when the gateway binds a loopback address. Requests carrying standard reverse-proxy forwarding headers still require login. Non-loopback binds use a single administrator stored as an Argon2 password hash in SQLite and an HttpOnly session cookie. Docker may bootstrap the first administrator with `OCG_ADMIN_USERNAME` and `OCG_ADMIN_PASSWORD`; otherwise the first registration wins.

The gateway binds loopback, validates the Gateway Key, selects an enabled account, rewrites auth for upstream, and records logs, usage, cooldown, and errors in SQLite.

Each node owns its account data and is managed through its own dashboard. There is no cross-node sync or Admin API.

## Release Artifacts

After a full build, expected outputs are:

```text
target/release/ocg-manager.exe
target/release/ocg-manager-cli.exe
target/release/bundle/nsis/OCG Manager_1.0.0_x64-setup.exe
release/
```

## Known Debt

- The HTTP dashboard and Tauri command layer overlap. Do not delete Tauri commands until browser and startup behavior are either migrated or intentionally removed.
- The startup helper exists in the Tauri layer, but the HTTP dashboard does not expose that setting.
- Existing generated Tauri schema files are noisy in diffs; avoid touching them unless the Tauri config actually changed.
