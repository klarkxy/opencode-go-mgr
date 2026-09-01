[简体中文](ci.zh-CN.md)

# CI Workflows

## quality.yml — the reusable quality gate

`.github/workflows/quality.yml` runs on pull requests and pushes to `main`.
`release.yml` calls it once for production tag releases. Manual candidate
dispatches skip it: they build a commit that already passed the gate. The gate
is three parallel jobs so frontend failures surface without waiting for Rust,
and Windows does not rebuild the dashboard:

- **Web** — `pnpm run contract:v3:check`, Node tests (`scripts/*.test.mjs`
  and `src/**/*.test.ts`), `pnpm run build:web` (TypeScript checking plus a
  Vite production bundle), `DESIGN.md` lint, and Compose validation.
- **Rust** — `cargo fmt`, locked workspace tests, and Clippy. The desktop
  crate is excluded (`--exclude ocg-manager`): only it needs WebKit headers
  and a stub `dist/index.html`, and the Windows job already covers it, so
  this leg installs no system packages at all. Linux compile coverage of
  `src-tauri` lives in the release build matrix.
- **Windows Tauri** — `cargo test -p ocg-manager --lib` / `clippy` against a
  stub `dist/index.html`. This is the only quality job that compiles the
  desktop crate; it also covers Windows-only auto-start without pnpm or
  Vite.

Node/pnpm and Rust build caches are shared across compatible runs. Pull
requests restore the Rust cache but do not write it; failed non-PR runs still
write the Rust cache so a follow-up fix can reuse the compile.

## release.yml — candidates and tag releases

`.github/workflows/release.yml` runs on `workflow_dispatch` and on `v*` tags.

- A manual candidate can select Windows x64, macOS Universal, Linux x64, or
  all three platforms and intentionally produces unsigned smoke artifacts,
  even when a manual dispatch selects a tag as its ref.
- Only a `push` event for a `v*` tag forces the complete three-platform
  matrix and supplies the repository signing secrets. For this
  single-maintainer repository, pushing that tag is the explicit publication
  authorization.
- On a production tag push the quality gate runs in parallel with an
  Ubuntu preflight that parses the extracted installer smoke under `pwsh`,
  runs the release-helper tests, validates all version manifests, and proves
  the signing pair and committed public-key fingerprint before any native
  runner starts. Manual candidates skip the quality job and receive empty
  signing values in preflight.

After preflight, each selected native runner restores its platform Rust cache
and installs dependencies. The workflow injects signing secrets only when its
plan proves the event is an actual `v*` tag push; manual jobs receive empty
signing values and run the ordinary unsigned build. Both paths run CLI/GUI
smokes and upload `release-<platform>` with seven-day retention. The generic
test/type/lint suite is not repeated on all three native runners.

## Per-runner smoke flows

- **Windows CLI** — verifies `SHA256SUMS`, expands the ZIP, runs
  `key add` / `key list` / `key disable` / `key enable` / `status` /
  `key remove` against a temp data dir, then starts `serve --port=19042` and
  waits for `id="app"` to appear in the dashboard HTML.
- **macOS / Linux CLI** — the same `key` and `serve` flow plus a
  `lipo -archs` check that the macOS CLI is a universal binary.
- **Windows GUI** — downloads the current published installer, silently
  installs and launches it, writes a data sentinel, and enables `auto_start`.
  It then runs the candidate NSIS package through `/UPDATE /P /R /ARGS
  --startup` without uninstalling, verifies the old PID exits, the candidate
  version returns through `/settings/update-status`, and both the sentinel
  and `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OCG Manager`
  survive. Installer processes have an explicit timeout and are waited
  independently from the `/R`-launched GUI process so a successful restart
  cannot hang CI; uninstall completion is bounded and checked through removal
  postconditions. It then runs the existing off/on cleanup checks, silently
  uninstalls, and confirms user data remains. The PowerShell implementation
  lives in `scripts/smoke-windows-release.ps1` instead of an inline YAML
  block. A manual dispatch whose candidate is already the latest release may
  use the candidate-only install path.
