[English](releasing.md)

# 发布流程

1. 确定 `X.Y.Z`（或 `X.Y.Z-beta.N` 这类不可变 SemVer 预发布版本），同步修改 `package.json`、`src-tauri/tauri.conf.json`、 workspace `Cargo.toml`、`src-tauri/Cargo.toml`，以及 `compose.example.yaml` 的标题、主镜像与浏览器镜像默认值。
2. 运行 `cargo check --workspace --all-targets` 刷新 `Cargo.lock`，再运行 `pnpm install --frozen-lockfile`、`cargo fmt --all -- --check`、 `pnpm run test`、`pnpm run test:tooling`、`pnpm run design:lint`、`pnpm run contract:v3:check`、 `pnpm run release:check` 和 `pnpm run build`。提交预期的 lockfile 改动；这些改动应由 `cargo check` 与 `pnpm install` 自动生成。
3. 与上一个公开 tag 比较，复核 diff 和当前平台的 `release/` payload，然后提交版本、lockfile、文档与 Release notes 改动。
4. 先合并已经审查的改动，再在 `main` 的最终 commit 上执行 `git tag -a vX.Y.Z -m "OCG Manager vX.Y.Z"`（如为预发布，保留对应后缀）创建附注 tag 并推送。避免在之后还会 squash merge 的分支 commit 上打 tag。
5. 等待 `quality`、`preflight`、全部原生矩阵 job、`draft-release`、 `verify-release` 和 `publish-release` 通过。确认公开的是同一个已验证 draft，再复核与组装产物逐名一致的附件集合、冒烟日志、平台警告，以及基于上一个 tag diff 编写的说明。
6. 对已发布 tag 显式触发 `container.yml`（例如 `gh workflow run container.yml --ref main -f tag=vX.Y.Z -f publish_latest=true`，无需传 `source_ref`），等待它通过，确认两个 GHCR package 已公开，分别核验版本与 digest，再匿名拉取两个完整版本标签。

已发布资产与 tag 不可变。发布有误时通过新 patch 版本修复，禁止替换资产或移动 tag。

## 发布前验证清单

推送 `v*` tag 前完成本清单。CI 覆盖大部分；需真实桌面的条目手工验证。

- [ ] 可复用质量门中的三个 job 全绿（含 `contract:v3:check`）；tag-only 签名
      `release:check` 通过；选中的每个 `pnpm run build` 与平台冒烟全绿。
- [ ] `git diff --check` 干净；相对上一个 tag 的 diff 只含预期范围；四份代码
      版本清单、`compose.example.yaml` 与 `Cargo.lock` 中全部 workspace 包条目一致。
- [ ] 每个 runner 的 `release/SHA256SUMS` 与目录内全部 payload 一致；
      `verify-release` 接受与组装产物逐名一致的附件集合、升级 manifest、四份
      签名、checksum 和 GitHub 服务端 digest。
- [ ] 跑 `cargo test -p ocg-core gemini` 与
      `cargo test -p ocg-core claude_desktop`；用 Bearer、`x-api-key`、
      `x-goog-api-key` 分别请求 Gemini `generateContent` 与
      `streamGenerateContent`，覆盖 Chat 原生与 Messages 原生模型，确认错误
      envelope、usage envelope、HTTP 状态和 SSE 终止行为符合客户端协议。确认
      `countTokens` / `embedContent` 返回 `501`，未知 action 返回 `404`。
- [ ] 确认非空 Gemini `safetySettings` 返回 `400`，`null` 与 `[]` 仍接受。用
      代表性的 `cachedContent`、`fileData`、Google Search、`urlContext` 请求验
      证它们在任何上游计费前失败。对 `topK`、`thinkingConfig` 只验证兼容可用，
      不在冒烟中断言与 Gemini 原生等价的语义。
- [ ] 验证带鉴权的 Claude Desktop 模型发现与 Messages 别名改写。通过
      `PUT /dashboard/api/v3/claude-desktop/models`（带 CAS 令牌）保存全部三个
      映射，用同一数据目录重启后确认映射仍在；非回环面板上确认无会话时映射 API
      返回 `401`。确认已退役 V2 `PUT /dashboard/api/claude-desktop/models` 在
      已鉴权时为 `410`。
- [ ] 打开 **应用** 视图，确认 17 个教程完整可选；逐项抽查复制结果不含掩码
      Key，并实际启动 Claude Desktop 与 Gemini CLI 各完成一次文本和工具调用。
