[English](architecture.md)

# 架构

## 四层 crate

供应商扩展是 **静态且封闭** 的。没有插件槽、JSON DSL 或用户自定义适配器。 Custom API 是 `ProviderAdapterKind::ConfigurableHttp`，不是其他适配器继承的基类。

```text
ocg-gateway -> ocg-domain
ocg-core    -> ocg-domain + ocg-gateway + ocg-infra
ocg-cli     -> ocg-core
src-tauri   -> ocg-core
```

```text
  ocg-domain                      ocg-infra
  IDs, BUILTIN_PLANS,             crypto, proxy HTTP,
  MODEL_PROTOCOLS, Zen            inference HTTP, log SQL
       ^                               ^
       |                               |
  ocg-gateway                          |
  alias, AttemptSpec,                  |
  classify, selector,                  |
  JSON convert (no I/O)                |
       ^                               |
       |                               |
       +---------------+---------------+
                       |
                    ocg-core
           SQLite, CoreState, Dashboard V3,
           GatewayExecutor, adapters, host_router
                       |
             +---------+----------+
             |                    |
          ocg-cli             src-tauri
       ocg-manager-cli        ocg-manager (tray)

  aside: ocg-browser-worker   独立进程，不依赖 ocg-*
         Vue SPA in src/      静态资源；只走 HTTP V3
```

`ocg-domain` 与 `ocg-infra` 都不依赖内部 `ocg-*` crate。 `ocg-browser-worker` 是独立进程，不依赖任何内部 crate。

| Crate | 负责 | 不持有 |
| --- | --- | --- |
| `ocg-domain` | ID、`BUILTIN_PLANS`、`ProviderAdapterKind`、协议表、Zen ID 规范化、账号/步骤枚举 | DB、`CoreState`、reqwest、rusqlite、tokio、axum、文件系统、时钟 |
| `ocg-gateway` | Alias 注册表、`AttemptSpec`、classify 策略、无密钥 selector、整文档 JSON 转换 | DB、`CoreState`、原始 reqwest、rusqlite、axum、明文凭据 |
| `ocg-infra` | Key 混淆、与目录剥离的代理客户端、推理 HTTP 辅助、单语句日志 SQL | 产品目录、`AppConfig`、Dashboard DTO |
| `ocg-core` | SQLite、`CoreState`、Dashboard V3、供应商适配器、`GatewayExecutor`、`forward_once`、用量同步、宿主组合 | 插件注册表；适配器同样不持有 DB/`CoreState`/原始客户端 |

`ocg-core` 用 **显式兼容门面** 保留历史公开路径（`alias.rs`、`provider.rs`、 `crypto.rs`、`http_client.rs`、`kernel/{ids,catalog,protocol,zen}.rs`、 `gateway/{attempt,classify,protocol,selector}.rs`）。应避免通过 glob 再导出 `ocg_domain` / `ocg_gateway` / `ocg_infra`；`kernel/mod.rs` 的生产图守卫要求 DAG，**不存在多节点 SCC**。`redaction.rs` 是 crate 级叶子。`db` 不依赖 `pricing` 或 `gateway_keys`。`dashboard_v3` 不导入 `gateway` 或 `dashboard`。 `account_control`、`gateway_keys` 与 `usage_sync` 不点名 `CoreState`。

`ocg-gateway` 生产依赖恰好是 `anyhow`、`base64`、`ocg-domain`、 `serde_json`。`ocg-domain` 生产依赖恰好是 `chrono`（仅 serde+std，无 clock feature）、`serde`、`serde_json`。

## ocg-core 作为组合 / 控制面

`ocg-core` 把其他 crate 接起来。只有它打开 SQLite、持有 `CoreStateInner`、挂载 HTTP、访问上游。

- `host_router.rs` 是 HTTP 组合根：推理路由 + `/dashboard/api/v3` + 已退役 V2 REST 墓碑 + 面板静态资源。`gateway` 不导入面板挂载。
- `host_gateway.rs` 实现 `GatewayRebindHost`，让 `state` 在不导入 `gateway` 的情况下重绑监听器。
- `gateway_runtime.rs` / `routing_runtime.rs` 是 DAG 叶子，在 `gateway` 与 `state` 之外持有 `GatewayHandle` 与路由槽。
- `account_control.rs` 是与 HTTP 无关的账号变更服务。Dashboard V3 用 CAS 包装它；CLI 调用同一组函数，argv 上没有 CAS 令牌。两者在成功持久化后都 bump `settings_revision`。
- `gateway_keys.rs` 拥有 `access_keys` 表与内存凭据快照。具体 `KeyStore` / `KeyHost` 实现在 `state`。
- `control/observability.rs` 是与 HTTP 无关的本地读取逻辑，供遗留 V2 适配器与 V3 共用。它不发出站 HTTP。