- **macOS GUI** — mount the DMG, `codesign --verify --deep --strict`, check
  the binary is universal with `lipo -archs`, launch with `--startup`, wait
  for the dashboard.
- **Linux GUI** — `dpkg-deb --info` / `dpkg-deb --contents` on the deb,
  `file` on the AppImage, then launch under `dbus-run-session -- xvfb-run -a
  env APPIMAGE_EXTRACT_AND_RUN=1 WEBKIT_DISABLE_COMPOSITING_MODE=1` and wait
  for the dashboard.

`scripts/smoke-windows-release.ps1` uses Dashboard V3 for both the published
baseline and the candidate. Auto-start writes obtain the live `revision` /
`processGeneration` pair from `GET /dashboard/api/v3/settings` and send a
CAS-aware V3 `PUT`.

## draft-release and verify-release

On a `v*` tag push, `draft-release` downloads the three per-runner artifacts,
assembles their payloads, signatures, `compose.example.yaml`, and
`cpa-config.example.yaml` into
`release/`, generates `latest.json` with immutable tag URLs and bundle-aware
platform keys, regenerates `SHA256SUMS` over the manifest and all attachments,
and creates or updates a **draft** GitHub Release.

`verify-release` checks that GitHub asset names match the assembled `release/`
set exactly. The local verifier pins the current 16-file contract, re-derives
`latest.json`, recomputes every checksum, verifies all four updater signatures,
and compares each downloaded artifact with the digest reported by GitHub Release
storage.

The draft job passes its numeric Release ID downstream. Verification and
publication re-check that exact ID, tag, and draft state, because the tag lookup
endpoint does not expose draft Releases.

SemVer prerelease tags such as `v1.5.8-beta.1` use the same signed tag path
and the same immutable attachments. The updater manifest keeps the full
prerelease identifier in payload names and download URLs; the Windows packaged
smoke accepts that same prerelease `CandidateVersion`.

Generated notes begin with a Beta warning that managed account registration and
isolated browser profiles are still unverified. The warning also lists the
unverified Google/OpenCode signup and payment flows, noVNC keyboard/clipboard,
and first-public GHCR paths, and notes that gateway, redaction, and release
changes are included. The preview is not production-ready.

When a later stable tag is released, automatic notes skip same-version
prerelease tags as their baseline, preserving the full feature scope since the
previous stable release.

## publish-release — publish only the verified tag build

The `v*` tag push is the single maintainer's release authorization.
`publish-release` runs automatically after `verify-release` succeeds. It
compares the current asset/digest-set fingerprint with the verified fingerprint
and rejects any draft that changed after verification. Manual candidates cannot
reach the draft, verification, or publication jobs. A missing signing key,
failed smoke, or failed verification leaves the Release unpublished.

Publication is serialized through the repository-wide
`release-moving-channels` queue. Before publishing, the job compares the
candidate with the current GitHub latest release and advances `latest` only for
a strictly newer stable SemVer. A delayed older run can still publish its
immutable release without rolling `latest` back.

Prerelease tags mark both draft and public Release as `prerelease=true`,
force `make_latest=false`, and skip the stable-only latest-channel comparison.
Stable tag behavior is unchanged.

## Updater signing key

Generate the production updater key once on a trusted workstation and write it
outside the checkout:

```powershell
node node_modules/@tauri-apps/cli/tauri.js signer generate -w <secure-path-outside-repository>/ocg-updater.key
```

