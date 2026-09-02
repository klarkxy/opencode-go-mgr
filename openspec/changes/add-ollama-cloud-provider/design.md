## Context

上游 v2.1.0（HEAD `9eed30c`）已是六 crate workspace（`ocg-domain`/`ocg-gateway`/`ocg-core`/`ocg-infra`/`ocg-browser-worker`/`ocg-cli`），schema v33，V3 dashboard 契约驱动。密封注册表现状：`ProviderAdapterKind` 六变体（`crates/ocg-domain/src/provider.rs:505`），adapter 只产出 data-only 的 `AttemptSpec`（`crates/ocg-gateway/src/attempt.rs:93-111`），DB/解密/reqwest 留在宿主。Custom API 模型映射（aed05bc）已覆盖 Ollama Cloud 的转发与公开名别名（子代理验证 FEASIBLE）。本变更只做 Custom 表达不了的两件事：wire 规范化与 Cookie 用量。参照系：GOAT（固定源 adapter + 目录发现）、Kimi/MiniMax（手动用量、Bearer、固定源、`plan_usage.rs` 的 `configured_builder` 出站先例）、Wei-Shaw/sub2api 的 `ollama_cloud_usage*.go`（抓取与解析参照实现）。

关键管线事实（约束 Decision 1）：请求体在候选选择**之前**经 `convert_request_json` 一次性转换并序列化进 `RequestPlan.body`（Bytes），同一份字节被所有 fallback 尝试共享（`forwarder.rs:786`）；`AttemptSpec` 是每个尝试在选择循环内才产出；SSE 方向在成功尝试后构造 `StreamConverter`（`forwarder.rs:1338`），其同协议路径目前是逐帧透传语义。别名命名空间全局且 Go 优先：`deepseek-v4-flash`、`deepseek-v4-pro` 已是 Go 拥有的已发布别名（`MODEL_PROTOCOLS` → `supported_model_ids()` → 发布表），GOAT 已通过 overlay 向既有别名追加映射（`alias.rs` `overlay_goat_catalog`）。at-rest "加密"实为 `.encryption-key` 派生的 XOR 混淆（`ocg-infra/src/crypto.rs` 自述非密码学安全），Argon2id+AES-GCM 仅存在于导出传输信封。

## Goals / Non-Goals

**Goals:**
- Ollama Cloud 家族的转发在协议层与 DeepSeek/OpenAI 客户端期望对齐（`reasoning_content` 可见、超大 `max_tokens` 不 400），且对混合候选链中非本家族尝试零字节影响。
- 快照型上游 id 的轮换不触发发版：代码只拥有词干与解析规则，快照 id 永远是目录刷新的运行时数据。
- 用量能力以最小信任面落地：粘贴 Cookie、固定 URL、手动优先、与推理状态完全隔离。

**Non-Goals:**
- 不做请求驱动的自动用量刷新、不做多源泛化、不做 wire 规范化的用户开关。
- 不改变 Custom API 的任何行为；两者并存。
- 不引入动态 Provider/Alias 注册表或插件点（上游不变量）。
- 不提升 at-rest 存储的密码学强度（保持与现有 Key 同级的混淆设施）。

## Decisions

1. **wire 规范化：`AttemptSpec` 增加规范化标记字段（仍 data-only），请求方向按尝试改写，响应方向扩展 `StreamConverter` 输入。**
   - 标记：`AttemptSpec` 增 `wire_normalization: OllamaCloud` 型枚举字段（默认 None）。这是新增纯数据字段，不破坏 `attempt.rs` 纯度约束（ocg-gateway 已依赖 ocg-domain）。
   - 请求方向：规范化函数为 ocg-gateway 纯函数（输入/输出 Bytes）。宿主 forwarder 在解析出 `AttemptSpec` 之后、`forward_once` 之前，若标记命中则对 `plan.body` 做**本次尝试级**改写再发送。理由：请求体在选择前一次性序列化并被整条候选链共享，而共享别名（Decision 2）使 Go↔Ollama 混合候选链成为常态，plan 期一次性规范化会污染非本家族尝试；尝试级改写保证 C2 场景（混合链字节隔离）。副作用：`upstream_body_bytes` 日志记录改写后的实际发送字节（诚实原则），contract 测试锁定。
   - 响应/SSE 方向：成功尝试后以该尝试的标记构造 `StreamConverter`，为其增加规范化输入；同协议逐帧透传语义为标记家族开例外（仅补 `reasoning_content`，不改其他字段）；`[DONE]` 与非 JSON 行原样。
   - `max_tokens`/`max_completion_tokens` > 65535 单向钳制写死常量（注释标注上游实测依据，不提供覆盖项）。

