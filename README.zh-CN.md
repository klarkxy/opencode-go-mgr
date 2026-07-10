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

## 真熔断与假熔断

5 小时、每周和每月用量条由本地转发记录估算。本地用量达到限额属于假熔断：本地计费口径或刷新时间可能与上游不同，因此即使进度条已经满额，Gateway 也会继续使用该账号发送请求，不会写入冷却状态。本地满额只表示警告，不能证明上游已经限制账号。

只有上游返回 HTTP 429 才会触发真熔断。Gateway 会保存上游错误，根据响应中的重置时间写入 `cooldown_until`，并切换到下一个可用账号。已知的 5 小时、每周和每月限额提示会采用上游给出的重置时长；无法识别重置时间的 429 默认冷却 5 分钟。如果所有已启用账号都在冷却，Gateway 会返回 429，并给出最早的恢复时间。

真熔断后，管理面板会把对应的 5 小时、每周或每月进度条拉满并标红，即使本地估算值更低。账号会在 `cooldown_until` 到期后自动恢复，也可以在管理面板中手动解除冷却。

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
