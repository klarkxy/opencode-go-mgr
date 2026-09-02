## Context

当前网关只有一个客户端鉴权 key：`AppConfig.gateway_key` 字符串，随整个 config 序列化存入 SQLite `settings` 表（键 `"config"`）；唯一校验点是 `check_auth()`（`gateway/handler.rs:804`），在中间件与各协议入口共 7 处调用；`forward_logs` 表只记录上游账号（`account_id/account_name`），不记录客户端 key。动机见 proposal.md，需求契约见 specs/。

规模约束：`forward_logs` 是最大的表，且 `diagnostic_json` 30 天后被清理，行数有上界（本地单机工具量级，但需按"可能很大"设计）。数据全部存在本机 SQLite，无外部依赖。

## Goals / Non-Goals

**Goals:**
- 多 key 全生命周期管理（创建/改名/启停/重新生成/软删除），向后兼容现有单 key 客户端。
- 鉴权多 key 匹配且能返回"命中的 key id"供日志使用。
- forward_logs 按 key 记录与查询；历史数据安全回填（幂等、可恢复、不阻塞网关）。
- 发布渠道（桌面/CLI/Docker）升级无感，可回滚。

**Non-Goals:**
- 不为 key 引入加密存储（与现状一致，本地可信环境；管理面已有 dashboard 会话鉴权）。
- 不做 key 用量配额/限流（本提案只做记录与查询）。
- 不提供硬删除（彻底删除）入口；软删除是唯一删除语义。
- 不做旧 key 值的时间线精确归因（旧 key 值无留存，见 Decisions - 回填）。

## Decisions

### D1: 存储 —— `gateway_keys` 列表放进 AppConfig，保留 `gateway_key` 兼容字段
`AppConfig` 新增 `#[serde(default)] pub gateway_keys: Vec<GatewayKeyEntry>`（`id`、`name`、`key`、`enabled`、`deleted_at: Option<DateTime<Utc>>`、`created_at`），`gateway_key: String` 字段**保留且始终镜像主 key 的值**。

- 理由：serde `#[serde(default)]` 让旧 config JSON 缺字段时自动为空列表，无需 DB 迁移；保留 `gateway_key` 使旧二进制降级时仍能读到 key（旧二进制反序列化会忽略未知字段），回滚无感；外部依赖 `/gateway/status.key` 或 CLI 输出的消费者不受破坏。
- 备选：独立 `gateway_keys` 表。否决：config 已是 JSON 整体存取、体量极小，建表带来双写与迁移复杂度，收益为零。

### D2: 主 key 与"至少一个启用 key"不变量
主 key = 列表首条（迁移时由旧 `gateway_key` 生成，`name = "Primary"`）。任何删除/停用操作后必须仍有 ≥1 个启用 key，否则网关无可接受凭据。**删除主 key 是允许的**（其余 key 存在时）：删除后剩余启用 key 中最早创建者提升为新主 key，`gateway_key` 镜像随之更新，历史日志仍指向旧主 key id（软删除保留记录，归因不丢）。

### D3: 鉴权收口 —— `extract_client_key_id()` 单一入口
把 `check_auth` 重构为：遍历启用且未删除的 key，匹配则返回 `Some(key_id)`，否则 `None`。现有 7 处调用点全部改走新 helper，**不再各自比对字符串**，消除漏改风险。`Bearer`、`x-api-key`、`x-goog-api-key` 三种携带方式继续支持。

### D4: key 删除 = 软删除，删除即停用且清除明文
删除仅设置 `deleted_at` 并**同时置空 `key` 明文值**（保留 id/name/deleted_at 供归因），该 key 立即从鉴权集合剔除；记录永久保留，日志按 id 可解析到具名 key。禁用 = `enabled=false`（记录可见、不可鉴权，值保留）。理由：规避"日志归因悬空"，同时不让已删除 key 的明文继续暴露在 `/settings`、`/gateway/status` 上；与 accounts 的日志快照模式互补（见 D6）。

### D5: forward_logs 加列（schema v18，幂等）
`ensure_column(forward_logs, "client_key_id", "TEXT")` 与 `ensure_column(forward_logs, "client_key_name", "TEXT")`（复用 v14 的幂等模式），加 `idx_forward_logs_client_key` 索引。旧二进制 SELECT 明确列名，多出的列被忽略，降级安全。历史行 `client_key_id IS NULL` 表示"未归因（升级前/待回填）"。

### D6: 日志写入时快照 key 名称（对齐 accounts 惯例）
`log_forward` 写入 `client_key_id` + `client_key_name`（写入时刻的 key 名）。理由：与现有 `account_name` 快照模式一致，Logs 页无需 JOIN，且为将来可能的清理/彻底删除留余地。代价：改名不回溯旧日志——与 accounts 现状一致，可接受。

