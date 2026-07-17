[简体中文](MAINTAINER.zh-CN.md)

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
├── compose.yaml       Source-build and image Compose service definition
└── compose.example.yaml  Pull-only Compose example attached to each Release
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
- The handler accepts the configured Gateway Key as
  `Authorization: Bearer`, `x-api-key`, or `x-goog-api-key`, selects an enabled
  account, rewrites auth for upstream, and records logs, usage, cooldown, and
  errors in SQLite.
- `protocol.rs` and `protocol_stream.rs` convert between Chat Completions,
  Responses, Anthropic Messages, and the client-side Gemini format. Gemini is
  never an upstream protocol: `/v1beta/models/{model}:generateContent` and
  `:streamGenerateContent` (plus their `/v1/models/...` aliases) are converted
  to the known model's native Chat Completions or Messages path and back into
  Google JSON/SSE envelopes. `selector.rs` picks the next account and skips
  disabled / cooled-down / already-failed accounts. `limit.rs` parses the
  upstream 429 reset phrase. `cost.rs` aggregates token counts into the local
  5-hour / weekly / monthly windows.
- Claude Desktop has isolated `/claude-desktop/v1/messages` and
  `/claude-desktop/v1/models` routes. Its three advertised Claude aliases are
  rewritten from `AppConfig.claude_desktop_models` before entering the normal
  Messages preparation path.

### Dashboard

- The dashboard is served by the gateway under `/dashboard` and uses
  `/dashboard/api` for its JSON. Tauri still registers command handlers,
  but those are not the main Vue data path.
- Dashboard authentication is **skipped for direct requests** when the
  gateway binds a loopback address. Requests carrying standard
  reverse‑proxy forwarding headers still require login. Non‑loopback binds
  use a single administrator stored as an Argon2 password hash in SQLite
  and an HttpOnly session cookie.
- Settings uses the protected `GET /dashboard/api/settings/check-update`
  endpoint for GitHub Release metadata. In an updater-enabled installed
  desktop runtime, the user can continue through a signed download and
  install; development builds, CLI, and Docker retain the metadata/release-link
  path. The outbound request runs only when the user clicks the button; it is
  not telemetry.
- The Applications view is generated from 13 guides: Claude Code, Claude
  Desktop, Codex, Gemini CLI, OpenCode, OpenClaw, Hermes, Cherry Studio,
  VS Code Copilot Chat, Cline, Roo Code, Continue, and Chatbox. Its ordinary
  model selections and edited snippets are in-memory view state. Claude
  Desktop mappings are durable and use the protected
  `GET/PUT /dashboard/api/claude-desktop/models` endpoint.
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
- `AppConfig` uses serde defaults for backward-compatible loading. A pre-1.3
  config without `claude_desktop_models` receives the default Sonnet target
  `minimax-m3` and is canonically rewritten to SQLite. Model updates are
  serialized by `settings_update`; an ordinary settings save preserves the
  dedicated Claude Desktop mapping.

### Per‑Node Boundaries

Each node owns its account data and is managed through its own dashboard.
There is no cross‑node sync and no Admin API. Do not add one.

## Upgrades And Database Migrations

SQLite migrations run in place when the GUI or CLI starts. Back up the complete
data directory before upgrading, including the database and `.encryption-key`
when present; stop the process first for a direct/manual upgrade. The signed
desktop updater manages its own stop and restart. Downgrades are not guaranteed;
to roll back, restore the data backup made by the matching older version
instead of opening a migrated database with an older binary.

Version 1.4.1 has neither the updater runtime nor its embedded verification
key. For the one-time Windows transition, instruct users to quit the tray app,
run the first updater-enabled setup, and choose the second upgrade-method
option, **Install without uninstalling** (不要卸载，直接安装). Tauri merely
selects the first option by default; that option is not required. Users must
not uninstall 1.4.1 first. The optional equivalent for advanced users is:

```powershell
Start-Process -FilePath .\ocg-manager_<version>_windows-x64-setup.exe -ArgumentList '/UPDATE','/P','/R' -Wait
```

