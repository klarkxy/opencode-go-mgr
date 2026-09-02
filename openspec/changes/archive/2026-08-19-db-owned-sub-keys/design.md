## Context

PR #43（未合并）在 `AppConfig.gateway_keys` 内嵌 key 列表实现了多 Key，评审证明该存储选址带来四类结构性问题（见 proposal Why），且与上游 v1.6.2 的 config 瘦身路线冲突。上游 v1.6.2 已含：`Database::open` 的 WAL + busy_timeout、Dashboard 去除 `ref<AppConfig>`、`update_settings` 的 request-id 防抖等。本变更在 PR 分支上重构：回滚 config 内嵌形态，改为"主 Key 遗留标量 + 子 Key 独立表"。`forward_logs` 的 v18 归因列、分块回填、Logs 按 Key 过滤、审计条目、i18n 文案等资产保留复用。

## Goals / Non-Goals

**Goals:**

- 凭证存储选址正确：遗留标量留 config，列表型凭证住独立表，升/降级矩阵全绿
- 关键约束各有结构归宿：主 Key 约束进 `AppConfig::validate`，子 Key 约束由表约束 + API 强制
- 消除与上游 config 瘦身路线的冲突面（Dashboard 不再持有完整 settings 形状）
- 鉴权恢复并保持 v1.6.1 的多候选头 OR 语义，热路径不再随 AppConfig 整份 clone

**Non-Goals:**

- 不做子 Key 级别的用量配额或限速（仅归因统计）
- 不为曾运行 PR #43 构建的开发库做数据迁移（未发布形态，直接忽略丢弃 config 内 `gateway_keys`）
- 不改 CLI 的 `key`/`status` 行为（继续走主 Key）
- 不引入远端同步或跨节点 Key 分发

## Decisions

### D1: 双层凭证模型与存储选址

主 Key = `AppConfig.gateway_key`（v1.6.1 语义原样：settings 可自定义、regenerate 端点可轮换、永不禁用/删除）。子 Key = 新表 `sub_gateway_keys`，只经 Key 生命周期 API 变更。

- 为什么不用"全部进表"（含主 Key）：主 Key 是遗留契约（CLI、旧客户端、降级二进制都读 config 标量），移入表反而制造新的兼容层。
- 为什么不用"全部进 config"（PR #43 现状）：即本变更要修复的四类问题；且 config 形态一旦发布即成永久兼容负担，未发布是唯一零成本窗口。
- 主 Key 归因标识取**硬编码 UUID 常量**（实现时定值，如 `PRIMARY_KEY_ID`；形态选**可识别的固定式样**（如 `00000000-0000-0000-0000-000000000001`），与生成的 v4 UUID 及 nil 均视觉可区分）。约束是"**发布后保持稳定、直至显式迁移**"而非永久占用：未来若需把主 Key 重设计为可管理对象，可用一条分块索引 UPDATE（`WHERE client_key_id = PRIMARY_KEY_ID`，与回填同类机制）把历史行重归因——属可回收债。与子 Key 的随机 UUID 共享同一 id 命名空间，仅靠常量集中定义标识身份。主 Key 名称快照固定 **"Primary"**（UI 侧用既有 i18n "主 Key"），不可重命名——重命名只影响展示，价值低而状态面增加；且 /keys 的"按 id 取最新名称"机制天然兼容未来放开重命名。

### D2: schema v19 —— `sub_gateway_keys` 表

```sql
CREATE TABLE sub_gateway_keys (
  id TEXT PRIMARY KEY,            -- UUID v4
  name TEXT NOT NULL,
  key TEXT NOT NULL,              -- 明文；软删除时置空
  enabled INTEGER NOT NULL DEFAULT 1,
  deleted_at TEXT,                -- 软删除标记
  created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_sub_gateway_keys_key ON sub_gateway_keys(key) WHERE deleted_at IS NULL AND key <> '';
```

- 软删除保留 id/name/deleted_at、清空明文（延续归因语义，与 PR #43 相同）。
- 部分唯一索引兜底**子 Key 之间**的值唯一（活跃值不得重复）；**跨层唯一**（主 Key 值 ↔ 子 Key 值）由 API 层强制，且必须覆盖**任何非删除**子 Key（含已禁用未软删者，其明文值保留）：`POST /settings` 提交的 `gateway_key` 不得等于任何非删除子 Key 的值；子 Key 的 create/regenerate 生成循环避开主 Key 当前值；**set_key_enabled 启用路径**增加"该值 ≠ 主 Key 当前值"校验——否则「禁用 S → 主 Key 设为 S 的值 → 重新启用 S」序列可绕过全部校验，快照内同一值双重归因。**三条规则收口为单一闸口 helper**（集中"候选主 Key 值 ↔ 非删除子 Key 值集合"的互斥检查，`dashboard.rs` 与 Tauri `update_settings_inner` 复用），避免规则分散漂移。数量上限（≤64 活跃）同样只在 API/CRUD 层强制（schema 不承载计数约束）。
- v18 的 `forward_logs.client_key_id/client_key_name` 与回填机制不动，回填目标值从"主 key UUID"改为常量 `PRIMARY_KEY_ID`（硬编码 UUID，非字符串 "primary"）。
- 墓碑不计入 64 上限（活跃 = `deleted_at IS NULL`）。

