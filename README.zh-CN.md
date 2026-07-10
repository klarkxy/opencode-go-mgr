# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="140">
</p>

OCG Manager 是一个本地 OpenCode-Go 多账号管理器，并提供 OpenAI 兼容 Gateway。它本地保存 key，通过 Gateway 托管管理面板，并由 Windows 托盘应用常驻后台。

<p align="center">
  <img src="assets/opencode娘.png" alt="OpenCode-Go 娘" width="360">
</p>

## 快速开始

```text
Gateway: http://127.0.0.1:9042/v1
鉴权:    Authorization: Bearer <gateway-key>
```

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

## 文档

- [User guide](docs/USER.md)
- [Maintainer guide](docs/MAINTAINER.md)
- [OpenCode-Go 防滥用声明](OPENCODE_GO_ANTI_ABUSE.zh-CN.md)
- [English README](README.md)

## 常用命令

```bash
pnpm install
pnpm run build:web
pnpm run test
pnpm run build:gui
pnpm run build:cli
```

## 许可证

见 [LICENSE](LICENSE)。
