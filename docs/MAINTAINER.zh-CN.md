[English](MAINTAINER.md)

# 维护者指南

本指南面向修改代码、构建发布、调试 Gateway 以及验证桌面端安装包的开发者。
内容覆盖仓库结构、开发循环、测试与构建流水线、架构说明、发布矩阵、CI 冒烟
流程，以及明确不在支持范围内的能力。

## 目录

- [仓库结构](#仓库结构)
- [环境前置条件](#环境前置条件)
- [开发模式](#开发模式)
- [检查与构建](#检查与构建)
- [Rust 检查](#rust-检查)
- [前端检查](#前端检查)
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
│   ├── ocg-core/      Gateway、面板 HTTP API、SQLite、models、crypto、selector、cooldown、用量统计
│   └── ocg-cli/       无头 CLI 与 Gateway 入口
├── src/               Vue 3 管理面板（TypeScript、naive-ui、Vite）
│   ├── App.vue        顶层外壳、登录页、侧边栏、顶栏
│   ├── api/tauri.ts   历史命名；HTTP 封装 /dashboard/api（不是 Tauri invoke）
│   ├── components/    LocaleSwitcher、StackedBarChart
│   ├── i18n/          i18n 注册表 + 各语言文案 + 单元测试
│   ├── styles/        主题 token、设计系统覆盖
│   └── views/         Dashboard、Accounts、Applications、Logs、Settings（含单元测试）
├── src-tauri/         跨平台托盘应用、单实例行为、Tauri commands、原生打包
├── docs/              USER.md、MAINTAINER.md（中英双语）
├── scripts/           free-dev-port.mjs、release.mjs
├── DESIGN.md          设计系统源（CI 中 lint）
├── .github/workflows/ 跨平台发布工作流
├── Dockerfile         多阶段无头 Gateway 镜像
├── compose.yaml       支持源码构建与镜像拉取的 Compose 服务定义
└── compose.example.yaml  每个 Release 附带的只拉取镜像示例
```

`src/api/tauri.ts` 是历史命名，封装的是 HTTP `/dashboard/api`，**不是**
Tauri `invoke()`。Tauri commands 仍注册在 `src-tauri/src/commands/`，但
不是当前 Vue 的主数据路径，主路径是 HTTP 面板。

## 环境前置条件

使用 Node.js 22（CI 基线）、pnpm 10.29.2 和 Rust 1.85 或更高版本。原生
构建依赖随 runner 调整，以 `.github/workflows/release.yml` 为准。当前
Linux runner 安装 `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
librsvg2-dev libxdo-dev libssl-dev patchelf libfuse2 xvfb xauth xdg-utils
dbus-x11`。

## 开发模式

先退出 release 托盘程序，释放单实例锁和 `9042` 端口，然后启动完整开发栈：

```bash
pnpm install
pnpm run dev
```

`pnpm run dev` 实际执行 `tauri dev`。Windows 上 `predev` 脚本
（`scripts/free-dev-port.mjs`）会检查 `127.0.0.1:30001` 并清理上一次残留
的 Vite 进程。Tauri 启动 Vite，等 Gateway 就绪后打开
`http://127.0.0.1:30001/dashboard/`。

- 前端（Vue、CSS、TypeScript）改动走 Vite HMR。
- Rust 改动走 Tauri watcher + Cargo 增量编译，然后重启进程。Rust 代码
  **不会** 在进程内热替换，需要重启。

## 检查与构建

```bash
pnpm install
pnpm run test
pnpm run build:web
pnpm run design:lint
pnpm run build
```

- `pnpm run build:web` 是 **纯前端** 生产构建（`vue-tsc && vite build`），
  只验证面板时用它。
- `pnpm run test` 跑 `cargo test --workspace`、前端单元测试
  （`src/i18n.test.ts`、`src/views/*.test.ts`、`src/theme.test.ts`）和
  `vue-tsc --noEmit` 类型检查。
- `pnpm run design:lint` 用 `@google/design.md` lint `DESIGN.md`，让设
  计系统与代码保持一致。
- `pnpm run build` **只用于发版验证**。它会跑 `scripts/release.mjs`，
  为当前支持的原生平台构建 GUI 与 CLI，并在每个产物都通过校验后原子替
  换 `release/`。失败时旧 `release/` 保留。Cargo 增量编译缓存不会被清
  空。

## Rust 检查

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
cargo test -p ocg-core gemini
cargo test -p ocg-core claude_desktop
```

测试真实账号流时，先在沙箱里跑 CLI：

```bash
ocg-manager-cli --data-dir /tmp/ocg-cli-test key add smoke sk-smoke
ocg-manager-cli --data-dir /tmp/ocg-cli-test key list
ocg-manager-cli --data-dir /tmp/ocg-cli-test serve --port 19042
```

## 前端检查

前端单元测试与代码放在同一目录（`src/i18n.test.ts`、
`src/views/accounts-usage.test.ts`、`src/views/dashboard-connection.test.ts`、
`src/views/logs.test.ts`、`src/theme.test.ts`），用 Node 实验性的
`--experimental-strip-types` 跑，不需要额外测试框架。最后再跑一次
`pnpm run build:web` 做冒烟。应用教程由 `src/views/application-guides.ts` 的
13 个条目驱动；改动注册表时同时检查教程数量、唯一 ID、协议端点、display/copy
脱敏差异，以及 Claude Desktop 三个角色模型的持久化行为。

## 架构说明

### Gateway

- Gateway 在 `crates/ocg-core/src/gateway/`，使用 Axum + Tokio +
  reqwest，默认监听 `127.0.0.1:9042`。
- 处理器接受客户端的 `Authorization: Bearer <gateway-key>`、`x-api-key` 或
  `x-goog-api-key`，与配置里的 Gateway Key 比对。`forwarder.rs` 必须移除这
  些客户端凭据，再按实际 Chat/Messages 上游协议注入所选账号 Key；不要把
  Gemini 或 Anthropic 客户端凭据透传到 OpenCode‑Go。
- 标准入口是 `/v1/chat/completions`、`/v1/responses`、`/v1/messages` 与
  `/v1/models`。Claude Desktop 使用 `/claude-desktop/v1/messages` 和
  `/claude-desktop/v1/models`；Gemini 同时接受 `/v1beta/models/{model}:*`
  与 `/v1/models/{model}:*`，其中 `generateContent`、`streamGenerateContent`
  进入转换链，`countTokens`、`embedContent` 返回 `501`。
- `protocol.rs`、`protocol_stream.rs` 在 Chat Completions、Responses、
  Anthropic Messages 与客户端 Gemini generateContent 之间转换。Gemini 不能
  成为上游格式，只能路由到已知模型的 Chat/Messages 原生协议；未知模型直接
  `400`。非空 `safetySettings` 必须 `400` 拒绝，不能静默丢弃安全策略；空数组
  可以接受。`topK`、`thinkingConfig` 这两个 Gemini 专属字段只作为跨协议兼容提示，
  不得在文档或测试中宣称与 Google Gemini 等价。
- Claude Desktop handler 在进入既有 Messages 准备流程前，把服务端公布的
  Sonnet、Opus、Haiku 别名改写为 `AppConfig.claude_desktop_models` 中的实际
  模型。模型配置由受保护的 `/dashboard/api/claude-desktop/models` 读写；常规
  settings 更新必须保留它。
- `selector.rs` 选下一个账号并跳过禁用、冷却、本次已失败的账号；`limit.rs`
  解析上游 429 中的重置时长；`cost.rs` 把 token 数聚合成 5 小时、本周、本月
  窗口。

### 管理面板

- 面板由 Gateway 在 `/dashboard` 提供，数据走 `/dashboard/api`。Tauri
  仍注册 command handler，但不再是 Vue 的主调用路径。
- **回环监听时** 直接访问跳过登录。带标准反向代理转发头但没 Cookie
  的请求仍需登录。**非回环监听** 走单管理员模型：密码以 Argon2 哈希
  存 SQLite，登录下发 HttpOnly 会话 Cookie。
- 设置页通过受保护的 `GET /dashboard/api/settings/check-update` 获取 GitHub
  Release 元数据。支持升级的已安装桌面运行时可继续下载、校验签名并安装；开发
  构建、CLI、Docker 只保留元数据/发布页路径。出站请求只在用户点击按钮时发
  起，不属于遥测。
- Docker 可用 `OCG_ADMIN_USERNAME` 与 `OCG_ADMIN_PASSWORD` 引导首个管
  理员；不提供时由首位注册者创建。
- **应用** 视图维护 13 个教程：Claude Code、Claude Desktop、Codex、Gemini
  CLI、OpenCode、OpenClaw、Hermes、Cherry Studio、VS Code Copilot Chat、
  Cline、Roo Code、Continue、Chatbox。Claude Desktop 的复制动作还会保存三个
  角色模型；其他教程只生成客户端配置，不修改 Gateway 设置。

### 持久化

- `crates/ocg-core/src/db.rs` 定义 SQLite schema、迁移与查询；
  `crates/ocg-core/src/models.rs` 定义共享 serde 类型和 `AppConfig`；
  `crates/ocg-core/src/crypto.rs` 提供 Key 混淆与 `.encryption-key` 管
  理。
- `crates/ocg-core/src/state.rs` 是 `CoreStateInner`，由 Gateway、面
  板、CLI 共享。

### 节点边界

每个节点由自己的面板独立管理；不提供跨节点同步，也不提供 Admin API。
不要新增。

## 升级与数据库迁移

GUI 或 CLI 启动时会原地执行 SQLite 迁移。升级前备份完整数据目录，包括数据库
与存在时的 `.encryption-key`；直接/手动升级时先停止进程，签名桌面升级器会自行
停止并重启。项目不保证降级兼容；
如需回滚，恢复对应旧版本升级前的数据备份，不要让旧二进制直接打开已迁移
的数据库。

v1.4.1 既没有升级运行时，也没有内置签名校验公钥。Windows 的一次性过渡需要明
确指导用户：退出托盘程序，运行首个支持升级的 setup，在“升级方式”页选择第二
项**不要卸载，直接安装（Install without uninstalling）**。第一项只是 Tauri 默
认选中项，并非升级所必需；不要先卸载 v1.4.1。高级用户可选择执行等价命令：

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

每个 CLI 压缩包都包含可执行文件、`dist/`、`LICENSE`。**不要**只发 CLI
可执行文件：`serve` 需要同级的 `dist/`。Windows 没有 portable GUI 安装
包。

`linux/amd64` 容器单独发布为 `ghcr.io/klarkxy/opencode-go-mgr`。GitHub Release
包含七份常规平台 payload、额外的 macOS 升级压缩包、四份升级签名、只拉取镜像
的 Compose 示例、`latest.json` 与 `SHA256SUMS`（合计 15 个附件）。运行镜像内
的许可证位于 `/usr/share/licenses/ocg-manager/LICENSE`。

`scripts/release.mjs` 负责所有繁重工作：

1. 校验 `package.json`、`src-tauri/tauri.conf.json`、workspace
   `Cargo.toml`、`src-tauri/Cargo.toml` 的版本一致；如有 Git tag，与之
   比对。
2. 在创建暂存目录前解析升级签名模式；设置
   `OCG_REQUIRE_UPDATER_ARTIFACTS=1` 时，缺私钥或
   `TAURI_UPDATER_PUBLIC_KEY` 都会在替换 `release/` 前失败。
3. 配置签名密钥时，合并 `src-tauri/tauri.updater.conf.json` 和临时公钥配
   置，启用 Tauri 升级产物。`TAURI_SIGNING_PRIVATE_KEY` 可直接填写私钥内容或仓
   库外的安全路径，不另设 path 变量。没有签名密钥时保持普通本地构建，并明确提
   示该结果只适合冒烟，不是可发布的升级版本。
4. 拒绝不支持的 host/arch 组合（`process.platform`/`process.arch`）。
5. 用绝对 bundle 路径调用 `@tauri-apps/cli`：Windows 走 `nsis`，
   Linux 走 `appimage,deb`，macOS 走 `--target universal-apple-darwin
   --bundles dmg`。
6. 每份 payload/签名在暂存前都使用实际 `TAURI_UPDATER_PUBLIC_KEY` 做密码学
   验证，再收集 NSIS、AppImage 签名与 macOS `.app.tar.gz`/签名；deb 不是
   Tauri 原生升级产物，因此显式执行 `tauri signer sign`。公私钥即使都非空但
   不匹配，也会 fail closed。
7. 构建 CLI 二进制，与 `dist/`、`LICENSE` 一起打成对应平台的压缩包；
   macOS 上用 `lipo` + `codesign -` 拼出 universal CLI。
8. 对暂存 `release/` 目录内的每份 payload 与签名写 `SHA256SUMS`。
9. 原子替换 `release/`。任意步骤失败，旧 `release/` 保留，暂存目录清
   理。

`scripts/release.mjs` **不会** 清空 Cargo 增量编译缓存——多次发布共用同
一个 `target/`。

## CI 工作流

`.github/workflows/release.yml` 由 `workflow_dispatch` 和 `v*` tag 触发，
3 runner 矩阵：Windows x64、macOS Universal、Linux x64（Ubuntu 22.04）。
每个 runner 都会：

1. macOS 上安装对应 Rust target；Linux 上安装
   `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
   libxdo-dev libssl-dev patchelf libfuse2 xvfb xauth xdg-utils
   dbus-x11`。
2. 跑 `pnpm install --frozen-lockfile`、`pnpm run build:web`、
   `pnpm run test`、`pnpm run design:lint`、`pnpm run build`。构建从 GitHub
   Actions secrets 注入 `TAURI_SIGNING_PRIVATE_KEY`、
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，从 repository Actions variable 注入
   `TAURI_UPDATER_PUBLIC_KEY`，并设置 `OCG_REQUIRE_UPDATER_ARTIFACTS=1`。
3. 把每个 runner 的 `release/` 目录上传为 `release-<platform>` Actions
   artifact。

每个 runner 还会对新构建的产物做冒烟：

- **Windows CLI**——校验 `SHA256SUMS`，解压 ZIP，对临时 data dir 跑
  `key add` / `key list` / `key disable` / `key enable` / `status` /
  `key remove`，启动 `serve --port=19042` 后等 dashboard HTML 中出现
  `id="app"`。
- **macOS / Linux CLI**——同样的 `key` 与 `serve` 流程；macOS 上额外用
  `lipo -archs` 校验 universal 二进制。
- **Windows GUI**——下载当前已发布安装包，静默安装并启动，写入数据哨兵并启
  用 `auto_start`；不卸载旧版，直接用 `/UPDATE /P /R /ARGS --startup` 运行候
  选 NSIS，确认旧 PID 退出、`/settings/update-status` 返回候选版本、哨兵与
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OCG Manager` 都保
  留。随后继续原有的自启关闭/恢复检查，静默卸载并确认用户数据仍在。手动触发且
  候选版本已经是 latest 时，可走仅安装候选版的路径。
- **macOS GUI**——挂载 DMG，`codesign --verify --deep --strict`，
  `lipo -archs` 校验 universal，`--startup` 启动后等 dashboard。
- **Linux GUI**——`dpkg-deb --info` / `dpkg-deb --contents` 校验 deb，
  `file` 校验 AppImage；用
  `dbus-run-session -- xvfb-run -a env APPIMAGE_EXTRACT_AND_RUN=1
  WEBKIT_DISABLE_COMPOSITING_MODE=1` 启动后等 dashboard。

`v*` tag 触发时，下游 `draft-release` job 下载三个 runner 的 Actions
artifact，把平台 payload、签名与 `compose.example.yaml` 组装进 `release/`，生
成使用不可变 tag URL 和 bundle 感知平台键的 `latest.json`，再重写覆盖 manifest、
签名和其余附件的 `SHA256SUMS`，最后创建或更新 **draft** GitHub
Release——**不会**自动发布。
人工复核 draft 和原生冒烟结果后，再去 GitHub 把 release 转为 published，
或执行 `gh release edit vX.Y.Z --draft=false`。

生产升级密钥只在可信工作站生成一次，并写到仓库外的安全路径（不要把仓库内路
径传给此命令）：

```powershell
node node_modules/@tauri-apps/cli/tauri.js signer generate -w <仓库外安全路径>/ocg-updater.key
```

私钥内容与密码分别保存到上述两个 GitHub Actions secrets；私钥和密码都至少保
留两份独立存放的加密备份。它们一旦丢失，已经信任对应公钥的客户端就无法再走
应用内升级，只能重新直接安装引导版本。公钥可安全分享；本项目通过 repository
Actions variable `TAURI_UPDATER_PUBLIC_KEY` 注入其内容，而不提交到仓库。升级签
名证明 payload 由本项目发布，但不等同于操作系统代码签名。GitHub 中保存的是
生成后的密钥内容，不是本地文件路径。

GitHub Release 发布后会触发 `.github/workflows/container.yml`。该工作流检出
Release tag，构建并冒烟验证加固后的 `linux/amd64` 容器，把 `X.Y.Z`、`X.Y`、
稳定版 `latest` 与 `sha-<12 位 commit>` 标签推送到
`ghcr.io/klarkxy/opencode-go-mgr`，并记录 SPDX SBOM、BuildKit SLSA provenance
与 GitHub 签名 provenance。`X.Y.Z` 与 `sha-*` 是发布专用标签，按策略不得移动；
`X.Y` 与 `latest` 是滚动通道。技术上只有 manifest digest 真正不可变。

手动触发可回填已有 Release tag，且只有显式选择后才会更新 `latest`。它的 GitHub
签名证书记录发起 dispatch 的 workflow ref，即使构建随后检出了指定 tag。因此不要
把历史手动回填描述成“由该 tag 触发的 provenance”；正常 `release.published`
使用 Release tag 上下文。

发布后记录 digest，并同时核验 OCI index 与 GitHub attestation；验证时约束到本
仓库的 signer workflow：

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:X.Y.Z
docker buildx imagetools inspect --raw \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest>
docker buildx imagetools inspect --format '{{json .SBOM}}' \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> > sbom.json
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
```

SBOM 与 provenance 是供应链元数据，不等于漏洞扫描。GitHub attestation 签名的是
provenance statement；项目当前没有另加独立 Cosign 镜像签名。

当前 Windows 安装包未签名，macOS 用 ad‑hoc 签名（`-`），没有 Developer
ID 公证。原生冒烟与平台警告复核完成前，release 保持 draft。Windows /
Linux ARM64、32 位 x86、RPM、Snap、应用商店包仍不支持。签名的应用内升级只
用于支持升级的已安装桌面版；v1.4.1、开发构建、CLI、Docker 仍走直接/手动路
径。

### CI 覆盖边界

仓库没有 `pull_request` workflow，因此 PR 不会自动运行这些检查。容器工作流
只覆盖 `linux/amd64`，并且只在 Release 发布后或手动触发时运行。CI 不会操作
真实桌面 UI，也不启动真实 Claude Desktop 或 Gemini CLI，不测试容器 ARM64、
备份恢复、数据库降级、迁移回滚、真实上游账号或真实 Gateway 请求。Rust 测试
覆盖 Gemini/Claude Desktop 路由、鉴权、别名改写、非流式转换和 SSE 事件形状，
但不能证明第三方客户端的新版本仍接受生成的配置。容器冒烟只检查 TCP 健康、
Dashboard HTML、auth status、镜像内许可证，以及未登录 settings 返回 `401`。
改动未覆盖路径时需手工验证。

## 发版步骤

1. 确定 `X.Y.Z`，同步修改 `package.json`、
   `src-tauri/tauri.conf.json`、workspace `Cargo.toml`、
   `src-tauri/Cargo.toml`。
2. 运行 `cargo check --workspace --all-targets` 刷新 `Cargo.lock`，再运行
   `pnpm install --frozen-lockfile`、`cargo fmt --all -- --check`、
   `pnpm run test`、`pnpm run design:lint`、`pnpm run build`。提交预期的
   lockfile 改动，不要手工编辑 lockfile。
3. 与上一个公开 tag 比较，复核 diff 和当前平台的 `release/` payload，然后
   提交版本、lockfile、文档与 Release notes 改动。
4. 先合并已经审查的改动，再在 `main` 的最终 commit 上执行
   `git tag -a vX.Y.Z -m "OCG Manager vX.Y.Z"` 创建附注 tag 并推送。不要在
   之后还会 squash merge 的分支 commit 上提前打 tag。
5. 等待 `release.yml` 的全部矩阵 job 与 `draft-release` 通过，复核 draft
   中的七份平台 payload、`compose.example.yaml`、`SHA256SUMS`、冒烟日志、
   平台警告，以及基于上一个 tag diff 编写的说明。
6. 在 GitHub 发布 draft，或执行 `gh release edit vX.Y.Z --draft=false`，
   再核验公开 release。
7. 等待 `container.yml` 通过，确认 GHCR package 已公开，核验版本与 digest，
   再匿名拉取完整版本标签。

应把已发布的资产和 tag 视为不可变。已发布 payload 有误时发新的 patch 版本，不要
替换资产或移动 tag。

## 发版前检查清单

推送 `v*` tag **前** 跑完这些检查。CI 冒烟覆盖大部分；需要真实桌面的部
分手动验证。

- [ ] 三台 runner 上 `pnpm run test`、`pnpm run design:lint`、
      `pnpm run build` 全绿。
- [ ] `git diff --check` 干净；相对上一个 tag 的 diff 只含预期范围；四份
      版本清单与 Cargo.lock 三个本地包条目一致。
- [ ] 每个 runner 的 `release/SHA256SUMS` 与目录内全部 payload 一致；聚合
      release 的校验文件与七份平台 payload 加 `compose.example.yaml` 一致。
- [ ] Windows 上本地跑一次安装包，确认 SmartScreen 警告文案，打开
      面板、添加账号、发一条请求。
- [ ] macOS 上挂载 DMG，确认 **Open Anyway** 流程可用，打开面板、添
      加账号、发一条请求。
- [ ] Linux 上装 `.deb`、跑 AppImage，CI 上 Xvfb 跑通，本地 Wayland
      或 X11 真实会话里再确认一遍。
- [ ] Windows 上验证 `auto_start` 开关能切换
      `HKCU\...\Run\OCG Manager`，且卸载后清理。
- [ ] 确认 `scripts/release.mjs` 报告原子替换 `release/` 成功，旧
      `release/` 已清掉。
- [ ] 本地构建容器，并在隔离卷上确认 UID/GID `10001`、内置 `LICENSE`、
      只读/capability 加固、面板鉴权和备份恢复后的属主权限。
- [ ] 跑 `cargo test -p ocg-core gemini` 与
      `cargo test -p ocg-core claude_desktop`；用 Bearer、`x-api-key`、
      `x-goog-api-key` 分别请求对应入口，确认错误 envelope、HTTP 状态和 SSE
      终止行为符合客户端协议。
- [ ] 用隔离数据目录启动 Gateway，确认 Claude Desktop 三个别名都改写到保存
      的实际模型；Gemini 非空 `safetySettings` 返回 `400`，空数组可通过准备
      阶段，`countTokens` / `embedContent` 返回 `501`，未知 action 返回 `404`。
- [ ] 打开 **应用** 视图，确认 13 个教程完整可选；逐项抽查复制结果不含掩码
      Key，并实际启动 Claude Desktop 与 Gemini CLI 各完成一次文本和工具调用。
      对 `topK`、`thinkingConfig` 只验证兼容可用，不宣称跨协议语义等价。
- [ ] 复核 draft GitHub Release 说明与未签名 / ad‑hoc 警告，再把
      `--draft=false` 翻过来。
- [ ] 发布后确认 `container.yml` 通过，并按预期 digest 匿名拉取
      `ghcr.io/klarkxy/opencode-go-mgr:<version>`，再验证 signer workflow、
      SBOM 与 SLSA provenance。

## 已知缺口

- HTTP 面板与 Tauri command 层有重叠。Tauri commands 在 WebView 与启
  动行为迁移完成或主动下线前，**不要删除**。
- `auto_start` 受能力门控：只有 Windows release / 已安装的 Tauri 进程
  注入注册表同步钩子。开发构建、CLI、Docker、macOS、Linux 面板不暴露
  该开关。
- 生成的 Tauri schema 文件会让 diff 变吵；除非 Tauri 配置真的改了，
  否则不要动它们。
- 流式用量仅在上游发出 usage chunk 时精确，否则记为 `success_no_usage`。
- HTTP 面板没有暴露旧的隔离 WebView 浏览器 command；Tauri command 层
  里仍保留。
- Responses 端点是无状态。`previous_response_id`、`conversation`、
  `store: true`、`background: true` 直接返回 `400`，不会静默忽略。这
  是有意为之，详见 `protocol.rs` 和用户指南。
- Gemini 是客户端兼容格式，不是新的上游协议。仅实现 generateContent 文本、
  内联图片、函数调用、单候选 TEXT/JSON Schema 与 SSE 转换；没有 Google Search、
  URL Context、Code Execution、cached content、Gemini embeddings 或服务端
  token 计数。非空 `safetySettings` 明确拒绝；`topK`、`thinkingConfig` 这两个兼容
  提示不保证在 Chat/Messages 上游保持等价行为；其他非空 `generationConfig`
  字段必须明确映射或返回 `400`，不得静默丢弃。
- Claude Desktop 只公布三个固定 Claude 别名，再映射到受支持的实际模型；它不
  代表 OCG Manager 提供了原生 Claude 4.6 模型或完整 Anthropic Models API。

## 编码约定

- **Ponytail 原则**：能删就删，能复用现有代码就复用。代码库偏向扁平
  调用点，不要为想象中的需求加抽象。
- **不要新增前端 Tauri `invoke()` 路径**。Vue 主数据路径是 HTTP
  `/dashboard/api`。只有在明确恢复桌面 WebView 能力时才重新引入。
- **不要削弱安全边界**。Gateway 鉴权、Key 混淆、URL 白名单、冷却写入、
  SSE 透传都不能为了简化拿掉。
- **不要重新引入远端同步**。每个节点由自己的面板管理。
- **`auto_start` 受能力门控**。只有 Windows release / 已安装的 Tauri
  进程注入注册表同步钩子；开发构建、CLI、Docker、macOS、Linux 面板必
  须保持隐藏。
- **不要重新发明 `cargo test` 体验**。CLI 用 `parking_lot::Mutex`，不可
  重入。函数需要调用另一个持锁函数时，先 `drop` 掉外层 guard。
- **风格与周围一致**。修改某段代码时，新代码要像旧代码：注释密度、命
  名风格、惯用法保持一致。

---

[English maintainer guide](MAINTAINER.md) · [中文维护者指南](MAINTAINER.zh-CN.md) ·
[User guide](USER.md) · [用户指南](USER.zh-CN.md) · [回到 README](../README.zh-CN.md)
