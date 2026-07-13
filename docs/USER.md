# User Guide

This guide is for people running OCG Manager as a desktop app or a headless gateway.

## What It Does

OCG Manager keeps OpenCode-Go account keys in a local SQLite database and exposes a loopback gateway at `http://127.0.0.1:9042/v1`. The gateway supports `POST /v1/chat/completions`, `POST /v1/responses`, `POST /v1/messages`, and `GET /v1/models`.

The dashboard lets you manage accounts, view cost estimates, inspect logs, and edit gateway settings.

## Install And First Run

- **Windows 10/11 x64:** run the NSIS setup. It installs for the current user without administrator rights. Windows SmartScreen may warn because the first release line is unsigned.
- **macOS 11+ Intel/Apple Silicon:** open the Universal DMG and drag OCG Manager to Applications. The app is ad-hoc signed, so use **Privacy & Security → Open Anyway** if macOS blocks the first launch.
- **Linux x64:** install the `.deb`, or make the AppImage executable with `chmod +x` and run it. Verify the download against `SHA256SUMS` first.

Then launch **OCG Manager**, open the dashboard from its tray icon, add an account, copy the Key from the dashboard, and point your OpenAI-compatible client at `http://127.0.0.1:9042/v1`.

Windows uninstallation asks whether to delete `%USERPROFILE%\.ocg-mgr`; silent upgrades and uninstalls preserve it.

## Gateway Behavior

The gateway consumes your client `Authorization` header for local gateway auth. It then forwards upstream with the selected OpenCode-Go account key.

The optional **Downstream Access Root** setting controls only the URLs shown and copied by the dashboard and application tutorials. Leave it empty to use the current dashboard origin, or set the externally reachable root such as `https://ocg.example.com` when clients use a reverse proxy or another host. A trailing `/v1` is accepted and removed automatically. This setting does not change the gateway bind address, configure DNS, or create a reverse proxy; those must already route to the running gateway. Plain HTTP is allowed for LAN deployments, but it exposes the gateway Key and request contents to the network.

Chat Completions, Responses, and Anthropic Messages clients can use the same gateway. OCG Manager routes each request to the model's native OpenCode-Go protocol and converts request, non-streaming response, and SSE events when the client protocol differs. This includes text, system instructions, images, tool calls/results, reasoning content, completion status, errors, and usage fields. Unknown models keep the request's native Chat Completions or Messages protocol; an unknown model requested through Responses is rejected because selecting an upstream protocol by trial could duplicate a billed request.

Accounts are tried in list order. Disabled accounts, cooled-down accounts, and accounts already failed during the current request are skipped. A 429 response with a reset phrase writes `cooldown_until`; 401/403 fail over without writing cooldown; 5xx and network errors are retried once for non-streaming requests before trying the next account.

### True And False Circuit Breakers

The 5-hour, weekly, and monthly usage bars are local estimates. Reaching a locally calculated limit is a false circuit breaker: local accounting and upstream billing or reset boundaries may not match, so the gateway keeps sending requests with that account and does not write a cooldown. A full local bar is therefore a warning, not proof that the upstream account is blocked.

A true circuit breaker starts only when the upstream returns HTTP 429. The gateway stores the upstream error, parses its reset phrase, writes `cooldown_until`, and tries the next available account. Known 5-hour, weekly, and monthly limit messages use the reset duration reported by the upstream; an unrecognized 429 falls back to a five-minute cooldown. If every enabled account is cooling down, the gateway returns 429 with the soonest reset time.

During a true circuit breaker, the dashboard forces the matching 5-hour, weekly, or monthly bar to 100% and marks it as an error even when the local estimate is lower. The account becomes eligible automatically after `cooldown_until`, or immediately after its cooldown is reset manually.

## CLI

Download the archive for your platform and extract it as a directory. It contains the executable, `dist/`, and `LICENSE`; keep `dist/` beside the executable so `serve` can provide the dashboard.

```bash
./ocg-manager-cli key add main sk-...
./ocg-manager-cli key list
./ocg-manager-cli serve --port 9042
```

On Windows, use `ocg-manager-cli.exe`. Linux users may need `chmod +x ocg-manager-cli` after extracting the archive.

## Docker

Build and start the headless gateway with its dashboard:

```bash
cp .env.example .env
# Edit .env and choose the initial administrator credentials.
docker compose up -d --build
docker compose logs ocg-manager
```

`OCG_ADMIN_USERNAME` and `OCG_ADMIN_PASSWORD` create the administrator only when the database has no administrator yet. If both are omitted, the first visitor creates the administrator in the dashboard. Setting only one variable stops startup with an error. Later environment changes do not reset an existing administrator.

Open the dashboard URL printed in the logs and sign in. Data and the generated encryption key persist in the `ocg-data` volume. The container publishes the gateway only at `127.0.0.1:9042` on the host. Direct requests to a gateway bound to a loopback address skip dashboard login; reverse-proxied requests still require it.

For HTTPS, point an existing reverse proxy at that loopback port. For example, with Caddy:

```caddyfile
ocg.example.com {
    reverse_proxy 127.0.0.1:9042
}
```

After signing in, set a non-empty Key before sending API traffic. Stop the service with `docker compose down`; add `-v` only when you intentionally want to delete all stored accounts, credentials, and keys.

## Data And Security

GUI data lives under `%USERPROFILE%\.ocg-mgr` on Windows and `~/.ocg-mgr` on macOS/Linux. CLI data defaults to `~/.ocg-mgr-cli` on every platform.

Keys are obfuscated before storage, not strongly encrypted. macOS/Linux GUI data and CLI data include `.encryption-key`; back it up with the database because losing it makes stored credentials unreadable. Treat anyone with the data directory and binary as able to recover stored keys.

Each node manages its own accounts through its own dashboard. OCG Manager does not synchronize account credentials between nodes.

## Limits

- `/embeddings` is not implemented.
- Gemini protocol conversion is not implemented.
- Responses is stateless: requests must set `store: false`. `previous_response_id`, `conversation`, `store: true`, and `background: true` return 400 instead of being silently ignored.
- Responses image URLs and data URLs are supported; `input_image.file_id` returns 400 because the gateway has no Files API.
- Structured output and custom-tool grammar formats return 400 when cross-protocol conversion cannot preserve their constraints.
- Responses hosted tools such as `web_search`, `web_search_preview`, and `tool_search` cannot run on OpenCode-Go. Their declarations are ignored in automatic tool mode; explicitly forcing one returns a 400 error. Function, custom, and namespace tools are converted normally.
- Streaming cost is exact only when upstream emits usage chunks; otherwise logs end as `success_no_usage`.
- The current HTTP dashboard does not expose the older isolated WebView browser command.
- The installed Windows desktop dashboard can start OCG Manager in the tray when the user logs in. Auto-start is not implemented for development builds, macOS, Linux, CLI, or Docker deployments.
- Windows/Linux ARM64 and 32-bit x86 builds are not published. RPM, Snap, app-store packages, automatic updates, Windows signing, and Apple notarization are not implemented.
