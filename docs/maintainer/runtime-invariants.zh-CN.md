[English](runtime-invariants.md)

# 运行时不变式

运行系统的行为不变式。修改 Gateway 路由、别名、Zen Free、套餐目录、访问 Key、出站代理或用量同步前请先阅读本文。代码是最终权威；本页梳理容易出错的语义。

## Gateway 与模型列表

- Core Gateway：Axum + Tokio + reqwest。同一端口暴露 OpenAI Chat Completions / Responses、Anthropic Messages、Gemini `generateContent` 客户端入口，以及 Claude Desktop 别名入口。
- 已认证的 `GET /v1/models` 首先列出最早 OpenCode Go 静态协议表以及密封 MiniMax CN、Kimi CN 和选定 GOAT 长名称映射授权且当前可路由的 Alias，然后合并符合条件的 Custom 账号声明模型 ID（enabled+ready+非空 Key；验证为可选）。保存的 Zen 行只能加入 Go 已授权 Alias；保存的 Command 行可以加入任一代码持有的 Alias；保存的 MiniMax/Kimi 行只激活代码中精确列出的 CN 映射。无法匹配的内置行只保留精确 raw pin，不会作为新 Alias 公布。受保护的 `GET /dashboard/api/v3/application-models` 仍是 **Go 可路由别名 ∩ 当前定价快照**（highspeed 继承 base-price 行；空交集为 `[]`），不含 Custom 与 CN Plan。两条 GET 路径都不会请求上游；目录刷新只能由控制面显式触发。Custom ID 来自符合条件账号的声明能力。未知模型名在所有支持的客户端格式上返回 `400`。
- Custom 第一阶段映射细化：合格 Custom 能力同时保存公开模型名与精确 `upstream_model`。`/v1/models` 只公布可路由公开名称，绝不形成第二份上游 ID 目录。发现只返回上游 ID，导入时精确写入 `public_model = upstream_model`；不剥离后缀，不合成 Alias。raw 冲突从公布列表排除，并以 `ambiguous_model_id` fail-closed。独立的别名页只是既有合约与能力的只读聚合，不新增 Alias API、store、cache 或编辑面。
- Gemini 客户端使用 `/v1beta/models/{model}:generateContent` 或 `:streamGenerateContent`（`/v1/models/...` 也接受），可以用 `x-goog-api-key` 认证；Gemini 只是一种客户端格式，Gateway 总是把请求转换成目标模型的推荐上游协议。未知模型名在 Chat / Responses / Messages / Gemini 上返回 `400`；禁止探测协议。
- 开发时协议默认值硬编码在 `ocg_domain::protocol::MODEL_PROTOCOLS` 中（`ocg-core` 的 `kernel/protocol.rs` 与 `gateway/protocol.rs` 是 facade/host 转换）：`preferred` 与 `supported` 都取自开发该版本时官方为模型列出的原生端点。客户端协议只有匹配该官方默认值或后续持久化的正面探测证据时才透传，否则转换到 effective preferred；运行时请求路径绝不探测。显式 Provider 探测只能在适配器已登记、可构造的官方上限内追加正面证据。未来未知 Zen Free 行默认关闭，直到后续版本补入官方基线或显式探测成功。整篇 JSON 转换内核在 `ocg-gateway` 中。

一次逻辑客户端请求在入口捕获 `RequestSnapshots`（`crates/ocg-core/src/gateway/executor.rs`）。fallback 迭代重读实时账号状态，不会重新捕获冻结行。没有额外的 snapshot 服务或 trait。

| 入口冻结 | fallback 每轮重读 |
| --- | --- |
| `AppConfig` | accounts |
| 定价快照 | 符合条件的 Custom runtime |
| `ForwardRouteSet` | Zen Free cooldown |
| provider contracts | 当前账号可用性 |
| 已解析 Alias / raw identity | — |

## Dashboard V3 与 V2 墓碑

