## Why

出站代理目前是全局三选一（自动/手动/强制直连），不区分模型。当前上游仅 `gpt-5.6-luna`、`grok-4.5`、`muse-spark-1.2`（含 `-contributor` 变体）是区域受限的：受限区域用户一旦配置手动代理让这几个模型可用，其余所有模型都被迫绕路，平白承受延迟、带宽与稳定性损失。需要把代理粒度从"进程全局"细化到"模型"。

## What Changes

- **`ProxyMode` 新增第四种模式 `List`（按模型名单）**，语义固定为两段式：
  - **白名单方向**：名单内模型走 `proxy_url` 手动代理，名单外模型强制直连；
  - **黑名单方向**：名单内模型强制直连，名单外模型走 `proxy_url` 手动代理；
  - 两种方向都要求 `proxy_url` 非空。"直连"与现有 `Direct` 模式同义（忽略系统/环境代理）。
- **名单存储在 `AppConfig`**（`proxy_list_direction` + `proxy_list_models: Vec<String>`），随现有 `load_config`/`save_config` 以 JSON blob 持久化进 SQLite `settings` 表（key `config`），无 schema 迁移成本；serde 缺省值保证旧配置加载后行为不变。
- **名单值为项目已知模型 id**（`MODEL_PROTOCOLS` 注册表，公开接口 `supported_model_ids()` 已存在），UI 做成勾选式，不做自由文本与通配符（`muse-spark-1.2-contributor` 本身是独立条目，直接可勾）。**名单校验落在 settings 写闸口**（dashboard `update_settings`）：空名单、未知 id、空 `proxy_url` 拒绝保存；**加载路径容忍**存量名单中的失效 id 与空名单（视为"无命中"，路由函数全量可解析）——未来版本从注册表移除模型不会砖死任何已存配置的启动。
- **`CoreState` 单客户端升级为"路由集"（route set）**：一个原子单元同时持有路由元数据（mode/url/direction）与两端客户端（默认段 + 例外段，仅 List 模式例外段为 `Some`），`set_config` 在单锁内整体换新。**每个下游请求在入口克隆一份路由集快照**，转发循环内每次尝试用快照按 `active_plan.model` 纯函数选客户端——配置热切换对在飞请求零影响（持旧快照飞完即止），不存在"新配置 + 旧客户端"的错配窗口。
- **`configured_builder` 增加 List 分支 = 方向默认段**（白名单→直连构建、黑名单→手动代理构建）。价格刷新、官方用量同步、账号测试 ping、`/v1/models`、连接/代理测试等所有直调 `configured_builder` 的自建客户端路径**自动获得**默认段行为，无需逐点改造；`src-tauri/src/updater.rs` 的 `updater_proxy_setting` 同步增加 List 映射（白名单→`Disabled`、黑名单→`Manual(proxy_url)`），保持"签名升级下载与出站策略对齐"的既有约束。
- **可观测性**：`forward_logs` 新增 `route` 列（schema v22），**每次转发尝试（成功与失败行 alike）**记录实际路由段（`auto`/`proxy`/`direct`），由 `ForwardAttemptContext` 携带、`log_forward` 插入时落列；Logs 视图详情展示该字段。历史行为空值（诚实标记"未记录"）。
- **UI**：Settings 出站代理小节增加第四个模式单选；选中 `List` 时展示方向单选与已知模型勾选列表（附 preferred 协议提示；free 模型可选，附"Zen free 额度按出口 IP 共享，走代理会改变额度归属"提示）；从 Manual 切到白名单时以文案明示"非聊天出站将改为直连"。
- **非目标**：不做 PAC 脚本（PAC 按 URL 决策，而所有聊天流量打到同一上游 host、判别维度在请求体，字面 PAC 无法实现）；不做自定义模型自由文本输入；不做按账号路由（项目规则明确禁止）；不引入"聊天转发一套规则、其余出站另一套规则"的分裂口径；不做代理不可达时的自动直连回退（会让受限模型在代理故障时静默直连，破坏配置意图）。

## Capabilities

### New Capabilities

- `model-proxy-routing`：按模型代理名单路由的能力契约——配置形状与写闸口校验（含注册表收缩容忍）、路由决策规则（含 free 回退/粘滞会话/热切换快照交互）、路由集生命周期、`configured_builder` 与 updater 的默认段对齐、forward_logs 路由可观测性、UI 勾选交互与持久化语义。

### Modified Capabilities

（无——`openspec/specs/` 无主规格，本能力全新定义。）

## Impact

- **Rust（`crates/ocg-core`）**：
  - `models.rs`：`ProxyMode::List`、`ProxyListDirection`、`proxy_list_direction`/`proxy_list_models` 字段；`AppConfig::validate` 仅含自包含不变量（List 需合法 `proxy_url`）；名单成员校验在 dashboard `update_settings` 写闸口；
  - `http_client.rs`：`RouteLeg` 与纯函数 `route_leg`、`configured_builder` 的 List 分支（方向默认段）、路由集类型与两端构建；
  - `state.rs`：`http_client` 字段升级为路由集（单锁原子换新），`upstream_context()` 语义保持（返回默认段客户端）；
  - `gateway/handler.rs`：请求入口取路由集快照，重试循环内每次尝试按 `active_plan.model` 从快照选客户端；
  - `gateway/forwarder.rs`：`ForwardAttemptContext` 增加 route 标签并随 `log_forward` 落列；
  - `gateway/protocol.rs`：无改动（`supported_model_ids()` 已公开）；preferred 协议字符串复用 `gateway::diagnostics::api_format_name` 既有口径；
  - `db.rs`：schema v22 迁移（`forward_logs` 加 `route` 列，历史行空值）、INSERT/SELECT 列清单与 `ForwardLog` 行结构透出（`forward_log_from_row` 映射同步）；
  - `dashboard.rs`：settings GET 响应附 `proxy_supported_models`、写闸口增名单校验、logs 响应透出 route、代理测试端点接受方向参数。
- **Tauri（`src-tauri/src/updater.rs`）**：`updater_proxy_setting` 增加 List 映射并扩展现有 proxy 对齐测试。
- **前端（`src/`）**：`api/tauri.ts`（`ProxyMode` 加 `"list"`、settings/logs 形状）、`views/Settings.vue` + `views/settings-proxy.ts`、Logs 视图 route 展示；校验文案沿用 `settings-proxy.ts` 现有硬编码中文口径，UI 标签沿用 Settings.vue 现有 `t()` 机制。
- **文档**：`AGENTS.md`（全局代理条目改写为"默认段+例外段"统一规则）、`docs/USER.md` / `USER.zh-CN.md`（设置说明、free 模型出口 IP 提示、Manual→白名单切换的行为变化提示）。
- **数据库**：schema v22（仅 forward_logs 加列）。
- **兼容性**：旧配置无新字段 → serde 缺省 → 行为不变；`List` 为显式 opt-in；注册表收缩不砖启动。
- **风险面**：双客户端并存的小幅空闲连接增长（每池 idle 30s 回收，量级个位数连接）；route 列为字符串枚举（三值封闭集，无脱敏负担——不含 URL/凭据）。
