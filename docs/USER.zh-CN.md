[English](USER.md)

# 用户指南

本指南面向把 OCG Manager 当作桌面应用、无头 Gateway 或 Docker 服务运行的使用者，
按实际使用顺序讲解安装、日常使用和排障，并解释 Gateway 行为、真/假熔断与协议
转换的工作方式。

## 目录

- [产品定位](#产品定位)
- [安装与首次启动](#安装与首次启动)
- [接入第一个客户端](#接入第一个客户端)
- [升级、备份、恢复与卸载](#升级备份恢复与卸载)
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

OCG Manager 把 OpenCode-Go 账号 Key 保存在本地 SQLite，并通过回环 Gateway
`http://127.0.0.1:9042/v1` 暴露给客户端。同一个 Gateway 同时承载 Vue 3 管理
面板（路径 `/dashboard/`）和面板的 JSON API（路径 `/dashboard/api`）。每个
节点都独立运行——项目不提供远端同步、Admin API、遥测。

Gateway 的四件事：

1. 用面板签发的 **Gateway Key** 验证客户端。
2. 为请求挑一个可用的 OpenCode-Go 账号。
3. 把请求转换到该模型在 OpenCode-Go 上的原生协议，把响应再转回客户端协议。
4. 把请求日志、用量、冷却全部写回 SQLite，并在面板里呈现。

## 安装与首次启动

### Windows 10/11 x64

1. 运行 NSIS 安装包 `ocg-manager_<version>_windows-x64-setup.exe`，按当前用户
   安装，不需要管理员权限。
2. 在开始菜单中启动 **OCG Manager**。正常启动会在系统浏览器打开管理面板；之后
   可从托盘图标重新打开。
3. 当前 Windows 包未签名，SmartScreen 可能弹出警告，点击 **更多信息 → 仍要运行**
   继续。
4. 在 **账号** 视图添加 OpenCode-Go 账号，复制 Gateway Key，把客户端指向
   `http://127.0.0.1:9042/v1`。
5. 卸载时会询问是否删除 `%USERPROFILE%\.ocg-mgr`；静默升级与静默卸载保留数据
   目录。

### macOS 11+ Intel / Apple Silicon

1. 打开 Universal DMG，把 **OCG Manager** 拖入 **Applications**。
2. 应用使用临时签名（ad-hoc），首次启动可能被 Gatekeeper 拦截。打开
   **Privacy & Security**，点击 **Open Anyway** 放行。
3. 启动应用。正常启动会在系统浏览器打开管理面板；之后可从托盘图标重新打开。
   添加账号，复制 Gateway Key，配置客户端。

### Linux x64

1. 安装前先核对 `SHA256SUMS`。
2. 用发行版包管理器安装 `.deb`，或对 AppImage 执行
   `chmod +x ocg-manager_<version>_linux-x64.AppImage`。
3. 启动可执行文件。正常启动会在系统浏览器打开管理面板；之后可从托盘图标重新
   打开。
4. 数据保存在 `~/.ocg-mgr/`。

已安装的 Windows 版自动启动时只会进入托盘，不会主动打开浏览器。

## 接入第一个客户端

1. 在 **账号** 视图用 Key 添加一个 OpenCode-Go 账号。登录账号可选；新增时如果
   先填写账号，它会自动作为必填名称，直到你手动修改名称。面板不收集或维护
   OpenCode-Go 登录密码。
2. 在面板的 **接入中心** 复制 **Gateway Key** 和 **API Base URL**
   （`http://127.0.0.1:9042/v1`）。
3. 把客户端指向该 Base URL 并填入 Gateway Key。**应用** 视图内置了 13 个常见
   客户端的教程。
4. 发一个真实请求验证。

Gateway Key 是客户端唯一需要的凭证，支持三种请求头：
`Authorization: Bearer <key>`、Anthropic 风格的 `x-api-key: <key>`、Gemini
风格的 `x-goog-api-key: <key>`。它是本地密钥，与 OpenCode-Go 账号 Key 无关；
账号 Key 由 Gateway 从 SQLite 取出后自行注入上游。

五类兼容入口的最小 POSIX shell 检查：

```bash
BASE=http://127.0.0.1:9042
KEY=replace-with-gateway-key

# OpenAI Chat Completions
curl "$BASE/v1/chat/completions" -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","messages":[{"role":"user","content":"ping"}],"stream":false}'

# OpenAI Responses
curl "$BASE/v1/responses" -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","input":"ping","store":false}'

# Anthropic Messages
curl "$BASE/v1/messages" -H "x-api-key: $KEY" \
  -H "anthropic-version: 2023-06-01" -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","max_tokens":16,"messages":[{"role":"user","content":"ping"}]}'

# Claude Desktop：别名会改写为“应用”视图保存的实际模型
curl "$BASE/claude-desktop/v1/messages" -H "x-api-key: $KEY" \
  -H "anthropic-version: 2023-06-01" -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4-6","max_tokens":16,"messages":[{"role":"user","content":"ping"}]}'

# Gemini generateContent
curl "$BASE/v1beta/models/deepseek-v4-flash:generateContent" \
  -H "x-goog-api-key: $KEY" -H "Content-Type: application/json" \
  -d '{"contents":[{"role":"user","parts":[{"text":"ping"}]}]}'
```

## 升级、备份、恢复与卸载

从 [GitHub 最新 Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest)
下载升级包，并用同一 Release 的 `SHA256SUMS` 校验：PowerShell 使用
`Get-FileHash <文件> -Algorithm SHA256`，macOS 使用 `shasum -a 256 <文件>`，
Linux 使用 `sha256sum <文件>`。

### 从 v1.4.1 进入升级通道

v1.4.1 还没有签名的应用内升级能力。Windows 用户只需按下面步骤进入升级通道
一次：

1. 从 OCG Manager 托盘图标选择 **退出**。
2. 运行首个支持升级的 Windows setup。
3. 在“升级方式”页选择第二项 **不要卸载，直接安装（Install without
   uninstalling）**，然后继续。第一项只是 Tauri 默认选中项，并非升级所必需。

不要先卸载 v1.4.1——这次直接覆盖会保留现有数据目录。高级用户也可选择执行等价
命令：

```powershell
Start-Process -FilePath .\ocg-manager_<version>_windows-x64-setup.exe -ArgumentList '/UPDATE','/P','/R' -Wait
```

macOS 与 Linux 用户按下文的直接替换方式完成这一次过渡。装好首个支持升级的
Release 后，后续签名桌面版可在 **设置** 中一键下载并安装。CLI 与 Docker 仍需
手动升级。

### 备份

1. 停止所有会写数据的进程：从桌面托盘选择 **退出**，用 Ctrl+C 或服务管理器停止
   CLI，Docker 则执行 `docker compose stop`。
2. 复制 **整个** GUI 数据目录、CLI 数据目录或 Docker `ocg-data` 卷。已停止的
   Docker 容器可用 `docker compose cp ocg-manager:/data/. ../ocg-data-backup`
   复制数据。
3. 备份必须放在仓库外，并确认其中有 `data.sqlite`，以及适用时的
   `.encryption-key`。

### 恢复

1. 先停进程，把现有数据移到别处，再把完整备份放回原目录或空的 Docker 卷。
2. 启动相同或更新的版本。

注意事项：

- Docker `/data` 中的文件必须继续允许 UID/GID `10001` 写入。
- Windows GUI 的混淆信息绑定 Windows 用户与机器，换机后不能直接恢复账号 Key 或
  密码；请在新机器创建全新数据并重新录入凭据。
- macOS/Linux GUI、CLI 与 Docker 恢复时必须保留 `.encryption-key`，或原来显式
  传入的 `--encryption-key` / `OCG_MANAGER_ENCRYPTION_KEY` 值。
- 项目不保证数据库自动向下兼容，不要用旧版本打开新版数据库。

### 恢复 Docker 备份到全新卷

先确认备份有效，并确认 `.env` 固定到原版本或更新版本。下面的
`docker compose down -v` 会永久删除当前卷，必须先把当前数据另行保存后才能执行：

```bash
docker compose down -v
docker compose run --rm --no-deps --user root \
  --cap-add CHOWN --cap-add DAC_OVERRIDE --cap-add FOWNER \
  --entrypoint sh \
  --volume ../ocg-data-backup:/backup:ro \
  ocg-manager \
  -c 'cp -a /backup/. /data/ && chown -R 10001:10001 /data && \
      find /data -type d -exec chmod 700 {} + && \
      find /data -type f -exec chmod 600 {} +'
docker compose up -d --no-build
docker compose ps
```

原部署如果使用了 `OCG_MANAGER_ENCRYPTION_KEY`，恢复前先把同一个秘密值写回
`.env`。在管理面板、账号和一次真实 Gateway 请求都验证通过前，请保留备份。

### 分运行方式的升级与卸载

应用内升级不可用时，GUI 也按下面方式直接覆盖。

- **Windows GUI**：退出托盘程序，运行新版安装包，在“升级方式”页选择 **不要
  卸载，直接安装**。在 Windows **已安装的应用** 中卸载；卸载程序会询问是否删除
  `%USERPROFILE%\.ocg-mgr`。
- **macOS GUI**：用新版 DMG 中的应用替换 **Applications** 里的旧应用。删除应用
  即可卸载；只有确定也要删除数据时才另行删除 `~/.ocg-mgr`。
- **Linux GUI**：用新版 `.deb` 覆盖安装，或替换 AppImage。卸载软件包或删除
  AppImage 后，数据仍保留在 `~/.ocg-mgr`，除非手动删除。
- **CLI**：整体替换解压目录，保持可执行文件、`dist/` 与 `LICENSE` 同级。删除该
  目录即可卸载；数据仍保留在 `~/.ocg-mgr-cli` 或自定义 `--data-dir`。
- **Docker**：备份后依次执行 `docker compose pull` 和
  `docker compose up -d --no-build`。生产部署建议把 `OCG_IMAGE` 固定到完整版本
  标签。`docker compose down` 只删容器、保留 `ocg-data`；
  `docker compose down -v` 会永久删除卷，只能在确认备份有效且确实要重置时使用。
  切换到旧镜像不等于回滚数据库；需要数据库回滚时，应同时恢复该旧版本升级前制作
  的完整备份。

## 管理面板

管理面板是 Gateway 提供的单页 Vue 3 应用，左侧边栏四个主视图：**仪表盘**、
**账号**、**应用**、**日志**，外加 **设置** 入口。顶栏右侧是主题切换、语言
切换、退出登录。

面板原生支持十种语言：简体中文、繁體中文、English、日本語、한국어、Español、
Français、Deutsch、Português (Brasil)、Русский，默认简体中文。语言选择持久化
在 `localStorage` 的 `ocg-manager.locale`；如果浏览器拒绝持久化（例如隐私窗
口），当前会话仍能正常使用。

### 接入中心

首屏第一个面板——也是始终在最上方的面板——是 **接入中心**，它集中展示客户端
需要的全部信息：

- **Gateway Key**（也称 *Key*）：支持一键重新生成和复制。重新生成后旧 Key 立即
  失效。
- **API Base URL**（例如 `http://127.0.0.1:9042/v1`）：一键复制，另附 Chat
  Completions、Responses、Messages 的完整端点。
- **Gateway 转发到的上游地址** 与复制按钮。
- **HTTP 警告**：当解析出的根地址是非回环的明文 `http://` 时出现，提醒 Gateway
  Key 与请求内容会明文传输。

**设置 → 下游访问根地址（Downstream Access Root）** 只控制面板展示的 URL 和
教程里复制的 URL。有效值按以下顺序决定：

1. 非空的 `OCG_CLIENT_ROOT_URL` 环境变量。
2. 面板保存的手工值。
3. 自动推导值：生产面板使用当前 origin，开发面板使用
   `http://127.0.0.1:<Gateway 端口>`。

环境变量接管时输入框为只读，修改变量并重启后生效，变量值不会写入 SQLite。自动
值会显示在输入框中，但不会被保存。

如果客户端通过反向代理或别的主机访问 Gateway，就设置外部可访问的根地址，例如
`https://ocg.example.com`。尾部的 `/v1` 会被自动识别并去掉。**这个设置不会**
改变 Gateway 的监听地址、配置 DNS、也不会创建反向代理——这些必须已经指向正在
运行的 Gateway。明文 HTTP 允许用于局域网部署，但会暴露 Gateway Key 与请求内容。

### 应用教程

**应用** 视图为 13 个常见客户端预置了配置片段：Claude Code、Claude Desktop、
Codex、Gemini CLI、OpenCode、OpenClaw、Hermes、Cherry Studio、VS Code Copilot
Chat、Cline、Roo Code、Continue、Chatbox。每个教程展示协议、官方文档链接、操作
步骤、模型选择，以及一个或多个带 **复制** 按钮的代码块。屏幕上的代码块中 Key
已脱敏，复制出来的才是真实 Key，方便分享截图。

各客户端的 Base URL 约定：

- Claude Code、Cherry Studio、Chatbox 使用不带 `/v1` 的根地址。
- Claude Desktop 使用根地址加 `/claude-desktop`，由客户端继续请求
  `/claude-desktop/v1/messages` 与 `/claude-desktop/v1/models`。
- Gemini CLI 使用根地址，并设置 `GOOGLE_GENAI_API_VERSION=v1beta`。远端 Base
  URL 必须使用 HTTPS；只有 `localhost`、`127.0.0.1` 与 `[::1]` 可用 HTTP。解析
  出的根地址不符合该客户端限制时，应用页会禁用 Gemini 配置复制。
- OpenCode、OpenClaw、Hermes、Cline、Roo Code、Continue 使用带 `/v1` 的 API
  Base URL。
- VS Code Copilot Chat 使用完整 `/v1/chat/completions` 端点；Codex 除 `/v1`
  Base URL 外还需 `wire_api = "responses"`。

可选模型是上游当前公布、Gateway 已知可路由且存在于活动价格快照中的交集；每次
返回应用页都会重新同步，因此确认价格刷新后模型类型也会跟着更新。模型选择和编辑
过的代码片段按应用分别缓存在当前页面里，刷新页面后重置。
**恢复默认** 会重置当前应用的模型选择与片段草稿。

Claude Desktop 是例外，它的模型映射是持久化的：复制配置前，选中的 `sonnet`、
`opus`、`haiku` 目标模型会通过受保护的面板 API 保存到 SQLite。留空的角色回退
到第一个已配置模型，三个角色不能同时为空。它的恢复操作回到当前页面已加载或
最后保存的映射。

### 账号

**账号** 视图提供 OpenCode-Go 账号的创建、编辑、启用、禁用与删除。每张账号卡
显示账号名、冷却状态，以及由本地估算驱动的 5 小时、本周、本月用量条。

- **用量基线**：每个窗口都可以输入百分比或拖动进度条，将其保存为当前实际用量
  基线；保存后，OCG Manager 记录的成功请求成本会继续累加到该基线上。达到 100%
  仍只是提示，不会阻止 Gateway 选择这个账号。
- **标识与凭据**：名称是必填的主要展示标识。登录账号可选；新增时如果先填写
  账号，它会自动同步为名称，手动修改名称后不再跟随。面板保存账号 Key，但不
  收集或维护 OpenCode-Go 登录密码。
- **购买日期**：新增账号默认使用浏览器当天，也可以在新增或展开编辑表单里修改。
  到期日取下一个自然月同日；目标月份没有该日时取月末，例如 `2026-01-31` 的
  到期日是 `2026-02-28`。账号页与仪表盘显示剩余天数、今天到期或已到期天数。
  该信息只作提醒，不会自动禁用账号或阻止 Gateway 选择。
- **优先级顺序**：账号卡左侧的拖动手柄用于调整优先级，鼠标、触屏和触控笔都
  可以使用；聚焦手柄后也可以按上、下方向键移动。排序保存在当前节点的 SQLite
  中，仪表盘、日志账号筛选、CLI 列表和 Gateway 选择器都使用同一顺序。
- **解除冷却**：冷却也可以在这个视图手动解除。解除后，进度条会立刻回到本地
  估算值。

### 价格表

**价格表** 视图显示当前 revision、文档更新时间、窗口额度、四类美元 token 单价、
`Usage` 和用于额度结算的单一官方倍率。可组合价格以标准档作为完整根行；展开根行后，
可查看 Qwen 的更高上下文档，以及 MiniMax 的长上下文、高速、优先服务和组合升级档。

官方倍率默认按“月额度 / Usage”推导，也可以按临时活动手动修改；修改会生成新的
持久化 revision 并立即用于后续本地额度估算。只有用户点击刷新时才会访问
`https://opencode.ai/docs/go/`。如果刷新的官方倍率与当前设置不同，面板会先列出
差异，并让用户选择保留当前倍率或采用最新官方倍率；确认前不会启用新的价格和
模型列表。抓取或校验失败时继续使用最后一次成功快照。

### 日志

**日志** 视图展示 Gateway 转发的请求滚动列表：时间戳、所选账号、模型、状态码、
上游错误（如果有），以及上游发出 usage chunk 时的精确流式用量。

- 没有 usage chunk 的行会标 `success_no_usage`。usage chunk 只会让 token 数量
  准确；费用仍按当前 OpenCode Go 价格快照估算。
- `outcome_unknown` 表示上游可能已经完成并扣额，但 Gateway 超时或丢失响应；
  这类请求不会自动重试，且本地额度消耗保持未知。

### 设置

**设置** 视图暴露持久化的 Gateway 配置：

- **Gateway 端口**：Gateway 监听端口（默认 `9042`）。
- **Gateway Key**：与接入中心同一个值。
- **上游地址**：OpenCode-Go 基础 URL。
- **下游访问根地址**：见 [接入中心](#接入中心)。
- **登录后自动启动**：只有已安装的 Windows 桌面版暴露此开关；开发构建、CLI、
  Docker、macOS、Linux 面板不显示。
- **连接 / 非流式 / 流式空闲超时**：默认分别为 30、900、300 秒。非流式值是
  整个请求的总时限；流式空闲值按相邻响应 chunk 之间的等待时间执行。旧安装只有
  在完整的旧默认组合仍为 `30/120/300` 时才会迁移到 `30/900/300`，任何自定义
  组合都会原样保留。
- **检查更新 / 立即升级**：支持升级的已安装桌面版会检查 GitHub 最新 Release，
  并可下载、校验签名、安装对应平台的包。v1.4.1 必须先完成上文的一次直接覆盖
  安装；开发构建、CLI、Docker 仍显示发布页并手动升级。主机必须能访问 GitHub；
  检查或安装失败不影响 Gateway 转发。

配置项写入 SQLite，下次启动时重新加载；检查更新是按需动作，不会持久化。

## Gateway 行为

### 端点

Gateway 监听 `http://<bind>:<port>`，暴露：

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/messages` | Anthropic Messages |
| `GET`  | `/v1/models` | OpenAI 模型列表 |
| `POST` | `/v1beta/models/{model}:generateContent` | Gemini 非流式生成；`/v1/...` 同样可用 |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | Gemini SSE 生成；`/v1/...` 同样可用 |
| `POST` | `/v1beta/models/{model}:countTokens` | 返回 `501`，Gemini CLI 可回退到本地估算 |
| `POST` | `/v1beta/models/{model}:embedContent` | 返回 `501`；当前不支持 embeddings |
| `GET`  | `/claude-desktop/v1/models` | Claude Desktop 可选别名列表 |
| `POST` | `/claude-desktop/v1/messages` | Claude Desktop Messages；改写三个 Claude 模型别名 |
| `GET`  | `/dashboard/` | Vue 3 管理面板（HTML） |
| `*`    | `/dashboard/api/...` | 管理面板 JSON API |

默认监听 `127.0.0.1:9042`。CLI 可用 `serve --host 0.0.0.0` 覆盖监听地址，用
`serve --port <port>` 覆盖端口。桌面端同样绑定回环，并由 Tauri 单实例锁防止
两个托盘程序争抢端口。项目没有 HTTP 健康检查端点；Docker 只检查容器内部的
TCP `9042` 端口。

### 鉴权

Gateway API 必须携带 **Gateway Key**，可使用 `Authorization: Bearer <key>`、
`x-api-key: <key>` 或 `x-goog-api-key: <key>` 三种请求头。转发前 Gateway 会
移除客户端鉴权头，再按实际上游协议注入所选 OpenCode-Go 账号 Key：Messages 上
游使用 `x-api-key`，Chat Completions 与 Responses 上游使用
`Authorization: Bearer`。

管理面板的鉴权模式取决于监听地址：

- **回环监听（默认）**：直接发到回环地址的请求跳过面板登录；但只要带有
  `Forwarded`、`x-forwarded-for`、`x-forwarded-proto` 或 `x-real-ip` 中任一请求
  头，仍必须登录。客户端还需要 **Gateway Key** 才能访问上游端点。桌面端与默认
  CLI 都走这个分支。
- **非回环监听**：管理面板由唯一的 **管理员账号** 管控，密码以 Argon2 哈希存
  在 SQLite 中，登录后下发 HttpOnly 会话 Cookie。携带标准反向代理转发头但没有
  Cookie 的请求仍需要登录。Docker 可以用 `OCG_ADMIN_USERNAME` 与
  `OCG_ADMIN_PASSWORD` 引导首个管理员；不提供时由首位注册者创建。

### 协议转换

每个已知模型都映射到自己的原生 OpenCode-Go 协议。客户端用别的协议访问时，
Gateway 会把 **请求体** 转换到上游协议，把 **响应体**（或 SSE 流）再转换回客户
端协议，覆盖文本、system、图像、工具调用与工具结果、推理内容、完成状态、错误、
usage 字段。

Chat Completions 与 Messages 的未知模型保留客户端选择的协议；Responses 与
Gemini 上的未知模型、未知 Claude Desktop 别名直接 `400` 拒绝——Gateway 不会靠
试探选协议，否则可能把同一请求重复计费。

Gateway 协议端点最多接受 16 MiB 的 JSON 请求体；这是 HTTP 传输上限，与具体模型
的上下文窗口不是同一个概念。若 OCG Manager 前面还有反向代理，需要把代理的请求
体上限设为至少 16 MiB，否则请求可能尚未到达 Gateway 就被代理以
`413 Payload Too Large` 拒绝。

#### Responses 是无状态端点

下列字段会直接 `400` 拒绝，不会静默忽略：

- `previous_response_id`
- `conversation`
- `store: true` 或任何不是 `false` 的 `store`
- `background: true`
- `input_image.file_id`（Gateway 没有 Files API）

function、custom、namespace 工具正常转换。`web_search`、`web_search_preview`、
`tool_search` 等 OpenCode-Go 不支持的托管工具在自动工具模式下会被丢弃；显式
强制使用则返回 `400`。

#### Gemini 是客户端兼容层

Gateway 不会把 Gemini 线格式数据发往上游。它把 `contents`、纯文本
`systemInstruction`、受支持的 `inlineData` 图片、`functionDeclarations`、函数
调用/结果、JSON Schema 输出、生成选项、Google 错误信封、usage 元数据和 SSE 帧，
转换到已知模型的 Chat Completions 或 Messages 原生协议并转回。`v1beta` 与 `v1`
两种 URL 形式都接受。

兼容边界——不会静默假装等价：

- 非空 `safetySettings` 无法跨协议执行同一套内容安全阈值，直接返回
  `400 INVALID_ARGUMENT`；省略、`null` 或空数组可以使用。不要把
  `safetySettings` 当作会被上游执行的提示。
- `generationConfig.topK` 与 `generationConfig.thinkingConfig` 只作为跨协议
  兼容提示接受；采样、推理预算和 thoughts 展示不保证与 Google Gemini 等价，实际
  能力由所选 OpenCode-Go 模型决定。
- 其他无法跨协议保留的非空生成选项（包括 `seed`、presence/frequency penalty、
  logprobs 与 media resolution）会返回 `400`，不会静默丢弃。
- `cachedContent`、`fileData`、Google Search、URL Context、Code Execution、
  多模态 function response、function response 的 schema/behavior、`VALIDATED`
  函数调用模式、`candidateCount` 大于 1、非 TEXT 输出模态会返回 `400`。图片请
  改用 base64 `inlineData`，支持 PNG、JPEG、GIF、WebP。
- `countTokens` 与 `embedContent` 返回 `501 UNIMPLEMENTED`；Gemini CLI 对前者
  失败可使用本地估算，Gateway 当前没有 embeddings 路由。

#### Claude Desktop 别名

专用入口只接受服务端公布的 `claude-sonnet-4-6`、`claude-opus-4-6`、
`claude-haiku-4-5-20251001` 三个别名。Gateway 在进入现有 Messages 转换链前，
把别名替换成“应用”视图保存的实际模型；响应中的模型能力、工具支持和上下文限制
仍以实际模型为准。`sonnet`、`opus`、`haiku` 映射序列化在 `AppConfig` 中；留空
角色继承第一个已配置角色，面板返回补全后的三角色映射。

### 账号选择与切换

按 **列表顺序** 尝试账号；该顺序可在账号页拖动调整并持久保存。选择器会跳过：

- 已禁用账号；
- 处于冷却中的账号；
- 已经在本次请求里失败过的账号（例如拿到 `429`）。

带有可识别 `Resets in …` 时间短语的 `429` 写入 `cooldown_until`，然后尝试下一
个账号。`401`/`403` 不写冷却、直接切换——这是鉴权问题，不是配额问题。只有能
证明请求尚未发出的 DNS/TCP/TLS 建连失败，才会在同一账号重试一次，流式请求也
遵循这一规则。

`408`、`5xx`、建连后的传输失败、响应体超时和流式中断一律不重放。无法确认上游
是否已经完成的失败会以 `upstream_outcome_unknown` 返回并记为
`outcome_unknown`，因为它可能已经消耗额度。当所有已启用账号都在冷却，Gateway
返回 `429` 并带上最近的恢复时间。

### 用量估算

5 小时、本周、本月进度条是 **本地估算**，由 Gateway 实际转发的请求驱动，不是
上游账单视图。四类 token 美元单价、窗口额度和模型 `Usage` 都来自当前 OpenCode
Go 快照。

- 官方倍率默认按“月额度 / Usage”推导。用户可以为临时活动修改该倍率，新的请求
  会使用当前持久化值；价格刷新不会在未确认时覆盖它。
- 当前 `deepseek-v4-pro`（DS V4 Pro）、`mimo-v2.5-pro` 和 Grok 的 `$15` Usage
  对应 `60 / 15 = 4×` 官方倍率。
- 最后再叠加适用的本地 MiniMax 调整。计算不使用供应商 API 实际价格、人民币或
  汇率。

日志中的几种特殊情况：

- 没有流式 usage chunk 时，日志记 `success_no_usage`。
- 快照中没有的模型仍可转发，但会记为 `success_unpriced`，额度消耗显示为空且
  不进入累计。
- 快照功能上线前的成功记录保留原值、标记为“旧口径”，不会回算。
- 手动保存的百分比会成为对应窗口的基线；此后有价格的成功请求继续累加，直到
  再次手动修改，或收到可识别的上游限额重置。
- `outcome_unknown` 表示上游可能已经完成并扣额，但 Gateway 超时或丢失响应；
  这类请求不会自动重试，本地额度消耗保持未知。

面板上每条进度条都关联账号冷却状态，见下一节。

### 真熔断与假熔断

- **假熔断（本地估算）**：本地估算是 **信号**，不是停止信号。本地估算到顶时
  Gateway **不会停用** 该账号，仍会用它发请求。本地计费口径与上游账单/刷新边界
  可能不同，本地满格只是警告，不能证明上游已封禁账号。
- **真熔断（上游 429）**：Gateway 记录上游错误，解析响应中的 `Resets in …`
  时间，写入 `cooldown_until`，并切换到下一个可用账号。已知的 5 小时、本周、
  本月限额消息采用上游给出的重置时长，并清零对应窗口的用量基线；冷却期间该进
  度条保持 100%，冷却结束后从 0% 开始累加新的本地成功成本。无法识别的 429 默
  认冷却 5 分钟，但不会清除任何手动维护的用量值。
- **无账号可用**：所有已启用账号都在冷却时，Gateway 返回 `429`，并带上最近的
  恢复时间。
- **面板展示**：真熔断生效时，对应 5 小时、本周或本月进度条被强制拉满并标红，
  即使本地估算值更低。账号在 `cooldown_until` 到期后自动恢复，也可以在面板里
  手动解除。

## CLI

下载对应平台的压缩包并解压成目录，目录里有可执行文件、`dist/` 与 `LICENSE`。
`dist/` 必须与可执行文件同级，`serve` 才能提供管理面板。Windows 上可执行文件
是 `ocg-manager-cli.exe`；Linux 解压后可能需要 `chmod +x ocg-manager-cli`。

CLI 数据目录默认在 `~/.ocg-mgr-cli`（所有平台一致），可用 `--data-dir <path>`
覆盖。混淆密钥默认保存在 `<data-dir>/.encryption-key`，可用名为
`--encryption-key <key>` 的参数或 `OCG_MANAGER_ENCRYPTION_KEY` 环境变量覆盖。

```text
ocg-manager-cli
├── serve         启动 Gateway 服务
│   --host        监听地址（默认 127.0.0.1）
│   -p, --port    Gateway 端口（设置并保存配置）
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

`serve --port <port>` 会把新端口写入 SQLite；之后不带 `--port` 的 `serve` 会
继续使用该值。

`key ping` 会读取混淆后的 Key、发送一条极小的 chat completion、打印真实的上游
状态码与一段响应体摘要——绕过面板直接拿到每个 Key 真实的
`401`/`403`/`429`/`200`。

## Docker

GHCR 上的公开无头镜像无需登录即可拉取。它是 Linux 容器，目前只发布
`linux/amd64`，没有原生 ARM64 镜像。每个 Release 也会附带只拉取镜像的
`compose.example.yaml`；把它保存为 `compose.yaml`，并按需在同目录创建 `.env`。
示例默认固定对应的发布版本，也可用 `OCG_IMAGE` 覆盖。或者在包含 `compose.yaml`
与 `.env.example` 的仓库目录中运行（建议检出对应 Release tag）：

```bash
git clone --branch v1.5.0 --depth 1 https://github.com/klarkxy/opencode-go-mgr.git
cd opencode-go-mgr
cp .env.example .env
# PowerShell：Copy-Item .env.example .env
# 对外开放服务前先编辑 .env
docker compose pull
docker compose up -d --no-build
docker compose ps
```

### 选择镜像

- 仓库内支持源码构建的 `compose.yaml` 默认使用
  `ghcr.io/klarkxy/opencode-go-mgr:latest`；Release 中的 `compose.example.yaml`
  默认固定对应的完整版本。
- 生产部署建议在 `.env` 中用 `OCG_IMAGE` 固定完整版本标签，例如
  `ghcr.io/klarkxy/opencode-go-mgr:1.5.0`。
- 完整版本与 `sha-<commit>` 标签用于标识单次发布，按发布策略不应移动；`1.5`
  与 `latest` 会继续移动。技术上只有
  `ghcr.io/klarkxy/opencode-go-mgr@sha256:...` digest 真正不可变。
- 需要调试当前源码时，设置 `OCG_IMAGE=ocg-manager:local`，再执行
  `docker compose up -d --build`。`NPM_REGISTRY` 与 `CARGO_REGISTRY` 只属于源码
  构建参数，不会改变已拉取镜像。

| 变量 | 作用范围 | 含义 |
| --- | --- | --- |
| `OCG_IMAGE` | Compose | 镜像标签、镜像站、本地名称或不可变 digest。 |
| `OCG_PORT` | Compose | 宿主机回环端口；容器内仍监听 `9042`。 |
| `OCG_ADMIN_USERNAME` + `OCG_ADMIN_PASSWORD` | 首次启动 | 可选管理员引导；必须同时设置或都不设置。 |
| `OCG_CLIENT_ROOT_URL` | 运行时 | 只读覆盖外部客户端根地址。 |
| `OCG_MANAGER_ENCRYPTION_KEY` | 恢复时 | 原部署曾显式使用的混淆密钥。 |
| `NPM_REGISTRY` + `CARGO_REGISTRY` | 源码构建 | 仅 `--build` 使用的依赖注册表。 |

### 管理员引导

`OCG_ADMIN_USERNAME` 与 `OCG_ADMIN_PASSWORD` **只在数据库里还没有管理员时**
生效。

- 两个变量必须同时设置；只设一个会启动报错。
- 已有管理员后，后续修改环境变量不会再覆盖。
- 都不设置时，由首位访客在面板里创建管理员。
- 管理员创建后，只要保留卷，就可以移除这两个变量，数据库里的账号仍然有效。
  执行 `docker compose up -d --no-build --force-recreate` 把它们从容器环境中
  移除。

拥有 Docker daemon 权限的人可以看到容器环境变量；请保护 `.env`、使用长随机
密码，且不要把未初始化的面板直接暴露到公网。

### 密钥与地址

`OCG_MANAGER_ENCRYPTION_KEY` 是高级恢复覆盖项。正常部署请留空，让生成的
`.encryption-key` 留在数据卷中。原部署如果显式使用了该变量，恢复时必须使用同
一值；修改或丢失会导致已保存凭据无法读取。请把它当作密码保管。

可选的 `OCG_CLIENT_ROOT_URL` 等同于面板里的“下游访问根地址”，适合在反向代理
或 Dashboard 与 Gateway 使用不同外部地址时显式指定客户端根地址。非空值必须是
绝对 HTTP(S) URL；设置后优先于 SQLite 中的手工值，非法值会让进程启动失败。它
不配置监听、DNS 或反向代理。一般填写 `https://ocg.example.com`，不要填写
`/dashboard/` 或具体 API 端点；末尾 `/v1` 可省略或保留。

### 运行时行为

在 `.env` 中设置 `OCG_PORT` 可修改宿主机端口，容器内仍固定使用 `9042`。打开
`http://127.0.0.1:<OCG_PORT>/dashboard/` 并登录。请访问 `/dashboard/`，不要
把服务根路径 `/` 当作面板地址。

- 数据与生成的 `.encryption-key` 混淆密钥持久化在 `ocg-data` 卷中。
- 容器进程监听 `0.0.0.0`，因此即使只发布到宿主机 `127.0.0.1`，管理面板也必须
  使用管理员登录；宿主机端口映射只限制可达范围，不会启用回环免登录。
- 容器的 `HEALTHCHECK` 每 30 秒对容器内 `127.0.0.1:9042` 做 TCP 探活，不存在
  `/healthz` 路由。这个 TCP 检查只说明进程正在监听，不能证明面板 API、上游账号
  或真实模型请求可用。
- 镜像以非特权 `ocg` 用户（UID/GID 10001）运行。随附 Compose 把根文件系统设为
  只读、把 `/tmp` 挂成 tmpfs、丢弃全部 Linux capability，并启用
  `no-new-privileges`；只有命名卷 `ocg-data` 保存可写应用状态。
- 启动日志会打印 Gateway Key，因此日志输出和 Docker daemon 权限都属于敏感信息。
  如果 Docker 主机默认没有限制日志大小，请由部署方配置日志轮转。

常用检查命令：

```bash
docker compose config --quiet
docker compose ps
docker compose logs --tail=100 -f ocg-manager
curl --fail http://127.0.0.1:9042/dashboard/
```

如果修改过 `OCG_PORT`，请把 curl 命令里的 `9042` 替换成实际宿主机端口。

### 校验镜像

每个稳定镜像都带 SPDX SBOM、BuildKit SLSA provenance 与 GitHub 签名的
provenance attestation。可这样检查发布版本：

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:1.5.0
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr:1.5.0 \
  --repo klarkxy/opencode-go-mgr
```

第二条命令要求 GitHub CLI 已登录。公开镜像可匿名拉取；如果 OCI 客户端仍要求
registry 凭据，请用具备 package 读取权限的 token 登录 `ghcr.io`。Provenance
证明产物如何构建，不等于漏洞扫描。

如果 Gateway Key 泄露，请重新生成。

### HTTPS

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
- **凭据存储**：账号 Key 与保存的登录密码在存储前都只做混淆，**不是密码学
  保护**。macOS / Linux GUI 与 CLI 的数据目录里还有 `.encryption-key` 文件；
  **必须和数据库一起备份**，丢失后已存的凭据将无法读取。混淆不是安全边界：拿到
  数据目录及其 `.encryption-key`，或能在原 Windows 用户/机器上下文运行 Windows
  GUI 的人，都能恢复账号 Key 与保存的登录密码。
- **无跨节点同步**：每个节点由自己的面板管理，OCG Manager 不会在节点间同步账号
  凭据。
- **明文 HTTP 警告**：非回环的 `http://` 根地址会把 Gateway Key 与请求内容明文
  传输到网络中。请使用 HTTPS 或仅在可信局域网使用。
- **管理员密码**：唯一的管理员密码以 Argon2 哈希保存在 SQLite 中，没有自助找回
  流程——请保护好数据目录。

## 限制

- 未实现 `/embeddings`。Gemini `embedContent` 会被路由，但返回 Google 风格的
  `501 UNIMPLEMENTED`。
- Gemini `countTokens` 同样返回 `501`；Gemini CLI 预期回退到本地估算。只有
  `generateContent` 与 `streamGenerateContent` 会真正转发。
- 非空 Gemini `safetySettings` 返回 `400`，因为不同上游协议无法保留其安全语义；
  `null` 与空数组不携带策略，可以接受。
- Gemini `cachedContent`、`fileData`、Google Search 工具、`urlContext`、Code
  Execution、多模态 function response、function response 的 schema/behavior、
  `VALIDATED` 函数调用、`candidateCount` 大于 1、非 TEXT 输出模态返回 `400`。
  图片请改用 base64 `inlineData`，支持 PNG、JPEG、GIF、WebP。
- Gemini `topK` 与 `thinkingConfig` 只作为跨协议兼容提示接受；Chat Completions
  或 Messages 原生上游可能忽略或实现不同语义，不保证与 Gemini 原生后端的采样和
  思考行为等价。
- 其他无法保留的非空生成选项（包括 `seed`、presence/frequency penalty、logprobs
  与 media resolution）返回 `400`，不会静默丢弃。
- Responses 是无状态端点：必须设置 `store: false`。`previous_response_id`、
  `conversation`、`store: true`、`background: true` 全部直接 `400` 拒绝，不会
  静默忽略。
- Responses 支持图片 URL 与 data URL；`input_image.file_id` 返回 `400`，因为
  Gateway 没有 Files API。
- 跨协议转换无法保留约束的结构化输出与自定义工具语法会返回 `400`。
- Responses 的 `web_search`、`web_search_preview`、`tool_search` 等托管工具在
  OpenCode-Go 上无法运行；自动工具模式下会被丢弃，显式强制使用则返回 `400`。
  function、custom、namespace 工具正常转换。
- 流式 token 数量仅在上游发出 usage chunk 时准确；额度消耗使用当前 OpenCode Go
  价格快照。没有 usage 时日志记为 `success_no_usage`。
- 当前 HTTP 面板没有暴露旧的隔离 WebView 浏览器命令。
- 已安装的 Windows 桌面版可以在用户登录时把 OCG Manager 拉起到托盘；开发构建、
  macOS、Linux、CLI、Docker 不暴露面板里的 `auto_start` 开关。Docker Compose 另
  由 `restart: unless-stopped` 在 Docker daemon 重启后恢复服务。
- 不发布 Windows / Linux ARM64、32 位 x86 构建；不支持 RPM、Snap、应用商店包、
  Windows Authenticode 正式签名、Apple 公证。支持升级的已安装桌面版可在设置页
  安装签名 Release；v1.4.1、开发构建、CLI、Docker 使用直接/手动升级路径。

## 常见问题

- **托盘里点不开管理面板。**`127.0.0.1:9042` 被其他进程占用，或上一个托盘程序
  还握着单实例锁。退出占用端口的进程或上一个 release 托盘程序后重试。仅源码开
  发时可用 `scripts/free-dev-port.mjs` 清理 `30001` 上的残留 Vite 进程；它不会
  释放 `9042`，也不会释放桌面端单实例锁。
- **上游返回 `401 Unauthorized`。**OpenCode-Go 账号 Key 无效或被吊销。打开
  **账号** 视图替换 Key 再试；`key ping <id>` 是最快的验证手段。
- **本地进度条满格但请求依然成功。**这是 **假熔断**——本地估算不是上游账单。
  继续使用即可，Gateway 会继续转发。
- **本地进度条满格，Gateway 返回 `429`。**这是 **真熔断**。等 `cooldown_until`
  到期，或在 **账号** 视图手动解除冷却。
- **Gateway 返回 `429` 并提示 "all accounts cooling down"。**所有已启用账号都
  在冷却。等最近的恢复时间，或新增/启用其他账号。
- **Gemini 请求因 `safetySettings` 返回 `400`。**Gateway 不能把 Google 的安全
  阈值等价施加到 Chat/Messages 上游，因此拒绝非空数组。删除该字段后重试；不要
  假设删除后仍执行了同一套 Google 内容安全策略。
- **Docker 首次注册的 `OCG_ADMIN_PASSWORD` 没生效。**这两个变量只在数据库还没
  有管理员时生效，请使用数据库里已有的管理员账号。只有在确认备份有效且确实要
  完全重置时才重建 `ocg-data`；这样会删除全部账号、凭据和设置。
- **SmartScreen / Gatekeeper 弹窗警告。**当前 Windows 包未签名、macOS 应用使用
  ad-hoc 签名。首次启动请用 **Open Anyway** 放行，警告本身不代表篡改。

---

[English user guide](USER.md) · [Maintainer guide](MAINTAINER.md) ·
[维护者指南](MAINTAINER.zh-CN.md) · [回到 README](../README.zh-CN.md)
