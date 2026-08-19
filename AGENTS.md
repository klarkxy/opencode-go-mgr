# AGENTS.md — ocg-manager

本文件给 AI 编码助手使用。以当前代码为准，别按旧 README 或过期需求文档补不存在的东西。
用户文档见 `docs/USER*.md`；产品 README 只做仓库入口（定位、下载、三步上手、推荐协议分组），不要按旧 README 里的长表补文档。发版细节以 `docs/MAINTAINER.md` 为准。

## 项目事实

- 产品：OCG Manager，OpenCode-Go 多账号本地管理器。
- 前端：Vue 3 + TypeScript + naive-ui，源码在 `src/`。
- 前端 API：`src/api/tauri.ts` 是历史命名，当前封装 HTTP `/dashboard/api`，不是 Tauri `invoke()`。
- 面板视图（侧栏顺序）：Dashboard / Access Keys / Accounts / Pricing / Applications / Logs / Settings。
- UI 文案：接入凭证在面板上显示为 **Key**（不要写 “Gateway Key”）；设计系统以 `DESIGN.md` + `src/theme.ts` 为准。
- Rust workspace：`crates/ocg-core`、`crates/ocg-cli`（二进制名 `ocg-manager-cli`）、`src-tauri`。
- 核心 Gateway：Axum + Tokio + reqwest，默认监听 `127.0.0.1:9042`；同一端口提供 OpenAI Chat Completions / Responses、Anthropic Messages、Gemini `generateContent` 客户端入口与 Claude Desktop 别名入口。
- 接入凭证分两层：主 Key 是遗留 `AppConfig.gateway_key` 标量（`AppConfig::validate` 强制 trim 后非空，永不可禁用/删除，日志归因固定 `gateway_keys::PRIMARY_KEY_ID`，名称快照 "Primary"）；子 Key 存于 SQLite `sub_gateway_keys` 表（schema v20，活跃上限 64，软删保留名称、清除明文），仅经 `/dashboard/api/settings/keys*` 生命周期 API 变更，每次成功变更 bump settings_revision。主/子 Key 值互斥由统一闸口 `gateway_keys::ensure_primary_value_allowed` 在 dashboard、Tauri settings 与子 Key 启用路径强制；config JSON 不再内嵌 Key 列表。
- 鉴权收集 Bearer / x-api-key / x-goog-api-key 全部非空候选头，任一命中凭证快照（`CoreStateInner.credential_snapshot`，含主 Key 与启用子 Key）即通过，首个命中按候选头顺序归因；快照同源供 forward log 名称快照。`GET /dashboard/api/connection` 返回接入中心专用轻量 `ConnectionInfo`（含明文 Key，处于 dashboard 会话保护层），Dashboard 不持有完整 settings 形状。
- 持久化：SQLite。GUI 数据目录为 Windows `%USERPROFILE%\.ocg-mgr` 或 macOS/Linux `~/.ocg-mgr`；CLI 默认 `~/.ocg-mgr-cli`。
- 桌面端：Tauri v2 跨平台托盘应用，主窗口默认隐藏；托盘/单实例逻辑用系统浏览器打开 `http://127.0.0.1:<port>/dashboard/`，回环监听自动跳过登录。
- Tauri commands 仍注册在 `src-tauri/src/commands/`，但不是当前 Vue dashboard 的主调用路径。
- 每个节点都由自己的 dashboard 管理；项目不提供远端同步或 Admin API。
- 全局出站代理保存在 `AppConfig`，模式为自动（系统/环境）、手动 HTTP、强制直连；模型、账号测试、用量、价格与已安装桌面版的签名升级下载必须遵守同一套策略。reqwest 路径复用 `http_client.rs`，Tauri updater 用其 `proxy` / `no_proxy` 对齐，不得按账号配置或绕过。
- 非回环监听使用单管理员登录。Docker 可通过 `OCG_ADMIN_USERNAME` 和 `OCG_ADMIN_PASSWORD` 首次初始化（两个必须同时设置，只设一个会启动报错）；未提供时由首个注册者创建管理员。
- 设置页通过受保护的 `/dashboard/api/settings/check-update` 手动检查 GitHub 最新 Release。内置升级公钥的已安装桌面版可继续下载、校验签名并原位安装；开发构建、CLI、Docker 与尚未进入升级通道的旧版保留发布页/手动覆盖路径。
- 价格表通过受保护的 `GET /dashboard/api/pricing`、`PUT /dashboard/api/pricing/multipliers`、`POST /dashboard/api/pricing/refresh` 管理；只在用户点击刷新时访问 `https://opencode.ai/docs/go/`，不得自动轮询。
- 公开 GitHub Release 发布后，`.github/workflows/container.yml` 在 amd64（`ubuntu-24.04`）与 arm64（`ubuntu-24.04-arm`）原生 runner 上构建并冒烟验证 `linux/amd64` 与 `linux/arm64` 镜像，各架构按 digest 推送后合并为同一 tag 的多架构 OCI index，发布到 `ghcr.io/klarkxy/opencode-go-mgr`。Compose 默认使用该镜像；本地源码构建需设置 `OCG_IMAGE=ocg-manager:local` 后执行 `docker compose up -d --build`。
- `.github/workflows/quality.yml` 在 PR / `main` 上拆成三个并行 job：Web 测试/类型/lint、Linux workspace Rust 测试/Clippy（占位 `dist/`，编译含 Tauri crate）、Windows Tauri 定向测试（占位 `dist/`，不跑 Vite）。`release.yml` 的手动候选（即使选择 tag ref）始终无签名且可只构建指定平台；只有 `v*` tag 的 push 事件才构建三平台并读取 repository signing secrets。tag push 视为单维护者的明确发布授权：工作流在校验附件集合与组装产物逐名一致（数量由产物推导，不硬编码）、升级签名、公钥连续性与 GitHub 服务端 digest 后自动公开同一个未变更 draft。
- 容器固定以 UID/GID `10001` 运行并内置 `LICENSE`；Compose 透传可选的 `OCG_MANAGER_ENCRYPTION_KEY` 以支持显式密钥恢复，正常部署仍优先保留卷内 `.encryption-key`。
- 下游访问根地址优先级：非空 `OCG_CLIENT_ROOT_URL` > SQLite 手工值 > 前端按生产 origin / 开发 Gateway 端口自动推导。环境变量覆盖只读且不得写回 SQLite。
- Gemini 客户端使用 `/v1beta/models/{model}:generateContent` 或 `:streamGenerateContent`（也接受 `/v1/models/...`），可用 `x-goog-api-key` 鉴权；Gemini 只是客户端格式，Gateway 始终转换到已知模型的推荐上游协议。
- 模型协议能力在 `protocol.rs` 的 `MODEL_PROTOCOLS` 硬编码：`preferred` 对齐官方 Go docs endpoint 表，`supported` 为测试账号探测结论。客户端协议 ∈ supported 时透传，否则转到 preferred；请求路径禁止试探协议（防双计费）。`grok-4.5` 仅 `supported = Responses`（Chat 入口须转换）。`gpt-5.6-luna` preferred 仍是 Responses，但 Chat 已可透传。
- Free 模型策略：`AppConfig.free_model_routing` 为 `deny` / `explicit`（默认）/ `prefer`；Zen free 与 Go 使用独立 `cooldown_free_until`。Zen free 额度按出口 IP 共享，429 后整条 free 通道冷却，不换 Key 重试；401/403 仍按账号故障切换。prefer 仅映射 `deepseek-v4-flash`/`mimo-v2.5`，上下文粗估装得下才降级，free 耗尽回落 Go。free 通道成功行记 `cost_state=free`，不计入 Go 额度。
- Claude Desktop 使用 `/claude-desktop/v1/messages` 与 `/claude-desktop/v1/models`；`sonnet`、`opus`、`haiku` 映射保存在 `AppConfig.claude_desktop_models`，由受保护的 `GET/PUT /dashboard/api/claude-desktop/models` 管理。
- 托管账号（Beta）：`setup_step` 为 `google_account`（UI：登录身份，可跳过）→ `opencode_registration` → `payment` → `key_verification` → `ready`。`PATCH .../setup` 允许前进一格或回退更早步骤，禁止跳步与直接 `ready`。创建草稿可编辑邀请链接并写回 `opencode_invite_url`（`DEFAULT_OPENCODE_INVITE_URL` 为演示默认）。浏览器目标含 Google/GitHub 注册与登录、邀请 URL、控制台 `https://opencode.ai/auth`。
- 已完成账号的额度：官方 `https://opencode.ai/zen/go/v1/usage`（`go_usage.rs`）是周期性校准基线；本地 forward_logs 在上次成功校准后仍做实时估算。`usage_sync.rs` 协调手动与后台路径：ready+enabled 且近 24h 有本地活动的账号约每小时对账，无活动约每天；禁用/非 ready/空 Key 不自动刷新。全局并发 1、带抖动与可注入 clock/jitter/fetch 缝；启动不轰鸣。手动 `POST /dashboard/api/accounts/{id}/usage/refresh` 仍可用，服务端每账号 60s 节流（成功/失败都算），并发去重，返回 Retry-After/`next_allowed_at`；失败保留上次基线与 last-success。本地最大 Go 用量 ≥80% 时最多每 15 分钟加速对账一次。真实推理 429 仍写现有 cooldown/selector，并额外调度约 1–2 分钟后的官方对账（不 inline）；官方失败或 `status=rate-limited` 永不写推理冷却。成功后按最早 `resetsAt`（加有界抖动）并尊重活跃/非活跃节奏再调度。失败退避：5m → 15m → 1h → 6h。sync 元数据落在 accounts 表（schema v21）。共享实现含 CAS/三窗口原子校准与全局代理。公开 Go docs 尚未列出该 endpoint。`console_usage.rs` 已冻结弃用，至少两个 minor 且有稳定真号证据后再删。勿为刷新引入 CDP 自动化。

