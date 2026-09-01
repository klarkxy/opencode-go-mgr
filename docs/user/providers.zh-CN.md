[English](providers.md)

# 供应商

要接入另一个上游或贡献内置集成，请先阅读[新增供应商](add-provider.zh-CN.md)；其中包含用户定义供应商、Custom API 与密封适配器注册表路径。

**供应商** 是供应商控制面——如果你的旧书签还挂着 `?view=pricing`，进来的就是这个视图。

适配器注册表保持静态密封。内置供应商与用户定义供应商共用本页，并分别标注 **内置** 或 **用户定义**。Custom API 是作为账号所有路径使用的 Configurable HTTP 适配器，不是大家继承的基类。范围划分如下：

- 每个精确的内置供应商合约使用 `Provider(contract_scope_id)`；既有 scope ID 继续保留历史上类似 Provider ID 的取值。
- 用户定义供应商作为类型化定义持久化，并绑定 Configurable HTTP。它们的 Endpoint、协议、鉴权方式和映射在本页编辑。
- `CustomEndpoint(account_id)` 范围内的 Custom 映射仍归账号所有，不能在本页编辑。

左侧列出内置合约范围和用户定义供应商。内置主区仍有三个子页签：**模型目录**、**价格** 与 **Alias**。用户定义面板展示配置、映射以及编辑/删除。用户定义供应商未定价。

**Alias** 是只读聚合页。它把现有 Provider 合约和账号能力汇总成公开名称，并展示可路由性与精确上游身份。它不会新建 Alias API、store、cache 或编辑器。Custom 映射通过 `?view=accounts&account_id=<id>` 跳转到**账号**页唯一的编辑器；加载该链接会打开对应账号编辑框。关闭编辑框会移除 `account_id`；账号不存在时会提示，并清掉失效参数。

**模型目录** 是本地的。矩阵只列出当前目录中的模型，并以三个上游协议（Chat Completions、Responses、Messages）为列。每格是 effective 模型/协议状态的二态开关：打开写入 `force_on`，关闭写入 `force_off`；列菜单可以整列打开或关闭。开关会先立即更新显示，再在后台执行带 CAS 保护的保存，只有受影响的格子显示保存进度。

底层静态、预设与探测证据仍保留在合约中，但紧凑矩阵不再显示独立徽标。显式开关或成功探测写入覆盖前，存储默认仍是 `auto`。供应商级探测成功会固定为 `force_on`；账号尝试失败会报告并保留证据，但不会把共享协议固定为 `force_off`，只有显式关闭开关才会这样做。

内置 **OpenCode Go**、**Zen Free**、**Command Code GOAT**、**MiniMax CN** 与 **Kimi Code CN** 的目录头部都提供 **恢复静态协议快照**。它不会请求上游，保留当前模型目录，清除手动开关和探测证据，并恢复日期为 **2026-08-27** 的静态协议快照。当前目录中未出现在该快照里的模型/协议对默认保持关闭，但有两类密封适配器例外：MiniMax CN 与 Kimi Code CN 只恢复 Chat Completions；GOAT 在该日期之后新增的模型会恢复供应商家族预设（Anthropic ID 使用 Messages，其他 ID 使用 Chat Completions）。这些 GOAT 行显示为“预设”，不会伪装成静态已验证或探测确认。仅刷新目录时，新发现的 GOAT 行仍默认关闭。

轻量来源信息、刷新动作与矩阵共用同一块内容区域，不再有独立的目录摘要卡片，也没有刷新账号选择器。所有可刷新的范围使用同一个动作：OpenCode Go 由后端选择符合条件的 Go 账号访问官方鉴权目录；Zen Free 访问固定的官方无鉴权目录 `https://opencode.ai/zen/v1/models`；Command Code 直接访问固定的公开官方 `/models` 目录，不选择账号。刷新始终由用户显式触发。

MiniMax 与 Kimi 需要一个符合条件的账号 Key。MiniMax 刷新 `https://api.minimaxi.com/v1/models`；Kimi 刷新 `https://api.kimi.com/coding/v1/models`。保存的模型只激活代码内的密封映射；无法匹配的模型保留为精确 raw ID。MiniMax 把 M3、M2.7/M2.5/M2.1 的标准与 highspeed 变体，以及 M2 映射到对应的小写 kebab Alias。Kimi 映射为 `kimi-for-coding` → `kimi-k2.7-code`、`kimi-for-coding-highspeed` → `kimi-k2.7-code-highspeed`、`k3` → `kimi-k3`、`k3-256k` → `kimi-k3-256k`。转发始终保留每个准确的上游 ID。请求时不会临时访问上游。

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
- MiniMax CN 与 Kimi Code CN 在 OCG 内为 unpriced，但账号卡可手工读取官方订阅窗口（`/token_plan/remains` 与 `/usages`）。这些快照只用于展示，不自动轮询，也不影响推理资格。

不存在按模型划分的额度池。

客户端请求不会探测：请求路径不会发现或探测。流程是：别名 → 账号资格 → 适配器上限 → 已保存合约 → 按模型/按协议 effective 状态 → 透传或转换。带鉴权的 `GET /v1/models` 与受保护的 `GET /dashboard/api/v3/application-models` 只公布当前可路由且 effective 协议已启用的公开名称。应用选择器仍是 Go 别名 ∩ 当前价格快照，不含 Custom。

---

[用户指南索引](../USER.zh-CN.md) · [English](providers.md) · [文档索引](../README.zh-CN.md)
