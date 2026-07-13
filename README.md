# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="140">
</p>

OCG Manager is a local, multi-account operations console for OpenCode‑Go. It
stores your OpenCode‑Go account keys in a local SQLite database, exposes an
OpenAI‑compatible gateway at `http://127.0.0.1:9042/v1`, and serves a Vue 3
dashboard from that same gateway. The desktop build is a Tauri v2 tray app for
Windows, macOS, and Linux; a headless CLI is shipped alongside the GUI.

<p align="center">
  <img src="assets/opencode娘.png" alt="OpenCode-Go mascot" width="360">
</p>

## Highlights

- **OpenAI / Anthropic compatible gateway** — `POST /v1/chat/completions`,
  `POST /v1/responses`, `POST /v1/messages`, and `GET /v1/models` are accepted
  on the same port. Requests are routed to the model's native OpenCode‑Go
  protocol and the response is converted back to the client protocol.
- **Local multi‑account rotation** — accounts are tried in list order. Disabled
  accounts, accounts cooling down, and accounts that already failed during the
  current request are skipped, with a fast failover.
- **Local cost accounting** — 5‑hour, weekly, and monthly usage bars are
  estimated from the requests the gateway actually forwards.
- **Dashboard first run** — the first visitor on a non‑loopback bind creates
  the single administrator account; the desktop and CLI builds bind loopback
  by default and skip login.
- **Tray app on every desktop platform** — the Windows installer, the macOS
  Universal DMG, and the Linux AppImage/`.deb` all share the same Tauri v2
  codebase and a single instance lock.
- **Headless CLI** — `ocg-manager-cli` ships with the same dashboard assets
  and is ideal for servers, Docker, and remote gateways.
- **No remote sync, no telemetry** — each node owns its own data; there is no
  cloud service, no Admin API, and no cross‑node synchronization.

## Download

Download the GUI installer or CLI archive for your platform from the
[latest GitHub Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest).
Download `SHA256SUMS` from the same release and compare the matching entry with
the artifact's SHA-256 before installing. On PowerShell use
`Get-FileHash <file> -Algorithm SHA256`; on macOS use `shasum -a 256 <file>`;
on Linux use `sha256sum <file>`.

## Quick Start

The default gateway address and the header that carries the local key:

```text
Gateway: http://127.0.0.1:9042/v1
Auth:    Authorization: Bearer <key>
```

The key shown in the dashboard is the **Gateway Key**. It is the only secret
your client needs; the gateway uses the OpenCode‑Go account key that the
dashboard already stores.

The smallest end‑to‑end check — a streaming Chat Completions request against a
representative model:

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

On a normal desktop launch, OCG Manager opens the dashboard in your system
browser after the Gateway is ready. Add an OpenCode‑Go account, copy the
Gateway Key, and point any OpenAI‑compatible client at
`http://127.0.0.1:9042/v1`. If the browser does not open or you close the tab,
use the tray icon to reopen the dashboard.

## True And False Circuit Breakers

The 5‑hour, weekly, and monthly usage bars are **local estimates** derived
from the requests the gateway has forwarded. They are not the upstream's
authoritative view of your billing window.

- **False circuit breaker (local estimate):** when the local estimate reaches
  the limit, the gateway **keeps sending** requests with that account. Local
  accounting and upstream billing/reset boundaries may not match, so a full
  local bar is a warning, not proof that the upstream account is blocked.

- **True circuit breaker (upstream 429):** the gateway writes the upstream
  error, parses the `Resets in …` phrase from the response, stores
  `cooldown_until`, and switches to the next available account. The known
  5‑hour, weekly, and monthly limit messages use the reset duration reported
  by the upstream; an unrecognized 429 falls back to a five‑minute cooldown.
  If every enabled account is cooling down, the gateway returns `429` with
  the soonest reset time.

While a true circuit breaker is active, the dashboard forces the matching
5‑hour, weekly, or monthly bar to 100% and marks it as an error, even when
the local estimate is lower. The account becomes eligible automatically
after `cooldown_until`, or immediately after you reset its cooldown in the
dashboard.

## Supported Models And Protocols

Each known model is mapped to its native OpenCode‑Go protocol. Requests in a
different protocol are converted automatically; this includes text, system
instructions, images, tool calls and tool results, reasoning content,
completion status, errors, and usage fields.

- **OpenAI Chat Completions** — `glm-5.2`, `glm-5.1`, `kimi-k2.7-code`,
  `kimi-k2.6`, `deepseek-v4-pro`, `deepseek-v4-flash`, `mimo-v2.5`,
  `mimo-v2.5-pro`.
- **Anthropic Messages** — `minimax-m3`, `minimax-m2.7`, `minimax-m2.5`,
  `qwen3.7-max`, `qwen3.7-plus`, `qwen3.6-plus`.

Unknown models keep the request's native Chat Completions or Messages
protocol. An unknown model requested through the Responses endpoint is
rejected with `400` — the gateway will not guess a protocol by trial because
that could double‑bill the request.

## Documentation

- [中文 README](README.zh-CN.md) · [English README](README.md)
- [User guide](docs/USER.md) · [用户指南](docs/USER.zh-CN.md)
- [Maintainer guide](docs/MAINTAINER.md) · [维护者指南](docs/MAINTAINER.zh-CN.md)
- [Security policy](SECURITY.md)
- [OpenCode‑Go anti‑abuse statement](OPENCODE_GO_ANTI_ABUSE.md) ·
  [OpenCode‑Go 防滥用声明](OPENCODE_GO_ANTI_ABUSE.zh-CN.md)

## Development

```bash
pnpm install
pnpm run dev
```

Before you start, exit any running release tray app so the single‑instance
lock and port `9042` are free. Tauri starts Vite, waits for the Gateway to be
ready, and opens `http://127.0.0.1:30001/dashboard/`. Vue, CSS, and frontend
TypeScript changes use Vite HMR; Rust changes use Cargo's incremental
compiler and restart the process. Checks, builds, release validation, and
the supported platform matrix live in the
[Maintainer guide](docs/MAINTAINER.md).

## Release Artifacts

`pnpm run build` builds the GUI and CLI for the **current supported native
platform** and atomically replaces `release/`. There is no cross‑compile
matrix on a single machine.

| Platform | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe` (NSIS) | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel and Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` and `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

A CLI archive contains the executable, `dist/`, and `LICENSE`. The `dist/`
folder must sit beside the executable so `serve` can serve the dashboard.
`SHA256SUMS`, signing and SmartScreen/Gatekeeper caveats, and the
unsupported list (ARM64, 32‑bit x86, RPM, Snap, app stores, auto‑update) live
in the [Maintainer guide](docs/MAINTAINER.md).

## License

See [LICENSE](LICENSE).