## 关键文件

- `crates/ocg-core/src/gateway/`：OpenAI / Anthropic / Gemini 客户端协议路由与转换、Claude Desktop 别名改写、转发、选择器、冷却、费用统计。
- `crates/ocg-core/src/gateway_keys.rs`：子 Key 生命周期门面（`sub_gateway_keys` CRUD 封装、凭证快照构建/重建、`PRIMARY_KEY_ID` 常量、跨层值唯一闸口）；改 Key 存储或鉴权快照时从这里入手。
- `crates/ocg-core/src/http_client.rs`：核心出站 HTTP 客户端共享的全局代理策略。
- `crates/ocg-core/src/dashboard.rs`：当前 Vue 面板使用的 `/dashboard/api`。
- `crates/ocg-core/src/go_usage.rs`：官方 Go usage 客户端（`/zen/go/v1/usage`）；手动与调度刷新共用。
- `crates/ocg-core/src/usage_sync.rs`：自适应官方用量同步（节流、去重、活跃/非活跃节奏、80% 加速、429/reset 调度、失败退避）。后台循环按 `CoreState` 启停（Gateway start 时 spawn，随 CoreState drop 退出；不是可取消的 per-Gateway task）。失败退避地板不可被阈值/节奏/reset 提前；真实推理 429 的 1–2 分钟调度是刻意覆盖。
- `crates/ocg-core/src/console_usage.rs`：冻结弃用的 Profile Cookie/HTML 控制台用量实现；勿调用、勿扩展。
- `crates/ocg-core/src/db.rs`：SQLite schema、迁移、查询。
- `crates/ocg-core/src/models.rs`：共享 serde 类型和 `AppConfig`（含 `DEFAULT_OPENCODE_INVITE_URL`）。
- `crates/ocg-core/src/pricing.rs`：OpenCode Go 价格快照、倍率与额度估算。
- `crates/ocg-cli/src/main.rs`：CLI `serve`、`key`、`status`。
- `src-tauri/src/lib.rs`：Tauri 启动、Gateway 启动、托盘、命令注册。
- `src-tauri/src/updater.rs`：签名桌面升级器桥接；由受保护的 dashboard HTTP API 触发，不向 WebView 暴露 updater command 权限。
- `src-tauri/src/tray.rs`：托盘菜单和 dashboard 打开逻辑。
- `src/views/`：Dashboard / Keys / Accounts / Pricing / Applications / Logs / Settings。
- `src/components/ManagedAccountWizard.vue`：托管注册向导（步骤回退、Google/GitHub）。
- `src/views/application-guides.ts`：16 个应用教程注册表和 `APPLICATION_MODEL_METADATA` 能力表（改数量/协议/脱敏/能力时同步测，并同步 USER 能力表；README 只保留推荐协议分组）。
- `src/theme.ts` + `DESIGN.md`：主题 token 与设计规范；改色/字号时两边一起改。
- `vite.config.ts`：`build.target`/`esbuild` 须支持 top-level await（`@novnc/novnc`）。
- `docs/`：USER（用户可见事实与模型表）、MAINTAINER、防滥用声明、CONTRIBUTORS、文档索引。根目录 README 是落地页，不是能力表/协议表的权威副本。

