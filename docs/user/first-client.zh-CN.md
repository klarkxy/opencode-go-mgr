[English](first-client.md)

# 接入第一个客户端

Gateway 启动后，接入客户端主要是复制配置。请使用接入中心显示的完整 Base URL；多数 OpenAI 兼容客户端需要末尾的 `/v1`。

1. 在 **账号** 视图用官方分发的 API Key 添加一个 OpenCode Go 账号。登录账号可选；新增时如果先填写账号，它会自动作为必填名称，直到你手动修改名称。面板不收集或维护 OpenCode 登录密码。
2. 在面板的 **接入中心** 复制 **Key** 和 **API Base URL** （`http://127.0.0.1:9042/v1`）。
3. 把客户端指向该 Base URL 并填入 Key。**应用** 视图内置了 17 个常见客户端的教程。
4. 发一个真实请求验证。

**Key** 是你唯一要交给客户端的秘密。它接受三种请求头形态——`Authorization: Bearer <key>`、Anthropic 风格的 `x-api-key: <key>`、Gemini 风格的 `x-goog-api-key: <key>`——并且和 OpenCode-Go 账号 Key 毫无关系；后者由 Gateway 从 SQLite 取出后自行注入上游。

五类兼容入口的最小 POSIX shell 检查：

```bash
BASE=http://127.0.0.1:9042
KEY=replace-with-gateway-key

# OpenAI Chat Completions
curl "$BASE/v1/chat/completions" -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","messages":[{"role":"user","content":"ping"}],"stream":false}'

# OpenAI Responses
curl "$BASE/v1/responses" -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","input":"ping","store":false}'

# Anthropic Messages
curl "$BASE/v1/messages" -H "x-api-key: $KEY" \
  -H "anthropic-version: 2023-06-01" -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","max_tokens":16,"messages":[{"role":"user","content":"ping"}]}'

# Claude Desktop: the alias is rewritten to the model saved in the Applications view
curl "$BASE/claude-desktop/v1/messages" -H "x-api-key: $KEY" \
  -H "anthropic-version: 2023-06-01" -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4-6","max_tokens":16,"messages":[{"role":"user","content":"ping"}]}'

# Gemini generateContent
curl "$BASE/v1beta/models/deepseek-v4-flash:generateContent" \
  -H "x-goog-api-key: $KEY" -H "Content-Type: application/json" \
  -d '{"contents":[{"role":"user","parts":[{"text":"ping"}]}]}'
```

---

[用户指南索引](../USER.zh-CN.md) · [English](first-client.md) · [文档索引](../README.zh-CN.md)
