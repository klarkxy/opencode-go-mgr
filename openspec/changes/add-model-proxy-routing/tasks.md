## 1. 配置模型与分层校验（crates/ocg-core/src/models.rs）

- [x] 1.1 `ProxyMode` 增加 `List` 变体（沿用既有 `rename_all = "kebab-case"` 属性写法）；新增 `ProxyListDirection`（kebab-case，serde default `Whitelist`）与 `AppConfig.proxy_list_direction` / `proxy_list_models`（serde default）；`Default` 与测试工厂同步。
- [x] 1.2 `AppConfig::validate` 增加且仅增加自包含不变量：List 模式下 `normalize_proxy_url` 必须通过；不校验名单内容（validate 在加载路径也执行，注册表依赖校验放这里会砖启动）。单元测试：List+空 URL 拒绝、List+合法 URL 通过、非 List 模式名单字段原样保留。
- [x] 1.3 旧配置兼容回归：无新字段的 AppConfig JSON 反序列化后三模式行为不变；`ProxyMode:"list"` 在无 serde(other) 下 fail-loud（与 D8 决策一致，仅测试确认现状）；空名单/含失效 id 的名单加载成功且 `client_for` 判为"无命中"（空白名单=全直连、空黑名单=全走 URL）。

## 2. 路由集与热路径（crates/ocg-core/src/http_client.rs、state.rs、gateway/handler.rs）

- [x] 2.1 `http_client.rs`：新增 `RouteLabel { Auto, Proxy, Direct }` 与路由集 `ForwardRouteSet`（mode/url/direction 元数据 + default_client + exception_client，仅 List 时例外段 `Some`）、`client_for(model) -> (&Client, RouteLabel)` 纯函数（`normalize_model_name` 匹配、空名单/未知 id 全量容忍）、按段构建辅助；`configured_builder` 增加 List 分支 = 方向默认段（白名单→`no_proxy`、黑名单→`Proxy::all(proxy_url)`）。单元测试：两方向 × 名单内/外 × 未知/空名单的 `client_for` 归类，以及 List 分支下 builder 的代理策略（`no_proxy`/`proxy` 生效）。
- [x] 2.2 `state.rs`：`http_client: Mutex<reqwest::Client>` 升级为 `Mutex<Arc<ForwardRouteSet>>`；`set_config` 在既有"先 build 再进双锁块"路径内整体换新（元数据与客户端同代）；`upstream_context()` 签名与语义不变（返回默认段客户端）；新增快照访问器（`Arc` 克隆整个集合）。单元测试：配置切换后集合整体换新、在飞快照持旧值自洽。
- [x] 2.3 `gateway/handler.rs`：`execute_plan` 入口捕获路由集快照；转发重试循环内每次尝试 `snapshot.client_for(&active_plan.model)` 取客户端与 route 标签（替换循环外一次性获取的 `client`）；`upstream_context()` 全部非转发调用点保持不变。集成测试：free 回退换模型后路由段跟随（白名单下 free 名单内模型 → 回退 Go 名单外模型走直连）；请求进行中 `set_config` 切换模式，在飞请求后续尝试仍用入口快照（评审 C2 场景）。

## 3. forward_logs 路由列（crates/ocg-core/src/db.rs、models.rs、gateway/forwarder.rs）

- [x] 3.1 `db.rs`：schema v22 迁移 `ALTER TABLE forward_logs ADD COLUMN route TEXT NOT NULL DEFAULT ''`；`log_forward` INSERT 增加 route 列；`ForwardLog`（models.rs）增加 `route` 字段；`update_forward_log` 不触碰 route（插入时值即终值）。
- [x] 3.2 `gateway/forwarder.rs`：`ForwardAttemptContext` 增加 `route: RouteLabel`（handler 选路时注入），`log_forward` 组装 `ForwardLog` 时落列；成功行与失败行 alike 携带（闭集字符串 `auto`/`proxy`/`direct`，不含 URL/凭据）。单元测试：成功路径与失败路径的行都带正确 route 标签；历史行 route 为空串。
- [x] 3.3 查询与 API 透出：`ForwardLog` 行结构增加 route 字段、`forward_log_from_row` 映射与 `list_forward_logs`/`query_forward_logs` 的 SELECT 列清单同步加列（`ForwardLogSummary` 是聚合统计结构，不动）；dashboard logs 响应增加 route；前端 Logs 视图详情增加"路由"行（空值显示"—"）。