## 常用命令

```powershell
pnpm install
pnpm run hooks:install   # once per clone; enables pre-commit cargo fmt
pnpm run dev
pnpm run build:web
pnpm run test
pnpm run design:lint
pnpm run release:check
pnpm run build
```

开发前先退出 release 托盘程序，释放单实例锁和 `9042` 端口，然后执行 `pnpm run dev`。Tauri 会启动 Vite，并在 Gateway 就绪后打开 `http://127.0.0.1:30001/dashboard/`；前端由 Vite 热更新，Rust 由 Cargo 增量编译并重启进程。

`pnpm run build` 只用于当前原生平台的最终 release 构建，并在成功后原子替换 `release/`；只验证前端时用 `pnpm run build:web`。Windows 仅发布 x64 NSIS 安装包，macOS 发布 Universal DMG，Linux x64 发布 AppImage 和 deb；CLI 压缩包必须包含同级 `dist/` 与 `LICENSE`。

## 本地 Release 构建（Windows 速查）

完整发版流程、CI 矩阵与签名密钥见 `docs/MAINTAINER.md`。本地 smoke 构建：

1. 确保 `pnpm` 可用（`packageManager: pnpm@10.29.2`）。PATH 无 pnpm 时可在用户目录做 shim。
2. 退出已安装 release 版，释放单实例锁和 `9042`：

   ```powershell
   Get-NetTCPConnection -LocalPort 9042 -ErrorAction SilentlyContinue |
     Select-Object OwningProcess | Get-Process | Stop-Process -Force
   ```

