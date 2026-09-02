## 1. build job matrix 化

- [x] 1.1 `container.yml` build job 加 `strategy.matrix.arch: [amd64, arm64]` 与 `include:` 段（`arch: amd64, runs-on: ubuntu-24.04`、`arch: arm64, runs-on: ubuntu-24.04-arm`），`timeout-minutes` 保持 120；`concurrency.group` 维持 `ghcr-<tag>` 不变——workflow 级并发组只作用于跨 run 的同 tag 串行（期望行为），matrix 两腿属同一 run 内的不同 job，本就不经该组交互，**不**并入 arch
- [x] 1.2 smoke 资源环境变量（`SMOKE_IMAGE`/`SMOKE_BROWSER_IMAGE`/`SMOKE_CONTAINER`/`SMOKE_VOLUME` 等全部）追加 `-${{ matrix.arch }}` 后缀（防御性：两条腿理论上各在独立 runner VM，本地 Docker 资源本不冲突；后缀无成本且让日志/清理一目了然）
- [x] 1.3 `docker-bake.hcl` 新增 `variable "CACHE_SCOPE" { default = "ocg-manager-linux-amd64" }` 与 `variable "CACHE_BROWSER_SCOPE" { default = "ocg-browser-linux-amd64" }`；两个 smoke target 的 `cache-from` 改为引用变量；`container.yml` 的 `vars:` 块追加 `CACHE_SCOPE=ocg-manager-linux-${{ matrix.arch }}`、`CACHE_BROWSER_SCOPE=ocg-browser-linux-${{ matrix.arch }}`；两处 build-push 步骤的 `cache-from`/`cache-to` scope 同步改为引用同名变量（scope 字符串 amd64 腿与既有完全一致、已存在缓存继续命中；arm64 腿为新命名空间）
- [x] 1.4 build job `outputs` 重构：**采用 Option B 引入独立 resolve job**——把"解析 release tag/version/minor/stable/publish_latest"、"Check out release source + 校验版本一致"、"Resolve release commit"整体移至 resolve job（`permissions: contents: read`），产出 `tag`/`version`/`minor`/`stable`/`publish_latest`/`image`/`browser_image`/`full_sha`/`short_sha`；build job 改为 `needs: resolve`、各腿自行 checkout（ref 取 `needs.resolve.outputs.tag` 对应的 tag），outputs 仅 `digest_${{ matrix.arch }}` / `browser_digest_${{ matrix.arch }}` 两组（每腿只写本架构两个）；publish job 改 `needs: [resolve, build]` 并把原 `steps.release.outputs.*` 全部改指 `needs.resolve.outputs.*`
- [x] 1.5 **平台参数化（关键）**：两处 build-push 步骤的 `platforms: linux/amd64` 改为 `platforms: linux/${{ matrix.arch }}`；`docker-bake.hcl` 两个 smoke target **删除**现有硬编码 `platforms = ["linux/amd64"]`（当前文件带 pin，不删则 arm64 腿会把 amd64 镜像经 QEMU 烤出来、合并出双 amd64 index——与变更目标相反）；删除后 bake 在各原生 runner 上默认产出本机架构镜像

## 2. publish job 多架构合并

- [x] 2.1 publish job 前置校验：四个架构 digest 全非空，缺失即 fail-fast；`verify_published_digest`/`inspect_digest` 沿用
- [x] 2.2 不可变 tag（version/`sha-`）——**候选 digest 推导顺序定案**：先对 version tag 执行两源 create（附 2.2 的 annotation），`inspect_digest` 回读该 index digest 作为本次发布的候选 digest，再按 D3 确定性复用于 `sha-`/minor/latest 的全部预检（保持"全部决策先于首次变更"的既有顺序）；已存在 → 校验 digest 一致跳过，不存在 → create 后回读校验（buildx `combine` 无时间戳/nonce，同源重放同 digest；`release-policy.mjs` 不改）
- [x] 2.3 每个两源 create 调用**必须带 `--annotation index:org.opencontainers.image.version=$CANDIDATE_VERSION` 与 `index:org.opencontainers.image.revision=$FULL_SHA`**——多源 `imagetools create` 构造全新 index，**不会继承**子清单/build 阶段的任何 annotation；不带此参数则 index 无版本注解，annotation-first `inspect_version` 必然落空（移动配对通道 minor/latest 同样带 annotation，sidecar 先行、main 随后、`verify_paired_moving_tag` 断言不变）
- [x] 2.4 publish 步骤对每个被创建的 index 用 `imagetools inspect` 校验 `mediaType` 为 `application/vnd.oci.image.index.v1+json`（annotation 与 OCI index 绑定的前提），否则 fail-fast 并附"切换 OCI 媒体类型再发"错误
- [x] 2.5 `inspect_version` 回退：优先读 index annotation `org.opencontainers.image.version`（由 2.3 显式写入保证存在），缺失时按 `[amd64, arm64]` 顺序取首个子清单读 label（替换现有硬编码 amd64 选择；旧单平台 tag 行为不变）
- [x] 2.6 匿名拉取验证扩展：`docker pull` 后断言 manifest 为双架构 index（`docker buildx imagetools inspect` 或 `docker manifest inspect` 校验 amd64+arm64 子清单均存在）

