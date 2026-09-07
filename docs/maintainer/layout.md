[简体中文](layout.zh-CN.md)

# Layout

```
crates/ocg-domain          IDs, catalogs, protocol policy
crates/ocg-gateway         I/O-free alias, AttemptSpec, selector, JSON convert
crates/ocg-infra           crypto, proxy/inference HTTP, log SQL
crates/ocg-core            SQLite, Dashboard V3, adapters, executor
crates/ocg-cli             ocg-manager-cli: serve / key / status
crates/ocg-browser-worker  Linux Chromium sidecar (no ocg-* deps)
src/                       Vue 3 dashboard (HTTP Dashboard V3 only)
src-tauri/                 Desktop host capabilities registered into CoreState
schema/                    frozen dashboard-api-v3.schema.json
docs/                      USER / MAINTAINER / anti-abuse
scripts/                   release, contract, smokes
```

Workspace members and `rust-version` are in the root `Cargo.toml`. Dashboard
HTTP client: `src/api/dashboard-v3.ts` plus presenters in
`src/api/dashboard.ts` and `src/api/providers.ts`. Images:
`Dockerfile`, `Dockerfile.browser`, `compose.yaml`, `compose.example.yaml`,
`docker-bake.hcl`.
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](layout.zh-CN.md) · [Docs index](../README.md)
