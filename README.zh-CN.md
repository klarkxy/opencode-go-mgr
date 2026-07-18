[English](README.md)

# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="140">
</p>

OCG Manager 是一个本地 OpenCode‑Go 多账号运维控制台。它把 OpenCode‑Go 账号
Key 保存在本地 SQLite，并通过 OpenAI、Anthropic 与 Gemini 兼容 Gateway 暴露
给客户端使用；管理面板也由这个 Gateway 提供。桌面端是 Tauri v2 托盘应用，
覆盖 Windows、macOS、Linux；同步发布无头 CLI。

<p align="center">
  <img src="assets/opencode娘.png" alt="OpenCode-Go 娘" width="360">
</p>

## 主要特性

- **OpenAI / Anthropic / Gemini 兼容 Gateway**：同一端口同时支持
  `POST /v1/chat/completions`、`POST /v1/responses`、`POST /v1/messages`、
  `GET /v1/models`、Claude Desktop 专用的 `/claude-desktop/v1/*`，以及 Gemini
  `/v1beta/models/{model}:generateContent` / `:streamGenerateContent`。Gateway
  按模型原生协议转发到 OpenCode‑Go，并把响应转换回客户端协议。
- **可控的本地多账号轮询**：在账号页拖动卡片即可持久调整优先级；Gateway
  按该列表顺序尝试，并自动跳过已禁用、冷却中或本次请求已失败的账号。
- **购买周期提醒**：每个账号记录购买日期，按下一个自然月同日计算到期日，
  在账号页和仪表盘直观显示剩余天数；提醒不会自动禁用账号。
- **OpenCode Go 额度估算**：5 小时、本周、本月进度条按当前 OpenCode Go
  官方文档快照中的美元口径估算。设置页只在用户主动刷新时拉取文档；刷新失败
  会继续使用最后一次成功的 revision。
- **首次启动自动建管理员**：非回环监听时首位访客创建唯一管理员；桌面版
  与 CLI 默认绑定回环地址，自动跳过登录。
- **跨平台托盘应用**：Windows 安装包、macOS Universal DMG、Linux
  AppImage 与 `.deb` 共用同一份 Tauri v2 代码与单实例锁。
- **同步发布无头 CLI**：`ocg-manager-cli` 自带管理面板 `dist/`，适合服务
  器、Docker、远程 Gateway。
- **13 个应用配置教程**：面板可直接生成 Claude Code、Claude Desktop、
  Codex、Gemini CLI、OpenCode、OpenClaw、Hermes、Cherry Studio、VS Code
  Copilot Chat、Cline、Roo Code、Continue 与 Chatbox 的配置片段。
- **手动检查更新**：设置页可检查 GitHub 最新 Release 并打开发布页，不会
  自动下载或安装。
- **无远端同步、无遥测**：每个节点独立管理自己的数据；不提供云服务、
  Admin API 或跨节点同步。

## 下载

从 [GitHub 最新 Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest)
下载对应平台的 GUI 安装包或 CLI 压缩包。安装前同时下载该 Release 中的
`SHA256SUMS`，并将产物的 SHA-256 与对应条目比较：PowerShell 使用
`Get-FileHash <文件> -Algorithm SHA256`，macOS 使用
`shasum -a 256 <文件>`，Linux 使用 `sha256sum <文件>`。

### Docker 快速启动

公开无头镜像为 `ghcr.io/klarkxy/opencode-go-mgr`，匿名即可拉取；当前发布
目标为 `linux/amd64`。如果不需要源码，可使用仓库中的
[`compose.example.yaml`](compose.example.yaml)（每个 Release 也会附带），保存为
`compose.yaml`，并按需在同目录创建 `.env`。

```bash
git clone --branch v1.4.2 --depth 1 https://github.com/klarkxy/opencode-go-mgr.git
cd opencode-go-mgr
cp .env.example .env
# 编辑 .env：选择首次管理员创建方式，并把 OCG_IMAGE 固定到 1.4.2。
docker compose pull
docker compose up -d --no-build
docker compose ps
```

