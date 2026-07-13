# 用户指南

本指南面向把 OCG Manager 当作桌面应用、无头 Gateway 或 Docker 服务运行的使用者，
覆盖安装、配置、排障、Gateway 行为、真假熔断、协议转换与 CLI/Docker 的实际使用方法。

## 目录

- [产品定位](#产品定位)
- [安装与首次启动](#安装与首次启动)
- [管理面板](#管理面板)
  - [接入中心](#接入中心)
  - [应用教程](#应用教程)
  - [账号](#账号)
  - [日志](#日志)
  - [设置](#设置)
- [Gateway 行为](#gateway-行为)
  - [端点](#端点)
  - [鉴权](#鉴权)
  - [协议转换](#协议转换)
  - [账号选择与切换](#账号选择与切换)
  - [用量估算](#用量估算)
  - [真熔断与假熔断](#真熔断与假熔断)
- [CLI](#cli)
- [Docker](#docker)
- [数据与安全](#数据与安全)
- [限制](#限制)
- [常见问题](#常见问题)

## 产品定位

OCG Manager 把 OpenCode‑Go 账号 Key 保存在本地 SQLite，并通过回环 Gateway
`http://127.0.0.1:9042/v1` 暴露给客户端。同一个 Gateway 同时承载 Vue 3 管理面
板（路径 `/dashboard/`）和面板的 JSON API（路径 `/dashboard/api`）。每个节点都
独立运行——项目不提供远端同步、Admin API、遥测。

Gateway 的四件事：

1. 用面板签发的 **Gateway Key** 验证客户端。
2. 为请求挑一个可用的 OpenCode‑Go 账号。
3. 把请求转换到该模型在 OpenCode‑Go 上的原生协议，把响应再转回客户端协议。
4. 把请求日志、用量、冷却全部写回 SQLite，并在面板里呈现。

## 安装与首次启动

### Windows 10/11 x64

1. 运行 NSIS 安装包 `ocg-manager_<version>_windows-x64-setup.exe`，按当前用户
   安装，不需要管理员权限。
2. 在开始菜单中启动 **OCG Manager**。主窗口默认隐藏，从托盘图标打开管理面
   板。
3. Windows 首发版本未签名，SmartScreen 可能弹出警告，点击 **更多信息 →
   仍要运行** 继续。
4. 在 **账号** 视图添加 OpenCode‑Go 账号，复制 Gateway Key，把客户端指向
   `http://127.0.0.1:9042/v1`。
5. 卸载时会询问是否删除 `%USERPROFILE%\.ocg-mgr`；静默升级与静默卸载保留
   数据目录。

### macOS 11+ Intel / Apple Silicon

1. 打开 Universal DMG，把 **OCG Manager** 拖入 **Applications**。
2. 应用使用临时签名（ad‑hoc），首次启动可能被 Gatekeeper 拦截。打开
   **Privacy & Security**，点击 **Open Anyway** 放行。
3. 从托盘图标打开管理面板，添加账号，复制 Gateway Key，配置客户端。

### Linux x64

1. 用发行版包管理器安装 `.deb`，或对 AppImage 执行 `chmod +x
   ocg-manager_<version>_linux-x64.AppImage`。
2. 安装前先核对 `SHA256SUMS`。
3. 启动可执行文件，从托盘图标打开管理面板。
4. 数据保存在 `~/.ocg-mgr/`。

## 管理面板

管理面板是 Gateway 提供的单页 Vue 3 应用，左侧边栏四个主视图：**仪表盘**、
**账号**、**应用**、**日志**，外加 **设置** 入口。顶栏右侧是主题切换、语言
切换、退出登录。底栏展示版本信息。

面板原生支持十种语言：简体中文、繁體中文、English、日本語、한국어、Español、
Français、Deutsch、Português (Brasil)、Русский，默认简体中文。语言选择持久
化在 `localStorage` 的 `ocg-manager.locale`；如果浏览器拒绝持久化（例如隐私
窗口），当前会话仍能正常使用。

### 接入中心

首屏第一个面板——也是始终在最上方的面板——是 **接入中心**，它集中展示客户端需
要的全部信息：

- **Gateway Key**（也称 *Key*）：支持一键重新生成和复制。重新生成后旧 Key
  立即失效。
- **API Base URL**（例如 `http://127.0.0.1:9042/v1`）：一键复制。
- **Chat Completions**、**Responses**、**Messages** 的完整端点。
- **Gateway 转发到的上游地址** 与复制按钮。
- **HTTP 警告**：当解析出的根地址是公网或局域网上的明文 `http://` 时出现，提
  醒 Gateway Key 与请求内容会明文传输。

**设置 → 下游访问根地址（Downstream Access Root）** 只控制面板展示的 URL 和
教程里复制的 URL。留空则使用当前面板的 origin；如果客户端通过反向代理或别的
主机访问 Gateway，就填外部可访问的根地址，例如 `https://ocg.example.com`。尾
部的 `/v1` 会被自动识别并去掉。**这个设置不会**改变 Gateway 的监听地址、配
置 DNS、也不会创建反向代理——这些必须已经指向正在运行的 Gateway。明文 HTTP
允许用于局域网部署，但会暴露 Gateway Key 与请求内容。

### 应用教程

**应用** 视图为十个常见客户端预置了配置片段：Claude Code、Codex、OpenCode、
Cherry Studio、VS Code Copilot Chat、Trae、Cline、Roo Code、Continue、
Chatbox。每个教程展示协议、官方文档链接、简要说明、操作步骤以及一个或多个带
**复制** 按钮的代码块。代码块渲染两次：屏幕上的 *display* 版本中 Key 已脱
敏，*copy* 版本中是真实 Key，方便分享截图。

- 不带 `/v1` 的根地址：Claude Code、Cherry Studio、Chatbox。
- 带 `/v1` 的 API Base URL：OpenCode、Trae、Cline、Roo Code、Continue。
- 完整 `/v1/chat/completions` 端点：VS Code Copilot Chat。
- Codex：`/v1` 之外还需 `wire_api = "responses"`。

### 账号

**账号** 视图提供 OpenCode‑Go 账号的创建、编辑、启用、禁用与删除。每张账号
卡显示账号名、冷却状态，以及由本地估算驱动的 5 小时、本周、本月用量条。可
以和 Key 一起填入 OpenCode‑Go `username` 与 `password`，密码会加密存储，必要
时 Gateway 用它刷新上游会话。

冷却也可以在这个视图手动解除。解除后，进度条会立刻回到本地估算值。

### 日志

**日志** 视图展示 Gateway 转发的请求滚动列表：时间戳、所选账号、模型、状态
码、上游错误（如果有），以及上游发出 usage chunk 时的精确流式用量。没有
usage chunk 的行会标 `success_no_usage`，意味着本次流式用量无法精确统计。

### 设置

**设置** 视图暴露持久化的 Gateway 配置：

- **Gateway 端口**：Gateway 监听端口（默认 `9042`）。
- **Gateway Key**：与接入中心同一个值。
- **上游地址**：OpenCode‑Go 基础 URL。
- **下游访问根地址**：见 [接入中心](#接入中心)。
- **登录后自动启动**：只有已安装的 Windows 桌面版暴露此开关；开发构建、
  CLI、Docker、macOS、Linux 面板不显示。
- **连接 / 非流式 / 流式空闲超时**：应用于上游 HTTP 请求。

所有设置写入 SQLite，下次启动时重新加载。

## Gateway 行为

### 端点

Gateway 监听 `http://<bind>:<port>`，暴露：

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/messages` | Anthropic Messages |
| `GET`  | `/v1/models` | OpenAI 模型列表 |
| `GET`  | `/dashboard/` | Vue 3 管理面板（HTML） |
| `*`    | `/dashboard/api/...` | 管理面板 JSON API |
| `GET`  | `/healthz` | Docker 健康检查使用的存活探针 |

默认监听 `127.0.0.1:9042`。CLI 可用 `serve --host 0.0.0.0` 覆盖监听地址，用
`serve --port <port>` 覆盖端口。桌面端同样绑定回环，并由 Tauri 单实例锁防止
两个托盘程序争抢端口。

### 鉴权

Gateway 根据请求来源使用两种鉴权：

- **回环监听（默认）**：直接发到回环地址的请求跳过面板登录。客户端只需要在
  `Authorization: Bearer <key>` 中携带 **Gateway Key** 就能访问上游端点。桌
  面端与默认 CLI 都走这个分支。
- **非回环监听**：管理面板由唯一的 **管理员账号** 管控，密码以 Argon2 哈希存
  在 SQLite 中，登录后下发 HttpOnly 会话 Cookie。携带标准反向代理转发头但
  没有 Cookie 的请求仍需要登录。Docker 可以用 `OCG_ADMIN_USERNAME` 与
  `OCG_ADMIN_PASSWORD` 引导首个管理员；不提供时由首位注册者创建。

`Authorization` 里的 Gateway Key 是客户端唯一需要配置的凭证。它是**本地**
的——与 OpenCode‑Go 账号 Key 无关，Gateway 会从 SQLite 取出账号 Key，以自己
的 `Authorization: Bearer <opencode-go-key>` 头转给上游。

### 协议转换

每个已知模型都映射到自己的原生 OpenCode‑Go 协议。客户端用别的协议访问时，
Gateway 会把 **请求体** 转换到上游协议，把 **响应体**（或 SSE 流）再转换回
客户端协议，覆盖文本、system、图像、工具调用与工具结果、推理内容、完成状
态、错误、usage 字段。

Responses 端点在 Gateway 里是 **无状态** 的。下列字段会直接 `400` 拒绝，不
会静默忽略：

- `previous_response_id`
- `conversation`
- `store: true` 或任何不是 `false` 的 `store`
- `background: true`
- `input_image.file_id`（Gateway 没有 Files API）

function、custom、namespace 工具正常转换。`web_search`、`web_search_preview`、
`tool_search` 等 OpenCode‑Go 不支持的托管工具在自动工具模式下会被丢弃；显
式强制使用则返回 `400`。

### 账号选择与切换

按 **列表顺序** 尝试账号。选择器会跳过：

- 已禁用账号；
- 处于冷却中的账号；
- 已经在本次请求里失败过的账号（例如拿到 `429`）。

带有可识别 `Resets in …` 时间短语的 `429` 写入 `cooldown_until`，然后尝试
下一个账号。`401`/`403` 不写冷却、直接切换——这是鉴权问题，不是配额问题。
`5xx` 与网络错误对非流式请求重试一次后切换。当所有已启用账号都在冷却，
Gateway 返回 `429` 并带上最近的恢复时间。

### 用量估算

5 小时、本周、本月进度条是 **本地估算**，由 Gateway 实际转发的请求驱动，不
是上游账单视图。流式用量在 **上游发出 usage chunk 时** 才精确，否则日志
记 `success_no_usage`。

面板上每条进度条都关联账号冷却状态。真熔断生效时，对应进度条会被强制拉到
100% 并标红，见下一节。

### 真熔断与假熔断

- **假熔断（本地估算）**：本地估算是 **信号**，不是停止信号。本地估算到顶
  时 Gateway **不会停用** 该账号，仍会用它发请求。本地计费口径与上游账单/
  刷新边界可能不同，本地满格只是警告，不能证明上游已封禁账号。
- **真熔断（上游 429）**：Gateway 记录上游错误，解析响应中的 `Resets in …`
  时间，写入 `cooldown_until`，并切换到下一个可用账号。已知的 5 小时、本
  周、本月限额消息采用上游给出的重置时长；无法识别的 429 默认冷却 5 分钟。
- **无账号可用**：所有已启用账号都在冷却时，Gateway 返回 `429`，并带上最
  近的恢复时间。
- **面板展示**：真熔断生效时，对应 5 小时、本周或本月进度条被强制拉满并标
  红，即使本地估算值更低。账号在 `cooldown_until` 到期后自动恢复，也可以
  在面板里手动解除。

## CLI

下载对应平台的压缩包并解压成目录，目录里有可执行文件、`dist/` 与 `LICENSE`。
`dist/` 必须与可执行文件同级，`serve` 才能提供管理面板。Windows 上可执行文件
是 `ocg-manager-cli.exe`；Linux 解压后可能需要 `chmod +x ocg-manager-cli`。

CLI 数据目录默认在 `~/.ocg-mgr-cli`（所有平台一致），可用 `--data-dir <path>`
覆盖。加密 Key 默认是 `<data-dir>/.encryption-key`，可用 `--encryption-key
<key>` 或 `OCG_MANAGER_ENCRYPTION_KEY` 环境变量覆盖。

```text
ocg-manager-cli
├── serve         启动 Gateway 服务
│   --host        监听地址（默认 127.0.0.1）
│   -p, --port    Gateway 端口（覆盖配置）
│   --dashboard-dir  内置管理面板 dist 目录
├── key list      列出账号与启用状态
├── key add <name> <key>
│   --username    OpenCode-Go 登录账号
│   --password    OpenCode-Go 登录密码
├── key remove <id>      删除账号
├── key enable <id>      启用账号
├── key disable <id>     禁用账号
├── key ping [id]
│   --model       测试模型（默认 deepseek-v4-flash）
│   --message     用户消息（默认 "ping"）
│   --max-tokens  ping 的 max_tokens（默认 3）
└── status        显示数据目录、端口、Key、上游、账号总数
```

最快搭出一个无头 Gateway：

```bash
./ocg-manager-cli key add main sk-...
./ocg-manager-cli key list
./ocg-manager-cli serve --port 9042
```

`key ping` 会解密 Key、发送一条极小的 chat completion、打印真实的上游状态
码与一段响应体摘要——绕过面板直接拿到每个 Key 真实的 `401`/`403`/`429`/
`200`。

## Docker

构建并启动带管理面板的无头 Gateway：

```bash
cp .env.example .env
# 编辑 .env，选择初始管理员凭据
docker compose up -d --build
docker compose logs ocg-manager
```

`OCG_ADMIN_USERNAME` 与 `OCG_ADMIN_PASSWORD` **只在数据库里还没有管理员时**
生效。两个变量必须同时设置；只设一个会启动报错。已有管理员后，后续修改环境
变量不会再覆盖。都不设置时，由首位访客在面板里创建管理员。

打开日志里打印的管理面板 URL 并登录。数据与生成的加密 Key 持久化在
`ocg-data` 卷中。容器只把 Gateway 暴露在宿主机的 `127.0.0.1:9042`。直接发到
回环地址的请求跳过管理面板登录；通过反向代理的请求仍需登录。容器的
`HEALTHCHECK` 每 30 秒对 `127.0.0.1:9042` 做一次 TCP 探活。

需要 HTTPS 时，把现有反向代理指向该回环端口即可，例如 Caddy：

```caddyfile
ocg.example.com {
    reverse_proxy 127.0.0.1:9042
}
```

登录后先在面板里设置一个非空的 Gateway Key，再发送 API 流量。用
`docker compose down` 停止服务；只有当你想彻底删除账号、凭据、Key 时才追加
`-v`。

## 数据与安全

- **GUI 数据目录**：Windows `%USERPROFILE%\.ocg-mgr`；macOS / Linux
  `~/.ocg-mgr`。CLI 数据默认 `~/.ocg-mgr-cli`（所有平台一致），可用
  `--data-dir <path>` 覆盖。
- **Key 存储**：Key 在存储前做了混淆，**不是强加密**。macOS / Linux GUI 与
  CLI 的数据目录里还有 `.encryption-key` 文件；**必须和数据库一起备份**，
  丢失后已存的凭据将无法读取。把数据目录与可执行文件一并交给别人，等于把
  Key 交出去。
- **无跨节点同步**：每个节点由自己的面板管理，OCG Manager 不会在节点间同步
  账号凭据。
- **明文 HTTP 警告**：非回环的 `http://` 根地址会把 Gateway Key 与请求内容
  明文传输到网络中。请使用 HTTPS 或仅在可信局域网使用。
- **管理员密码**：唯一的管理员密码以 Argon2 哈希保存在 SQLite 中，没有自助
  找回流程——请保护好数据目录。

## 限制

- 未实现 `/embeddings`。
- 未实现 Gemini 协议转换。
- Responses 是无状态端点：必须设置 `store: false`。`previous_response_id`、
  `conversation`、`store: true`、`background: true` 全部直接 `400` 拒绝，不
  会静默忽略。
- Responses 支持图片 URL 与 data URL；`input_image.file_id` 返回 `400`，因为
  Gateway 没有 Files API。
- 跨协议转换无法保留约束的结构化输出与自定义工具语法会返回 `400`。
- Responses 的 `web_search`、`web_search_preview`、`tool_search` 等托管工具
  在 OpenCode‑Go 上无法运行；自动工具模式下会被丢弃，显式强制使用则返回
  `400`。function、custom、namespace 工具正常转换。
- 流式用量仅在上游发出 usage chunk 时精确，否则日志记为 `success_no_usage`。
- 当前 HTTP 面板没有暴露旧的隔离 WebView 浏览器命令。
- 已安装的 Windows 桌面版可以在用户登录时把 OCG Manager 拉起到托盘；开发
  构建、macOS、Linux、CLI、Docker 暂未实现该能力。
- 不发布 Windows / Linux ARM64、32 位 x86 构建；不支持 RPM、Snap、应用商店
  包、自动更新、Windows 正式签名、Apple 公证。

## 常见问题

- **托盘里点不开管理面板。**`127.0.0.1:9042` 被其他进程占用，或上一个托盘程
  序还握着单实例锁。退出 release 托盘程序（或跑 `scripts/free-dev-port.mjs`
  清理残留 Vite 进程）后重试。
- **上游返回 `401 Unauthorized`。**OpenCode‑Go 账号 Key 无效或被吊销。打开
  **账号** 视图替换 Key 再试；`key ping <id>` 是最快的验证手段。
- **本地进度条满格但请求依然成功。**这是 **假熔断**——本地估算不是上游账单。
  继续使用即可，Gateway 会继续转发。
- **本地进度条满格，Gateway 返回 `429`。**这是 **真熔断**。等
  `cooldown_until` 到期，或在 **账号** 视图手动解除冷却。
- **Gateway 返回 `429` 并提示 "all accounts cooling down"。**所有已启用账号
  都在冷却。等最近的恢复时间，或新增/启用其他账号。
- **Docker 首次注册的 `OCG_ADMIN_PASSWORD` 没生效。**这两个变量只在数据库还
  没有管理员时生效。重置 `ocg-data` 卷可以重新引导管理员。
- **SmartScreen / Gatekeeper 弹窗警告。**Windows 首发版本未签名、macOS 应用
  使用 ad‑hoc 签名。首次启动请用 **Open Anyway** 放行，警告本身不代表篡改。

---

[English user guide](USER.md) · [中文用户指南](USER.zh-CN.md) ·
[Maintainer guide](MAINTAINER.md) · [维护者指南](MAINTAINER.zh-CN.md) ·
[回到 README](../README.zh-CN.md)
