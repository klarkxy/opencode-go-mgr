# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="140">
</p>

OCG Manager is a local OpenCode-Go account manager with an OpenAI-compatible gateway. It stores your keys locally, serves a dashboard from the gateway, and keeps a Windows, macOS, or Linux tray app running in the background.

<p align="center">
  <img src="assets/opencode娘.png" alt="OpenCode-Go mascot" width="360">
</p>

## Start

```text
Gateway: http://127.0.0.1:9042/v1
Auth:    Authorization: Bearer <gateway-key>
```

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

## Docs

- [中文 README](README.zh-CN.md)
- [User guide](docs/USER.md)
- [Maintainer guide](docs/MAINTAINER.md)
- [OpenCode-Go anti-abuse statement](OPENCODE_GO_ANTI_ABUSE.md)

## Development

Install dependencies once with `pnpm install`.

Exit any running release tray app so the single-instance lock and port `9042` are free, then start the complete development stack:

```bash
pnpm run dev
```

Tauri starts Vite and opens `http://127.0.0.1:30001/dashboard/` after the Gateway is ready. Vue, CSS, and frontend TypeScript changes use Vite HMR; Rust changes use Cargo incremental compilation and restart the process. This is development reload, not runtime code replacement. Use `pnpm run build` only for final release validation.

Useful checks:

```bash
pnpm run test
pnpm run build:web
pnpm run design:lint
pnpm run build
```

## Release artifacts

`pnpm run build` builds the GUI and CLI for the current supported native platform, then atomically replaces `release/`. It does not cross-build all platforms from one machine.

| Platform | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe` | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel and Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` and `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

Every build also writes `SHA256SUMS`. A CLI archive contains the executable, `dist/`, and `LICENSE`; keep `dist/` beside the executable so `serve` can provide the dashboard.

The Windows GUI is installer-only; no portable Windows GUI is published. The first release line is unsigned on Windows, ad-hoc signed on macOS, and checksum-verified on Linux. Windows SmartScreen may warn, and macOS may require approval in **Privacy & Security**. Windows and Linux ARM64, 32-bit x86, RPM, Snap, app stores, and automatic updates are not currently supported.

## License

See [LICENSE](LICENSE).
