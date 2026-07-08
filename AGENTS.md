# AGENTS.md — ocg-manager

> 本文件面向 AI 编码助手。阅读本文档前，默认你对项目一无所知。
> 项目背景与详细功能需求见 `REQUIREMENTS.md`。
> 以下内容完全基于当前目录中的实际文件与配置，不做过度推断。

## 1. 项目概述

| 项 | 值 |
|---|---|
| 项目代号 | `ocg-manager`（OpenCode-Go 多账号管理器） |
| npm 包名 | `ocg-manager-ui` |
| Cargo 包名 | `ocg-manager` |
| 产品名 | `OCG Manager` |
| Tauri 应用标识符 | `com.ocg-manager.app` |
| 目标平台 | Windows 桌面端（单 exe，系统托盘常驻）+ 跨平台 CLI（Linux / macOS / Windows / Docker） |
| 核心目标 | 管理多个 OpenCode-Go 账号的 API key，并提供统一的 OpenAI 兼容 Gateway |
| 当前状态 | **功能基本实现**：Gateway、账号管理、熔断、用量统计、内置浏览器、系统托盘、日志、设置均已落地；已拆分为 `ocg-core` lib + Tauri GUI + CLI 三个 Rust crate |

当前代码已具备：

- **前端**：Vue 3 + TypeScript + naive-ui，包含仪表盘、账号管理、日志、设置四个页面。
- **Rust 后端**：完整的 Axum Gateway（`/v1/chat/completions`）、SQLite 持久化、多账号轮询/熔断/故障转移、用量估算、SSE 流式透传。
- **Tauri 集成**：系统托盘、WebView2 内置浏览器、invoke 命令桥接。
- **CLI**：跨平台命令行应用 `ocg-manager-cli`，最小化管理 key 列表与熔断状态，无 GUI。
- 需求文档 `REQUIREMENTS.md` 中规划的系统托盘、Gateway、账号管理、熔断、用量估算、内置浏览器、日志、设置等模块均已实现。

## 2. 技术栈与运行时架构

### 2.1 技术栈

| 层级 | 技术 | 版本/说明 |
|---|---|---|
| 前端框架 | Vue 3 + TypeScript | `vue@3.x`，`typescript@5.x` |
| UI 组件库 | naive-ui | `naive-ui@2.x` |
| 前端构建 | Vite | `vite@5.x`，端口 `30001` |
| 桌面壳 | Tauri v2 | `tauri@2.x`，启用 `tray-icon` |
| Tauri 插件 | shell / http / notification / store | 均为 v2 |
| Rust Gateway | Axum + Tokio | `axum@0.8`，`tokio@1`（full） |
| HTTP 转发 | reqwest | `reqwest@0.12`（json + stream） |
| HTTP 中间件 | tower / tower-http | CORS、trace |
| 持久化 | SQLite | `rusqlite@0.33`（bundled） |
| 序列化 | serde / serde_json | — |
| 错误处理 | anyhow | — |
| 日志 | tracing / tracing-subscriber | 当前代码中未实际使用 |
| 工具 crate | parking_lot / chrono / uuid / base64 / bytes | — |
| CLI 参数解析 | clap | `ocg-manager-cli` 使用 v4 derive API |

### 2.2 运行时架构

```text
用户客户端（OpenAI SDK / 任意工具）
  → http://127.0.0.1:PORT/v1/...
  → Tauri 应用（Rust + WebView2）
       - 系统托盘
       - 管理界面（账号 / 用量 / 日志 / 设置）
       - 内嵌 Gateway
  → Rust Gateway（Axum）
       - 接收 OpenAI 格式请求
       - 多账号轮询 / 熔断 / 切换
       - 透传 SSE 流式响应
       - 用量与熔断状态记录
  → 多个 OCG key
```

## 3. 目录结构

