## Context

出站代理现状（见 proposal.md - Why）：`AppConfig.proxy_mode`（Auto/Manual/Direct）全局三选一，`http_client.rs::build` 在客户端构建时烤死策略，`CoreState.http_client: Mutex<reqwest::Client>` 单客户端，`upstream_context()` 返回 `(config, client.clone())` 供 CoreState 路径共用。此外 `pricing.rs`、`go_usage.rs`（`usage_sync.rs` 经由它）、dashboard 代理测试、`console_usage.rs` 与 `src-tauri/src/updater.rs` **各自**从 `configured_builder` / `updater_proxy_setting` 构建 HTTP 栈——这些路径不走 CoreState 客户端。聊天转发路径上 `forward_request(client: &Client, …, plan)` 每次尝试接收客户端参数，`plan.model` 为解析后的模型（别名改写、prefer 映射、free 回退后），handler 重试循环内 `active_plan` 可能中途更换模型。所有聊天流量打到同一 `upstream_base_url`，模型判别维度只在请求体。

已知模型注册表 `MODEL_PROTOCOLS`（`gateway/protocol.rs`）有公开接口 `supported_model_ids()`（`models.rs` 的 `ClaudeDesktopModels::validate` 已有 models→gateway 引用先例）；`AppConfig` 以 JSON blob 经 `load_config`/`save_config` 持久化进 SQLite `settings` 表（key `config`）；dashboard settings API 是 `#[serde(flatten)] AppConfig`（新字段自动往返，校验经 `update_settings` 内 `config.validate()`）；forward_logs 现为 schema v21，`log_forward` 逐列 INSERT，成功行的 `diagnostic_json` 为 NULL（诊断只随失败写）。

## Goals / Non-Goals

**Goals:**

- 白名单/黑名单两方向可用，一条"默认段 + 例外段"规则同时覆盖聊天转发与全部非模型出站路径（含 updater 签名下载）。
- 路由每次转发尝试独立解析且对配置热切换免疫；free 回退/别名改写后自动跟随实际模型。
- 配置无行为迁移，旧配置零行为变化；注册表未来收缩不砖启动；成功与失败转发行 alike 可归因路由。

**Non-Goals:**

- 不做 PAC 脚本、通配符/自由文本模型输入、按账号路由（项目规则禁止）。
- 不做代理不可达时的自动直连回退——那会让受限模型在代理故障时静默直连，恰好破坏用户配置意图。
- 不做代理连通性双通道测试按钮（代理测试端点按方向默认段测试，见 D7）。
- 不改 diagnostic_json 的"仅失败行携带"现状（route 走独立列，见 D5）。

## Decisions

### D1：路由决策放调用点，不放 HTTP 层

reqwest 的 `Proxy::custom(|url|…)` 只能看到目标 URL，而所有聊天流量打到同一上游 host，判别维度（模型）在请求体里——字面 PAC 无法实现。决策：**在 handler 转发循环内、每次 `forward_request` 调用前**按 `active_plan.model` 从请求入口捕获的路由集快照选客户端，并把 route 标签作为**新增的一个参数**随 `&Client` 一起传入（`Client` 不暴露代理配置，forwarder 无法自派生；签名仅增此一参，不收路由集），改动收敛在调用点与该参数。

替代方案（拒绝）：每次请求现建客户端——连接池失效、握手成本反复付出；在 forwarder 内部选路——把策略渗进转发器，调用点本来就有快照和 plan。

### D2：路由集（route set）= 元数据 + 双客户端的原子单元，请求入口快照

```rust
// http_client.rs
pub(crate) struct ForwardRouteSet {
    mode: ProxyMode,                      // 路由元数据与客户端同代生成
    url: String,
    direction: ProxyListDirection,
    default_client: reqwest::Client,      // 非 List = 全局模式客户端；List = 方向默认段
    exception_client: Option<reqwest::Client>,  // 仅 List 为 Some（例外段）
}
impl ForwardRouteSet {
    fn client_for(&self, model: &str) -> (&reqwest::Client, RouteLabel);  // 纯函数，无锁无 IO
}
```

关键点：`state.rs` 的 `http_client: Mutex<reqwest::Client>` 升级为 `Mutex<Arc<ForwardRouteSet>>`；路由元数据与两端客户端在同一个结构里、`set_config` 在既有"先 build 再进双锁块整体换新"路径（state.rs 445-464）内同代生成。请求入口 `execute_plan` 一次 `Arc` 克隆整个集合作为快照；循环内每次尝试 `snapshot.client_for(&active_plan.model)`。由此：

- **配置热切换零窗口**（评审 C2）：在飞请求持旧快照（旧元数据 + 旧客户端，内部自洽）飞完即止；不存在"新配置说走例外段但例外客户端是 None"的错配——`client_for` 只读自己持有的元数据，永不跨代。free 回退换 `active_plan` 后下一轮迭代自然重解析；会话粘滞只影响选模型不影响选路。
- 非 List 模式 `exception_client = None`，`client_for` 恒返回默认段（Auto/Manual/Direct 行为与今天一致）。
- `upstream_context()` 保持现有签名与语义（返回默认段客户端），CLI、dashboard、examples 等全部既有调用点零改动。

