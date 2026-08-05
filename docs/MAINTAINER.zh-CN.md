[English](MAINTAINER.md)

# 维护者指南

本指南面向修改代码、构建发布、调试 Gateway 以及验证桌面端安装包的开发者。内容
覆盖仓库结构、开发循环、测试与构建流水线、架构说明、发布矩阵、CI 流程，以及
明确不在支持范围内的能力。

## 目录

- [仓库结构](#仓库结构)
- [环境前置条件](#环境前置条件)
- [开发模式](#开发模式)
- [检查与构建](#检查与构建)
- [架构说明](#架构说明)
- [升级与数据库迁移](#升级与数据库迁移)
- [发布产物](#发布产物)
- [CI 工作流](#ci-工作流)
- [发版步骤](#发版步骤)
- [发版前检查清单](#发版前检查清单)
- [已知缺口](#已知缺口)
- [编码约定](#编码约定)

## 仓库结构

```
ocg-manager/
├── crates/
│   ├── ocg-core/      Gateway、面板 HTTP API、SQLite、models、crypto、selector、cooldown、pricing
│   ├── ocg-cli/       无头 CLI 与 Gateway 入口
│   └── ocg-browser-worker/  Linux Chromium Sidecar 控制服务
├── browser/           Xvfb、Openbox、x11vnc、noVNC 启动脚本
├── src/               Vue 3 管理面板（TypeScript、naive-ui、Vite）
│   ├── App.vue        顶层外壳、登录页、侧边栏、顶栏
│   ├── api/tauri.ts   历史命名；HTTP 封装 /dashboard/api（不是 Tauri invoke）
│   ├── components/    LocaleSwitcher、PricingCatalog、StackedBarChart、…
│   ├── i18n/          i18n 注册表 + 各语言文案 + 单元测试
│   ├── styles/        主题 token、设计系统覆盖
│   └── views/         Dashboard、Accounts、Pricing、Applications、Logs、Settings（含单测）
├── src-tauri/         跨平台托盘应用、单实例、升级桥接、原生打包
├── docs/              USER / MAINTAINER / 防滥用（中英）、CONTRIBUTORS、索引
├── scripts/           free-dev-port、release、updater manifest、release notes、冒烟脚本、…
├── AGENTS.md          给 AI 编码助手的项目事实与约束
├── DESIGN.md          设计系统源（CI 中 lint）
├── .github/workflows/ quality.yml、release.yml、container.yml
├── Dockerfile         多阶段无头 Gateway 镜像
├── Dockerfile.browser Chromium/noVNC Sidecar 镜像
├── compose.yaml       支持源码构建与镜像拉取的 Compose 服务定义
└── compose.example.yaml  每个 Release 附带的只拉取镜像示例
```

`src/api/tauri.ts` 是历史命名，封装的是 HTTP `/dashboard/api`，**不是** Tauri
`invoke()`。Tauri commands 仍注册在 `src-tauri/src/commands/`，但不是当前 Vue
的主数据路径，主路径是 HTTP 面板。

## 环境前置条件

使用 Node.js 22（CI 基线）、pnpm 10.29.2 和 Rust 1.85 或更高版本。原生构建
依赖随 runner 调整，以 `.github/workflows/release.yml` 为准。当前 Linux
runner 安装 `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
libxdo-dev libssl-dev patchelf libfuse2 xvfb xauth xdg-utils dbus-x11`。

## 开发模式

先退出 release 托盘程序，释放单实例锁和 `9042` 端口，然后启动完整开发栈：

```bash
pnpm install
pnpm run dev
```

`pnpm run dev` 实际执行 `tauri dev`。Windows 上 `predev` 脚本
（`scripts/free-dev-port.mjs`）会检查 `127.0.0.1:30001` 并清理上一次残留的
Vite 进程。Tauri 启动 Vite，等 Gateway 就绪后打开
`http://127.0.0.1:30001/dashboard/`。

- 前端（Vue、CSS、TypeScript）改动走 Vite HMR。
- Rust 改动走 Tauri watcher + Cargo 增量编译，然后重启进程。Rust 代码 **不会**
  在进程内热替换，需要重启。

## 检查与构建

```bash
pnpm install
pnpm run test
pnpm run build:web
pnpm run design:lint
pnpm run build
```

- `pnpm run build:web` 是 **纯前端** 生产构建（`vue-tsc && vite build`），只
  验证面板时用它。
- `pnpm run test` 跑 `cargo test --workspace`、前端单元测试
  （`src/i18n.test.ts`、`src/views/*.test.ts`、`src/theme.test.ts`）和
  `vue-tsc --noEmit` 类型检查。
- `pnpm run design:lint` 用 `@google/design.md` lint `DESIGN.md`，让设计系统
  与代码保持一致。
- `pnpm run build` **只用于发版验证**。它会跑 `scripts/release.mjs`，为当前
  支持的原生平台构建 GUI 与 CLI，并在每个产物都通过校验后原子替换
  `release/`。失败时旧 `release/` 保留。Cargo 增量编译缓存不会被清空。

### Rust 检查

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
```

第一条命令只检查格式，不修改文件；需要格式化时运行 `cargo fmt --all`。

聚焦工作：

```bash
cargo test -p ocg-core
cargo test -p ocg-manager-cli
cargo test -p ocg-browser-worker
cargo test -p ocg-core gemini
cargo test -p ocg-core claude_desktop
```

测试真实账号流时，先在沙箱里跑 CLI：

```bash
ocg-manager-cli --data-dir /tmp/ocg-cli-test key add smoke sk-smoke
ocg-manager-cli --data-dir /tmp/ocg-cli-test key list
ocg-manager-cli --data-dir /tmp/ocg-cli-test serve --port 19042
```

### 前端检查

前端单元测试与代码放在同一目录（`src/i18n.test.ts`、
`src/views/accounts-usage.test.ts`、`src/views/dashboard-connection.test.ts`、
`src/views/logs.test.ts`、`src/views/pricing-view.test.ts`、
`src/views/settings-update.test.ts`、`src/theme.test.ts`），用 Node 实验性的
`--experimental-strip-types` 跑，不需要额外测试框架。最后再跑一次
`pnpm run build:web` 做冒烟。

应用教程由 `src/views/application-guides.ts` 的 16 个条目驱动；改动注册表时
同时检查教程数量、唯一 ID、协议端点、display/copy 脱敏差异，以及 Claude
Desktop 三个角色模型的持久化行为。

## 架构说明

### Gateway

- Gateway 在 `crates/ocg-core/src/gateway/`，使用 Axum + Tokio + reqwest，
  默认监听 `127.0.0.1:9042`。
- 处理器接受客户端的 `Authorization: Bearer <gateway-key>`、`x-api-key` 或
  `x-goog-api-key`，与配置里的 Gateway Key 比对。`forwarder.rs` 必须移除这些
  客户端凭据，再按实际 Chat/Messages 上游协议注入所选账号 Key；不要把 Gemini
  或 Anthropic 客户端凭据透传到 OpenCode-Go。
- 标准入口是 `/v1/chat/completions`、`/v1/responses`、`/v1/messages` 与
  `/v1/models`。Claude Desktop 使用 `/claude-desktop/v1/messages` 和
  `/claude-desktop/v1/models`；Gemini 同时接受 `/v1beta/models/{model}:*` 与
  `/v1/models/{model}:*`，其中 `generateContent`、`streamGenerateContent` 进入
  转换链，`countTokens`、`embedContent` 返回 `501`。
- `protocol.rs`、`protocol_stream.rs` 在 Chat Completions、Responses、
  Anthropic Messages 与客户端 Gemini generateContent 之间转换。Gemini 不能成为
  上游格式。已知模型用硬编码 `MODEL_PROTOCOLS`（`preferred` + `supported`）：
  客户端协议在 `supported` 内则透传，否则转到 `preferred`；Responses/Gemini
  未知模型直接 `400`，请求路径禁止试探协议。
  非空 `safetySettings` 必须 `400` 拒绝，不能静默丢弃安全策略；空数组可以接受。
  `topK`、`thinkingConfig` 这两个 Gemini 专属字段只作为跨协议兼容提示，不得在
  文档或测试中宣称与 Google Gemini 等价。
- Claude Desktop handler 在进入既有 Messages 准备流程前，把服务端公布的
  Sonnet、Opus、Haiku 别名改写为 `AppConfig.claude_desktop_models` 中的实际模
  型。模型配置由受保护的 `/dashboard/api/claude-desktop/models` 读写；常规
  settings 更新必须保留它。
- `selector.rs` 选下一个账号并跳过禁用、冷却、本次已失败的账号；`limit.rs`
  解析上游 429 中的重置时长；`pricing.rs` 从当前 OpenCode Go 价格快照计算
  token 对应的额度消耗，面板窗口额度也来自同一快照。
  `PricingModel.quota_multiplier` 是唯一实际参与结算的官方倍率；抓取的快照按
  `月额度 / Usage` 推导，受保护的倍率更新端点可以为临时活动保存用户覆盖值，并
  生成新的不可变 revision。
- 价格读写由受保护的 `GET /dashboard/api/pricing`、
  `PUT /dashboard/api/pricing/multipliers` 和
  `POST /dashboard/api/pricing/refresh` 提供。刷新发现官方倍率与当前值不同时，先返回
  不激活的差异预览；后续请求同时绑定当前 revision 与刚预览的官方 content hash，
  再选择保留当前值或采用官方值，官方候选变化时必须重新确认。抓取器
  仅允许 OpenCode Go HTTPS 主机和同主机重定向，总时限 20 秒、响应体上限 2 MiB；
  任何校验失败都不会激活不完整数据，`pricing_snapshots` 会保留最后成功 revision。
  MiniMax 长上下文、priority 和 high-speed 调整是本地策略，运行时不会访问供应商
  价格页。
- `forwarder.rs` 向 `handler.rs` 返回显式动作：只有能证明请求尚未发出的
  DNS/TCP/TLS 建连失败可以在同一账号重试一次；`401`/`403`/`429` 可以切换账号。
  `408`、`5xx`、建连后的失败、响应体超时和流式中断均不得重放，无法确认的结果
  记为 `outcome_unknown`。共享 reqwest client 只设置 30 秒建连超时；非流式请求
  使用 900 秒总时限，流式请求按 chunk 执行 300 秒空闲时限。

### 管理面板

- 面板由 Gateway 在 `/dashboard` 提供，数据走 `/dashboard/api`。Tauri 仍注册
  command handler，但不再是 Vue 的主调用路径。
- **回环监听时** 直接访问跳过登录。带标准反向代理转发头但没 Cookie 的请求仍
  需登录。**非回环监听** 走单管理员模型：密码以 Argon2 哈希存 SQLite，登录下
  发 HttpOnly 会话 Cookie。
- 设置页通过受保护的 `GET /dashboard/api/settings/check-update` 获取 GitHub
  Release 元数据。支持升级的已安装桌面运行时可继续下载、校验签名并安装；开发
  构建、CLI、Docker 只保留元数据/发布页路径。出站请求只在用户点击按钮时发起，
  不属于遥测。
- Docker 可用 `OCG_ADMIN_USERNAME` 与 `OCG_ADMIN_PASSWORD` 引导首个管理员；
  不提供时由首位注册者创建。
- 侧栏包含仪表盘、账号、价格表、应用、日志、设置。**应用** 视图维护 16 个教程：
  Claude Code、Claude Desktop、Codex、Gemini CLI、Pi、Kimi Code CLI、OpenCode、
  WorkBuddy、OpenClaw、Hermes、Cherry Studio、VS Code Copilot Chat、Cline、Roo
  Code、Continue、Chatbox。Claude Desktop 的复制动作还会保存三个角色模型；其他
  教程只生成客户端配置，不修改 Gateway 设置。**价格表** 视图通过受保护的定价
  API 读取并刷新当前 OpenCode Go 快照。

### 账号生命周期与浏览器运行时

- schema v16 给账号增加 `account_type`（`key | managed`）与 `setup_step`
  （`google_account → opencode_registration → payment → key_verification → ready`）。
  旧行迁移为 `key + ready`。托管草稿立即持久化为空 Key、`enabled=false`；选择器、
  启用接口和路由都必须同时要求 `ready` 与非空 Key。步骤名 `google_account` 在 UI
  上展示为「登录身份」，可跳过。
- `AppConfig::default()` 的 `opencode_invite_url` 带演示默认值
  （`DEFAULT_OPENCODE_INVITE_URL`）。规范化后只接受最长 2048 字符、无用户名密码的
  HTTPS URL，主机严格限定为 `opencode.ai` 或 `console.opencode.ai`。创建托管草稿
  时可编辑邀请链接；与设置不同时写回 SQLite。注册/支付/验证码仍由用户在浏览器中
  完成，Key 由用户复制回填；**不得** CDP 自动填表或代点支付。
- 托管状态允许 **向前一步** 或 **回退到任意更早的未完成步骤**（`can_transition_to`）；
  禁止跳步前进，禁止经 setup API 直接进入 `ready`。Key 实测返回 `2xx` 时进入
  `ready + enabled`；`429` 同样证明 Key 有效并写入冷却；`401`/`403`、网络错误或
  `5xx` 保持 `key_verification`。API、DTO 与日志绝不能返回明文 Key。
- 已完成的托管账号可通过 `POST /dashboard/api/accounts/{id}/usage/refresh` 从该
  账号 Chromium Profile 的 Cookie 读取 OpenCode 控制台 Go 页用量并校准本地基线
  （`console_usage.rs`）。实现须处理 Chrome 127+ Cookie 32 字节域哈希前缀、Windows
  Cookie 库共享读，以及 Solid 序列化 / SSR comment 包裹的百分比。Key 账号仍用手
  动校准；刷新失败必须显式报错。
- 受 Dashboard 鉴权保护的生命周期接口为 `POST /accounts/managed`、
  `PATCH /accounts/{id}/setup`、`POST /accounts/{id}/setup/verify-key` 与
  `POST /accounts/{id}/usage/refresh`；浏览器接口为 `GET /browser/capabilities`、
  `POST /accounts/{id}/browser`、`DELETE /accounts/{id}/browser-profile` 和
  `/browser/sessions/{token}/ws`（都位于 `/dashboard/api` 下）。浏览目标允许
  Google 注册/登录、GitHub 注册/登录、配置的邀请 URL 与 OpenCode 控制台
  （`https://opencode.ai/auth`）。worker 主机白名单含 `accounts.google.com`、
  `github.com`、`opencode.ai`、`console.opencode.ai`、`auth.opencode.ai`。远程
  会话令牌只在内存中保存，绑定管理员会话并检查 Origin，空闲 30 分钟或总计 4 小时
  失效。
- 桌面端由 Tauri 注册原生启动/停止 hook，但 Vue 仍通过 HTTP API 调用。Windows
  依次查 Edge、Chrome；macOS 查 Chrome、Edge、Chromium；Linux 从 `PATH` 查
  Chrome/Chromium/Edge。外部浏览器使用
  `browser-profiles/<account_id>`、`--no-first-run`、
  `--no-default-browser-check` 与新窗口，不得加入 CDP、automation、
  `--no-sandbox` 或关闭 Web 安全的参数。
- `crates/ocg-browser-worker` 每节点只保留一个 Chromium。切换账号先 SIGTERM 当前
  进程组并等待 Profile 写盘，超时才强制结束。Sidecar 以 UID/GID 10001、只读根
  文件系统、零 capability 运行；控制 token 由共享运行时卷随机生成。Chromium
  需要建立自身的 user/PID/network namespace 和 renderer seccomp 沙箱，因此 browser
  服务使用 `seccomp=unconfined` 且不能启用 `no-new-privileges`。Sidecar 仍不挂载
  SQLite，不发布宿主机端口。浏览器项目桥接网络不能设为 Docker `internal`，因为
  Chromium 需要访问 Google/OpenCode 的 HTTPS 出站网络。
- Profile 删除必须先停浏览器，校验账号 ID 防目录穿越，再把新旧 Profile 原子改名
  暂存；数据库操作成功后清理暂存目录，失败则恢复。重置完成账号不删除 Key；重置
  注册中账号还要回到 `google_account`。删除账号的 UI 确认必须写明 Cookie/Profile
  也会删除。

### 持久化

- `crates/ocg-core/src/db.rs` 定义 SQLite schema、迁移与查询；
  `crates/ocg-core/src/models.rs` 定义共享 serde 类型和 `AppConfig`；
  `crates/ocg-core/src/crypto.rs` 提供 Key 混淆与 `.encryption-key` 管理。
- `crates/ocg-core/src/state.rs` 是 `CoreStateInner`，由 Gateway、面板、CLI
  共享。
- `AppConfig` 使用 serde 默认值做向后兼容加载。1.3 之前没有
  `claude_desktop_models` 的配置会得到默认 Sonnet 目标 `minimax-m3`，并被规范
  写回 SQLite。模型更新由 `settings_update` 序列化；常规 settings 保存会保留专
  用的 Claude Desktop 映射。
- Docker 将 SQLite、Key 与 `.encryption-key` 放在 `ocg-data`，长期 Cookie 与
  浏览器状态放在 `ocg-browser-profiles`。两卷都是高敏感持久状态，必须在服务停止
  后成对备份；`ocg-browser-runtime` 只含运行时控制 token，不应加入备份。浏览器
  Profile 不由 OCG Manager 加密。

### 节点边界

每个节点由自己的面板独立管理；不提供跨节点同步，也不提供 Admin API。不要新
增。

## 升级与数据库迁移

GUI 或 CLI 启动时会原地执行 SQLite 迁移。升级前备份完整数据目录，包括数据库、
存在时的 `.encryption-key` 与 `browser-profiles/`；Docker 同时备份
`ocg-data` 和 `ocg-browser-profiles`。直接/手动升级时先停止进程，签名桌面升级器会自
行停止并重启。项目不保证降级兼容；如需回滚，恢复对应旧版本升级前的数据备份，
不要让旧二进制直接打开已迁移的数据库。

v1.4.1 既没有升级运行时，也没有内置签名校验公钥。Windows 的一次性过渡需要明
确指导用户：退出托盘程序，运行首个支持升级的 setup，在“升级方式”页选择第二
项 **不要卸载，直接安装（Install without uninstalling）**。第一项只是 Tauri
默认选中项，并非升级所必需；不要先卸载 v1.4.1。高级用户可选择执行等价命令：

```powershell
Start-Process -FilePath .\ocg-manager_<version>_windows-x64-setup.exe -ArgumentList '/UPDATE','/P','/R' -Wait
```

macOS/Linux 按各自常规方式直接替换一次。此后的桌面版可走设置页签名升级。CLI
与 Docker 仍手动升级。

## 发布产物

支持的发布矩阵刻意保持精简：

| Runner | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | NSIS 当前用户安装包 | x64 ZIP |
| macOS 11+ | Universal DMG（x64 + ARM64） | Universal tar.gz |
| Linux x64 | AppImage + deb | x64 tar.gz |

稳定的产物命名：

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
latest.json
SHA256SUMS
```

每个 CLI 压缩包都包含可执行文件、`dist/`、`LICENSE`。**不要** 只发 CLI 可执
行文件：`serve` 需要同级的 `dist/`。Windows 没有 portable GUI 安装包。

`linux/amd64` 容器单独发布为 `ghcr.io/klarkxy/opencode-go-mgr`。GitHub Release
包含七份常规平台 payload、额外的 macOS 升级压缩包、四份升级签名、只拉取镜像
的 Compose 示例、`latest.json` 与 `SHA256SUMS`（合计 15 个附件）。运行镜像内
的许可证位于 `/usr/share/licenses/ocg-manager/LICENSE`。

### scripts/release.mjs

`scripts/release.mjs` 负责所有繁重工作：

1. 校验 `package.json`、`src-tauri/tauri.conf.json`、workspace `Cargo.toml`、
   `src-tauri/Cargo.toml`，以及 `compose.example.yaml` 的三个带版本字段
   （标题、主镜像和浏览器镜像默认值）一致；如有 Git tag，与之比对。
2. 在创建暂存目录前解析升级签名模式；设置 `OCG_REQUIRE_UPDATER_ARTIFACTS=1`
   时，缺私钥或 `TAURI_UPDATER_PUBLIC_KEY` 都会在替换 `release/` 前失败；配置
   的公钥还必须匹配 `src-tauri/updater-public-key.sha256` 中已提交的 SHA-256
   连续性基线。
3. 配置签名密钥时，合并 `src-tauri/tauri.updater.conf.json` 和临时公钥配置，
   启用 Tauri 升级产物。`TAURI_SIGNING_PRIVATE_KEY` 可直接填写私钥内容或仓库外
   的安全路径，不另设 path 变量。没有签名密钥时保持普通本地构建，并明确提示该
   结果只适合冒烟，不是可发布的升级版本。
4. 拒绝不支持的 host/arch 组合（`process.platform`/`process.arch`）。
5. 用绝对 bundle 路径调用 `@tauri-apps/cli`：Windows 走 `nsis`，Linux 走
   `appimage,deb`。macOS 普通本地构建走
   `--target universal-apple-darwin --bundles dmg`；启用升级签名时走
   `--bundles app,dmg`，因为 Tauri 只有在构建 `app` target 时才会生成升级压缩
   包。
6. 每份 payload/签名在暂存前都使用实际 `TAURI_UPDATER_PUBLIC_KEY` 做密码学验
   证，再收集 NSIS、AppImage 签名与 macOS `.app.tar.gz`/签名；deb 不是 Tauri
   原生升级产物，因此显式执行 `tauri signer sign`。公私钥即使都非空但不匹配，
   也会 fail closed。
7. 构建 CLI 二进制，与 `dist/`、`LICENSE` 一起打成对应平台的压缩包；macOS 上
   用 `lipo` + `codesign -` 拼出 universal CLI。
8. 对暂存 `release/` 目录内的每份 payload 与签名写 `SHA256SUMS`。
9. 原子替换 `release/`。任意步骤失败，旧 `release/` 保留，暂存目录清理。

`scripts/release.mjs` **不会** 清空 Cargo 增量编译缓存——多次发布共用同一个
`target/`。

`pnpm run release:check` 校验版本、Compose 与已配置签名密钥，不构建原生安装
包。无密钥预检先覆盖未签名契约；生产 tag push 中，每台 runner 都会先用
repository signing secret 签一个临时 payload，并用已通过连续性检查的
`TAURI_UPDATER_PUBLIC_KEY` 验证，再开始昂贵的原生构建。

## CI 工作流

### quality.yml —— 可复用质量门

`.github/workflows/quality.yml` 在 PR 和 `main` push 上自动运行，`release.yml`
发版时只调用一次。Ubuntu job 完成格式检查、锁定依赖的 Rust/Node 测试、
TypeScript 检查、Vite 生产构建、Clippy、`DESIGN.md` lint 与 Compose 校验；另
一个有界的 Windows job 会编译并执行 Tauri library 测试，使 Windows 专属自动启
动实现也在发版前得到覆盖。兼容的运行共享 Node/pnpm 和 Rust 构建缓存；PR 只恢
复 Rust 缓存，不写回。

### release.yml —— 候选与 tag 发布

`.github/workflows/release.yml` 由 `workflow_dispatch` 和 `v*` tag 触发。

- 手动候选可选 Windows x64、macOS Universal、Linux x64 或全部平台，刻意只生成
  未签名冒烟产物；即使手动运行选择 tag 作为 ref，也不会获得生产签名权限。
- 只有 `v*` tag 的 `push` 事件才会强制走完整三平台矩阵并注入 repository signing
  secrets。对这个单维护者仓库，推送该 tag 就是明确的公开发布授权。
- 质量门与无密钥 Windows 预检并行：预检会解析抽出的安装器冒烟脚本、运行发布
  辅助测试并校验所有版本清单。

预检通过后，每个选中的原生 runner 恢复对应 Rust 缓存并安装依赖。工作流只有在
plan 根据事件确认这是真实 `v*` tag push 时才注入签名 secrets，随后验证公私钥和
已提交公钥指纹，再执行带签名构建；手动 job 得到空签名值，只执行普通未签名构
建。两条路径都会运行 CLI/GUI 冒烟并上传保留 7 天的 `release-<platform>`。通用
测试、类型和 lint 不再在三台 runner 上重复执行。

### 各 runner 的冒烟流程

- **Windows CLI**——校验 `SHA256SUMS`，解压 ZIP，对临时 data dir 跑
  `key add` / `key list` / `key disable` / `key enable` / `status` /
  `key remove`，启动 `serve --port=19042` 后等 dashboard HTML 中出现
  `id="app"`。
- **macOS / Linux CLI**——同样的 `key` 与 `serve` 流程；macOS 上额外用
  `lipo -archs` 校验 universal 二进制。
- **Windows GUI**——下载当前已发布安装包，静默安装并启动，写入数据哨兵并启用
  `auto_start`；不卸载旧版，直接用 `/UPDATE /P /R /ARGS --startup` 运行候选
  NSIS，确认旧 PID 退出、`/settings/update-status` 返回候选版本、哨兵与
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OCG Manager` 都保留。
  安装器进程有显式超时，并与 `/R` 拉起的常驻 GUI 分开等待，避免成功重启反而卡
  住 CI；卸载完成也有时间上限，并通过已安装文件消失等后置条件判断。随后继续自
  启关闭/恢复检查，静默卸载并确认用户数据仍在。PowerShell 实现在
  `scripts/smoke-windows-release.ps1`，不再内嵌在 YAML。手动触发且候选版本已经
  是 latest 时，可走仅安装候选版的路径。
- **macOS GUI**——挂载 DMG，`codesign --verify --deep --strict`，
  `lipo -archs` 校验 universal，`--startup` 启动后等 dashboard。
- **Linux GUI**——`dpkg-deb --info` / `dpkg-deb --contents` 校验 deb，`file`
  校验 AppImage；用 `dbus-run-session -- xvfb-run -a env
  APPIMAGE_EXTRACT_AND_RUN=1 WEBKIT_DISABLE_COMPOSITING_MODE=1` 启动后等
  dashboard。

### draft-release 与 verify-release

`v*` tag 触发时，下游 `draft-release` job 下载三个 runner 的 Actions
artifact，把平台 payload、签名与 `compose.example.yaml` 组装进 `release/`，
生成使用不可变 tag URL 和 bundle 感知平台键的 `latest.json`，再重写覆盖
manifest、签名和其余附件的 `SHA256SUMS`，最后创建或更新 **draft** GitHub
Release。`verify-release` 随后要求资产集合恰好为 15 个，重新推导
`latest.json`、重算全部 checksum、验证四份升级签名，并把每个下载文件与
GitHub Release 存储层报告的 digest 对比。draft job 会把数字 Release ID 传给下
游；验证和公开 job 都重新校验该 ID、tag 与 draft 状态，不使用无法显示 draft
Release 的 tag 查询端点。

`v1.5.8-beta.1` 这类 SemVer 预发布 tag 走同一条真实签名 tag 路径，并保持恰好
15 个不可变附件。升级 manifest 会在 payload 文件名和下载 URL 中保留完整预发布
后缀，Windows 安装包冒烟也接受同一个预发布 `CandidateVersion`。自动生成的说明
开头会显著标注“托管账号注册与隔离浏览器 Profile 均为 Beta，尚未充分测试”，并
列出尚未实测的 Google/OpenCode 真实注册与支付、noVNC 键盘/剪贴板、GHCR 首次
公开发布路径；同时说明 preview 还包含 Gateway、脱敏和发布链路改动，不能视为生
产可用。之后生成稳定版说明时会跳过同版本预发布 tag，以前一个稳定版为基线，避免
完整功能范围被 Beta tag 隐藏。

### publish-release —— 只公开已验证的 tag 构建

`v*` tag push 是单维护者的明确发版授权，因此 `verify-release` 成功后会自动运行
`publish-release`。发布 job 会再次比对当前资产/digest 集合指纹与已验证指纹；
验证后 draft 有任何变化都会拒绝发布。手动候选无法进入 draft、验证或公开发布
job。缺少签名密钥、冒烟失败或验证失败时，Release 都不会公开。

发布 job 还进入仓库级 `release-moving-channels` 串行队列；正式公开前会比较候
选版本和当前 GitHub latest，只允许严格更高的稳定 SemVer 推进 `latest`。延迟
完成的旧 run 仍可公开自己的不可变 Release，但不能把移动通道回滚。
预发布 tag 的 draft 与最终 Release 都设为 `prerelease=true`，固定
`make_latest=false`，且不会调用只适用于稳定版的 latest 比较；稳定 tag 的行为不
变。

### 升级签名密钥

生产升级密钥只在可信工作站生成一次，并写到仓库外的安全路径（不要把仓库内路
径传给此命令）：

```powershell
node node_modules/@tauri-apps/cli/tauri.js signer generate -w <仓库外安全路径>/ocg-updater.key
```

- 私钥内容与密码分别保存为 repository Actions secrets
  `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。发布工
  作流只会在事件派生的 plan 确认真实 `v*` tag push 时引用它们；手动候选得到空
  值并保持未签名。
- Repository secrets 不具备 Environment 隔离；如果以后增加有写权限的维护者，
  应在下一次发版前重新评估受保护签名 Environment 或 tag ruleset。
- 私钥和密码都至少保留两份独立存放的加密备份。它们一旦丢失，已经信任对应公
  钥的客户端就无法再走应用内升级，只能重新直接安装引导版本。
- 公钥可安全分享；本项目通过 repository Actions variable
  `TAURI_UPDATER_PUBLIC_KEY` 注入其内容，而不提交到仓库。GitHub 中保存的是生
  成后的密钥内容，不是本地文件路径。
- 升级签名证明 payload 由本项目发布，但不等同于操作系统代码签名。

### 密钥连续性与轮换

`src-tauri/updater-public-key.sha256` 是生产信任连续性的已提交锚点，正常 CI
没有绕过开关：repository variable 不匹配时，签名预检和 Release 验证都会 fail
closed。密钥轮换属于 break-glass 恢复，不是普通 secret 更新。必须先生成并备份
新密钥、为所有既有客户端准备直接安装引导，再在明确的安全审查变更中更新已提交
指纹；不能只改 variable 或只改指纹，旧安装版无法信任仅由新密钥签出的版本。

### container.yml —— 镜像流水线

GitHub Release 发布后会触发 `.github/workflows/container.yml`。该工作流检出
Release tag，同时构建并冒烟验证两个 `linux/amd64` 镜像：主服务
`ghcr.io/klarkxy/opencode-go-mgr` 与 Sidecar
`ghcr.io/klarkxy/opencode-go-mgr-browser`。主镜像冒烟检查 Dashboard、鉴权和许可
证；浏览器镜像在只读根文件系统、零 capability、Chromium 可用的 seccomp
配置、无宿主机端口下启动 Xvfb/noVNC，并通过受 token 保护的控制 API 真正拉起
普通 Chromium 与持久 Profile。

两个镜像都先按 digest 推送而不分配可变名称，再进入仓库级串行标签队列。
`X.Y.Z` 与 `sha-<12 位 commit>` 仅在不存在时创建；已存在时只有各自 digest 与
候选完全相同才接受，否则失败。稳定版 `X.Y` 和选择更新的 `latest` 按镜像对整体
决策：要么都收敛到候选版本，要么保留已经对齐的较新版本对；若通道仍会分裂，工作
流会失败而不是静默保留。两个镜像各自记录 SPDX SBOM、BuildKit SLSA provenance
与 GitHub 签名 provenance。`X.Y.Z` 和 `sha-*` 是不可变发布标签；`X.Y` 与
`latest` 是单调移动通道。浏览器镜像是 GHCR 包，不会增加 GitHub Release 附件；
原生发布仍必须恰好 15 个附件。

Package 可见性独立于关联仓库管理，工作流不能依赖 repository token 代为改成
公开；新的浏览器 package 在首次推送 digest 前也根本不存在。因此第一次创建该
package 的 `container.yml` 会先完成推送，再因 GitHub 默认的私有可见性停在匿名
拉取门禁。这是唯一允许的引导例外：在 GitHub Package 设置中把新浏览器 package
设为**公开**（并确认主 package 也是公开），然后对同一个 tag 手动重跑
`container.yml`。不可变标签只有 digest 完全相同时才允许重放，所以重跑只完成
原发布，不会替换产物。在重跑全绿前，不得把容器发布视为完成；之后每个 Release
都必须在第一次运行时直接通过匿名门禁。

标签发布后，门禁使用空 Docker 凭证目录匿名拉取两个精确版本标签；package 若仍
为私有或不可访问，`container.yml` 会失败，而不会伪装成可供公开 Compose 使用的
成功发布。

手动触发可回填已有 Release tag，且只有显式选择后才会更新 `latest`。工作流会
显式检出 `refs/tags/<tag>`，验证 HEAD 确实由该 tag 解析得到，并在发布任何镜像
前运行仓库版本预检。若重建内容与既有完整版本或 `sha-*` 标签的 digest 不同，
会失败而不是覆盖；只接受完全相同 digest 的重放。它的 GitHub 签名证书记录发起
dispatch 的 workflow ref，即使构建随后检出了指定 tag。因此不要把历史手动回填
描述成“由该 tag 触发的 provenance”；正常 `release.published` 使用 Release
tag 上下文。

发布后记录 digest，并同时核验 OCI index 与 GitHub attestation；验证时约束到
本仓库的 signer workflow：

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:X.Y.Z
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr-browser:X.Y.Z
docker buildx imagetools inspect --raw \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest>
docker buildx imagetools inspect --format '{{json .SBOM}}' \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> > sbom.json
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr-browser@sha256:<browser-digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
```

SBOM 与 provenance 是供应链元数据，不等于漏洞扫描。GitHub attestation 签名的
是 provenance statement；项目当前没有另加独立 Cosign 镜像签名。

当前 Windows 安装包未签名，macOS 用 ad-hoc 签名（`-`），没有 Developer ID
公证。推送 release tag 前必须复核原生候选冒烟和这些平台警告，因为 tag 工作流
成功后会自动公开。Windows / Linux ARM64、32 位 x86、RPM、Snap、应用商店包仍
不支持。签名的应用内升级只用于支持升级的已安装桌面版；v1.4.1、开发构建、CLI、
Docker 仍走直接/手动路径。

### CI 覆盖边界

PR 会自动运行平台无关的质量门，额外的 Windows job 覆盖 Windows 专属 Tauri 行
为的编译和单测；原生安装包/打包冒烟仍只在手动候选或 tag 流程运行。容器工作流
只覆盖 `linux/amd64`，并且只在 Release 发布后或手动触发时运行。

CI 不会操作真实桌面 UI，也不启动真实 Claude Desktop 或 Gemini CLI，不测试容
器 ARM64、备份恢复、数据库降级、迁移回滚、真实上游账号或真实 Gateway 请求。
Rust 测试覆盖 Gemini/Claude Desktop 路由、鉴权、别名改写、非流式转换和 SSE 事
件形状，但不能证明第三方客户端的新版本仍接受生成的配置。容器冒烟只检查 TCP
健康、Dashboard HTML、auth status、镜像内许可证，以及未登录 settings 返回
`401`。浏览器容器冒烟会启动真实 Chromium、确认 Profile 目录和无公开端口，但不
登录 Google/OpenCode、不操作 noVNC 键鼠/剪贴板，也不执行真实支付。Google 数据
中心 IP 风控、桌面浏览器发现、Cookie 跨重启保留和远程账号切换仍需手工验证。

## 发版步骤

1. 确定 `X.Y.Z`（或 `X.Y.Z-beta.N` 这类不可变 SemVer 预发布版本），同步修改
   `package.json`、`src-tauri/tauri.conf.json`、
   workspace `Cargo.toml`、`src-tauri/Cargo.toml`，以及
   `compose.example.yaml` 的标题、主镜像与浏览器镜像默认值。
2. 运行 `cargo check --workspace --all-targets` 刷新 `Cargo.lock`，再运行
   `pnpm install --frozen-lockfile`、`cargo fmt --all -- --check`、
   `pnpm run test`、`pnpm run design:lint`、`pnpm run release:check` 和
   `pnpm run build`。提交预期的 lockfile 改动，不要手工编辑 lockfile。
3. 与上一个公开 tag 比较，复核 diff 和当前平台的 `release/` payload，然后提交
   版本、lockfile、文档与 Release notes 改动。
4. 先合并已经审查的改动，再在 `main` 的最终 commit 上执行
   `git tag -a vX.Y.Z -m "OCG Manager vX.Y.Z"`（如为预发布，保留对应后缀）创建
   附注 tag 并推送。不要在之后
   还会 squash merge 的分支 commit 上提前打 tag。
5. 等待 `quality`、`preflight`、全部原生矩阵 job、`draft-release`、
   `verify-release` 和 `publish-release` 通过。确认公开的是同一个已验证 draft，
   再复核恰好 15 个附件、冒烟日志、平台警告，以及基于上一个 tag diff 编写的说
   明。
6. 等待 `container.yml` 通过，确认两个 GHCR package 已公开，分别核验版本与
   digest，再匿名拉取两个完整版本标签。

应把已发布的资产和 tag 视为不可变。已发布 payload 有误时发新的 patch 版本，
不要替换资产或移动 tag。

## 发版前检查清单

推送 `v*` tag **前** 跑完这些检查。CI 冒烟覆盖大部分；需要真实桌面的部分手
动验证。

- [ ] 可复用质量门中的 Ubuntu 与 Windows job 全绿；tag-only 签名
      `release:check` 通过；选中的每个 `pnpm run build` 与平台冒烟全绿。
- [ ] `git diff --check` 干净；相对上一个 tag 的 diff 只含预期范围；四份代码
      版本清单、`compose.example.yaml` 与 Cargo.lock 四个本地包条目一致。
- [ ] 每个 runner 的 `release/SHA256SUMS` 与目录内全部 payload 一致；
      `verify-release` 接受恰好 15 个附件、升级 manifest、四份签名、checksum
      和 GitHub 服务端 digest。
- [ ] 跑 `cargo test -p ocg-core gemini` 与
      `cargo test -p ocg-core claude_desktop`；用 Bearer、`x-api-key`、
      `x-goog-api-key` 分别请求 Gemini `generateContent` 与
      `streamGenerateContent`，覆盖 Chat 原生与 Messages 原生模型，确认错误
      envelope、usage envelope、HTTP 状态和 SSE 终止行为符合客户端协议。确认
      `countTokens` / `embedContent` 返回 `501`，未知 action 返回 `404`。
- [ ] 确认非空 Gemini `safetySettings` 返回 `400`，`null` 与 `[]` 仍接受。用
      代表性的 `cachedContent`、`fileData`、Google Search、`urlContext` 请求验
      证它们在任何上游计费前失败。对 `topK`、`thinkingConfig` 只验证兼容可用，
      不在冒烟中断言与 Gemini 原生等价的语义。
- [ ] 验证带鉴权的 Claude Desktop 模型发现与 Messages 别名改写。通过面板 API
      保存全部三个映射，用同一数据目录重启后确认映射仍在；非回环面板上确认无
      会话时映射 API 返回 `401`。
- [ ] 打开 **应用** 视图，确认 16 个教程完整可选；逐项抽查复制结果不含掩码
      Key，并实际启动 Claude Desktop 与 Gemini CLI 各完成一次文本和工具调用。
- [ ] 覆盖 schema v16 迁移、旧账号 `key + ready`、托管状态机（前进一格 / 回退更
      早步骤、禁止跳步）、Pending 路由隔离、邀请 URL 白名单与演示默认写回，以及
      Key 验证的 `2xx`/`429`/`401`/`403`/网络/`5xx` 分支；确认任何 DTO 和日志都
      没有明文 Key。
- [ ] 在 Windows 验证 Edge/Chrome 优先级，在 macOS/Linux 验证浏览器发现；用两个
      账号确认 Profile 隔离和重启后 Cookie 保留。确认重置会退出控制台但保留完成
      账号 Key，删除会同时清理新旧 Profile，旧 WebView Profile 不会被导入。
- [ ] 人工完成（可跳过）登录身份 → 邀请链接 → OpenCode 登录 → 支付前确认页 →
      Key 回填；真实支付只由测试者明确执行。控制台打开 `opencode.ai/auth`。旧
      Key 账号首次打开控制台后登录一次，再验证实际额度和邀请使用情况可回访。
      已完成托管账号验证 **刷新额度**（缺会话 / Cookie 锁定有明确错误）。分别
      覆盖桌面和 Docker Sidecar。
- [ ] Windows 上本地跑一次安装包，确认 SmartScreen 警告文案，打开面板、添加
      账号、发一条请求。
- [ ] macOS 上挂载 DMG，确认 **Open Anyway** 流程可用，打开面板、添加账号、
      发一条请求。
- [ ] Linux 上装 `.deb`、跑 AppImage，CI 上 Xvfb 跑通，本地 Wayland 或 X11 真
      实会话里再确认一遍。
- [ ] Windows 上验证 `auto_start` 开关能切换 `HKCU\...\Run\OCG Manager`，且卸
      载后清理。
- [ ] 确认 `scripts/release.mjs` 报告原子替换 `release/` 成功，旧 `release/`
      已清掉。
- [ ] 本地构建两个容器，并在隔离卷上确认 UID/GID `10001`、内置 `LICENSE`、只读/
      capability 加固、面板鉴权和备份恢复后的属主权限。用
      `docker compose --profile browser up -d` 验证单 Chromium、noVNC 键鼠/剪贴板、
      账号切换、Sidecar 重启、1 GiB shm、无公开端口和双卷备份恢复。
- [ ] 推送 tag 前复核计划使用的 GitHub Release 说明与未签名 / ad-hoc 警告；公
      开后确认同一份说明和精确的已验证资产集合已经发布。
- [ ] 发布后确认 `container.yml` 通过，并按预期 digest 匿名拉取主镜像与
      `ghcr.io/klarkxy/opencode-go-mgr-browser:<version>`，再分别验证 signer
      workflow、SBOM 与 SLSA provenance；GitHub Release 仍恰好 15 个附件。

## 已知缺口

- HTTP 面板与 Tauri command 层有重叠。Tauri commands 在 WebView 与启动行为迁移
  完成或主动下线前，**不要删除**。
- `auto_start` 受能力门控：只有 Windows release / 已安装的 Tauri 进程注入注册
  表同步钩子。开发构建、CLI、Docker、macOS、Linux 面板不暴露该开关。
- 生成的 Tauri schema 文件会让 diff 变吵；除非 Tauri 配置真的改了，否则不要动
  它们。
- 流式用量仅在上游发出 usage chunk 时精确，否则记为 `success_no_usage`。
- 旧 `profiles/<account_id>` WebView Profile 不会迁移到外部 Chromium；升级后首次
  需要重新登录。保留旧路径仅用于重置/删除时安全清理，不能尝试跨引擎直接复用。
- Responses 端点是无状态。`previous_response_id`、`conversation`、
  `store: true`、`background: true` 直接返回 `400`，不会静默忽略。这是有意为
  之，详见 `protocol.rs` 和用户指南。
- Gemini 是客户端兼容格式，不是新的上游协议。仅实现 generateContent 文本、内联
  图片、函数调用、单候选 TEXT/JSON Schema 与 SSE 转换；没有 Google Search、URL
  Context、Code Execution、cached content、Gemini embeddings 或服务端 token 计
  数。非空 `safetySettings` 明确拒绝；`topK`、`thinkingConfig` 这两个兼容提示
  不保证在 Chat/Messages 上游保持等价行为；其他非空 `generationConfig` 字段必
  须明确映射或返回 `400`，不得静默丢弃。
- Claude Desktop 只公布三个固定 Claude 别名，再映射到受支持的实际模型；它不代
  表 OCG Manager 提供了原生 Claude 4.6 模型或完整 Anthropic Models API。

## 编码约定

- **Ponytail 原则**：能删就删，能复用现有代码就复用。代码库偏向扁平调用点，
  不要为想象中的需求加抽象。
- **不要新增前端 Tauri `invoke()` 路径**。Vue 主数据路径是 HTTP
  `/dashboard/api`。只有在明确恢复桌面 WebView 能力时才重新引入。
- **不要削弱安全边界**。Gateway 鉴权、Key 混淆、URL 白名单、冷却写入、SSE 透
  传都不能为了简化拿掉。
- **不要重新引入远端同步**。每个节点由自己的面板管理。
- **`auto_start` 受能力门控**。只有 Windows release / 已安装的 Tauri 进程注入
  注册表同步钩子；开发构建、CLI、Docker、macOS、Linux 面板必须保持隐藏。
- **不要重新发明 `cargo test` 体验**。CLI 用 `parking_lot::Mutex`，不可重入。
  函数需要调用另一个持锁函数时，先 `drop` 掉外层 guard。
- **风格与周围一致**。修改某段代码时，新代码要像旧代码：注释密度、命名风格、
  惯用法保持一致。

---

[English maintainer guide](MAINTAINER.md) · [User guide](USER.md) ·
[用户指南](USER.zh-CN.md) · [文档索引](README.md) ·
[回到 README](../README.zh-CN.md)