- Dashboard V3 挂载在 `/dashboard/api/v3`。控制面变更需要 CAS（`expectedRevision`，以及 `processGeneration`；价格写入还需要 `expectedPricingRevision`）。`ConnectionInfo` 是唯一允许返回明文 Key 的 V3 响应 DTO；Key 变更响应不包含明文，客户端会重新 `GET /connection`。
- `GET /contract` 返回当前进程的 live revision / generation token，不是契约导出端点。
- 节点迁移只存在于 V3：`POST /accounts/transfer/export|preview|import`，并且只允许从回环面板执行；转发 scheme 请求头不会赋予权限。导出不再做管理员二次确认。面板只会收到版本 1 的 Argon2id（64 MiB、3 次迭代、单 lane）+ AES-256-GCM 固定 AAD 密文包，绝不会收到账号或接入 Key 明文。密码学工作在进程级单飞限制下执行，且不占用设置或 SQLite 锁；全部响应都带 `Cache-Control: no-store`。预览与导入复用同一解密/校验流程，旧 payload V1 仍可读取。导入在 KDF 后重新检查 CAS，并通过一次 SQLite 事务写入数据库归并结果。payload V2 建立了稳定 ID 身份语义：同 ID 的账号/接入 Key 采用迁移包字段；Plan 或名称相同但 ID 不同的记录可并存；目标端已有账号 ID 保持当前顺序，来源端新增账号 ID 按迁移包顺序接在后面。目标端独有接入 Key 和 Provider 范围保留。不同 ID 的 Key 值冲突、归并后超过 64 个有效子 Key、包内重复 ID 或任一非法行都会拒绝并回滚整个数据库归并。V2 包含可用普通账号及验证状态、ready 账号 Key、主/有效子接入 Key、可迁移配置、Zen Free 状态/目录以及 Provider 目录/证据/覆盖。同 ID 目标记录保留本机浏览器数据和用量/冷却历史；迁移包账号凭据覆盖时会清除过时的鉴权错误和最近错误。来源端浏览器 Profile/Cookie、登录密码、邀请码、日志、用量、冷却状态和未完成托管草稿不迁移。目标端监听/根地址以及操作系统负责的开机启动/Dock 设置保持本机值。所有可能失败的运行态快照构建都读取尚未提交的归并事务；只有构建成功后 SQLite 才提交，随后执行不会失败的快照替换并只递增一次 revision。V1 导入继续采用旧的 Plan/名称重复跳过规则。
- 当前导出细化：密码学 envelope 继续使用版本 1；新导出的 portable 内容改为 payload V3。V3 带有 Custom “公开模型 → 上游模型”映射。解密/校验仍接受 payload V1 与 V2，并把其中旧的单一模型 ID 同时作为两种身份。
- 旧版受保护 V2 REST（`/dashboard/api/...`，不含 V3）向已授权面板会话返回结构化 `410`（`code=dashboardV2Removed`）；匿名请求先返回 `401`。以下语义与墓碑相互独立，不得混淆：V2/V3 认证与会话、browser WebSocket、推理入口。唯一保留的无版本路径是精确的 `auth/status|register|login|logout` 与 `browser/sessions/{token}/ws`。
- `crates/ocg-core/src/dashboard.rs` 现在只处理 SPA `index`/`assets` 与保留的 V2 认证、browser WS 处理器；已退役 V2 REST 的墓碑中间件位于 `host_router.rs`。Provider 模型/协议探测是低频 V3 路径 `POST /providers/{provider_id}/protocol-probes`：它拒绝 effective Provider 目录之外的模型，不提供账号选择，可按保存顺序遍历符合条件的账号，并持久化正向协议证据/`force_on`。账号页使用独立的 operational 路径 `POST /accounts/{id}/model-tests`，请求仅含 `{ modelId }`。该路径按精确账号的当前 scope 准入模型，自动选择有效协议，并且只通过这一账号发送一次最小请求。它不带 CAS，因为不改变控制面状态；不会选择同 Provider 的其他账号，不走 Gateway fallback，不改变冷却、验证或启停，也不写 Provider 证据。面板在前端顺序编排 **测试全部**，结果只保留在当前弹窗。已退役 V2 account 路径授权后仍返回 `410`，不要复活。
- 请求可观测性只属于 `forward_logs`：转发尝试、供应商协议探测，以及已认证的本地解析/校验/路由失败都不得重复写入 `gateway_logs`。账号选择前失败使用“未解析/Gateway”归因，token 字段为零且 `cost_state=not_applicable`。`gateway_logs` 只保留运行生命周期、控制面事件，以及请求日志行持久化或收尾失败。未认证请求与预期的本地 fallback 仍不持久化。

