[English](README.md)

# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="140">
</p>

OCG Manager 是一个本地 OpenCode-Go 多账号运维控制台。它把账号 Key 保存在本地
SQLite，并通过 `http://127.0.0.1:9042` 上的多协议 Gateway 暴露给客户端——管理
面板也由同一个端口提供。客户端可以使用 OpenAI、Anthropic、Gemini 或 Claude
Desktop 协议；Gateway 会把请求转换到模型的 OpenCode-Go 原生协议，再把响应转回
客户端协议。

<p align="center">
  <img src="assets/opencode娘.png" alt="OpenCode-Go 娘" width="360">
</p>

## 主要特性

- **多协议 Gateway**：同一端口支持 OpenAI Chat Completions / Responses、
  Anthropic Messages、Gemini `generateContent` / `streamGenerateContent`、
  模型列表与 Claude Desktop 别名入口。
- **本地多账号轮询**：拖动账号卡片即可持久调整优先级；Gateway 自动跳过已禁用、
  冷却中或本次请求已失败的账号。
- **购买周期提醒**：每个账号记录购买日期，按自然月计算到期日并显示剩余天数；
  提醒不会自动禁用账号。
- **OpenCode Go 额度估算**：5 小时、本周、本月用量条按官方文档美元快照估算，
  可在设置页手动刷新。
- **16 个应用配置教程**：为 Claude Code、Claude Desktop、Codex、Gemini CLI、
  Pi、Kimi Code CLI、WorkBuddy 等 16 个客户端生成可直接复制的配置片段。
- **托盘应用与无头 CLI**：Tauri v2 托盘应用覆盖 Windows、macOS、Linux；
  `ocg-manager-cli` 适合服务器与 Docker。
- **签名桌面升级**：已安装的桌面版可在设置页检查、验签并原位安装更新。
- **无远端同步、无遥测**：每个节点独立管理自己的数据。

## 下载

从 [GitHub 最新 Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest)
下载对应平台的 GUI 安装包或 CLI 压缩包，安装前用同一 Release 的 `SHA256SUMS`
校验：PowerShell 使用 `Get-FileHash <文件> -Algorithm SHA256`，macOS 使用
`shasum -a 256 <文件>`，Linux 使用 `sha256sum <文件>`。

| 平台 | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe`（NSIS） | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel 与 Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` 和 `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

CLI 压缩包包含可执行文件、`dist/` 与 `LICENSE`；`dist/` 必须与可执行文件同级，
`serve` 才能提供管理面板。项目不做交叉编译：`pnpm run build` 只构建当前原生平台
的产物。签名升级行为、SmartScreen/Gatekeeper 提示与不支持清单（ARM64、32 位
x86、RPM、Snap、应用商店）见 [维护者指南](docs/MAINTAINER.zh-CN.md)。

## 快速开始

```text
Gateway: http://127.0.0.1:9042/v1
鉴权:    Authorization: Bearer <key>
```

`Bearer` 后面跟的是管理面板里显示的 **Gateway Key**，也是客户端唯一需要配置的
密钥；Gateway 会在上游侧注入已保存的 OpenCode-Go 账号 Key。

1. 安装并启动 OCG Manager。Gateway 就绪后管理面板会在系统浏览器中打开；之后可
   通过托盘图标重新打开。
2. 在 **账号** 视图添加 OpenCode-Go 账号，复制 Gateway Key。
3. 把客户端指向 `http://127.0.0.1:9042/v1`。**应用** 视图提供了各客户端的配置
   教程。

最小端到端验证——一次流式 Chat Completions 请求：

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

### Docker 快速启动

公开无头镜像为 `ghcr.io/klarkxy/opencode-go-mgr`（当前只发布 `linux/amd64`，
匿名即可拉取）。不需要源码时，把仓库中的
[`compose.example.yaml`](compose.example.yaml)（每个 Release 也会附带）保存为
`compose.yaml`，并按需在同目录创建 `.env`。也可以在仓库检出中直接运行：

```bash
git clone --branch v1.5.2 --depth 1 https://github.com/klarkxy/opencode-go-mgr.git
cd opencode-go-mgr
cp .env.example .env
# 编辑 .env：选择首次管理员创建方式，并把 OCG_IMAGE 固定到 1.5.2。
docker compose pull
docker compose up -d --no-build
docker compose ps
```