```text
                    127.0.0.1:9042
                            |
              host_router.rs（HTTP 组合根）
                            |
      +----------+----------+----------+----------+
      |          |          |          |          |
      v          v          v          v          v
  推理入口     Dash V3    V2 REST    保留的      SPA
  /v1 ...     /dashboard 墓碑       V2 auth +   /dashboard
              /api/v3    /dashboard browser WS  /assets
                         /api
                             |
                     匿名 -> 401
                     会话 -> 410 dashboardV2Removed

  推理入口
    POST /v1/chat/completions
    POST /v1/responses
    POST /v1/messages
    GET  /v1/models                  本地；不访问上游
    POST /v1beta|/v1/models/{model}:*
    POST /claude-desktop/v1/messages
    GET  /claude-desktop/v1/models   只公布三个角色别名

  保留的未版本化 /dashboard/api
    auth/status | register | login | logout
    browser/sessions/{token}/ws
  SPA 鉴权走 /dashboard/api/v3/auth/...
```

同一节点的用户向文字图：[架构图](../user/architecture.zh-CN.md)。
路径表：[HTTP 路由](http-routes.zh-CN.md)。

## Gateway 执行

客户端推理在 `crates/ocg-core/src/gateway/`。Axum + Tokio + reqwest，默认绑定 `127.0.0.1:9042`。鉴权前请求体上限 16 MiB。

职责拆分：

1. **`handler.rs`** — 请求 id（`x-ocg-request-id`）、凭证鉴权、客户端解析/ 格式校验、Claude Desktop 改写、Alias 解析。然后把已解析、已解析 Alias 的请求交给 executor。
2. **`GatewayExecutor`** — 冻结的请求入口快照、候选选择、同账号重试、账号回退。一个逻辑客户端请求从头到尾使用同一份不可变价格 revision、同一份 `ForwardRouteSet`、同一份合约集、同一次 Alias 解析。每次回退迭代仍 **重新读取** 账号、合格 Custom 运行时与 Zen Free 冷却。
3. **`provider_adapter.rs`** — 对封闭的 `ProviderAdapterKind` 穷尽匹配。返回纯数据的 `AttemptSpec`（URL、路径、上游协议、鉴权方案、重定向策略、不透明 `CredentialHandle`、`ProxyRoutingModel`）。适配器接收账号、配置与请求 plan。它们 **不** 解密 Key、不打开数据库、不构建 HTTP 客户端。
4. **`forwarder.rs` / `forward_once`** — 每次调用恰好一次上游 `.send()`。只负责传输选择与超时。`forward_once` 内没有策略、没有重试、没有回退。
5. **宿主 `CredentialResolver`** — 在外层循环已经选中账号之后再解密 handle。

```text
  handler.rs
    1. x-ocg-request-id ；鉴权前请求体上限 16 MiB
    2. Key vs credential_snapshot
       Bearer / x-api-key / x-goog-api-key（按头顺序首次命中归因）
    3. 解析客户端协议
    4. Claude Desktop 角色改写（仅该入口）
    5. Alias 解析（kebab / 原始 ID / Custom overlay）
         未知 -> 400
         重叠 -> 400 ambiguous_model_id   （不调用上游）
                    |
                    v
  GatewayExecutor     入口冻结：
                      价格 revision、ForwardRouteSet、
                      合约集、Alias 解析
    6. 物化候选
    7. 过滤卡片 + ocg-gateway::selector
       StrictPriority / StickyGlobal / RoundRobin
       回退迭代重新读取账号、Custom、Zen 冷却
    8. provider_adapter -> AttemptSpec
       （不解密、不开 DB、不建 HTTP 客户端）
    9. CredentialResolver 解密已选 handle
   10. forward_once = 恰好一次上游 .send()
   11. classify  （不在 forward_once 内）
         建连失败     -> 同一账号重试一次
         403 / Go 429 -> 下一张卡
         Free 429     -> 冷却共享 free 通道
         Go CreditsError 401 -> 换号并持久化 auth_error
         其他 OpenCode 401   -> 原样返回
         Custom 401   -> 换号并持久化 auth_error
         408 / 5xx / 响应体超时 / 流中断 -> 不重放
   12. 转换响应；写入 forward_logs
       requested_model、resolved_alias、upstream_model
       （没有 requested_alias 字段）
```

