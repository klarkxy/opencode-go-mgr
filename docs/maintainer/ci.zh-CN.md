[English](ci.md)

# CI 工作流

## quality.yml —— 可复用质量门

`.github/workflows/quality.yml` 会直接在 pull request 与 `main` push 上运行，也可通过
`workflow_call` 复用。`release.yml` 只在生产 tag 发布时调用它；手动候选构建跳过。
门禁由三条并行 job 组成，前端失败不必等 Rust，Windows 也不必重做 dashboard 构建：

- **Web** —— `pnpm run contract:v3:check`、`pnpm run typecheck`、`pnpm run test:web`（只跑 `src/**/*.test.ts`）、Vite 生产构建、`DESIGN.md` lint，以及 `docker compose -f compose.example.yaml config --quiet`。发版工具测试有意单独放在 `pnpm run test:tooling`。
- **Rust** —— `cargo fmt`、锁定依赖的 workspace 测试与 Clippy。桌面 crate 被排除（`--exclude ocg-manager`）：只有它需要 WebKit 头文件和占位 `dist/index.html`，而 Windows job 已经覆盖它，所以这个 leg 不安装任何系统包。`src-tauri` 的 Linux 编译覆盖由 release 构建矩阵承担。
- **Windows Tauri** —— 对 `ocg-manager` 跑 `cargo test --lib`/`clippy`，用占位 `dist/index.html` 满足 tauri-build。这是质量门中唯一编译桌面 crate 的 job，同时覆盖 Windows 专属自动启动，不装 pnpm 也不跑 Vite。

兼容的运行共享 Node/pnpm 和 Rust 构建缓存。PR 只恢复 Rust 缓存，不写回；非 PR
失败时仍会写回 Rust 缓存，方便后续修复复用编译结果。

## release.yml —— 候选与 tag 发布

`.github/workflows/release.yml` 由 `workflow_dispatch` 和 `v*` tag 触发。

- 手动候选可选 Windows x64、macOS Universal、Linux x64 或全部平台，刻意只生成未签名冒烟产物；即使手动运行选择 tag 作为 ref，也不会获得生产签名权限。
- 只有 `v*` tag 的 `push` 事件才会强制走完整三平台矩阵并注入 repository signing secrets。对这个单维护者仓库，推送该 tag 就是明确的公开发布授权。
- 生产 tag push 时，质量门与 Ubuntu 预检并行：预检在 `pwsh` 下解析抽出的安装器冒烟脚本、运行发布辅助测试、校验所有版本清单，并在任何原生 runner 启动前验证签名公私钥和已提交公钥指纹。手动候选跳过质量门，预检中得到空签名值。

预检通过后，每个选中的原生 runner 恢复对应 Rust 缓存并安装依赖。工作流只在 plan
确认事件是真实 `v*` tag push 时才注入签名 secrets；手动 job 得到空签名值，只执
行普通未签名构建。两条路径都会运行 CLI/GUI 冒烟，并上传保留 7 天的
`release-<platform>`。通用测试、类型和 lint 不在三台 runner 上重复执行。

## 各 runner 的冒烟流程

