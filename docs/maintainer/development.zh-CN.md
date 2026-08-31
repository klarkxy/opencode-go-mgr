[English](development.md)

# 开发

## 前置要求

使用 Node.js 22（CI 基线）、pnpm 10.29.2（`package.json` 的 `packageManager`）和 Rust 1.85 或更高版本。原生构建依赖随 runner 调整，以 `.github/workflows/release.yml` 为准。当前 Linux runner 安装 `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf libfuse2 xvfb xauth xdg-utils dbus-x11`。

## 开发模式

退出 release 托盘程序以释放单实例锁和 `9042` 端口，然后启动完整开发栈：

```bash
pnpm install
pnpm run dev
```

`pnpm run dev` 执行 `tauri dev`。Windows 上 `predev`
（`scripts/free-dev-port.mjs`）检查 `127.0.0.1:30001` 并清理残留 Vite
进程。Tauri 启动 Vite，等 Gateway 就绪后打开
`http://127.0.0.1:30001/dashboard/`。Vite 把 `/dashboard/api`（含
WebSocket）代理到开发 Gateway 端口。`pnpm run dev` 默认使用 `19042`，避免
Windows HNS/WSL/Docker 在 `9042` 附近的排除端口范围阻塞开发栈；安装版仍默认
使用 `9042`。

如需改用其他开发端口，可在启动前用同一个运行时变量同时覆盖 Tauri 与 Vite：

```powershell
$env:OCG_GATEWAY_PORT = "19042"
pnpm run dev
```

变量生效时，设置页会以只读方式显示实际端口。以后要恢复保存的端口，可在启动前
执行 `Remove-Item Env:OCG_GATEWAY_PORT`。

- 前端（Vue、CSS、TypeScript）改动走 Vite HMR。
- Rust 改动走 Tauri watcher + Cargo 增量编译，然后重启进程。Rust 代码 **不会** 在进程内热替换，需要重启。

克隆后启用一次共享 git hooks（`pnpm install` 的 `prepare` 脚本也会运行）：

```bash
pnpm run hooks:install
# equivalent: git config core.hooksPath .githooks
```

当提交暂存了 `*.rs` 文件时，`.githooks/pre-commit` 会运行
`cargo fmt --all` 并重新 `git add`，保证提交符合 rustfmt（CI 用同一套
`cargo fmt --all -- --check`）。

## 检查与构建

开发过程中，默认运行能覆盖本次改动归属边界的最小检查：

| 改动范围 | 本地检查 |
| --- | --- |
| 单个前端或脚本行为 | `node --experimental-strip-types --test <test-file>` |
| Vue/dashboard 改动 | 相邻聚焦测试，再运行 `pnpm run build:web` |
| 单个 Rust crate | `cargo test -p <package>`；必要时再加测试名过滤 |
| Core 或 Dashboard V3 行为 | `cargo test -p ocg-core <filter>` |
| Desktop Host 行为 | `cargo test -p ocg-manager --lib` |
| Dashboard V3 Schema 或生成类型 | `pnpm run contract:v3:check` |

只在首次克隆或 pnpm 锁文件变化后运行 `pnpm install --frozen-lockfile`，不必每次
测试前都安装依赖。只有改动跨越前端/Rust、涉及共享清单或测试基础设施，或者进入
集成/发版门禁时，才运行完整的 `pnpm run test`。改动 `DESIGN.md` 或主题规则时运行
`pnpm run design:lint`。只有确实需要原生发版产物时才运行 `pnpm run build`；完整的
tag 前检查序列仍以 `releasing.zh-CN.md` 为准。

- `pnpm run build:web` 是 **纯前端** 生产构建（`vue-tsc && vite build`），只验证面板时用它。不要在 `pnpm run test` 后立即再跑一次：后者已经执行相同的 TypeScript 检查与 Vite 构建。
- `pnpm run test` 跑 `pnpm run test:web`（Node `--experimental-strip-types` 覆盖 `scripts/*.test.mjs` 与 `src/**/*.test.ts`）、`vue-tsc --noEmit`、`vite build`，然后 `cargo test --workspace --locked`。
- `pnpm run test:rust` 单独跑锁定依赖的 workspace Rust 套件。
- `pnpm run contract:v3:check` 用 `ocg-core` 的 `export_dashboard_v3_schema` example 重新生成 Dashboard V3 JSON Schema，若 `schema/dashboard-api-v3.schema.json` 或 `src/api/generated/dashboard-v3.ts` 漂移则失败。写入用 `pnpm run contract:v3:generate`。
- `pnpm run design:lint` 用 `@google/design.md` lint `DESIGN.md`。
- `pnpm run build` **只用于发版验证**。它运行 `scripts/release.mjs`，为当前原生平台构建 GUI 与 CLI，所有产物通过校验后原子替换 `release/`；失败时保留旧 `release/`。Cargo 增量编译缓存不清空。发版二进制使用 thin LTO（workspace `Cargo.toml` 的 `[profile.release]`），控制原生 CI 链接时间。

