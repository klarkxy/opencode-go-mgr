[English](README.md)

# 文档索引

OCG Manager 文档按读者拆分：打开与你角色匹配的那本指南即可。修改成对页面时保持中英同步；文档与代码打架时，HEAD 的代码说了算。

## 目录

| 读者 | English | 简体中文 | 范围 |
| --- | --- | --- | --- |
| 产品概览 | [../README.md](../README.md) | [../README.zh-CN.md](../README.zh-CN.md) | 定位、下载矩阵、三步上手、指向 USER |
| 终端用户 | [USER.md](USER.md) | [USER.zh-CN.md](USER.zh-CN.md) | [`user/`](user/) 下 20 章，包含专门的[供应商](user/add-provider.zh-CN.md)与[应用](user/add-application.zh-CN.md)接入指南 |
| 维护者 | [MAINTAINER.md](MAINTAINER.md) | [MAINTAINER.zh-CN.md](MAINTAINER.zh-CN.md) | [`maintainer/`](maintainer/) 下 13 章：结构、开发循环、架构、发布矩阵、CI、验证 |
| 防滥用 | [OPENCODE_GO_ANTI_ABUSE.md](OPENCODE_GO_ANTI_ABUSE.md) | [OPENCODE_GO_ANTI_ABUSE.zh-CN.md](OPENCODE_GO_ANTI_ABUSE.zh-CN.md) | OpenCode-Go 使用边界 |
| 贡献者 | [CONTRIBUTORS.md](CONTRIBUTORS.md) | 中英同页 / bilingual | 社区贡献者 |
| 设计系统 | [../DESIGN.md](../DESIGN.md) | 英文为准 | 主题、字号、Key 命名、布局规则 |
| AI 助手 | [../AGENTS.md](../AGENTS.md) | 仅英文 | 项目事实与编码约束 |

根目录保留跳转页，把旧路径指到本目录：

- [../CONTRIBUTORS.md](../CONTRIBUTORS.md) → [CONTRIBUTORS.md](CONTRIBUTORS.md)
- [../OPENCODE_GO_ANTI_ABUSE.md](../OPENCODE_GO_ANTI_ABUSE.md) → [OPENCODE_GO_ANTI_ABUSE.md](OPENCODE_GO_ANTI_ABUSE.md)
- [../OPENCODE_GO_ANTI_ABUSE.zh-CN.md](../OPENCODE_GO_ANTI_ABUSE.zh-CN.md) → [OPENCODE_GO_ANTI_ABUSE.zh-CN.md](OPENCODE_GO_ANTI_ABUSE.zh-CN.md)

## 事实归属

文档冲突时以下列为准，并回修另一侧。

