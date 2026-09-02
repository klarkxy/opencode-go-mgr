[简体中文](protocol-conversion.zh-CN.md)

# Protocol Conversion

OCG Manager speaks five client protocols on one port, then translates each
request into whatever the upstream Plan actually understands. The conversion
layer is deterministic: it resolves the Alias, checks account
eligibility, applies the adapter ceiling and saved provider contract, checks
the per-model/per-protocol effective state, and only then passthroughs or
converts. A force_off or globally closed protocol wins — even if the model
claims it supports it.

Each known OpenCode Go model starts from a hardcoded **preferred** protocol
and a **supported** set, maintained after test-account probes; the request
path does not discover protocols. A successful probe on **Providers** can
confirm or add support only within that adapter ceiling; failures are recorded
but never remove static capability. If the client protocol is supported and
effectively enabled, request and response pass through. Otherwise the gateway
converts the **request body** to the preferred upstream protocol and the
**response body** — or SSE stream — back to the client protocol. Custom API
does the same to the account's declared upstream protocol, then honors that
endpoint's contract and per-model overrides. Conversion covers text, system
instructions, images, tool calls and results, reasoning content, completion
status, errors, and usage fields. `grok-4.6`, `grok-4.5`, and
`gpt-5.6-luna` are Responses-only; `glm-5.3` and `glm-5.2` are Chat-only.
Other client formats, including Gemini, convert instead of triggering an
upstream protocol trial.

| Preferred upstream | Models |
| --- | --- |
| OpenAI Chat Completions | `glm-5.3-flash`, `glm-5.3`, `glm-5.2`, `glm-5.1`, `glm-5`, `kimi-k3`, `kimi-k2.7-code`, `kimi-k2.6`, `kimi-k2.5`, `deepseek-v4-pro`, `deepseek-v4-flash`, `deepseek-v4-flash-vision-exp`, `mimo-v2.5`, `mimo-v2.5-pro`, `hy3`, `longcat-2.0`, `ox-alpha-free`, `big-pickle`, `hy3-free`, `deepseek-v4-flash-free`, `mimo-v2.5-free`, `ling-3.0-flash-free`, `laguna-s-2.1-free`, `longcat-2.0-free`, `north-mini-code-free`, `nemotron-3-ultra-free`, `nemotron-3.5-lightning-free`, `ling-3.0-flash-fin-free`, `hy4-preview` |
| OpenAI Responses | `grok-4.6`, `grok-4.5`, `gpt-5.6-luna`, `muse-spark-1.2`, `muse-spark-1.2-contributor`, `muse-spark-1.2-contributor-free` |
| Anthropic Messages | `minimax-m3`, `minimax-m2.7`, `minimax-m2.7-highspeed`, `minimax-m2.5`, `minimax-m2.5-highspeed`, `qwen3.8-max`, `qwen3.8-flash`, `qwen3.7-max`, `qwen3.7-plus`, `qwen3.6-plus`, `qwen3.5-plus` |

Passthrough matrix (checked-in official baseline, 2026-09-01). ✓ = the client
protocol is forwarded as-is; empty = the baseline has no direct-passthrough
evidence for that protocol. Provider catalogs and effective contracts still
decide whether the model is routeable; a known but inadmissible model is
rejected locally rather than converted or sent upstream. Source of truth:
`MODEL_PROTOCOLS` in `crates/ocg-domain/src/protocol.rs`.

`reasoning.effort` aliases (applied before forwarding or conversion):
`muse-spark-1.2`, `muse-spark-1.2-contributor`, and
`muse-spark-1.2-contributor-free` map `max` → `xhigh` (upstream rejects
`max`). Other models pass `reasoning.effort` through unchanged.

| Model | Preferred | Chat | Responses | Messages |
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

Unknown model names return `400` on every supported client format — Chat
Completions, Responses, Messages, and Gemini `generateContent` /
`streamGenerateContent` — and unknown Claude Desktop aliases do too. The
gateway refuses to guess a protocol by trial — that would bill the request
twice. See [Aliases](gateway.md#aliases).

Gateway protocol endpoints accept JSON request bodies up to 16 MiB. That is
a transport limit, not a context-window limit. If a reverse proxy sits in
front of OCG Manager, allow at least 16 MiB request bodies or the proxy may
return `413 Payload Too Large` before the gateway sees the request.

## Responses is stateless

The following fields return `400` instead of being silently ignored:

- `previous_response_id`
- `conversation`
- `store: true` or any `store` value other than `false`
- `background: true`
- `input_image.file_id` (the gateway has no Files API)

Function, custom, and namespace tools convert normally. Hosted tools such as
`web_search`, `web_search_preview`, and `tool_search` cannot run on
OpenCode-Go; their declarations are dropped in automatic tool mode, and
forcing one returns `400`.

## Gemini is a client-only format

The gateway never sends Gemini wire data upstream. It converts `contents`,
text-only `systemInstruction`, supported `inlineData` images,
`functionDeclarations`, function calls/results, JSON-schema output,
generation options, Google error envelopes, usage metadata, and SSE frames to
and from the known model's native Chat Completions or Messages protocol. Both
the `v1beta` and `v1` URL forms are accepted.

The compatibility boundary — nothing is silently pretended equivalent:

- Non-empty `safetySettings` return `400 INVALID_ARGUMENT`, because a
  different upstream protocol cannot preserve their safety semantics.
  Omitted, `null`, and `[]` are accepted. Do not treat `safetySettings` as a
  hint the upstream will enforce.
- `generationConfig.topK` and `generationConfig.thinkingConfig` are accepted
  as cross-protocol compatibility hints only; sampling, reasoning budgets,
  and thought display are not guaranteed equivalent to a native Gemini
  backend and depend on the selected OpenCode-Go model.
- Other non-null generation options that cannot be preserved — including
  `seed`, presence/frequency penalties, log-probability controls, and media
  resolution — return `400` instead of being silently discarded.
- `cachedContent`, `fileData`, Google Search, URL Context, Code Execution,
  multimodal function-response parts, function response schemas/behavior,
  `VALIDATED` function calling, candidate counts other than one, and response
  modalities other than `TEXT` return `400`. Use base64 `inlineData` for PNG,
  JPEG, GIF, or WebP images.
- `countTokens` and `embedContent` return `501 UNIMPLEMENTED`; Gemini CLI can
  fall back to local token estimation, and the gateway has no embeddings
  route.

## Claude Desktop aliases

The dedicated entry accepts only the advertised aliases
`claude-sonnet-4-6`, `claude-opus-4-6`, and `claude-haiku-4-5-20251001`.
Before entering the existing Messages conversion path, the gateway rewrites
the alias to the actual model saved from the Applications view; model
capabilities, tool support, and context limits in the response still follow
the actual model. The `sonnet`, `opus`, and `haiku` mappings are serialized
inside `AppConfig`; omitted roles inherit the first configured role, while
the dashboard returns the resolved three-role mapping.

---

[User guide index](../USER.md) · [简体中文](protocol-conversion.zh-CN.md) · [Docs index](../README.md)
