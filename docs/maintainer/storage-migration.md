[简体中文](storage-migration.zh-CN.md)

# Storage And Migrations

Operator contract for upgrades, backups, and rollback. Schema details are in [Persistence](state-and-lifecycle.md#persistence).

## Data directories and cipher identity

Every database open uses the Host-resolved cipher (`Database::open_with_cipher` on CLI, desktop, and Docker). A different cipher fails closed; rewriting ciphertext does not fix a mismatch.

| Surface | Default data directory | Cipher identity |
| --- | --- | --- |
| Windows desktop (Tauri) | `%USERPROFILE%\.ocg-mgr` | `MachineBoundCipher` from `USERNAME`, `COMPUTERNAME`, and `APPDATA`. The data directory is not the cipher seed; there is no `.encryption-key` on this path. |
| macOS / Linux desktop (Tauri) | `~/.ocg-mgr` | `StaticKeyCipher` from `<data-dir>/.encryption-key` (created on first launch). |
| CLI | `~/.ocg-mgr-cli`, or `--data-dir <path>` | Priority: `--encryption-key` > `OCG_MANAGER_ENCRYPTION_KEY` > `<data-dir>/.encryption-key`. |
| Docker | container `--data-dir /data` (Compose volume `ocg-data`) | Same CLI resolution. Optional `OCG_MANAGER_ENCRYPTION_KEY` is an explicit restore override; a normal volume keeps `.encryption-key`. Files in `/data` must stay writable by UID/GID `10001`. |

Do not mix these identities:

- Windows desktop data cannot decrypt account ciphertext on another Windows user or machine, nor under the CLI/Docker static cipher.
- Copying a GUI directory onto the CLI default path (or the reverse) uses a different directory and, on Windows, a different cipher.
- If the process was started with `--encryption-key` or `OCG_MANAGER_ENCRYPTION_KEY`, restoring only `.encryption-key` is not enough; supply the same explicit secret again.

## Upgrades and backups

SQLite migrations run in place when the GUI or CLI starts. Before opening a newer binary:

1. Stop every process that has the data directory open (desktop tray **Quit**, CLI Ctrl+C / service stop, `docker compose stop`). WAL files belong with `data.sqlite`.
2. Back up the **whole** data directory, including `.encryption-key` and `browser-profiles/` when present; for Docker, both `ocg-data` and `ocg-browser-profiles`. Keep the matching cipher material listed above.
3. The signed desktop updater manages its own stop and restart; CLI and Docker upgrades stay manual.

Downgrades are not supported: never point an older binary at a migrated database. To roll back, restore the whole-directory backup made before the upgrade.

## Schema v27 and the pre-v3 snapshot

`CURRENT_SCHEMA_VERSION = 34` (`crates/ocg-core/src/db.rs`). Opening a historical database first migrates canonically to v26, then the v27 rewrite copies the primary Key and every `sub_gateway_keys` row into one `access_keys` table (live primary id `00000000-0000-0000-0000-000000000001`), drops `sub_gateway_keys`, and drops the five legacy `accounts.usage_sync_*` columns (usage-sync metadata lives in `provider_usage_sync_state`). v33 adds the exact Custom upstream model identity; v34 adds the singleton CPA configuration table without importing or exporting CPA state. Account `key_cipher` / `password_cipher` bytes are validated with the Host cipher and never re-encrypted.

## Schema v31 — per-model/per-protocol overrides

v31 creates the `provider_contract_model_protocol_overrides` table. It stores one row per contract scope × model × protocol, with `state` ∈ `force_on` / `force_off`; an absent row means "auto". The composite primary key is `(scope_kind, scope_id, model_id, protocol)`. The `provider_contract_scopes` switch columns remain in the database for backward compatibility but are no longer read by effective contract derivation.

## Schema v32 — single-protocol Custom Endpoint

v32 replaces `account_custom_configs.base_url`, JSON `upstream_protocols`, and `auth_scheme` with `endpoint_url` and one `upstream_protocol`. Historical rows choose Chat Completions, then Responses, then Messages, append that protocol's standard inference suffix, and are disabled with verification reset to `pending`. Capabilities, evidence, and overrides for non-selected protocols are removed in the same transaction. Administrators must review and explicitly re-enable migrated Custom accounts.

## Schema v33 — Custom upstream model identity

v33 adds the non-null `account_model_capabilities.upstream_model` column.
Existing rows are backfilled from `model_id`, preserving their former
public-name = upstream-ID behavior exactly. New Custom mappings may retain a
distinct public model name and exact upstream model ID; no suffix normalization
or generated Alias is applied by this migration.

Before any v27 write, an existing (non-empty) library gets a unique, never-overwritten sibling snapshot:

```text
data.sqlite.pre-v3.<timestamp>.bak
data.sqlite.pre-v3.<timestamp>.bak.sha256
```

The snapshot is a standalone v26 SQLite file (`VACUUM INTO`, `quick_check` on both sides); the sidecar's first field is the lowercase SHA-256 of the `.bak`. A brand-new empty directory creates the current schema directly and does not write this copy. The snapshot is a rollback point, not a substitute for the whole-directory backup. Verify the sidecar from the data directory before any restore:

```bash
sha256sum -c data.sqlite.pre-v3.<timestamp>.bak.sha256      # Linux
shasum -a 256 -c data.sqlite.pre-v3.<timestamp>.bak.sha256  # macOS
```

On Windows, compare `Get-FileHash -Algorithm SHA256` with the first field of the sidecar. A hash mismatch means do not restore that file.

## Rollback and failed opens

**There is no down-migration.** Rollback is an offline, exact-file restore:

1. Stop every process that has the directory open.
2. Verify the sidecar hash as above; stop if it does not match.
3. Copy the verified `.bak` over `data.sqlite`, and remove the stale `data.sqlite-wal` / `data.sqlite-shm` left behind by the previous live file.
4. Start a v26-capable binary with the same cipher identity, or retry the v27 upgrade on that restored v26 file. Restoring after a successful v27 open discards every write made since the snapshot.

A failed v27 transaction rolls back: the live file must remain schema 26 with `sub_gateway_keys` intact. Leave any pre-v3 files in place; a later successful open creates another unique name instead of overwriting. A wrong or missing Host cipher fails closed; never rewrite `key_cipher` / `password_cipher`. `ocg-manager-cli status` opens the database and will attempt v27; it is not a read-only schema inspector.

---
[Maintainer guide index](../MAINTAINER.md) · [简体中文](storage-migration.zh-CN.md) · [Docs index](../README.md)
