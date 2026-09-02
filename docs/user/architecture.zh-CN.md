[English](architecture.md)

# 架构图

这是一组单个本地节点的文字版图。按当前 HEAD：已上线路由是 OpenCode Go、Zen Free、Command Code GOAT、MiniMax CN Token Plan、Kimi Code CN、Custom API，以及你在 **供应商** 页创建的用户定义供应商。每张图下面链接到负责详情的章节；图与章节冲突时，以章节和代码为准。

## 目录

- [同一节点、同一端口](#同一节点同一端口)
- [一次客户端请求](#一次客户端请求)
- [Plan](#plan)
- [面板、Key 与账号卡](#面板key-与账号卡)
- [两份本地模型列表](#两份本地模型列表)
- [协议转换](#协议转换)
- [接下来读哪一章](#接下来读哪一章)

## 同一节点、同一端口

桌面、CLI 与 Docker 只是同一份 `ocg-core` 进程的三种启动方式。默认绑定 `127.0.0.1:9042`。托盘应用用系统浏览器打开面板，不会通过 Tauri `invoke` 远程操控面板。没有远端同步、Admin API 或遥测。

```text
   桌面托盘              CLI `serve`           Docker
   (ocg-manager)      (ocg-manager-cli)   (ghcr.io/.../opencode-go-mgr)
           \                  |                    /
            \                 |                   /
             +----------------+------------------+
             |            ocg-core               |
             |         127.0.0.1:9042            |
             +----------------+------------------+
                    /                    \
                   /                      \
        GET /dashboard/              推理入口
        Vue 3 SPA                    /v1/chat/completions
        （系统浏览器）                /v1/responses
                                     /v1/messages
                                     /v1/models
                                     Gemini generateContent
                                     /claude-desktop/v1/...
                   \                      /
                    \                    /
             +----------------+------------------+
             |         SQLite schema v35         |
             |  桌面  ~/.ocg-mgr                 |
             |  CLI   ~/.ocg-mgr-cli             |
             +-----------------------------------+
```

安装、首个客户端、CLI 与 Docker：[安装](install.zh-CN.md)、
[接入第一个客户端](first-client.zh-CN.md)、[CLI](cli.zh-CN.md)、
[Docker](docker.zh-CN.md)。

## 一次客户端请求

面板签发的 **Key** 让客户端接入本节点。选中账号的凭据才是本节点发给上游的东西——Zen Free 没有上游 Key。额度条只是警告，不会停流量；只有上游 `429` 会冷却一张卡。

```text
  AI 客户端                         本节点                          Plan
  ---------                         ------                          ----
      |                                  |                              |
      |  Key + 别名 / Custom ID          |                              |
      |--------------------------------->|                              |
      |                                  | 1. 校验 Key                  |
      |                                  |    Bearer / x-api-key /      |
      |                                  |    x-goog-api-key            |
      |                                  | 2. 解析别名                  |
      |                                  | 3. 挑选可用账号卡            |
      |                                  | 4. 透传或转换协议            |
      |                                  |----------------------------->|
      |                                  |                              |
      |                                  |<-----------------------------|
      |                                  | 5. 转换响应                  |
      |                                  | 6. 记录 requested_model、    |
      |                                  |    resolved_alias、          |
      |                                  |    upstream_model            |
      |<---------------------------------|                              |
```

`GET /v1/models` 是本地列表，不会访问上游。未知模型名在 Chat、Responses、
Messages 与 Gemini generate / stream 上均为 `400`。重叠的原始 ID 返回
`400` `ambiguous_model_id`，且不会调用上游。

鉴权、别名、选择与熔断：[Gateway 行为](gateway.zh-CN.md)、
[路由](routing.zh-CN.md)。

## Plan

每张账号卡对应一个 Plan（只有 `provider_id`）。内置家族以及你创建的用户定义供应商均可路由。

```text
  已上线（可路由）
  ----------------
  OpenCode Go
    官方 Key，/zen/go
  Zen Free
    无上游 Key
    在供应商页刷新目录
  Command Code GOAT
    官方 Provider API；在供应商页刷新目录
    GOAT 预置行默认开启；额外发现行默认关闭
  MiniMax CN Token Plan
    固定官方 Chat 路由；需认证的目录刷新
  Kimi Code CN
    固定官方 Chat 路由；需认证的目录刷新
  Custom API
    一个受信管理员 HTTP/HTTPS API URL：根地址、/v1 基址或兼容的完整 Endpoint
    一个上游协议；鉴权自动推导
  用户定义供应商
    在供应商页保存类型化定义；绑定 Configurable HTTP
    Endpoint/协议/鉴权/映射归供应商所有
    账号 Key/启停/顺序留在账号页；价格与官方用量始终未知


  Custom API 生命周期

    保存 / 更新  ->  pending 时也可启用
           |
           v
    用第一个声明模型验证所选协议
    （向解析后的推理 Endpoint 发送一次最小非流式请求；
     须返回一个 2xx JSON object）
           |
           v
    验证状态变为 verified
    （账号可能已在路由中）

  Key、API 地址、声明能力或协议变更
  会使验证状态变为 pending，但保持该卡启用。
```

Zen Free 只有启用开关；不需要时直接关掉卡片。目录刷新在供应商页，不在账号卡上。账号与供应商见 [账号](accounts.zh-CN.md) 和 [供应商](providers.zh-CN.md)。

## 面板、Key 与账号卡

侧栏有八个核心视图，包含 **别名**。`browser` 是托管会话覆盖页。设置下方的可选 Extensions 分组提供本机 CPA 接入入口。SPA 读写 `/dashboard/api/v3`。回环监听默认跳过面板登录（带转发头时仍需登录）；客户端访问 `/v1` 仍然需要 Key。

```text
  Dashboard -> Access Keys -> Accounts -> Providers
      ^                                      |
      |                                      v
  Settings <- Logs <- Applications <---------+

  接入中心（Dashboard 首屏）
    复制 API 根地址 / Key / 轮换当前 Key
  Access Keys
    创建、重命名、启用、删除、重置
    主 Key 不可禁用、不可删除


  两种密钥、两个方向

    AI 客户端 --Key--> 本节点 --账号凭据--> Plan

    Key            access_keys（当前 schema v35）
                   主 Key + 可选子 Key（活跃上限 64）
    账号凭据       Go Key、Custom Key，或 Zen Free（无）
```

唯一会返回 Key 明文的 V3 响应是 `GET /dashboard/api/v3/connection`。视图、CAS 与数据目录见 [管理面板](dashboard.zh-CN.md) 和 [数据与安全](data-security.zh-CN.md)。

## 两份本地模型列表

这两条 GET 都不会在请求时做上游发现。目录刷新必须由管理员在供应商页显式触发，不属于任何一条 GET。

```text
  GET /v1/models                         （客户端；需要 Key）
    Go 表或密封 CN 映射授权且当前可路由的 Alias
      ∪ 合格 Custom 声明 ID
    保存行只激活代码持有的映射；未知行只保留 raw ID
    合格 Custom = enabled + ready + 非空 Key（验证为可选）
    Custom ID 不得抢走已公布的内置 Alias

  GET /dashboard/api/v3/application-models   （面板会话）
    Go 可路由别名 ∩ 当前 Go 价格快照
    highspeed 变体继承基价行
    空交集为 []
    不含 Custom ID

  GET /claude-desktop/v1/models
    只公布三个角色别名（sonnet / opus / haiku）
```

应用选择器与客户端列表：[应用教程](applications.zh-CN.md)、
[Gateway 行为](gateway.zh-CN.md)。

## 协议转换

请求路径不会试探协议——避免双计费。Gemini 只是客户端格式；Gateway 不会把流量发到 Google。

```text
  客户端线路
    Chat Completions / Responses / Messages / Gemini generateContent
           |
           v
  已解析别名并选中账号卡
           |
           +-- 客户端协议 ∈ supported 且已启用？ -- 是 --> 透传
           |                                              |
           否                                             |
           v                                              |
  把请求体转到该 Plan 的 preferred / 声明上游协议         |
           |                                              |
           +----------------------+-----------------------+
                                  v
                             上游 Plan
                                  |
                                  v
                      把响应（或 SSE）转回客户端协议
```

推荐/已验证协议表与转换边界：[协议转换](protocol-conversion.zh-CN.md)。

## 接下来读哪一章

| 如果你要… | 打开 |
| --- | --- |
| 安装与第一条 curl | [安装](install.zh-CN.md)、[接入第一个客户端](first-client.zh-CN.md) |
| 额度条与真正的冷却 | [路由](routing.zh-CN.md) |
| 出站代理模式 | [日志与设置](logs-settings.zh-CN.md) |
| crate DAG、`host_router`、executor | [维护者架构](../maintainer/architecture.zh-CN.md) |

---

[用户指南索引](../USER.zh-CN.md) · [English](architecture.md) · [文档索引](../README.zh-CN.md)