macOS/Linux use their normal direct replacement once. Later desktop releases
can use the signed Settings update path. CLI and Docker upgrades remain manual.

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
ocg-manager_<version>_windows-x64-setup.exe.sig
ocg-manager-cli_<version>_windows-x64.zip
ocg-manager_<version>_macos-universal.dmg
ocg-manager_<version>_macos-universal.app.tar.gz
ocg-manager_<version>_macos-universal.app.tar.gz.sig
ocg-manager-cli_<version>_macos-universal.tar.gz
ocg-manager_<version>_linux-x64.AppImage
ocg-manager_<version>_linux-x64.AppImage.sig
ocg-manager_<version>_linux-x64.deb
ocg-manager_<version>_linux-x64.deb.sig
ocg-manager-cli_<version>_linux-x64.tar.gz
compose.example.yaml
latest.json
SHA256SUMS
```

Each CLI archive contains its executable, `dist/`, and `LICENSE`. Do not
ship the CLI executable alone: `serve` needs the sibling dashboard
assets. Windows has no portable GUI artifact.

The `linux/amd64` container is published separately as
`ghcr.io/klarkxy/opencode-go-mgr`; the GitHub Release contains the seven
ordinary platform payloads, the extra macOS updater archive, four updater
signatures, the pull-only Compose example, `latest.json`, and `SHA256SUMS`
(15 attachments total). The runtime image includes `LICENSE` at
`/usr/share/licenses/ocg-manager/LICENSE`.

`scripts/release.mjs` does the heavy lifting:

1. Validates that `package.json`, `src-tauri/tauri.conf.json`, the
   workspace `Cargo.toml`, and `src-tauri/Cargo.toml` all agree on the
   version. It also checks the Git tag, if any, against that version.
2. Resolves the updater signing mode before creating the staging tree. With
   `OCG_REQUIRE_UPDATER_ARTIFACTS=1`, either a missing private key or missing
   `TAURI_UPDATER_PUBLIC_KEY` fails before `release/` can be replaced.
3. When a signing key is configured, merges
   `src-tauri/tauri.updater.conf.json` plus an ephemeral public-key config and
   enables Tauri updater artifacts. `TAURI_SIGNING_PRIVATE_KEY` accepts either
   the private-key content or its secure path outside the repository; there is
   no separate path variable. With no signing key, the script preserves the
   ordinary local build and prints that the result is for smoke testing, not an
   updater-enabled published release.
4. Rejects unsupported host/architecture pairs
   (`process.platform`/`process.arch`).
5. Invokes `@tauri-apps/cli` with the exact bundle path for the platform
   (`nsis` on Windows and `appimage,deb` on Linux). macOS uses `dmg` with
   `--target universal-apple-darwin` for unsigned local builds and `app,dmg`
   when updater signing is enabled, because Tauri only emits the updater
   archive for the `app` target.
6. Cryptographically verifies every payload/signature pair against the actual
   `TAURI_UPDATER_PUBLIC_KEY` before staging it, then collects the NSIS and
   AppImage signatures plus the macOS `.app.tar.gz`/signature. It explicitly
   signs the deb with `tauri signer sign` because deb is not a native Tauri
   updater artifact. A nonempty but mismatched key therefore fails closed.
7. Builds the CLI binary, packages it with `dist/` and `LICENSE` into
   the per‑platform archive, and on macOS uses `lipo` + `codesign -` to
   create the universal CLI.
8. Writes `SHA256SUMS` over every payload and signature in the staged `release/`
   directory.
9. Atomically replaces `release/`. On any error, the previous `release/`
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
   `pnpm run test`, `pnpm run design:lint`, and `pnpm run build`. The build
   receives `TAURI_SIGNING_PRIVATE_KEY`,
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from GitHub Actions secrets and
   `TAURI_UPDATER_PUBLIC_KEY` from a repository Actions variable, with
   `OCG_REQUIRE_UPDATER_ARTIFACTS=1`.
3. Uploads the per‑runner `release/` directory as a
   `release-<platform>` Actions artifact.

Each runner also runs a smoke flow on the freshly built bundle:

- **Windows CLI** — verifies `SHA256SUMS`, expands the ZIP, runs
  `key add` / `key list` / `key disable` / `key enable` / `status` /
  `key remove` against a temp data dir, then starts `serve --port=19042`
  and waits for `id="app"` to appear in the dashboard HTML.
- **macOS / Linux CLI** — the same `key` and `serve` flow plus a `lipo
  -archs` check that the macOS CLI is a universal binary.
- **Windows GUI** — downloads the current published installer, silently
  installs and launches it, writes a data sentinel, and enables `auto_start`.
  It then runs the candidate NSIS package through `/UPDATE /P /R /ARGS
  --startup` without uninstalling, verifies the old PID exits, the candidate
  version returns through `/settings/update-status`, and both the sentinel and
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OCG Manager` survive.
  It then runs the existing off/on cleanup checks, silently uninstalls, and
  confirms user data remains. A manual dispatch whose candidate is already
  the latest release may use the candidate-only install path.
