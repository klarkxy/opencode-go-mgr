# 协议探测快照 — 2026-09-06（Zen Free）

本目录保存 2026-09-06 Zen Free 协议基线的脱敏证据。这是维护证据，不是运行时状态，也不是正常路由过程中探测模型的指令。

## 范围与方法

- 官方目录：`GET https://opencode.ai/zen/v1/models`（2026-09-06）。7 个以 `-free` 结尾的 ID。
- 每个当前 Free 模型都发送了最小的 Chat Completions、Responses、Messages 请求，非流式与流式各一次（42 次）。
- Zen Free 走匿名通道（`Authorization: Bearer public`），并带官方客户端身份头（`User-Agent: opencode`、`x-opencode-client: cli`，以及稳定的 session/project）。
- 并发为 1，请求之间有短间隔。`muse-spark-1.3-contributor-free` 的 Responses 在首次 429 后重试。
- 输出上限 16 token。未写入 SQLite。

## 文件

- `all-attempts.jsonl`：44 次请求（42 次扫描 + 两次 Responses 重试）。
- `retry-1.3-responses.jsonl`：两次成功的 Responses 重试，也已追加进 `all-attempts.jsonl`。
- `classified-pairs.json`：每个当前 Free 模型一条分类。
- `probe_zen_free.py` / `retry_1_3.py` / `summarize.py`：生成本快照的一次性脚本。

## 分类规则

与 `2026-08-27` 相同：

- `live_supported`：流式与非流式都返回可用的协议形态 2xx，或在短暂 429 后重试成功。
- `model_unavailable`：官方 Chat 路径报告模型或端点不可用（400/503）。这不能证明其他协议形态可用。
- 当另一条路径已确认可用时，非推荐路径上的泛型 500 `Internal server error` 视为静态表中不支持的协议形态。

## 静态表结果

已写入 `MODEL_PROTOCOLS`（`OFFICIAL_PROTOCOL_BASELINE_DATE = 2026-09-06`）：

| 模型 | 推荐 | 已验证支持 |
| --- | --- | --- |
| `muse-spark-1.2-contributor-free` | Responses | Responses |
| `muse-spark-1.3-contributor-free` | Responses | Responses |
| `mimo-v2.5-free` | Chat | Chat |
| `nemotron-3-ultra-free` | Chat | Chat |
| `nemotron-3.5-lightning-free` | Chat | Chat |
| `ling-3.0-flash-fin-free` | Chat | Chat |
| `deepseek-v4-flash-free` | Chat | 本次无 |

因官方 Free 目录已不包含而移出：`hy3-free`、`ling-3.0-flash-free`、`laguna-s-2.1-free`、`longcat-2.0-free`、`north-mini-code-free`。
