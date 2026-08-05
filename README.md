[简体中文](README.zh-CN.md)

# OCG Manager

OCG Manager is a local operations console for OpenCode-Go accounts. It stores
your account keys in a local SQLite database and serves them through a
multi-protocol gateway at `http://127.0.0.1:9042` — the same port that hosts
the management dashboard. Clients can speak OpenAI, Anthropic, Gemini, or
Claude Desktop protocol; the gateway converts each request to the model's
native OpenCode-Go protocol and converts the response back.

<p align="center">
  <img src="assets/骑自行车的opencode娘.png" alt="OpenCode-Go mascot" width="360">
</p>

## Highlights

- **Multi-protocol gateway** — OpenAI Chat Completions / Responses, Anthropic
  Messages, Gemini `generateContent` / `streamGenerateContent`, model
  discovery, and Claude Desktop aliases on one port.
- **Local multi-account rotation** — drag account cards to persist priority;
  the gateway skips disabled, cooling, or already-failed accounts.
- **Managed onboarding and isolated profiles (Beta)** — manually complete
  optional sign-in identity (Google/GitHub), OpenCode Go, payment, and key
  verification through an invite URL; steps can rewind; each account keeps its
  own browser login, with an optional Docker noVNC sidecar. This feature has
  not been thoroughly tested; do not rely on it in production.
- **Purchase-cycle reminders** — per-account purchase dates and monthly
  expiry with remaining days in the dashboard; expiry never blocks an account.
- **OpenCode Go quota estimates** — 5-hour, weekly, and monthly usage bars
  from a USD pricing snapshot. Key accounts support manual calibration; ready
  managed accounts can refresh quota from the console session in that profile.
- **16 client guides** — copy-ready configuration snippets for Claude Code,
  Claude Desktop, Codex, Gemini CLI, Pi, Kimi Code CLI, WorkBuddy, and nine
  other tools.
- **Tray app and headless CLI** — a Tauri v2 tray app for Windows, macOS, and
  Linux, plus `ocg-manager-cli` for servers and Docker.
- **Signed desktop updates** — installed desktop builds check, verify, and
  install updates from Settings.
- **No remote sync, no telemetry** — every node owns its own data.

## Download

Download the GUI installer or CLI archive for your platform from the
[latest GitHub Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest),
and verify it against `SHA256SUMS` from the same release before installing:
`Get-FileHash <file> -Algorithm SHA256` on PowerShell, `shasum -a 256 <file>`
on macOS, or `sha256sum <file>` on Linux.

| Platform | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe` (NSIS) | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel and Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` and `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

Each CLI archive contains the executable, `dist/`, and `LICENSE`; keep `dist/`
beside the executable so `serve` can serve the dashboard. There is no
cross-compile matrix: `pnpm run build` only produces artifacts for the current
native platform. Signed-updater behavior, SmartScreen/Gatekeeper caveats, and
the unsupported list (ARM64, 32-bit x86, RPM, Snap, app stores) are covered in
the [Maintainer guide](docs/MAINTAINER.md).

## Quick Start

```text
Gateway: http://127.0.0.1:9042/v1
Auth:    Authorization: Bearer <key>
```

The key shown in the dashboard is the **Gateway Key** — the only secret your
client needs. The gateway injects the stored OpenCode-Go account key on the
upstream side.

1. Install and launch OCG Manager. The dashboard opens in your system browser
   once the gateway is ready; use the tray icon to reopen it.
2. Import an existing key in **Accounts**, or use managed onboarding (edit the
   invite URL on the create-draft form; replace the demo default with your own
   before a real signup); then copy the Gateway Key.
3. Point your client at `http://127.0.0.1:9042/v1`. The **Applications** view
   has per-client configuration guides.

The smallest end-to-end check — a streaming Chat Completions request:

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

### Docker quick start

The public headless image is `ghcr.io/klarkxy/opencode-go-mgr` (currently
`linux/amd64`, no registry login required). For a pull-only deployment without
the source tree, save [`compose.example.yaml`](compose.example.yaml) (also
attached to each Release) as `compose.yaml` and optionally create a
neighboring `.env`. Or run from a checkout:

```bash
git clone --branch v1.5.7 --depth 1 https://github.com/klarkxy/opencode-go-mgr.git
cd opencode-go-mgr
cp .env.example .env
# Edit .env: choose first-run administrator setup and pin OCG_IMAGE to 1.5.7.
docker compose pull
docker compose up -d --no-build
docker compose ps
```

For managed onboarding and website login on a Linux server, reserve at least
2 CPUs, 2 GiB of RAM, and 1 GiB of `/dev/shm`, then run
`docker compose --profile browser up -d`. This enables the optional
`ghcr.io/klarkxy/opencode-go-mgr-browser` sidecar. Back up the sensitive
`ocg-data` and `ocg-browser-profiles` volumes together.

