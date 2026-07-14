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
└── compose.yaml       Compose 服务定义
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
`pnpm run build:web` 做冒烟。

## 架构说明

### Gateway

- Gateway 在 `crates/ocg-core/src/gateway/`，使用 Axum + Tokio +
  reqwest，默认监听 `127.0.0.1:9042`。
- 处理器解析客户端的 `Authorization: Bearer <gateway-key>`，与配置里的
  Key 比对，选一个已启用账号，把鉴权头改写成上游账号 Key，再把日志、用
  量、冷却、错误全部写回 SQLite。
- `protocol.rs`、`protocol_stream.rs` 在 Chat Completions、Responses、
  Anthropic Messages 之间转换；`selector.rs` 选下一个账号并跳过禁用、
  冷却、本次已失败的账号；`limit.rs` 解析上游 429 中的重置时长；
  `cost.rs` 把 token 数聚合成 5 小时、本周、本月窗口。

### 管理面板

- 面板由 Gateway 在 `/dashboard` 提供，数据走 `/dashboard/api`。Tauri
  仍注册 command handler，但不再是 Vue 的主调用路径。
- **回环监听时** 直接访问跳过登录。带标准反向代理转发头但没 Cookie
  的请求仍需登录。**非回环监听** 走单管理员模型：密码以 Argon2 哈希
  存 SQLite，登录下发 HttpOnly 会话 Cookie。
- 设置页通过受保护的 `GET /dashboard/api/settings/check-update` 查询 GitHub
  最新 Release。响应包含当前版本、最新版本和发布页 URL；Gateway 不会下载
  或安装发布产物。该出站请求只在用户点击检查按钮时发起，不属于遥测。
- Docker 可用 `OCG_ADMIN_USERNAME` 与 `OCG_ADMIN_PASSWORD` 引导首个管
  理员；不提供时由首位注册者创建。

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

GUI 或 CLI 启动时会原地执行 SQLite 迁移。升级前先停止进程，再备份完整
数据目录，包括数据库与存在时的 `.encryption-key`。项目不保证降级兼容；
如需回滚，恢复对应旧版本升级前的数据备份，不要让旧二进制直接打开已迁移
的数据库。

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
ocg-manager-cli_<version>_windows-x64.zip
ocg-manager_<version>_macos-universal.dmg
ocg-manager-cli_<version>_macos-universal.tar.gz
ocg-manager_<version>_linux-x64.AppImage
ocg-manager_<version>_linux-x64.deb
ocg-manager-cli_<version>_linux-x64.tar.gz
SHA256SUMS
```

每个 CLI 压缩包都包含可执行文件、`dist/`、`LICENSE`。**不要**只发 CLI
可执行文件：`serve` 需要同级的 `dist/`。Windows 没有 portable GUI 安装
包。

`linux/amd64` 容器单独发布为 `ghcr.io/klarkxy/opencode-go-mgr`，不计入
GitHub Release 附带的七份平台文件。

`scripts/release.mjs` 负责所有繁重工作：

1. 校验 `package.json`、`src-tauri/tauri.conf.json`、workspace
   `Cargo.toml`、`src-tauri/Cargo.toml` 的版本一致；如有 Git tag，与之
   比对。
2. 拒绝不支持的 host/arch 组合（`process.platform`/`process.arch`）。
3. 用绝对 bundle 路径调用 `@tauri-apps/cli`：Windows 走 `nsis`，
   Linux 走 `appimage,deb`，macOS 走 `--target universal-apple-darwin
   --bundles dmg`。
4. 构建 CLI 二进制，与 `dist/`、`LICENSE` 一起打成对应平台的压缩包；
   macOS 上用 `lipo` + `codesign -` 拼出 universal CLI。
5. 对暂存 `release/` 目录内的每份 payload 写 `SHA256SUMS`。
6. 原子替换 `release/`。任意步骤失败，旧 `release/` 保留，暂存目录清
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
   `pnpm run test`、`pnpm run design:lint`、`pnpm run build`。
3. 把每个 runner 的 `release/` 目录上传为 `release-<platform>` Actions
   artifact。

每个 runner 还会对新构建的产物做冒烟：

- **Windows CLI**——校验 `SHA256SUMS`，解压 ZIP，对临时 data dir 跑
  `key add` / `key list` / `key disable` / `key enable` / `status` /
  `key remove`，启动 `serve --port=19042` 后等 dashboard HTML 中出现
  `id="app"`。
- **macOS / Linux CLI**——同样的 `key` 与 `serve` 流程；macOS 上额外用
  `lipo -archs` 校验 universal 二进制。
- **Windows GUI**——静默 NSIS 安装到临时目录，`--startup` 启动，等
  `127.0.0.1:9042` 出现 dashboard；把 `auto_start` 打开，校验
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OCG Manager` 的
  值；关掉，确认清理；静默卸载，确认用户数据目录保留。