- [ ] 覆盖 schema v16 迁移、schema v27（`access_keys`、pre-v3 备份 + SHA-256
      sidecar、删除 `sub_gateway_keys` 与 `accounts.usage_sync_*`、密文只校验
      不重加密）、v29 SCNet 清理、v30/v31 合约兼容、v32 Custom 单协议转换、v33 上游模型身份、v34 CPA 单例状态、v35 Provider/Plan 身份迁移及其预检备份、别名 / 上游日志身份、可选原生成本、历史 GOAT 验证状态统一为 `not_required`、Zen Free 模型
      快照持久化、供应商合约范围 / 模型协议表、旧账号 `key + ready`、托管状态
      机（前进一格 / 回退更早步骤、不支持跳步）、Pending 路由隔离、邀请 URL 白
      名单与演示默认写回，以及 Key 验证的 `2xx`/`429`/`401`/`403`/网络/`5xx`
      分支；确认除会话保护的 `GET /dashboard/api/v3/connection` 外，任何 DTO
      和日志都没有明文 Key。
- [ ] 确认带鉴权的 `GET /v1/models` 与受保护的
      `GET /dashboard/api/v3/application-models` 是本地读取，GET 本身不访问
      上游。`/v1/models` 是当前可路由已公布别名加上合格 Custom ID；
      `application-models` 是 Go 可路由别名 ∩ 当前价格快照（highspeed 继承基价
      行），不返回 Custom ID。未知模型在 Chat / Responses / Messages / Gemini
      上返回 `400`，除非命中该 `/v1/models` 列表。Command Code 目录刷新公开且不需要 Key；GOAT
      预设行默认开启，额外发现行默认关闭，供应商全关后会从 `/v1/models` 撤下。
      这些本地列表检查不需要真实供应商 Key；发版冒烟不得执行可能计费的推理。
- [ ] 有界假上游 Custom API 冒烟（不需要真实供应商 Key）：拒绝 URL 内嵌凭据；
      合法新账号默认启用且验证保持可选；`2xx` JSON object 会把验证标记为成功，
      但不会改变启用状态；声明的模型/协议可转发；拒绝重定向；不转发
      dashboard/client 鉴权，只发送由协议自动决定的 Bearer 或 `x-api-key`；成功日志为
      unpriced/`cost_state=unknown` 且不扣额度；编辑 URL、Key、能力或协议会使验证回到
      pending，同时保持启用状态。确认 Direct/Manual/Auto 继承进程级代理。
- [ ] 在 Windows 验证 Edge/Chrome 优先级，在 macOS/Linux 验证浏览器发现；用两个
      账号确认 Profile 隔离和重启后 Cookie 保留。确认重置会退出控制台但保留完成
      账号 Key，删除会同时清理新旧 Profile，旧 WebView Profile 不会被导入。
- [ ] 人工完成（可跳过）登录身份 → 邀请链接 → OpenCode 登录 → 支付前确认页 →
      Key 回填；真实支付只由测试者明确执行。控制台打开 `opencode.ai/auth`。旧
      Key 账号首次打开控制台后登录一次，再验证实际额度和邀请使用情况可回访。
      已完成 Key 账号与托管账号都要验证 **刷新额度**（官方 `/zen/go/v1/usage`：
      无效 Key、换 Key 后 409、网络/schema 失败须明确报错且保留上次本地校准）。
      分别覆盖桌面和 Docker Sidecar。
- [ ] Windows 上本地跑一次安装包，确认 SmartScreen 警告文案，打开面板、添加
      账号、发一条请求。
- [ ] macOS 上挂载 DMG，确认 **Open Anyway** 流程可用，打开面板、添加账号、
      发一条请求。
- [ ] Linux 上装 `.deb`、跑 AppImage，CI 上 Xvfb 跑通，本地 Wayland 或 X11 真
      实会话里再确认一遍。
- [ ] Windows 上验证 `auto_start` 开关能切换 `HKCU\...\Run\OCG Manager`，且卸
      载后清理。
- [ ] 确认 `scripts/release.mjs` 报告原子替换 `release/` 成功，旧 `release/`
      已清掉。
- [ ] 本地构建两个容器，并在隔离卷上确认 UID/GID `10001`、内置 `LICENSE`、只读/
      capability 加固、面板鉴权和备份恢复后的属主权限。用
      `docker compose --profile browser up -d` 验证单 Chromium、noVNC 键鼠/剪贴板、
      账号切换、Sidecar 重启、1 GiB shm、无公开端口和双卷备份恢复。
- [ ] 推送 tag 前复核计划使用的 GitHub Release 说明与未签名 / ad-hoc 警告；公
      开后确认同一份说明和精确的已验证资产集合已经发布。
- [ ] 发布后确认 `container.yml` 通过，并按预期 digest 匿名拉取主镜像与
      `ghcr.io/klarkxy/opencode-go-mgr-browser:<version>`，再分别验证 signer
      workflow、SBOM 与 SLSA provenance；GitHub Release 仍为与组装产物逐名一致的
      附件集合。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](releasing.md) · [文档索引](../README.zh-CN.md)
