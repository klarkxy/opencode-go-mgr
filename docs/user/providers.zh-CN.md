[English](providers.md)

# 供应商

要接入另一个上游或贡献内置集成，请先阅读[新增供应商](add-provider.zh-CN.md)；其中包含上游 HTTP 接口与密封注册表路径。

**供应商** 是供应商控制面——如果你的旧书签还挂着 `?view=pricing`，进来的就是这个视图。

底层是静态 Provider Registry 加几个按能力拆分的适配器。Custom API 只是其中一个 Configurable HTTP 适配器，不是基类，其他方案不会继承它。范围划分如下：

- 每个精确的内置 Provider/Offering 合约使用 `Provider(contract_scope_id)`；既有 scope ID 继续保留历史上类似 Provider ID 的取值。
- `CustomEndpoint(account_id)` 范围内的 Custom 映射仍归账号所有，不能在本页编辑。

左侧列出内置 Provider/Offering 合约范围。主区有两个子页签：**模型目录**与**价格**。原来的目录与模型合约视图合并为模型目录页签上的一张矩阵表。

**别名** 是独立的核心页面，因为它的只读表跨越所有 Provider 合约与 Custom 账号，而不属于当前选中的供应商。它把现有合约和账号能力汇总成公开名称，并展示可路由性与精确上游身份。它不会新建 Alias API、store、cache 或编辑器。Custom 映射通过 `?view=accounts&account_id=<id>` 跳转到**账号**页唯一的编辑器；加载该链接会打开对应账号编辑框。关闭编辑框会移除 `account_id`；账号不存在时会提示，并清掉失效参数。

**模型目录** 是本地的。矩阵只列出当前目录中的模型，并以三个上游协议（Chat Completions、Responses、Messages）为列。每格是 effective 模型/协议状态的二态开关：打开写入 `force_on`，关闭写入 `force_off`；列菜单可以整列打开或关闭。开关会先立即更新显示，再在后台执行带 CAS 保护的保存，只有受影响的格子显示保存进度。

底层静态、预设与探测证据仍保留在合约中，但紧凑矩阵不再显示独立徽标。显式开关或成功探测写入覆盖前，存储默认仍是 `auto`。供应商级探测成功会固定为 `force_on`；账号尝试失败会报告并保留证据，但不会把共享协议固定为 `force_off`，只有显式关闭开关才会这样做。

内置 **OpenCode Go**、**Zen Free**、**Command Code GOAT**、**MiniMax CN** 与 **Kimi Code CN** 的目录头部都提供 **恢复官方协议基线**。它不会请求上游，保留当前模型目录，清除手动开关和探测证据，并恢复在 **2026-09-01** 开发时核对的官方默认能力。OpenCode Go 与已知 Zen 行默认只开启官方为该模型列出的原生上游端点；GOAT 的 Anthropic 模型 ID 使用 Messages，其他供应商家族使用 Chat Completions，新发现且不属于内置预设的模型仍默认关闭；MiniMax CN 与 Kimi Code CN 都默认支持 Chat Completions 与 Messages，均不声明 Responses。官方基线未列出的协议保持关闭，只有管理员显式开启可构造路径或显式探测成功后才会写入额外正面证据。

轻量来源信息、刷新动作与矩阵共用同一块内容区域，不再有独立的目录摘要卡片，也没有刷新账号选择器。所有可刷新的范围使用同一个动作：OpenCode Go 由后端选择符合条件的 Go 账号访问官方鉴权目录；Zen Free 访问固定的官方无鉴权目录 `https://opencode.ai/zen/v1/models`；Command Code 直接访问固定的公开官方 `/models` 目录，不选择账号。刷新始终由用户显式触发。