2. **别名：共享别名映射追加（Go 优先），本家族永不创建或抢占别名。** 首期词干 `deepseek-v4-flash`、`deepseek-v4-pro` 是 Go 已拥有别名，本家族的唯一贡献是在目录刷新后向其候选映射集**追加**一个 Ollama 映射（GOAT overlay 同款机制），绑定 id 由"剥 `:` 标签后词干唯一匹配"守卫解析：恰好一个匹配才可路由；零/多匹配仅本家族映射不可路由，同名别名由 Go 等既有家族照常服务（不产生 400，不扰动既有发布与路由）。管理员钉定复用模型矩阵数据形状（选择域 = 发现的精确 id），持久且刷新尊重。跨家族回退意味着同名请求可能由 Go（计价）或本家族（unpriced）服务——归因与计价跟随实际服务家族，这是共享别名的既有语义，spec 场景锁定。反向隔离写死：本家族映射 MUST NOT 反向改写 Go/Zen/GOAT 映射。
   - 快照 id 禁止进代码写成 spec 不变量；预设表只登记裸词干（两个共享别名）与尺寸变体的完整 id。

3. **目录发现复用 GOAT 模式（公开 `GET /models`，Provider catalog 刷新，非 Key 验证），并以"免鉴权可达"为实现前提。** 实现期实测（tasks 9.2）：若端点要求 Bearer，目录刷新路径改为绑定账号 Key 的认证刷新后重评，启用位在此之前保持关闭。

4. **Cookie 用量为独立服务模块（ocg-core），形状是 `go_usage.rs` + `usage_sync.rs` 的最小子集。** 只做手动刷新 + 固定阶梯退避（5m→15m→1h→6h 封顶，对齐现行用量同步；settings 页无 `Retry-After` 可言，不依赖响应头）+ 30s 节流 + 并发去重；不做后台循环、不做请求驱动触发（sub2api 的 activity-debounce 模型被否：ocg 用量哲学是"显示与校准，绝不影响路由资格"）。持久化：v34 新表 `ollama_cloud_usage_state`（`account_id` PK + `FK accounts(id) ON DELETE CASCADE`、`status`、`snapshot`（仅成功时写入）、`last_success_at`、`last_attempt_at`、`next_eligible_at`、`failure_streak`）；失败路径只更新 status/尝试元数据/退避列，**不动 snapshot 列**（满足"失败不清空上次成功快照"）。Cookie 以现有混淆设施存储（与 API Key 同级，`.encryption-key` 派生 XOR 流；明示非 AEAD），独立存储位、不进 `account_custom_configs`（家族边界清晰）。错误信息字段 ≤256 字符且过滤 HTML 片段与 URL 查询串。
   - 请求边界照抄 sub2api 的已验证参数：精确 URL 校验（scheme/host/path 全等）、不跟随重定向（校验响应链最终 URL）、15s 超时、512KB 上限、`Cookie` 头归一化校验（拒 Set-Cookie 属性、拒重名、拒 `$` 前缀、拒空值、≤16KB）。
   - 出站 MUST 经 `http_client.rs` facade / `configured_builder` 方向默认段（`plan_usage.rs:32-43` 先例），禁止自建绕过全局代理的客户端。
   - 解析锚定 `data-usage-track`/`data-usage-segment`/`data-model`/`data-requests`/`data-time`/`data-usage-window` + 标签文本（plan/balance），登录页判定 → `unauthorized`；解析器输入为有界 HTML，轻量 DOM 遍历。

