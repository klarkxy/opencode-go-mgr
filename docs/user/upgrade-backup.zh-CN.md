[English](upgrade-backup.md)

# 升级、备份、恢复与卸载

从 [GitHub 最新 Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest) 下载升级包，并用同一 Release 的 `SHA256SUMS` 校验：PowerShell 用 `Get-FileHash <文件> -Algorithm SHA256`，macOS 用 `shasum -a 256 <文件>`，Linux 用 `sha256sum <文件>`。下面把备份、恢复和卸载一起讲完——都是平时很枯燥、关键时刻恨自己没看的操作。

## 数据库迁移与接入 Key（schema v34）

数据库 schema 是 **v34**，历史库启动时原地迁移。从单 Key 版本升级会保留既有凭证为 **主 Key**（id 固定为 `00000000-0000-0000-0000-000000000001`），客户端无需改动即可继续鉴权。主 Key 与额外子 Key 共用 `access_keys` 表：未删除子 Key 最多 64 把，删除为软删除，保留名称用于日志归因并清除明文。

已有（非空）库会先规范迁移到 v26，再由 v27 重写把主 Key 与全部 `sub_gateway_keys` 行复制进 `access_keys` 表，删除 `sub_gateway_keys`，并删除 `accounts` 上遗留的五列 `usage_sync_*`。任何 v27 写入前，库会得到同级快照 `data.sqlite.pre-v3.<timestamp>.bak` 及 SHA-256 sidecar。全新空数据目录直接创建 schema v34，不写该副本。快照只是 v26 回滚点，不能替代完整备份：恢复前先校验 sidecar，只能恢复到仍支持 v26 的程序，或用于重试尚未提交的 v27 打开。旧版程序无法打开已迁移的数据库——单 Key 时代不识额外 Key，已撤销的值也不会因降级复活。

v29 从目录中移除 SCNet Token Plans，并在迁移期间删除所有现有 SCNet 账号行。每次启动时，历史 Command Code GOAT 验证状态都会统一为 `not_required`，因为公开目录不是 Key 验证；Custom API 的 enabled 状态保留。OpenCode Go、Zen Free 与未知 provider/offering pair 不受影响。

v30 将 Custom API 的 `account_custom_configs` 从单一 `upstream_protocol` 列扩展为 JSON `upstream_protocols` 集合，按旧值回填每个现有 Custom 账号。Custom 配置/能力编辑保持账号启用，但将 `verification_status` 重置为 `pending`。

v31 新增 `provider_contract_model_protocol_overrides` 表以支持按模型/按协议启用，并停止读取已弃用的 `provider_contract_scopes` 开关列。

v32 将 Custom API 的 base URL、协议集合与可配置鉴权收敛为一个完整推理 Endpoint 和一个上游协议。历史 Custom 行按 Chat Completions → Responses → Messages 选择协议，追加对应标准推理后缀，并置为 disabled/pending 供管理员复核；非所选协议状态在同一事务中移除。

v33 新增 `account_model_capabilities.upstream_model`。历史映射行会以此前的公开 `model_id` 回填，因此升级后既有路由保持不变；新建 Custom 行可使用不同的公开名称与上游名称。

## 备份

1. 停止所有会写数据的进程：从桌面托盘选择 **退出**，用 Ctrl+C 或服务管理器停止 CLI，Docker 则执行 `docker compose stop`。
2. 复制 **整个** GUI 数据目录、CLI 数据目录；桌面账号的 `browser-profiles/` 已包含在 GUI 数据目录中。Docker 必须同时备份 `ocg-data` 与 `ocg-browser-profiles` 两个敏感卷。已停止的 Docker 容器可分别执行 `docker compose cp ocg-manager:/data/. ../ocg-data-backup` 和 `docker compose cp ocg-manager:/browser-profiles/. ../ocg-browser-profiles-backup`。
3. 备份必须放在仓库外，并确认其中有 `data.sqlite`，以及适用时的 `.encryption-key`。浏览器 Profile 含长期 Cookie 和登录状态，不由 OCG Manager 加密，必须按账号 Key 与数据库同等级保护。

