## Why

PR #43 的多 Key 方案把 key 列表内嵌在 `AppConfig.gateway_keys`（settings JSON）里，评审暴露了该存储选址的结构性缺陷：主 Key 可被禁用但镜像字段保留其值，降级旧版本后已禁用的凭证会重新生效（安全问题）；关键约束（值唯一、全部禁用、数量上限）未接入 `AppConfig::validate`，绕过生命周期函数的写路径无结构防护；旧设置接口提交 Key 被静默忽略却返回成功；且与上游 v1.6.2 的"config 瘦身"路线持续冲突（Dashboard 被迫重新持有完整 `ref<AppConfig>`）。PR 尚未合并、该 config 形态从未发布——现在是零迁移成本修正存储选址的最后窗口。

## What Changes

- **回滚 config 内嵌 key 列表（BREAKING，仅针对未发布的 PR 形态）**：移除 `AppConfig.gateway_keys` 与 `GatewayKeyEntry` serde 字段、镜像同步、主 Key 删除提升、config 内墓碑、`update_settings` 的 key 防清空覆盖等全部配套复杂度。
- **主 Key 回归遗留路径**：`AppConfig.gateway_key` 保持 v1.6.1 语义——可通过 `POST /settings` 自定义值（trim + 非空校验，且不得与任何活跃子 Key 值相同）、可经 `POST /settings/regenerate-gateway-key` 轮换，**永不可禁用/删除**；"trim 后非空"约束**新增**入 `AppConfig::validate`（该校验现状仅在设置更新的 payload 守卫，validate 本不含——即评审意见所指缺口），降级任意旧版本都不存在"禁用凭证复活"问题。
- **子 Key 独立数据库表（新增能力）**：新表 `sub_gateway_keys`（schema v19：id/name/key/enabled/deleted_at/created_at，value 唯一索引），经独立生命周期 API（创建/改名/启停/重新生成/软删除）管理；软删除保留名称供日志归因。旧二进制不识别此表，子 Key 可安全扛住降级往返。
- **鉴权：候选头 OR 语义 + 内存快照**：收集请求中全部非空候选头（Bearer / x-api-key / x-goog-api-key），任一命中**当前有效凭证**（主 Key 值或任一启用子 Key）即通过——恢复 v1.6.1 的 OR 语义，修复"错误 x-api-key + 正确 x-goog-api-key 被拒"回归；凭证快照 `RwLock<HashMap<value, (id, name)>>` 同时承载主 Key 与子 Key（`set_config` 后刷新主 Key 条目），鉴权热路径与日志写入的名称快照共用同一份，不再读取 AppConfig、不取 config 锁——中间件/纯鉴权路径的每请求整份 clone 随之消失（代理转发路径的配置 clone 为既有行为，不在范围）。
- **Dashboard 切换器改用精简类型**：接入中心不再持有完整 `ref<AppConfig>`，改用新端点 `GET /dashboard/api/connection` 提供的 `ConnectionInfo`（主 Key 值、子 Key 列表、revision 与端口/根地址/上游地址）；Key 的新增、启停、改名、重新生成、删除全部走独立 Key API，不依赖通用 settings 保存。
- **按 Key 用量归因保留并修正**：保留 `forward_logs.client_key_id/client_key_name`（v18）与分块回填机制，回填目标改为硬编码 UUID 常量 `PRIMARY_KEY_ID`（主 Key 归因标识；发布后保持稳定、直至显式迁移，可识别式样与生成的 UUID 视觉区分）；`/logs/forward/keys` 按 id 分组取**最新**名称快照、保持纯日志驱动（修复重命名后筛选显示字典序最大旧名的问题）。
- **Key 命名遵循 DESIGN.md（有边界）**：本变更触及的用户可见面——dashboard UI 文案与日志页可见的审计消息——只称 "Key"（主 Key 显示 "Primary"/"主 Key"），不出现 "Gateway Key" 字样；CLI stdout 与上游 401 错误串维持 v1.6.1 现状（"gateway key" 字样），不在本约束范围。内部标识符（表名、能力名、端点路径）不受此约束。
- **数量上限与跨层唯一**：子 Key 活跃数量上限 64，由 API 层强制（表的部分唯一索引仅兜底子 Key 之间的值唯一）；主 Key 值与子 Key 值之间也不得重复（API 层双向校验），杜绝同一凭证双重归因。主 Key 恒为 1 把，"≥1 有效凭证"不变量自动成立。

