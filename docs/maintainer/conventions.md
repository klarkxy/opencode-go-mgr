[简体中文](conventions.zh-CN.md)

# Coding Conventions

- **Keep the crate DAG.** Domain and gateway stay I/O-free. Facades reexport
  item-by-item. Adapters return `AttemptSpec`. `forward_once` is one upstream
  call. Dashboard V3 does not import `gateway`.
- **No Tauri `invoke()` paths.** The Vue data path is HTTP `/dashboard/api/v3`.
- **Do not revive protected V2 REST.** New JSON is V3. The 410 tombstone stays.
- **Do not weaken security boundaries.** Gateway authentication, key
  obfuscation, URL validation, cooldown writes, SSE pass-through, and the
  ConnectionInfo secret boundary stay.
- **Do not add remote sync.** Each node is managed through its own dashboard.
- **Capability-gate `auto_start` and `show_dock_icon`.** Windows x64, macOS,
  and Linux x64 release/installed Tauri processes inject the login-start sync
  hook; Dock is macOS Tauri only.
- **Local Alias lists stay local.** Authenticated `GET /v1/models` and dashboard
  `application-models` must not grow request-time upstream discovery. The
  explicit Zen Free refresh on Providers is the only directory-fetch exception
  and is restricted to the fixed official endpoint. Do not equate the two
  lists; do not invent a `requested_alias` log field.
- **Respect `parking_lot::Mutex` non-reentrancy.** Drop the guard before
  calling another lock holder.

## Documentation

- Code is authoritative. Follow the source-of-truth pointers in `AGENTS.md`.
- The root README is a landing page. Capability tables live in `docs/user/`;
  procedures live in `docs/maintainer/`. Keep paired English and `.zh-CN.md`
  heading structure, links, and TOC anchors aligned.
- `DESIGN.md` and `src/theme.ts` own visual tokens and the user-facing **Key**
  name. Package manifests and `compose.example.yaml` own version pins.
- Describe current behavior. Put known gaps in `docs/user/limits.md` or
  `docs/maintainer/known-debt.md`. Write in the affirmative; negate only when
  a first-time reader of that page would reasonably assume the opposite.
- Repository docs and `AGENTS.md` carry shared project facts. Keep personal
  model choices, agent roles, and local tool paths in user-level configuration.

---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](conventions.zh-CN.md) · [Docs index](../README.md)