- **macOS GUI** — mount the DMG, `codesign --verify --deep --strict`,
  check the binary is universal with `lipo -archs`, launch with
  `--startup`, wait for the dashboard.
- **Linux GUI** — `dpkg-deb --info` / `dpkg-deb --contents` on the deb,
  `file` on the AppImage, then launch under
  `dbus-run-session -- xvfb-run -a env APPIMAGE_EXTRACT_AND_RUN=1
  WEBKIT_DISABLE_COMPOSITING_MODE=1` and wait for the dashboard.

When a `v*` tag is pushed, a downstream `draft-release` job downloads the
three per-runner Actions artifacts, assembles their payloads/signatures and
`compose.example.yaml` in `release/`, generates `latest.json` with immutable
tag URLs and bundle-aware platform keys, regenerates `SHA256SUMS` over the
manifest, signatures, and every other attachment, and creates or updates a
**draft** GitHub Release. It never publishes the release. After reviewing the
draft and the native smoke results, publish the release in GitHub or run
`gh release edit vX.Y.Z --draft=false`.

Generate the production updater key once on a trusted workstation, writing it
to a secure path outside the checkout (do not run this with a repository path):

```powershell
node node_modules/@tauri-apps/cli/tauri.js signer generate -w <secure-path-outside-repository>/ocg-updater.key
```

Store the private-key content and password as the two GitHub Actions secrets
named above. Keep at least two independently stored encrypted backups of both
the private key and its password. If they are lost, already-installed clients
that trust the matching public key cannot receive another in-app update and
will need a new direct-install bootstrap. The public key is safe to share; this
project injects its content through the `TAURI_UPDATER_PUBLIC_KEY` repository
Actions variable instead of committing it. Store the generated key contents,
not local filesystem paths, in GitHub. Updater signatures prove that a payload
was issued by this project, but are separate from operating-system code signing.

Publishing the GitHub Release triggers `.github/workflows/container.yml`.
That workflow checks out the release tag, builds and smoke-tests the hardened
`linux/amd64` container, pushes `X.Y.Z`, `X.Y`, stable-release `latest`, and
`sha-<12-character-commit>` tags to `ghcr.io/klarkxy/opencode-go-mgr`, and
records an SPDX SBOM, BuildKit SLSA provenance, and GitHub signed provenance.
Treat `X.Y.Z` and `sha-*` as release-specific tags that must not be moved;
`X.Y` and `latest` are moving channels. Only the manifest digest is technically
immutable.

A manual dispatch can backfill an existing release tag and must opt in before
updating `latest`. Its GitHub signing certificate identifies the workflow ref
that triggered the dispatch, even though the build checks out the requested
tag. Do not describe a historical manual backfill as tag-triggered provenance;
normal `release.published` runs use the release tag context.

