# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="140">
</p>

OCG Manager 是一个本地 OpenCode-Go 多账号管理器，并提供 OpenAI 兼容 Gateway。它本地保存 Key，通过 Gateway 托管管理面板，并由 Windows、macOS 或 Linux 托盘应用常驻后台。

<p align="center">
  <img src="assets/opencode娘.png" alt="OpenCode-Go 娘" width="360">
</p>

## 快速开始

```text
Gateway: http://127.0.0.1:9042/v1
鉴权:    Authorization: Bearer <gateway-key>
```

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

## 真熔断与假熔断

5 小时、每周和每月用量条由本地转发记录估算。本地用量达到限额属于假熔断：本地计费口径或刷新时间可能与上游不同，因此即使进度条已经满额，Gateway 也会继续使用该账号发送请求，不会写入冷却状态。本地满额只表示警告，不能证明上游已经限制账号。

只有上游返回 HTTP 429 才会触发真熔断。Gateway 会保存上游错误，根据响应中的重置时间写入 `cooldown_until`，并切换到下一个可用账号。已知的 5 小时、每周和每月限额提示会采用上游给出的重置时长；无法识别重置时间的 429 默认冷却 5 分钟。如果所有已启用账号都在冷却，Gateway 会返回 429，并给出最早的恢复时间。

真熔断后，管理面板会把对应的 5 小时、每周或每月进度条拉满并标红，即使本地估算值更低。账号会在 `cooldown_until` 到期后自动恢复，也可以在管理面板中手动解除冷却。

## 文档

- [User guide](docs/USER.md)
- [Maintainer guide](docs/MAINTAINER.md)
- [OpenCode-Go 防滥用声明](OPENCODE_GO_ANTI_ABUSE.zh-CN.md)
- [English README](README.md)

## 开发模式

首次开发前运行 `pnpm install` 安装依赖。

只修改 Vue、CSS 或前端 TypeScript 时，保持 Gateway 或 release 程序在 `9042` 端口运行，然后启动 Vite：

```bash
pnpm run dev
```

打开 `http://127.0.0.1:30001/dashboard/`。Vite 会热更新前端，不需要重新编译 Rust，也不需要打 release。`9042` 端口上的管理面板来自已构建文件，不会热更新。

修改 `crates/` 或 `src-tauri/` 下的 Rust 代码时，先退出正在运行的 release 托盘程序，再启动 Tauri 开发模式：

```bash
pnpm run dev:gui
```

Tauri 会监听 Rust workspace，执行增量编译并自动重启进程；这属于开发时重载，不是真正的运行时代码替换。只有最终验收和发版时才运行 `pnpm run build`。

常用检查：

```bash
pnpm run test
pnpm run build:web
pnpm run design:lint
pnpm run build
```

## 发布产物

`pnpm run build` 会为当前受支持的原生平台构建 GUI 和 CLI，并原子替换 `release/`；不会在一台机器上交叉构建全部平台。

| 平台 | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe` | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel 与 Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` 和 `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

每次构建还会生成 `SHA256SUMS`。CLI 压缩包包含可执行文件、`dist/` 和 `LICENSE`；必须让 `dist/` 与可执行文件保持同级，`serve` 才能提供管理面板。

Windows GUI 只发布安装包，不发布便携版。首轮 Windows 安装包不签名，macOS 使用 ad-hoc 签名，Linux 通过 SHA-256 校验；Windows SmartScreen 可能警告，macOS 可能要求在“隐私与安全性”中手动允许。当前不支持 Windows/Linux ARM64、32 位 x86、RPM、Snap、应用商店和自动更新。

## 许可证

见 [LICENSE](LICENSE)。
