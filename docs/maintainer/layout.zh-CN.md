[English](layout.md)

# 仓库结构

仓库结构，以及各 crate、面板与宿主的边界。

```
ocg-manager/
├── crates/
│   ├── ocg-domain/     Pure identities, catalogs, protocol policy, Zen normalize
│   ├── ocg-gateway/    I/O-free alias, AttemptSpec, classify, selector, JSON convert
│   ├── ocg-infra/      Catalog-stripped crypto, proxy HTTP, inference HTTP, SQLite log SQL
│   ├── ocg-core/       Composition / control plane: state, SQLite, Dashboard V3, adapters, executor
│   ├── ocg-cli/        Headless CLI (`ocg-manager-cli`): serve / key / status
│   └── ocg-browser-worker/  Linux Chromium sidecar control service (independent of ocg-core)
├── browser/           Xvfb, Openbox, x11vnc, and noVNC startup script
├── src/               Vue 3 dashboard (TypeScript, naive-ui, Vite, Pinia)
│   ├── App.vue        Shell, auth, side rail, header
│   ├── api/
│   │   ├── dashboard-v3.ts            Hand-written `/dashboard/api/v3` client
│   │   ├── generated/dashboard-v3.ts  Types generated from the frozen JSON Schema
│   │   ├── dashboard.ts               Presenter over V3 for existing pages
│   │   ├── providers.ts               Provider-page presenter (Zen Free, pricing, model-protocol overrides)
│   │   └── dashboard-presenters.ts    Field projection (camelCase wire → page shapes)
│   ├── stores/        session, controlPlane (CAS tokens), connection, accounts, providers, settings
│   ├── components/    Account cards, managed wizard, pricing catalog, …
│   ├── i18n/          i18n setup + per-locale message tables + tests
│   ├── styles/        Theme tokens, design-system overrides
│   └── views/         Dashboard, Keys, Accounts, Providers, Applications, Logs, Settings, BrowserSession
├── src-tauri/         Tray host: Native Browser, Gateway Lifecycle, Desktop Settings, Updater
│   └── src/host/      Process-owned capabilities; no `invoke` commands
├── schema/            Frozen Dashboard V3 JSON Schema (`dashboard-api-v3.schema.json`)
├── docs/              USER / MAINTAINER / anti-abuse (EN+ZH), CONTRIBUTORS, index, v27 recovery note
├── scripts/           release, updater manifest, dashboard-v3-contract, smokes, …
├── AGENTS.md          Facts and constraints for AI coding assistants
├── DESIGN.md          Design system source of truth (linted in CI)
├── .github/workflows/ quality.yml, release.yml, container.yml
├── docker-bake.hcl    Parallel container smoke targets used by container.yml
├── Dockerfile         Multi-stage headless gateway image
├── Dockerfile.browser Chromium/noVNC sidecar image
├── compose.yaml       Source-build and image Compose service definition
└── compose.example.yaml  Pull-only Compose example attached to each Release
```

Workspace 成员在根目录 `Cargo.toml` 声明：`ocg-domain`、`ocg-gateway`、`ocg-infra`、`ocg-core`、`ocg-cli`、`ocg-browser-worker`、`src-tauri`（包名 `ocg-manager`）。二进制名：`ocg-manager-cli` 与 Tauri 应用。当前 workspace 版本为 `2.1.0`；`rust-version` 为 `1.85.0`；edition 为 `2024`。

生产面板使用 HTTP Dashboard V3（`src/api/dashboard-v3.ts` 以及 `src/api/dashboard.ts` / `src/api/providers.ts` 的 presenter）。不存在 `src-tauri/src/commands/` 模块，也没有 `#[tauri::command]` 表面；面板不调用 `invoke()`。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](layout.md) · [文档索引](../README.zh-CN.md)