MiniMax 与 Kimi 需要一个符合条件的账号 Key。MiniMax 刷新 `https://api.minimaxi.com/v1/models`；Kimi 刷新 `https://api.kimi.com/coding/v1/models`。保存的模型只激活代码内的密封映射；无法匹配的模型保留为精确 raw ID。MiniMax 把 M3、M2.7/M2.5/M2.1 的标准与 highspeed 变体，以及 M2 映射到对应的小写 kebab Alias。Kimi 映射为 `kimi-for-coding` → `kimi-k2.7-code`、`kimi-for-coding-highspeed` → `kimi-k2.7-code-highspeed`、`k3` → `kimi-k3`、`k3-256k` → `kimi-k3-256k`。转发始终保留每个准确的上游 ID。请求时不会临时访问上游。

Ollama Cloud 刷新公开且无需鉴权的目录 `https://ollama.com/v1/models`，不选择账号；该端点只用于目录发现，绝不是 Key 校验。目录精确 id（含尺寸变体等 `: ` 标签）原样登记并保留为 raw pin。家族固定只支持 Chat Completions：发现的行会立即启用 Chat，Responses 与 Messages 不受支持，页面也不提供协议探测入口。Ollama Cloud 绝不创建或抢占已发布别名：它对别名的唯一贡献，是在剥离 `: ` 标签后恰好命中一个目录 id 时，向 Go 拥有的别名（如共享的 `deepseek-v4-flash`）追加一个可路由映射。上游轮换导致同词干两个快照并存时，该映射自动退出——别名仍由既有家族照常服务，直到你在矩阵中钉定一个精确 id；钉定会被后续刷新尊重。带日期标签的快照 id 是运行时目录数据，绝不出现在代码或发版中。

首次成功刷新前，内置静态目录只是初始预设；刷新成功后，保存的官方快照成为权威目录并替代静态预设。刷新新增的模型会出现在矩阵中。OpenCode Go 与 Command Code 的新增协议单元格默认关闭，只有手动打开或测试成功后才会启用；MiniMax CN 与 Kimi Code CN 则直接启用密封合约中的 Chat Completions，Responses 与 Messages 不受支持。仍留在目录中的模型会保留既有覆盖与探测结果；刷新失败或结果为空时继续保留旧快照。

Custom API 继续使用账号所有的公开名称 → 上游 ID 映射，发现结果不会静默替换它们。账号表单里的 **获取模型** 只是未保存表单辅助，且只返回上游 ID。选择一个 ID 时，原样导入为“公开名称 = 上游 ID”，不剥离后缀、不生成 Alias。Command Code 使用官方公开的 `/models` 目录：GOAT 预设默认开启，后续发现的额外模型默认关闭，只有在矩阵中开启其受支持协议后才会供应；不再存在独立的 Max 或账号级 GOAT/全部模式。