```text
d:\0 code\ocg-manager
├── AGENTS.md                 # 本文件
├── Cargo.toml                # Rust workspace 根配置
├── README.md                 # 项目 README（英文）
├── README.zh-CN.md           # 项目 README（中文）
├── REQUIREMENTS.md           # 需求文档 v1.0
├── assets/                   # 静态资源（Logo、源图、脚本）
├── crates/                   # Rust workspace crates
│   ├── ocg-core/             # 通用 lib：Gateway、DB、熔断、选择器、成本、crypto、models
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── crypto.rs     # KeyCipher trait + MachineBoundCipher/StaticKeyCipher
│   │       ├── db.rs         # SQLite 打开 + 迁移 + 查询
│   │       ├── models.rs     # serde 结构体、AppConfig、枚举
│   │       ├── state.rs      # CoreState、配置读写、gateway-key 生成
│   │       └── gateway/      # Axum Gateway（mod/handler/forwarder/selector/limit/cost）
│   └── ocg-cli/              # 命令行应用
│       ├── Cargo.toml
│       └── src/
│           └── main.rs       # clap 命令解析 + serve/key/circuit/status
├── docs/                     # 中英文文档
│   ├── en/
│   └── zh/
├── index.html                # Vite 入口 HTML
├── package.json              # 前端 npm 配置
├── package-lock.json         # 锁定依赖版本
├── tsconfig.json             # TypeScript 配置
├── vite.config.ts            # Vite 配置
├── src/                      # 前端源码
│   ├── api/tauri.ts          # invoke() 封装 + 共享 TS 类型
│   ├── views/                # Dashboard / Accounts / Logs / Settings
│   ├── App.vue               # 外壳 + 侧边栏导航
│   ├── main.ts               # Vue + naive-ui 启动入口
│   └── styles/main.css
└── src-tauri/                # Tauri GUI binary crate
    ├── Cargo.toml            # Tauri 包配置
    ├── build.rs              # Tauri 构建脚本
    ├── tauri.conf.json       # Tauri 应用配置
    ├── installer.nsh          # NSIS 卸载钩子（询问是否删除数据目录）
    ├── capabilities/
    │   └── default.json      # Tauri v2 权限配置
    ├── icons/                # 32/128/256/512 + icon.ico
    └── src/
        ├── commands/         # Tauri 命令处理（account/setting/gateway/log/dashboard/browser）
        ├── state.rs          # GuiState 包装 CoreState + current_browser_window
        ├── tray.rs           # 系统托盘初始化
        ├── lib.rs            # run()——串联 DB、Gateway、托盘、命令
        └── main.rs           # 可执行入口 → run()
```

## 4. 关键配置文件

### 4.1 `package.json`

- 包名：`ocg-manager-ui`
- 版本：`0.1.0`
- 类型：`module`
- scripts：`dev`、`build`（`vue-tsc && vite build`）、`preview`
- 依赖：Tauri API 与四个插件（shell / http / notification / store）、`naive-ui`、`@vicons/antd`、`vfonts`
- 开发依赖：`vite`、`vue`、`typescript`、`vue-tsc`、`@vitejs/plugin-vue`、`@types/node`
- `@tauri-apps/cli` 已在 dependencies 中，`npx tauri ...` 可直接使用。

### 4.2 根目录 `Cargo.toml`

- Workspace 成员：`crates/ocg-core`、`crates/ocg-cli`、`src-tauri`
- 共享 `workspace.package`：`version = "0.1.0"`、`edition = "2024"`、`rust-version = "1.85.0"`
- Release 配置：`opt-level = 3`、`lto = true`、`strip = true`、`panic = "abort"`

### 4.3 `crates/ocg-core/Cargo.toml`

- 包名：`ocg-core`
- 类型：`lib`
- 包含通用依赖：`axum`、`tokio`、`reqwest`、`rusqlite`、`serde`、`chrono`、`uuid`、`parking_lot`、`anyhow`、`base64`、`bytes`、`futures-util`、`tower`、`tower-http`
- 提供 `KeyCipher` trait 及两种实现：`MachineBoundCipher`（Windows 机器绑定）与 `StaticKeyCipher`（跨平台固定密钥）

### 4.4 `crates/ocg-cli/Cargo.toml`

- 包名：`ocg-manager-cli`
- 类型：`bin`
- 依赖：`ocg-core`、`clap`、`tokio`、`anyhow`、`chrono`、`uuid`

### 4.5 `src-tauri/Cargo.toml`

