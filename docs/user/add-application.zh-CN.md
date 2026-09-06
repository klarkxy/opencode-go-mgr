[English](add-application.md)

# 新增应用

当某个客户端没有出现在 **应用** 页面时，使用这份指南。只要应用能为一种受支持协议配置自定义 Base URL、API Key 和模型 ID，就先手动接入。内置教程卡片与桌面自动连接器是两个彼此独立的可选贡献。

| 目标 | 接入层级 |
| --- | --- |
| 立即使用未收录应用 | 手动配置 Gateway |
| 在 **应用** 页发布可复制教程 | 新增 `ApplicationGuide` |
| 让已安装桌面版修改该客户端的本机配置 | 手动教程验证成功后，再新增静态桌面连接器 |

## 接入未收录客户端

从 **接入中心** 复制 **Key** 与地址，选择客户端本身已经支持的接口：

| 客户端协议 | 常见 Base 值 | Open Console Gateway 请求路径 | 鉴权 |
| --- | --- | --- | --- |
| OpenAI Chat Completions | `http://127.0.0.1:9042/v1` | `POST /v1/chat/completions` | `Authorization: Bearer <key>` |
| OpenAI Responses | `http://127.0.0.1:9042/v1` | `POST /v1/responses` | `Authorization: Bearer <key>` |
| Anthropic Messages | 客户端会追加 `/v1/messages` 时使用 `http://127.0.0.1:9042` | `POST /v1/messages` | `x-api-key: <key>` |
| Gemini | `http://127.0.0.1:9042`，API 版本为 `v1beta` | `POST /v1beta/models/{model}:generateContent` 或 `:streamGenerateContent` | `x-goog-api-key: <key>` |
| Claude Desktop Gateway | `http://127.0.0.1:9042/claude-desktop` | `POST /claude-desktop/v1/messages` | Static API key / Bearer |

若客户端要求填写 **完整 Endpoint** 而不是 Base URL，就使用表中的请求路径。若它会自动追加 `/v1`，填写根地址；若它要求 OpenAI API Base，通常填写带 `/v1` 的地址。最终以该客户端的官方文档为准。

使用本地带鉴权模型发现返回的准确模型：

```bash
curl http://127.0.0.1:9042/v1/models \
  -H "Authorization: Bearer <key>"
```

这份列表是本地读取，包含当前可路由且由代码持有的 Alias 与合格 Custom ID。面板应用选择器是[应用教程](applications.zh-CN.md)中的 Go 可路由交集。五类接口的最小请求体见[接入第一个客户端](first-client.zh-CN.md)。

配置完成后发送一次真实请求，并在 **日志** 中确认。

## 新增应用教程

教程注册表位于 `src/views/application-guides.ts`。教程负责展示与生成可复制配置。

1. 核对客户端当前官方文档，选择一个 `endpointKind`：`chat`、`responses`、`messages` 或 `gemini`。
2. 添加稳定的 kebab-case `id`、显示名称、分类、协议、官方文档地址、短摘要、按顺序排列的步骤、操作注意事项，以及一个或多个动态配置片段。
3. 用 `GuideContext` 提供的地址与模型选择生成片段。可见预览使用 `displayKey`，只有通过现有 keyed-snippet helper 生成的复制值才使用 `actualKey`；完整 Key 不得进入日志、测试、标签或静态源码。
4. 只有客户端确实拥有对应模型设置时才使用 `modelFields`；只有它确实消费多个已选模型时才使用 `multipleModels`；快捷动作必须安全且受当前 UI 支持。
5. 更新 `src/views/dashboard-connection.test.ts`：预期教程数量、ID 唯一性、官方文档地址、脱敏断言与客户端专用输出断言。同步教程使用的翻译。
6. 运行 `pnpm run test:web` 与 `pnpm run build:web`。若模型能力元数据也变化，还要同步[应用教程与模型能力](applications.zh-CN.md)中的能力表。

合格教程应明确告诉用户：客户端需要哪种 URL 形式、Key 存在哪里、使用什么协议、如何选择模型、是否需要重启，以及怎样在 OCG 日志中证明第一条请求。

## 新增桌面自动连接器

自动连接器是本机已安装桌面版针对少量静态客户端提供的能力。CLI、Docker 与远程面板使用手动教程。只有目标配置稳定且有文档，并且 connect/restore 能保留所有无关字段时，才适合新增。

本机面板使用以下受 session 保护的 V3 接口：

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| `GET` | `/dashboard/api/v3/applications/connectors` | 检查静态连接器集合，不返回 Key |
| `POST` | `/dashboard/api/v3/applications/connectors/{id}/preview` | 生成脱敏的 connect/restore 计划与文件指纹 |
| `POST` | `/dashboard/api/v3/applications/connectors/{id}/commit` | 在 CAS 与指纹检查下执行同一份预览 |

Preview 只接受 `action`、可选 `keyId` 与 `modelValues`。调用方不能传入目标路径、Gateway URL、配置文本或明文 Key。Commit 还需要 `expectedRevision`、`processGeneration` 与 `previewFingerprint`。revision 过期或目标文件变化时必须交给用户处理，不能自动重放。

实现所有权按层拆分：

- 在 `crates/ocg-core/src/application_connectors.rs` 添加静态连接器身份与不含秘密的 DTO 行为。
- 在 `src-tauri/src/host/application_connectors.rs` 实现固定目标检测、字段级合并、预览、原子提交、恢复、权限与失败补偿，再把 Host capability 注册到 `CoreState`。
- 同步 `src/views/Applications.vue` 中显式的连接器集合与 UI 状态。Pi/DSH 原生包属于 `integrations/`，并遵循各客户端的原生凭据规则。
- 添加 Core/V3、Desktop Host 与前端测试。按[开发](../maintainer/development.zh-CN.md)对这些表面跑对应检查。

受支持的路径是通过上述受 session 保护的 V3 preview/commit 接口实现的静态桌面连接器。自动检测或写入不受支持时，手动教程始终保留。仓库约束见[扩展 Open Console Gateway](../maintainer/extending.zh-CN.md)与[编码约定](../maintainer/conventions.zh-CN.md)。

---

[用户指南索引](../USER.zh-CN.md) · [English](add-application.md) · [新增供应商](add-provider.zh-CN.md) · [文档索引](../README.zh-CN.md)
