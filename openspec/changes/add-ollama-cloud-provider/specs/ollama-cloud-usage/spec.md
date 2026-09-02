# ollama-cloud-usage 规格（delta）

## Purpose

定义 Ollama Cloud 账号的 Cookie 用量能力契约：Cookie 的配置校验与混淆存储、settings 页抓取边界、DOM 锚点解析与快照形状、手动刷新限速与失败退避、用量与推理冷却的严格隔离、账号/Cookie 生命周期状态与导出边界。

## ADDED Requirements

### Requirement: Cookie 配置与混淆存储
账号级 Ollama web 会话 MUST 以粘贴的 Cookie 头形式配置，且 MUST 满足：仅含 `name=value` 对；含 Cookie 属性（如 `Path`、`HttpOnly`、`SameSite`）的 Set-Cookie 形态 MUST 被拒绝；重复 cookie 名 MUST 被拒绝；`$` 前缀名 MUST 被拒绝；空值字段 MUST 被拒绝；总长 MUST 不超过 16KB。Cookie MUST 以与现有 API Key 同级的混淆设施（`.encryption-key` 派生，非密码学 AEAD）存储，系统 MUST 明示该保护级别；任何 API 响应、日志与 UI MUST NOT 回显明文，已配置态仅展示状态与有界脱敏摘要。

#### Scenario: 粘贴 Set-Cookie 形态被拒绝
- **WHEN** 管理员提交形如 `session=abc; Path=/; HttpOnly` 的值
- **THEN** 保存被拒绝并提示应粘贴 Cookie 请求头而非 Set-Cookie

### Requirement: 抓取边界固定且最小化
用量抓取 MUST 仅访问固定精确 URL `https://ollama.com/settings`，鉴权仅使用该账号配置的 Cookie；MUST NOT 跟随重定向（被重定向即判定失败）；单次请求超时 MUST 为 15 秒；响应体超过 512KB MUST 按失败处理；出站 MUST 复用进程级出站代理设施的方向默认段（MUST NOT 自建绕过全局代理的客户端）；dashboard/客户端 Key 与上游 API Key MUST NOT 出现在该请求中；本账号 Cookie MUST NOT 出现在任何其他出站（含推理）请求中。

#### Scenario: 重定向视为失败
- **WHEN** settings 端点将请求重定向到其他 URL
- **THEN** 本次刷新按失败处理并进入退避，不追随新 URL

### Requirement: 锚点解析与脱敏快照
快照 MUST 基于页面的 `data-usage-track`/`data-usage-segment`/`data-model`/`data-requests`/`data-time`/`data-usage-window` 锚点解析，产出：5 小时与每周两个窗口（使用百分比与重置时间，可缺省）、按模型按窗口的请求计数、可选的 plan 名与余额。页面为登录页时状态 MUST 判定为 `unauthorized`。持久化与下发的快照 MUST NOT 包含原始 HTML、Cookie 或会话信息；解析失败 MUST 记为 `failed` 且 MUST 只更新状态与尝试元数据，MUST NOT 覆盖或清除上一次成功快照；错误信息字段 MUST 有长度上限且 MUST NOT 含 HTML 片段或 URL 查询串。

#### Scenario: 成功快照形状
- **WHEN** settings 页包含两个用量轨道与每模型分段
- **THEN** 快照包含两个窗口的 used_percent/reset_at 与按模型的请求计数，且不含任何 HTML 或 Cookie 字段

#### Scenario: 会话过期
- **WHEN** 返回内容为登录页
- **THEN** 快照状态为 `unauthorized`，提示重新配置 Cookie，账号推理可用性不受影响

#### Scenario: 失败不破坏上次成功快照
- **WHEN** 上一次快照为成功状态，随后一次刷新解析失败
- **THEN** 快照数据保持上次成功内容可读，状态与错误信息反映最近一次尝试

### Requirement: 手动优先与冷却隔离
用量刷新 MUST 为 opt-in 且手动优先：同一账号手动刷新 MUST 至少间隔 30 秒；系统 MUST NOT 引入请求驱动的自动刷新或后台轮询。失败退避 MUST 采用固定阶梯（对齐现行用量同步：5 分钟 → 15 分钟 → 1 小时 → 6 小时封顶），MUST NOT 依赖响应头。**任何用量路径失败（HTTP 失败、解析失败、unauthorized）MUST NOT 写入推理冷却、MUST NOT 改变账号启用/就绪状态、MUST NOT 影响路由资格**；真实推理的冷却仍由推理路径自身语义负责。

#### Scenario: 用量失败不影响推理
- **WHEN** settings 抓取连续失败并进入退避
- **THEN** 该账号的推理请求照常路由，无冷却写入，退避仅约束下一次用量刷新

#### Scenario: 手动刷新限速
- **WHEN** 同一账号 30 秒内发起第二次手动刷新
- **THEN** 第二次被限速拒绝并返回可重试时间

### Requirement: 生命周期状态与导出边界
Cookie 清除 MUST 使能力回到未配置态：快照与状态行随之清除，UI 不再呈现用量入口。账号禁用期间用量入口 MUST 不可用（手动刷新被拒绝）；账号删除时用量状态 MUST 级联删除。**导出/导入载荷 MUST NOT 携带用量快照、Cookie 明文或密文**（与现行导出省略 usage/浏览器数据的边界一致）；目标节点导入后该能力为未配置态。

#### Scenario: 清除 Cookie 归零
- **WHEN** 管理员清除已配置账号的 Cookie
- **THEN** 快照与状态被清除，能力回到未配置态，路由与推理不受影响

#### Scenario: 导出不携带用量数据
- **WHEN** 管理员导出包含已配置 Ollama Cloud 账号与用量快照的节点数据并在另一节点导入
- **THEN** 导出包与导入结果均不含快照与 Cookie，新节点该能力为未配置态，其余账号数据完整迁移

### Requirement: 持久化与迁移
Cookie 混淆存储与用量状态的存储 MUST 通过 schema v34 迁移引入；v33 → v34 迁移 MUST 非破坏且不改变现有路由行为，并遵循现行 pre-v3 备份策略。

#### Scenario: v33 数据升级
- **WHEN** v33 数据目录首次以新版本打开
- **THEN** 迁移生成备份后完成，既有账号与路由行为不变，用量能力初始为未配置
