## Why

容器镜像目前只发布 `linux/amd64`（`container.yml` 硬编码单平台），ARM 服务器/NAS/树莓派等自托管用户只能依赖 QEMU 用户态模拟运行 amd64 镜像，性能差且 Chromium 侧车在模拟下基本不可用。GitHub 已为公共仓库提供免费的 `ubuntu-24.04-arm` 原生 runner，且现有发布流水线本就是"按 digest 推送 + imagetools 组装 tag"的多架构就绪形态——补齐 arm64 的成本集中在 CI 编排，不在代码。

## What Changes

- **build job 改架构 matrix**：`container.yml` 的 build job 从单 `ubuntu-24.04` 改为 matrix（`amd64` → `ubuntu-24.04`、`arm64` → `ubuntu-24.04-arm`）；两处 build-push 的 `platforms` 由硬编码 `linux/amd64` 参数化为 `linux/${{ matrix.arch }}`，`docker-bake.hcl` smoke 目标删除现有 `platforms` pin（否则 arm64 腿会烤出 amd64 镜像）；每个架构原生构建主镜像与 browser 侧车镜像、原生执行现有全部 smoke（含真实 Chromium 启动检查），各自以 `push-by-digest=true` 推送单平台镜像；release 元数据（tag/version/minor 等）由新增独立 resolve job 统一产出。
- **publish job 合并多架构清单**：`docker buildx imagetools create` 从单一 digest 源改为合并两个架构 digest（显式带 `--annotation index:org.opencontainers.image.version/revision`——多源 create 不继承源注解），产出同一个 version / `sha-<short>` / minor / latest tag 的 OCI index；配对通道（main 与 browser 同步推进）语义不变。
- **按架构修正细节**：`inspect_version` 优先读 index annotation、缺失时按 `[amd64, arm64]` 顺序取首个子清单；build-push 显式 `oci-mediatypes=true` 并在 publish 后校验 index mediaType 为 OCI（annotation 仅落于 OCI index）；GHA build cache scope 按架构区分（amd64 字符串不变、既有缓存继续命中）；smoke 资源命名按 run+arch 加后缀（并发组保持 tag 级不并入 arch）。
- **文档与口径同步**：`docs/USER.md` / `USER.zh-CN.md`（"仅 amd64"两处）、`docs/MAINTAINER.md` / `MAINTAINER.zh-CN.md`（发布矩阵与平台覆盖表述）、`README.md`（镜像平台说明）、`compose.example.yaml`（`platform: linux/amd64` 锁定移除并加英文回锁注释）、`AGENTS.md` 项目事实；明确"桌面端仍不发布 ARM64 安装包，本变更仅覆盖容器镜像"。
- **零代码改动**：`Dockerfile` / `Dockerfile.browser` 不改（依赖已验证：rustls 纯 Rust 无 OpenSSL，基础镜像官方多架构，Debian bookworm arm64 具备 chromium 及全套 X 组件）；`release-policy.mjs` 不可变 tag 逻辑基于 digest 对比，天然兼容多架构 index。

## Capabilities

### New Capabilities

- `container-arm64-release`：容器发布渠道的多架构交付契约——两架构原生构建与 smoke、digest 级合并发布、配对通道不拆分、平台声明与文档口径。

### Modified Capabilities

（无——`openspec/specs/` 无主规格，本能力全新定义。）

## Impact

- **CI**：`.github/workflows/container.yml` 是唯一实质改动面（build job matrix 化、outputs 携带按架构 digest、publish job 合并源、inspect 回退、cache scope、smoke 资源命名）；`docker-bake.hcl` smoke 目标保持本机架构原生构建（不加 platforms 交叉）。
- **发布产物**：`ghcr.io/klarkxy/opencode-go-mgr` 与 `ghcr.io/klarkxy/opencode-go-mgr-browser` 的既有 tag 从单平台 manifest 变为多架构 OCI index；旧 tag 不受影响（不可变策略只管新发布）。
- **文档**：USER 双语、README、compose 示例、AGENTS.md。
- **风险面**：arm64 原生构建时长与缓存冷启动；免费 arm64 runner 的配额与可用性；首次多架构发布必须先经 `workflow_dispatch` + 临时 tag 演练（发布到测试 tag，验证合并清单与匿名拉取后再进正式渠道）。
- **不在范围**：桌面端（Tauri）ARM64 安装包、Windows/Linux ARM64 原生构建、RPM/Snap、`quality.yml` 的架构扩展。