鉴权接受 Bearer、`x-api-key`、`x-goog-api-key`。候选头中首个命中 `CoreStateInner.credential_snapshot`（主 Key 或启用子 Key）者即通过，并作为转发日志名称归因。客户端凭据在出站前剥离，只注入所选账号配置的鉴权方案。Gemini 或 Anthropic 客户端凭据不会透传到上游 offering；Command Code 模型可以与 OpenCode Go 模型共享客户端 Alias，但 mapping 仍保留独立供应商身份，其 Key 也不会发往 OpenCode endpoint。

标准入口为 `/v1/chat/completions`、`/v1/responses`、`/v1/messages`、`/v1/models`。Claude Desktop 使用 `/claude-desktop/v1/messages` 与 `/claude-desktop/v1/models`。Gemini 接受 `/v1beta/models/{model}:*` 与 `/v1/models/{model}:*`；`generateContent` 与 `streamGenerateContent` 进入转换链，`countTokens` 与 `embedContent` 返回 `501`，未知 action 返回 `404`。带鉴权的 `GET /v1/models` 仅本地读取代码持有且当前可路由的 Alias，再追加合格 Custom 声明 ID。最早 OpenCode Go 表授权 Go 名称，精确密封 MiniMax CN、Kimi CN 与选定 GOAT 长名称映射表授权供应商专属名称但不新增 Go 路由；保存的 Zen 目录只能加入 Go Alias，Command 目录可以加入任一代码持有的 Alias，CN 目录只激活密封映射，无法匹配的内置目录 ID 保留为精确 raw pin。受保护的 `GET /dashboard/api/v3/application-models` 是另一份本地列表：Go 可路由别名 ∩ 当前 Go 价格快照（highspeed 变体继承基价行），不含 Custom ID。Claude Desktop `/claude-desktop/v1/models` 只公布三个角色别名。

Alias 注册表在 `ocg-gateway::alias`（门面 `ocg_core::alias`）。首选别名是小写 kebab-case，由最早 OpenCode Go 静态表或密封 CN/GOAT 映射表授权。Command 会去掉 Provider 命名空间；已知套餐后缀只有在短 Alias 已获代码授权时才去掉；`nvidia/nemotron-3-ultra-550b-a55b` 映射为 `nemotron-3-ultra`。Alias 拼写大小写折叠；内置 raw ID 则严格区分大小写，含 `/`、`_` 或空白的名称同样不会折叠成 kebab 别名。原始 ID 在注册表中恰好对应一个 mapping 时钉在该 mapping，之后再检查可路由性；不可路由的 mapping 会被识别，但不能产出生产路由。重叠的精确原始 ID 返回 `400` `ambiguous_model_id`，不调用上游。未知名称在 Chat Completions、Responses、Messages 以及 Gemini generate / streamGenerate 上返回 `400`。合格 Custom ID 继续按其既有匹配规则 overlay 进解析与 `/v1/models`，但不替换已公布的内置 Alias。已公布 kebab 别名 `deepseek-v4-flash` 可以同时含 Go、Zen 与 Command Code mapping；原始 ID `deepseek/deepseek-v4-flash` 精确钉在 Command Code。转发日志持久化 `requested_model`、`resolved_alias`、`upstream_model`、`provider_id` 与 `offering_id`；没有 `requested_alias` 字段。

JSON 转换在 `ocg-gateway::protocol`；宿主 `gateway/protocol.rs` 保留解析、usage、流式与路由身份类型。Gemini 只是客户端格式。已知模型使用 `ocg-domain` 中硬编码的 `MODEL_PROTOCOLS`：客户端协议在 `supported` 内则透传，否则转到 `preferred`。未知模型在所有受支持的客户端格式上返回 `400`；请求路径不试探协议。非空 `safetySettings` 返回 `400`；空数组可以接受。`topK` 与 `thinkingConfig` 只是兼容提示，不保证与 Gemini 等价。