打开 `http://127.0.0.1:9042/dashboard/`；服务根路径 `/` 不是管理面板地址。
管理员、持久化、备份恢复、HTTPS、升级、digest/attestation 校验和本地源码构建
方法见[用户指南的 Docker 章节](docs/USER.zh-CN.md#docker)。

## 模型与协议

每个已知模型都映射到自己的原生 OpenCode-Go 协议；用其他协议访问时，Gateway 会
自动转换文本、system、图像、工具调用与结果、推理内容、完成状态、错误和 usage
字段。

| 客户端协议 | 模型 |
| --- | --- |
| OpenAI Chat Completions | `glm-5.2`、`glm-5.1`、`kimi-k2.7-code`、`kimi-k2.6`、`deepseek-v4-pro`、`deepseek-v4-flash`、`mimo-v2.5`、`mimo-v2.5-pro` |
| Anthropic Messages | `minimax-m3`、`minimax-m2.7`、`minimax-m2.5`、`qwen3.7-max`、`qwen3.7-plus`、`qwen3.6-plus` |

- **Gemini 只是客户端格式**：`/v1beta/models/{model}:generateContent` 与
  `:streamGenerateContent`（也接受 `/v1/models/...`）会转换到所选模型的原生
  协议；客户端可用 `x-goog-api-key` 鉴权。请求不会发往 Google。
- **Claude Desktop** 使用 `/claude-desktop/v1/...`，公布
  `claude-sonnet-4-6`、`claude-opus-4-6`、`claude-haiku-4-5-20251001` 三个
  别名，每个别名改写为面板中保存的模型映射。
- **未知模型** 保留请求自身的 Chat Completions 或 Messages 协议；Responses 与
  Gemini 上的未知模型、未知 Claude Desktop 别名直接 `400` 拒绝——Gateway 不会
  靠试探选协议，否则可能把同一请求重复计费。

自动重放采取保守策略：只有能证明请求尚未发出的 DNS/TCP/TLS 建连失败才在同一
账号重试一次；`401`/`403`/`429` 可切换账号；`408`、`5xx`、建连后失败、响应体
超时与流式中断一律不重放。Gemini 兼容边界（`countTokens`/`embedContent` 返回
`501`、被拒绝的字段）与完整重放规则见
[用户指南](docs/USER.zh-CN.md#限制)。

## 真熔断与假熔断

5 小时、本周、本月进度条都是 **本地估算**，不是上游的权威账单视图。

- **假熔断（本地估算）**：本地满格只是警告——Gateway **不会停用** 该账号，仍会
  继续用它发请求。
- **真熔断（上游 429）**：Gateway 解析响应中的 `Resets in …` 时间，写入
  `cooldown_until`，并切换到下一个可用账号。无法识别的 429 默认冷却 5 分钟；
  所有已启用账号都在冷却时返回 `429` 并带上最近的恢复时间。

真熔断生效时，管理面板会把对应进度条拉满并标红。账号在 `cooldown_until` 到期
后自动恢复，也可以在面板中手动解除冷却。详见
[用户指南](docs/USER.zh-CN.md#真熔断与假熔断)。

## 文档

- [中文 README](README.zh-CN.md) · [English README](README.md)
- [User guide](docs/USER.md) · [用户指南](docs/USER.zh-CN.md)
- [Maintainer guide](docs/MAINTAINER.md) · [维护者指南](docs/MAINTAINER.zh-CN.md)
- [OpenCode-Go anti-abuse statement](OPENCODE_GO_ANTI_ABUSE.md) ·
  [OpenCode-Go 防滥用声明](OPENCODE_GO_ANTI_ABUSE.zh-CN.md)
- [Contributors / 贡献者](CONTRIBUTORS.md)

## 开发模式

```bash
pnpm install
pnpm run dev
```

开发前先退出 release 托盘程序，释放单实例锁和 `9042` 端口。Tauri 会启动 Vite，
并在 Gateway 就绪后打开 `http://127.0.0.1:30001/dashboard/`；前端热更新，Rust
改动增量编译并重启进程。检查、构建与发布流水线见
[维护者指南](docs/MAINTAINER.zh-CN.md)。

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
