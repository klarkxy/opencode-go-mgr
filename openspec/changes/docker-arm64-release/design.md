## Context

`container.yml` 的 build job 在单个 `ubuntu-24.04` 上构建两个镜像（`Dockerfile` 主镜像 + `Dockerfile.browser` 侧车），先以 bake 构建 smoke 镜像跑完整冒烟（主镜像健康检查/dashboard/401；侧车真实 Chromium 启动与加固断言），再以 `push-by-digest=true` 推送单平台镜像，最后 publish job 用 `docker buildx imagetools create` 组装 version/`sha-`/minor/latest 并做不可变校验、配对通道（paired-channel）推进、匿名拉取验证与签名 provenance。这个"digest 推送 + 远端组装"结构正是多架构清单的标准做法，改动集中在编排层。依赖已核实全绿：reqwest 系 rustls（纯 Rust、无 OpenSSL）、无 `cfg(target)` 原生分支、三个基础镜像官方多架构、Debian bookworm arm64 具备 chromium 与全部 X 组件。

## Goals / Non-Goals

**Goals:**

- 每架构原生构建 + 原生冒烟，发布产物不经 QEMU
- 已有 tag 从单平台 manifest 变为双架构 OCI index，且旧 tag 不受影响
- 配对通道、不可变策略、provenance、匿名拉取验证语义全部保留
- 首次发布走 `workflow_dispatch` + 临时 tag 演练后才进正式渠道

**Non-Goals:**

- 桌面端（Tauri）ARM64/安装包、`quality.yml` 架构扩展、RPM/Snap（维持 USER 文档"桌面不发布 ARM64"口径，仅镜像声明变更）
- `Dockerfile` / `Dockerfile.browser` 任何改动
- `release-policy.mjs` 策略逻辑改动（digest 对比天然兼容 index）

## Decisions

### D1: 原生 runner matrix，不用 QEMU 交叉构建

build job 加 `strategy.matrix.arch: [amd64, arm64]`，runner 映射 `amd64→ubuntu-24.04`、`arm64→ubuntu-24.04-arm`（公共仓库免费）。理由：QEMU 模拟下两个 cargo release 构建 + 真实 Chromium 冒烟既慢又不可靠（在 120 分钟 timeout 内完成模拟构建风险高、模拟 Chromium 假失败/假通过），原生构建时长与 amd64 相当且冒烟结果可信。代价：matrix 两腿并行使 job 数翻倍， arm64 runner 冷启动可能排队——用按架构区分的 GHA cache（scope 加 `-amd64`/`-arm64` 后缀）缓解增量构建时长。

### D2: 平台参数化与每架构独立 smoke

**前提修正**：当前 `docker-bake.hcl` 两个 smoke target 与 `container.yml` 两处 build-push 步骤**都硬编码了 `linux/amd64`**。matrix 化必须同步：build-push `platforms` 改为 `linux/${{ matrix.arch }}`；bake smoke target **删除**现有 `platforms` pin——删除后 bake 在各原生 runner 上默认产出本机架构镜像，arm64 腿才真正构建 arm64。若只加 matrix 不动 platforms，arm64 腿会把 amd64 镜像经 QEMU 烤出（慢且冒烟跑在模拟 Chromium 上），合并出双 amd64 index，与变更目标相反——这是本变更最易漏也最致命的一步，已列为 task 1.5 并由 task 3.3 的接线断言防回归。

smoke 资源名追加 `-${{ matrix.arch }}` 后缀属防御性整洁（两腿在独立 runner VM 上本不冲突，后缀让日志与清理一目了然）。每腿跑完整现有冒烟——这正是 spec"每架构原生冒烟"要求，且失败即整腿失败、publish 依赖两腿全部成功，天然满足"arm64 失败阻塞双架构发布"。

### D3: outputs 携带按架构 digest，publish 合并两源；digest 确定性澄清

build job 的 outputs 从 `digest`/`browser_digest` 改为 `digest_amd64`/`digest_arm64`/`browser_digest_amd64`/`browser_digest_arm64`（matrix job 的 outputs 会被各腿覆盖，因此每腿只写自己架构的两个 output；tag/version 等元数据经独立 resolve job 统一产出，见 task 1.4）。publish job 的 `imagetools create --tag <ref> --annotation ... <image>@<digest_amd64> <image>@<digest_arm64>` 以两源合并出 index。

**澄清**：buildx 的 `imagetools create` (`combine`) 只产出 OCI image-index，由 `mediaType` / `schemaVersion:2` / `manifests[]` 子清单描述符 / `annotations{}` 组成——**没有时间戳、随机 nonce 或 created annotation**，因此同样的子清单 digest 与 annotation 输入下两次 create 产出 byte-identical 的 index（同一 digest）。候选 index digest 的推导顺序定案为：先对 version tag 执行两源 create 并 `inspect_digest` 回读，该 digest 借确定性复用于 `sha-`/minor/latest 的全部预检——既给出可执行的推导机制，又保住"全部决策先于首次（移动通道）变更"的既有顺序约束；`check_immutable`/`paired-channel` 的 digest 比较路径语义不变，`release-policy.mjs` 不改。**实现细化**：paired-channel 决策只比较远端通道版本、不依赖候选 digest，实际接线中提升到 derive 之前执行（`EXPECTED_DIGESTS` 在 derive 后回填），使"决策先于首次写入"的窗口收窄到仅剩 `sha-` 预检——其候选 digest 必须由 create 推导，属固有代价。