After publication, record the digest and verify both the OCI index and the
GitHub attestation. Constrain verification to this signer workflow:

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:X.Y.Z
docker buildx imagetools inspect --raw \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest>
docker buildx imagetools inspect --format '{{json .SBOM}}' \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> > sbom.json
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
```

SBOM and provenance are supply-chain metadata, not vulnerability scanning.
The GitHub attestation signs the provenance statement; this project does not
currently add a separate Cosign image signature.

Current Windows installers are unsigned and macOS uses ad‑hoc signing
(`-`), not Developer ID notarization. Keep releases in draft until
native smoke checks and platform warnings are reviewed. Windows/Linux
ARM64, 32‑bit x86, RPM, Snap, and app stores remain unsupported. Signed
in-app update is limited to updater-enabled installed desktop builds; 1.4.1,
development builds, CLI, and Docker retain the direct/manual path.

### CI Coverage Boundaries

The repository has no `pull_request` workflow, so these checks do not run
automatically on PRs. The container workflow covers `linux/amd64` only and
runs after a release is published or manually dispatched. CI does not drive
real desktop UI interactions or test container ARM64, backup/restore, database
downgrade, migration rollback, an upstream account, or a real Gateway request.
Its container smoke checks TCP health, dashboard HTML, auth status, the bundled
license, and a protected settings request returning `401`. Run the relevant
checks manually when changing uncovered paths.

## Release Procedure

1. Choose `X.Y.Z` and set it in `package.json`,
   `src-tauri/tauri.conf.json`, the workspace `Cargo.toml`, and
   `src-tauri/Cargo.toml`.
2. Run `cargo check --workspace --all-targets` to refresh `Cargo.lock`, then
   run `pnpm install --frozen-lockfile`, `cargo fmt --all -- --check`,
   `pnpm run test`, `pnpm run design:lint`, and `pnpm run build`. Commit the
   intended lockfile changes; never hand-edit them.
3. Compare against the previous public tag, review the diff and
   current-platform `release/` payloads, then commit the version, lockfile,
   documentation, and release-note changes.
4. Merge the reviewed change first. On the final commit already on `main`,
   create an annotated tag with
   `git tag -a vX.Y.Z -m "OCG Manager vX.Y.Z"`, then push the tag. Never tag a
   branch commit that will later be squash-merged.
5. Wait for every `release.yml` matrix job and `draft-release` to pass. Review
   the draft's seven platform payloads, `compose.example.yaml`, `SHA256SUMS`,
   smoke logs, platform warnings, and notes generated from the previous-tag
   diff.
6. Publish the draft in GitHub or run
   `gh release edit vX.Y.Z --draft=false`, then verify the public release.
7. Wait for `container.yml`, verify the GHCR package is public, inspect its
   version and digest, and anonymously pull the full-version tag.

Treat published assets and tags as immutable. If a published payload is wrong,
ship a new patch version; do not replace the asset or retarget the tag.

## Release Validation Checklist

Run these checks **before** publishing a `v*` tag. The CI smoke flow
covers most of them; the manual parts need a real desktop.

- [ ] `pnpm run test`, `pnpm run design:lint`, `pnpm run build` are
      green on the three runners.
- [ ] Exercise Gemini `generateContent` and `streamGenerateContent` with
      `x-goog-api-key` against both a Chat-native and a Messages-native model;
      confirm Google JSON/SSE error and usage envelopes. Confirm `countTokens`
      and `embedContent` return the documented `501` response.
- [ ] Confirm a non-empty Gemini `safetySettings` request returns `400`, while
      `null` and `[]` remain accepted. Exercise representative unsupported
      `cachedContent`, `fileData`, Google Search, and `urlContext` requests so
      they fail before any upstream request is billed. Treat `topK` and
      `thinkingConfig` as compatibility hints only; do not assert native
      Gemini-equivalent semantics in smoke tests.
- [ ] Exercise authenticated Claude Desktop model discovery and Messages alias
      rewriting. Save all three mappings through the dashboard API, restart
      with the same data directory, and verify the mappings survive. On a
      non-loopback dashboard, verify the mapping API returns `401` without a
      valid session.
- [ ] `git diff --check` is clean, the previous-tag diff contains only the
      intended release scope, and all four version manifests plus the three
      local Cargo lock entries agree.
- [ ] Each runner's `release/SHA256SUMS` matches every payload in that
      directory; the aggregated release checksum matches all seven platform
      payloads plus `compose.example.yaml`.
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
- [ ] Build the container locally and confirm UID/GID `10001`, bundled
      `LICENSE`, read-only/capability hardening, dashboard authentication, and
      backup/restore ownership on an isolated volume.
- [ ] Review the draft GitHub Release notes and the unsigned/ad‑hoc
      warnings before flipping `--draft=false`.
- [ ] After publishing, confirm `container.yml` passed and anonymously pull
      `ghcr.io/klarkxy/opencode-go-mgr:<version>` by the expected digest; then
      verify the signer workflow, SBOM, and SLSA provenance.

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
- Gemini is a compatibility input, not a native upstream. Only
  `generateContent` and `streamGenerateContent` forward requests;
  `countTokens` and `embedContent` return `501`. Non-empty safety policy,
  cached content, file-backed media, Google-hosted tools, and other semantics
  that cannot survive conversion are rejected with `400`. `topK` and
  `thinkingConfig` may be accepted for client compatibility but are not a
  promise of equivalent behavior on Chat Completions or Messages upstreams.
  Every other non-null `generationConfig` field must be mapped or rejected;
  never add a silent pass-through exception.

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
[用户指南](USER.zh-CN.md) · [Back to README](../README.md)
