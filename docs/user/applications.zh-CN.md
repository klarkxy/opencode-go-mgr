[English](applications.md)

# 应用教程与模型能力

若客户端尚未收录，请阅读[新增应用](add-application.zh-CN.md)，其中包含手动 Gateway 接口、教程贡献与可选 Desktop 连接器路径。

## 应用教程

本章提供 17 个客户端的可复制配置。从 **接入中心** 复制 **Key** 与地址，再按对应教程操作。每个教程列出协议、官方文档链接、步骤和示例片段。注册表在 `src/views/application-guides.ts`。

覆盖任何现有配置文件前，请先备份原文件。

### 本机 Desktop 自动接入

自动 Desktop 连接器仍在 `src/views/Applications.vue` 与 Desktop Host 中；它们不在面板侧栏。当前用户路径是从接入中心和本章手动配置。Claude Code、Codex、Gemini CLI、OpenCode、OpenClaw 与 Hermes 使用字段级受管配置；Pi 使用客户端原生插件；DeepSeek Harness（DSH）使用 companion 插件，并由 OCG 字段级管理一个专属 `.env` 变量。Claude Desktop 只走手动教程。CLI、Docker、公共监听和远程节点同样使用手动路径。

所有连接器都支持脱敏预览和可逆操作。受管配置连接器使用字段级写入与恢复；Codex 只写用户级 `config.toml` 中 OCG 自有的 Provider 字段；Hermes 只写 `config.yaml` 中四个 `model` 字段与 `.env` 中一个专用变量。Pi 与 DSH 只通过客户端自己的包管理器安装或卸载 OCG 自有包，Key 不会写入插件源码；DSH 的所选 Key 只保存到其主目录 `.env` 的专属 `OCG_MANAGER_API_KEY` 行。

第一阶段的实机发布门槛只覆盖五个客户端：Codex、Claude Code、OpenCode、Pi 与 DSH。Gemini CLI、OpenClaw 和 Hermes 仍保留受管连接器与手动教程。

连接器先展示固定的字段级 OCG 补丁，提交前同时复核 Dashboard revision 与目标文件指纹。每个客户端只记录 OCG 实际修改字段的初始值；恢复时只撤销这些字段并保留其他编辑。如果其他程序改动了 OCG 持有的字段，系统会报告冲突，不会覆盖。

写入使用跨进程锁、同目录临时文件、替换与回读校验；多文件中途失败时会补偿，无法完全补偿则留下可恢复的“部分完成”状态。预览、错误、日志和所有权记录都不会暴露所选 Key。

损坏、链接、过大、非 UTF-8、重复键，以及连接器不支持的 JSON5/YAML 结构都会安全拒绝，并继续提供手动教程。Hermes 会逐字节保留顶层 `model` 区段之外的内容；若受管的 `model` 区段内部含注释，则会拒绝写入，而不是静默丢掉注释。接入前请关闭目标客户端，提交后再重新打开；Open Console Gateway 不会强制结束可能包含未保存会话的进程。Hermes 只管理默认原生 profile：显式 `HERMES_HOME` 优先；Windows 默认使用 `%LOCALAPPDATA%/hermes`，`~/.hermes` 仅作为旧版位置识别。检测到多个位置时会拒绝双写。

各客户端对 Base URL 都有自己的执念：

- Claude Code、Cherry Studio、Chatbox 使用不带 `/v1` 的根地址。
- Claude Desktop 使用根地址加 `/claude-desktop`，由客户端继续请求 `/claude-desktop/v1/messages` 与 `/claude-desktop/v1/models`。
- Gemini CLI 使用根地址，并设置 `GOOGLE_GENAI_API_VERSION=v1beta`。远端 Base URL 必须使用 HTTPS；只有 `localhost`、`127.0.0.1` 与 `[::1]` 可用 HTTP。非回环 HTTP 根地址不符合该客户端限制。
- Pi、Kimi Code CLI、OpenCode、OpenClaw、Hermes、Cline、Roo Code、Continue 使用带 `/v1` 的 API Base URL。
- VS Code Copilot Chat 与 WorkBuddy 使用完整 `/v1/chat/completions` 端点。Codex 使用带 `/v1` 的 API Base URL，且必须 `wire_api = "responses"`。CLI 用 `~/.codex/ocg.config.toml` + `codex --profile ocg`，Desktop 或常驻默认则合并进用户级 `~/.codex/config.toml`。`~/.codex/ocg-model-catalog.json` 可选：不写也能请求；只有需要选择器、真实上下文窗口和推理档位时才启用 `model_catalog_json`。启用后会整份替换 Codex 内置目录，且必须包含当前必填字段。不写 catalog 时，未知 slug 按 Codex 的 272K 回退元数据。请求始终走 Open Console Gateway 的 Responses 入口。

Codex 连接器会尊重 `CODEX_HOME`，否则使用 `~/.codex/config.toml`。它只接管根级 `model`、根级 `model_provider` 与 `model_providers.ocg_manager` 表，绝不读取或修改 `~/.codex/auth.json`、`openai_base_url`、其他 Provider、MCP、权限或模型目录。所选 Key 会作为 Codex 的 `experimental_bearer_token` 保存在 OCG 自有 Provider 表中，因此该文件必须只允许当前系统用户读取。与其他配置切换器混用前请单独备份。

Hermes 把所选 Key 仅保存到 `.env` 的 `OCG_MANAGER_API_KEY`，并由 `model.api_key` 引用；其他 YAML 区段与环境变量不在连接器所有权内。命名 profile 与容器卷不会被推断或修改。

