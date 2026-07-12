# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="140">
</p>

OCG Manager is a local OpenCode-Go multi-account manager with an OpenAI-compatible gateway. It stores your keys locally, serves a dashboard from the gateway, and keeps a Windows, macOS, or Linux tray app running in the background.

<p align="center">
  <img src="assets/opencode娘.png" alt="OpenCode-Go mascot" width="360">
</p>

## Quick Start

```text
Gateway: http://127.0.0.1:9042/v1
Auth:    Authorization: Bearer <key>
```

The gateway accepts OpenAI Chat Completions, OpenAI Responses, and Anthropic Messages requests, then converts each request to the selected model's native OpenCode-Go protocol and converts each response back to the client's protocol.

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

## True and False Breakers

The 5-hour, weekly, and monthly usage bars are local estimates. Reaching a locally calculated limit is a false circuit breaker: local accounting and upstream billing or reset boundaries may not match, so the gateway keeps sending requests with that account and does not write a cooldown. A full local bar is only a warning, not proof that the upstream account is blocked.

A true circuit breaker starts only when the upstream returns HTTP 429. The gateway stores the upstream error, parses its reset phrase, writes `cooldown_until`, and switches to the next available account. Known 5-hour, weekly, and monthly limit messages use the reset duration reported by the upstream; an unrecognized 429 falls back to a five-minute cooldown. If every enabled account is cooling down, the gateway returns 429 with the soonest reset time.

During a true circuit breaker, the dashboard forces the matching 5-hour, weekly, or monthly bar to 100% and marks it as an error even when the local estimate is lower. The account becomes eligible automatically after `cooldown_until`, or immediately after its cooldown is reset manually.

## Docs

- [中文 README](README.zh-CN.md)
- [User guide](docs/USER.md)
- [Maintainer guide](docs/MAINTAINER.md)
- [OpenCode-Go anti-abuse statement](OPENCODE_GO_ANTI_ABUSE.md)

## Development

Install dependencies once with `pnpm install`. Exit any running release tray app so the single-instance lock and port `9042` are free, then:

```bash
pnpm run dev
```

Tauri starts Vite and opens `http://127.0.0.1:30001/dashboard/` once the Gateway is ready; Vue, CSS, and TypeScript changes use Vite HMR and Rust changes restart the process. See the [Maintainer guide](docs/MAINTAINER.md) for checks, builds, release validation, and platform coverage.

## Release artifacts

`pnpm run build` builds the GUI and CLI for the current supported native platform, then atomically replaces `release/` — no cross-building from one machine.

| Platform | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe` | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel and Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` and `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

A CLI archive needs `dist/` beside the executable so `serve` can provide the dashboard. For `SHA256SUMS`, signing and SmartScreen/Gatekeeper caveats, and the unsupported list (ARM64, 32-bit x86, RPM, Snap, app stores, auto-update), see the [Maintainer guide](docs/MAINTAINER.md).

## License

See [LICENSE](LICENSE).
