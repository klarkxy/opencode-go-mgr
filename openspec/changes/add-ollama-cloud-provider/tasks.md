## 1. 域层：身份、Plan 与协议事实（fail-closed）

- [x] 1.1 `crates/ocg-domain/src/ids.rs`：新增 `OLLAMA_PROVIDER_ID`/`OLLAMA_CLOUD_OFFERING_ID` 常量与固定源/路径常量（`https://ollama.com`、`/v1/chat/completions`、`/v1/models`、settings 页 URL），含单元测试
- [x] 1.2 `crates/ocg-domain/src/provider.rs`：`ProviderAdapterKind` 增加 `OllamaCloud` 变体（`ALL`、`from_offering` 同步穷举），`BUILTIN_PLANS` 增行（凭据 Bearer、`upstream_protocols` 仅 Chat、unpriced、`usage_availability = LocalState`、`protocol_probe_supported = false`），offering 初始 `routable=false`，启用写入被拒的契约测试与 probe=false 断言
- [x] 1.3 `crates/ocg-domain/src/protocol.rs`：以 GOAT 式**家族独立协议种子**登记 Ollama Cloud 仅 Chat 事实（注明验证依据）——**严禁写入 `MODEL_PROTOCOLS`**（该表经 `supported_model_ids()` 派生 Go 已发布别名，会造成 Go 路由伪造）；表内容锁定测试（行数与内容）
- [x] 1.4 运行 `cargo test -p ocg-domain` 全绿

## 2. 共享别名映射与目录解析（快照 id 不进代码）

- [x] 2.1 `crates/ocg-gateway/src/alias.rs`：新增剥 `:` 标签的词干派生 + 目录单匹配守卫（复用 GOAT 的 `matches.len() == 1` 语义）；零/多匹配时**仅本家族映射**不可路由，同名别名由 Go 等既有家族照常服务（不产生 400、不扰动既有发布）
- [x] 2.2 共享别名追加：`deepseek-v4-flash`、`deepseek-v4-pro` 为 Go 已拥有别名，本家族只向其候选映射集追加（overlay 同款机制），不创建/抢占别名；断言源代码中不存在任何带日期标签的快照 id（守卫测试）；尺寸变体 `gpt-oss:20b`/`:120b` 共存不产生词干别名
- [x] 2.3 模型矩阵管理员钉定数据形状（选择域 = 发现的精确 id）与刷新尊重语义的持久化测试
- [x] 2.4 场景测试：轮换重绑（`:0731`→`:0915` 自动重绑）、新旧并存时本家族映射不可路由且别名仍由 Go 服务、裸 id 请求 PinnedRaw 无跨账号回退、本家族不新增 `/v1/models` 发布项（发布闸门：既有别名发布保持不变）
- [x] 2.5 运行 `cargo test -p ocg-gateway` 全绿

## 3. Wire 规范化内核（尝试级触发）

- [x] 3.1 `AttemptSpec` 增加规范化标记字段（data-only，默认 None，仅 OllamaCloud adapter 置位）；规范化函数为 ocg-gateway 纯函数（Bytes→Bytes），宿主 forwarder 在 spec 解析后、`forward_once` 前按尝试改写 `plan.body`；`upstream_body_bytes` 日志记录改写后实际发送字节（contract 测试锁定）
- [x] 3.2 请求方向：assistant `reasoning_content` → `reasoning`（缺失才补、双空跳过）；混合候选链字节隔离契约测试（由 Go 尝试服务的请求与本家族不存在时字节完全一致）
- [x] 3.3 响应与 SSE 方向：`StreamConverter` 增加规范化输入；`message`/`delta` 的 `reasoning`|`thinking` → 补 `reasoning_content`（已有则不动）；`[DONE]`/非 JSON 行原样透传；SSE 逐行改写增量测试
- [x] 3.4 `max_tokens`/`max_completion_tokens` > 65535 单向钳制常量实现 + 边界测试（等于 65535 不动、缺失不动、其他家族不动）
- [x] 3.5 forward log 面向客户端呈现请求名、上游侧记录精确 id 与实际路由段（`route` 列）的归因测试；Cookie 绝不出现在推理出站的负向测试

## 4. Adapter 与宿主路由

- [x] 4.1 `crates/ocg-core/src/gateway/provider_adapter.rs`：`OllamaCloud` adapter 产出 `AttemptSpec`（固定源、Chat 路径、Bearer、no-redirect、`ProcessWideNoRedirect` 代理语义——入站快照选路、List 方向默认段，本家族模型 id 不参与例外段匹配），DB/解密/HTTP 留在宿主（纯度守卫通过）
- [x] 4.2 目录刷新走 GOAT 同款公开 `GET /models` 控制面路径（非 Key 验证），证据/`force_on` 复用现有 provider catalog 存储
- [x] 4.3 未知模型名在零上游调用前返回 400 的运行时测试（对齐 `v2_alias_runtime` 既有锁定）；共享别名经 Go 账号服务时计价归因为 Go、经本家族服务时 unpriced 的归因测试

## 5. Schema v34 与用量持久化

