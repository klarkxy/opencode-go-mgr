# User Guide

This guide is for people running OCG Manager as a desktop app or a headless gateway.

## What It Does

OCG Manager keeps OpenCode-Go account keys in a local SQLite database and exposes a loopback gateway at `http://127.0.0.1:9042/v1`. The gateway supports `POST /v1/chat/completions`, `POST /v1/messages`, and `GET /v1/models`.

The dashboard lets you manage accounts, view cost estimates, inspect logs, edit gateway settings, and manually push local keys to a remote admin node.

## First Run

1. Install the Windows NSIS package.
2. Launch **OCG Manager**.
3. Open the dashboard from the tray icon.
4. Add an account in **Accounts**.
5. Copy the Gateway Key from the dashboard or settings page.
6. Point your OpenAI-compatible client at `http://127.0.0.1:9042/v1`.

## Gateway Behavior

The gateway consumes your client `Authorization` header for local gateway auth. It then forwards upstream with the selected OpenCode-Go account key.

Accounts are tried in list order. Disabled accounts, cooled-down accounts, and accounts already failed during the current request are skipped. A 429 response with a reset phrase writes `cooldown_until`; 401/403 fail over without writing cooldown; 5xx and network errors are retried once for non-streaming requests before trying the next account.

## CLI

```bash
pnpm run build:cli
target/release/ocg-manager-cli.exe key add main sk-...
target/release/ocg-manager-cli.exe key list
target/release/ocg-manager-cli.exe serve --port 9042
```

For a remote sync target:

```bash
target/release/ocg-manager-cli.exe serve --admin-port 9091
```

The admin API binds `127.0.0.1` and requires a Bearer token. Add your own reverse proxy, TLS, and network auth before exposing it to another machine.

## Data And Security

GUI data lives under `%USERPROFILE%\.ocg-mgr`. CLI data defaults to `~/.ocg-mgr-cli`.

Keys are obfuscated before storage, not strongly encrypted. Treat anyone with the data directory and binary as able to recover stored keys.

Remote sync sends account keys, and optionally login fields, to the configured remote admin API. Leave the remote URL and token empty for local-only use.

## Limits

- `/embeddings` is not implemented.
- Gemini protocol conversion is not implemented.
- Streaming cost is exact only when upstream emits usage chunks; otherwise logs end as `success_no_usage`.
- The current HTTP dashboard does not expose the older isolated WebView browser command.
- The current HTTP dashboard does not expose the Windows startup toggle.
