## Why

当前网关只有**一个**客户端鉴权 key（`AppConfig.gateway_key`）。多设备/多团队共用一台 OCG Manager 时，所有客户端共用同一把钥匙：无法按客户端区分用量、无法单独吊销某台设备的钥匙、任何设备泄露 key 都只能整体换新。本地网关需要支持**多个 gateway key**，每个 key 可独立命名、独立启停、独立统计用量。

## What Changes

- **多 key 存储与管理（新增能力）**：`AppConfig` 增加 `gateway_keys` 列表（含主 key），新增 CRUD API（创建/重命名/启停/重新生成/删除），key 删除采用**软删除**（保留记录以维持日志归因），旧 `gateway_key` 字段保留为兼容层并在加载时自动迁移为列表首条。
- **鉴权改为多 key 匹配（修改现有行为）**：`check_auth` 从比对单个 `gateway_key` 改为遍历启用的 key 列表，并**返回命中的 key id** 供日志埋点使用。现有单 key 客户端在迁移后无需任何改动（原 key 自动成为主 key）。
- **Dashboard 接入中心多 key 展示（新增能力）**：接入中心 Key 行支持在多个 key 之间切换、复制当前 key、对当前 key 重新生成；"刷新 Key"语义从"整体换新"改为"仅当前 key 失效"，交互文案同步调整。
- **按 key 记录与查询用量（新增能力）**：`forward_logs` 新增 `client_key_id`（及可选 `client_key_name` 快照）列，转发请求时记录所用客户端 key；Logs 页 forward 查询支持按 key 过滤，汇总（请求数/Token/费用）按 key 维度展示。
- **历史数据回填（数据迁移）**：升级后旧日志按迁移时的主 key 归集（迁移前仅一个 key；若用户轮换过 key，历史段为近似归因，UI 注明）。大表采用**分块、幂等、可恢复**的回填策略，不阻塞启动。

## Capabilities

### New Capabilities

- `gateway-key-management`: 多 gateway key 的存储、生命周期管理（创建/重命名/启停/重新生成/软删除）、旧单 key 平滑迁移与鉴权多 key 匹配。
- `dashboard-key-panel`: Dashboard 接入中心的多 key 展示、切换、复制与单 key 重新生成交互。
- `usage-by-key`: forward_logs 记录客户端 key、按 key 过滤查询与用量汇总、历史日志安全回填。

### Modified Capabilities

（无既有 spec，全部为新增能力。）

## Impact

- **后端**（`crates/ocg-core`）：`models.rs`（`AppConfig`/`GatewayStatus` 新增字段）、`state.rs`（config 加载迁移、`set_config` 不变量强制、`check_auth` 相关 reset 逻辑）、`gateway/handler.rs`（`check_auth` 现有 7 处调用收口为多 key 匹配并返回 key id）、`gateway/forwarder.rs`（`ForwardAttemptContext` 与 `log_forward` 携带客户端 key）、`db.rs`（schema v18：`forward_logs.client_key_id` 列；日志查询 options 增加 `key_id`；回填方法）、`dashboard.rs`（key CRUD API、`get_settings`/`gateway_status` 返回 keys 列表）。
- **前端**（`src/`）：`api/tauri.ts`（类型与 API）、`views/Settings.vue`（多 key 维护 UI）、`views/Dashboard.vue`（接入中心 key 切换复制）、`views/Logs.vue`（key 筛选）、`views/Applications.vue`（复制默认主 key）、`views/settings-merge.ts`（key 字段继续排除在可编辑列表外）、`i18n/messages/*`（10 个语言源文案）。
- **CLI**（`crates/ocg-cli`）：状态输出从单 key 改为展示主 key。
- **测试**：`gateway/handler.rs`、`dashboard_auth.rs`、`core_state.rs` 等单 key 断言需适配；新增多 key CRUD、鉴权匹配、回填幂等的测试。
- **发布**：Docker 镜像、桌面安装包、CLI 归档多发布渠道共用同一数据格式，schema 变更仅涉及**加列**与 config 字段，需验证各渠道升级路径无感（见发布评估）。