替代方案（拒绝）：`forward_client(config, model)` 每次尝试即时从 state 取——config 克隆与客户端换新之间存在跨代错配窗口（评审 C2 指出可能静默直连或 panic）；常驻三客户端——Auto 客户端在 List 下永不使用、proxy 客户端在 Auto/Direct 下可能无 URL 可建，按需两份是最小充分形状。reqwest::Client 内部 Arc，快照成本可忽略。

### D3：配置形状与校验分层——写闸口严格，加载路径全量容忍

```rust
enum ProxyMode { Auto, Manual, Direct, List }            // 沿用既有 rename_all = "kebab-case"
enum ProxyListDirection { Whitelist, Blacklist }         // 同 kebab-case，serde default = Whitelist

AppConfig {
    proxy_list_direction: ProxyListDirection,            // serde default
    proxy_list_models: Vec<String>,                      // serde default = []
}
```

- **`AppConfig::validate` 只保留自包含不变量**：List 模式下 `normalize_proxy_url` 必须通过（与 Manual 同性质；hand-edit 出 List+空 URL 启动失败与今天 Manual+空 URL 的先例一致）。**不校验名单内容**——validate 在加载路径也执行（state.rs load 后立即调用），任何依赖注册表的校验放这里都会让"注册表未来移除模型"砖死存量启动（评审 M3）。
- **写闸口（dashboard `update_settings`）**：在 `config.validate()` 之外显式校验名单——非空、逐项 trim 后 ∈ `supported_model_ids()`、去重后回写。UI 前置同样校验。Tauri 旧 commands 路径若绕过 dashboard 直调 `set_config`，则跳过名单成员校验——非主路径，接受并文档化。
- **路由函数全量可解析（total function）**：`client_for` 对空名单/未知 id 一律"无命中"——空白名单 = 全直连（恰是白名单默认段）、空黑名单 = 全走代理 URL（恰是黑名单默认段）、失效 id 永不匹配。加载路径因此天然容忍一切存量形状。
- settings PUT/GET 是 flatten `AppConfig`：新字段自动往返，无"增字段"工作；settings GET 响应在 flatten 之外附 `proxy_supported_models: [{ id, preferred_protocol }]`（`supported_model_ids()` + `gateway::diagnostics::api_format_name` 的既有协议字符串口径——`/v1/models` 是上游透传，没有本地序列化口径可复用）。
- 名单存储 trim 后 id；热路径匹配 `normalize_model_name`（pricing.rs，trim+小写+分隔符归一）与之一致。

### D4：`configured_builder` 的 List 分支 = 方向默认段，五个直调点自动对齐

`configured_builder` 对 `ProxyMode` 穷尽 match（加 `List` 后编译器强制补分支）。定案：**List 分支按方向构建默认段**——白名单 → `no_proxy()` 构建（默认段是直连），黑名单 → `Proxy::all(proxy_url)` 构建（默认段是代理）。价格刷新（pricing.rs）、官方用量（go_usage.rs / usage_sync.rs）、代理测试（dashboard.rs）、冻结的 console_usage.rs 等直调点**零改动自动获得**正确行为（评审 C3）；它们的 config 取自 `state.config()` 锁，单次调用内自洽。回环/进程内控制通道（browser worker 连本机 Chromium 的 `no_proxy` 客户端）不属代理策略管辖，保持现状（规格已豁免）。

### D5：route 走 forward_logs 独立列（schema v22），成功行失败行 alike 落列

评审 C1 证实成功行的 `diagnostic_json` 恒为 NULL（`forwarder.rs` 成功路径 `failure.as_ref().map(...)` 为 None），"route 塞 diagnostic_json"覆盖不了大多数行——而"这条请求为什么慢"恰恰要查成功行。定案：

- schema v22：`ALTER TABLE forward_logs ADD COLUMN route TEXT NOT NULL DEFAULT ''`（历史行空值 = 诚实标记"未记录"，不猜 `auto`）。
- `ForwardAttemptContext` 增加 `route: RouteLabel`（闭集 `auto`/`proxy`/`direct`：Auto→auto，Manual→proxy，Direct→direct，List 按段），handler 选路时注入；`ForwardLog`（models.rs）增加 `route` 字段，`log_forward` INSERT 落列（每行一次尝试，行即尝试，插入时值即终值，`update_forward_log` 无需触碰）。
- 查询侧：行级透出走 `ForwardLog` 结构 + `forward_log_from_row` 映射 + `list_forward_logs`/`query_forward_logs` 的 SELECT 列清单（`ForwardLogSummary` 是聚合统计结构，与行级字段无关，不动）；dashboard logs 响应携带 route，Logs 视图详情行展示（空值显示"—"）。闭集字符串枚举，无 URL/凭据，天然满足脱敏。

替代方案（拒绝）：成功路径也生成最小 diagnostic_json——把"仅失败行有诊断"的既有语义改掉，动静大且诊断 JSON 与一等列查询能力不对等；route 仅记失败行——不满足规格"每行记录"，排查场景缺成功行。