| 主题 | 权威来源 |
| --- | --- |
| 用户可见产品行为 | 代码 + [`user/`](user/) 下章节（如 [`user/accounts.md`](user/accounts.md) / [`user/accounts.zh-CN.md`](user/accounts.zh-CN.md)、[`user/providers.md`](user/providers.md) / [`user/providers.zh-CN.md`](user/providers.zh-CN.md)） |
| Plan 目录 | `crates/ocg-domain/src/provider.rs`（`BUILTIN_PLANS`、密封 `ProviderRegistry`）；`crates/ocg-core/src/provider.rs` 是兼容门面加 Custom URL 检查；[`user/accounts.md`](user/accounts.md) / [`user/accounts.zh-CN.md`](user/accounts.zh-CN.md) 镜像 live 与 pending 家族；[`user/providers.md`](user/providers.md) / [`user/providers.zh-CN.md`](user/providers.zh-CN.md) 镜像控制面。Custom 是 `ConfigurableHttpAdapter`，不是基类或动态插件 |
| 供应商合约 | `crates/ocg-core/src/provider_contracts.rs`；[`user/providers.md`](user/providers.md) / [`user/providers.zh-CN.md`](user/providers.zh-CN.md) 镜像范围、本地目录、开关、探测与请求时选择 |
| 客户端别名 | `crates/ocg-gateway/src/alias.rs`；`crates/ocg-core/src/alias.rs` 是兼容门面；[`user/gateway.md`](user/gateway.md) / [`user/gateway.zh-CN.md`](user/gateway.zh-CN.md) 镜像约定 |
| 本地 `GET /v1/models` | `crates/ocg-core/src/gateway/handler.rs`；已鉴权的 Go 别名 ∪ 已保存 Zen Free 别名 ∪ 用户定义供应商公开模型 ∪ 有效已启用协议的合格 Custom ID；GET 本身不访问上游。镜像在 [`user/gateway.md`](user/gateway.md) |
| 应用选择器列表 | `crates/ocg-core/src/dashboard_v3/`（`GET /dashboard/api/v3/application-models`）经 `control/observability.rs`；Go 可路由别名 ∩ 当前价格；不含 Custom。镜像在 [`user/applications.md`](user/applications.md) |
| Custom API HTTP | `crates/ocg-core/src/custom.rs` + `custom_http.rs`；受信管理员目的地，Direct/Manual/Auto，不跟随重定向，独立鉴权。镜像在 [`user/accounts.md`](user/accounts.md) |
| 模型推荐/已验证协议表 | `crates/ocg-domain/src/protocol.rs`（`MODEL_PROTOCOLS`）；转换内核 `crates/ocg-gateway/src/protocol.rs`；宿主 parse/stream `crates/ocg-core/src/gateway/protocol.rs`；[`user/protocol-conversion.md`](user/protocol-conversion.md) / [`user/protocol-conversion.zh-CN.md`](user/protocol-conversion.zh-CN.md) 镜像该表 |
| 模型上下文/输入/推理能力表 | `src/views/application-guides.ts`（`APPLICATION_MODEL_METADATA`）；[`user/applications.md`](user/applications.md) / [`user/applications.zh-CN.md`](user/applications.zh-CN.md) 镜像该表 |
| 面板 HTTP API | `crates/ocg-core/src/dashboard_v3/` 挂载于 `/dashboard/api/v3`；冻结契约 `schema/dashboard-api-v3.schema.json`；SPA 客户端 `src/api/dashboard-v3.ts` + 投影 `src/api/dashboard.ts` / `src/api/providers.ts` + 契约类型 `src/api/generated/dashboard-v3.ts`。组成根 `crates/ocg-core/src/host_router.rs`。受保护未版本化 `/dashboard/api` REST 返回结构化 `410`；auth/session、browser WebSocket 与推理入口语义独立。 |
| 接入凭证 | SQLite `access_keys`（当前 schema v35，v27 引入），经 `crates/ocg-core/src/gateway_keys.rs` 与 `dashboard_v3/keys.rs`。主 Key id 为 `PRIMARY_KEY_ID`。历史 `sub_gateway_keys` 不是现行权威 |
| SQLite 库版本 | `crates/ocg-core/src/db.rs`（`CURRENT_SCHEMA_VERSION = 35`）；升级/备份/回滚约定：[maintainer/storage-migration.zh-CN.md](maintainer/storage-migration.zh-CN.md) |
| 发布产物、CI、签名 | [`maintainer/release-artifacts.md`](maintainer/release-artifacts.md) / [`maintainer/release-artifacts.zh-CN.md`](maintainer/release-artifacts.zh-CN.md)、[`maintainer/ci.md`](maintainer/ci.md) / [`maintainer/ci.zh-CN.md`](maintainer/ci.zh-CN.md)、[`maintainer/releasing.md`](maintainer/releasing.md) / [`maintainer/releasing.zh-CN.md`](maintainer/releasing.zh-CN.md) |
| 当前版本钉 | `package.json` / workspace `Cargo.toml` / `src-tauri/tauri.conf.json` / `compose.example.yaml` |
| 接入凭证文案 | 面板显示 **Key**（`DESIGN.md`、`src/theme.ts`），不使用 “Gateway Key” |
| 设计 token | [../DESIGN.md](../DESIGN.md) + `src/theme.ts` |
| 助手约束 | [../AGENTS.md](../AGENTS.md) |

