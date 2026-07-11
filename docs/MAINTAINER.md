# Maintainer Guide

This guide is for people changing code, building releases, or debugging the project.

## Layout

- `src`: Vue 3 dashboard. `src/api/tauri.ts` is a historical name; it wraps HTTP `/dashboard/api`, not Tauri `invoke()`.
- `crates/ocg-core`: Gateway, dashboard HTTP API, SQLite, models, crypto, selectors, cooldown, and cost accounting.
- `crates/ocg-cli`: Headless CLI and gateway entrypoint.
- `src-tauri`: Cross-platform tray app, single-instance behavior, Tauri commands, and native packaging.

## Development

Exit any running release tray app so the single-instance lock and port `9042` are free, then run the complete development stack:

```bash
pnpm run dev
```

Tauri starts Vite and opens `http://127.0.0.1:30001/dashboard/` after the Gateway is ready. Frontend changes use Vite HMR; Rust changes use Tauri's watcher and Cargo incremental compilation, then restart the process. Rust code is not replaced inside the running process.

## Checks and builds

```bash
pnpm install
pnpm run test
pnpm run build:web
pnpm run design:lint
pnpm run build
```

`pnpm run build:web` is the frontend-only production build. `pnpm run build` is reserved for release validation; it builds the current supported native platform and atomically replaces `release/` only after every expected file passes validation.

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

The gateway binds loopback, validates the local Key, selects an enabled account, rewrites auth for upstream, and records logs, usage, cooldown, and errors in SQLite.

Each node owns its account data and is managed through its own dashboard. There is no cross-node sync or Admin API.

## Release Artifacts

The supported matrix is intentionally small:

| Runner | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | NSIS current-user setup | x64 ZIP |
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

Each CLI archive contains its executable, `dist/`, and `LICENSE`. Do not ship the CLI executable alone: `serve` needs the sibling dashboard assets. Windows has no portable GUI artifact.

The release script rejects unsupported host/architecture pairs, checks package/Tauri/Cargo versions, uses exact Tauri bundle paths, and preserves the previous `release/` on failure. It does not erase Cargo incremental build caches.

`.github/workflows/release.yml` runs `pnpm install --frozen-lockfile`, tests, design lint, and the native release build on Windows, macOS, and Linux. A manual run uploads three Actions artifacts. A `v*` tag also combines their files, regenerates `SHA256SUMS`, and creates or updates a **draft** GitHub Release; it never publishes the release.

CI extracts every CLI archive, checks its contents and checksum, runs add/list/disable/enable/status/remove, and starts `serve` to probe the bundled dashboard. It also installs and uninstalls NSIS on Windows, mounts and verifies the Universal DMG on macOS, and launches the AppImage under Xvfb on Linux before probing the GUI gateway. SmartScreen and Gatekeeper approval dialogs still require manual release-candidate checks on real desktops.

The initial Windows setup is unsigned and macOS uses ad-hoc signing (`-`), not Developer ID notarization. Keep releases in draft until native smoke checks and platform warnings are reviewed. Windows/Linux ARM64, 32-bit x86, RPM, Snap, app stores, and automatic updates remain unsupported.

## Known Debt

- The HTTP dashboard and Tauri command layer overlap. Do not delete Tauri commands until browser and startup behavior are either migrated or intentionally removed.
- The startup helper exists in the Tauri layer, but the HTTP dashboard does not expose that setting.
- Auto-start is Windows-only; the non-Windows implementation is intentionally a no-op.
- Existing generated Tauri schema files are noisy in diffs; avoid touching them unless the Tauri config actually changed.
