[English](protocol-conversion.md)

# 协议转换

OCG Manager 在一个端口上提供五种客户端协议，再把每份请求转换成上游 Plan 所需的格式。转换过程是确定性的：解析 Alias、检查账号资格、应用适配器上限与已保存的供应商合约、检查按模型/按协议的 effective 状态，然后决定透传或转换。强制关闭或全局关闭的协议优先于模型声明。

每个已知 OpenCode Go 模型从硬编码的 **推荐协议** 与 **已验证可用协议集合** 起步，由测试账号探测后写入代码；请求路径不做协议试探。在 **供应商** 页探测成功只能在该适配器上限内确认或新增支持，失败会被记录，但不会删除静态能力。客户端协议在集合内且 effective 启用时透传；否则 **请求体** 转到推荐上游协议，**响应体** 或 SSE 流转回客户端协议。Custom API 同样转到该账号声明的上游协议，再遵守该端点的合约与按模型覆盖。转换覆盖文本、system、图像、工具调用与结果、推理内容、完成状态、错误与 usage 字段。`grok-4.6`、`grok-4.5` 与 `gpt-5.6-luna` 均仅支持 Responses；`glm-5.3` 与 `glm-5.2` 均仅支持 Chat。其他客户端格式（包括 Gemini）会转换，不会触发上游协议试探。

| 推荐上游协议 | 模型 |
| --- | --- |
| OpenAI Chat Completions | `glm-5.3-flash`、`glm-5.3`、`glm-5.2`、`glm-5.1`、`glm-5`、`kimi-k3`、`kimi-k2.7-code`、`kimi-k2.6`、`kimi-k2.5`、`deepseek-v4-pro`、`deepseek-v4-flash`、`deepseek-v4-flash-vision-exp`、`mimo-v2.5`、`mimo-v2.5-pro`、`hy3`、`longcat-2.0`、`ox-alpha-free`、`big-pickle`、`hy3-free`、`deepseek-v4-flash-free`、`mimo-v2.5-free`、`ling-3.0-flash-free`、`laguna-s-2.1-free`、`longcat-2.0-free`、`north-mini-code-free`、`nemotron-3-ultra-free`、`nemotron-3.5-lightning-free`、`ling-3.0-flash-fin-free`、`hy4-preview` |
| OpenAI Responses | `grok-4.6`、`grok-4.5`、`gpt-5.6-luna`、`muse-spark-1.2`、`muse-spark-1.2-contributor`、`muse-spark-1.2-contributor-free` |
| Anthropic Messages | `minimax-m3`、`minimax-m2.7`、`minimax-m2.7-highspeed`、`minimax-m2.5`、`minimax-m2.5-highspeed`、`qwen3.8-max`、`qwen3.8-flash`、`qwen3.7-max`、`qwen3.7-plus`、`qwen3.6-plus`、`qwen3.5-plus` |

透传矩阵（检入的官方基线，2026-09-01）。✓ = 客户端协议原样转发；空 = 基线没有该协议的直接透传证据。模型是否可路由仍由 Provider 目录与 effective 合约决定；已知但不符合准入条件的模型会在本机被拒绝，不会转换或发送到上游。权威来源：`crates/ocg-domain/src/protocol.rs` 的 `MODEL_PROTOCOLS`。

`reasoning.effort` 别名（转发或转换前应用）：`muse-spark-1.2`、 `muse-spark-1.2-contributor` 与 `muse-spark-1.2-contributor-free` 把 `max` 映射为 `xhigh`（上游拒绝 `max`）；其他模型的 `reasoning.effort` 原样透传。

