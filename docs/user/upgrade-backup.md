[简体中文](upgrade-backup.zh-CN.md)

# Upgrade, Backup, Restore, And Uninstall

Download upgrades from the
[latest GitHub Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest)
and verify them against the release's `SHA256SUMS`:
`Get-FileHash <file> -Algorithm SHA256` on PowerShell, `shasum -a 256 <file>`
on macOS, or `sha256sum <file>` on Linux. Backups, restores, and removal are
the kind of operations that are boring right up until they aren't.

## Database Migration And Access Keys (Schema v33)

The database schema is **v35**; historical databases migrate in place on
startup. Upgrading from a single-key version keeps your existing credential
as the **primary key** (fixed id
`00000000-0000-0000-0000-000000000001`), so clients keep authenticating
with the same value. The `access_keys` table holds the primary key plus up
to 64 non-deleted sub keys; deleting a sub key clears its plaintext but
keeps the name for log attribution.

An existing, non-empty database migrates canonically to v26 first. The v27
rewrite copies the primary Key and every `sub_gateway_keys` row into
`access_keys`, drops `sub_gateway_keys`, and drops the legacy
`accounts.usage_sync_*` columns. Before any v27 write the database receives a
sibling snapshot `data.sqlite.pre-v3.<timestamp>.bak` plus a SHA-256 sidecar.
A fresh empty data directory creates schema v35 directly and skips the
snapshot. That snapshot is a v26 rollback point, not a substitute for a
complete backup; verify the sidecar before restoring it, and restore it only
onto a v26-capable binary or to retry a v27 open that never committed. Never
open a migrated database with an older build — extra Keys do not authenticate
on a single-key-era build, and a revoked value cannot come back to life by
downgrading.

v29 removes SCNet Token Plans from the catalog and deletes any existing
SCNet account rows during migration. Every startup normalizes historical
Command Code GOAT verification state to `not_required`, because the public
catalog is not Key verification. Custom API enabled state is preserved.
OpenCode Go, Zen Free, and unknown provider identities are left alone.

v30 expands Custom API `account_custom_configs` from the single
`upstream_protocol` column to a JSON `upstream_protocols` set, backfilling
each existing Custom account from its old value. Custom config/capability
edits keep the account enabled but reset `verification_status` to `pending`.

v31 adds `provider_contract_model_protocol_overrides` for per-model/per-protocol
enablement and stops reading the deprecated `provider_contract_scopes` switch
columns.

v32 replaces the Custom API base URL, protocol set, and configurable auth with
one complete inference Endpoint and one upstream protocol. Historical Custom
rows choose Chat Completions, then Responses, then Messages; the corresponding
standard inference suffix is appended, the account is disabled/pending for
administrator review, and non-selected protocol state is removed atomically.

v33 adds `account_model_capabilities.upstream_model`. Existing mapping rows are
backfilled with their prior public `model_id`, so an upgrade preserves existing
routing exactly. New Custom rows may use distinct public and upstream names.

v35 collapses Provider/Plan identity to `provider_id` only after a fail-closed
preflight of known v34 pairs, and stores typed user-defined Providers in
`dynamic_providers` / `dynamic_provider_models`. Non-empty v34 libraries also
write `data.sqlite.pre-v35.<timestamp>.bak`. Node backups export payload V4
with `providerId` only.

## Backup

1. Stop every process using the data: choose **Quit** from the desktop tray,
   stop the CLI with Ctrl+C or its service manager, or run
   `docker compose stop`.
2. Copy the **entire** GUI or CLI data directory. Desktop
   `browser-profiles/` is already inside the GUI data directory. For Docker,
   back up both sensitive volumes: `ocg-data` and `ocg-browser-profiles`.
   With the containers stopped, run
   `docker compose cp ocg-manager:/data/. ../ocg-data-backup` and
   `docker compose cp ocg-manager:/browser-profiles/. ../ocg-browser-profiles-backup`.
3. Keep the backup outside the repository, and check that it contains
   `data.sqlite` and, where present, `.encryption-key`. Browser profiles hold
   long-lived cookies and login state and are not encrypted by OCG Manager;
   protect them like account keys and the database.

## Restore

1. Stop the process, move the current data aside, and copy the whole backup
   back to its original directory or an empty Docker volume.
2. Start the same or a newer version.

Caveats:

- Docker files in `/data` must remain writable by UID/GID `10001`.
- Docker files in `/browser-profiles` must also remain writable by UID/GID
  `10001`.
- Windows GUI obfuscation is bound to the Windows user and machine, so its
  data cannot restore account keys or passwords on another machine — create
  fresh data there and re-enter the credentials.
- macOS/Linux GUI, CLI, and Docker restores must preserve `.encryption-key`
  or the explicitly supplied `--encryption-key` /
  `OCG_MANAGER_ENCRYPTION_KEY` value.
- There is no automatic downgrade compatibility guarantee; do not open a
  newer database with an older build.

## Docker Restore Into A Fresh Volume

Verify the backup first and make sure `.env` pins the intended same or
newer image. The `docker compose down -v` command below permanently deletes
all current named volumes; run it only after both persistent data sets are
safely elsewhere:

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

If the original deployment used `OCG_MANAGER_ENCRYPTION_KEY`, put the same
secret back into `.env` before the restore. Keep the backup until the
dashboard, accounts, and a real gateway request have all been verified.

## Upgrade And Uninstall By Surface

The direct GUI steps also work when in-app update is unavailable.

- **Windows GUI:** quit the tray app, run the new installer, and choose
  **Install without uninstalling**. Uninstall from Windows **Installed
  apps**; the uninstaller asks whether to delete `%USERPROFILE%\.ocg-mgr`.
- **macOS GUI:** replace the app in **Applications** with the new DMG copy.
  Delete the app to uninstall; remove `~/.ocg-mgr` separately only when you
  also intend to delete the data.
- **Linux GUI:** install the new `.deb` over the old package, or replace the
  AppImage. Remove the package or AppImage to uninstall; data remains in
  `~/.ocg-mgr` until you delete it.
- **CLI:** replace the extracted package as a unit so the executable,
  `dist/`, and `LICENSE` stay together. Delete that package to uninstall;
  data remains in `~/.ocg-mgr-cli` or the custom `--data-dir`.
- **Docker:** after backing up, run `docker compose pull` followed by
  `docker compose up -d --no-build`. If the browser profile is enabled, use
  `docker compose --profile browser pull` followed by
  `docker compose --profile browser up -d --no-build` so both images are
  upgraded together. Pin `OCG_IMAGE` and `OCG_BROWSER_IMAGE` to full release tags
  for repeatable production deployments. `docker compose down` removes
  containers but keeps `ocg-data` and `ocg-browser-profiles`;
  `docker compose down -v` permanently deletes them and is only for an
  intentional reset after a verified two-volume backup. Selecting an older
  image does not roll back the database; restore
  the complete backup made by that older version when a database rollback is
  required.

---

[User guide index](../USER.md) · [简体中文](upgrade-backup.zh-CN.md) · [Docs index](../README.md)