5. **身份与 Plan**：`provider_id = "ollama"`、`offering_id = "cloud"`；`BUILTIN_PLANS` 增行：凭据 Bearer、`upstream_protocols` 仅 Chat、unpriced、`usage_availability = LocalState`（手动用量，GOAT 同款）、`quota_unit` 按现行闭集取百分比口径、协议探测 `protocol_probe_supported = false`（固定仅 Chat 家族无可探测协议面，GOAT/MiniMax/Kimi 同款；Providers 页不呈现探测入口）。Cookie 输入不是标准 `form_fields`，走账号卡专用 UI（V3 契约单独字段）。

6. **V3 契约与导出边界**：`dashboard_v3/types.rs` 增 DTO 并入 `CATALOG_TYPE_NAMES`，`pnpm run contract:v3:generate` 重生成；Providers 页家族卡（目录刷新、模型/协议矩阵）复用现有控件（无探测入口）；Accounts 表单在 Key 之外增加可选 Cookie 输入（CAS 写；回显一律脱敏）。**导出/导入载荷不变**：现行不变量本就省略 usage/浏览器数据，用量快照与 Cookie 属同一省略类——不扩 payload、不 bump 版本（避免 `deny_unknown_fields` 下旧二进制拒读新版 V3 的兼容问题）；导出边界回归测试锁定新表数据不出现在导出包。

## Risks / Trade-offs

- [Ollama 改版导致 DOM 锚点失效] → 锚点全部属性化（比 class/文本稳健）；解析失败只置 `failed` 且不动快照列；spec 冻结锚点集合，改版是数据问题不是路由问题。
- [Cookie 是高敏感凭据，泄露面大于普通 Key；at-rest 仅为混淆级] → 明示保护级别并与现有 Key 同风险口径接受；补偿控制：全链路脱敏、导出永不携带、日志脱钩、错误串过滤、UI 仅显示状态与有界摘要。若未来需要真 AEAD，属独立设施变更（更新 storage-migration 与恢复语义），不在本变更。
- [规范化启发式对非本家族流量误伤] → 尝试级触发 + 仅字段缺失补写 + 仅超限钳制 + 混合链字节隔离契约测试锁定其他家族字节不变。
- [共享别名使同名请求可能从 Go（计价）滑到本家族（unpriced）] → 这是共享别名既有语义（GOAT/Custom overlay 先例）；spec 锁定"归因与计价跟随实际服务家族"；用户可通过账号禁用或裸 id 钉定控制流向。
- [快照轮换窗口内本家族映射短暂不可路由] → 接受。别名仍由既有家族服务；矩阵人工钉定是显式出口。
- [`/v1/models` 免鉴权假设不成立] → tasks 9.2 实测前置；若需鉴权，目录刷新改为认证模式后重评，启用位保持关闭。
- [上游将来也实现 Ollama 家族造成冲突] → 共享别名追加与守卫语义同构（GOAT 先例），冲突时以最小 diff 对齐上游命名。

## Migration Plan

1. schema v33 → v34：新表 + 幂等迁移；非空库先产 `data.sqlite.pre-v3.<UTC>.bak` + sha256（现行策略）；失败回滚到 v33。
2. 家族代码以 fail-closed 合入（offering `routable=false`），路由/控制面/用量三条路径各自完成后才打开启用位。
3. 回滚：禁用启用位即恢复不可路由；v34 表对新代码无逆向依赖，旧二进制按 schema 新于自身拒绝打开（现行保护）。

## Open Questions

- Ollama Cloud 对裸名（无标签）请求是否等价 `:latest`——实现期用真实 Key 做一次最小验证后作为协议事实记录；spec 不依赖该结论（默认转发目录精确 id）。
- `/v1/models` 免鉴权可达性——实测前视为前提而非结论；不成立则目录刷新改认证模式并重评（Decision 3）。
- 首期别名是否需要覆盖尺寸变体（如 `gpt-oss` 系）——不覆盖（词干双匹配必然 fail-closed），保留给矩阵；实现期与维护者确认默认开集合。
