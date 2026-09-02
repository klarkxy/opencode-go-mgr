[English](overview.md)

# 产品定位

OCG Manager 是一台本地 Gateway：把受支持的供应商 Plan Key 与受信的 Custom API 目的地保存在 SQLite 数据库里，并通过回环地址 `http://127.0.0.1:9042/v1` 暴露给客户端。每张账号卡对应一个 **Plan**（provider + offering）。客户端发送本地注册表里的 **别名** 或符合要求的 Custom 模型 ID；当前可路由的是 OpenCode Go、Zen Free、Command Code GOAT、MiniMax CN Token Plan、Kimi Code CN 与 Custom API。Vue 3 管理面板在 `/dashboard/`，当前 SPA 通过 `/dashboard/api/v3` 读写 JSON。每个节点独立运行——没有远端同步，没有 Admin API，也没有遥测。

Gateway 只做四件事，顺序基本符合直觉：

1. 用面板签发的 **Key** 验证客户端。
2. 用本地 Alias 注册表（以及合格 Custom 声明 ID）解析客户端模型名，再经能力过滤、适配器上限、已保存的供应商合约，以及按模型协议 effective 状态后挑一张可用账号卡。
3. 把请求转换到所选 Plan 的有效上游协议，再把响应转回客户端协议。客户端请求路径不会发现或探测。
4. 把请求日志（`requested_model`、`resolved_alias`、`upstream_model`）、用量、冷却全部写回 SQLite，并在面板里呈现。

## 一个节点长什么样

桌面端、CLI 和 Docker 都运行同一个 `ocg-core` 进程，默认绑定 `127.0.0.1:9042`。
面板在系统浏览器里打开；AI 客户端用 OpenAI、Anthropic、Gemini 或 Claude Desktop
格式访问 `/v1`。

```text
   桌面托盘 / CLI `serve` / Docker
                    |
                    v
              ocg-core @ 127.0.0.1:9042
               /                    \
    /dashboard/  Vue SPA          /v1  推理
    （系统浏览器）                 客户端 + Key
               \                    /
                v                  v
              SQLite schema v35（仅本地）
```

请求路径、Plan、八个面板视图和协议转换的文字图见
[架构图](architecture.zh-CN.md)。

---

[用户指南索引](../USER.zh-CN.md) · [English](overview.md) · [文档索引](../README.zh-CN.md)
