# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="140">
</p>

OCG Manager is a local OpenCode-Go account manager with an OpenAI-compatible gateway. It stores your keys locally, serves a dashboard from the gateway, and keeps a Windows tray app running in the background.

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

For Vue, CSS, or frontend TypeScript changes, keep a Gateway or the release app running on port `9042`, then start Vite:

```bash
pnpm run dev
```

Open `http://127.0.0.1:30001/dashboard/`. Vite applies frontend changes with HMR; no Rust compilation or release build is needed. The dashboard on port `9042` is served from built files and does not hot-update.

For changes under `crates/` or `src-tauri/`, exit the running release tray app first, then use Tauri development mode:

```bash
pnpm run dev:gui
```

Tauri watches the Rust workspace, recompiles incrementally, and restarts the process. This is development reload, not runtime code replacement. Use `pnpm run build` only for final release validation.

Useful checks and focused builds:

```bash
pnpm run typecheck
pnpm run test
pnpm run build:web
pnpm run build:cli
pnpm run build:gui
pnpm run build
```

## Release artifacts

Use `pnpm run build` for a distributable release. It builds the web dashboard, CLI, and Windows GUI, then replaces `release/` with the current artifacts:

```text
release/
├── ocg-manager.exe
├── ocg-manager-cli.exe
├── OCG Manager_1.0.0_x64-setup.exe
└── dist/
```

- `release/ocg-manager.exe` is the portable tray app and must stay beside `release/dist/`.
- `release/OCG Manager_1.0.0_x64-setup.exe` is the Windows installer.
- `release/ocg-manager-cli.exe` is the standalone CLI.
- `target/release/` contains intermediate Rust/Tauri outputs and is not the delivery directory.

`pnpm run build:gui` only updates `target/release`; run `pnpm run artifacts` to resync an already-built GUI, CLI, installer, and dashboard into `release/`. This command replaces the whole delivery directory, so stale release files are removed.

Running the portable GUI may create `ocg-manager.exe.WebView2/`. It is runtime browser data, not a release artifact, and can be deleted after OCG Manager exits.

## License

See [LICENSE](LICENSE).
