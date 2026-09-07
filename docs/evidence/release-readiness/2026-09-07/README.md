# 2.2.0 release-candidate review

[简体中文](README.zh-CN.md)

This review started from `39e85125`. Fixes are committed on the candidate branch;
draft PR #57 is open, and three-platform candidate CI passed as described below. No tag,
production release, or deployment was performed.

## Repairs

- No-auth user-defined Providers now forward without requiring a Key or leaking
  client credentials. OpenCode identity headers are confined to Go/Zen routes,
  while explicit supported identity values are preserved there.
- Saved, previously unknown Zen Free models default to Chat. Their exact raw
  names and suffix-stripped aliases both route; unsaved names still fail locally
  and explicit protocol-off overrides remain effective.
- Account edits and Ollama billing changes commit in one transaction. Injected
  billing failures preserve the original fields, Key ciphertext, and billing tier.
- Host-cipher database opens probe persisted account ciphertext before migration,
  including current-schema databases. Decrypt failures do not rewrite the data.
- Desktop initialization follows single-instance ownership. A second launch does
  not initialize another profile, and a failed listener bind stops startup.
- Unix CPA ownership uses a transient mode of the existing executable and a
  lifetime pipe. Its supervisor retains the process-group identity through bounded
  TERM/KILL cleanup. Host death, early CPA exit, and retained output pipes no longer
  leave unsafe group signaling or an indefinitely blocked log reader.
- Frontend stores reject stale loads; logout invalidates pending Key loads;
  same-process CAS revisions cannot move backwards. Business conflicts retain
  their actual error, unchanged settings arrays adopt remote updates, and primary
  Key rotation refreshes the displayed/copyable value.
- Added the missing Traditional Chinese label and the multi-account probe warning.
  Corrected the obsolete navigation description and Zen test expectations.
  Rust minimum versions now match the locked dependencies at 1.88. Compiler-proposed
  equivalent syntax changes keep the strict warning gate clean.
- Windows product-rename updates now retain the old installation directory and
  write the new files there. Existing shortcuts migrate to the new name, and
  legacy installation records are removed only after an in-place replacement.
  Candidate CI exposed this defect; its final overwrite/uninstall smoke passed.

Grok and Kimi supplied bounded reviews and repairs. A separate design review and
implementation handled Unix process ownership, followed by an independent source
review. Returned changes and test evidence were inspected by the primary.

## Verification

| Boundary | Result |
| --- | --- |
| Frontend tests | 229 passed |
| Frontend types and production build | Passed |
| Release/tooling tests | 77 passed |
| Dashboard V3 generated contract | Passed; no schema change |
| Design lint, Rust formatting, diff whitespace | Passed |
| Windows Rust workspace campaign | All targets passed after rerunning the corrected gateway target |
| Final Windows core unit suite | 762 passed, 2 ignored |
| Final gateway integration suite | 86 passed |
| Linux workspace, excluding Tauri desktop | 1,446 passed, 3 ignored across 51 targets, Rust 1.88 |
| Linux CPA lifetime suite | 14 passed, including Host SIGKILL and descendant cleanup |
| Strict Windows workspace Clippy, all targets | Passed with warnings denied |
| Version and local release preflight | Passed for 2.2.0 |
| Windows installer and CLI archive build | Passed, unsigned local artifacts |
| Final archive checksums | Both verified at `release/` |
| CLI extracted from the final archive | Dashboard, CRUD, CAS conflict, auth, model mapping, non-stream and SSE passed against a local fixture upstream |
| Final desktop executable | Duplicate launch exits before opening a second data profile; first gateway remains; occupied port fails startup; restart and version passed |
| Actual dashboard interaction | Final dashboard and Accounts rendered; CPA model-catalog empty state checked; Key remains masked |
| Candidate Quality on `b72a82fb` | Web, Linux Rust, and Windows Tauri jobs passed |
| macOS and Linux native CPA checks | 14 tests passed on each platform |
| macOS Universal candidate | DMG architecture/ad-hoc signature check, mounted GUI startup, packaged CLI passed |
| Linux x64 candidate | AppImage startup under Xvfb, Debian package metadata/content, packaged CLI passed |
| Windows final candidate | Published 2.1.0 overwrite, preserved directory/data, shortcut migration, startup toggles, uninstall passed |