3. 版本一致：`package.json`、`src-tauri/tauri.conf.json`、workspace `Cargo.toml`、`src-tauri/Cargo.toml`，以及 `compose.example.yaml` 的标题与默认镜像。
4. 执行 `pnpm run build`（调用 `scripts/release.mjs`）。

签名相关环境变量（与 CI / MAINTAINER 一致）：

- `TAURI_SIGNING_PRIVATE_KEY`：私钥内容，或仓库外安全路径（脚本会规范化为 Tauri 的 path 形式）。
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥密码（如有）。
- `TAURI_UPDATER_PUBLIC_KEY`：公钥内容；须匹配 `src-tauri/updater-public-key.sha256`。
- `OCG_REQUIRE_UPDATER_ARTIFACTS=1`：强制要求签名产物；缺密钥则失败。

**没有 `TAURI_SIGNING_PRIVATE_KEY` 时只产出普通本地包，不能用于应用内升级，仅做本地 smoke test。**

Windows 上 Tauri 可能把 `src-tauri/Cargo.toml` 与 `src-tauri/gen/schemas/*.json` 行尾改成 CRLF；构建后如需干净工作树：

```powershell
git checkout -- src-tauri/Cargo.toml src-tauri/gen/schemas/desktop-schema.json src-tauri/gen/schemas/windows-schema.json
```