- [x] 5.1 `crates/ocg-core/src/db.rs`：`CURRENT_SCHEMA_VERSION = 34`；新表 `ollama_cloud_usage_state`（`account_id` PK + `FK accounts(id) ON DELETE CASCADE`、`status`、`snapshot`（仅成功写入）、`last_success_at`、`last_attempt_at`、`next_eligible_at`、`failure_streak`）与 Cookie 混淆存储位（现有 `.encryption-key` 设施）
- [x] 5.2 v33→v34 幂等迁移 + 手工构造 v33 库的迁移测试（非破坏、路由行为不变、pre-v3 备份策略生效）
- [x] 5.3 更新 `docs/maintainer/storage-migration.md`（+zh-CN）迁移说明

## 6. Cookie 用量服务

- [x] 6.1 Cookie 归一化校验模块：`name=value` 对、拒 Set-Cookie 属性、拒重名、拒 `$` 前缀、拒空值、≤16KB（逐条单元测试，与 spec 契约一致）
- [x] 6.2 抓取客户端：精确 URL 校验、不跟随重定向、15s 超时、512KB 上限、仅携带该账号 Cookie；出站经 `http_client.rs` facade / `configured_builder` 方向默认段（`plan_usage.rs` 先例，禁自建客户端）；可注入 clock/fetch 缝（对齐 `usage_sync` 模式），本地回环测试夹具
- [x] 6.3 DOM 锚点解析器：`data-usage-track/segment/model/requests/time/window` → 5h/7d 窗口（used_percent、reset_at）+ 按模型请求计数 + 可选 plan/balance；登录页 → `unauthorized`；解析失败 → `failed` 且**只更新状态/尝试元数据，不动 snapshot 列**；错误信息 ≤256 字符并过滤 HTML 片段与 URL 查询串（用本地化重制的净化 HTML 夹具）
- [x] 6.4 手动刷新端点：30s 节流（成功/失败都计）、并发去重、固定阶梯退避 5m→15m→1h→6h 封顶（不依赖响应头）；**零推理冷却写入**的负向测试
- [x] 6.5 生命周期测试：清除 Cookie → 状态/快照归零回到未配置态；账号禁用 → 手动刷新被拒绝；账号删除 → 状态行级联删除
- [x] 6.6 快照脱敏审计测试：持久化与 API 下发均不含原始 HTML/Cookie/会话字段

## 7. V3 契约、控制面与导出边界

- [x] 7.1 `dashboard_v3/types.rs` 增 DTO 入 `CATALOG_TYPE_NAMES`；Providers/Accounts 相关端点扩展（家族卡数据、账号 Cookie 配置写、用量查询读、禁用态入口拒绝），写操作全走 CAS
- [x] 7.2 `schema/dashboard-api-v3.schema.json` 与 `src/api/generated/dashboard-v3.ts` 经 `pnpm run contract:v3:generate` 重生成，`pnpm run contract:v3:check` 通过
- [x] 7.3 导出边界回归测试：用量快照与 Cookie（明文/密文）不出现在导出载荷，payload 结构不变、无版本 bump，旧 payload 双向可读；node-transfer 往返后目标节点该能力为未配置态
- [x] 7.4 `crates/ocg-core/tests/dashboard_v3_providers.rs`/`dashboard_v3_accounts.rs` 家族集成测试；`cargo test -p ocg-core` 全绿

## 8. 前端

- [x] 8.1 Providers 页 Ollama Cloud 家族卡（目录刷新、模型/协议矩阵复用现有控件；**无协议探测入口**）；Alias 聚合视图自然纳入本家族
- [x] 8.2 Accounts 表单：API Key + 可选 Cookie 粘贴框（校验错误就地显示；已配置仅显示状态与脱敏摘要）；用量格显示 5h/7d 窗口与手动查询按钮（30s 限速反馈、禁用态隐藏入口）
- [x] 8.3 `src/domain/` 契约类型与组件测试；i18n 全部 locale（en-US 为 MessageKey 源，zh-CN/zh-TW/ja-JP/ko-KR/es-ES/fr-FR/de-DE/pt-BR/ru-RU 补齐）通过 i18n 测试；`pnpm run test`、`pnpm run build:web`、`pnpm run design:lint` 全绿

## 9. 文档与收尾

- [x] 9.1 `docs/user/providers.md`/`accounts.md`（+zh-CN）家族事实；`docs/maintainer/runtime-invariants.md`（+zh-CN）不变量增补（共享别名候选家族加入 Ollama、快照 id 不进代码、用量失败不写冷却、导出边界不变）
- [x] 9.2 实现期协议验证记录：真实 Key 验证 `/v1/models` 免鉴权可达性（若需鉴权则目录刷新改认证模式并重评启用位）、裸名请求是否等价 `:latest`、reasoning/thinking 实测字段名、65535 钳制实测，回填协议种子注释与 USER 文档
- [x] 9.3 全量回归：`cargo test -p ocg-domain && cargo test -p ocg-gateway && cargo test -p ocg-core`、相关 crate `cargo clippy`（对齐 CI quality.yml 目标）、`pnpm run test`、`pnpm run build:web`