## Rust 检查

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --locked
```

第一条命令只检查格式，不修改文件；需要格式化时运行 `cargo fmt --all`。启用 hooks 后，`.githooks/pre-commit` 会自动为暂存了 Rust 文件的 commit 执行格式化。

聚焦工作：

```bash
cargo test -p ocg-domain
cargo test -p ocg-gateway
cargo test -p ocg-infra
cargo test -p ocg-core
cargo test -p ocg-manager-cli
cargo test -p ocg-browser-worker
cargo test -p ocg-manager --lib
cargo test -p ocg-core gemini
cargo test -p ocg-core claude_desktop
cargo test -p ocg-core dashboard_v3
cargo test -p ocg-core v3_runtime_invariants
```

分层约束（`ocg-domain` 与 `ocg-gateway` 保持无 I/O，kernel 不导入宿主代码）
属于设计意图，写在各模块头部注释里，由 crate 依赖图和代码评审保证，不再用
源码文本断言来守卫。

Rust 单元测试放在同名子模块文件里，而不是内联在源文件中，这样源文件本身保持
可读：`src/db.rs` 里只写 `#[cfg(test)] mod tests;`，测试正文放在
`src/db/tests.rs`。新增单元测试请写在那里。有两点需要注意：`include_str!`
是相对所在文件解析的，因此 `tests.rs` 里的 fixture 路径要比内联时多一层
`../`；另外少数被生产代码引用的 `#[cfg(test)]` 小工具仍然有意保留在源文件里。

不要再写断言源码文本、工作流 YAML 或文档正文的测试：读取 `.rs`、`.vue`、
`.yml`、`.md` 再用正则匹配内容，或用 `syn` 遍历 AST 来管制 import，都只会在
有人修改该文件时失败，而修法永远是在同一个提交里改测试本身。请改为通过公开
API 断言行为。

测试真实账号流时先在沙箱跑 CLI：

```bash
ocg-manager-cli --data-dir /tmp/ocg-cli-test key add smoke sk-smoke
ocg-manager-cli --data-dir /tmp/ocg-cli-test key list
ocg-manager-cli --data-dir /tmp/ocg-cli-test serve --port 19042
```

CLI 只暴露 `serve` / `key` / `status`。`key add` 通过
`account_control::create_go_api_key` 创建启用且 ready 的 OpenCode Go 卡，并
bump 该进程的 `settings_revision`。它不能创建 Custom 账号、子 Key 或设置。直接
`Database::update_account` 仍不 bump revision；这是有意的，也不是 CLI 路径。

## 前端检查

前端单元测试与代码同目录（`src/**/*.test.ts`），用 Node 的
`--experimental-strip-types` 运行，不需要额外测试框架。脚本级测试在
`scripts/*.test.mjs`（发版辅助、Dashboard V3 契约、容器发布）。最后跑
`pnpm run build:web` 与 `pnpm run contract:v3:check`。

17 个应用教程由 `src/views/application-guides.ts` 驱动；改动注册表时检查教程
数量、唯一 ID、协议端点、display/copy 脱敏差异，以及 Claude Desktop 三个角色
模型的持久化行为。

侧栏是仪表盘、接入 Key、账号、供应商、应用、日志、设置。`pricing` 查询是供应
商页的遗留别名。`BrowserSession` 是会话层，不是第八个侧栏项。

## 本地发布冒烟构建（Windows）

本地冒烟构建步骤如下。完整发布流程、CI 矩阵与签名密钥见
`docs/maintainer/releasing.md` 与 `docs/maintainer/ci.md`。

1. 确保 `pnpm` 可用（`packageManager: pnpm@10.29.2`）。如果 PATH 中没有 pnpm，请在用户目录创建一个 shim。
2. 退出已安装的 release 版本，释放单实例锁与 `9042`：

   ```powershell
   Get-NetTCPConnection -LocalPort 9042 -ErrorAction SilentlyContinue |
     Select-Object OwningProcess | Get-Process | Stop-Process -Force
   ```

3. 版本对齐：`package.json`、`src-tauri/tauri.conf.json`、workspace `Cargo.toml`、`src-tauri/Cargo.toml`，以及 `compose.example.yaml` 中的 title/default image。
4. 运行 `pnpm run build`（调用 `scripts/release.mjs`）。

签名相关环境变量（与 CI / MAINTAINER 一致）：

- `TAURI_SIGNING_PRIVATE_KEY`：私钥内容，或仓库外的安全路径（脚本会将其规范化为 Tauri 路径形式）。
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥密码（如有）。
- `TAURI_UPDATER_PUBLIC_KEY`：公钥内容；必须与 `src-tauri/updater-public-key.sha256` 匹配。
- `OCG_REQUIRE_UPDATER_ARTIFACTS=1`：强制生成签名产物；缺少密钥时失败。

**没有 `TAURI_SIGNING_PRIVATE_KEY` 时只生成普通本地包，不能用于应用内升级，仅供本地冒烟测试。**

在 Windows 上，Tauri 可能把 `src-tauri/Cargo.toml` 与 `src-tauri/gen/schemas/*.json` 的换行符转为 CRLF；构建后若要干净的工作树：

```powershell
git checkout -- src-tauri/Cargo.toml src-tauri/gen/schemas/desktop-schema.json src-tauri/gen/schemas/windows-schema.json
```

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](development.md) · [文档索引](../README.zh-CN.md)