- 包名：`ocg-manager`
- `rust-version = "1.85.0"`（与 `edition = "2024"` 一致）
- Tauri feature：`["tray-icon"]`
- 依赖 `ocg-core = { path = "../crates/ocg-core" }`，不再直接声明 Gateway/持久化相关 crate
- `tauri-plugin-store` 已声明但被注释掉（暂未使用）
- Release profile 包含 `opt-level = 3`、`lto = true`、`strip = true`、`panic = "abort"`

### 4.6 `src-tauri/tauri.conf.json`

- `devUrl`: `http://localhost:30001`
- `frontendDist`: `../dist`
- `beforeDevCommand`: `npm run dev`
- `beforeBuildCommand`: `npm run build`
- Bundle 目标：`nsis`（Windows 安装包），`perMachine` 安装，含 `installer.nsh` 卸载钩子
- 主窗口：1200×800，可缩放，标题 `OCG Manager`，`visible: false`（由 setup 控制显示）
- 系统托盘：`trayIcon.iconPath = "icons/32x32.png"`
- `security.csp` 已配置：`default-src 'self'; connect-src 'self' http://localhost:9042; ...`
- `security.capabilities`: `["default"]`
- `plugins`: `{}`

### 4.7 `vite.config.ts`

- 插件：`@vitejs/plugin-vue`
- 端口：`30001`，`strictPort: true`，`host: "127.0.0.1"`
- 路径别名：`"@": "./src"`（与 `tsconfig.json` 一致）
- `envPrefix`: `["VITE_", "TAURI_"]`
- 构建目标：`es2022`，开启 minify，sourcemap 仅在 `TAURI_DEBUG` 时启用
- `watch.ignored`: `["**/target/**", "**/src-tauri/target/**"]`

### 4.8 `tsconfig.json`

- `target`: `ES2022`，`module`: `ESNext`
- `strict`: `true`
- 路径别名：`"@/*": ["./src/*"]`

### 4.9 `src-tauri/capabilities/default.json`

- 标识符：`default`，作用于 `main` 窗口
- 权限：`core:default`、`core:window:*`、`shell:allow-open`

## 5. 构建、开发与测试命令

### 5.1 环境要求

- **Node.js**：当前环境 `v24.16.0`（`package.json` 未声明 `engines`）
- **Rust**：当前环境 `rustc 1.96.0`、`cargo 1.96.0`
- **Tauri CLI**：已在 `package.json` 依赖中，`npx tauri ...` 可直接使用
- **Windows**：10/11 + WebView2

### 5.2 npm 脚本

| 命令 | 作用 |
|---|---|
| `npm install` | 安装前端依赖 |
| `npm run dev` | 启动 Vite 开发服务器（`http://127.0.0.1:30001`） |
| `npm run build` | `vue-tsc` 类型检查 + `vite build` 生成 `dist/` |
| `npm run preview` | 预览生产构建 |
| `node playwright-debug.mjs` | 运行 Playwright UI 冒烟测试（需 `npm run dev` 已启动） |

### 5.3 Cargo / Tauri 命令

| 命令 | 作用 |
|---|---|
| `cargo check --workspace` | 检查整个 workspace |
| `cargo build --release --bin ocg-manager` | 编译 Tauri GUI Release 二进制 |
| `cargo build --release --bin ocg-manager-cli` | 编译 CLI Release 二进制 |
| `cargo test --workspace` | 运行 workspace 全部 Rust 测试 |
| `cargo run --bin ocg-manager-cli -- --help` | 查看 CLI 帮助 |
| `npx tauri dev` | 启动 Tauri 开发模式 |
| `npx tauri build` | 打包 Windows 安装包（NSIS） |

### 5.4 当前可行性

项目处于可运行状态。`cargo check --workspace`、`cargo test --workspace`、`npx tauri dev` 和 `npx tauri build` 均可正常执行。CLI 可在 Windows / Linux / macOS 上编译运行（GUI 仍依赖 Windows + WebView2）。

## 6. 代码组织

### 6.1 前端