The Windows full run exposed the new free-model alias diagnostic failure; its
corrected gateway suite was rerun successfully. The final core suite was also
rerun. Compiler-only syntax normalization was checked with formatting and strict
Clippy. Linux completed its full non-desktop suite on the integrated changes.

## Artifacts

| File in `release/` | SHA-256 |
| --- | --- |
| `ocg-manager_2.2.0_windows-x64-setup.exe` | `494f90b1514a2be162a838be258ab0369f6def23958a6308e7cf9ac6a1ce1edc` |
| `ocg-manager-cli_2.2.0_windows-x64.zip` | `322e6878f10be4043ec1f660d171808357478ee291c72a58477c84698903abcb` |

These are the final Windows CI packages from `b72a82fb`, replacing the initial
local packages. Both were verified after download; the extracted final CLI and
its bundled dashboard also passed the local request smoke. macOS packages in
`release/ci-917984b9/macos/` passed local checksum verification. Linux packages
passed CI checksum verification; two local artifact downloads ended with EOF,
so this report does not claim a locally verified Linux copy.

## Limits and review dispositions

- Application integration is planned for retirement. The user explicitly excluded
  its testing, including Claude Desktop and Gemini CLI, from this review's
  acceptance scope. It is not a release blocker for this candidate.
- Native macOS/Linux CI and Windows installer acceptance passed. Signed updater
  delivery and live supplier/OAuth sessions were not exercised. Candidate checks
  do not constitute production signing or distribution verification.
- The existing XOR Key obfuscation is not authenticated encryption. The new
  decrypt probe cannot distinguish every wrong key that happens to produce valid
  UTF-8; no cryptographic-format migration was introduced.
- Applications navigation was intentionally removed earlier and was not restored.
  No-auth Provider deletion retains the documented account-first, non-cascading
  workflow; deleting its only account ends routing, and the empty Provider remains
  removable. Recreating the Provider recreates its no-auth singleton.
- Broad short request timeouts were not added: legitimate management and inference
  operations can take longer. Review claims about missing automatic CAS updates,
  the entire Traditional Chinese namespace, and gateway-log pagination were
  rejected after tracing the current implementation.
- Dependency advisory lookup failed because of local TLS/proxy errors. This report
  does not claim a completed dependency-vulnerability audit.

All verification used isolated test data. Temporary desktop processes were stopped
and the two application startup registry entries were checked/restored.

## CI evidence and remaining gates

- [Draft PR #57](https://github.com/klarkxy/opencode-go-mgr/pull/57).
- [Quality on b72a82fb](https://github.com/klarkxy/opencode-go-mgr/actions/runs/34081349089): passed.
- [Final Windows candidate](https://github.com/klarkxy/opencode-go-mgr/actions/runs/34081353017): passed on `b72a82fb`.
- [macOS Universal](https://github.com/klarkxy/opencode-go-mgr/actions/runs/34079065269/job/101610886207) and [Linux x64](https://github.com/klarkxy/opencode-go-mgr/actions/runs/34079065269/job/101610886205): passed on `917984b9`. Subsequent source changes affect only the Windows installer and its smoke; paired guides were also updated.

Code review, gateway checks, and three-platform candidate acceptance passed.
The candidate is ready to enter [the release procedure](../../../maintainer/releasing.md)
within the agreed scope; application-integration testing is excluded by the user.
Manual candidate workflows do not create a Release or receive updater signing
keys, so signing and distribution remain release-stage checks. No merge, tag,
production publication, or deployment was performed.
