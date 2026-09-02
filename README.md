[简体中文](README.zh-CN.md)

# OCG Manager

A local gateway that keeps provider credentials in one SQLite database and
serves five client protocols on one port (`http://127.0.0.1:9042`). Your local
AI tools share routing and access control instead of maintaining a separate
provider setup in every client.

Each account belongs to one Provider/Plan (`provider_id`) and, when required,
one credential. Clients send local aliases; the gateway converts requests to
the Plan's upstream protocol and converts responses back. Built-in routes cover
OpenCode Go, OpenCode Zen Free, Command Code GOAT, MiniMax CN Token Plan, Kimi
Code CN, and Custom API. Typed user-defined Providers use Configurable HTTP
without loading plugin code; CPA is an optional local Extension. OCG Manager
has no telemetry or remote sync.

## Highlights

- **One port, five wire formats** — OpenAI Chat Completions, OpenAI Responses,
  Anthropic Messages, Gemini `generateContent` / `streamGenerateContent`, and
  Claude Desktop.
- **Drag to reroute** — account cards persist one global order; strict
  priority, sticky, and round-robin reuse it after capability filtering.
- **Quota bars are warnings, not walls** — local estimates never stop traffic;
  only an upstream `429` cools an account down.
- **Desktop, CLI, Docker** — a Tauri v2 tray app, `ocg-manager-cli`, and
  `ghcr.io/klarkxy/opencode-go-mgr` for local operation.

## Architecture At A Glance

[![OCG Manager local-node architecture](https://klarkxy.github.io/opencode-go-mgr/diagrams/local-node.visual-check.1440x900.light.png)](https://klarkxy.github.io/opencode-go-mgr/diagrams/local-node/)

[Explore all interactive architecture and workflow diagrams](https://klarkxy.github.io/opencode-go-mgr/).

## Download

Grab the GUI installer or CLI archive from the
[latest GitHub Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest)
and check it against that release's `SHA256SUMS` (`Get-FileHash <file>
-Algorithm SHA256` on PowerShell, `shasum -a 256` on macOS, `sha256sum` on
Linux):

| Platform | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe` (NSIS) | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel and Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` and `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

Keep `dist/` beside the CLI executable, or `serve` has no dashboard to serve.
Platform caveats: [install guide](docs/user/install.md).

## Quick Start

```text
Gateway: http://127.0.0.1:9042/v1
Auth:    Authorization: Bearer <key>
```

1. Install and launch. The dashboard opens in your system browser when the
   gateway is ready; the tray icon brings it back.
2. In **Accounts**, add a Plan and its credential when needed. Copy a client
   **Key** from **Access Keys**; it is the only OCG Manager credential your
   client needs.
3. Point your client at `http://127.0.0.1:9042/v1`. **Applications** has
   per-client setup guides.

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

Install details, first-client checks, backup, and upgrades: [User guide](docs/USER.md).

## Docker

Run it with the published image or from source; browser sidecar, backup, HTTPS,
image pins, and Compose instructions are in the [Docker guide](docs/user/docker.md).

## Preferred Protocol Groups

OpenCode Go models have a preferred upstream protocol. Matching supported
client protocols pass through; other supported clients are converted. The gateway never probes protocols on a request path.

| Preferred upstream | Group |
| --- | --- |
| OpenAI Chat Completions | General and free OpenCode Go models |
| OpenAI Responses | Reasoning and contributor models |
| Anthropic Messages | MiniMax and Qwen models |

Zen Free uses a saved official catalog snapshot. Gemini is a client format, not
an upstream destination. Complete model, capability, and conversion tables are in [model capabilities](docs/user/applications.md) and
[protocol conversion](docs/user/protocol-conversion.md).

## Next

[User guide](docs/USER.md) · [Maintainer guide](docs/MAINTAINER.md) ·
[Documentation index](docs/README.md) · [Contributors](docs/CONTRIBUTORS.md) ·
[DESIGN.md](DESIGN.md) · [AGENTS.md](AGENTS.md)

## Community

Join the OCG Manager QQ group: **1104321231**.

<p align="center">
  <img src="assets/qq-group.png" alt="OCG Manager QQ group QR code" width="360" />
</p>

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