Open `http://127.0.0.1:9042/dashboard/`; the server root `/` is not the
dashboard URL. See the [Docker guide](docs/USER.md#docker) for credentials,
persistence, backup/restore, HTTPS, upgrades, digest/attestation verification,
and local source builds.

## Supported Models And Protocols

Each known model has a hardcoded **preferred** OpenCode-Go protocol (from the
official docs endpoint table) and a **supported** set verified with a test
account. When the client protocol is in that set, the gateway **passthroughs**
without conversion. Otherwise it converts to the preferred protocol — text,
system instructions, images, tool calls and results, reasoning content,
completion status, errors, and usage fields.

| Preferred upstream | Models |
| --- | --- |
| OpenAI Chat Completions | `grok-4.5`, `glm-5.2`, `glm-5.1`, `glm-5`, `kimi-k3`, `kimi-k2.7-code`, `kimi-k2.6`, `kimi-k2.5`, `deepseek-v4-pro`, `deepseek-v4-flash`, `mimo-v2.5`, `mimo-v2.5-pro`, `hy3` |
| OpenAI Responses | `gpt-5.6-luna` |
| Anthropic Messages | `minimax-m3`, `minimax-m2.7`, `minimax-m2.7-highspeed`, `minimax-m2.5`, `minimax-m2.5-highspeed`, `qwen3.7-max`, `qwen3.7-plus`, `qwen3.6-plus`, `qwen3.5-plus` |

Passthrough matrix (live test-account probe, 2026-07-31). ✓ = client protocol is forwarded as-is; empty = converted to the model's preferred protocol. Source of truth: `MODEL_PROTOCOLS` in `crates/ocg-core/src/gateway/protocol.rs`.

| Model | Preferred | Chat | Responses | Messages |
| --- | --- | :---: | :---: | :---: |
| `grok-4.5` | Chat | ✓ | ✓ | |
| `glm-5.2` | Chat | ✓ | ✓ | ✓ |
| `glm-5.1` | Chat | ✓ | ✓ | ✓ |
| `glm-5` | Chat | ✓ | ✓ | ✓ |
| `gpt-5.6-luna` | Responses | | ✓ | |
| `kimi-k3` | Chat | ✓ | | |
| `kimi-k2.7-code` | Chat | ✓ | | |
| `kimi-k2.6` | Chat | ✓ | | |
| `kimi-k2.5` | Chat | ✓ | | |
| `deepseek-v4-pro` | Chat | ✓ | | |
| `deepseek-v4-flash` | Chat | ✓ | | |
| `mimo-v2.5` | Chat | ✓ | | |
| `mimo-v2.5-pro` | Chat | ✓ | | |
| `hy3` | Chat | ✓ | | |
| `minimax-m3` | Messages | ✓ | | ✓ |
| `minimax-m2.7` | Messages | ✓ | | ✓ |
| `minimax-m2.7-highspeed` | Messages | ✓ | | ✓ |
| `minimax-m2.5` | Messages | ✓ | | ✓ |
| `minimax-m2.5-highspeed` | Messages | ✓ | | ✓ |
| `qwen3.7-max` | Messages | ✓ | | ✓ |
| `qwen3.7-plus` | Messages | ✓ | | ✓ |
| `qwen3.6-plus` | Messages | ✓ | | ✓ |
| `qwen3.5-plus` | Messages | ✓ | | ✓ |

- **Gemini is a client-only format**: `/v1beta/models/{model}:generateContent`
  and `:streamGenerateContent` (plus `/v1/models/...` aliases) are converted
  to the selected model's preferred protocol; clients may authenticate with
  `x-goog-api-key`. Requests never go to Google.
- **Claude Desktop** uses `/claude-desktop/v1/...` with the aliases
  `claude-sonnet-4-6`, `claude-opus-4-6`, and `claude-haiku-4-5-20251001`;
  each alias is rewritten to its saved model mapping.
- **Unknown models** keep the request's native Chat Completions or Messages
  protocol. Unknown models on Responses or Gemini, and unknown Claude Desktop
  aliases, are rejected with `400` — the gateway never guesses a protocol by
  trial, because that could double-bill a request.

Replay is deliberately conservative: only a pre-send DNS/TCP/TLS connection
failure is retried once on the same account; `401`/`403`/`429` may fail over
to another account; `408`, `5xx`, post-connect failures, body timeouts, and
interrupted streams are never replayed. The Gemini compatibility boundary
(`countTokens`/`embedContent` `501`, rejected fields) and the full replay
rules live in the [User guide](docs/USER.md#limits).

## True And False Circuit Breakers

The 5-hour, weekly, and monthly usage bars are **local estimates**, not the
upstream's authoritative billing view.

- **False circuit breaker (local estimate):** a full local bar is only a
  warning — the gateway **keeps sending** requests with that account.
- **True circuit breaker (upstream 429):** the gateway parses the `Resets in …`
  phrase, stores `cooldown_until`, and switches to the next available account.
  An unrecognized 429 cools down for five minutes; if every enabled account is
  cooling down, the gateway returns `429` with the soonest reset time.

While a true circuit breaker is active, the dashboard forces the matching bar
to 100% and marks it as an error. The account recovers automatically after
`cooldown_until`, or immediately when you reset its cooldown. Details:
[User guide](docs/USER.md#true-and-false-circuit-breakers).

## Documentation

| Audience | English | 简体中文 |
| --- | --- | --- |
| README | [README.md](README.md) | [README.zh-CN.md](README.zh-CN.md) |
| End users | [User guide](docs/USER.md) | [用户指南](docs/USER.zh-CN.md) |
| Maintainers | [Maintainer guide](docs/MAINTAINER.md) | [维护者指南](docs/MAINTAINER.zh-CN.md) |
| Policy | [Anti-abuse statement](docs/OPENCODE_GO_ANTI_ABUSE.md) | [防滥用声明](docs/OPENCODE_GO_ANTI_ABUSE.zh-CN.md) |
| Credits | [Contributors](docs/CONTRIBUTORS.md) | same file |
| Index | [docs/](docs/README.md) | — |

## Development

```bash
pnpm install
pnpm run dev
```

Exit any running release tray app first so the single-instance lock and port
`9042` are free. Tauri starts Vite and opens
`http://127.0.0.1:30001/dashboard/` once the gateway is ready; the frontend
hot-reloads, Rust changes rebuild and restart. Checks, builds, and the release
pipeline live in the [Maintainer guide](docs/MAINTAINER.md).

## License

See [LICENSE](LICENSE).

## Star History

<a href="https://www.star-history.com/?type=date&repos=klarkxy%2Fopencode-go-mgr">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&theme=dark&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
 </picture>
</a>