打开 `http://127.0.0.1:9042/dashboard/`；服务根路径 `/` 不是管理面板地址。
管理员、持久化、备份恢复、HTTPS、升级、digest/attestation 校验和本地源码构建
方法见[用户指南的 Docker 章节](docs/USER.zh-CN.md#docker)。

## 快速开始

默认 Gateway 地址和本地鉴权头：

```text
Gateway: http://127.0.0.1:9042/v1
鉴权:    Authorization: Bearer <key>
```

`Bearer` 后面跟的是管理面板里显示的 **Gateway Key**，也是客户端唯一需要
配置的密钥；Gateway 内部会使用管理面板里已存好的 OpenCode‑Go 账号 Key
向上游发送请求。

最小端到端验证——向示例模型发起一次流式 Chat Completions 请求：

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

桌面应用正常启动时，会在 Gateway 就绪后自动用系统浏览器打开管理面板。
添加 OpenCode‑Go 账号、复制 Gateway Key，然后在 OpenAI 兼容客户端中填写
`http://127.0.0.1:9042/v1`。如果浏览器没有自动打开，或管理面板标签页已关
闭，可通过托盘图标重新打开。

Gateway 对自动重放采取保守策略：只有能证明请求尚未发出的 DNS/TCP/TLS 建连
失败，才会在同一账号重试一次；`401`、`403`、`429` 可以切换账号；`408`、
`5xx`、建连后的传输失败、响应体超时和流式中断均不重放，因为上游可能已经消
耗额度。无法确认结果时返回 `upstream_outcome_unknown`。连接、非流式总时限、
流式空闲时限默认分别为 30、900、300 秒。

## 真熔断与假熔断

5 小时、本周、本月进度条都是 **基于 Gateway 实际转发的请求做的本地估算**，
不是上游的权威账单视图。

- **假熔断（本地估算）**：本地估算到顶时，Gateway **不会停用** 该账号，
  仍会继续使用它发请求。本地计费口径与上游账单/刷新边界可能不同，因
  此本地满格只是警告，不能证明上游已经封禁该账号。
- **真熔断（上游 429）**：Gateway 会记录上游错误，解析响应中的
  `Resets in …` 时间，写入 `cooldown_until`，并切换到下一个可用账号。已
  知的 5 小时、本周、本月限额消息采用上游给出的重置时长；无法识别的
  429 默认冷却 5 分钟。如果所有已启用账号都在冷却，Gateway 会返回
  `429`，并带上最近的恢复时间。

真熔断生效时，管理面板会把对应的 5 小时、本周或本月进度条拉满并标红，
即使本地估算值更低。账号在 `cooldown_until` 到期后自动恢复，也可以在
管理面板中手动解除冷却。

## 模型与协议

每个已知模型都映射到自己的原生 OpenCode‑Go 协议；客户端可通过 Chat
Completions、Responses、Messages 或 Gemini generateContent 访问，Gateway 会
自动转换文本、system、图像、工具调用与结果、完成状态、错误和 usage 字段。
Claude Desktop 使用 `/claude-desktop/v1/messages`，把 Sonnet、Opus、Haiku
别名改写为面板中保存的实际模型。

- **OpenAI Chat Completions**：`glm-5.2`、`glm-5.1`、`kimi-k2.7-code`、
  `kimi-k2.6`、`deepseek-v4-pro`、`deepseek-v4-flash`、`mimo-v2.5`、
  `mimo-v2.5-pro`。
- **Anthropic Messages**：`minimax-m3`、`minimax-m2.7`、`minimax-m2.5`、
  `qwen3.7-max`、`qwen3.7-plus`、`qwen3.6-plus`。

未知模型保留请求自身的 Chat Completions 或 Messages 协议。Responses 与 Gemini
端点的未知模型会直接 `400` 拒绝——Gateway 不会靠试探选协议，否则可能把同
一请求重复计费。Gemini 是客户端兼容层，不会把请求转发到 Google：非空
`safetySettings` 因无法跨协议保真而返回 `400`；只有 `topK` 与
`thinkingConfig` 作为兼容提示接受且不保证与实际 Chat/Messages 上游语义等价，
其他未明确映射的非空 `generationConfig` 字段返回 `400`。完整限制见
[用户指南的协议转换章节](docs/USER.zh-CN.md#协议转换)。

## 文档

- [中文 README](README.zh-CN.md) · [English README](README.md)
- [User guide](docs/USER.md) · [用户指南](docs/USER.zh-CN.md)
- [Maintainer guide](docs/MAINTAINER.md) ·
  [维护者指南](docs/MAINTAINER.zh-CN.md)
- [OpenCode‑Go anti‑abuse statement](OPENCODE_GO_ANTI_ABUSE.md) ·
  [OpenCode‑Go 防滥用声明](OPENCODE_GO_ANTI_ABUSE.zh-CN.md)
- [Contributors / 贡献者](CONTRIBUTORS.md)

## 开发模式

```bash
pnpm install
pnpm run dev
```

开发前先退出 release 托盘程序，释放单实例锁和 `9042` 端口。Tauri 会启动
Vite，在 Gateway 就绪后打开 `http://127.0.0.1:30001/dashboard/`。Vue、
CSS 与前端 TypeScript 由 Vite 热更新，Rust 改动走 Cargo 增量编译并重启
进程。检查、构建、发版验证与平台覆盖见
[维护者指南](docs/MAINTAINER.zh-CN.md)。

## 发布产物

`pnpm run build` 会为 **当前受支持的原生平台** 构建 GUI 与 CLI，并原子替
换 `release/`；不在一台机器上交叉构建全部平台。

| 平台 | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe`（NSIS） | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel 与 Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` 和 `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

CLI 压缩包包含可执行文件、`dist/` 与 `LICENSE`，`dist/` 必须与可执行文
件同级，`serve` 才能提供管理面板。`SHA256SUMS`、签名与 SmartScreen/
Gatekeeper 提示，以及不支持清单（ARM64、32 位 x86、RPM、Snap、应用商店、
自动下载/安装更新）见 [维护者指南](docs/MAINTAINER.zh-CN.md)。设置页可手动
检查新版本。

## 许可证

见 [LICENSE](LICENSE)。

## Star 历史

<a href="https://www.star-history.com/?type=date&repos=klarkxy%2Fopencode-go-mgr">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&theme=dark&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
 </picture>
</a>
