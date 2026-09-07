# 2.2.0 local release-candidate review

[简体中文](README.zh-CN.md)

The local candidate is ready to enter the release pipeline. This review started
from `39e85125`; changes remain in the working tree. No commit, push, tag,
publication, or deployment was performed.

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

The Windows full run exposed the new free-model alias diagnostic failure; its
corrected gateway suite was rerun successfully. The final core suite was also
rerun. Compiler-only syntax normalization was checked with formatting and strict
Clippy. Linux completed its full non-desktop suite on the integrated changes.

## Artifacts

| File in `release/` | SHA-256 |
| --- | --- |
| `ocg-manager_2.2.0_windows-x64-setup.exe` | `cc832e436a794fdce9c955229f963fbcbb501794aba6be63c8836d7c278e762e` |
| `ocg-manager-cli_2.2.0_windows-x64.zip` | `f46d48e4104729e992113ed346318e70a7a7a74fc2d5cbb11109cee1996d0eff` |

## Limits and review dispositions

- macOS native execution, installer install/uninstall, signed updater delivery,
  and live supplier/OAuth sessions were not exercised here. The signed platform
  matrix and platform smokes remain part of the authorized release workflow.
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

## Next acceptance gate

Full release readiness remains unproven. The next step requires committing the
candidate changes, pushing a separate candidate branch, opening a draft PR for
Quality, then dispatching `release.yml` with `target=all`. Workflow inspection
confirms manual candidates do not create or publish a GitHub Release and receive
no updater signing key. They cover three-platform builds, Windows installation
checks, macOS mounted application startup, and Linux virtual-display startup.
These checks do not replace signed-update validation or the manual platform
checks above. Candidate branch publication and the draft PR have not been
authorized or performed.