- **Windows CLI**——校验 `SHA256SUMS`，解压 ZIP，对临时 data dir 跑 `key add` / `key list` / `key disable` / `key enable` / `status` / `key remove`，启动 `serve --port=19042` 后等 dashboard HTML 中出现 `id="app"`。
- **macOS / Linux CLI**——同样的 `key` 与 `serve` 流程；macOS 上额外用 `lipo -archs` 校验 universal 二进制。
- **Windows GUI**——下载当前已发布安装包，静默安装并启动，写入数据哨兵并启用 `auto_start`；不卸载旧版，直接用 `/UPDATE /P /R /ARGS --startup` 运行候选 NSIS，确认旧 PID 退出、`/settings/update-status` 返回候选版本、哨兵与 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OCG Manager` 都保留。安装器进程有显式超时，并与 `/R` 拉起的常驻 GUI 分开等待，避免成功重启反而卡住 CI；卸载完成也有时间上限，并通过已安装文件消失等后置条件判断。随后继续自启关闭/恢复检查，静默卸载并确认用户数据仍在。PowerShell 实现在 `scripts/smoke-windows-release.ps1`，不再内嵌在 YAML。手动触发且候选版本已经是 latest 时，可走仅安装候选版的路径。
- **macOS GUI**——挂载 DMG，`codesign --verify --deep --strict`， `lipo -archs` 校验 universal，`--startup` 启动后等 dashboard。
- **Linux GUI**——`dpkg-deb --info` / `dpkg-deb --contents` 校验 deb，`file` 校验 AppImage；用 `dbus-run-session -- xvfb-run -a env APPIMAGE_EXTRACT_AND_RUN=1 WEBKIT_DISABLE_COMPOSITING_MODE=1` 启动后等 dashboard。

`scripts/smoke-windows-release.ps1` 对已发布基线和候选版均使用 Dashboard V3。
自启写入先从
`GET /dashboard/api/v3/settings` 取得实时 `revision` / `processGeneration`，再发送带
CAS 的 V3 `PUT`。

## draft-release 与 verify-release

`v*` tag 触发时，下游 `draft-release` job 下载三个 runner 的 Actions artifact，
把平台 payload、签名、`compose.example.yaml` 与 `cpa-config.example.yaml` 组装进
`release/`，生成使用不可变
tag URL 和 bundle 感知平台键的 `latest.json`，再重写覆盖 manifest、签名和其余附
件的 `SHA256SUMS`，最后创建或更新 **draft** GitHub Release。

`verify-release` 要求 GitHub 附件名称与组装后的 `release/` 集合逐名一致。本地验
证器还固定校验当前 16 个文件，再重新推导 `latest.json`、重算全部 checksum、验
证四份升级签名，并把每个下载文件与 GitHub Release 存储层报告的 digest 对比。

draft job 会把数字 Release ID 传给下游；验证和公开 job 都重新校验该 ID、tag 与
draft 状态，因为 tag 查询端点无法显示 draft Release。

`v1.5.8-beta.1` 这类 SemVer 预发布 tag 走同一条签名 tag 路径，并保持相同的不可
变附件集合。升级 manifest 在 payload 文件名和下载 URL 中保留完整预发布后缀；
Windows 安装包冒烟也接受同一个预发布 `CandidateVersion`。

自动生成的说明开头标注托管账号注册与隔离浏览器 Profile 为 Beta、尚未充分测试，
同时列出尚未实测的 Google/OpenCode 真实注册与支付、noVNC 键盘/剪贴板、GHCR 首次
公开发布路径，并说明 preview 包含 Gateway、脱敏和发布链路改动，不能视为生产可用。
生成稳定版说明时跳过同版本预发布 tag，以前一个稳定版为基线，避免完整功能范围被
Beta tag 隐藏。

## publish-release —— 只公开已验证的 tag 构建

`v*` tag push 是单维护者的发版授权，因此 `verify-release` 成功后会自动运行
`publish-release`。发布 job 会比对当前资产/digest 集合指纹与已验证指纹；验证后
draft 有任何变化都会拒绝发布。手动候选无法进入 draft、验证或公开发布 job。缺少
签名密钥、冒烟失败或验证失败时，Release 都不会公开。

发布 job 进入仓库级 `release-moving-channels` 串行队列；正式公开前会比较候选版
本和当前 GitHub latest，只允许严格更高的稳定 SemVer 推进 `latest`。延迟完成的旧
run 仍可公开自己的不可变 Release，但不能把移动通道回滚。

预发布 tag 的 draft 与最终 Release 都设为 `prerelease=true`，固定
`make_latest=false`，且跳过只适用于稳定版的 latest 比较；稳定 tag 的行为不变。

## 升级签名密钥

生产升级密钥只在可信工作站生成一次，并写到仓库外的安全路径：

```powershell
node node_modules/@tauri-apps/cli/tauri.js signer generate -w <secure-path-outside-repository>/ocg-updater.key
```

- 私钥内容与密码分别保存为 repository Actions secrets `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。发布工作流只会在事件派生的 plan 确认真实 `v*` tag push 时引用它们；手动候选得到空值并保持未签名。
- Repository secrets 不具备 Environment 隔离；如果以后增加有写权限的维护者，应在下一次发版前重新评估受保护签名 Environment 或 tag ruleset。
- 私钥和密码都至少保留两份独立存放的加密备份。它们一旦丢失，已经信任对应公钥的客户端就无法再走应用内升级，只能重新直接安装引导版本。
- 公钥可安全分享；本项目通过 repository Actions variable `TAURI_UPDATER_PUBLIC_KEY` 注入其内容，而不提交到仓库。GitHub 中保存的是生成后的密钥内容，不是本地文件路径。
- 升级签名证明 payload 由本项目发布，但不等同于操作系统代码签名。

## 密钥连续性与轮换