### D3: 鉴权 —— 候选头收集 + OR 语义 + 内存快照

```
候选值 = [Bearer 值, x-api-key, x-goog-api-key].filter(非空)   // 固定顺序
命中 = 候选值中任一 ∈ 快照
快照 = RwLock<HashMap<value, (id, name)>>，含主 Key（id = PRIMARY_KEY_ID, name = "Primary"）
       与全部启用未删子 Key；鉴权与日志写入（client_key_name 快照）共用同一份
```

- 恢复 v1.6.1 "任一匹配即通过"语义，修复"错误 x-api-key + 正确 x-goog-api-key 被拒"。安全等价：所有候选都是客户端主动出示的凭证声明，多出示一个错误值不构成降权理由。与 v1.6.1 的已知微差：v1.6.1 整头精确匹配 `Bearer {key}`，本实现 strip_prefix + trim 对多余空格更宽容——无害，接受。
- **归因顺序定案**：以**候选头固定顺序**（Bearer → x-api-key → x-goog-api-key）为首要，首个命中即归因。"主 Key 先于子 Key"仅指**快照构建次序**（单值查找时主 Key 条目先插入）——跨层唯一保证同一值不可能同时命中主 Key 与子 Key，两种表述实际等价。多候选同时命中不同凭证时结果确定。
- **主 Key 值一并纳入快照**（启动时与每次 `set_config` 后刷新其中主 Key 条目）：鉴权热路径完全不读取 AppConfig、不取 config 锁——中间件/纯鉴权路径（PR #43 每请求整份 clone 的来源）随之消失；代理转发路径的 `upstream_context()` 整份 clone 为既有行为，不在本变更范围。
- **日志写入的名称快照同源**：`forwarder.rs::set_client_key` 现状从内存 config 解析 `key_name`（子 Key 移 DB 后断源）——改为经快照按 id 取 name（快照已携带 `(id, name)`），主 Key 恒 "Primary"；写入路径不查 DB、不取 config 锁。
- 快照失效模型：子 Key 只经 Key API 变更（持 `settings_update` 锁，同步重建）；主 Key 经 `set_config`（regenerate、settings 自定义）刷新。**外部直接改 SQLite 表/config 存储的边缘情况按决策忽略**——必须在快照构建代码处留下注释，说明"表仅由 Key API 写入、config 仅经 set_config 写入，外部改动不在失效模型内，重启前不生效"。

### D4: 约束归宿

- 主 Key："非空（trim 后）"**新增**入 `AppConfig::validate`（models.rs 的该函数现状只校验 timeouts/proxy/invite/claude 模型，**不含** gateway_key——非空守卫目前仅在 `update_settings` 的 payload 层，这正是 PR #43 评审意见 ① 的实锤；入 validate 后 `set_config` 统一强制）。与 `load_config` 的兼容：空 config 的自动铸新发生在 validate 之前，铸新后必非空，两者不冲突；`set_config` 显式拒绝空值。另加跨层约束：提交值不得等于任何活跃子 Key 值（见 D2）。"永不禁用"由**不存在该操作**保证（API 面上主 Key 无启停/删除）。
- 子 Key：名称非空且 ≤64 字符、活跃数量 ≤64、值唯一（表内部分索引兜底 + 生成循环避开主 Key 值 + **启用路径校验值 ≠ 主 Key 当前值**）、墓碑无明文——API 层校验。
- routing reset 接线：子 Key API 不走 `set_config`，撤销类子 Key 变更（禁用/软删/轮换）由端点**显式调用 `routing.reset()`**；改名、新增、启用不触发（与 PR #43 修复后的语义一致）。主 Key 值变化仍由 `set_config` 原有逻辑触发。
- 删除 PR #43 的 `gateway_keys::normalize/validate` 对 config 的整套不变量（镜像一致、≥1 启用、提升、防清空覆盖）——不再存在需要维护的 config 列表不变量。

### D5: API 面