## 3. provenance 与策略脚本测试

- [x] 3.1 两镜像的 `actions/attest` subject-digest 改为合并后回读的 index digest（2.2 产出），确认 provenance 对多架构 index 生成成功
- [x] 3.2 **更新既有断言**：`scripts/release-policy.test.mjs` 现有 container.yml 钉死断言随 Option B 接线调整——attest `subject-name` 的 `needs.build.outputs.browser_image` 改指 `needs.resolve.outputs.browser_image`；checkout ref 断言（`steps.release.outputs.tag`）按 resolve job 内保留"Check out release source + id: release"步骤的布局校准；"决策先于首次变更"顺序断言若因 2.2 的 create-then-read 顺序变化需同步调整断言范围
- [x] 3.3 `scripts/release-policy.test.mjs` **新增**断言：`needs.build.outputs.digest_amd64`/`digest_arm64`/`browser_digest_amd64`/`browser_digest_arm64` 四个接线全在；publish 步骤出现主镜像与 browser 两处双源 `imagetools create --tag ... @<amd64> @<arm64>` 且带 `--annotation index:org.opencontainers.image.version`；OCI 媒体类型校验步骤存在；`inspect_version` 注解优先路径存在；build-push `platforms: linux/${{ matrix.arch }}` 接线存在（防 1.5 回归）

## 4. 文档与示例

- [x] 4.1 `docs/USER.md` 与 `docs/USER.zh-CN.md`：两处容器平台行"仅 linux/amd64"改为"amd64 与 arm64"；桌面端"不发布 ARM64"句**保持原样不加括号**（桌面≠容器，不混淆）；在该段或独立 bullet 新增一句"容器镜像（`ghcr.io/klarkxy/opencode-go-mgr`）发布 `linux/amd64` 与 `linux/arm64`"明确与桌面端区分
- [x] 4.2 `README.md` 镜像平台行改为 `linux/amd64, linux/arm64`
- [x] 4.3 `compose.example.yaml` 删除两处 `platform: linux/amd64`；在文件顶部追加英文注释（与该文件既有英文注释一致）：`# On legacy Compose v1 or Podman-compose without manifest-list support, re-add "platform: linux/amd64" to pin amd64`
- [x] 4.4 `AGENTS.md` 项目事实：container.yml 条目改为"构建并冒烟验证 `linux/amd64` 与 `linux/arm64` 镜像"
- [x] 4.5 `docs/MAINTAINER.md` 与 `docs/MAINTAINER.zh-CN.md`：更新所有"仅 linux/amd64 / 不测试容器 ARM64"表述为双架构（发布矩阵、冒烟矩阵、平台覆盖章节），保持中英对等——spec 的"maintainer notes"要求覆盖此二文件

## 5. 演练与验证（发布前执行）

- [x] 5.1 推送前本地静态自查：`act` 不可用则以 YAML 语法 + actionlint（或等效人工审阅）核验 matrix/outputs/platforms 引用
- [ ] 5.2 `workflow_dispatch` 用临时 tag（如 `v0.0.0-arm64-rehearsal-1`，非 stable、`publish_latest=false`）全流程演练：两架构构建+冒烟、合并发布、匿名拉取、provenance 全绿
- [ ] 5.3 演练 tag 在 amd64 与 arm64 真机（或 CI 双架构 job）各 `docker pull` 一次，断言 `docker image inspect` 架构分别为 x86_64/aarch64；确认移动通道未被触碰（minor/latest digest 不变）
- [ ] 5.4 删除演练 tag（保留至少 24h 给拉取缓存）；GHCR 包版本删除走 `gh api -X DELETE /user/packages/container/<package>/versions/<version_id>`（个人仓库）或 `/orgs/<org>/packages/container/<package>/versions/<version_id>`（组织仓库）——先用 `gh api /user/packages/container/<package>/versions`（或 org 路径）按 tag 查出 version id，再对主镜像与 `-browser` 两个包各删一次；包名即仓库名（opencode-go-mgr / opencode-go-mgr-browser）
