# ollama-cloud-provider 规格（delta）

## Purpose

定义 Ollama Cloud 作为第七个密封内置 Provider 家族的行为契约：固定源与仅 Chat Completions 的协议事实、wire 规范化的精确触发与作用边界、共享别名映射与目录快照解析、转发改写、冷却/错误/记账语义、fail-closed 启用语义。

## ADDED Requirements

### Requirement: 密封身份与固定源
系统 MUST 为 Ollama Cloud 提供稳定的 Provider/Offering 身份，并将其加入适配器种类的穷举集合；其上游源 MUST 固定为 `https://ollama.com`，上游协议 MUST 仅为 Chat Completions（Bearer 鉴权），出站 MUST NOT 跟随重定向，且 MUST 声明为进程级出站代理语义（入站请求快照选路；本家族模型 id 不参与按模型 List 的例外段匹配，走方向默认段）。协议事实 MUST 记录在 GOAT 式的家族独立协议种子中，MUST NOT 写入 OpenCode Go 的全局模型协议表（该表派生 Go 已发布别名）。请求路径 MUST NOT 为猜测协议而向计费端点发送探测请求。本家族的协议探测能力 MUST 为不支持（与 GOAT/MiniMax/Kimi 同）：固定仅 Chat 家族无可探测的协议面，Providers 页 MUST NOT 呈现探测入口。

#### Scenario: 仅 Chat Completions 可路由
- **WHEN** 客户端以 Chat Completions 请求一个已发布的 Ollama Cloud 模型
- **THEN** 请求经该家族适配器路由到固定源的标准 Chat 路径；其他客户端格式（Responses/Messages/Gemini）被转换为 Chat 后转发

#### Scenario: 家族在完成前不可启用
- **WHEN** 该家族 offering 处于 `routable=false` 状态
- **THEN** 任何持久化路径的启用写入均被拒绝（fail-closed）

### Requirement: Wire 规范化为固定行为且按尝试生效
对最终由 Ollama Cloud 尝试服务的请求，系统 MUST 执行以下不可配置的双向规范化：(1) 请求中 assistant 消息存在非空 `reasoning_content` 且无 `reasoning` 时，将其复制到 `reasoning`；(2) 响应 JSON 与 SSE 流的 `message`/`delta` 中存在非空 `reasoning` 或 `thinking` 且无 `reasoning_content` 时，补写 `reasoning_content`；(3) `max_tokens` 与 `max_completion_tokens` 大于 65535 时 MUST 被单向钳制为 65535。规范化 MUST 提供用户不可见的开关，MUST NOT 提供用户开关，且逐字节作用于且仅作用于 Ollama Cloud 尝试：当同一客户端请求的候选链同时包含其他家族时，发往非 Ollama 尝试的请求字节 MUST 与未引入本家族前完全一致；上游侧日志 MUST 记录实际发送的（规范化后）字节，面向客户端的归因 MUST 使用客户端请求名。Cookie MUST NOT 附着于本家族的推理出站请求。

#### Scenario: 思维链对 DeepSeek 风格客户端可见
- **WHEN** 上游响应 delta 携带 `thinking` 字段且无 `reasoning_content`
- **THEN** 客户端收到的同一 delta 包含从 `thinking` 补齐的 `reasoning_content`，原始字段保留

#### Scenario: 超限输出上限被钳制
- **WHEN** 客户端请求 `max_tokens: 200000` 且请求由 Ollama Cloud 尝试服务
- **THEN** 发往上游的该次尝试请求体中该值为 65535；上游侧日志记录钳制后的值

#### Scenario: 混合候选链字节隔离
- **WHEN** 同一请求的候选链同时含 Go 与 Ollama Cloud 账号，且最终由 Go 尝试服务
- **THEN** 发往 Go 的请求体与本家族不存在时的字节完全一致，无 `reasoning` 补写与 `max_tokens` 钳制