## 恢复

1. 先停进程，把现有数据移到别处，再把完整备份放回原目录或空的 Docker 卷。
2. 启动相同或更新的版本。

注意事项：

- Docker `/data` 中的文件必须继续允许 UID/GID `10001` 写入。
- Docker `/browser-profiles` 中的文件也必须继续允许 UID/GID `10001` 写入。
- Windows GUI 的混淆信息绑定 Windows 用户与机器，换机后不能直接恢复账号 Key 或密码；请在新机器创建全新数据并重新录入凭据。
- macOS/Linux GUI、CLI 与 Docker 恢复时必须保留 `.encryption-key`，或原来显式传入的 `--encryption-key` / `OCG_MANAGER_ENCRYPTION_KEY` 值。
- 项目不保证数据库自动向下兼容，旧版本无法打开新版数据库。

## 恢复 Docker 备份到全新卷

先确认备份有效，并确认 `.env` 固定到原版本或更新版本。下面 `docker compose down -v` 会永久删除当前全部命名卷，必须先把两类持久数据另行保存：

```bash
docker compose down -v
docker compose run --rm --no-deps --user root \
  --cap-add CHOWN --cap-add DAC_OVERRIDE --cap-add FOWNER \
  --entrypoint sh \
  --volume ../ocg-data-backup:/backup/data:ro \
  --volume ../ocg-browser-profiles-backup:/backup/browser-profiles:ro \
  ocg-manager \
  -c 'cp -a /backup/data/. /data/ && \
      cp -a /backup/browser-profiles/. /browser-profiles/ && \
      chown -R 10001:10001 /data /browser-profiles && \
      find /data /browser-profiles -type d -exec chmod 700 {} + && \
      find /data /browser-profiles -type f -exec chmod 600 {} +'
docker compose --profile browser up -d --no-build
docker compose ps
```

原部署如果使用了 `OCG_MANAGER_ENCRYPTION_KEY`，恢复前先把同一个秘密值写回 `.env`。在管理面板、账号和一次真实 Gateway 请求都验证通过前，请保留备份。

## 分运行方式的升级与卸载

应用内升级不可用时，按下面方式直接覆盖安装。

- **Windows GUI**：退出托盘程序，运行新版安装包，在“升级方式”页选择 **直接安装（无需先卸载）**。在 Windows **已安装的应用** 中卸载；卸载程序会询问是否删除 `%USERPROFILE%\.ocg-mgr`。
- **macOS GUI**：用新版 DMG 中的应用替换 **Applications** 里的旧应用。删除应用即可卸载；只有确定也要删除数据时才另行删除 `~/.ocg-mgr`。
- **Linux GUI**：用新版 `.deb` 覆盖安装，或替换 AppImage。卸载软件包或删除 AppImage 后，数据仍保留在 `~/.ocg-mgr`，除非手动删除。
- **CLI**：整体替换解压目录，保持可执行文件、`dist/` 与 `LICENSE` 同级。删除该目录即可卸载；数据仍保留在 `~/.ocg-mgr-cli` 或自定义 `--data-dir`。
- **Docker**：备份后依次执行 `docker compose pull` 和 `docker compose up -d --no-build`。如果启用了 browser profile，应改用 `docker compose --profile browser pull` 和 `docker compose --profile browser up -d --no-build`，确保两个镜像同步升级。生产部署建议把 `OCG_IMAGE` 与 `OCG_BROWSER_IMAGE` 固定到完整版本标签。`docker compose down` 只删容器、保留 `ocg-data` 与 `ocg-browser-profiles`；`docker compose down -v` 会永久删除这些卷，只能在确认双卷备份有效且确实要重置时使用。切换到旧镜像不等于回滚数据库；需要数据库回滚时，应同时恢复该旧版本升级前制作的完整备份。

---

[用户指南索引](../USER.zh-CN.md) · [English](upgrade-backup.md) · [文档索引](../README.zh-CN.md)