## 访问 Key 与认证

- 从 schema v31 起，权威表是 SQLite `access_keys`（主 Key 固定 id `gateway_keys::PRIMARY_KEY_ID` / `00000000-0000-0000-0000-000000000001`，名称快照为 "Primary"，永不可禁用/删除；子 Key 是非主行，活跃上限 64，软删除保留名称但清空明文）。消毒后的配置 JSON 把 `gateway_key` 存为 `""`，不再是 DB 权威；进程内 `AppConfig.gateway_key` 与 `GET /dashboard/api/v3/connection` 仍暴露 live 主 Key。生命周期只能通过 `/dashboard/api/v3/keys*`（包括 `POST /keys/primary/regenerate`）。主/子 Key 值互斥由 `gateway_keys::ensure_primary_value_allowed` 强制执行。`sub_gateway_keys` 只出现在迁移到 v27 之前的历史库中，迁移后即丢弃；不要把它描述为当前权威表。
- 认证收集所有非空候选头 Bearer / x-api-key / x-goog-api-key；任一匹配凭据快照（`CoreStateInner.credential_snapshot`，含主 Key 与已启用子 Key）即通过，归因按候选头顺序中的第一个匹配；同一快照也用于转发日志名称快照。
- 非 loopback 监听器使用单管理员登录。Docker 可通过 `OCG_ADMIN_USERNAME` 与 `OCG_ADMIN_PASSWORD` 首次初始化（两者必须同时设置；只设一个会导致启动错误）；未提供时，首个注册用户成为管理员。

## 持久化

- 当前 schema 是 **v35**。v33 新增非空 `account_model_capabilities.upstream_model` 并从 `model_id` 回填；v34 新增单例 `cpa_integration` 行。CPA 推理凭证、启停与顺序仍归保留账号，模型快照仍在 `provider_model_catalogs`。v35 创建 `ollama_cloud_usage_state`（Ollama Cloud Cookie 用量状态，详见 `storage-migration.zh-CN.md`）两次迁移都不会合成 Alias。详见 `storage-migration.zh-CN.md`。
- SQLite 到 **v32** 的迁移历史继续保留：v27 把主 Key 与 `sub_gateway_keys` 复制进 `access_keys`，并删除 `accounts` 上遗留的五个 `usage_sync_*` 列。v29 移除 SCNet Token Plans，v30 引入过渡期 Custom 多协议 JSON 集合，v31 新增按模型/按协议覆盖表。v32 将每条 Custom 配置替换为 `endpoint_url` 与单值 `upstream_protocol`；历史行按 Chat → Responses → Messages 选择协议并拼出标准推理路径，清理非所选协议的能力/证据/覆盖，同时把账号设为 disabled/pending。已有非空库在跨越主要 schema 边界前生成不覆盖的 `data.sqlite.pre-v3.<UTC>.bak` 与 `.sha256`；全新空库直接创建当前 schema。GUI 数据目录在 Windows 为 `%USERPROFILE%\.ocg-mgr`，macOS/Linux 为 `~/.ocg-mgr`；CLI 默认为 `~/.ocg-mgr-cli`。升级与回滚详见 `docs/maintainer/storage-migration.zh-CN.md`。
- 下游访问根 URL 优先级：非空 `OCG_CLIENT_ROOT_URL` > SQLite 手动值 > 前端从生产 origin / 开发 Gateway 端口自动推导。环境变量覆盖是只读的，不得写回 SQLite。

## 桌面端宿主

- Tauri v2 跨平台托盘应用；主窗口默认隐藏；托盘/单实例逻辑打开 `http://127.0.0.1:<port>/dashboard/`，loopback 监听器自动跳过登录。宿主能力（Gateway 生命周期、原生浏览器、自动启动、Dock、升级器）注册进 `CoreState`，**不会**注册为 `#[tauri::command]` / `invoke_handler`。不要描述“仍有 live Tauri invoke 命令”为当前状态。
- Settings 页通过受保护 `GET /dashboard/api/v3/settings/check-update` 手动检查最新 GitHub Release。内置了升级器公钥的已安装桌面版可下载、校验签名并原地安装；开发版、CLI、Docker 以及尚未进入更新通道的旧版本仍走 release 页面 / 手动覆盖路径。

