[简体中文](conventions.zh-CN.md)

# Coding Conventions

- **Ponytail principle — delete before adding.** Reuse existing helpers; add
  abstractions only for real needs. Keep call sites flat, but keep required
  CAS, tombstones, and fail-closed checks.
- **Keep the crate DAG.** Domain and gateway stay I/O-free. Facades reexport
  item-by-item. Adapters return `AttemptSpec`. `forward_once` is one upstream
  call. Dashboard V3 does not import `gateway`.
- **No Tauri `invoke()` paths.** The Vue data path is HTTP `/dashboard/api/v3`.
  Do not register `generate_handler`.
- **Do not revive protected V2 REST.** New JSON is V3. The 410 tombstone stays
  in front of retired `/dashboard/api/...` paths.
- **Do not weaken security boundaries.** Gateway authentication, key
  obfuscation, URL validation, cooldown writes, SSE pass-through, and the
  ConnectionInfo secret boundary are not simplification candidates.
- **Do not add remote sync.** Each node is managed through its own dashboard.
- **Capability-gate `auto_start` and `show_dock_icon`.** Only the Windows
  release/installed Tauri process injects the registry sync hook; Dock is macOS
  Tauri only.
- **Local Alias lists stay local.** Authenticated `GET /v1/models` and dashboard
  `application-models` must not grow request-time upstream discovery. The
  explicit Zen Free refresh on Providers is the only directory-fetch exception
  and is restricted to the fixed official endpoint. Do not equate the two
  lists; do not invent a `requested_alias` log field.
- **Respect `parking_lot::Mutex` non-reentrancy.** The CLI and core use it.
  When a function needs to call another lock holder, `drop` the guard first.
- **Match the surrounding style.** Same comment density, naming, and idiom as
  the existing code.

## Documentation ownership and editing

- Code is authoritative for current behavior. Before changing a runtime fact,
  follow the relevant source-of-truth pointer in `AGENTS.md`; do not turn this
  page into a second project-fact inventory.
- The root README is a landing page. Keep detailed capability, conversion, and
  integration material in the matching chapters under `docs/user/`; keep
  maintenance procedures in `docs/maintainer/`.
- User-visible workflows belong to paired `docs/user/*.md` and
  `*.zh-CN.md` guides. Keep their heading structure, links, and TOC anchors
  aligned; write English first, then synchronize Chinese.
- `DESIGN.md` and `src/theme.ts` own visual tokens and the user-facing **Key**
  name. Package manifests and `compose.example.yaml` own release version pins;
  update matching Docker examples in the same release change.
- Describe current behavior and explicit limits only. Put known gaps in
  `docs/user/limits.md` or `docs/maintainer/known-debt.md`, and claim browser,
  billable inference, or installed-desktop behavior only when that exact check
  was run.
- Keep the documentation index as an audience router. Its source ownership and
  editing guidance belongs here, not in another long index table.

---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](conventions.zh-CN.md) · [Docs index](../README.md)