- 子 Key 生命周期端点沿用 `/dashboard/api/settings/keys` 路径族（POST 创建 / PATCH 改名启停 / POST `{id}/regenerate` / DELETE 软删除），语义改为操作 `sub_gateway_keys` 表；全部持 `settings_update` 锁 + 可选 `expected_revision`（沿用 settings_revision，操作成功后 bump）+ 审计（rename 记旧名与新名）；撤销类操作后显式触发 routing reset（见 D4）。
- `POST /settings` 恢复 v1.6.1 的 `gateway_key` 可设置（trim + 非空守卫 + **不得等于任何活跃子 Key 值**，Tauri `update_settings_inner` 同步该校验）；payload 中出现的任何子 Key 字段一概忽略且不生效（老客户端载荷本就不含）。
- **`ConnectionInfo` 定案为新端点 `GET /dashboard/api/connection`**：`{ gateway_port, client_root_url, upstream_base_url, primary_key, sub_keys: [{id,name,enabled,value}], revision }`——供 Dashboard 接入中心与切换器。**子 Key 携带完整 `value` 明文**（与 `primary_key` 明文、`/settings` 现状同层）：切换器的掩码预览在前端本地计算（`maskConnectionKey`，现状行为），复制动作需要完整值；"创建返回完整值仅一次"约束只约束 Key API 的**创建响应**，不限制受保护端点的持续暴露（spec 明文保护 requirement 已列举 connection info response 为合法明文出口）。`GatewayStatus` 回滚为遗留形状（移除 PR #43 增加的 `keys`/`primary_key_id`，保留 `key` 字段），两者分工：前者是接入中心专用聚合视图，后者维持状态语义。**主 Key 明文暴露面清单**（均为 v1.6.1 既有或本变更新增，处于**同一 dashboard 会话保护层**——会话 Cookie 或回环 local-mode，无转发头）：`GET /settings`（既有）、`GET /gateway/status`（既有遗留面，前端无消费者）、`GET /connection`（本变更新增，Dashboard 从 `/settings` 切换而来）。`/gateway/status` 的 `key` 字段当前无任何前端消费者，其下线属遗留清理，**另立变更**处理，不在本范围膨胀。
- `regenerate-gateway-key` 遗留端点保留，收敛为轮换主 Key（Tauri command 同步）。

### D6: `/logs/forward/keys` 最新名称

按 id 分组取最新名称快照（`GROUP BY client_key_id` + 按 `rowid`/写入时间取最新 `client_key_name`），替代 PR #43 的 `ORDER BY MAX(client_key_name)`（字典序最大旧名 bug）。列表保持**纯日志驱动**（只返回 DB 中实际出现过的 id，与 PR #43 行为一致，不合成常量条目）：主 Key 一旦产生日志行即出现（其行内名称快照为 "Primary"），无行时不显示——过滤一个零行的 Key 本就无意义。

### D7: 前端与命名约束

- **命名（DESIGN.md 强制，有边界）**：本变更触及的用户可见面——dashboard UI 文案、日志页可见的审计消息、**dashboard API 校验错误串**——只称 **"Key"**，不出现 "Gateway Key" 字样；审计消息措辞用 "created key `Laptop`"、"renamed key `Old` to `New`"、"regenerated primary key" 等（不带 gateway）；`update_settings` 的 "gateway key is required" 校验错误串顺带改为 "key is required"（零成本合规，无测试断言旧串）。**范围外**：CLI stdout（`gateway key: ...`，v1.6.1 现状）、上游 401 错误串（"invalid gateway key"，v1.6.1 现状）——二者属协议/终端遗留消息，维持原样；如需覆盖另立变更。内部标识符（表名 `sub_gateway_keys`、能力名、端点路径）非用户可见，不受限。**Settings Key 管理区边界**：主 Key 行"只读 + 轮换"指 Key 管理区内无启停/删除操作；通用设置表单的 `gateway_key` 输入恢复可编辑（v1.6.1 语义），两处不矛盾。
- `Dashboard.vue`：以 `ConnectionInfo` 替代完整 settings；切换器默认主 Key、popover 交互、复制/单 Key 重新生成（子 Key 走 keys API，主 Key 走遗留 regenerate）全部保留（交互已是评审通过形态）。`assert.doesNotMatch(dashboard, /ref<AppConfig>/)` 重新成立。
- `Settings.vue`：Key 区分两层——主 Key 行（只读 + 轮换）、子 Key 行（改名/启停/轮换/软删）。`settings-merge.ts` 恢复 `gateway_key` 为可编辑键。
- `Logs.vue`：筛选与归因展示不变（数据源同形状）。
- i18n：PR #43 的 25 条 Key 文案基本复用，仅移除"主 Key 可停用"相关措辞（若有）。