## 出站代理

- 全局出站代理存储在 `AppConfig` 中，模式包括 auto（系统/环境）、manual HTTP、force-direct，以及按模型列表（List）。非 List 模式三者互斥；List 模式（`proxy_list_direction` allow/deny list + `proxy_list_models` 已知模型 id）中，列入模型的走方向例外段（allowlist → proxy / denylist → direct），未列入模型与非模型出站（账号测试/验证、Zen Free 手动模型刷新、用量、定价、升级器下载）走方向默认段（allowlist → direct / denylist → proxy）。列表成员校验只在面板 `PUT /dashboard/api/v3/settings` 写入关卡运行（非空、精确已知 id、去重）；加载路径容忍过期值。Zen Free 仅在管理员显式刷新时命中固定 `https://opencode.ai/zen/v1/models`，无需 Key、不跟随重定向，刷新失败或返回空时保留旧快照。reqwest 路径经 `ocg-core` 的 `http_client.rs` facade 进入 `ocg_infra::http` 的 route set / `configured_builder`；Tauri 升级器使用其 `proxy` / `no_proxy` 以与默认段保持一致，不得绕过按账号配置。转发从请求入口快照选取路由；热配置切换不影响飞行中请求。Custom HTTP（`custom.rs` + `custom_http.rs`，传输可能复用 `ocg_infra::inference_http`）遵循同一代理策略；永不跟随重定向；永不转发面板/客户端认证；从唯一协议自动选择一个鉴权头（Chat/Responses 为 Bearer，Messages 为 `x-api-key`）；超时由 `connect_timeout_secs` 限制在 5–60 秒。

## 套餐目录与 Custom API 边界

