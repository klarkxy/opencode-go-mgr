[English](release-artifacts.md)

# 发布产物

发布矩阵只保留三个桌面平台与一份多架构容器镜像。

| Runner | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | NSIS 当前用户安装包 | x64 ZIP |
| macOS 11+ | Universal DMG（x64 + ARM64） | Universal tar.gz |
| Linux x64 | AppImage + deb | x64 tar.gz |

稳定的产物命名：

```text
ocg-manager_<version>_windows-x64-setup.exe
ocg-manager_<version>_windows-x64-setup.exe.sig
ocg-manager-cli_<version>_windows-x64.zip
ocg-manager_<version>_macos-universal.dmg
ocg-manager_<version>_macos-universal.app.tar.gz
ocg-manager_<version>_macos-universal.app.tar.gz.sig
ocg-manager-cli_<version>_macos-universal.tar.gz
ocg-manager_<version>_linux-x64.AppImage
ocg-manager_<version>_linux-x64.AppImage.sig
ocg-manager_<version>_linux-x64.deb
ocg-manager_<version>_linux-x64.deb.sig
ocg-manager-cli_<version>_linux-x64.tar.gz
compose.example.yaml
cpa-config.example.yaml
latest.json
SHA256SUMS
```

每个 CLI 压缩包包含可执行文件、`dist/` 和 `LICENSE`；必须整体分发，`serve` 依赖同级的 `dist/`。Windows 没有便携 GUI 安装包。

`linux/amd64` 与 `linux/arm64` 容器单独发布为 `ghcr.io/klarkxy/opencode-go-mgr`。GitHub Release 包含七份平台 payload、macOS 升级压缩包、四份升级签名、Compose 与 CPA 配置示例、`latest.json` 和 `SHA256SUMS`，当前共 16 个附件。本地验证器和工作流都要求 GitHub 附件的名称与数量同组装后的 `release/` 目录完全一致。运行镜像中的许可证位于 `/usr/share/licenses/ocg-manager/LICENSE`。

## scripts/release.mjs

`scripts/release.mjs` 负责构建并暂存 `release/` 目录：

1. 校验 `package.json`、`src-tauri/tauri.conf.json`、workspace `Cargo.toml`、 `src-tauri/Cargo.toml`，以及 `compose.example.yaml` 的三个带版本字段（标题、主镜像和浏览器镜像默认值）一致；如有 Git tag，与之比对。
2. 在创建暂存目录前解析升级签名模式；设置 `OCG_REQUIRE_UPDATER_ARTIFACTS=1` 时，缺私钥或 `TAURI_UPDATER_PUBLIC_KEY` 都会在替换 `release/` 前失败；配置的公钥还必须匹配 `src-tauri/updater-public-key.sha256` 中已提交的 SHA-256 连续性基线。
3. 配置签名密钥时，合并 `src-tauri/tauri.updater.conf.json` 和临时公钥配置，启用 Tauri 升级产物。`TAURI_SIGNING_PRIVATE_KEY` 可直接填写私钥内容或仓库外的安全路径，不另设 path 变量。没有签名密钥时保持普通本地构建，并明确提示该结果只适合冒烟，不是可发布的升级版本。
4. 拒绝不支持的 host/arch 组合（`process.platform`/`process.arch`）。
5. 用绝对 bundle 路径调用 `@tauri-apps/cli`：Windows 走 `nsis`，Linux 走 `appimage,deb`。macOS 普通本地构建走 `--target universal-apple-darwin --bundles dmg`；启用升级签名时走 `--bundles app,dmg`，因为 Tauri 只有在构建 `app` target 时才会生成升级压缩包。
6. 每份 payload/签名在暂存前都使用实际 `TAURI_UPDATER_PUBLIC_KEY` 做密码学验证，再收集 NSIS、AppImage 签名与 macOS `.app.tar.gz`/签名；deb 不是 Tauri 原生升级产物，因此显式执行 `tauri signer sign`。公私钥即使都非空但不匹配，也会 fail closed。
7. 构建 CLI 二进制，与 `dist/`、`LICENSE` 一起打成对应平台的压缩包；macOS 上用 `lipo` + `codesign -` 拼出 universal CLI。
8. 对暂存 `release/` 目录内的每份 payload 与签名写 `SHA256SUMS`。
9. 原子替换 `release/`。任意步骤失败，旧 `release/` 保留，暂存目录清理。

`scripts/release.mjs` 不触碰 Cargo 增量编译缓存，多次构建复用同一 `target/`。

`pnpm run release:check` 校验版本、Compose 与已配置签名密钥，不构建原生安装包。无密钥预检覆盖未签名契约；生产 tag push 时，每台 runner 先用仓库签名密钥对临时 payload 签名，再用已通过连续性检查的 `TAURI_UPDATER_PUBLIC_KEY` 验证，之后才开始昂贵的原生构建。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](release-artifacts.md) · [文档索引](../README.zh-CN.md)