`materialize.rs` 只解析一次客户端协议与 Alias，再按候选物化 model、protocol、endpoint 与 auth。适配器不会通过可计费推理路径试探协议支持。OpenCode `MODEL_PROTOCOLS` 表只服务 Go。表中未知的动态 Zen `-free` ID 默认按 Chat 物化。Custom 按账号把唯一协议、由 API 地址解析出的推理 Endpoint、隔离 origin 与由协议自动决定的鉴权重新物化为该卡声明值。

`zen_models.rs` 是唯一 Zen Free 模型发现路径。受保护的供应商页显式刷新通过全局代理请求固定无 Key endpoint `https://opencode.ai/zen/v1/models`，不跟随重定向，只保留合法且以 `-free` 结尾的 ID；完整快照先持久化，再切换运行时。每个模型保留精确 raw ID；只有去掉 `-free` 后的名称已获最早 Go 静态表授权时，才加入对应 Alias。失败或过滤结果为空时保留旧快照，`/v1/models` 只读取这份快照。Go 所属的 `ox-alpha-free` 是保留排除项。

选择器：宿主 `gateway/selector.rs` 按能力、enabled/ready、凭据有效性、冷却与本次已失败账号过滤卡片，然后无密钥的 `ocg-gateway::selector` 状态机按剩余顺序行走，使用 `StrictPriority`、`StickyGlobal` 或 `RoundRobin`。没有模型路由页，也没有按模型额度池。Zen free 额度按出口 IP 共享：任一有效 `cooldown_free_until` 即视为整条 free 通道耗尽，不换 Key。

价格快照不可变且按供应商分范围。刷新只在用户点击时发生。Provider 路径只抓取并启用该 Provider 自己有价格的 offering；OpenCode 与 Command Code 使用独立 revision 与最后成功状态，一个来源失败不能否决另一个。对 OpenCode Go，月额度只推导账号额度扣减倍率（`月额度 / Usage`），不是可路由额度池。官方表中 Input/Output/Usage 全是 `-` 的行（目前 Ox Alpha Free / `ox-alpha-free`）按无价格促销跳过。官方倍率与当前值不同时，先返回不激活的差异预览；后续请求同时绑定当前 revision 与刚预览的官方 content hash。抓取器仅允许 OpenCode Go HTTPS 主机和同主机重定向，总时限 20 秒、响应体上限 2 MiB。MiniMax 长上下文、priority 与 high-speed 调整是本地策略。

Command Code GOAT 的请求计费只读取其最新、已验证的 Provider 范围快照。模型唯一匹配后写入原始美元成本、额度扣减（`原始成本 × 倍率`）和套餐实付等价值；缺失、歧义或未验证的行仍为 unpriced，绝不会套用 Go 价格。应用倍率可通过 Provider 范围价格写入修改，并持久化为新的快照 revision。GOAT 复用与 Provider 无关的账号窗口投影器：OCG 内已定价日志按 `$14 / $35 / $70` 累计，并可手工修正基线；未定价与外部请求不会计入，也不会按新价格追溯改写旧日志。

回退 / 重试（executor + classify，**不是** `forward_once`）：

- 只有能证明请求尚未发出的 DNS/TCP/TLS 建连失败可以在同一账号重试 **一次**，且必须发生在任何下游字节之前。
- 部分 SSE 不会回退。无法确认的流式结果记为 `outcome_unknown`。 `StreamOutcomeGuard` 在 drop 时收口。
- OpenCode Go 推理 `401` 仅在结构化错误精确为 `CreditsError` 时换号并持久化 `auth_error`；`ModelError`、未知/畸形 401 以及全部 Zen Free 401 都原样返回。普通 Custom `401` 换号并持久化 `auth_error`。面板 Ping / Key 验证的 401 仍记录 `auth_error`。
- `403` 与 Go 通道 `429` 可以切换账号。free 通道 `429` 冷却按 IP 共享的 free 池，不换 Key，并按持久化卡片顺序继续尝试后续兼容候选。普通 Custom/GOAT `429` 不解析 Go 窗口。
- `408`、`5xx`、建连后的失败、响应体超时和流式中断均不会被重放。
- 共享 reqwest client 只设置 30 秒建连超时；非流式请求使用 900 秒总时限，流式请求按 chunk 执行 300 秒空闲时限。