- 套餐目录位于静态 `ocg_domain::provider` 的 `BUILTIN_PLANS`：OpenCode Go、Zen Free、Command Code GOAT、MiniMax CN Token Plan、Kimi Code CN 与 Custom API。内部身份是 `provider_id` + `offering_id`。Command Code GOAT 可路由；其官方公开 `GET /models` 是供应商目录发现，不是对已保存 Key 的验证。历史 GOAT 验证状态统一为 `not_required`，真实 Key 鉴权边界仍是推理请求返回的 401/403。账号只贡献 enabled+ready+非空 Key 的凭据与顺序，模型供应由供应商模型/协议合约控制：GOAT 内含预设默认开启，额外发现的模型默认关闭，必须显式 `force_on`；不再有账号级 GOAT/全部或 Max 权限模式。已验证的 GOAT 价格快照提供逐请求的价格与倍率核算，并绝不回退套用 Go 价格。Command Code 没有可机读的账号用量端点，因此 GOAT 的用量可用性为 `local_state`，绝不是权威值：面板把 OCG 内已定价请求日志投影到公开的 `$14 / $35 / $70` 三个窗口，并允许手工修正基线。未定价日志与 OCG 外流量不会计入；这套估算不影响推理资格，也不会启动自动同步。所有持久化变更路径仍会在写入、revision 或 timestamp 变更前拒绝启用真正 `routable=false` 的套餐；桌面 UI 只通过 Dashboard V3 HTTP 变更，没有单独的 invoke 变更路径。
- MiniMax（`minimax`/`cn`）与 Kimi（`kimi`/`cn`）是两个独立密封适配器，不是 Custom 预设。两者都使用 Bearer Key、固定且禁止重定向的官方来源、需要鉴权的显式 `/models` 刷新、429 通用供应商冷却，以及 unpriced 转发。开发时官方协议上限都是 Chat Completions + Anthropic Messages，不存在 Responses 路径。MiniMax Chat 使用 `https://api.minimaxi.com/v1/chat/completions`，Messages 使用 `https://api.minimaxi.com/anthropic/v1/messages`；Kimi 使用 `https://api.kimi.com/coding/v1/chat/completions` 与 `/v1/messages`。MiniMax 密封映射覆盖 `MiniMax-M3`、M2.7/M2.5/M2.1 的标准与 highspeed 变体，以及 `MiniMax-M2`，客户端使用对应的小写 kebab Alias。Kimi 映射为 `kimi-for-coding` → `kimi-k2.7-code`、`kimi-for-coding-highspeed` → `kimi-k2.7-code-highspeed`、`k3` → `kimi-k3`、`k3-256k` → `kimi-k3-256k`；转发始终保留精确上游 ID。MiniMax 手工读取 `https://api.minimaxi.com/v1/token_plan/remains`；Kimi 手工读取 `https://api.kimi.com/coding/v1/usages`。返回窗口经过大小限制后，在 V3 CAS 下只替换同账号同来源的快照。这些数据只用于展示：不自动同步，也不改变推理资格。其他 Kimi/GLM 普通或私有端点仍归 Custom API；GLM 没有内置 Plan。
- Custom API（`custom`/`api`，`routable=true`）是可信管理员目标：每张账号卡配置一个 HTTP/HTTPS API 地址和一个 `chat_completions` / `responses` / `messages` 协议；合法新账号默认启用。拒绝内嵌凭据、query、fragment 与重定向；不转发面板/客户端认证。Chat/Responses 只构造 Bearer，Messages 只构造 `x-api-key`，不存在鉴权覆盖或换头重试。根地址通过 `/v1` 加所选协议路径解析；已经以 `/v1` 结尾的基址不会重复该段。现有标准或非标准完整 Endpoint 继续原样使用，其中非标准路径保留手工模型。唯一协议对全部声明模型统一生效，也是 effective preferred protocol：同协议请求透传，其他受支持客户端格式（包括 Gemini）转换到它。Custom 配置与完整模型能力列表通过一次 V3 CAS 原子替换。模型发现对根地址/版本基址使用 `/v1/models`，对标准完整 Endpoint 使用同级 `/models`；不从非标准完整路径猜测。Custom 覆盖只能指向账号声明协议，`force_on` 不能扩张到未声明协议；`force_off` 后模型不可路由且没有固定顺序回退。符合条件的账号动态路由声明模型 ID。Custom 成本/用量未定价/未知。多端点供应商应以后新增静态 Provider Adapter，而不是扩张通用 Custom 路由。
- Custom 第一阶段仍保持静态、密封的 Provider Registry。账号页是唯一映射编辑器：它原子保存一个账号级协议及公开名称 → 精确上游 ID 各行。别名页只展示聚合；其 Custom 编辑深链为 `?view=accounts&account_id=<id>`。关闭时清除 `account_id`；失效 ID 会提示并清除。不会增加动态 Provider、Alias 注册表、插件点、运行时适配器加载或请求时发现。

## 外部接入

- 外部接入是静态产品能力，不是 Provider/Plan 插件。CPA 实现目前暂留给独立验证：常规导航入口隐藏、目录 Plan 不可路由、持久化层强制关闭单例路由账号；typed V3 控制面仍保留供隔离测试。OCG 不会内嵌、启动、升级或读取该服务的私有 auth 文件。
- 桌面/CLI 的 CPA 地址只允许 loopback；Compose profile 只允许固定环境覆盖 `http://cpa:8317`。禁止重定向、内嵌凭证、query/fragment、远程/LAN 主机和转发客户端 Key。CPA 推理强制直连，并与全局出站代理隔离。
- 保留的 CPA 账号只是一张进入现有账号顺序与 selector 状态机的路由卡。CPA 内部 OAuth 账号只是实时投影；日志只归因到 CPA 订阅池，不虚构内部账号。Management 与 Inference Key 在本地加密，OAuth Token 始终留在 CPA，节点迁移排除全部 CPA 状态。
- CPA 目录只能加入代码持有的 Alias；其他保存 ID 仍是精确 raw pin，冲突 fail closed。已知 ID 使用 OCG 协议表，未知 ID 默认 Chat Completions，禁止计费探测。CPA 成本/用量保持 unpriced/unknown；任何 CPA 故障只参与普通候选 fallback，不得影响既有路由。

## 别名