### D6：前端勾选列表由注册表驱动，free 模型明示出口 IP 语义

`ProxyMode` TS 类型加 `"list"`；Settings 出站代理小节第四个单选；选中时渲染方向单选 + 模型勾选网格（settings GET 内嵌 `proxy_supported_models`，每项附 preferred 协议提示）。**free 模型（`*-free`）正常出现在列表中且可选**，但其勾选项附提示"Zen free 额度按出口 IP 共享，走代理会改变额度归属"（评审 M6）——free 模型入名单是合法配置（例如借代理出口隔离/获取 free 额度），语义交由用户知情决定。从任何非白名单模式切到白名单方向时以 caption 明示"非聊天出站（价格/用量/升级检查）将改为直连"（评审 M1 的行为变化提示，Auto 与 Manual 来源同样适用）。校验错误文案沿用 `settings-proxy.ts` 硬编码中文口径（新增同风格函数），UI 标签沿用 Settings.vue 现有 `t()` 机制；保存成功走现有 settings_revision 刷新链路。

### D7：updater 与代理测试端点的 List 语义

- **`src-tauri/src/updater.rs::updater_proxy_setting`**（评审 M4，对 `ProxyMode` 穷尽 match，加 List 编译失败）：白名单 → `Disabled`（默认段直连），黑名单 → `Manual(proxy_url)`（默认段代理）。dashboard `check-update` 走 `upstream_context()`（默认段）已自洽；签名下载在 updater 自己的 HTTP 栈上按此映射对齐，维持"升级下载遵守出站策略"的既有约束。扩展现有 `updater_follows_the_process_wide_proxy_policy` 测试覆盖 List 两方向。
- **代理测试端点 `test_proxy`**（评审 M5）：`ProxyTestRequest` 增加可选 `proxy_list_direction`（缺省取当前 config）；`mode=list` 时按该方向的**默认段**构建测试客户端（白名单→直连连通性、黑名单→代理连通性）。URL 为空在 `normalize_proxy_url` 闸口对 List 视同 Manual 一律 400（方向参数只决定测试段的构建，不改变 URL 校验）；前端把 UI 当前选择的方向传入。语义即"测试你将实际使用的默认段"。

### D8：降级 fail-loud，不做 serde(other) 兜底

新版本写入 `proxy_mode: "list"` 后，旧版本二进制打开同一 data dir 时 serde 反序列化失败经 `?` 传播为启动失败（`load_config` 行为）。决策：**不加 `#[serde(other)]` 静默回落**——静默把 List 读成 Auto 会让受限模型全部意外直连，比启动报错更糟。单维护者、版本只前进是项目既有假设；USER 文档与设置页提示"list 模式需要 ≥ 本版本"。

## Risks / Trade-offs

- [List 模式下代理不可达 → 名单内模型全挂] → 与现状手动模式同性质；不做自动直连回退（见 Non-Goals）；route 列让日志立即归因。
- [双客户端并存的小幅空闲连接增长] → 每池 `pool_idle_timeout` 30s 自动回收，量级个位数连接，可接受。
- [Manual→白名单切换的非聊天出站静默变直连] → UI caption + USER 文档明示（D6）；这是"默认段"规则的直接后果，不是回归。
- [注册表收缩后存量名单含失效 id] → 加载容忍、失效 id 惰性（D3）；下次保存被写闸口强制清理；UI 勾选网格不渲染失效 id，存储值与渲染解耦。
- [validate 不校验名单 ⇒ 非 dashboard 写路径可能写入非法名单] → 只有绕过 dashboard 的 `set_config` 直调（Tauri 旧 commands）可触发，非主路径；路由函数全量容忍，最坏行为是"无命中"，无安全后果。
- [route 列迁移] → 单列 ADD COLUMN + 默认空串，v1→v22 迁移链既有机制照搬，回滚兼容（旧二进制不读该列）。

## Migration Plan

- 上线：无配置迁移；schema v22 随启动自动迁移（仅加列）；旧配置缺新字段 → serde 缺省 → 行为不变；部署后设置页 opt-in。
- 回滚：先把模式切回 Manual/Direct（名单字段留存但惰性），再回退二进制；若旧二进制已因 `proxy_mode:"list"` 无法启动，用 sqlite3 修改存储：`UPDATE settings SET value = replace(value, '"proxy_mode":"list"', '"proxy_mode":"direct"') WHERE key = 'config';`（config 在 SQLite `settings` 表 key `config` 的 JSON 值里，**不是**数据目录下的独立 JSON 文件）；只影响代理模式，账号/日志/子 Key 无损失。
- 文档：`AGENTS.md` 全局代理条目改写为"默认段+例外段"统一规则；`docs/USER.md` / `USER.zh-CN.md` 补名单模式说明（受限模型示例、free 出口 IP 提示、切换行为变化、版本要求）；README 不加长表。

## Open Questions

（无——路由集快照、configured_builder 分支、updater 映射、route 列、写闸口分层、测试端点语义、free 模型提示、回滚步骤均已在上述决策定案。）