## 开发约束

- 工作区可能是脏树。先看 `git status --short`，不要回退不是你改的内容。
- 复杂度控制以完整交付为前提：优先复用现有代码和简单架构，但不得因此省略需求明确要求的流程、状态、错误处理或用户体验。
- 不要新增 Tauri `invoke` 前端路径，除非你明确要恢复桌面 WebView 内调用；当前主路径是 HTTP dashboard。
- 安全边界别省：Gateway 鉴权、key 存储混淆、HTTP URL 校验、冷却状态写入、SSE 透传都不能为了简化拿掉。
- 不要重新引入远端同步；远端节点通过自己的 dashboard 管理。
- `auto_start` 仅在 Windows release/安装版 Tauri 桌面进程中可用；HTTP dashboard 依据运行时能力显示开关，开发构建、CLI、Docker、macOS 和 Linux 不暴露该设置。
- `show_dock_icon` 仅在 macOS Tauri 桌面进程中可用；关闭后保留菜单栏托盘图标。Windows、Linux、CLI 与 Docker 不暴露该设置。
- 改文档时保持中英对、路径与 TOC 一致；用户可见事实以代码与 `docs/USER*.md` 为准。协议表跟 `protocol.rs`，能力表跟 `application-guides.ts`，由 USER 镜像。不要把透传矩阵、能力表或熔断长文再写回 README。
- 改 UI 外观时遵循 `DESIGN.md`：六档字号、七主题、接入中心首屏、Key 命名；主题实现以 `src/theme.ts` 为准。

## 测试策略

- Rust 逻辑优先跑 `cargo test -p ocg-core`。
- CLI 改动跑 `cargo test -p ocg-manager-cli`，必要时用临时 data dir 做真实 `key add/list`、`status`。
- 前端改动跑 `pnpm run build:web`。
- Rust 和前端回归跑 `pnpm run test`；GUI/打包改动跑当前平台的 `pnpm run build`。需要声明真实桌面可用时，要实际启动安装包、DMG 或 AppImage 并验证 dashboard/gateway 行为。

## 当前已知缺口

- 曾运行多 Key 开发构建（PR #43 config 内嵌形态，从未发布）的数据库：历史 forward_logs 已按当时主 Key 的随机 UUID 回填（非 NULL，不触发重回填），新二进制下 `/logs/forward/keys` 会出现旧 UUID 与 `PRIMARY_KEY_ID` 两个同名 "Primary" 条目。属可接受开发期残留；洁癖修复可删除 data.sqlite 重建，不提供自动迁移。首次启动照常把 NULL 历史行回填到 `PRIMARY_KEY_ID`。
- `/embeddings` 与 Gemini `embedContent` 未实现；Gemini `countTokens` 返回 `501`，供 Gemini CLI 回退本地估算。
- Gemini `generateContent` / `streamGenerateContent` 已实现，但非空 `safetySettings`、`cachedContent`、`fileData`、Google Search、`urlContext` 及未明确支持的非空 `generationConfig` 字段会返回 `400`。`topK` 与 `thinkingConfig` 只能视为跨协议兼容提示，不能承诺与 Gemini 原生后端语义等价。
- 流式 usage 依赖上游 usage chunk；Chat 流式请求会设置 `stream_options.include_usage`。没有 chunk 时会记为 `success_no_usage`。
- Tauri 隔离浏览器 command 存在，但当前 HTTP dashboard 没有按钮调用它。
- `src-tauri/src/commands/*` 与 `crates/ocg-core/src/dashboard.rs` 有部分重复逻辑；当前不要大拆，除非同时迁移缺失行为并补验证。
- 当前不发布 Windows/Linux ARM64、32 位 x86、RPM、Snap 或应用商店包，也没有 Windows Authenticode 正式签名或 Apple notarization。v1.4.1 需要最后一次直接覆盖安装首个 updater-enabled Release；不要先卸载，之后的已安装桌面版可在设置页完成签名升级。