Pi 通过自己的包管理器安装 `ocg-manager-pi`，并通过 Provider 原生登录保存所选 Key。DSH 把 `ocg-manager-dsh` 作为 OCG 自有的 companion 插件安装到 `web` profile；插件只注册固定路由，OCG 仅字段级管理 DSH 主目录 `.env` 中的 `OCG_MANAGER_API_KEY`，保留其他行，并在卸载时恢复原值。基础 profile 或其他 bundle 已注册的 Provider 不会被覆盖。第一阶段不会自动安装到 TUI、headless 或自定义 profile。卸载只移除 OCG 自有包和上述专属变量写入。

选择器列表来自受保护的 `GET /dashboard/api/v3/application-models`：当前可路由的 OpenCode Go 别名与当前价格快照求交。highspeed 变体继承基价行。空交集是 `[]`。带鉴权的 `GET /v1/models` 公布代码持有且当前可路由的 Go 与密封供应商 Alias，以及合格 Custom 声明 ID；保存的 Zen `-free` 行会公布去掉后缀后的 Alias，Command 目录可以加入任一代码持有的 Alias，保存的 MiniMax/Kimi 行只激活精确密封映射。两条路径都是本地读取，只返回当前可路由且协议有效启用的模型。目录刷新是 **供应商** 页上的显式动作；价格刷新后，这里的 Go 别名可能变化。

## 模型能力

下面这张限额表来自 `src/views/application-guides.ts`（2026-08-14），已核对过。输入列是 OCG 实际能带上的模态。透传 / 转换矩阵见 [协议转换](protocol-conversion.zh-CN.md#协议转换)。

| 模型 | 上下文 | 输出 | 输入 | 推理 | 工具 | 力度 |
| --- | ---: | ---: | --- | --- | :---: | --- |
| `grok-4.5` | 500K | 500K | 文本、图像 | 始终 | ✓ | low / medium / high（默认 high） |
| `gpt-5.6-luna` | 1.05M | 128K | 文本、图像 | ✓ | ✓ | low / medium / high / max（默认 medium） |
| `muse-spark-1.2` | 1M | 128K | 文本、图像 | ✓ | ✓ | low / medium / high（默认 high） |
| `muse-spark-1.2-contributor` | 1M | 128K | 文本、图像 | ✓ | ✓ | low / medium / high（默认 high） |
| `glm-5.3` | 1M | 128K | 文本 | ✓ | ✓ | low / high / max（默认 max） |
| `glm-5.2` | 1M | 128K | 文本 | ✓ | ✓ | high / max（默认 max） |
| `glm-5.1` | 198K | 32K | 文本 | ✓ | ✓ | — |
| `kimi-k3` | 1M | 128K | 文本、图像、视频 | 始终 | ✓ | max |
| `kimi-k2.7-code` | 256K | 256K | 文本、图像、视频 | 始终 | ✓ | — |
| `kimi-k2.6` | 256K | 64K | 文本、图像、视频 | ✓ | ✓ | — |
| `mimo-v2.5` | 1M | 128K | 文本、图像、音频、视频 | ✓ | ✓ | — |
| `mimo-v2.5-pro` | 1M | 128K | 文本 | ✓ | ✓ | — |
| `minimax-m3` | 1M | 128K | 文本、图像 | ✓ | ✓ | — |
| `minimax-m2.7` | 200K | 128K | 文本 | 始终 | ✓ | — |
| `minimax-m2.7-highspeed` | 200K | 128K | 文本 | 始终 | ✓ | — |
| `minimax-m2.5` | 200K | 64K | 文本 | 始终 | ✓ | — |
| `minimax-m2.5-highspeed` | 200K | 64K | 文本 | 始终 | ✓ | — |
| `qwen3.8-max` | 1M | 128K | 文本 | ✓ | ✓ | — |
| `qwen3.7-max` | 1M | 64K | 文本 | ✓ | ✓ | — |
| `qwen3.7-plus` | 1M | 64K | 文本、图像 | ✓ | ✓ | — |
| `qwen3.6-plus` | 1M | 64K | 文本、图像 | ✓ | ✓ | — |
| `deepseek-v4-pro` | 1M | 384K | 文本 | ✓ | ✓ | high / max（默认 high） |
| `deepseek-v4-flash` | 1M | 384K | 文本 | ✓ | ✓ | high / max（默认 high） |
| `hy3` | 256K | 64K | 文本 | ✓ | ✓ | low / high（默认 high） |

`muse-spark-1.2` 使用零数据保留（ZDR）：提示词和补全内容不会用于训练。 `muse-spark-1.2-contributor` 不使用 ZDR；提示词和补全内容可能用于训练。仅在你有权这样使用的数据上选择 Contributor。Muse 标准价格来自实时 Go 用量测量，因为公开 Go 价格表只列出 Contributor。

显示取整：198K = 202,752；200K = 204,800；256K = 262,144；1M = 1,000,000 或 1,048,576。`glm-5.3` 的限额与 token 单价已按 models.dev `opencode-go/glm-5.3` 独立行对齐（$1.40 / $4.40 / $0.26，与官方 Go 表一致；Usage 仍为 $15）。

Claude Desktop 是例外，它的模型映射是持久化的：复制配置前，选中的 `sonnet`、 `opus`、`haiku` 目标模型会通过受保护的 `GET/PUT /dashboard/api/v3/claude-desktop/models` 保存到 SQLite。留空的角色回退到第一个已配置模型，三个角色不能同时为空。它的恢复操作回到当前页面已加载或最后保存的映射。

---

[用户指南索引](../USER.zh-CN.md) · [English](applications.md) · [文档索引](../README.zh-CN.md)
