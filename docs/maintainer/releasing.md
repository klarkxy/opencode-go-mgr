[简体中文](releasing.zh-CN.md)

# Release Procedure

1. Choose `X.Y.Z` (or an immutable SemVer prerelease such as
   `X.Y.Z-beta.N`) and set it in `package.json`, `src-tauri/tauri.conf.json`,
   the workspace `Cargo.toml`, `src-tauri/Cargo.toml`, and the header plus
   default main and browser images in `compose.example.yaml`.
2. Run `cargo check --workspace --all-targets` to refresh `Cargo.lock`, then
   run `pnpm install --frozen-lockfile`, `cargo fmt --all -- --check`,
   `pnpm run test`, `pnpm run test:tooling`, `pnpm run design:lint`,
   `pnpm run contract:v3:check`, `pnpm run release:check`, and `pnpm run build`. Commit the intended
   lockfile changes; never hand-edit them.
3. Compare against the previous public tag, review the diff and
   current-platform `release/` payloads, then commit the version, lockfile,
   documentation, and release-note changes.
4. Merge the reviewed change first. On the final commit already on `main`,
   create an annotated tag with `git tag -a vX.Y.Z -m "OCG Manager vX.Y.Z"`
   (preserving the prerelease suffix when applicable), then push the tag.
   Never tag a branch commit that will later be squash-merged.
5. Wait for `quality`, `preflight`, every native matrix job, `draft-release`,
   `verify-release`, and `publish-release` to pass. Confirm that publication
   converted the same verified draft, then review the exact assembled
   attachments, smoke logs, platform warnings, and notes generated from the
   previous-tag diff.
6. Explicitly dispatch `container.yml` for the published tag (for example,
   `gh workflow run container.yml --ref main -f tag=vX.Y.Z -f publish_latest=true`;
   omit `source_ref`), wait for it to pass, verify both GHCR packages are
   public, inspect each version and digest, and anonymously pull both
   full-version tags.

Published assets and tags are immutable. Fix a bad release with a new
patch version; never replace an asset or retarget a tag.

## Release Validation Checklist

Run this checklist before pushing a `v*` tag. CI covers most items;
desktop-specific steps need a real machine.

- [ ] All three jobs in the reusable quality gate are green (including
      `contract:v3:check`); the tag-only signed `release:check` passed; every
      selected `pnpm run build` and platform smoke is green.
- [ ] `git diff --check` is clean, the previous-tag diff contains only the
      intended release scope, and all four code version manifests,
      `compose.example.yaml`, plus all workspace package entries in `Cargo.lock`
      agree.
- [ ] Each runner's `release/SHA256SUMS` matches every payload in that
      directory; `verify-release` accepted the exact assembled asset set,
      updater manifest, four signatures, checksums, and GitHub server digests.
- [ ] Run `cargo test -p ocg-core gemini` and
      `cargo test -p ocg-core claude_desktop`. Exercise Gemini
      `generateContent` and `streamGenerateContent` with Bearer, `x-api-key`,
      and `x-goog-api-key` against both a Chat-native and a Messages-native
      model; confirm Google JSON/SSE error and usage envelopes, HTTP status,
      and SSE termination match the client protocol. Confirm `countTokens`
      and `embedContent` return the documented `501` response and an unknown
      action returns `404`.
- [ ] Confirm a non-empty Gemini `safetySettings` request returns `400`,
      while `null` and `[]` remain accepted. Exercise representative
      unsupported `cachedContent`, `fileData`, Google Search, and `urlContext`
      requests so they fail before any upstream request is billed. Treat
      `topK` and `thinkingConfig` as compatibility hints only; do not assert
      native Gemini-equivalent semantics in smoke tests.
- [ ] Exercise authenticated Claude Desktop model discovery and Messages
      alias rewriting. Save all three mappings through
      `PUT /dashboard/api/v3/claude-desktop/models` (with CAS tokens), restart
      with the same data directory, and verify the mappings survive. On a
      non-loopback dashboard, verify the mapping API returns `401` without a
      valid session. Confirm the retired V2
      `PUT /dashboard/api/claude-desktop/models` is authenticated `410`.
- [ ] Open the **Applications** view and confirm all 17 guides are present
      and selectable. Spot-check that copied results contain no masked key,
      and actually launch Claude Desktop and Gemini CLI once each for a text
      and a tool call.