### D8: 回滚清单（相对 PR #43 分支）

**删除**：`AppConfig.gateway_keys`/`GatewayKeyEntry` serde 字段；`GatewayStatus.keys/primary_key_id`（保留 `key` 字段）及 `src-tauri/src/commands/gateway.rs`、`setting.rs` 中对它们的构造（否则编译失败）；`gateway_keys.rs` 的 config 操作（normalize/validate/镜像/提升/墓碑-in-config）；`update_settings` 的 keys 防清空覆盖与"非 Key API 不可变"语义；state.rs 的 key 迁移与 routing-reset 的 key 值集合逻辑（改为：主 Key 值变化或子 Key 撤销类变更时 reset，见 D4 接线）。
**保留**：v18 列与回填（目标改 PRIMARY_KEY_ID）、回填 DONE 后 NULL 探测重启（本周期修复）、Logs 按 Key 过滤/汇总/未归因特值、审计、release 冒烟断言、大部分 i18n、前端切换器交互。
**重写**：`gateway_keys.rs` → DB CRUD + 快照管理；`dashboard.rs` keys 端点内部实现。

## Risks / Trade-offs

- [快照重建顺序产生鉴权窗口] → **硬性规则**：撤销类操作（禁用/软删/轮换）**先改快照、再提交表写**——活进程内最坏情况是 fail-closed（变更生效前旧值已被拒，可接受）；**表写失败时回滚快照**（恢复旧条目并随端点报错返回，保持状态一致）；**回滚自身失败**时 `eprintln!` 告警 + 端点 500，下一次任意 Key API 操作入口（已持锁）先从 DB 重建快照，启动加载为天然自愈点。创建/启用类**先提交表写、再重建快照**——最坏情况是 fail-open（新 Key 延迟到重建完成才生效）。两步均持 `settings_update` 锁。真崩溃（两步之间进程死亡）由重启时从表重建快照自愈——进程死后陈旧快照不再服务请求，"已删 Key 复活"仅在活进程窗口出现，而该窗口已被先改快照的顺序消除。快照 `RwLock` 纳入 state.rs 现有锁序文档（与 `settings_update` 的先后关系）。
- [外部直接改 SQLite 表或 config 存储导致快照过期] → 按决策忽略；快照代码处注释声明失效模型（表仅由 Key API 写入、config 仅经 set_config 写入，外部改动重启前不生效）。
- [两处凭证真相（config 标量 + DB 表）] → 职责正交（主 Key=遗留契约，子 Key=新能力），无镜像同步需求；鉴权统一走快照（含主 Key 条目），由 D3 封装在单一函数内。
- [开发库丢弃 PR #43 形态的 config 内子 Key] → 未发布形态，可接受；proposal Impact 已声明需重建设备子 Key。
- [开发库 forward_logs 残留重名 "Primary"] → 曾跑 PR #43 构建的库中，历史行已回填到当时主 Key 的随机 UUID（非 NULL，不触发重回填）——新二进制下这些行既非 `PRIMARY_KEY_ID` 也不会被改写，`/logs/forward/keys` 会出现旧 UUID 与 `PRIMARY_KEY_ID` 两个同名 "Primary" 条目。属开发期残留，**声明可接受**；洁癖修复可手动删 data.sqlite 重建（不提供自动迁移）。
- [`PRIMARY_KEY_ID` 硬编码 UUID 发布后保持稳定] → 常量集中定义 + 注释警示 + 可识别式样（与 v4/nil 视觉区分）；它是**可回收债**而非永久占用——未来重设计可经一次性分块 UPDATE 迁移历史行（与回填同类机制）。它与子 Key 随机 UUID 同空间但不冲突（表主键独立，日志 id 仅作归因键）。
- [上游后续再动 config 结构] → 冲突面已收缩为主 Key 标量本身（v1.6.1 就存在，上游不会删）。

## Migration Plan

1. v1.6.2 → 本版本：config 无需迁移（无 `gateway_keys` 字段）；v19 建表为纯增量；v18 列已存在（随本变更首次发布），历史行回填至 `PRIMARY_KEY_ID`。
2. 本版本 → v1.6.x 降级：旧二进制忽略 `sub_gateway_keys` 表；保存 settings 不触碰该表；主 Key 标量语义即 v1.6.1 语义。不存在凭证复活。
3. 回滚本变更（git 层面）：表残留无害（无人读写）。

## Open Questions

无（主 Key 归因用硬编码 UUID 常量、名称固定 Primary、乐观锁沿用 settings_revision 均已在 D1/D5 定案；实现期如遇 UI 细节措辞问题可后置调整，不影响规格与任务拆分）。
