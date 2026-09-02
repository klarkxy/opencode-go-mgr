[English](known-debt.md)

# 已知缺口与明确非目标

## 已知缺口

- `auto_start` 受能力门控：只有 Windows release / 已安装的 Tauri 进程注入注册表同步钩子。开发构建、CLI、Docker、macOS、Linux 面板不暴露该开关。Dock 可见性仅 macOS Tauri。
- 生成的 Tauri schema 文件会让 diff 变吵；只在 Tauri 配置确实改动时才需要修改它们。
- 流式用量仅在上游发出 usage chunk 时精确；Chat 流式请求会设置 `stream_options.include_usage`。没有 chunk 时 Go 行记为 `success_no_usage`； Zen 无 usage 的成功仍为 `success` / `free`。
- 旧 `profiles/<account_id>` WebView Profile 不会迁移到外部 Chromium；升级后首次需要重新登录。旧路径只保留用于重置/删除时的安全清理，跨引擎无法直接复用。
- Responses 端点是无状态。`previous_response_id`、`conversation`、 `store: true`、`background: true` 直接返回 `400`，不会静默忽略。这是有意为之，详见 `protocol.rs` 和[用户指南](../USER.zh-CN.md)。
- Gemini 是客户端兼容格式，不是原生上游协议。仅 `generateContent` 与 `streamGenerateContent` 会转发；`countTokens` 与 `embedContent` 返回 `501`。非空 `safetySettings`、`cachedContent`、文件媒体、Google-hosted 工具等无法跨协议转换的语义返回 `400`。`topK`、`thinkingConfig` 仅为兼容提示，不保证在 Chat Completions 或 Messages 上游等价生效；其余非空 `generationConfig` 字段必须显式映射或返回 `400`，不会静默丢弃。
- Claude Desktop 只公布三个固定 Claude 别名，再映射到受支持的实际模型；它不代表 OCG Manager 提供了原生 Claude 4.6 模型或完整 Anthropic Models API。
- Command Code GOAT 没有可机读的官方用量端点。其公开模型目录不能验证已保存 Key，因此鉴权失败只能从真实推理 401/403 得知。Custom API 仍是独立的已上线路由，遵循受信管理员边界（`custom.rs` + `custom_http.rs`）。
- 按模型/按协议覆盖已在 V3；Custom 账号级按协议探测暂无 V3 对应端点，历史 V2 账号侧探测路径已 410。Custom 验证与模型发现是现行路径。

## 明确非目标

- 动态适配器/插件加载、用户自定义适配器实现，或持有 SQLite、`CoreState`、原始
  `reqwest::Client` 的适配器。类型化用户定义 Provider 仍受支持，但它只是绑定到
  密封 Configurable HTTP 适配器的数据。
- 远端节点同步、Admin API 或多租户控制面。
- Tauri `invoke` 不是面板数据路径；WebView command 保持移除。
- 不会在 `GET /v1/models` 或 `GET /dashboard/api/v3/application-models` 上做请求时上游发现。
- GOAT 官方权威用量 API，或把其公开目录当作 Key 验证。
- `/embeddings`、Gemini `embedContent`（501），或把 Gemini `countTokens` 做成真实上游计数（501 供 Gemini CLI 回退本地估算）。
- Gemini 不作为上游协议使用。
- 自动轮询价格或 Zen 目录。
- 旧 WebView Profile 不会跨引擎复用。
- 数据库降级，或让旧二进制打开更新后的 schema。
- Windows/Linux ARM64、32 位 x86、RPM、Snap、应用商店包、Windows Authenticode 或 Apple 公证。
- 在 GitHub provenance 之外再加一份 Cosign 镜像签名。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](known-debt.md) · [文档索引](../README.zh-CN.md)