## 4. updater 与代理测试（src-tauri、dashboard）

- [x] 4.1 `src-tauri/src/updater.rs`：`updater_proxy_setting` 增加 List 映射（白名单→`Disabled`、黑名单→`Manual(proxy_url)`）；扩展现有 proxy 对齐测试覆盖两方向。
- [x] 4.2 `dashboard.rs` 代理测试端点：`ProxyTestRequest` 增加可选 `proxy_list_direction`（缺省取当前 config）；`mode=list` 按方向默认段构建测试客户端；URL 为空在 `normalize_proxy_url` 闸口一律 400（List 视同 Manual，方向参数只决定测试段构建）；集成测试覆盖两方向的测试行为与缺省方向回退。

## 5. settings API（crates/ocg-core/src/dashboard.rs）

- [x] 5.1 写闸口：`update_settings` 在 `config.validate()` 之外显式校验名单——List 模式下名单非空、逐项 trim 后 ∈ `supported_model_ids()`、去重后回写（利用 flatten `AppConfig` 自动往返，新字段无需增删请求/响应结构）。集成测试：空名单 / 未知 id / 重复项（去重通过）三类行为 + revision 递增。
- [x] 5.2 settings GET 响应在 flatten 之外附 `proxy_supported_models: [{ id, preferred_protocol }]`（`supported_model_ids()` + `gateway::diagnostics::api_format_name` 字符串口径）；响应形状测试。

## 6. 前端（src/）

- [x] 6.1 `src/api/tauri.ts`：`ProxyMode` 类型加 `"list"`；settings 响应类型补 `proxy_list_direction` / `proxy_list_models` / `proxy_supported_models`；logs 响应类型补 route。
- [x] 6.2 `src/views/settings-proxy.ts`：新增名单校验函数（仅 list 模式下拒绝空名单与未知 id，对照 GET 返回的 `proxy_supported_models`），文案沿用该文件现有硬编码中文口径。
- [x] 6.3 `src/views/Settings.vue`：第四个模式单选"按模型名单"；选中时渲染方向单选 + 已知模型勾选网格（附 preferred 协议提示；free 模型附"Zen free 额度按出口 IP 共享，走代理会改变额度归属"提示；存储名单含失效 id 时显示"未知模型将被忽略"提示）；从任何非白名单模式切到白名单方向显示"非聊天出站将改为直连" caption；代理测试请求携带当前方向；空名单/空 URL 保存前拦截。
- [x] 6.4 前端验证：`pnpm run build:web` 通过；`pnpm run test` 全量回归。

## 7. 文档与回归收尾

- [x] 7.1 `AGENTS.md`：全局出站代理条目改写为"聊天转发与非模型出站共用默认段+例外段规则（List 模式）；非 List 模式维持三选一"，关键文件清单补 `http_client.rs` 路由集职责与 forward_logs route 列。
- [x] 7.2 `docs/USER.md` / `USER.zh-CN.md`：设置节补名单模式说明（白/黑名单语义、受限模型示例、直连含义、free 模型出口 IP 影响、Manual→白名单切换的行为变化、CLI ping / 账号测试始终走默认段不代表受限模型真实转发路径、list 模式版本要求与回滚提示），保持中英对与 TOC 一致；README 不加长表。
- [x] 7.3 Rust 回归：`cargo test -p ocg-core` 全量（含新增路由集/迁移/写闸口/日志标签测试）；`cargo test -p ocg-manager-cli`；workspace `cargo clippy` 干净；`src-tauri` 定向测试（updater 映射）。
- [x] 7.4 冒烟：`pnpm run dev` 启动后设置页配置白名单（勾选 gpt-5.6-luna），保存生效且 revision 递增；经 Gateway 转发名单内与名单外模型各一次，Logs 详情分别显示 `proxy` / `direct`；切回 Manual 后新请求 route 显示 `proxy`（无受限区域环境时用本地 mock 代理验证连接行为与 route 标签）。
