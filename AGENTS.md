# AGENTS.md — ocg-manager

本文件给 AI 编码助手使用。以当前代码为准，别按旧 README 或过期需求文档补不存在的东西。

## 项目事实

- 产品：OCG Manager，OpenCode-Go 多账号本地管理器。
- 前端：Vue 3 + TypeScript + naive-ui，源码在 `src/`。
- 前端 API：`src/api/tauri.ts` 是历史命名，当前封装 HTTP `/dashboard/api`，不是 Tauri `invoke()`。
- Rust workspace：`crates/ocg-core`、`crates/ocg-cli`、`src-tauri`。
- 核心 Gateway：Axum + Tokio + reqwest，默认监听 `127.0.0.1:9042`。
- 持久化：SQLite，GUI 数据目录为 Windows `%USERPROFILE%\.ocg-mgr` 或 macOS/Linux `~/.ocg-mgr`，CLI 默认 `~/.ocg-mgr-cli`。
- 桌面端：Tauri v2 跨平台托盘应用，主窗口默认隐藏；托盘/单实例逻辑用系统浏览器打开 `http://127.0.0.1:<port>/dashboard/`，回环监听自动跳过登录。
- Tauri commands 仍注册在 `src-tauri/src/commands/`，但不是当前 Vue dashboard 的主调用路径。
- 每个节点都由自己的 dashboard 管理；项目不提供远端同步或 Admin API。
- 非回环监听使用单管理员登录；Docker 可通过 `OCG_ADMIN_USERNAME` 和 `OCG_ADMIN_PASSWORD` 首次初始化，未提供时由首个注册者创建管理员。

## 关键文件

- `crates/ocg-core/src/gateway/`：OpenAI/Anthropic 兼容路由、转发、选择器、冷却、费用统计。
- `crates/ocg-core/src/dashboard.rs`：当前 Vue 面板使用的 `/dashboard/api`。
- `crates/ocg-core/src/db.rs`：SQLite schema、迁移、查询。
- `crates/ocg-core/src/models.rs`：共享 serde 类型和 `AppConfig`。
- `crates/ocg-cli/src/main.rs`：CLI `serve`、`key`、`status`。
- `src-tauri/src/lib.rs`：Tauri 启动、Gateway 启动、托盘、命令注册。
- `src-tauri/src/tray.rs`：托盘菜单和 dashboard 打开逻辑。
- `src/views/`：Dashboard / Accounts / Logs / Settings。

## 常用命令

```powershell
pnpm install
pnpm run dev
pnpm run build:web
pnpm run test
pnpm run design:lint
pnpm run build
```

开发前先退出 release 托盘程序，释放单实例锁和 `9042` 端口，然后执行 `pnpm run dev`。Tauri 会启动 Vite，并在 Gateway 就绪后打开 `http://127.0.0.1:30001/dashboard/`；前端由 Vite 热更新，Rust 由 Cargo 增量编译并重启进程。

`pnpm run build` 只用于当前原生平台的最终 release 构建，并在成功后原子替换 `release/`；只验证前端时用 `pnpm run build:web`。Windows 仅发布 x64 NSIS 安装包，macOS 发布 Universal DMG，Linux x64 发布 AppImage 和 deb；CLI 压缩包必须包含同级 `dist/` 与 `LICENSE`。

## 开发约束

- 工作区可能是脏树。先看 `git status --short`，不要回退不是你改的内容。
- Ponytail 原则优先：能删就删，能复用现有代码就复用，别加“以后可能用”的抽象。
- 不要新增 Tauri `invoke` 前端路径，除非你明确要恢复桌面 WebView 内调用；当前主路径是 HTTP dashboard。
- 安全边界别省：Gateway 鉴权、key 存储混淆、HTTP URL 校验、冷却状态写入、SSE 透传都不能为了简化拿掉。
- 不要重新引入远端同步；远端节点通过自己的 dashboard 管理。
- `auto_start` 目前只存在 Windows 注册表 helper，非 Windows 为 no-op，当前 HTTP dashboard 不暴露这个设置。

## 测试策略

- Rust 逻辑优先跑 `cargo test -p ocg-core`。
- CLI 改动跑 `cargo test -p ocg-manager-cli`，必要时用临时 data dir 做真实 `key add/list/status`。
- 前端改动跑 `pnpm run build:web`。
- Rust 和前端回归跑 `pnpm run test`；GUI/打包改动跑当前平台的 `pnpm run build`。需要声明真实桌面可用时，要实际启动安装包、DMG 或 AppImage 并验证 dashboard/gateway 行为。

## 当前已知缺口

- `/embeddings` 和 Gemini 协议转换未实现。
- 流式 usage 依赖上游 usage chunk；没有 chunk 时会记为 `success_no_usage`。
- Tauri 隔离浏览器 command 存在，但当前 HTTP dashboard 没有按钮调用它。
- `src-tauri/src/commands/*` 与 `crates/ocg-core/src/dashboard.rs` 有部分重复逻辑；当前不要大拆，除非同时迁移缺失行为并补验证。
- 当前不发布 Windows/Linux ARM64、32 位 x86、RPM、Snap 或应用商店包，也没有自动更新、Windows 正式签名或 Apple notarization。
