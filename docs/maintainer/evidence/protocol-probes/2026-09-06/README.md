# Protocol probe snapshot — 2026-09-06 (Zen Free)

This directory preserves the sanitized evidence behind the 2026-09-06 Zen Free protocol baseline. It is maintenance evidence, not runtime state and not an instruction to probe models during normal routing.

## Scope and method

- Official catalog: `GET https://opencode.ai/zen/v1/models` (2026-09-06). Seven IDs ending in `-free`.
- Every current Free model was sent a minimal Chat Completions, Responses, and Messages request in both non-streaming and streaming form (42 requests).
- Zen Free was anonymous (`Authorization: Bearer public`) with official-client identity headers (`User-Agent: opencode`, `x-opencode-client: cli`, stable session/project ids).
- Concurrency was 1, with a short delay between requests. `muse-spark-1.3-contributor-free` Responses was retried after an initial 429.
- Output limit was 16 tokens. No SQLite was written.

## Files

- `all-attempts.jsonl`: 44 requests (the 42-row sweep plus the two Responses retries).
- `retry-1.3-responses.jsonl`: the two successful Responses retries, also appended to `all-attempts.jsonl`.
- `classified-pairs.json`: one classification per current Free model.
- `probe_zen_free.py` / `retry_1_3.py` / `summarize.py`: the one-shot runners used to produce this snapshot.

## Classification rules

Same rules as `2026-08-27`:

- `live_supported`: both streaming and non-streaming requests returned a usable protocol-shaped 2xx response, or a retry did after a transient 429.
- `model_unavailable`: the official Chat path reported the model or endpoint unavailable (400/503). This is not proof that another protocol shape is supported.
- Generic 500 `Internal server error` on a non-preferred path, when another path is live, is treated as an unsupported protocol shape for the static table.

## Static table outcome

Checked into `MODEL_PROTOCOLS` (`OFFICIAL_PROTOCOL_BASELINE_DATE = 2026-09-06`):

| Model | Preferred | Supported |
| --- | --- | --- |
| `muse-spark-1.2-contributor-free` | Responses | Responses |
| `muse-spark-1.3-contributor-free` | Responses | Responses |
| `mimo-v2.5-free` | Chat | Chat |
| `nemotron-3-ultra-free` | Chat | Chat |
| `nemotron-3.5-lightning-free` | Chat | Chat |
| `ling-3.0-flash-fin-free` | Chat | Chat |
| `deepseek-v4-flash-free` | Chat | none this run |

Removed because they are no longer in the official Free catalog: `hy3-free`, `ling-3.0-flash-free`, `laguna-s-2.1-free`, `longcat-2.0-free`, `north-mini-code-free`.