Docker 示例里的版本钉应与当前发版线一致（现为 **v2.0.0**）。升版后 [`user/docker.md`](user/docker.md)、[`.env.example`](../.env.example)、[`compose.example.yaml`](../compose.example.yaml) 中的版本钉需要同步更新，避免停留在旧 patch。产品 README 不再钉 clone tag。

## 阅读顺序

1. **新用户** — README 快速开始 → 用户指南 `overview` → `architecture` → `install` → `first-client` → `accounts`（导入 Key / 托管 Beta）→ `providers`（目录、按模型覆盖、探测、范围内价格）→ `gateway` → `routing` → `applications` → `troubleshooting`。
2. **Docker / CLI 运维** — 用户指南 `overview` → `architecture` → `docker` 与 `cli` → `accounts` → `providers` → `routing` → `logs-settings`；托管注册需要时启用 browser profile。
3. **集成作者** — 上游先读用户指南 [`add-provider`](user/add-provider.zh-CN.md)，下游客户端先读 [`add-application`](user/add-application.zh-CN.md)，再读维护者指南 `extending` 了解仓库施工细节。
4. **贡献者** — 维护者指南 `layout` → `development` → `architecture` → `state-and-lifecycle` → `http-routes` → `conventions`；编码时以 `AGENTS.md` 为准（V3 crate 拆分、`/dashboard/api/v3`、当前 schema v35、`access_keys`、托管向导、刷新额度、协议表、Key 命名、类型化用户定义供应商）。未版本化 `/dashboard/api` REST 与 Tauri `invoke` 均不是当前面板路径。
5. **发版负责人** — 维护者指南 `release-artifacts` → `ci` → `releasing` → `known-debt`；发版前检查清单含托管回退与刷新额度路径。
6. **UI / 主题** — 先读 `DESIGN.md`，再改 `src/theme.ts` 与对应 Vue 页面。

## 编辑约定

- 成对指南保持中英标题结构与 TOC 锚点一致。
- 优先写短而可核验的事实，少写宣传句。
- 文档应反映当前实现：远端同步、Admin API、embeddings 与未支持的 Gemini 字段不在当前支持范围内，已知缺口见 [`user/limits.md`](user/limits.md)、[`maintainer/known-debt.md`](maintainer/known-debt.md) 与 `AGENTS.md`。Command Code GOAT 是已上线的固定官方源路由：公开目录不是 Key 验证，供应商矩阵控制模型供应，不存在账号级 GOAT/全部或 Max 模式。Custom API 已在受信管理员边界下作为可路由目的地运行，以 [`user/accounts.md`](user/accounts.md) 为准，不再使用 Phase-1 休眠或 SSRF denylist 表述。`requested_alias` 不是有效日志字段。`GET /v1/models` 与 `application-models` 不是同一份列表，后者只是 Go 可路由别名 ∩ 当前价格（不含 Custom 与用户定义供应商）。存在独立的供应商页，Zen 目录刷新属于供应商页，不在账号卡中。当前 schema 版本是 v35，`sub_gateway_keys` 不再是现行 Key 表，也没有 live Tauri `invoke` command；未版本化 `/dashboard/api` REST 不是面板主路径。SPA 走 `/dashboard/api/v3`。受保护 V2 REST 返回结构化 `410`；auth/session、browser WebSocket 与推理入口语义保持独立。适配器注册表静态密封；类型化 Provider 定义可作为数据持久化并绑定 Configurable HTTP。关于浏览器、可能计费的真实推理或已安装桌面版的实测结论，只在实际检查过的情况下写入。
- 发版升版后，同步更新 [`user/docker.md`](user/docker.md)、[`.env.example`](../.env.example)、[`compose.example.yaml`](../compose.example.yaml) 中的 clone tag 与镜像钉（`pnpm run release:check` 会核对 compose/package 版本一致性）。
- 产品 README 只保留入口内容：定位、下载、三步上手、一条 curl、Docker 指针、推荐协议分组，以及指向用户指南的链接；透传矩阵、能力表与熔断长文不属于 README 范围。