### D7: 客户端 key 从鉴权点穿到日志写入
`proxy_handler_inner` 鉴权后把 `Option<key_id>` 沿参数链传入：`execute_plan` → `forward_request`（forwarder.rs:174）→ `forward_request_impl`，在每次 attempt 新建 `ForwardAttemptContext`（forwarder.rs:217）时写入其新字段 `client_key_id`；`log_forward()` 据此写 `client_key_id/client_key_name`（由 id 反查启用/软删除 key 得名称，查不到则为 `None`）。未认证请求早退，天然不产生 forward 日志行。注意 `ForwardAttemptContext` 按 attempt 重建，key id 必须作为入参贯穿，不能只挂在 handler 层。

### D8: 大表安全回填 —— 分块、幂等、断点续跑、后台、不阻塞转发
回填目标：迁移时的主 key id（**近似归因**，见 Risks）。实现：
- **触发点**：主 key id 在 `load_config` 中才确定，但 `load_config` 是纯同步函数且 `CoreStateInner` 构造在无 tokio runtime 的同步上下文（Tauri 入口）。回填任务在**构造完成后的 runtime 上下文**启动，实现收敛为挂在 `start_gateway_on`（桌面 setup、CLI main、端口变更重启的唯一收口，未来新增调用点不会漏跑）。原计划分别挂 Tauri `setup` hook / CLI async main 以避免"测试直接调用 `start_gateway_on` 时 spawn 线程与临时目录清理竞争"，实现期用两层缓解取代：①首块内联——空表/小表在启动调用栈内同步完成并写完成标记，不 spawn 任何线程（绝大多数测试与全新安装永不产生后台线程）；②大表才 spawn 专用 `std::thread`，且只持 `Weak` 引用，state 被 drop 后 `upgrade()` 失败线程立即退出，不会与临时目录清理竞争存活。DB 操作为同步，用 `std::thread` 承载。纯同步构造（含单测）下不启动回填、不 panic。**不回填在 v18 schema 迁移里**（那里还没有主 key id）。
- **分块**：按 `rowid` 区间（如每块 50k 行，常量可调）执行 `UPDATE forward_logs SET client_key_id=?, client_key_name=? WHERE client_key_id IS NULL AND rowid BETWEEN ? AND ?`，每块独立事务、块间让出 CPU/`sleep`。约束：**持 `state.db` 锁期间不得 `.await`**；主 key id 与名称须在取锁前从 config 快照读取，避免锁序问题。块执行期间已认证请求的 `log_forward` 会短暂排队（本地 SQLite 每块几十毫秒量级），未认证请求不受影响——表述为"转发本身不阻塞，日志写入短暂等待"。
- **幂等/可恢复**：条件恒为 `client_key_id IS NULL`；另在 settings 表记录已回填最大 rowid（`backfill_forward_logs_client_key`），启动从断点续跑，避免大表每次启动全表扫描。终止条件：一轮扫描无 NULL 行即完成（写入路径保证新行必带 key id，NULL 集合单调收缩）并记录完成标记。表较小（< 阈值）时等效于一次全量 UPDATE。

### D9: API 与前端
- 新端点：`POST /settings/keys`（创建，返回完整 key 值仅此一次）、`PATCH /settings/keys/{id}`（改名/启停，拒绝空 name）、`POST /settings/keys/{id}/regenerate`（返回新值）、`DELETE /settings/keys/{id}`（软删除并清除明文）。全部走 `settings_update` 锁 + `settings_revision` 乐观锁，与现有 `update_settings` 并发安全；**每个变更操作写 gateway_logs 审计条目**（对齐账号生命周期操作的既有惯例）。**保留旧端点 `POST /settings/regenerate-gateway-key`**（内部收敛为"重新生成主 key"，避免双实现与镜像不同步）。`GatewayKeyEntry.id` 用 UUID v4 字符串（与 account id 风格一致）。
- **不变量收敛到 `set_config`（state.rs:409）**：任何写入路径（`update_settings`、key API）最终都经过它，统一强制"主 key 非空、≥1 启用 key、`gateway_key` 镜像主 key 值"。特别是 `update_settings` 必须把收到的 `gateway_keys` 替换为当前配置的 keys（`config.gateway_keys = previous_config.gateway_keys.clone()`，与 `claude_desktop_models` 的既有处理一致），防止前端/外部 POST `/settings` 缺字段时 `serde(default)` 把 keys 清空导致全线 401。
- `GatewayStatus` 增加 `keys: Vec<GatewayKeyEntry>` 与 `primary_key_id`，**保留 `key` 字段**（= 主 key 值）以兼容外部消费者；注意该接口由此暴露所有 key 明文（含软删除记录），仅限 dashboard 会话保护下访问；前端 `tauri.ts` 同步类型。
- Logs 查询：`ForwardLogQueryOptions` 增加 `key_id`（`__unattributed__` 特值选择 `client_key_id IS NULL` 行）。**key 筛选下拉的数据来源为新的 `/logs/forward/keys` 端点**（`SELECT DISTINCT client_key_id, client_key_name FROM forward_logs`，镜像 `/logs/forward/models` 模式），而非 config keys——这样下拉恰好覆盖"有日志记录的全部 key（含已停用/已软删除/降级产生的悬空 id）"，与日志中的 `client_key_id` 一一对应，保证已删 key 的历史日志可筛选可归因。
- `/settings` 的 key 字段语义变化需文档化：POST `/settings` 携带的 `gateway_key/gateway_keys` 从此被忽略（key 只能经 key API 管理）；`update_settings` 保留"gateway_key 非空"最小守卫（trim 后非空即可），防止空/残缺 JSON 把其它字段静默重置为默认值。
- `settings-merge.ts`：key 字段继续排除在可编辑列表外（key 只走独立管理 API），维持"key 永远来自服务器"原则，并补一条测试确认 `gateway_keys` 不在 `EDITABLE_SETTING_KEYS`。
- Settings 页新增"接入 Key"管理区（列表/增删改/启停/重新生成）；Dashboard 接入中心 Key 行改选择器 + 复制 + 单 key 重新生成；Applications 指南复制主 key。