### Requirement: 目录发现与快照 id 不进代码
Ollama Cloud 的模型目录 MUST 来自其公开 `GET /models` 的显式控制面刷新，请求路径 MUST NOT 触发发现；该端点免鉴权可达是实现前提，若实测需要鉴权则目录刷新路径 MUST 重新设计后方可启用。目录精确 id（含 `:` 标签）MUST 原样登记，并在存在已保存目录行且协议启用时可作为裸 id 请求（钉定该家族、不参与跨账号回退）。源代码 MUST NOT 硬编码任何带日期标签的快照 id（如 `deepseek-v4-flash:0731`）；尺寸标签变体 id（如 `gpt-oss:120b` 与 `gpt-oss:20b`）MUST 作为独立条目共存。转发与用量出站 MUST 在 forward_logs 中记录实际路由段（`route` 列），用量刷新出站 MUST 复用进程级出站代理的方向默认段。

#### Scenario: 裸 id 请求钉定家族
- **WHEN** 客户端请求目录中的精确 id `gpt-oss:120b` 且该行已保存并启用
- **THEN** 请求被钉定到 Ollama Cloud 家族按原 id 转发，不发生跨账号回退

### Requirement: 共享别名映射与单匹配守卫
别名命名空间是全局且 Go 优先的：本家族 MUST NOT 创建、抢占或改写任何已发布别名（首期词干 `deepseek-v4-flash`、`deepseek-v4-pro` 均为 Go 已拥有别名）。本家族对别名的唯一贡献方式是：在目录刷新时按"剥离 `:` 标签后词干与既有代码拥有别名唯一匹配"解析出快照 id，并将一个可路由映射**追加**进该别名的候选映射集，参与既有的账号顺序/sticky/跨账号回退语义；请求由哪个家族服务，就按哪个家族的计价与归因记账（Go 计价行 vs 本家族 unpriced）。零匹配或多匹配时，本家族映射 MUST 保持不可路由（其他家族候选不受影响，fail-closed 不猜测），管理员 MUST 能在模型矩阵中从发现的精确 id 中人工钉定本家族映射；钉定为持久数据，后续刷新 MUST 尊重。多匹配期间该别名的发布与既有发布保持一致（本家族不新增发布项），被钉定或单匹配恢复后本家族映射重新可路由。

#### Scenario: 快照轮换自动重绑
- **WHEN** 上游将唯一日期标签从 `:0731` 轮换为 `:0915` 且目录刷新完成
- **THEN** 本家族映射在无任何代码变更的情况下绑定新快照并恢复可路由

#### Scenario: 新旧快照并存时拒绝猜测且不扰动其他家族
- **WHEN** 目录同时存在同词干的两个日期标签
- **THEN** 本家族映射不可路由；同名别名仍由 Go 等既有家族照常服务，等待管理员钉定

#### Scenario: 本家族映射不窃取别名发布
- **WHEN** 客户端请求 `deepseek-v4-flash` 且本家族映射可路由
- **THEN** 该名仍按既有别名发布，`GET /v1/models` 不因本家族出现重复或新条目；请求按账号顺序/sticky 在 Go/GOAT/本家族映射间回退，归因与计价跟随实际服务家族

### Requirement: 冷却、错误与记账语义
本家族 MUST 沿用通用 Provider 冷却语义：推理 429 写入本家族冷却并参与选择器排除；401 语义与 Kimi/MiniMax 家族一致（账号凭证边界，按现行家族规则处理验证态与错误标记）。本家族流量 MUST 记账为 unpriced（无公开价格表），MUST NOT 回退到 OpenCode Go 价格表计价，MUST NOT 从本家族流量推导 Go 额度消耗。

#### Scenario: 429 触发家族冷却
- **WHEN** 上游对推理请求返回 429
- **THEN** 该账号进入通用 Provider 冷却并被选择器排除，冷却不波及其他家族账号

#### Scenario: unpriced 记账
- **WHEN** 本家族请求成功且带 usage
- **THEN** 日志成本状态为 unpriced，不产生 Go 价格换算，不影响 Go 额度估算