本地目录会进入解析，请求时不会再访问上游。内置 Alias 权威是静态且由代码持有：最早 OpenCode Go 表提供 Go 名称，密封 MiniMax CN、Kimi CN 与选定 GOAT 长名称映射表提供供应商 Alias，但不会据此新增 Go 路由。Command 会先去掉 Provider 命名空间并复用已有代码持有的 Alias；只有短名已获授权时才去掉已知套餐后缀。例如 `nvidia/nemotron-3-ultra-550b-a55b` 使用 Alias `nemotron-3-ultra`。保存的 CN 行只激活其精确密封映射。无法匹配的内置模型保留为精确 raw ID，不会作为新 Alias 公布；CN 映射仍保留上游 ID 的准确拼写。Zen Free 只有在去掉 `-free` 后的名称已被 Go 表授权时才加入该 Alias，原始 `-free` ID 始终可作为精确 raw pin 使用，见 [Zen Free 模型](routing.zh-CN.md#zen-free-模型)。

当某个供应商的模型/协议单元格全部关闭时，该供应商不再产生路由。带鉴权的下游 `GET /v1/models` 只公布可路由的公开名称；raw-only 身份和 raw 名称冲突都会排除。歧义 raw 身份以 `ambiguous_model_id` 失败，绝不请求上游。

所有内置供应商的每行都有 **测试** 按钮，不需要指定账号。供应商会按已保存的路由顺序自动尝试符合条件的账号，并在首次成功后停止。OpenCode Go 与 Zen Free 使用各自可构造的协议集合；GOAT 只测试密封的原生家族路径（Anthropic ID 使用 Messages，其他 ID 使用 Chat Completions）；MiniMax CN 与 Kimi Code CN 只测试密封的 Chat Completions 路径。任何供应商都不会提交不受支持的 Responses 或错误家族探测。Custom 端点测试仍由具体账号所有。模型必须属于当前供应商目录，包括静态表尚未收录的新拉取模型。Popconfirm 会提示这些真实最小请求可能消耗额度。页面会在矩阵上方逐项展示成功、失败或跳过状态、HTTP 状态、可读的上游错误消息，以及上游给出时的安全帮助/计费链接；每个真实账号尝试都会写入脱敏的请求日志，协议探测内容不会进入运行日志。单个账号失败不会禁用其他符合条件账号可以服务的协议。

**价格** 按所选供应商限定范围。**刷新价格表** 只抓取并校验当前所选 Provider 自己的官方来源。OpenCode 与 Command Code 的 revision 和最后成功快照彼此独立；一个失败不会动另一个。以后某个 Provider 若包含多个有价格的 Plan，一次操作也只刷新该 Provider 内的 Plan。刷新仍只能手动发起：

- OpenCode Go 展示 revision、文档更新时间、token 单价、`Usage` 和额度扣减倍率，点击刷新后才会访问 `https://opencode.ai/docs/go/`。抓取或校验失败时继续使用最后一次成功快照。allowance 不是额度池、不会参与路由，只用于推导扣减倍率（“月额度 / Usage”）。临时覆盖会创建新的持久化 revision，供后续估算使用。
- Command Code GOAT 展示从 `https://commandcode.ai/docs/plans/goat` 保存的官方费率快照，不再在供应商页展示订阅月费或时间窗口额度卡。每个已定价模型的应用倍率都可手动修改并保存；新请求使用保存后的 Provider revision 计算，缺失或歧义行仍为 unpriced。刷新若将覆盖手动倍率会先请求确认。它与 OpenCode Go 分开；账号卡会把 OCG 内已定价请求日志投影到本地 `$14 / $35 / $70` 三个窗口，并允许手工修正，但不会把这称为官方实时用量。
- Zen Free 无价格（额度按出口 IP 共享）。
- Custom API 为 unpriced：成功转发记 `cost_state=unknown`，不扣额度，也没有官方用量刷新。
- Ollama Cloud unpriced：转发按 `cost_state=unpriced` 记账，不做 Go 价格换算；用量来自可选的 Cookie 抓取，目标固定为 `https://ollama.com/settings` 页（手动刷新，30 秒限速，失败按 5 分钟 → 15 分钟 → 1 小时 → 6 小时固定退避）。用量失败绝不写推理冷却，也不影响路由资格。
- MiniMax CN 与 Kimi Code CN 在 OCG 内为 unpriced，但账号卡可手工读取官方订阅窗口（`/token_plan/remains` 与 `/usages`）。这些快照只用于展示，不自动轮询，也不影响推理资格。

不存在按模型划分的额度池。

客户端请求不会探测：请求路径不会发现或探测。流程是：别名 → 账号资格 → 适配器上限 → 已保存合约 → 按模型/按协议 effective 状态 → 透传或转换。带鉴权的 `GET /v1/models` 与受保护的 `GET /dashboard/api/v3/application-models` 只公布当前可路由且 effective 协议已启用的公开名称。应用选择器仍是 Go 别名 ∩ 当前价格快照，不含 Custom。

---

[用户指南索引](../USER.zh-CN.md) · [English](providers.md) · [文档索引](../README.zh-CN.md)