## Capabilities

### New Capabilities

- `gateway-key-management`: 主 Key（遗留标量、永不禁用、`AppConfig::validate` 约束）与子 Key（独立表、生命周期 API、软删除、数量上限）的双层凭证管理，以及多候选头 OR 语义鉴权与命中归因。
- `dashboard-key-panel`: 接入中心基于精简 `ConnectionInfo` 的 Key 切换器展示、复制与单 Key 重新生成；Settings 的 Key 管理区（主 Key 只读 + 轮换，子 Key 完整生命周期）。
- `usage-by-key`: forward_logs 按 Key 归因（主 Key 用硬编码 UUID 常量归因、回填）、按 Key 过滤与汇总、日志 key 列表取每 id 最新名称。

### Modified Capabilities

（无——`openspec/specs/` 尚无主规格，前序方案 `multi-gateway-keys` 已归档未同步，本变更全新定义这三个能力。）

## Impact

- **后端**（`crates/ocg-core` + `src-tauri`）：`models.rs`（移除 `GatewayKeyEntry`/`gateway_keys`；主 Key 非空约束**新增**入 `AppConfig::validate`；`GatewayStatus` 移除 `keys`/`primary_key_id` 保留 `key`）、`state.rs`（移除 config 迁移/镜像/提升逻辑；新增凭证快照与失效）、`gateway_keys.rs` 重写为 DB CRUD + 快照、`db.rs`（v19 建表 + 唯一索引 + 子 Key 查询/审计；`/keys` 列表按最新名称分组）、`gateway/handler.rs`（候选头 OR 语义鉴权）、`gateway/mod.rs`（回填目标改 PRIMARY_KEY_ID 常量）、`dashboard.rs`（子 Key API 改造 + `update_settings` 恢复主 Key 可设置 + 新端点 `GET /dashboard/api/connection`）、`src-tauri/src/commands/gateway.rs` 与 `setting.rs`（适配 GatewayStatus 回滚与主 Key 校验同步）。
- **前端**（`src/`）：`api/tauri.ts` 类型与 API（`AppConfig` 去 `gateway_keys`；新增 `ConnectionInfo`/子 Key 类型）、`Dashboard.vue` 精简为 `ConnectionInfo`（移除 `ref<AppConfig>`）、`Settings.vue` Key 区改造（主 Key 行只读 + 轮换）、`Logs.vue` 筛选不变、`settings-merge.ts` 恢复 `gateway_key` 可编辑。
- **CLI/发布**：CLI `key` 子命令与 `status` 输出继续走主 Key（不变）；release 冒烟断言保持。
- **数据**：v1.6.2 → 本版本零迁移（config 无 `gateway_keys` 需处理）；v18 列与回填保留，回填目标改硬编码 UUID 常量 PRIMARY_KEY_ID；曾运行 PR #43 构建的开发库中 config 内 `gateway_keys` 将被忽略丢弃（未发布形态，可接受，需重建设备子 Key），且其 forward_logs 已按旧主 Key 随机 UUID 回填的历史行不会重归因（非 NULL）——筛选列表将出现重名 "Primary" 条目，同属可接受的开发期残留（可手动删库重建）。
- **openspec**：取代已归档的 `multi-gateway-keys`（2026-08-17），归档未同步主规格，本变更全新定义能力规格。