- Store the private-key content and password as repository Actions secrets
  named `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The release workflow references them
  only when the event-derived plan identifies an actual `v*` tag push; manual
  candidates receive empty values and remain unsigned.
- Repository secrets are not isolated by an Environment. If another
  write-capable maintainer is added, reassess a protected signing Environment
  or tag ruleset before the next release.
- Keep at least two independently stored encrypted backups of both the
  private key and its password. If they are lost, already-installed clients
  that trust the matching public key cannot receive another in-app update and
  will need a new direct-install bootstrap.
- The public key is safe to share; this project injects its content through
  the `TAURI_UPDATER_PUBLIC_KEY` repository Actions variable instead of
  committing it. Store the generated key contents, not local filesystem
  paths, in GitHub.
- Updater signatures prove that a payload was issued by this project, but are
  separate from operating-system code signing.

## Key continuity and rotation

`src-tauri/updater-public-key.sha256` is the production trust anchor. Normal
CI has no override: a mismatched repository variable fails both signing
preflight and release verification. Key rotation is a break-glass recovery,
not a routine secret update. Generate and back up the new pair, prepare a
direct-install bootstrap for every existing client, and update the committed
fingerprint in an explicitly reviewed security change. Changing the variable
or fingerprint alone leaves old installed clients unable to trust releases
signed only by the replacement key.

## container.yml — the image pipeline

`.github/workflows/container.yml` accepts a published-Release event, but a
Release published by `release.yml` with `github.token` does not recursively
start another workflow. After the signed tag pipeline publishes, explicitly
dispatch `container.yml` for that tag with `publish_latest=true` for a stable
release.

The workflow checks out the release tag and builds each architecture natively:
amd64 on `ubuntu-24.04`, arm64 on `ubuntu-24.04-arm`. Release artifacts are
never built under QEMU emulation. Only the amd64 leg builds smoke images via
`docker-bake.hcl` and runs the smoke suite for the main
`ghcr.io/klarkxy/opencode-go-mgr` service and the
`ghcr.io/klarkxy/opencode-go-mgr-browser` sidecar. The main smoke checks the
dashboard, authentication, and license. The browser smoke starts Xvfb/noVNC
under a read-only root with zero capabilities, a Chromium-compatible seccomp
profile, and no host-published port, then uses the token-protected control API
to launch an ordinary Chromium process with a persistent profile. The arm64
leg builds and pushes both images without smoke.

Verified results — two images per architecture — are pushed by digest without
a mutable name, then enter the repository-wide serialized tag queue. Only
`resolve` interprets the requested tag or optional `source_ref`; both native
build legs check out that resolved full commit SHA and fail if `HEAD` differs.
The publishing job uses the immutable `github.workflow_sha`, so the privileged
registry helper matches the reviewed workflow definition, not executable files
from a hotfix ref.

Before writing a user-visible tag, the publishing job runs `docker buildx
imagetools create --dry-run` to assemble each candidate OCI index locally. It
hashes the returned JSON and validates both architecture children plus the
index version/revision annotations. The main and browser `X.Y.Z` and
`sha-<12-character-commit>` tags are preflighted against locally known digests
before the browser tags, then the main tags, are created and verified. Existing
immutable tags are accepted only at the exact candidate digest.

An empty Docker credential directory must then anonymously pull both exact
version tags, and GitHub must publish signed provenance for both final index
digests. Only then does the same serialized job re-read every remote moving
channel and preflight the pair again. Stable `X.Y` and opted-in `latest`
converge both images to the candidate or retain an already-aligned newer pair;
the browser moves before the main image, and a split pair fails closed. Each
architecture image records an SPDX SBOM and BuildKit SLSA provenance.
`X.Y.Z` and `sha-*` are release-specific immutable tags; `X.Y` and `latest`
are monotonic moving channels. The browser image is a GHCR package, not a
GitHub Release asset, so the native release keeps only the assembled GitHub
attachments. The workflow compares that exact set, and the local verifier pins
the current 16-file contract.

Package visibility is managed separately from the linked repository, so the
workflow cannot use its repository token to make a package public. A new
browser package does not exist until its first digest is pushed. The first
`container.yml` run that creates it is therefore expected to stop at the
anonymous-pull gate while the package still has GitHub's default private
visibility.

This is the only bootstrap exception: set the new browser package to
**Public** (and confirm the main package is also Public), then manually rerun
`container.yml` for the same tag. Immutable-tag replay is accepted only at the
same digests, so the rerun completes the original publication without
replacing artifacts. Do not treat the container distribution as complete until
that rerun is green. Every later release must pass the anonymous gate on its
first run.

Before the first stable release on this dual-architecture path, publish a
temporary SemVer prerelease and dispatch `container.yml` with
`publish_latest=false`. Use that rehearsal to prove both native runners,
package visibility, anonymous pulls, exact index children, and both signed
provenance records. Do not use a stable tag as the rehearsal, and do not
advance `X.Y` or `latest` until the prerelease run is fully green.

After tag publication, the gate pulls both exact-version tags with an empty
Docker credential directory. A private or inaccessible package therefore fails
`container.yml` instead of appearing as a successful public Compose
dependency.

A manual dispatch can backfill an existing release tag, but it must opt in
before updating `latest`. `resolve` checks out the exact `refs/tags/<tag>` ref
or the explicit hotfix `source_ref`, verifies the release tag and repository
version, and emits one full SHA; no downstream job re-resolves the symbolic
input. Rebuilding different bytes for an existing full-version or `sha-*` tag
fails instead of overwriting it; only an exact-digest replay is accepted. Its
GitHub signing certificate identifies the workflow ref that triggered the
dispatch, even though the build checks out the resolved release commit. Do not
describe a historical manual backfill as tag-triggered provenance; normal
`release.published` runs use the release tag context.

After publication, record the digest and verify the OCI index and GitHub
attestation against this signer workflow:

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:X.Y.Z
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr-browser:X.Y.Z
docker buildx imagetools inspect --raw \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest>
docker buildx imagetools inspect --format '{{json .SBOM}}' \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> > sbom.json
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr-browser@sha256:<browser-digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
```