- `src/main.ts`：创建 Vue 应用，注册 naive-ui，挂载 `#app`；开发模式下会自动注入 `src/api/dev-mock.ts`。
- `src/App.vue`：应用外壳，含侧边栏导航（仪表盘/账号管理/日志/设置）。
- `src/views/`：四个页面组件（`Dashboard.vue`、`Accounts.vue`、`Logs.vue`、`Settings.vue`）。
- `src/api/tauri.ts`：所有 Tauri `invoke()` 调用的类型化封装，含共享 TS 接口。
- `src/api/dev-mock.ts`：浏览器开发/Playwright 测试用的 Tauri `invoke` 模拟层；在真实 Tauri 环境中自动失效。
- `src/styles/main.css`：全局样式。

### 6.2 后端 / Rust

#### 通用 core lib（`crates/ocg-core`）

- `crates/ocg-core/src/lib.rs`：重新导出 `crypto`、`db`、`gateway`、`models`、`state`。
- `crates/ocg-core/src/gateway/`：Gateway 核心，含 `mod.rs`（Axum 路由构建与启停）、`handler.rs`（请求鉴权 + 重试循环）、`forwarder.rs`（上游转发 + SSE 透传 + 429 冷却写入）、`selector.rs`（账号选择策略）、`limit.rs`（解析 429 文案 `Resets in …` → Duration）、`cost.rs`（价格表 + 成本计算）。无独立 `circuit_breaker` 模块；冷却字段在 `accounts` 表上。
- `crates/ocg-core/src/db.rs`：SQLite 数据库打开、迁移、CRUD 操作。
- `crates/ocg-core/src/models.rs`：数据结构定义（`Account`、`AppConfig`、`ForwardLog` 等）。
- `crates/ocg-core/src/state.rs`：`CoreState`（Arc 包装），管理配置、DB、Gateway handle、HTTP client、选择器计数器、加密器。
- `crates/ocg-core/src/crypto.rs`：`KeyCipher` trait + `MachineBoundCipher`（Windows 机器绑定 XOR 混淆）+ `StaticKeyCipher`（跨平台固定密钥混淆）。

#### Tauri GUI（`src-tauri`）

- `src-tauri/src/lib.rs`：`run()` 函数——初始化 DB、加载配置、创建 `CoreState`、启动 Gateway、构建 Tauri app、注册托盘和命令、拦截窗口关闭事件。
- `src-tauri/src/main.rs`：Windows 子系统入口，调用 `run()`。
- `src-tauri/src/commands/`：9 个命令模块（`account.rs`、`setting.rs`、`gateway.rs`、`log.rs`、`dashboard.rs`、`browser.rs`、`mod.rs`），通过 `invoke_handler!` 注册。
- `src-tauri/src/state.rs`：`GuiState` 包装 `CoreState`，额外管理 `current_browser_window`。
- `src-tauri/src/tray.rs`：系统托盘初始化（左键显示窗口、右键菜单）。

#### CLI（`crates/ocg-cli`）

- `crates/ocg-cli/src/main.rs`：使用 `clap` 解析命令，初始化 `CoreState`（注入 `StaticKeyCipher`），实现 `serve` / `key`（list/add/remove/enable/disable/ping）/ `status` 命令。

### 6.3 Tauri 插件

- `tauri-plugin-shell`、`tauri-plugin-http`、`tauri-plugin-notification` 已在 `lib.rs` 中注册。
- `tauri-plugin-store` 已在 `Cargo.toml` 声明但被注释，暂未使用。
- 权限配置在 `src-tauri/capabilities/default.json` 中。

## 7. 开发规范

### 7.1 Ponytail 原则（简化阶梯）

在编码前按这个顺序检查：

1. 这个功能是否真的需要存在？不需要就删掉。
2. 项目里是否已经有类似实现？复用。
3. Rust 标准库能处理吗？用标准库。
4. 操作系统或 Tauri 内建功能能处理吗？用它们。
5. 依赖中已经引入的 crate 能处理吗？用它们。
6. 是否一行代码就能完成？就写一行。
7. 以上都不行，才写最小的可工作实现。

### 7.2 绝对不牺牲

以下事项不能因为"简化"而被省略：

- API key 安全存储（不能明文存储，至少做简单加密）
- 错误处理和网络超时重试
- 账号级熔断状态管理
- 数据库事务一致性
- 流式响应的 SSE 透传

### 7.3 代码风格