`src-tauri/updater-public-key.sha256` 是生产信任连续性的已提交锚点。正常 CI 没有
绕过开关：repository variable 不匹配时，签名预检和 Release 验证都会 fail closed。
密钥轮换属于 break-glass 恢复，不是普通 secret 更新。必须先生成并备份新密钥、
为所有既有客户端准备直接安装引导，再在明确的安全审查变更中同时更新 variable 与
已提交指纹；单独更新其中一项无法让旧安装版信任新密钥签出的版本。

## container.yml —— 镜像流水线

`.github/workflows/container.yml` 接受 Release 发布事件，但由 `release.yml` 使用
`github.token` 公开的 Release 不会递归启动另一个工作流。签名 tag 流水线公开
Release 后，稳定版必须对该 tag 显式触发 `container.yml`，并设置
`publish_latest=true`。

该工作流检出 Release tag，在各架构原生 runner 上构建：amd64 用 `ubuntu-24.04`，
arm64 用 `ubuntu-24.04-arm`；发布产物不经 QEMU 模拟。只有 amd64 leg 通过
`docker-bake.hcl` 构建冒烟镜像并运行主服务
`ghcr.io/klarkxy/opencode-go-mgr` 与 Sidecar
`ghcr.io/klarkxy/opencode-go-mgr-browser` 的冒烟套件。主镜像冒烟检查 Dashboard、
鉴权和许可证；浏览器镜像在只读根文件系统、零 capability、Chromium 可用的 seccomp
配置、无宿主机端口下启动 Xvfb/noVNC，并通过受 token 保护的控制 API 拉起普通
Chromium 与持久 Profile。arm64 leg 只构建并推送两个镜像，不跑冒烟。

全部验证通过的产物——每架构两个镜像——先按 digest 推送而不分配可变名称，再进入
仓库级串行标签队列。只有 `resolve` 可以解析请求 tag 或可选 `source_ref`；两个原
生架构 build leg 都检出它输出的完整 commit SHA，并断言 `HEAD` 必须相同。
publish job 使用不可变的 `github.workflow_sha`，确保带 registry 写权限的 helper
来自已审查的 workflow 定义，而不是热修 ref 中的可执行文件。

写入用户可见标签前，publish job 先用 `docker buildx imagetools create --dry-run`
在本地组装两个候选 OCI index，对返回的 JSON 求 digest，并校验两个架构 child 及
index 的 version/revision 注解。主镜像与浏览器镜像的 `X.Y.Z` 和
`sha-<12 位 commit>` 标签都要以本地已知 digest 完成预检，之后才按浏览器在先、
主镜像在后的顺序创建并验证；已存在标签只有 digest 与候选完全相同时才接受。

随后工作流必须用空 Docker 凭据目录匿名拉取两个精确版本标签，并为两个最终 index
digest 成功写入 GitHub 签名 provenance。只有这些步骤全绿，同一个串行 job 才会
重新读取并预检远端移动通道。稳定版 `X.Y` 和选择更新的 `latest` 要么让两个镜像都
收敛到候选版本，要么保留已经对齐的较新版本对；推进时浏览器在先、主镜像在后，若
仍会分裂则 fail closed。每个架构镜像还记录 SPDX SBOM 与 BuildKit SLSA provenance。
`X.Y.Z` 和 `sha-*` 是不可变发布标签；`X.Y` 与 `latest` 是单调移动通道。浏览器镜
像是 GHCR 包，不会增加 GitHub Release 附件；原生发布只保留组装后的 GitHub 附件，
校验器从该集合推导名称/数量。

Package 可见性独立于关联仓库管理，工作流不能依赖 repository token 代为改成公开；
新的浏览器 package 在首次推送 digest 前也不存在。因此第一次创建该 package 的
`container.yml` 会先完成推送，再因 GitHub 默认的私有可见性停在匿名拉取门禁。

这是唯一允许的引导例外：在 GitHub Package 设置中把新浏览器 package 设为**公开**
（并确认主 package 也是公开），然后对同一个 tag 手动重跑 `container.yml`。不可
变标签只有 digest 完全相同时才允许重放，所以重跑只完成原发布，不会替换产物。在
重跑全绿之前，容器发布尚未完成；之后每个 Release 都必须在第一次运行时直接通过
匿名门禁。

首次走这条双架构链路发布稳定版之前，必须先发布一个临时 SemVer prerelease，并以
`publish_latest=false` 触发 `container.yml`。这次 rehearsal 要证明两个原生 runner、
package 可见性、匿名拉取、index 精确 children 和两份签名 provenance 全部成立。
演练应使用临时预发布 tag，而非稳定 tag；`X.Y` 与 `latest` 须等到预发布全绿后再
推进。