`AttemptSpec` 上的 `ProxyRoutingModel`：

- `RequestEntrySnapshot` — 冻结的双段 `ForwardRouteSet`（Go / Zen）。跟随重定向。受限 URL（https 或回环 http）。
- `ProcessWideNoRedirect` — Command Code 固定官方源的公开目录与推理传输；禁止重定向。
- `IsolatedTrustedAdmin` — Custom：进程级代理、禁重定向、不转发客户端头、管理员受信 URL。

全局出站代理是进程级（`AppConfig`）：自动、手动 HTTP、强制直连或 List。List 模式使用 `proxy_list_direction` 与 `proxy_list_models`。名单内模型走例外段（白名单→代理，黑名单→直连）；名单外模型与非模型出站（验证、Zen 刷新、用量、价格、升级）走默认段。名单成员校验只在 dashboard `update_settings` 写闸口执行（非空、精确已知 id、去重）；加载路径容忍旧值。构造在 `ocg-infra::http`；`ocg-core::http_client` 在精确匹配前折叠目录别名。请求从入口持有一份 `ForwardRouteSet`；并发设置切换只影响之后启动的请求。

```text
  AppConfig  （进程级）
    自动 | 手动 HTTP | 强制直连 | List

  List 模式
    名单内模型 id  -> 例外段
      白名单: 代理
      黑名单: 直连
    名单外模型，以及非模型出站
      （验证、Zen 刷新、用量、价格、升级）
      -> 默认段
      白名单: 直连
      黑名单: 代理

  名单成员只在 dashboard PUT /settings 校验
  （非空、精确已知 id、去重）；加载容忍旧值

  在飞请求保持入口那份 ForwardRouteSet

  AttemptSpec.proxy_routing
    RequestEntrySnapshot     Go / Zen ；跟随重定向
    IsolatedTrustedAdmin     Custom ；不跟随重定向 ；不转发客户端头
    ProcessWideNoRedirect    Command Code 固定官方源；禁止重定向
```

## Plan 目录

`BUILTIN_PLANS` 与 `ProviderAdapterKind` 在 `ocg-domain::provider`（门面 `ocg_core::provider`）。六个家族：

| 家族 | ID | 可路由 | 说明 |
| --- | --- | --- | --- |
| OpenCode Go | `opencode` / `go` | 是 | 只接受官方分发 Key |
| Zen Free | `opencode-zen-free` / `anonymous-free` | 是 | 无凭据单例，数据库持有 |
| Command Code GOAT | `command-code` / `goat` | 是 | 固定官方源；公开供应商目录；模型供应由供应商矩阵控制 |
| MiniMax CN Token Plan | `minimax` / `cn-token-plan` | 是 | 固定中国区源与 Chat Completions 协议 |
| Kimi Code CN | `kimi-code` / `cn` | 是 | 固定中国区源与 Chat Completions 协议 |
| Custom API | `custom` / `api` | 是 | 受信管理员目的地 |

所有持久化变更路径都不会在改动行、revision 或时间戳之前把 `routable=false` offering 的 `enabled` 设为 `true`。每次 `Database::open` 都会把历史 Command Code 验证状态统一为 `not_required`，因为公开目录不是 Key 验证；enabled 状态保持不变。Go、Zen Free、GOAT、MiniMax、Kimi、Custom 与未知 pair 的其他状态不受影响。

Custom API（`custom.rs` + `custom_http.rs`）接受一个语法合法的 HTTP/HTTPS API URL。根地址会补 `/v1` 与所选协议路径；已经以 `/v1` 结尾的基址不会重复添加，兼容的完整 Endpoint 仍保持有效。URL 内嵌凭据、query、fragment 与重定向会被拒绝；dashboard 或客户端鉴权不会被转发。Chat/Responses 自动使用 Bearer，Messages 自动使用 `x-api-key`。账号声明一个协议，对全部模型统一生效并直接作为 effective preferred protocol；其他受支持客户端格式转换到它。配置与完整能力列表原子更新。验证可选，只向解析后的推理 URL 发送一次最小请求。只有标准协议后缀可推导 `/models`，其他路径必须手工填写模型。修改 Key、Endpoint、声明能力或协议会把 `verification_status` 重置为 `pending`，但账号保持启用。Custom 费用/用量为 unpriced/unknown，不扣供应商额度。


