[简体中文](USER.zh-CN.md)

# User Guide

This guide is for people running OCG Manager as a desktop app, a headless
gateway, or a Docker service. It explains how to install, configure, and
troubleshoot the gateway, the dashboard, the CLI, and Docker, and how the
true / false circuit breaker and protocol conversion actually work.

## Table Of Contents

- [What It Does](#what-it-does)
- [Install And First Run](#install-and-first-run)
- [Download, Upgrade, Backup, Restore, And Uninstall](#download-upgrade-backup-restore-and-uninstall)
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
2. Launch **OCG Manager** from the Start menu. A normal launch opens the
   dashboard in your system browser; use the tray icon to open it again.
3. Current Windows builds are unsigned; SmartScreen may warn. Click
   **More info → Run anyway** to continue.
4. Add an OpenCode‑Go account in the **Accounts** view, copy the Gateway
   Key, and point your client at `http://127.0.0.1:9042/v1`.
5. The Windows uninstaller asks whether to delete
   `%USERPROFILE%\.ocg-mgr`; silent upgrades and uninstalls preserve it.

### macOS 11+ Intel / Apple Silicon

1. Open the Universal DMG and drag **OCG Manager** to **Applications**.
2. The app is ad‑hoc signed, so on the first launch macOS may block it. Open
   **Privacy & Security** and click **Open Anyway** to allow it.
3. Launch the app. A normal launch opens the dashboard in your system
   browser; use the tray icon to reopen it. Add an account, copy the
   Gateway Key, and configure your client.

### Linux x64

1. Install the `.deb` with your package manager, or mark the AppImage
   executable with `chmod +x ocg-manager_<version>_linux-x64.AppImage`.
2. Verify the download against `SHA256SUMS` first.
3. Launch the executable. A normal launch opens the dashboard in your
   system browser; use the tray icon to reopen it.
4. Data lives in `~/.ocg-mgr/`.

The installed Windows auto-start path stays in the tray without opening a
browser.

## Download, Upgrade, Backup, Restore, And Uninstall

Download the GUI package or CLI archive for your platform from the
[latest GitHub Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest),
plus `SHA256SUMS` from the same release. Compare the matching checksum
before installing: use `Get-FileHash <file> -Algorithm SHA256` on
PowerShell, `shasum -a 256 <file>` on macOS, or `sha256sum <file>` on
Linux.

Before an upgrade or restore, stop every process using the data: choose
**Quit** from the desktop tray, stop the CLI with Ctrl+C or its service
manager, or run `docker compose stop`. Then copy the **entire** GUI data
directory, CLI data directory, or Docker `ocg-data` volume. A stopped
Docker container can be copied with
`docker compose cp ocg-manager:/data/. ../ocg-data-backup`. Keep the backup
outside the repository and check that the backup contains `data.sqlite` and,
where present, `.encryption-key`.

To restore, stop the process, move the current data aside, copy the whole
backup back to its original directory or an empty Docker volume, and then
start the same or a newer version. Docker files in `/data` must remain
writable by UID/GID `10001`. The Windows GUI obfuscation is bound to the
Windows user and machine, so its data cannot directly restore account keys
or passwords on another machine; create fresh data there and re-enter the
credentials. macOS/Linux GUI, CLI, and Docker restores must preserve
`.encryption-key` or the explicitly supplied `--encryption-key` /
`OCG_MANAGER_ENCRYPTION_KEY` value. There is no automatic downgrade
compatibility guarantee; do not open a newer database with an older build.

To restore a Docker backup into a fresh named volume, first verify the backup
and confirm that `.env` pins the intended same or newer image. The
`docker compose down -v` command below permanently deletes the current volume;
run it only after preserving that current data separately:

```bash
docker compose down -v
docker compose run --rm --no-deps --user root \
  --cap-add CHOWN --cap-add DAC_OVERRIDE --cap-add FOWNER \
  --entrypoint sh \
  --volume ../ocg-data-backup:/backup:ro \
  ocg-manager \
  -c 'cp -a /backup/. /data/ && chown -R 10001:10001 /data && \
      find /data -type d -exec chmod 700 {} + && \
      find /data -type f -exec chmod 600 {} +'
docker compose up -d --no-build
docker compose ps
```

If the original deployment used `OCG_MANAGER_ENCRYPTION_KEY`, put the same
secret back into `.env` before the restore. Keep the backup until the dashboard,
accounts, and a real Gateway request have all been verified.

Upgrade and uninstall by surface:

- **Windows GUI:** run the new installer over the current install. Remove
  it from Windows **Installed apps**; the uninstaller asks whether to
  delete `%USERPROFILE%\.ocg-mgr`.
- **macOS GUI:** replace the app in **Applications** with the new DMG copy.
  Delete the app to uninstall; remove `~/.ocg-mgr` separately only when
  you also intend to delete the data.
- **Linux GUI:** install the new `.deb` over the old package, or replace
  the AppImage. Remove the package or AppImage to uninstall; data remains
  in `~/.ocg-mgr` until you delete it.
- **CLI:** replace the extracted package as a unit so the executable,
  `dist/`, and `LICENSE` stay together. Delete that package to uninstall;
  its data remains in `~/.ocg-mgr-cli` or the custom `--data-dir`.
- **Docker:** after backing up, run `docker compose pull` followed by
  `docker compose up -d --no-build`. Pin `OCG_IMAGE` to the full release tag
  for repeatable production deployments. `docker compose down` removes
  containers but keeps `ocg-data`; `docker compose down -v` permanently
  deletes the volume and is only for an intentional reset after a verified
  backup. Selecting an older image does not roll back the database; restore
  the complete backup made by that older version when a database rollback is
  required.

## The Dashboard

The dashboard is a single‑page Vue 3 application served by the gateway. The
left rail exposes four views: **Dashboard**, **Accounts**, **Applications**,
and **Logs**, plus the **Settings** menu. The top right of the header
contains the theme switcher, the language switcher, and the sign‑out
button.

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
- The **Upstream URL** the gateway forwards to, with a copy action.
- An **HTTP warning** that appears whenever the resolved root URL is a
  non‑loopback `http://` URL, so the Gateway Key and request contents are
  not transmitted in clear text.

The "Downstream Access Root" setting in **Settings** controls only the
URLs the dashboard shows and the application tutorials emit. Its effective
value is selected in this order: a non-empty `OCG_CLIENT_ROOT_URL`
environment variable, the manually saved dashboard value, then an automatic
fallback. While the environment variable is active, the input is read-only;
changes take effect after restart and are never written to SQLite. With no
environment or saved value, production uses the current dashboard origin and
development uses `http://127.0.0.1:<Gateway port>`. The automatic value is
shown in the input but is not saved.

Set an externally reachable root such as `https://ocg.example.com` when
clients reach the gateway through a reverse proxy or a different host. A
trailing `/v1` is accepted and removed automatically. This setting does
**not** change the gateway bind address, configure DNS, or create a reverse
proxy — those must already route to the running gateway. Plain HTTP is
allowed for LAN deployments, but it exposes the Gateway Key and request
contents to the network.

### Application Guides

The **Applications** view ships with per-client configuration snippets for
13 downstream tools: Claude Code, Claude Desktop, Codex, Gemini CLI, OpenCode,
OpenClaw, Hermes, Cherry Studio, VS Code Copilot Chat, Cline, Roo Code,
Continue, and Chatbox. Each guide shows the protocol the tool speaks, the
official documentation URL, step-by-step instructions, model selectors, and
one or more editable code blocks with a **Copy** button. The displayed block
masks the Gateway Key; copying restores the real key, so screenshots remain
shareable without producing an unusable configuration.

- Claude Code, Cherry Studio, and Chatbox use the root URL without `/v1`.
- Claude Desktop uses that root plus `/claude-desktop`; its client then calls
  `/claude-desktop/v1/messages` and `/claude-desktop/v1/models`.
- Gemini CLI uses the root URL with `GOOGLE_GENAI_API_VERSION=v1beta`. Its
  remote Base URL must use HTTPS; only `localhost`, `127.0.0.1`, and `[::1]`
  may use HTTP. The Applications view disables Gemini configuration copying
  when the resolved root violates this client-side rule.
- OpenCode, OpenClaw, Hermes, Cline, Roo Code, and Continue use the API Base
  URL ending in `/v1`.
- VS Code Copilot Chat needs the full `/v1/chat/completions` URL. Codex needs
  `/v1` plus `wire_api = "responses"`.

Model choices and edited snippets are cached separately per application while
the current dashboard page remains alive. A page reload resets this in-memory
state. **Restore defaults** resets the active application's model selection and
snippet drafts. Claude Desktop is the exception with durable model mappings:
before its configuration is copied, the selected `sonnet`, `opus`, and `haiku`
targets are saved to SQLite through the protected dashboard API. Its restore
action returns to the mapping loaded or last saved in the current page.

### Accounts

The **Accounts** view lets you create, edit, enable, disable, and remove
OpenCode‑Go accounts. Each account card shows the account name, the
cooldown state, and the 5‑hour / weekly / monthly usage bars driven by
local accounting. Type a percentage or drag a bar to set its current
real-world usage baseline. After the value is saved, successful request
cost recorded by OCG Manager continues to accumulate above that baseline.
Reaching 100% is still only a warning; it does not stop the gateway from
selecting the account. You can paste an OpenCode‑Go `username` and
`password` alongside the key; both the key and saved password are only
obfuscated on disk. The gateway uses the password to refresh upstream
sessions when needed.

You can also reset a cooldown manually from this view. The bar snaps back
to its local estimate as soon as the cooldown is cleared.

### Logs

The **Logs** view shows the rolling buffer of requests the gateway has
forwarded, including the timestamp, the chosen account, the model, the
status code, the upstream error if any, and the streamed usage when the
upstream emitted a usage chunk. Rows with `success_no_usage` mean the
stream finished without a usage chunk. A usage chunk makes token counts
accurate; cost is still estimated from the local model price table.

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
- **Check for updates** — checks the latest GitHub Release and shows the
  current version, latest version, and a **View release** link. It does not
  download or install the release. The host running the Gateway must be able
  to reach `api.github.com`; a failed check does not affect Gateway forwarding.

Configuration settings are written to SQLite and reloaded on the next start.
The update check is an on‑demand action and is not persisted.

## Gateway Behavior

### Endpoints

The gateway is served at `http://<bind>:<port>` and exposes:

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/messages` | Anthropic Messages |
| `GET`  | `/v1/models` | OpenAI model list |
| `POST` | `/v1beta/models/{model}:generateContent` | Gemini non-stream generation (`/v1/models/...` is also accepted) |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | Gemini SSE generation (`/v1/models/...` is also accepted) |
| `GET`  | `/claude-desktop/v1/models` | Claude Desktop alias model list |
| `POST` | `/claude-desktop/v1/messages` | Claude Desktop Messages with alias rewriting |
| `GET`  | `/dashboard/` | Vue 3 dashboard (HTML) |
| `*`    | `/dashboard/api/...` | Dashboard JSON API |

The default bind is `127.0.0.1:9042`. The CLI can override the host with
`serve --host 0.0.0.0` and the port with `serve --port <port>`. The
desktop app also binds loopback and uses a Tauri single‑instance lock to
prevent two tray apps from competing for the port. There is no HTTP health
endpoint; Docker checks container-internal TCP port `9042`.

### Authentication

Gateway API endpoints require the **Gateway Key**, accepted as
`Authorization: Bearer <key>`, the Anthropic-compatible
`x-api-key: <key>`, or the Gemini-compatible `x-goog-api-key: <key>` header.
Dashboard authentication depends on the listener bind:

- **Loopback binds (the default).** Requests that come straight to the
  loopback address skip dashboard login unless they carry `Forwarded`,
  `x-forwarded-for`, `x-forwarded-proto`, or `x-real-ip`; any of those
  headers requires login. The client still needs the **Gateway Key** to
  reach the upstream endpoints. This is what the desktop app and the
  default CLI use.
- **Non‑loopback binds.** A single administrator account, stored as an
  Argon2 password hash in SQLite, governs the dashboard. Sign‑in returns
  an HttpOnly session cookie. Standard reverse‑proxy forwarding headers
  on a non‑loopback bind still require the cookie. In Docker, the first
  administrator can be bootstrapped with `OCG_ADMIN_USERNAME` and
  `OCG_ADMIN_PASSWORD`; otherwise the first registration wins.

The Gateway Key is the only credential the client needs. It is local — it
has nothing to do with the OpenCode‑Go account key, which the gateway
retrieves from SQLite and sends upstream with its own
`Authorization: Bearer <opencode-go-key>` header.

Minimal POSIX-shell checks for all four client formats:

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

# Gemini generateContent
curl "$BASE/v1beta/models/deepseek-v4-flash:generateContent" \
  -H "x-goog-api-key: $KEY" -H "Content-Type: application/json" \
  -d '{"contents":[{"role":"user","parts":[{"text":"ping"}]}]}'
```

Claude Desktop uses the same Gateway Key but sends one of the aliases returned
by `GET /claude-desktop/v1/models` to
`POST /claude-desktop/v1/messages`. The default aliases are
`claude-sonnet-4-6`, `claude-opus-4-6`, and
`claude-haiku-4-5-20251001`; the gateway rewrites them to the model targets
saved from the Applications view.

### Protocol Conversion

Each known model is mapped to its native OpenCode‑Go protocol. When a
request arrives in a different protocol, the gateway converts the
**request body** to the upstream protocol and the **response body** (or
SSE stream) back to the client protocol. Conversion covers text, system
instructions, images, tool calls and tool results, reasoning content,
completion status, errors, and usage fields.

Gateway protocol endpoints accept JSON request bodies up to 16 MiB. This
transport limit is separate from each model's context window. If a reverse
proxy is in front of OCG Manager, configure it to allow request bodies of at
least 16 MiB or it may return `413 Payload Too Large` before the gateway sees
the request.

Gemini `generateContent` and `streamGenerateContent` are client-only formats:
the gateway never sends Gemini wire data upstream. It converts `contents`,
text-only `systemInstruction`, supported `inlineData` images,
`functionDeclarations`, function calls/results, JSON-schema output, generation
options, Google error envelopes, usage metadata, and SSE frames to and from the
known model's native Chat Completions or Messages protocol. Both the `v1beta`
and `v1` URL forms are accepted. An unknown Gemini model returns `400` because
there is no safe native upstream protocol to choose.

Claude Desktop follows the existing Messages conversion path after its
advertised model alias has been rewritten. The `sonnet`, `opus`, and `haiku`
mappings are serialized inside `AppConfig`; omitted roles inherit the first
configured role, while the dashboard returns the resolved three-role mapping.

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
authoritative billing. An upstream usage chunk provides accurate streaming
token counts, but cost always remains an estimate calculated from the
local model price table. Without a usage chunk, the log row ends with
`success_no_usage`. A manually saved percentage becomes the baseline for
that window; successful costs recorded after the save are added to it until
the next manual change or a recognized upstream limit reset.

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
  5‑hour, weekly, and monthly limit messages use the reset duration reported
  by the upstream and reset the matching usage baseline. During cooldown the
  matching bar remains at 100%; after cooldown it starts at 0% and accumulates
  new successful local cost. An unrecognized 429 falls back to a five‑minute
  cooldown without clearing any manually maintained usage value.
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
can override it with `--data-dir <path>`. The obfuscation secret defaults
to `<data-dir>/.encryption-key`; you can override it with the named
`--encryption-key <key>` option or the `OCG_MANAGER_ENCRYPTION_KEY`
environment variable.

```text
ocg-manager-cli
├── serve         Start the gateway server
│   --host        Address to listen on (default 127.0.0.1)
│   -p, --port    Gateway port (sets and saves config)
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

`serve --port <port>` writes the new port to SQLite. Later `serve` runs
without `--port` reuse that saved value.

`key ping` reads the obfuscated key, sends a tiny chat completion, and
prints the real upstream status code and a short body excerpt — use it to
surface real `401`/`403`/`429`/`200` from each key without going through
the dashboard.

## Docker

The public headless image can be pulled from GHCR without signing in. It is a
Linux container and currently publishes `linux/amd64` only; there is no native
ARM64 image. Each release also includes a pull-only `compose.example.yaml`;
save it as `compose.yaml` and optionally create a neighboring `.env`. The
example pins its matching release by default, while `OCG_IMAGE` can override
it. Alternatively, run the Compose commands from a checkout containing
`compose.yaml` and `.env.example` (preferably the matching release tag):

```bash
git clone --branch v1.3.2 --depth 1 https://github.com/klarkxy/opencode-go-mgr.git
cd opencode-go-mgr
cp .env.example .env
# PowerShell: Copy-Item .env.example .env
# Edit .env before exposing the service outside the host.
docker compose pull
docker compose up -d --no-build
docker compose ps
```

The repository's source-capable `compose.yaml` defaults to
`ghcr.io/klarkxy/opencode-go-mgr:latest`; the Release
`compose.example.yaml` defaults to its matching full version. For repeatable
production deployments, set `OCG_IMAGE` in `.env` to a full release tag such
as `ghcr.io/klarkxy/opencode-go-mgr:1.3.2`. The full version and
`sha-<commit>` tags identify one release and are intended not to move;
`1.3` and `latest` move forward. Only a digest such as
`ghcr.io/klarkxy/opencode-go-mgr@sha256:...` is technically immutable. To build the current checkout
instead, set `OCG_IMAGE=ocg-manager:local` and run
`docker compose up -d --build`. `NPM_REGISTRY` and `CARGO_REGISTRY` are build
arguments for that source-build path only; they do not change a pulled image.

| Variable | Scope | Meaning |
| --- | --- | --- |
| `OCG_IMAGE` | Compose | Image tag, mirror, local name, or immutable digest. |
| `OCG_PORT` | Compose | Host loopback port; the container still listens on `9042`. |
| `OCG_ADMIN_USERNAME` + `OCG_ADMIN_PASSWORD` | First start | Optional administrator bootstrap; both or neither. |
| `OCG_CLIENT_ROOT_URL` | Runtime | Read-only external client root override. |
| `OCG_MANAGER_ENCRYPTION_KEY` | Runtime restore | Original explicit obfuscation key, when one was used. |
| `NPM_REGISTRY` + `CARGO_REGISTRY` | Source build | Dependency registries used only by `--build`. |

`OCG_ADMIN_USERNAME` and `OCG_ADMIN_PASSWORD` create the administrator
**only when the database has no administrator yet**. Both must be set
together; setting only one stops startup with an error. Once an
administrator exists, later environment changes do not reset it. When
both are omitted, the first visitor creates the administrator in the
dashboard. After the administrator exists, you may remove both variables
while keeping the volume; the stored account remains. Bootstrap credentials
are visible to anyone with Docker daemon access. Protect `.env`, use a long
random password, and do not expose an uninitialized dashboard publicly. After
initialization, remove both values and run
`docker compose up -d --no-build --force-recreate` to remove them from the
container environment.

`OCG_MANAGER_ENCRYPTION_KEY` is an advanced restore override. Leave it unset
for normal deployments so the generated `.encryption-key` stays in the data
volume. If the original deployment supplied this variable, the restored
deployment must use the same value; changing or losing it makes saved
credentials unreadable. Treat it like a password.

The optional `OCG_CLIENT_ROOT_URL` is the environment equivalent of the
dashboard's Downstream Access Root. Use it when a reverse proxy is present or
the Dashboard and Gateway have different externally reachable addresses. A
non-empty value must be an absolute HTTP(S) URL; when present, it overrides the
saved SQLite value, and an invalid value stops startup. It does not configure the
listener, DNS, or reverse proxy. Normally use `https://ocg.example.com`, not
`/dashboard/` or a concrete API endpoint; a trailing `/v1` is accepted.

Set `OCG_PORT` in `.env` to change the host port; the container still uses
port `9042`. Open `http://127.0.0.1:<OCG_PORT>/dashboard/` and sign in.
Data and the generated `.encryption-key` obfuscation secret persist in the
`ocg-data` volume. The container process binds `0.0.0.0`, so the dashboard
requires administrator login even when it is published only on host
`127.0.0.1`. That host mapping limits reachability; it does not enable
loopback login bypass. The container's `HEALTHCHECK` opens
`127.0.0.1:9042` over TCP every 30 seconds; there is no `/healthz` route.
That TCP check proves only that the process is listening; it does not prove
that the dashboard API, an upstream account, or a real model request works.
Use `/dashboard/`, not the server root `/`.

The image runs as the unprivileged `ocg` user (UID/GID 10001). The supplied
Compose service makes the root filesystem read-only, mounts `/tmp` as tmpfs,
drops every Linux capability, and enables `no-new-privileges`. The named
`ocg-data` volume remains writable and is the only persistent application
state. Routine operational checks are:

```bash
docker compose config --quiet
docker compose ps
docker compose logs --tail=100 -f ocg-manager
curl --fail http://127.0.0.1:9042/dashboard/
```

Replace `9042` in the curl command with the configured host `OCG_PORT` when
you changed it.

The startup log contains the Gateway Key, so log output and Docker daemon
access are sensitive. Configure log rotation in your Docker host if its
defaults are not bounded.

Each stable image includes an SPDX SBOM, BuildKit SLSA provenance, and a
GitHub signed provenance attestation. Inspect and verify a release with:

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:1.3.2
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr:1.3.2 \
  --repo klarkxy/opencode-go-mgr
```

The second command requires an authenticated GitHub CLI. Public pulls are
anonymous; if the OCI client still requests registry credentials, authenticate
to `ghcr.io` with a token that can read packages. Provenance proves how the
artifact was produced; it is not a vulnerability scan.

Regenerate the Gateway Key if it leaks.

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
- **Credential storage.** Account keys and saved login passwords are
  obfuscated before storage; this is not cryptographic protection. The
  macOS / Linux GUI and the CLI also place a
  `.encryption-key` file inside the data directory; **back it up with
  the database** because losing it makes stored credentials unreadable.
  Obfuscation is not a security boundary: anyone with the data directory
  and its `.encryption-key`, or able to run the Windows GUI in the original
  Windows user/machine context, can recover account keys and saved login
  passwords.
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

- `/embeddings` is not implemented. Gemini `embedContent` is routed but
  returns a Google-style `501 UNIMPLEMENTED` response.
- Gemini `countTokens` also returns `501`; Gemini CLI is expected to fall back
  to local token estimation. Only `generateContent` and
  `streamGenerateContent` are forwarding actions.
- Non-empty Gemini `safetySettings` return `400` because a different upstream
  protocol cannot preserve their safety semantics. `null` and an empty array
  are accepted because they impose no policy.
- Gemini `cachedContent`, `fileData`, Google Search tools, `urlContext`,
  multimodal function-response parts, function response schemas/behavior,
  `VALIDATED` function calling, candidate counts other than one, and response
  modalities other than `TEXT` return `400`. Use base64 `inlineData` for PNG,
  JPEG, GIF, or WebP images.
- Gemini `topK` and `thinkingConfig` are accepted only as cross-protocol
  compatibility hints. A native Chat Completions or Messages upstream may
  ignore them or implement different semantics; exact Gemini-equivalent
  sampling and thinking behavior is not guaranteed.
- Other non-null generation options that cannot be preserved, including
  `seed`, presence/frequency penalties, log-probability controls, and media
  resolution, return `400` instead of being silently discarded.
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
- Streaming token counts are accurate only when upstream emits usage
  chunks; cost always uses the local price table. Without usage, logs end
  as `success_no_usage`.
- The current HTTP dashboard does not expose the older isolated WebView
  browser command.
- The installed Windows desktop dashboard can start OCG Manager in the tray
  when the user logs in. Development builds, macOS, Linux, CLI, and Docker do
  not expose that dashboard `auto_start` switch. Docker Compose separately
  uses `restart: unless-stopped`, so its service can restart with the Docker
  daemon.
- Windows / Linux ARM64 and 32‑bit x86 builds are not published. RPM,
  Snap, app‑store packages, automatic update download/installation,
  Windows signing, and Apple notarization are not implemented. Settings
  can check the latest GitHub Release manually.

## Troubleshooting

- **The dashboard never opens from the tray.** Another process is bound
  to `127.0.0.1:9042`, or a previous tray app still holds the
  single‑instance lock. Quit that process or the previous release tray app
  and retry. For source development only, `scripts/free-dev-port.mjs`
  clears stale Vite processes on port `30001`; it does not release `9042`
  or the desktop single-instance lock.
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
  yet. Use the stored administrator account. Recreate `ocg-data` only for
  an intentional full reset after a verified backup; doing so erases every
  account, credential, and setting.
- **SmartScreen / Gatekeeper warns about the installer or the DMG.** The
  current Windows builds are unsigned and the macOS app is ad‑hoc
  signed. Use **Open Anyway** for the first launch; the warning is not a
  sign of tampering.

---

[中文用户指南](USER.zh-CN.md) · [Maintainer guide](MAINTAINER.md) ·
[维护者指南](MAINTAINER.zh-CN.md) · [Back to README](../README.md)
