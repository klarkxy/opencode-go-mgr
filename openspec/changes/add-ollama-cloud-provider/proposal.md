## Why

Ollama Cloud（`https://ollama.com`，OpenAI 兼容、仅 Chat Completions）是用户需要的下一个上游远端。上游 v2.1.0 的 Custom API 模型映射（schema v33，提交 aed05bc）已可覆盖其转发与公开名别名（本仓库子代理独立验证结论 FEASIBLE），但 Custom 账号有两个结构性盲区无法表达：一是 Ollama Cloud 的 wire 协议怪癖——思维链放在 `reasoning`/`thinking` 字段而 DeepSeek/OpenAI 客户端只认 `reasoning_content`，且 `max_tokens` 超过约 65535 会被上游直接 400；二是官方没有用量 JSON API，用量信息只能携带浏览器 Cookie 抓取 `https://ollama.com/settings` 页面。这两件事都属于"产品级协议事实与账号语义"，按 `docs/user/add-provider.md` 的判定标准必须走密封内置 Provider，而不是继续加厚 Custom。

## What Changes

- **新增第七个密封 Provider 家族 `OllamaCloud`**：`ProviderAdapterKind` 扩展 `OllamaCloud` 变体（`ALL` 数组与 `from_offering` 同步穷举），固定源 `https://ollama.com`，上游协议仅 Chat Completions（Bearer 鉴权，跟随现有 no-redirect 约束）。
- **Wire 协议规范化（adapter 内固定行为，无用户开关，按尝试生效）**：
  - 请求方向：assistant 消息的 `reasoning_content` 复制到 `reasoning`（若两者皆空则跳过）；
  - 响应与 SSE 流方向：`message`/`delta` 中的 `reasoning` 或 `thinking` 非空且无 `reasoning_content` 时补写 `reasoning_content`；
  - `max_tokens` / `max_completion_tokens` 超过 65535 时单向钳制到 65535（provider 级硬上限，与模型无关）；
  - 规范化仅作用于 Ollama Cloud 尝试：同一请求的候选链混合其他家族时，非本家族尝试的请求字节必须与本家族不存在时完全一致。
- **目录与别名（Go 优先，共享别名映射追加）**：公开 `GET /models` 作为目录发现（对齐 GOAT 模式，以免鉴权可达为实现前提）；目录精确 id（含尺寸标签、日期标签）原样登记为 raw pin；首期词干 `deepseek-v4-flash`、`deepseek-v4-pro` 均为 Go 已拥有的已发布别名，本家族**不创建、不抢占**任何别名，唯一贡献是目录刷新时按"剥标签词干单匹配"守卫向既有别名追加一个可路由映射（参与既有跨账号回退，归因与计价跟随实际服务家族）——**带日期标签的快照 id 严禁写进代码**，多匹配时仅本家族映射降级不可路由、由模型矩阵人工钉定。
- **Cookie 用量（opt-in，手动优先）**：账号级粘贴浏览器 Cookie 头（严格校验：必须为 `name=value` 对、拒绝 Set-Cookie 属性格式、拒绝重复名与 `$` 前缀、≤16KB），以与现有 Key 同级的混淆设施存储（明示非密码学 AEAD）；抓取固定 `GET https://ollama.com/settings`（严格 host+path 校验、不跟随重定向、15s 超时、响应 ≤512KB、经全局出站代理默认段）；解析 DOM 锚点 `data-usage-track`/`data-usage-segment`/`data-model`/`data-requests`/`data-time`，产出 5 小时/每周窗口（used_percent + reset_at）与每模型请求数、plan 名、余额；登录页 HTML 判定为 `unauthorized`。手动刷新 30s 限速；固定阶梯失败退避（5m→15m→1h→6h）；**不做请求驱动的自动刷新**（与 GOAT/MiniMax/Kimi 的手动用量语义对齐）；**用量失败永不写推理冷却**；快照脱敏（不含原始 HTML 与 Cookie）。
- **Schema v33 → v34**：Cookie 混淆存储与用量状态落账号侧新表；迁移路径与 pre-v3 备份策略遵循 `storage-migration.md`。
- **V3 控制面与 UI**：Providers 页新增 Ollama Cloud 家族（目录刷新、模型/协议矩阵沿用现有契约；固定仅 Chat 家族无协议探测入口，`protocol_probe_supported = false`）；账号卡含 API Key 与可选 Cookie 配置、手动查询用量；**导出/导入载荷不变**（用量快照与 Cookie 属现行导出省略边界，不扩 payload、不 bump 版本）。
- **fail-closed**：在路由与控制面完整就绪前，该家族所有 offering 保持不可启用；未验证协议事实不进 `protocol.rs`。
- **非目标**：不做请求驱动的自动用量刷新；不做非 ollama.com 源的泛化；不做用户自定义 wire 规范化开关；不引入动态 Provider/插件点；Custom API 映射路径保持现状不变（两者并存，Custom 仍是自由命名出口）。

## Capabilities

### New Capabilities

- `ollama-cloud-provider`：Ollama Cloud 密封适配器能力契约——身份与固定源、仅 Chat Completions 的协议事实（含 reasoning/max_tokens 规范化的精确触发与混合链字节隔离）、目录发现与共享别名映射追加（快照 id 不进代码、单匹配守卫、不抢占 Go 已发布别名）、转发改写与冷却/错误/unpriced 记账语义。
- `ollama-cloud-usage`：Cookie 用量能力契约——Cookie 配置校验与混淆存储、settings 页抓取边界（URL/超时/体积/重定向/代理默认段）、DOM 解析锚点与脱敏快照、手动刷新限速与固定阶梯退避、用量与推理冷却的隔离、生命周期状态与导出边界。

### Modified Capabilities

（无——`openspec/specs/` 无主规格，本能力全新定义。）

## Impact

- `crates/ocg-domain`：`ids.rs`（Provider/Offering 常量）、`provider.rs`（`ProviderAdapterKind` 穷举、目录事实、BUILTIN_PLANS）、`protocol.rs`（已验证协议行）。
- `crates/ocg-gateway`：`alias.rs`（词干别名与目录解析、单匹配守卫）。
- `crates/ocg-core`：`gateway/provider_adapter.rs`（`OllamaCloud` adapter → `AttemptSpec`）、wire 规范化（复用 `ocg-gateway` 转换内核）、`dashboard_v3`（providers/accounts 契约扩展 + `schema/dashboard-api-v3.schema.json` + `src/api/generated/dashboard-v3.ts` 重生成）、`db.rs`（v34 迁移）、用量服务新模块（参照 `go_usage.rs`/`usage_sync.rs` 的节流模式）。
- `src/`：Providers 页家族卡片与矩阵、Accounts 表单（Key + Cookie）、`src/domain/` 契约类型。
- 文档：`docs/user/providers.md`/`accounts.md`（+zh-CN）、`docs/maintainer/runtime-invariants.md`、AGENTS.md 若有涉及。
- 测试门槛：`cargo test -p ocg-domain`、`cargo test -p ocg-gateway`、`cargo test -p ocg-core`、相关前端测试、`pnpm run build:web`、`pnpm run contract:v3:check`。
