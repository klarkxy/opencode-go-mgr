[English](extending.md)

# 扩展 OCG Manager

扩展必须走以下三条明确路径之一。它们有意不同；不要为了复用名称而把新表面伪装成供应商。

## 1. 供应商或套餐：静态、密封

只用于 OCG 自己拥有完整路由、目录、协议、Key 与故障契约的上游家族。

1. 在 `ocg-domain`（`ids.rs`、`provider.rs`）加入身份与目录事实，为每个 Provider/Offering 合约声明唯一静态 `contract_scope_id`，并穷尽扩展 `ProviderAdapterKind`。既有 scope id 是兼容身份，不能复用，也不能在运行时临时推导。Custom 保持 `ConfigurableHttp`，不是超类。
2. 在 `ocg-domain::protocol` 加所需协议行，在 `ocg-gateway::alias` 加 Alias mapping。请求路径不会用来试探协议。
3. 在 `ocg-core` 实现只返回 `AttemptSpec` 的 `resolve_route`。适配器不能持有 DB、`CoreState` 或原始 reqwest client。
4. 控制面与路由语义未完成前保持 fail closed，完成后测试 domain、gateway 与 core 边界。

Provider 注册表始终静态、密封；不提供插件加载器、动态库、用户脚本或运行时发现的适配器。
同一 Provider 可以包含多个 Offering，但每个 Offering 都拥有独立合约范围、目录、证据与覆盖状态。

## 2. 应用连接器：本机 Desktop 能力

用于客户端配置或包接入。遵循应用教程/连接器边界：它由 Desktop Host 进程拥有，采用有文档的字段归属，不增加服务、daemon、远端同步路径或 Provider 注册表项。

## 3. 外部接入：静态本机服务适配器

用于用户自行本机部署、经过产品批准的服务。它在设置下方通用的 **扩展** 导航组中出现，不属于供应商、套餐或新增账号选择器。

- 定义窄的 typed Dashboard V3 contract 与 CAS 写入；不增加原始管理 API 代理或任意上游 path/body 转发。
- 明确数据归属：OCG 只保存连接与路由必需内容；外部服务保留自己的 OAuth Token、auth 文件、浏览器回调、内部调度和生命周期。
- 保持本机边界：桌面/CLI 仅回环，Docker 仅显式私有 Compose sibling。不得添加 LAN、互联网、跨节点、进程控制、自动升级、注册表或通用 SDK 表面。
- 只有产品契约明确要求时才复用 OCG 排序/选择/日志约定；不要虚构外部服务未提供的内部账号、费用或额度。

CPA 是此路径的第一个实例。第二个获批接入尚未证明共同需求前，不要抽取通用框架。

## Dashboard V3 端点变更

1. 在 `dashboard_v3/types.rs` 增加或扩展 DTO，并把新名字追加到 `CATALOG_TYPE_NAMES`；既有 `$defs` 不变。
2. 在 `dashboard_v3/mod.rs` 挂路由；写入走 `parse_mutation_json` 与 `check_expectation`，保持秘密脱敏。
3. 优先复用已有持久化/控制 helper，`dashboard_v3` 继续不依赖 `gateway`。
4. 补聚焦集成测试、更新 `src/api/dashboard-v3.ts`，并运行 `pnpm run contract:v3:check`。退役的 `/dashboard/api` REST 继续退役。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](extending.md) · [文档索引](../README.zh-CN.md)