- 函数名、变量名不要缩写，清晰表达意图。
- Rust 代码保持简洁、直接、易读。
- 错误信息要有质量，避免笨重的错误传递。
- 已有 naive-ui 组件库，新增 UI 优先使用其组件。
- 不对第一版做过度优化。

## 8. 测试策略

当前自动化测试：

- **Rust 单元测试**：`crates/ocg-core/src/crypto.rs` 中 crypto 加解密往返测试，`crates/ocg-core/src/gateway/limit.rs` 中 `parse_reset` 的已知文案用例。
- **Playwright UI 冒烟测试**：`playwright-debug.mjs`，在 headless Chromium 中验证仪表盘、账号新增、日志页、设置页、侧边栏折叠等关键 UI 流程。依赖 `npm run dev` 启动的 Vite 开发服务器，并通过 `src/api/dev-mock.ts` 模拟 Tauri 后端。

未找到：

- 前端 `*.test.ts` / `*.spec.ts`
- Rust `tests/` 目录
- `vitest`、`jest` 相关配置

建议按以下顺序补充：

1. **Rust 单元测试**：Gateway 账号选择策略、cooldown 边界条件、成本计算。
2. **Rust 集成测试**：模拟 OCG 上游，验证 `/v1/chat/completions` 转发与失败切换。
3. **前端组件测试**：账号卡片、设置表单等 Vue 组件。
4. **端到端测试**：真实 Tauri 应用启动、托盘、窗口行为（工具链较重，可延后）。

## 9. 部署与打包

- 通过 Tauri 打包为 Windows NSIS 安装程序（`targets: "nsis"`），`perMachine` 安装。
- Release 构建开启 LTO 与 strip，生成单 exe。
- `tauri.conf.json` 中 `bundle.icon` 已配置 `icons/32x32.png`、`icons/128x128.png`、`icons/256x256.png`、`icons/512x512.png`、`icons/icon.ico`，`src-tauri/icons/` 目录已存在。
- `installer.nsh` 在卸载时询问用户是否删除 `%USERPROFILE%\.ocg-mgr` 数据目录。
- 没有 CI/CD 配置，没有发布脚本。

## 10. 安全与合规

- **API key 存储**：使用机器绑定 XOR 混淆（`crypto.rs`），**非**密码学安全。阻止明文窥探，但不替代密钥保险库。需真正保密时替换为 `aes-gcm` 或 KMS。
- **Gateway 鉴权**：客户端统一使用 `Authorization: Bearer <gateway-key>`，不把 OCG key 直接暴露给客户端。
- **合规红线**：不自动注册、不自动充值、不绕过验证码；邀请码仅用于展示/手动复制；不收集用户数据到远程服务器。
- **Tauri 安全**：
  - CSP 已配置（`default-src 'self'; connect-src 'self' http://localhost:9042; ...`）。
  - capabilities 已配置（`src-tauri/capabilities/default.json`）。
- **网络**：Gateway 默认监听 `127.0.0.1`，不直接暴露到公网。

## 11. 已知缺口与待办事项

当前代码的功能已基本完整，但仍有一些已知限制：

1. 流式请求记录 0 token/0 成本（用量统计仅覆盖非流式响应）。
2. `test_account` 命令不探测上游——仅解密并掩码展示 key。
3. `auto_start` 配置标志未接入 OS 开机自启（`tauri-plugin-autostart` 未注册）。
4. 按月充值日期自动重置冷却未实现（仅支持手动重置）。
5. `reqwest::Client` 在 `CoreStateInner::new` 中构建一次，120s 超时——连接池为进程级、复用同 host。
6. 加密是 XOR 混淆，非 AEAD（`crypto.rs`）。
7. `tauri-plugin-store` 已声明在 `package.json` 但 Rust 端被注释，未注册。
8. Playwright 测试为浏览器内模拟，未覆盖真实 Tauri IPC、系统托盘、Gateway 网络转发等桌面端行为。
9. 无 CI/CD 配置。

## 12. 与需求文档的关系

- 详细需求、功能规格、界面规划、开发顺序见：`REQUIREMENTS.md`。
- 当本文件与 `REQUIREMENTS.md` 冲突时，以 `REQUIREMENTS.md` 为准；本文件仅提供项目现状梳理与编码风格引导。