- **macOS GUI**——挂载 DMG，`codesign --verify --deep --strict`，
  `lipo -archs` 校验 universal，`--startup` 启动后等 dashboard。
- **Linux GUI**——`dpkg-deb --info` / `dpkg-deb --contents` 校验 deb，
  `file` 校验 AppImage；用
  `dbus-run-session -- xvfb-run -a env APPIMAGE_EXTRACT_AND_RUN=1
  WEBKIT_DISABLE_COMPOSITING_MODE=1` 启动后等 dashboard。

`v*` tag 触发时，下游 `draft-release` job 下载三个 runner 的 Actions
artifact，把七份平台 payload 组装进 `release/`，重写覆盖全部七份 payload
的 `SHA256SUMS`，再创建或更新 **draft** GitHub Release——**不会**自动发布。
人工复核 draft 和原生冒烟结果后，再去 GitHub 把 release 转为 published，
或执行 `gh release edit vX.Y.Z --draft=false`。

GitHub Release 发布后会触发 `.github/workflows/container.yml`。该工作流检出
Release tag，构建并冒烟验证加固后的 `linux/amd64` 容器，把完整版本、次版本、
`latest` 与 commit SHA 标签推送到 `ghcr.io/klarkxy/opencode-go-mgr`，并记录
SBOM 与 provenance attestation。手动触发可回填已有 Release tag，但只有显式
选择后才会更新 `latest`。

当前 Windows 安装包未签名，macOS 用 ad‑hoc 签名（`-`），没有 Developer
ID 公证。原生冒烟与平台警告复核完成前，release 保持 draft。Windows /
Linux ARM64、32 位 x86、RPM、Snap、应用商店包以及自动下载/安装更新仍不
支持。设置页可手动检查 GitHub 最新的已发布 Release。

### CI 覆盖边界

仓库没有 `pull_request` workflow，因此 PR 不会自动运行这些检查。容器工作流
只覆盖 `linux/amd64`，并且只在 Release 发布后或手动触发时运行。CI 不会操作
真实桌面 UI，也不测试容器 ARM64、备份恢复、数据库降级或迁移回滚。改动这些
路径时需要手动验证。

## 发版步骤

1. 确定 `X.Y.Z`，同步修改 `package.json`、
   `src-tauri/tauri.conf.json`、workspace `Cargo.toml`、
   `src-tauri/Cargo.toml`。
2. 运行 `cargo check --workspace --all-targets` 刷新 `Cargo.lock`，再运行
   `pnpm install --frozen-lockfile`、`cargo fmt --all -- --check`、
   `pnpm run test`、`pnpm run design:lint`、`pnpm run build`。提交预期的
   lockfile 改动，不要手工编辑 lockfile。
3. 复核 diff 和当前平台的 `release/` payload，然后提交版本与 lockfile
   改动。
4. 在该提交上执行 `git tag -a vX.Y.Z -m "OCG Manager vX.Y.Z"` 创建附注
   tag，再推送分支和 tag。
5. 等待 `release.yml` 的全部矩阵 job 与 `draft-release` 通过，复核 draft
   中的七份 payload、`SHA256SUMS`、冒烟日志与平台警告。
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
- [ ] 每个 runner 的 `release/SHA256SUMS` 与目录内全部 payload 一致；聚合
      release 的校验文件与七份平台 payload 一致。
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
- [ ] 复核 draft GitHub Release 说明与未签名 / ad‑hoc 警告，再把
      `--draft=false` 翻过来。
- [ ] 发布后确认 `container.yml` 通过，并按预期 digest 匿名拉取
      `ghcr.io/klarkxy/opencode-go-mgr:<version>`。

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