- 客户端别名位于 `ocg_gateway::alias`（`ocg-core` 的 `alias.rs` 是兼容 facade）。内置 Alias 权威完全由代码持有：`ocg_domain::protocol::supported_model_ids()` 提供最早 OpenCode Go 命名空间，密封精确映射表提供 MiniMax CN、Kimi CN 与选定 GOAT 长名称 Alias，但不会借此新增 Go 路由。Command 会先去掉 Provider 命名空间并复用已有代码 Alias；`-paid` / `-free` 只有在短名已获授权时才去掉；`nvidia/nemotron-3-ultra-550b-a55b` 映射为 `nemotron-3-ultra`，`highspeed`、`vision-exp`、`contributor` 等语义变体不会按长度截断。保存的 MiniMax/Kimi 目录只激活其密封映射。未来未知行在代码明确分配 Alias 前只保留精确 raw pin。Alias 拼写可以大小写折叠；内置 raw ID 则严格区分大小写，包括 MiniMax 官方混合大小写 ID，以及含 `/`、`_` 或空格的 ID。Custom 声明 ID 保持原有大小写折叠 matcher。raw ID 若恰好只有一条注册表映射，则在检查可路由性前先固定到该 mapping；不可路由的 mapping 仍被识别，但无法产生生产路由。重叠的精确 raw ID（包括符合条件的 Custom 声明 ID 与另一套餐 mapping 冲突）返回 `ambiguous_model_id`，不会调用上游。Zen Free 的 `foo-free` 只有当 `foo` 已被 Go 表授权时才加入去后缀 Alias，原始 `foo-free` 始终是精确 raw pin；共享的已授权 Alias 按账号卡片持久化顺序在 Go/Zen/Command/MiniMax/Kimi 候选者中选择。符合条件的 Custom 声明 ID 叠加进解析与 `/v1/models`，但不得抢占已发布 Alias。`/v1/models` 只有在精确保存目录行仍存在且至少一个 mapping 有启用的 effective 协议时才发布该 Alias；供应商全关后不产生路由。转发日志区分 `requested_model`、`resolved_alias` 与 `upstream_model`；`native_cost_*` 为可选；不要臆造 `requested_alias` 字段。Claude Desktop 的三个角色别名仍在别名解析前被重写；`/claude-desktop/v1/models` 只发布这三个角色。

| 判定 | 结果 |
| --- | --- |
| 代码持有的 kebab Alias | 解析为 Alias；仅在已保存目录行存在且至少一个 mapping 有启用的 effective 协议时发布 |
| 唯一 raw ID | 在检查可路由性前固定到该 mapping；不可路由 mapping 仍被识别但不发布 |
| 重叠的精确 raw ID | `400` `ambiguous_model_id`；不上游 |
| Zen `foo-free` | 始终是精确 raw pin；仅当 `foo` 已被 Go 授权时才加入 Alias `foo` |
| GOAT 长名 / 后缀 | 去掉 Provider 命名空间；仅在短名已获授权时剥 `-paid`/`-free`；`nvidia/nemotron-3-ultra-550b-a55b` → `nemotron-3-ultra` |
| MiniMax / Kimi 密封映射 | 保存的目录只激活代码持有映射；未匹配行保持精确 raw pin |
| 符合条件的 Custom 声明 ID | 叠加进解析与 `/v1/models`；不得抢占已发布 Alias |
| `/v1/models` 发布门槛 | 精确保存目录行 + 至少一个启用的 effective 协议；全关 scope 不产生路由 |

## Zen Free

- Zen Free 是特殊的内置账号，没有 Key；只有账号卡片启用开关，不再有 `deny` / `explicit` / `prefer` 或自动 prefer 策略。管理员在 Providers 页点击“获取模型” (Fetch Models) 时，请求固定官方目录，仅保留以 `-free` 结尾的规范化有效 ID并持久化上次成功快照。每个保存 ID 都保留精确 raw pin；只有去掉后缀后的名称已获最早 Go 静态表授权时，才加入对应 Alias。刷新失败或空结果不会覆盖旧快照。不需要 Free 时关闭卡片；启用时按卡片顺序与其他账号一起被选择。协议探测控件也在 Providers 页，而不是账号卡片。Zen Free 与 Go 使用独立的 `cooldown_free_until`；Zen Free 配额按出口 IP 共享，收到 429 后整个 Free 通道冷却，不切换 Key，路由继续尝试后续兼容卡片，仅当只剩 Free 候选者时才返回共享冷却。Zen Free 的推理 `401` 原样返回。OpenCode Go 仅在已限长 JSON 错误体精确满足 `/error/type == "CreditsError"` 时换号并写入 `auth_error`；`ModelError`、未知、畸形、截断或读取失败的 401 仍原样返回。重新保存同一个 Key 会清除该断路状态。面板 Ping / Key 验证的 401 仍记录 `auth_error`。Free 通道成功行记录 `cost_state=free`，不计入 Go 配额。Go 的 `ox-alpha-free` 仍由 Go 静态映射处理，计为 `unpriced`，不算 Free。