标签发布后，门禁使用空 Docker 凭据目录匿名拉取两个精确版本标签。package 若仍为
私有或不可访问，`container.yml` 会失败，而不会伪装成可供公开 Compose 使用的成功
发布。

手动触发可回填已有 Release tag，且只有显式选择后才会更新 `latest`。`resolve` 显
式检出 `refs/tags/<tag>` 或明确指定的热修 `source_ref`，校验 Release tag 与仓库
版本后输出唯一的完整 SHA；后续 job 不再解析符号 ref。若重建内容与既有完整版本或
`sha-*` 标签的 digest 不同，会失败而不是覆盖；只接受完全相同 digest 的重放。它的
GitHub 签名证书记录发起 dispatch 的 workflow ref，即使构建随后检出的是已解析的
release commit。历史手动回填的 provenance 因此来自 dispatch workflow ref，不应
被描述为 tag-triggered provenance；正常 `release.published` 使用 Release tag 上
下文。

发布后记录 digest，并同时核验 OCI index 与 GitHub attestation，约束到本仓库的
signer workflow：

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:X.Y.Z
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr-browser:X.Y.Z
docker buildx imagetools inspect --raw \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest>
docker buildx imagetools inspect --format '{{json .SBOM}}' \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> > sbom.json
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr-browser@sha256:<browser-digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
```

SBOM 与 provenance 是供应链元数据，不等于漏洞扫描。GitHub attestation 签名的是
provenance statement；项目当前没有另加独立 Cosign 镜像签名。

当前 Windows 安装包未签名；macOS 用 ad-hoc 签名（`-`），没有 Developer ID 公证。
推送 release tag 前必须复核原生候选冒烟和这些平台警告，因为 tag 工作流成功后会
自动公开。Windows / Linux ARM64、32 位 x86、RPM、Snap、应用商店包仍不支持。签名
的应用内升级只用于支持升级的已安装桌面版；开发构建、CLI、Docker 仍走直接/手动路
径。

## pages.yml —— 架构图展厅

`.github/workflows/pages.yml` 会在相关变更进入 `main` 后，把静态 `docs/` 目录发布到
GitHub Pages；维护者也可通过 `workflow_dispatch` 手工运行。站点入口是
`docs/index.html`。`/diagrams/<name>/` 下的无扩展名 URL 包装已检入的 Archify
HTML；PNG 预览仍可在 GitHub 与离线文档中使用。`scripts/build-pages.mjs` 负责暂存
产物，并从发布副本移除可选的 Google Fonts 链接；图源与已检入验证产物保持不变。

该工作流与质量、发布、容器工作流相互独立，只持有 `contents: read`、`pages: write`
与 `id-token: write`，通过 `github-pages` environment 部署，并固定所有官方 Pages
Action。首次部署前，仓库必须把 Pages Source 设为 **GitHub Actions**。只有成功的工作流
部署才证明在线站点可用，本地打开 HTML 不算上线验证。

## CI 覆盖边界

可复用质量门覆盖前端检查（含 Dashboard V3 契约）、Linux workspace Rust 测试与 Clippy（排除
Tauri 桌面 crate），以及 Windows 上桌面 crate 的编译和单元测试。它会直接在 PR 与 `main`
push 上运行，也会由生产 tag 发布调用；原生安装包与打包冒烟只在手动候选或 tag 流程运行。
容器工作流覆盖 `linux/amd64` 与 `linux/arm64`，各自在原生 runner 上构建且仅 amd64 冒烟；它在 Release 发布后或手动触发时运行。

CI 不操作真实桌面 UI，也不启动真实 Claude Desktop 或 Gemini CLI，不测试备份恢复、
数据库降级、迁移回滚、真实上游账号或真实 Gateway 请求。Rust 测试覆盖 Gemini/Claude
Desktop 路由、鉴权、别名改写、非流式转换、SSE 事件形状、Dashboard V3 CAS、V2 410
墓碑、v27 open/备份以及宿主生命周期源码契约，但不能证明第三方客户端的新版本仍接
受生成的配置。

容器冒烟只检查 TCP 健康、Dashboard HTML、auth status、镜像内许可证，以及未登录
settings 返回 `401`。浏览器容器冒烟会启动真实 Chromium、确认 Profile 目录和无公
开端口，但不登录 Google/OpenCode、不操作 noVNC 键鼠/剪贴板，也不执行真实支付。
Google 数据中心 IP 风控、桌面浏览器发现、Cookie 跨重启保留和远程账号切换仍需手
工验证。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](ci.md) · [文档索引](../README.zh-CN.md)
