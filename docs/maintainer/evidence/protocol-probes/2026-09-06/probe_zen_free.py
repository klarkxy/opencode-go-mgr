#!/usr/bin/env python3
"""One-shot Zen Free protocol matrix. Writes JSONL next to this script."""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

MODELS = [
    "deepseek-v4-flash-free",
    "muse-spark-1.3-contributor-free",
    "muse-spark-1.2-contributor-free",
    "mimo-v2.5-free",
    "ling-3.0-flash-fin-free",
    "nemotron-3-ultra-free",
    "nemotron-3.5-lightning-free",
]

PROTOCOLS = {
    "chat_completions": "/v1/chat/completions",
    "responses": "/v1/responses",
    "messages": "/v1/messages",
}

BASE = "https://opencode.ai/zen"
OUT = Path(__file__).with_name("all-attempts.jsonl")


def body(model: str, protocol: str, stream: bool) -> dict:
    if protocol == "chat_completions":
        return {
            "model": model,
            "messages": [{"role": "user", "content": "Reply: PING"}],
            "max_tokens": 16,
            "stream": stream,
        }
    if protocol == "responses":
        return {
            "model": model,
            "input": "Reply: PING",
            "max_output_tokens": 16,
            "store": False,
            "stream": stream,
        }
    return {
        "model": model,
        "messages": [{"role": "user", "content": "Reply: PING"}],
        "max_tokens": 16,
        "stream": stream,
    }


def shaped(protocol: str, stream: bool, text: str) -> bool:
    if stream:
        token = {
            "chat_completions": "chat.completion.chunk",
            "responses": "response.",
            "messages": "message_",
        }[protocol]
        return "data:" in text and token in text
    try:
        value = json.loads(text) if text else {}
    except json.JSONDecodeError:
        return False
    if protocol == "chat_completions":
        return isinstance(value, dict) and bool(
            (((value.get("choices") or [None])[0]) if value.get("choices") else None)
        )
    if protocol == "responses":
        return isinstance(value, dict) and (
            "output" in value or "output_text" in value
        )
    return isinstance(value, dict) and isinstance(value.get("content"), list)


def probe(model: str, protocol: str, stream: bool) -> dict:
    url = BASE + PROTOCOLS[protocol]
    payload = json.dumps(body(model, protocol, stream), separators=(",", ":"))
    started = time.perf_counter()
    cmd = [
        "curl",
        "-sS",
        "-D",
        "-",
        "--max-time",
        "90",
        "-X",
        "POST",
        url,
        "-H",
        "Content-Type: application/json",
        "-H",
        "User-Agent: opencode",
        "-H",
        "x-opencode-client: cli",
        "-H",
        "x-opencode-session: ocg-probe-20260906",
        "-H",
        f"x-opencode-request: {model}-{protocol}-{int(stream)}",
        "-H",
        "x-opencode-project: ocg-probe-20260906",
        "-H",
        "Authorization: Bearer public",
        "--data-binary",
        payload,
    ]
    completed = subprocess.run(cmd, capture_output=True, text=True)
    duration_ms = int((time.perf_counter() - started) * 1000)
    raw = completed.stdout
    header_text, _, body_text = raw.partition("\r\n\r\n")
    if not body_text:
        header_text, _, body_text = raw.partition("\n\n")
    status = 0
    for line in header_text.splitlines():
        if line.upper().startswith("HTTP/"):
            parts = line.split()
            if len(parts) >= 2 and parts[1].isdigit():
                status = int(parts[1])
    ok_shape = shaped(protocol, stream, body_text)
    evidence = (
        "protocol-shaped SSE"
        if stream and ok_shape
        else "protocol-shaped JSON"
        if ok_shape
        else body_text.replace("\n", " ").replace("\r", " ")[:300]
    )
    if completed.returncode != 0 and not evidence:
        evidence = (completed.stderr or completed.stdout or "curl failed").replace(
            "\n", " "
        )[:300]
    return {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "providerId": "opencode-zen-free",
        "modelId": model,
        "protocol": protocol,
        "stream": stream,
        "status": status,
        "ok": status < 300 and ok_shape,
        "durationMs": duration_ms,
        "evidence": evidence,
        "curlExit": completed.returncode,
    }


def main() -> int:
    rows = []
    for model in MODELS:
        for protocol in PROTOCOLS:
            for stream in (False, True):
                row = probe(model, protocol, stream)
                rows.append(row)
                print(
                    f"{model} {protocol} stream={int(stream)} status={row['status']} ok={row['ok']} {row['durationMs']}ms",
                    flush=True,
                )
                time.sleep(0.4)
    OUT.write_text("\n".join(json.dumps(row, ensure_ascii=False) for row in rows) + "\n", encoding="utf-8")
    print(f"wrote {len(rows)} rows to {OUT}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