### D4: index 版本注解必须显式写入；OCI 媒体类型为前提

两层事实决定实现方式：

1. **多源 `imagetools create` 不继承 annotation**。现有单源 create 之所以 annotation 生效，是因为它实质把 build 阶段已带 `index:` 注解的单平台 index 直接重挂 tag；双源 create 构造全新 index，源 annotation 一概丢弃。因此两源 create **必须显式带** `--annotation index:org.opencontainers.image.version=$CANDIDATE_VERSION`（及 `index:org.opencontainers.image.revision`），否则 index 无版本注解、annotation-first 路径必然落空。
2. **annotation 只落在 OCI index**。`imagetools create` 按子清单媒体类型决定 index media type：全 Docker schema 2 → Docker manifest list（annotation 整段丢弃）；含 OCI → OCI image index（annotation 生效）。现有 build-push 因 `provenance: mode=max` + `sbom: true`（attestation 要求 OCI 媒体类型）默认产出 OCI 子清单，但这是隐式依赖——本变更在 build-push `outputs:` 显式声明 `oci-mediatypes=true` 加以锁定，并在 publish 步骤对每个 index 用 `imagetools inspect` 校验 `mediaType` 为 `application/vnd.oci.image.index.v1+json`，不满足即 fail-fast。

`inspect_version` 读取顺序：优先 index annotation `org.opencontainers.image.version`（由第 1 条显式写入保证）；缺失时按 `[amd64, arm64]` 顺序取首个子清单读 label（与既有 amd64 优先行为兼容，覆盖旧单平台 tag）。

### D5: 演练通道先行

新增/复用 `workflow_dispatch` 路径，用非 stable 的临时 tag（如 `v0.0.0-arm64-rehearsal-N`）在真实 GHCR 上全流程演练（两架构构建、合并、匿名拉取、provenance），验证 `docker pull` 于两架构各自解析正确后删除演练 tag，再执行一次正式 release。理由：多架构 index 组装与策略脚本回退路径只有真实远端才能验证；临时非 stable tag 不会触碰 minor/latest 移动通道。

### D6: 文档口径一次改齐

USER 双语两处"仅 amd64"、README 镜像平台行、`compose.example.yaml` 两处 `platform: linux/amd64`（删除，让 compose 自动选本机架构；保留注释说明如需强制锁定可自行加回）、AGENTS.md 项目事实。桌面"不发布 ARM64"表述保留但补"（容器镜像除外）"式限定，避免读者混淆。

## Risks / Trade-offs

- [arm64 免费_runner 排队/配额波动] → 构建无硬性时限压力（release 触发非紧急路径）；cache scope 按架构预热；若长期不可用可降级 QEMU 仅构建不冒烟（明确不走）或推迟本变更。
- [matrix outputs 覆盖踩坑] → 每腿仅写本架构 output（`digest_${{ matrix.arch }}`）；在 publish job 前 `assert` 四个 digest 全非空，缺失即 fail-fast。
- [index 媒体类型退化让 annotation 路径静默失效] → D4：build-push 显式声明 OCI；publish 后 `imagetools inspect` 校验 index `mediaType` 为 OCI image-index，失败即 fail-fast。
- [演练 tag 长期滞留 GHCR] → 演练专属非 stable tag 一次性使用，发布成功后由维护者删除（见 task 5.4）。
- [合并清单体积/拉取体验] → 双架构 index 对单架构宿主透明（registry 按需分发对应层）；镜像体积仅构建侧 ×2，用户侧不变。
- [浏览器侧车 arm64 字体/编码差异导致冒烟不稳] → 冒烟断言全部来自既有 amd64 用例；arm64 首次演练若暴露字体渲染差异，仅影响冒烟脚本的容错调整，不影响镜像内容（Debian 同源包）。

## Migration Plan

1. 合并本变更后，先跑 `workflow_dispatch`（tag=演练 tag，`publish_latest=false`）走全流程；在 amd64 与 arm64 机各 `docker pull` 验证架构解析；删除演练 tag。
2. 下一个正式 Release 照常触发，产出首个双架构正式 tag；旧 tag（单平台）不受影响。
3. 回滚：`workflow_dispatch` 支持 `source_ref` 热修复；若需整体回退，revert 工作流改动即可——已发布的多架构 tag 保持不动（不可变策略禁止重写）。

## Open Questions

（无——runner 可用性风险已列为 trade-off 并给出降级判断；其余实现细节在 tasks 粒度内可定。）