## 控制面

Vue SPA 是当前唯一的面板客户端，走 HTTP Dashboard V3。CLI 调用同一组变更服务，argv 上没有 CAS 令牌。没有 Tauri `invoke` 路径。

```text
  Vue 3  （七个视图，KeepAlive）
    Pinia: session / controlPlane / connection
           accounts / providers / settings
           |
           |  src/api/dashboard-v3.ts
           |  src/api/dashboard.ts
           |  src/api/providers.ts
           v
  /dashboard/api/v3
    公开:  /auth/status|register|login|logout
    其余:  dashboard 会话
           回环默认跳过登录（带转发头则仍要登录）
           |
           |  CAS expectedRevision + processGeneration
           |  价格写还要 expectedPricingRevision
           |  GET /contract = 进程内实时 token，不是契约导出
           |  GET /connection = 唯一带 Key 明文的 V3 DTO
           v
  account_control / gateway_keys / settings / ...
           |
           v
  SQLite schema v34
           ^
           |
  ocg-manager-cli  同一组服务，无 argv CAS
```

409 `revisionConflict` 会刷新 token；SPA 不会自动重放该变更。CAS 细节：[Dashboard API](dashboard-api.zh-CN.md)。

## 持久化地图

权威 schema 是 v34。`sub_gateway_keys` 只出现在迁到 v27 之前的历史库，迁完即丢弃。GUI 数据目录在 Windows 为 `%USERPROFILE%\.ocg-mgr`，在 macOS/Linux 为 `~/.ocg-mgr`；CLI 默认 `~/.ocg-mgr-cli`。

```text
  data.sqlite                         CURRENT_SCHEMA_VERSION = 34
    access_keys                       主 Key id PRIMARY_KEY_ID
                                      主 Key 不可禁用/删除
                                      子 Key 活跃上限 64
    accounts                          一张卡 = 一个 Plan
    settings                          AppConfig（gateway_key 存成 ""）
    forward_logs                      requested_model、resolved_alias、
                                      upstream_model、route、provider_id
    gateway_logs
    provider_pricing_snapshots
    provider_usage_sync_state         官方 Go 用量元数据
    provider_model_catalogs
    provider_contract_scopes          旧范围级开关列；effective 推导不再读取
    provider_contract_model_protocols 模型协议证据
    provider_contract_model_protocol_overrides
                                      按模型/按协议覆盖状态
    account_custom_configs
    account_model_capabilities
    account_acknowledgements

  既有非空库：任何 v27 写入前
    同目录不覆盖的 data.sqlite.pre-v3.<UTC>.bak + .sha256
  全新空库：直接建 v31，不写这份拷贝

  Key 经混淆存储；ConnectionInfo 是唯一带明文 Key 的 V3 DTO
```

升级、备份哈希与回滚：[存储与迁移](storage-migration.zh-CN.md)。

## 用量校准

官方 Go 用量是周期性校准基线。上次成功校准之后，本地 `forward_logs` 仍做实时估算。额度条不会停流量。

```text
  官方    GET https://opencode.ai/zen/go/v1/usage
          校准基线（SPA 不会自动轮询）

  本地    上次成功之后的 forward_logs
          账号卡上的实时估算

  后台（Gateway 启动时 spawn；CoreState drop 退出）
    ready+enabled 且近 24h 有本地活动  ~ 每小时
    ready+enabled 且空闲               ~ 每天
    禁用 / 非 ready / 空 Key           不自动刷新
    本地 Go 用量 >= 80%                加速，最少间隔 15 分钟
    推理 429                           约 1–2 分钟后调度官方对账
                                       （不 inline；官方失败
                                        永不写推理冷却）
    失败退避  5m -> 15m -> 1h -> 6h
    全局并发 1；启动带抖动，不轰鸣

  手动  POST /dashboard/api/v3/accounts/{id}/usage/refresh
        15 秒节流（成功/失败都算）
```

锁、时钟与凭据快照：[状态、凭据与生命周期](state-and-lifecycle.zh-CN.md)。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](architecture.md) · [文档索引](../README.zh-CN.md)