SBOM and provenance are supply-chain metadata, not vulnerability scanning.
The GitHub attestation signs the provenance statement; this project does not
currently add a separate Cosign image signature.

Current Windows installers are unsigned; macOS uses ad-hoc signing (`-`), not
Developer ID notarization. Review native candidate smoke results and these
platform warnings before pushing the release tag, because a successful tag
workflow publishes automatically. Windows/Linux ARM64, 32-bit x86, RPM, Snap,
and app stores remain unsupported. Signed in-app update is limited to
updater-enabled installed desktop builds; 1.4.1, development builds, CLI, and
Docker keep the direct/manual path.

## CI Coverage Boundaries

Pull requests run the three-job quality gate: frontend checks (including the
Dashboard V3 contract), Linux workspace Rust tests and Clippy excluding the
Tauri desktop crate, and the Windows job that covers desktop-crate compilation
and unit tests including Windows-only Tauri behavior. Native installer and
package smokes run only on manual release candidates or tag runs. The
container workflow covers `linux/amd64` and `linux/arm64`, each built on its
native runner and smoke-tested on amd64 only; it runs after a release is
published or is manually dispatched.

CI does not drive real desktop UI interactions, launch real Claude Desktop or
Gemini CLI clients, or test backup/restore, database downgrade, migration
rollback, upstream accounts, or real gateway requests. Rust tests cover
Gemini/Claude Desktop routing, authentication, alias rewriting, non-stream
conversion, SSE event shapes, Dashboard V3 CAS, the V2 410 tombstone, v27
open/backup, and host lifecycle source contracts, but they cannot prove that
new versions of third-party clients still accept the generated configuration.

The main container smoke checks TCP health, dashboard HTML, auth status, the
bundled license, and a protected settings request returning `401`. The browser
smoke launches real Chromium and verifies its profile and absence of public
ports, but it does not log in to Google/OpenCode, operate noVNC
keyboard/clipboard, or make a real payment. Google data-center-IP risk,
desktop browser discovery, cookie persistence across restarts, and remote
account switching remain manual checks.

---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](ci.zh-CN.md) · [Docs index](../README.md)