## Claude桌面

- Claude Desktop 使用 `/claude-desktop/v1/messages` 与 `/claude-desktop/v1/models`；`sonnet`、`opus`、`haiku` 映射存储在 `AppConfig.claude_desktop_models` 中，由受保护 `GET/PUT /dashboard/api/v3/claude-desktop/models` 管理。

## 托管账号（Beta）

- `setup_step` 顺序为 `google_account`（UI：登录身份，可跳过）→ `opencode_registration` → `payment` → `key_verification` → `ready`。`PATCH /dashboard/api/v3/accounts/{id}/setup` 允许前进一步或回退到更早步骤；禁止跳过步骤或直达 `ready`。草稿创建可编辑邀请链接并写回 `opencode_invite_url`（`DEFAULT_OPENCODE_INVITE_URL` 是演示默认值）。浏览器目标包括 Google/GitHub 注册与登录、邀请 URL，以及控制台 `https://opencode.ai/auth`。托管页可通过 dashboard HTTP 打开浏览器；桌面原生浏览器是 Host hook，不是 WebView invoke。

## Ollama Cloud

- 家族密封且固定源：`https://ollama.com`，仅 Chat Completions，Bearer，不跟随重定向，`ProcessWideNoRedirect` 代理语义（方向默认段；本家族模型 id 不参与按模型例外段匹配）。协议事实存于 `ocg_domain::protocol` 的家族独立种子（`OLLAMA_CLOUD_*`）；`MODEL_PROTOCOLS` 必须保持无 Ollama 专属 id——该表派生 Go 已发布别名。adapter 标记 `WireNormalization::OllamaCloud`；forwarder 按尝试改写请求字节（assistant `reasoning_content` 缺失时补 `reasoning`，`max_tokens`/`max_completion_tokens` 钳制到 65535），并对响应/SSE 帧做规范化（`reasoning`/`thinking` → 补 `reasoning_content`）。混合候选链中非 Ollama 尝试的请求字节必须逐字节不变；`upstream_body_bytes` 记录实际发送字节；面向客户端的归因保留请求名。
- Cookie 用量为 opt-in 且仅手动：存储的网页会话 Cookie（与 Key 同级混淆设施，非 AEAD）仅用于抓取固定 `https://ollama.com/settings` 页（不重定向，15 秒超时，512KB 上限，进程级出站默认段）。解析锚定 `data-usage-track`/`data-usage-segment`/`data-model`/`data-requests`/`data-time`/`data-usage-window` 及轨道的 `aria-label`（现网页面使用裸 `data-usage-track` + `aria-label="Session|Weekly usage N% used"`、兄弟 `data-time` 重置时间戳，以及按模型的 `data-usage-segment` 按钮）；快照仅持久化脱敏 JSON，失败只更新状态/退避且不动上次成功快照，`unauthorized` 表示会话过期。手动刷新 30 秒限速（成败都计），固定退避 5 分钟 → 15 分钟 → 1 小时 → 6 小时；**用量路径的任何失败绝不写推理冷却、绝不改变账号启用状态、绝不影响路由资格**。状态存于 `ollama_cloud_usage_state`（schema v35，随账号级联删除）。Cookie 与用量快照不进导出载荷；载荷结构与版本不变。
- 别名追加守卫：目录刷新仅在剥除 `:` 标签后恰好命中一个目录 id 时，向 Go 拥有的别名（如共享词干）追加一个可路由 Ollama 映射；同词干多快照并存时该映射退出，别名仍由既有家族服务，管理员矩阵钉定会被后续刷新尊重。带日期标签的快照 id 是运行时目录数据，严禁写进代码。