| 模型 | 推荐 | Chat | Responses | Messages |
| --- | --- | :---: | :---: | :---: |
| `grok-4.6` | Responses | | ✓ | |
| `grok-4.5` | Responses | | ✓ | |
| `glm-5.3-flash` | Chat | ✓ | | |
| `glm-5.3` | Chat | ✓ | | |
| `glm-5.2` | Chat | ✓ | | |
| `glm-5.1` | Chat | ✓ | | |
| `glm-5` | Chat | ✓ | | |
| `gpt-5.6-luna` | Responses | | ✓ | |
| `muse-spark-1.2` | Responses | | ✓ | |
| `muse-spark-1.2-contributor` | Responses | | ✓ | |
| `muse-spark-1.2-contributor-free` | Responses | | ✓ | |
| `kimi-k3` | Chat | ✓ | | |
| `kimi-k2.7-code` | Chat | ✓ | | |
| `kimi-k2.6` | Chat | ✓ | | |
| `kimi-k2.5` | Chat | ✓ | | |
| `deepseek-v4-pro` | Chat | ✓ | | |
| `deepseek-v4-flash` | Chat | ✓ | | |
| `deepseek-v4-flash-vision-exp` | Chat | ✓ | | |
| `mimo-v2.5` | Chat | ✓ | | |
| `mimo-v2.5-pro` | Chat | ✓ | | |
| `hy3` | Chat | ✓ | | |
| `longcat-2.0` | Chat | ✓ | | |
| `ox-alpha-free` | Chat | | | |
| `big-pickle` | Chat | ✓ | | |
| `hy3-free` | Chat | ✓ | | |
| `deepseek-v4-flash-free` | Chat | | | |
| `mimo-v2.5-free` | Chat | ✓ | | |
| `ling-3.0-flash-free` | Chat | | | |
| `laguna-s-2.1-free` | Chat | | | |
| `longcat-2.0-free` | Chat | | | |
| `north-mini-code-free` | Chat | | | |
| `nemotron-3-ultra-free` | Chat | ✓ | | |
| `nemotron-3.5-lightning-free` | Chat | ✓ | | |
| `ling-3.0-flash-fin-free` | Chat | ✓ | | |
| `hy4-preview` | Chat | ✓ | | |
| `minimax-m3` | Messages | | | ✓ |
| `minimax-m2.7` | Messages | | | ✓ |
| `minimax-m2.7-highspeed` | Messages | | | |
| `minimax-m2.5` | Messages | | | ✓ |
| `minimax-m2.5-highspeed` | Messages | | | |
| `qwen3.8-max` | Messages | | | ✓ |
| `qwen3.8-flash` | Messages | | | ✓ |
| `qwen3.7-max` | Messages | | | ✓ |
| `qwen3.7-plus` | Messages | | | ✓ |
| `qwen3.6-plus` | Messages | | | ✓ |
| `qwen3.5-plus` | Messages | | | ✓ |

未知模型名在所有支持的客户端格式上直接返回 `400`——Chat Completions、Responses、Messages，以及 Gemini `generateContent` / `streamGenerateContent`——未知 Claude Desktop 别名也一样。Gateway 不靠试探选协议——那会把同一请求计两次费。见 [别名](gateway.zh-CN.md#别名)。

Gateway 协议端点最多接受 16 MiB 的 JSON 请求体；这是传输上限，不是上下文窗口。若 OCG Manager 前面还有反向代理，需把请求体上限设为至少 16 MiB，否则请求可能还没到达 Gateway 就被代理以 `413 Payload Too Large` 拒绝。

## Responses 是无状态端点

下列字段会直接 `400` 拒绝，不会静默忽略：

- `previous_response_id`
- `conversation`
- `store: true` 或任何不是 `false` 的 `store`
- `background: true`
- `input_image.file_id`（Gateway 没有 Files API）

function、custom、namespace 工具正常转换。`web_search`、`web_search_preview`、 `tool_search` 等 OpenCode-Go 不支持的托管工具在自动工具模式下会被丢弃；显式强制使用则返回 `400`。

## Gemini 是客户端兼容层

Gateway 不会把 Gemini 线格式数据发往上游。它把 `contents`、纯文本 `systemInstruction`、受支持的 `inlineData` 图片、`functionDeclarations`、函数调用/结果、JSON Schema 输出、生成选项、Google 错误信封、usage 元数据和 SSE 帧，转换到已知模型的 Chat Completions 或 Messages 原生协议并转回。`v1beta` 与 `v1` 两种 URL 形式都接受。

兼容边界——不会静默假装等价：

- 非空 `safetySettings` 无法跨协议执行同一套内容安全阈值，直接返回 `400 INVALID_ARGUMENT`；省略、`null` 或空数组可以使用。`safetySettings` 只影响 Gateway 是否接受请求，不会作为上游执行的提示生效。
- `generationConfig.topK` 与 `generationConfig.thinkingConfig` 只作为跨协议兼容提示接受；采样、推理预算和 thoughts 展示不保证与 Google Gemini 等价，实际能力由所选 OpenCode-Go 模型决定。
- 其他无法跨协议保留的非空生成选项（包括 `seed`、presence/frequency penalty、 logprobs 与 media resolution）会返回 `400`，不会静默丢弃。
- `cachedContent`、`fileData`、Google Search、URL Context、Code Execution、多模态 function response、function response 的 schema/behavior、`VALIDATED` 函数调用模式、`candidateCount` 大于 1、非 TEXT 输出模态会返回 `400`。图片请改用 base64 `inlineData`，支持 PNG、JPEG、GIF、WebP。
- `countTokens` 与 `embedContent` 返回 `501 UNIMPLEMENTED`；Gemini CLI 对前者失败可使用本地估算，Gateway 当前没有 embeddings 路由。

## Claude Desktop 别名

专用入口只接受服务端公布的 `claude-sonnet-4-6`、`claude-opus-4-6`、 `claude-haiku-4-5-20251001` 三个别名。Gateway 在进入现有 Messages 转换链前，把别名替换成“应用”视图保存的实际模型；响应中的模型能力、工具支持和上下文限制仍以实际模型为准。`sonnet`、`opus`、`haiku` 映射序列化在 `AppConfig` 中；留空角色继承第一个已配置角色，面板返回补全后的三角色映射。

---

[用户指南索引](../USER.zh-CN.md) · [English](protocol-conversion.md) · [文档索引](../README.zh-CN.md)
