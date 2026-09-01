[English](external-integrations.md)

# 外部接入

> **当前状态：** CPA 暂留给后续独立测试。常规面板入口已隐藏，路由保持强制关闭；下述实现说明仅作为后续测试参考保留。

外部接入是可选、受本产品支持的本机服务。它扩展 OCG Manager，但不是供应商、套餐或插件。管理面板仍保留八个核心页面；只有重新通过验证门槛的扩展才会显示入口。

## CPA

CPA（CLI Proxy API）是本机订阅运行时。OCG Manager 可管理其当前稳定支持的 Codex、Claude、Antigravity、Kimi 和 xAI 账号流程，并路由得到的订阅池；但 OAuth 浏览器会话、Token、auth 文件和内部调度始终由 CPA 持有。OCG 只保存本地连接配置、两把 CPA 凭据和本地模型快照。

只支持以下本机部署：

- **桌面版或 CLI：** 在同一台机器运行 CPA，并配置回环地址，例如 `http://127.0.0.1:8317`。
- **Docker：** 启用 [Docker](docker.zh-CN.md) 中的可选 Compose 并列服务。OCG 使用只读的 `http://cpa:8317` 服务地址；面板不接受局域网、互联网或跨节点 CPA 地址。

CPA 有意不做远程接入。包含内嵌凭据、query、fragment、重定向或非回环主机的 URL 会被拒绝。不要把 OCG Manager Key 复用为任一 CPA Key。

### 通过验证门槛后的运维

下述流程描述的是已暂存的实现；CPA 仍被隐藏并强制关闭时，常规管理面板不提供这些操作。只有后续版本重新打开 CPA 验证门槛后才按此流程操作。

1. 在本机安装并启动 CPA，分别创建 **Management Key** 与 **Inference Key**。
2. 待入口恢复后打开 **扩展 → CPA**，保存本地地址和两把 Key，再运行连接检测。它分别显示可达性、受支持的 CPA 版本、Management 鉴权和 Inference 鉴权。OCG 要求 CPA 7.1.0 或更高版本；更高 major 仍继续接受相同的 typed 响应与精确账号校验，不会只因版本号被拒绝。
3. 在 CPA 账号表中发起 OAuth。浏览器回调类 provider 使用 CPA 自己的回环回调端口；Kimi 与 xAI 使用设备码流程。OCG 不会运行 OAuth 回调服务器，刷新页面或重启后也不会恢复旧流程。
4. 刷新 CPA 模型目录，并在就绪后启用 CPA 订阅池。Accounts 页中的 **CPA 订阅池** 单例卡可像其他路由候选一样排序、启停，但不会暴露 Key、不能删除，也不会把 CPA 内部 OAuth 账号伪装成 OCG 账号。

停用订阅池只会移出路由，不会忘记 CPA 配置。经确认的 **断开并清除** 会删除 OCG 保存的 CPA 配置、订阅池卡和本地模型快照；不会删除 CPA 自己的 OAuth 文件。CPA 故障只会让当前路由跳过该候选，其他合格 OCG 账号仍可继续被选择。

## 新增其他接入

静态外部接入只是 **扩展** 分组可以容纳的一类非核心功能。它们必须经过产品批准，使用 typed Dashboard V3 adapter 和有文档的本机边界；不支持动态 Provider 插件、用户脚本、通用管理 API 代理或运行时加载适配器。

---

[用户指南索引](../USER.zh-CN.md) · [English](external-integrations.md) · [文档索引](../README.zh-CN.md)
