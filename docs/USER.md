# User Guide

This guide is for people running OCG Manager as a desktop app, a headless
gateway, or a Docker service. It explains how to install, configure, and
troubleshoot the gateway, the dashboard, the CLI, and Docker, and how the
true / false circuit breaker and protocol conversion actually work.

## Table Of Contents

- [What It Does](#what-it-does)
- [Install And First Run](#install-and-first-run)
- [The Dashboard](#the-dashboard)
  - [Connection Center](#connection-center)
  - [Application Guides](#application-guides)
  - [Accounts](#accounts)
  - [Logs](#logs)
  - [Settings](#settings)
- [Gateway Behavior](#gateway-behavior)
  - [Endpoints](#endpoints)
  - [Authentication](#authentication)
  - [Protocol Conversion](#protocol-conversion)
  - [Account Selection And Failover](#account-selection-and-failover)
  - [Cost Accounting](#cost-accounting)
  - [True And False Circuit Breakers](#true-and-false-circuit-breakers)
- [CLI](#cli)
- [Docker](#docker)
- [Data And Security](#data-and-security)
- [Limits](#limits)
- [Troubleshooting](#troubleshooting)

## What It Does

OCG Manager keeps OpenCode‑Go account keys in a local SQLite database and
exposes a loopback gateway at `http://127.0.0.1:9042/v1`. The same gateway
also serves the Vue 3 dashboard at `/dashboard/` and the dashboard's JSON
API at `/dashboard/api`. Every node is independent: there is no remote
sync, no Admin API, and no telemetry.

The four jobs of the gateway are:

1. Authenticate the client with the **Gateway Key** issued by the dashboard.
2. Pick a usable OpenCode‑Go account for the request.
3. Convert the request to that model's native OpenCode‑Go protocol and the
   response back to the client protocol.
4. Log the request, write usage and any cooldown to SQLite, and surface
   everything back in the dashboard.

## Install And First Run

### Windows 10/11 x64

1. Run the NSIS setup `ocg-manager_<version>_windows-x64-setup.exe`. It
   installs for the current user without administrator rights.
2. Launch **OCG Manager** from the Start menu. The main window is hidden
   by default — open the dashboard from the tray icon.
3. The first Windows release line is unsigned; SmartScreen may warn. Click
   **More info → Run anyway** to continue.
4. Add an OpenCode‑Go account in the **Accounts** view, copy the Gateway
   Key, and point your client at `http://127.0.0.1:9042/v1`.
5. The Windows uninstaller asks whether to delete
   `%USERPROFILE%\.ocg-mgr`; silent upgrades and uninstalls preserve it.

### macOS 11+ Intel / Apple Silicon

1. Open the Universal DMG and drag **OCG Manager** to **Applications**.
2. The app is ad‑hoc signed, so on the first launch macOS may block it. Open
   **Privacy & Security** and click **Open Anyway** to allow it.
3. Open the dashboard from the tray icon, add an account, copy the Gateway
   Key, and configure your client.

### Linux x64

1. Install the `.deb` with your package manager, or mark the AppImage
   executable with `chmod +x ocg-manager_<version>_linux-x64.AppImage`.
2. Verify the download against `SHA256SUMS` first.
3. Launch the executable. The dashboard opens from the tray icon.
4. Data lives in `~/.ocg-mgr/`.

## The Dashboard

The dashboard is a single‑page Vue 3 application served by the gateway. The
left rail exposes four views: **Dashboard**, **Accounts**, **Applications**,
and **Logs**, plus the **Settings** menu. The top right of the header
contains the theme switcher, the language switcher, and the sign‑out
button. The footer carries the about line and version.

The dashboard speaks ten languages out of the box: 简体中文, 繁體中文,
English, 日本語, 한국어, Español, Français, Deutsch, Português (Brasil),
and Русский. The default is 简体中文. The language choice is persisted in
`localStorage` under `ocg-manager.locale`; if it cannot be persisted (for
example in a private window), the in‑memory locale still works for the
current session.

### Connection Center

The first panel above the fold is the **Connection Center** — the only
panel that always appears at the top. It contains:

- The **Gateway Key** (also called the *Key*) with a regenerate action and
  a one‑click copy. Regenerating invalidates the previous key immediately.
- The **API Base URL** (e.g. `http://127.0.0.1:9042/v1`) with a one‑click
  copy.
- The full **Chat Completions**, **Responses**, and **Messages** endpoints.
- The **Upstream URL** the gateway forwards to, with a copy action.
- An **HTTP warning** that appears whenever the resolved root URL is a
  non‑loopback `http://` URL, so the Gateway Key and request contents are
  not transmitted in clear text.

The "Downstream Access Root" setting in **Settings** controls only the
URLs the dashboard shows and the application tutorials emit. Leave it
empty to use the current dashboard origin, or set the externally reachable
root such as `https://ocg.example.com` when clients reach the gateway
through a reverse proxy or a different host. A trailing `/v1` is accepted
and removed automatically. The setting does **not** change the gateway
bind address, configure DNS, or create a reverse proxy — those must
already route to the running gateway. Plain HTTP is allowed for LAN
deployments, but it exposes the Gateway Key and request contents to the
network.

### Application Guides

The **Applications** view ships with per‑client configuration snippets for
ten downstream tools: Claude Code, Codex, OpenCode, Cherry Studio, VS Code
Copilot Chat, Trae, Cline, Roo Code, Continue, and Chatbox. Each guide
shows the protocol the tool speaks, the official documentation URL, a short
summary, step‑by‑step instructions, and one or more code blocks with a
**Copy** button. Snippets are rendered twice: a *display* version that
masks the key and a *copy* version that includes the real key, so the
on‑screen guide stays shareable.

Three clients need the root URL without `/v1` (Claude Code, Cherry Studio,
Chatbox). Five clients need the API Base URL with `/v1` (OpenCode, Trae,
Cline, Roo Code, Continue). VS Code Copilot Chat needs the full
`/v1/chat/completions` URL. Codex needs `/v1` plus `wire_api = "responses"`.

### Accounts

The **Accounts** view lets you create, edit, enable, disable, and remove
OpenCode‑Go accounts. Each account card shows the account name, the
cooldown state, and the 5‑hour / weekly / monthly usage bars driven by
local accounting. You can paste an OpenCode‑Go `username` and `password`
alongside the key; the password is stored encrypted and used by the
gateway to refresh upstream sessions when needed.

You can also reset a cooldown manually from this view. The bar snaps back
to its local estimate as soon as the cooldown is cleared.

### Logs

The **Logs** view shows the rolling buffer of requests the gateway has
forwarded, including the timestamp, the chosen account, the model, the
status code, the upstream error if any, and the streamed usage when the
upstream emitted a usage chunk. Rows with `success_no_usage` mean the
stream finished without a usage chunk; cost is exact only when the
upstream emits usage data.

### Settings

The **Settings** view exposes the persistent gateway configuration:

- **Gateway Port** — the port the gateway binds (default `9042`).
- **Gateway Key** — the same value shown in the Connection Center.
- **Upstream URL** — the OpenCode‑Go base URL.
- **Downstream Access Root** — see [Connection Center](#connection-center).
- **Auto‑start on login** — only the installed Windows desktop build
  exposes this switch. Development builds, the CLI, Docker, macOS, and
  Linux dashboards hide it.
- **Connect / non‑stream / stream‑idle timeouts** — apply to upstream
  HTTP requests.

All settings are written to SQLite and reloaded on the next start.

## Gateway Behavior

### Endpoints

The gateway is served at `http://<bind>:<port>` and exposes:

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/messages` | Anthropic Messages |
| `GET`  | `/v1/models` | OpenAI model list |
| `GET`  | `/dashboard/` | Vue 3 dashboard (HTML) |
| `*`    | `/dashboard/api/...` | Dashboard JSON API |
| `GET`  | `/healthz` | Liveness probe used by the Docker healthcheck |

The default bind is `127.0.0.1:9042`. The CLI can override the host with
`serve --host 0.0.0.0` and the port with `serve --port <port>`. The
desktop app also binds loopback and uses a Tauri single‑instance lock to
prevent two tray apps from competing for the port.

### Authentication

The gateway uses two different authentication mechanisms depending on
where the request comes from:

- **Loopback binds (the default).** Requests that come straight to the
  loopback address skip dashboard login. The client only needs the
  **Gateway Key** in `Authorization: Bearer <key>` to reach the upstream
  endpoints. This is what the desktop app and the default CLI use.
- **Non‑loopback binds.** A single administrator account, stored as an
  Argon2 password hash in SQLite, governs the dashboard. Sign‑in returns
  an HttpOnly session cookie. Standard reverse‑proxy forwarding headers
  on a non‑loopback bind still require the cookie. In Docker, the first
  administrator can be bootstrapped with `OCG_ADMIN_USERNAME` and
  `OCG_ADMIN_PASSWORD`; otherwise the first registration wins.

The Gateway Key in the `Authorization` header is the only credential the
client needs. It is local — it has nothing to do with the OpenCode‑Go
account key, which the gateway retrieves from SQLite and sends upstream as
its own `Authorization: Bearer <opencode-go-key>` header.

### Protocol Conversion

Each known model is mapped to its native OpenCode‑Go protocol. When a
request arrives in a different protocol, the gateway converts the
**request body** to the upstream protocol and the **response body** (or
SSE stream) back to the client protocol. Conversion covers text, system
instructions, images, tool calls and tool results, reasoning content,
completion status, errors, and usage fields.

The Responses endpoint is **stateless** in this gateway. The following
fields return `400` instead of being silently ignored:

- `previous_response_id`
- `conversation`
- `store: true` or any `store` value other than `false`
- `background: true`
- `input_image.file_id` (the gateway has no Files API)

Function, custom, and namespace tools convert normally. Hosted tools
such as `web_search`, `web_search_preview`, and `tool_search` cannot run
on OpenCode‑Go; their declarations are dropped in automatic tool mode,
and forcing one returns `400`.

### Account Selection And Failover

Accounts are tried in **list order**. The selector skips:

- Disabled accounts.
- Accounts that are cooling down.
- Accounts that have already failed during the current request (e.g. with
  a `429`).

A `429` with a recognized `Resets in …` phrase writes `cooldown_until` and
the gateway tries the next account. `401` and `403` responses fail over
without writing a cooldown — they are an authentication problem, not a
quota problem. `5xx` and network errors are retried once for non‑streaming
requests before moving to the next account. When every enabled account is
cooling down, the gateway returns `429` with the soonest reset time.

### Cost Accounting

The 5‑hour, weekly, and monthly bars are local estimates. They are driven
by the requests the gateway actually forwards, not by the upstream's
authoritative billing. Streaming cost is exact only when the upstream
emits a usage chunk; otherwise the log row ends with `success_no_usage`.

The dashboard always pairs a bar with the account's cooldown state. While
a true circuit breaker is active, the matching bar is forced to 100% and
marked as an error — see the next section.

### True And False Circuit Breakers

- **False circuit breaker (local estimate).** The local estimate is a
  *signal*, not a stop sign. When the local estimate reaches the limit,
  the gateway **keeps sending** requests with that account. Local
  accounting and upstream billing/reset boundaries may not match, so a
  full local bar is a warning, not proof that the upstream account is
  blocked.
- **True circuit breaker (upstream 429).** The gateway stores the upstream
  error, parses the `Resets in …` phrase from the response, writes
  `cooldown_until`, and tries the next available account. The known
  5‑hour, weekly, and monthly limit messages use the reset duration
  reported by the upstream; an unrecognized 429 falls back to a
  five‑minute cooldown.
- **No account available.** If every enabled account is cooling down, the
  gateway returns `429` with the soonest reset time.
- **Dashboard display.** While a true circuit breaker is active, the
  matching 5‑hour, weekly, or monthly bar is forced to 100% and marked
  as an error, even when the local estimate is lower. The account becomes
  eligible automatically after `cooldown_until`, or immediately after
  you reset its cooldown in the dashboard.

## CLI

Download the archive for your platform and extract it as a directory. It
contains the executable, `dist/`, and `LICENSE`. Keep `dist/` beside the
executable so `serve` can serve the dashboard. On Windows the executable
is `ocg-manager-cli.exe`; on Linux you may need `chmod +x
ocg-manager-cli` after extraction.

The CLI data directory defaults to `~/.ocg-mgr-cli` on every platform; you
can override it with `--data-dir <path>`. The encryption key defaults to
`<data-dir>/.encryption-key`; you can override it with `--encryption-key
<key>` or the `OCG_MANAGER_ENCRYPTION_KEY` environment variable.

```text
ocg-manager-cli
├── serve         Start the gateway server
│   --host        Address to listen on (default 127.0.0.1)
│   -p, --port    Gateway port (overrides config)
│   --dashboard-dir  Directory containing the built web dashboard
├── key list      List accounts and their enabled state
├── key add <name> <key>
│   --username    OpenCode-Go login account
│   --password    OpenCode-Go login password
├── key remove <id>      Remove an account
├── key enable <id>      Enable an account
├── key disable <id>     Disable an account
├── key ping [id]
│   --model       Model to send (default deepseek-v4-flash)
│   --message     User message (default "ping")
│   --max-tokens  max_tokens for the ping (default 3)
└── status        Show data dir, gateway port/key, upstream, account totals
```

The fastest way to bootstrap a headless gateway:

```bash
./ocg-manager-cli key add main sk-...
./ocg-manager-cli key list
./ocg-manager-cli serve --port 9042
```

`key ping` decrypts the key, sends a tiny chat completion, and prints the
real upstream status code and a short body excerpt — use it to surface
real `401`/`403`/`429`/`200` from each key without going through the
dashboard.

## Docker

Build and start the headless gateway with its dashboard:

```bash
cp .env.example .env
# Edit .env and choose the initial administrator credentials.
docker compose up -d --build
docker compose logs ocg-manager
```

`OCG_ADMIN_USERNAME` and `OCG_ADMIN_PASSWORD` create the administrator
**only when the database has no administrator yet**. Both must be set
together; setting only one stops startup with an error. Once an
administrator exists, later environment changes do not reset it. When
both are omitted, the first visitor creates the administrator in the
dashboard.

Open the dashboard URL printed in the logs and sign in. Data and the
generated encryption key persist in the `ocg-data` volume. The container
publishes the gateway only at `127.0.0.1:9042` on the host. Direct
requests to a gateway bound to a loopback address skip dashboard login;
reverse‑proxied requests still require it. The container's `HEALTHCHECK`
hits `127.0.0.1:9042` over TCP every 30 seconds.

For HTTPS, point an existing reverse proxy at that loopback port. For
example, with Caddy:

```caddyfile
ocg.example.com {
    reverse_proxy 127.0.0.1:9042
}
```

After signing in, set a non‑empty Gateway Key before sending API traffic.
Stop the service with `docker compose down`; add `-v` only when you
intentionally want to delete all stored accounts, credentials, and keys.

## Data And Security

- **GUI data location.** Windows: `%USERPROFILE%\.ocg-mgr`. macOS / Linux:
  `~/.ocg-mgr`. CLI data defaults to `~/.ocg-mgr-cli` on every platform
  and can be overridden with `--data-dir <path>`.
- **Key storage.** Keys are obfuscated before storage, not strongly
  encrypted. The macOS / Linux GUI and the CLI also place a
  `.encryption-key` file inside the data directory; **back it up with
  the database** because losing it makes stored credentials unreadable.
  Treat anyone with the data directory and the binary as able to recover
  stored keys.
- **No cross‑node sync.** Each node manages its own accounts through its
  own dashboard. OCG Manager does not synchronize account credentials
  between nodes.
- **Plain HTTP warning.** A non‑loopback `http://` root URL exposes the
  Gateway Key and request contents to the network. Use HTTPS or a
  trusted LAN only.
- **Administrator password.** The single administrator password is
  stored as an Argon2 hash in SQLite. There is no self‑service password
  recovery — protect the data directory.

## Limits

- `/embeddings` is not implemented.
- Gemini protocol conversion is not implemented.
- Responses is stateless: requests must set `store: false`.
  `previous_response_id`, `conversation`, `store: true`, and
  `background: true` return `400` instead of being silently ignored.
- Responses image URLs and data URLs are supported; `input_image.file_id`
  returns `400` because the gateway has no Files API.
- Structured output and custom‑tool grammar formats return `400` when
  cross‑protocol conversion cannot preserve their constraints.
- Responses hosted tools such as `web_search`, `web_search_preview`, and
  `tool_search` cannot run on OpenCode‑Go. Their declarations are dropped
  in automatic tool mode; explicitly forcing one returns a `400` error.
  Function, custom, and namespace tools are converted normally.
- Streaming cost is exact only when upstream emits usage chunks; otherwise
  logs end as `success_no_usage`.
- The current HTTP dashboard does not expose the older isolated WebView
  browser command.
- The installed Windows desktop dashboard can start OCG Manager in the
  tray when the user logs in. Auto‑start is not implemented for
  development builds, macOS, Linux, CLI, or Docker deployments.
- Windows / Linux ARM64 and 32‑bit x86 builds are not published. RPM,
  Snap, app‑store packages, automatic updates, Windows signing, and Apple
  notarization are not implemented.

## Troubleshooting

- **The dashboard never opens from the tray.** Another process is bound
  to `127.0.0.1:9042`, or a previous tray app still holds the
  single‑instance lock. Quit the release tray app (or run
  `scripts/free-dev-port.mjs` to clean stale Vite processes) and retry.
- **`401 Unauthorized` from the upstream.** The OpenCode‑Go account key
  is invalid or revoked. Open the **Accounts** view, replace the key,
  and try again. `key ping <id>` is the fastest way to confirm.
- **Local bar at 100% but requests still succeed.** That is a *false*
  circuit breaker — local accounting only. Continue using the account;
  the gateway will keep forwarding.
- **Local bar at 100% and the gateway returns `429`.** That is a *true*
  circuit breaker. Wait for `cooldown_until`, or reset the cooldown
  manually in the **Accounts** view.
- **Gateway returns `429` with "all accounts cooling down".** Every
  enabled account is in cooldown. Either wait for the soonest reset, or
  add / enable another account.
- **Docker first‑run registration does not pick up my `OCG_ADMIN_PASSWORD`.**
  The variables are only honored when the database has no administrator
  yet. Reset the `ocg-data` volume to bootstrap a fresh administrator.
- **SmartScreen / Gatekeeper warns about the installer or the DMG.** The
  first Windows release line is unsigned and the macOS app is ad‑hoc
  signed. Use **Open Anyway** for the first launch; the warning is not a
  sign of tampering.

---

[中文用户指南](USER.zh-CN.md) · [Maintainer guide](MAINTAINER.md) ·
[维护者指南](MAINTAINER.zh-CN.md) · [Back to README](../README.md)