### D10: CLI 与发布
`ocg-cli` 状态输出改为展示主 key（值不变，纯展示层调整）。Docker/桌面/CLI 共用同一 SQLite 格式与 config JSON；schema 变更仅为加列，config 变更由 `serde(default)` 兜底，所有渠道升级路径一致，无渠道差异化迁移。

## Risks / Trade-offs

- **[轮换过旧 key 的用户，历史段归因近似]** 旧 key 值无留存，回填只能归到迁移时主 key → UI 注明"升级前用量统一计入主 Key"；新数据严格精确，且随使用时间推移历史段占比自然下降。
- **[降级会丢失次级 key]** 旧二进制 `load_config` 的规范重写机制（`needs_persist` 比较序列化结果）会把不含 `gateway_keys` 的 config 写回存储，升级后创建的次级 key（含密钥值）在降级时被抹除，仅 `gateway_key` 镜像值（主 key）幸存；降级再升级后主 key 是新 UUID，历史日志 `client_key_id` 成为悬空 id → 降级路径必须文档化为显式预期（"回滚后主 key 保留、次级 key 丢失"），Logs 页对悬空 id 兜底展示"已删除/未知"，且不建议在生产回滚后继续依赖多 key 数据。
- **[多进程并发启动 → SQLITE_BUSY]** SQLite 连接未设 `busy_timeout`（db.rs 无 PRAGMA），旧进程写库时新进程 `migrate()` 立即失败 → 文档化"升级前先停止旧进程"；可选在 `Database::open` 设 `busy_timeout`（如 5s）彻底消除。失败不损坏数据，自愈 = 停旧进程重启。
- **[超大表首次启动延迟]** `CREATE INDEX idx_forward_logs_client_key` 与既有 30 天清理都在 `migrate()` 单事务内，全表扫描；超大表首次启动会持锁数秒 → 9.7 大表演练量化；Docker healthcheck 窗口需覆盖。
- **[超大表回填期间的并发]** 后台分块 UPDATE 与网关日志写入并发 → 每块独立短事务 + 块间让出 + 断点续跑；已认证请求日志写入短暂排队（毫秒级），未认证请求不受影响；日志查询对 NULL 行走"未归因"展示，语义明确。
- **[check_auth 重构引入鉴权回归]** 7 处调用点 → 单 helper + 既有单 key 测试全部保留 + 新增多 key 匹配/停用/删除/三种头矩阵用例。
- **[GatewayStatus/配置结构变化破坏外部消费者]** → 保留 `key` 字段与旧 config 字段；前端同版本升级；CLI 输出值不变。
- **[软删除记录累积]** 本地工具量级可忽略；将来如需可加清理策略（与日志快照列 D6 配合，不损失归因）。
- **[回填失败/未完成时的数据状态]** → 幂等可恢复 + "未归因"过滤可见，用户无数据丢失风险。

## Migration Plan

1. **部署**：schema v18 加列与索引（幂等，`migrate()` 单事务）→ 新二进制启动时 `load_config` 生成主 key 并持久化 → 构造完成后（Tauri setup / CLI async）后台启动回填 → 网关照常服务。
2. **升级路径**（全渠道一致）：桌面更新安装、CLI 换包、Docker 换镜像，均只读既有 `data.sqlite`；旧 config 经 `serde(default)` 自动获得空 `gateway_keys`，随后被迁移为主 key。升级前先停止旧进程（尤其 CLI/Docker，无进程锁）。
3. **回滚**：降级旧二进制——`gateway_key` 镜像值使旧客户端继续鉴权，新增列被旧二进制忽略，数据不损坏。**显式预期**：升级后创建的次级 key 在降级时会被旧二进制的 config 规范重写抹除（仅主 key 值幸存），且降级再升级后主 key 为新 id、历史日志 `client_key_id` 悬空（Logs 页兜底展示"已删除/未知"）。回滚仅建议在未创建次级 key 时使用。
4. **上线节奏**：阶段一（D1-D4 + D9 的 settings/dashboard 部分 + D10）先合入，多 key 即可用；阶段二（D5-D8 + Logs 页）以阶段一为基础（依赖其鉴权返回 key id 与主 key id），**不得先于阶段一合入**，但可独立回滚、互不阻塞发布窗口。

## Open Questions

无（设计决策均已收敛，可后置的仅为 UI 文案措辞，不影响 spec/任务拆分）。
