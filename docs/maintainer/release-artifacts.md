[简体中文](release-artifacts.zh-CN.md)

# Release Artifacts

OCG Manager ships desktop installers for three platforms, a CLI archive for each,
and a multi-arch container image.

| Runner | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | NSIS current-user setup | x64 ZIP |
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
cpa-config.example.yaml
latest.json
SHA256SUMS
```

Each CLI archive ships with its executable, a `dist/` directory, and `LICENSE`.
Do not distribute the executable by itself — `serve` needs the sibling dashboard
assets. Windows has no portable GUI artifact.

The `linux/amd64` and `linux/arm64` containers are published separately as
`ghcr.io/klarkxy/opencode-go-mgr`. A GitHub Release contains the seven platform
payloads, the extra macOS updater archive, four updater signatures, the Compose
and CPA configuration examples, `latest.json`, and `SHA256SUMS` — currently 16 attachments. The local
verifier and the workflow both require the GitHub asset names and count to match
the assembled `release/` directory exactly. The runtime image places `LICENSE` at
`/usr/share/licenses/ocg-manager/LICENSE`.

## scripts/release.mjs

`scripts/release.mjs` builds and stages the release directory:

1. Validates that `package.json`, `src-tauri/tauri.conf.json`, the workspace
   `Cargo.toml`, `src-tauri/Cargo.toml`, and all three versioned fields in
   `compose.example.yaml` all agree. It also checks the Git tag, if any,
   against that version.
2. Resolves the updater signing mode before creating the staging tree. With
   `OCG_REQUIRE_UPDATER_ARTIFACTS=1`, either a missing private key or missing
   `TAURI_UPDATER_PUBLIC_KEY` fails before `release/` can be replaced. A
   configured public key must also match the committed SHA-256 continuity
   baseline in `src-tauri/updater-public-key.sha256`.
3. When a signing key is configured, merges `src-tauri/tauri.updater.conf.json`
   plus an ephemeral public-key config and enables Tauri updater artifacts.
   `TAURI_SIGNING_PRIVATE_KEY` accepts either the private-key content or its
   secure path outside the repository; there is no separate path variable.
   With no signing key, the script preserves the ordinary local build and
   prints that the result is for smoke testing, not an updater-enabled
   published release.
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
7. Builds the CLI binary, packages it with `dist/` and `LICENSE` into the
   per-platform archive, and on macOS uses `lipo` + `codesign -` to create
   the universal CLI.
8. Writes `SHA256SUMS` over every payload and signature in the staged
   `release/` directory.
9. Atomically replaces `release/`. On any error, the previous `release/` is
   preserved and the staged tree is removed.

`scripts/release.mjs` leaves Cargo's incremental build cache in `target/`
untouched.

`pnpm run release:check` validates versions, Compose, and any configured
signing key without building a native bundle. The keyless preflight covers
the unsigned contract. For a production tag push, each runner signs a
temporary payload with the repository signing secret and verifies it against
the continuity-checked `TAURI_UPDATER_PUBLIC_KEY` before starting the
expensive native build.

---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](release-artifacts.zh-CN.md) · [Docs index](../README.md)
