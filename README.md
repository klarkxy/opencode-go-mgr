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

## Commands

```bash
pnpm install
pnpm run build:web
pnpm run test
pnpm run build:gui
pnpm run build:cli
```

## License

See [LICENSE](LICENSE).
