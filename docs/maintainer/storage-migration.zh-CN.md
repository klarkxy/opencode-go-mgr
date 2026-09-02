[English](storage-migration.md)

# 存储与迁移

本页是升级、备份与回滚的运维约定。schema 细节见 [持久化](state-and-lifecycle.zh-CN.md#持久化)。

## 数据目录与加密身份

每次打开数据库都使用 Host 解析的 cipher（CLI、桌面、Docker 均为 `Database::open_with_cipher`）。cipher 不匹配会 fail closed；改写密文无法修复不匹配。

| 形态 | 默认数据目录 | 加密身份 |
| --- | --- | --- |
| Windows 桌面（Tauri） | `%USERPROFILE%\.ocg-mgr` | `MachineBoundCipher`，取自 `USERNAME`、`COMPUTERNAME` 与 `APPDATA`。数据目录不作为 cipher 种子；此路径没有 `.encryption-key`。 |
| macOS / Linux 桌面（Tauri） | `~/.ocg-mgr` | `StaticKeyCipher`，取自 `<data-dir>/.encryption-key`（首次启动创建）。 |
| CLI | `~/.ocg-mgr-cli`，或 `--data-dir <path>` | 优先级：`--encryption-key` > `OCG_MANAGER_ENCRYPTION_KEY` > `<data-dir>/.encryption-key`。 |
| Docker | 容器内 `--data-dir /data`（Compose 卷 `ocg-data`） | 同 CLI 解析。可选 `OCG_MANAGER_ENCRYPTION_KEY` 是显式恢复覆盖；正常卷保留 `.encryption-key`。`/data` 内文件必须保持 UID/GID `10001` 可写。 |

这些身份不能混用：

- Windows 桌面数据无法在另一个 Windows 用户或机器上解密账号密文，也无法在 CLI/Docker 静态 cipher 下解密。
- 把 GUI 目录拷到 CLI 默认路径（或反向）会用不同的目录，且在 Windows 上是不同的 cipher。
- 如果进程以 `--encryption-key` 或 `OCG_MANAGER_ENCRYPTION_KEY` 启动，只恢复 `.encryption-key` 不够；必须再次提供同一个显式秘密值。

## 升级与备份

GUI 或 CLI 启动时会原地执行 SQLite 迁移。打开新版二进制前：

1. 停止所有打开该数据目录的进程（桌面托盘 **退出**、CLI Ctrl+C / 服务停止、`docker compose stop`）。WAL 文件与 `data.sqlite` 同属一份库。
2. 备份**整个**数据目录，包括存在时的 `.encryption-key` 与 `browser-profiles/`；Docker 同时备份 `ocg-data` 与 `ocg-browser-profiles` 两个卷。保留上表匹配的加密材料。
3. 签名桌面升级器会自行停止并重启；CLI 与 Docker 升级保持手动。

不支持降级：旧版二进制无法打开已迁移的数据库。需要回滚时，恢复升级前制作的整目录备份。

## Schema v27 与 pre-v3 快照

`CURRENT_SCHEMA_VERSION = 35`（`crates/ocg-core/src/db.rs`）。打开历史库会先规范迁移到 v26，再由 v27 重写把主 Key 与全部 `sub_gateway_keys` 行复制进一张 `access_keys` 表（主 Key 固定 id `00000000-0000-0000-0000-000000000001`），删除 `sub_gateway_keys`，并删除 `accounts` 上遗留的五列 `usage_sync_*`（用量同步元数据在 `provider_usage_sync_state`）。v33 新增 Custom 精确上游模型身份；v34 新增 CPA 单例配置表，但不会导入或导出 CPA 状态。账号 `key_cipher` / `password_cipher` 用 Host cipher 就地校验，**不会重新加密**。

## Schema v31 — 按模型/按协议覆盖

v31 创建 `provider_contract_model_protocol_overrides` 表。每行对应一个合约范围 × 模型 × 协议，`state` 取值 `force_on` / `force_off`；无行即表示“自动”。复合主键为 `(scope_kind, scope_id, model_id, protocol)`。`provider_contract_scopes` 的开关列仍保留在数据库中以保证向后兼容，但 effective 合约推导不再读取它们。

## Schema v32 — Custom 单协议完整 Endpoint

v32 用 `endpoint_url` 与单值 `upstream_protocol` 替换 `account_custom_configs.base_url`、JSON `upstream_protocols` 和 `auth_scheme`。历史行按 Chat Completions → Responses → Messages 选择协议，拼接对应标准推理后缀，并在同一事务中设为 disabled/pending、删除非所选协议的能力/证据/覆盖。管理员检查后必须显式重新启用迁移的 Custom 账号。

## Schema v33 — Custom 上游模型身份

v33 新增非空列 `account_model_capabilities.upstream_model`。历史行以 `model_id`
回填，完整保留原先“公开名称 = 上游 ID”的行为。新建 Custom 映射可保留不同的
公开模型名称与精确上游模型 ID；迁移不做后缀规范化，也不生成 Alias。

## Schema v35 — Ollama Cloud 用量状态

v35 创建 `ollama_cloud_usage_state` 表。每个已配置账号一行，包含：

- `cookie_cipher` —— 抓取 `https://ollama.com/settings` 用量页所需的浏览器会话
  Cookie 混淆密文。它与账号 Key 使用同一 `.encryption-key` 派生设施，明确不是
  AEAD；任何 API 都不回显，也绝不进入导出载荷。
- `status` —— `unconfigured` / `ok` / `unauthorized` / `failed`。
- `snapshot` —— 最近一次成功抓取的脱敏 JSON（5 小时/每周窗口、按模型请求计数、
  可选 plan/余额）。仅在成功时写入；失败只更新状态与退避列，从不清除它。
- `last_success_at`、`last_attempt_at`、`next_eligible_at`、`failure_streak` ——
  手动刷新节流与固定退避阶梯（5 分钟 → 15 分钟 → 1 小时 → 6 小时封顶）。

在任何 v27 写入前，既有（非空）库会得到一份唯一、不覆盖的同目录快照：

```text
data.sqlite.pre-v3.<timestamp>.bak
data.sqlite.pre-v3.<timestamp>.bak.sha256
```

快照是独立的 v26 SQLite 文件（`VACUUM INTO`，两侧都做 `quick_check`）；sidecar 第一个字段是 `.bak` 的小写 SHA-256。全新空目录直接创建到当前 schema，不写这份副本。快照只是回滚点，不能替代整目录备份。恢复前在数据目录内校验 sidecar：

```bash
sha256sum -c data.sqlite.pre-v3.<timestamp>.bak.sha256      # Linux
shasum -a 256 -c data.sqlite.pre-v3.<timestamp>.bak.sha256  # macOS
```

Windows 上用 `Get-FileHash -Algorithm SHA256` 与 sidecar 第一个字段比对。哈希不匹配时，该文件不可用于恢复。

## 回滚与失败的打开

**没有向下迁移。** 回滚是离线的精确文件恢复：

1. 停止所有打开该目录的进程。
2. 按上文校验 sidecar 哈希；不匹配就停止。
3. 把校验过的 `.bak` 复制覆盖 `data.sqlite`，并删除前一个活库留下的 `data.sqlite-wal` / `data.sqlite-shm`。
4. 用同一加密身份启动具备 v26 能力的二进制，或在恢复出的 v26 文件上重试 v27 升级。在 v27 成功打开之后再恢复会丢弃快照之后的全部写入。

失败的 v27 事务会回滚：活库必须仍是 schema 26 且 `sub_gateway_keys` 完好。已有的 pre-v3 文件留在原地；之后成功的 open 会再建一个唯一文件名，而不是覆盖第一份。错误或缺失的 Host cipher 会 fail closed，不会改写 `key_cipher` / `password_cipher`。`ocg-manager-cli status` 会打开数据库并尝试 v27，它不是只读的 schema 检查工具。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](storage-migration.md) · [文档索引](../README.zh-CN.md)
