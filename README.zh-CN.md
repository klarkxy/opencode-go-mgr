[English](README.md)

# OCG Manager

一个本地 Gateway：把各 Plan 的访问凭据收进一个 SQLite 数据库，在一个端口（`http://127.0.0.1:9042`）上提供五种客户端协议。机器上的 AI 工具共用路由和访问控制，不必分别维护上游配置。

每个账号归属一个 Provider/Plan（`provider_id`），并在需要时保存凭据。客户端发送本地 Alias；Gateway 负责与 Plan 上游协议之间的双向转换。内置路由涵盖 OpenCode Go、OpenCode Zen Free、Command Code GOAT、MiniMax CN Token Plan、Kimi Code CN 与 Custom API。用户定义 Provider 只能使用类型化 Configurable HTTP 配置，不加载插件代码；CPA 是可选的本机扩展。OCG Manager 没有遥测或远端同步。

## 主要特性

- **一个端口，五种线协议**：OpenAI Chat Completions、OpenAI Responses、Anthropic Messages、Gemini `generateContent` / `streamGenerateContent`，以及 Claude Desktop。
- **拖动即调序**：账号卡片持久保存一个全局顺序；严格优先、粘性、轮询都在能力过滤后复用它。
- **额度条是警告，不是墙**：本地估算不会停止流量；只有上游 `429` 才会让账号冷却。
- **桌面端、CLI、Docker**：Tauri v2 托盘应用、`ocg-manager-cli` 与 `ghcr.io/klarkxy/opencode-go-mgr` 都可用于本机运行。

## 架构总览

[![OCG Manager 单节点架构](https://klarkxy.github.io/opencode-go-mgr/diagrams/local-node.visual-check.1440x900.light.png)](https://klarkxy.github.io/opencode-go-mgr/diagrams/local-node/)

[查看全部交互式架构图与流程图](https://klarkxy.github.io/opencode-go-mgr/)。

## 下载

从 [GitHub 最新 Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest) 下载 GUI 安装包或 CLI 压缩包，并用同一 Release 的 `SHA256SUMS` 校验（PowerShell 用 `Get-FileHash <文件> -Algorithm SHA256`，macOS 用 `shasum -a 256`，Linux 用 `sha256sum`）：

| 平台 | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe`（NSIS） | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel 与 Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` 和 `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

CLI 的 `dist/` 必须与可执行文件同级，否则 `serve` 没有面板可服务。平台注意事项见[安装指南](docs/user/install.zh-CN.md)。

## 快速开始

```text
Gateway: http://127.0.0.1:9042/v1
鉴权:    Authorization: Bearer <key>
```

1. 安装并启动。Gateway 就绪后管理面板会在系统浏览器中打开；托盘图标随时唤回。
2. 在 **账号** 视图添加 Plan，并在需要时添加凭据。随后到 **访问密钥** 复制客户端 **Key**；客户端只需要这份 OCG Manager 凭据。
3. 把客户端指向 `http://127.0.0.1:9042/v1`。**应用** 视图有各客户端的配置教程。

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

安装细节、首个客户端检查、备份与升级见[用户指南](docs/USER.zh-CN.md)。

## Docker

可使用已发布镜像或从源码运行；浏览器 Sidecar、备份、HTTPS、镜像钉与 Compose 说明见 [Docker 指南](docs/user/docker.zh-CN.md)。

## 推荐协议分组

OpenCode Go 模型各有推荐上游协议。匹配且已支持的客户端协议会透传；其他已支持的客户端会转换。Gateway 不会在请求路径上试探协议。

| 推荐上游 | 分组 |
| --- | --- |
| OpenAI Chat Completions | 通用和免费 OpenCode Go 模型 |
| OpenAI Responses | 推理和贡献者模型 |
| Anthropic Messages | MiniMax 和 Qwen 模型 |

Zen Free 使用已保存的官方目录快照。Gemini 是客户端格式，不是上游目的地。完整模型、能力与转换表见[模型能力](docs/user/applications.zh-CN.md)和[协议转换](docs/user/protocol-conversion.zh-CN.md)。

## 下一步

[用户指南](docs/USER.zh-CN.md) · [维护者指南](docs/MAINTAINER.zh-CN.md) · [文档索引](docs/README.zh-CN.md) · [Contributors](docs/CONTRIBUTORS.md) · [DESIGN.md](DESIGN.md) · [AGENTS.md](AGENTS.md)

## 交流群

加入 OCG Manager QQ 群：**1104321231**。

<p align="center">
  <img src="assets/qq-group.png" alt="OCG Manager QQ 群二维码" width="360" />
</p>

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