## 用量同步

- 已完成账号的配额：官方 `https://opencode.ai/zen/go/v1/usage`（`go_usage.rs`）是周期性校准基线；本地 `forward_logs` 在上次成功校准后仍做实时估算。`usage_sync.rs` 协调手动与后台路径：ready+enabled 且最近约 24h 内有本地活动的账号约每小时对账一次，不活跃的约每天一次；disabled/not-ready/空 Key 账号不自动刷新。全局并发 1，带 jitter 与可注入 clock/jitter/fetch seams；无启动惊群。手动 `POST /dashboard/api/v3/accounts/{id}/usage/refresh` 仍可用；服务端限流为每账号 15s（无论成功失败都计入），带并发去重，返回 Retry-After / `next_allowed_at`；失败保留上次基线与上次成功。本地最大 Go 用量 ≥80% 时，加速对账至多每 15 分钟一次。真实推理 429 仍写入现有冷却/选择器，并额外安排约 1–2 分钟后官方对账（非内联）；官方失败或 `status=rate-limited` 从不写入推理冷却。成功后按最早 `resetsAt`（加有界 jitter）重新调度，尊重活跃/不活跃节奏。失败退避：5m → 15m → 1h → 6h。同步元数据位于 `provider_usage_sync_state`（`accounts.usage_sync_*` 不再使用）。共享实现包括 CAS / 三窗原子校准与全局代理。官方 Go 文档未列出该端点。不要引入 CDP 自动化刷新。

## 定价、容器与 CI 说明

- 定价按 Provider 独立管理：读取 `GET /dashboard/api/v3/providers/{provider_id}/{offering_id}/pricing`，刷新 `POST /dashboard/api/v3/providers/{provider_id}/pricing/refresh`，应用倍率通过 `PUT /dashboard/api/v3/providers/{provider_id}/{offering_id}/pricing/multipliers` 写入。Go 与 GOAT 的倍率写入各自使用本 Provider 的 pricing revision 做 CAS；GOAT 写入追加 Provider 快照，只影响后续请求估算。刷新若会覆盖本地倍率必须先展示差异。OpenCode 与 Command Code 各自维护修订号和最后一次成功快照；只有用户点击对应 Provider 的刷新按钮时才访问其固定官方来源，禁止自动轮询。
- 公开 GitHub Release 发布后，`.github/workflows/container.yml` 在原生 amd64（`ubuntu-24.04`）与 arm64（`ubuntu-24.04-arm`）runner 上构建 `linux/amd64` 与 `linux/arm64` 镜像（仅 amd64 跑冒烟测试），按 digest 推送各架构，再合并为同一标签下的多架构 OCI index，发布到 `ghcr.io/klarkxy/opencode-go-mgr`。Compose 默认使用该镜像；本地源码构建需 `OCG_IMAGE=ocg-manager:local` 后 `docker compose up -d --build`。
- `.github/workflows/quality.yml` 在 PR / `main` 上分为三个并行 job：Web（含 `pnpm run contract:v3:check`、前端测试/类型/lint）、Linux workspace Rust 测试/Clippy（排除 Tauri 桌面 crate，无需安装系统包）与 Windows Tauri 目标测试（stub `dist/`，不运行 Vite）。`release.yml` 仅在生产 `v*` tag push 时调用该质量门。`release.yml` 手动候选（即使选择 tag ref）始终未签名，且可能只构建所选平台；只有 `v*` tag push 事件才会构建全部三个平台并读取仓库签名密钥。tag push 被视为单维护者显式发版授权：工作流逐个校验附件集合与组装产物名称匹配（数量由产物推导，非硬编码）、升级器签名、公钥连续性，以及 GitHub 服务端摘要，然后自动发布同一未改动草稿。
- 容器以固定 UID/GID `10001` 运行，包含 `LICENSE`；Compose 透传可选 `OCG_MANAGER_ENCRYPTION_KEY` 以支持显式 Key 恢复，但正常部署仍倾向于在卷中保留 `.encryption-key`。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](runtime-invariants.md) · [文档索引](../README.zh-CN.md)