- [ ] Cover schema v16 migration, schema v27 (`access_keys`, pre-v3 backup +
      SHA-256 sidecar, dropped `sub_gateway_keys` and `accounts.usage_sync_*`,
      ciphertext validated not rewritten), v29 SCNet removal, v30/v31 contract
      compatibility, v32 single-protocol Custom conversion, v33 upstream-model
      identity, v34 CPA singleton state, v35 Provider/Plan identity migration
      and its preflight backup, Alias / upstream log identity, optional native
      cost, historical GOAT verification states normalize to `not_required`, Zen Free catalog
      persistence, provider contract scopes / model-protocol tables, legacy
      `key + ready`, managed transitions (forward one step / rewind earlier
      steps / no skip-forward), pending-route isolation, the invite URL
      allowlist and demo-default write-back, and the
      `2xx`/`429`/`401`/`403`/network/`5xx` key-verification branches. Confirm
      that no DTO or log contains a plaintext key except the session-protected
      `GET /dashboard/api/v3/connection` payload.
- [ ] Confirm authenticated `GET /v1/models` and protected
      `GET /dashboard/api/v3/application-models` are local reads and make no
      upstream request. `/v1/models` is currently routeable published aliases
      plus eligible Custom IDs; `application-models` is Go routeable aliases ∩
      the active pricing snapshot (highspeed inherits the base row) and must
      not include Custom. Unknown models return `400` on Chat / Responses /
      Messages / Gemini unless they match that `/v1/models` list. Command
      Code catalog refresh is public and keyless; GOAT preset rows start on,
      extra discovered rows start off, and all-off scopes disappear from
      `/v1/models`. These local-list checks do not require live provider keys;
      do not perform billable inference as part of the release smoke.
- [ ] Bounded fake-upstream Custom API smoke (no live provider key): URL
      credentials are rejected; a valid new account defaults enabled while
      verification remains optional; a `2xx` JSON object marks verification
      successful without changing enablement; declared model/protocol
      forwarding succeeds; redirects are denied; dashboard/client auth is not
      forwarded and only the protocol-derived Bearer or `x-api-key` is sent;
      successful logs are unpriced/`cost_state=unknown` with no quota debit;
      editing the URL, key, capabilities, or protocol re-pends verification
      while preserving enablement. Confirm Direct/Manual/Auto inherit the
      process-wide proxy.
- [ ] Verify Edge/Chrome priority on Windows and browser discovery on
      macOS/Linux. With two accounts, prove profile isolation and cookie
      persistence across restart. Reset must sign out of the console but keep a
      completed key; delete must clean new and legacy profiles; legacy WebView
      profiles must not be imported.
- [ ] Manually complete (optional) sign-in identity → invite URL → OpenCode
      login → payment review → key paste. A tester performs real payment only
      when explicitly intended. Console opens `opencode.ai/auth`. Log in once
      for a legacy key account and verify later access to authoritative quota
      and referral use. For a ready Key account and a ready managed account,
      exercise **Refresh quota** against official `/zen/go/v1/usage` (invalid
      key, 409 after a key change, and network/schema failures must error
      clearly and leave the previous local calibration). Cover desktop
      and Docker sidecar paths.
- [ ] On Windows, run the installer once, confirm SmartScreen warning text,
      open the dashboard, add an account, send one request.
- [ ] On macOS, mount the DMG, confirm the **Open Anyway** flow works, open
      the dashboard, add an account, send one request.
- [ ] On Linux, install the `.deb`, launch the AppImage, confirm the
      dashboard opens under Xvfb on CI and under a real Wayland or X11
      session locally.
- [ ] On Windows, verify `auto_start` toggles the `HKCU\...\Run\OCG Manager`
      value and that the value is removed on uninstall.
- [ ] Confirm `scripts/release.mjs` reported a successful atomic replacement
      of `release/` and that the previous `release/` is gone.
- [ ] Build both containers locally and confirm UID/GID `10001`, bundled
      `LICENSE`, read-only/capability hardening, dashboard authentication,
      and backup/restore ownership on isolated volumes. Run
      `docker compose --profile browser up -d` and verify one Chromium,
      noVNC keyboard/clipboard, account switching, sidecar restart, 1 GiB
      shm, no public port, and two-volume backup/restore.
- [ ] Review the intended GitHub Release notes and the unsigned/ad-hoc
      warnings before pushing the tag; after publication, confirm the same
      notes and exact verified asset set are public.
- [ ] After publishing, confirm `container.yml` passed and anonymously pull
      the main image and `ghcr.io/klarkxy/opencode-go-mgr-browser:<version>`
      by their expected digests; verify each signer workflow, SBOM, and SLSA
      provenance, while the GitHub Release remains the exact assembled asset
      set.
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](releasing.zh-CN.md) · [Docs index](../README.md)
