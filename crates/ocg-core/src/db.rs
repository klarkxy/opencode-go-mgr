use crate::models::*;
use crate::pricing::{PricingLimits, PricingSnapshot};
use anyhow::Result;
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use rusqlite::{
    Connection, OptionalExtension, Row, params, params_from_iter,
    types::{Type, Value},
};
use std::{collections::HashSet, fmt, path::PathBuf};

pub struct Database {
    conn: Connection,
}

pub struct ForwardLogQueryOptions<'a> {
    pub limit: i64,
    pub offset: i64,
    pub status: Option<&'a str>,
    pub account_id: Option<&'a str>,
    pub model: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub start_time: Option<&'a str>,
    pub end_time: Option<&'a str>,
    pub sort_by: Option<&'a str>,
    pub sort_order: Option<&'a str>,
}

pub struct ForwardLogDiagnosticUpdate<'a> {
    pub error_source: &'a str,
    pub error_stage: &'a str,
    pub duration_ms: i64,
    pub diagnostic_json: &'a str,
}

#[derive(Debug)]
pub enum ReorderAccountsError {
    DuplicateAccountId,
    AccountSetMismatch,
    Database(rusqlite::Error),
}

impl fmt::Display for ReorderAccountsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateAccountId => f.write_str("account_ids must not contain duplicates"),
            Self::AccountSetMismatch => {
                f.write_str("account set changed; reload the account list and retry")
            }
            Self::Database(error) => write!(f, "failed to reorder accounts: {error}"),
        }
    }
}

impl std::error::Error for ReorderAccountsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::DuplicateAccountId | Self::AccountSetMismatch => None,
        }
    }
}

impl From<rusqlite::Error> for ReorderAccountsError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

/// 幂等地为指定表添加列。若列已存在则跳过，避免 v1.4.2 -> v1.5.0 升级时
/// 旧 v9 migration（HEAD 固定窗口）和 upstream v9（cost_state）冲突导致的
/// "duplicate column" 错误。
fn ensure_column(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let exists = {
        let mut stmt = tx.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
        columns
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|existing| existing == column)
    };
    if !exists {
        tx.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn migrate_legacy_usage_baselines(
    tx: &rusqlite::Transaction<'_>,
    limits: &PricingLimits,
    now: DateTime<Utc>,
) -> Result<()> {
    type LegacyUsageRow = (
        String,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        String,
    );

    let accounts = {
        let mut stmt = tx.prepare(
            "SELECT id,
                    usage_5h_baseline_percent, usage_5h_anchor_success_cost,
                    usage_week_baseline_percent, usage_week_anchor_success_cost,
                    usage_month_baseline_percent, usage_month_anchor_success_cost,
                    recharge_date
             FROM accounts
             WHERE usage_5h_baseline_percent IS NOT NULL
                OR usage_week_baseline_percent IS NOT NULL
                OR usage_month_baseline_percent IS NOT NULL",
        )?;
        stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<LegacyUsageRow>>>()?
    };
    let now_string = now.to_rfc3339();

    for (
        id,
        percent_5h,
        anchor_5h,
        percent_week,
        anchor_week,
        percent_month,
        anchor_month,
        purchase_date,
    ) in accounts
    {
        let total_cost: f64 = tx.query_row(
            "SELECT COALESCE(SUM(cost), 0) FROM forward_logs
             WHERE account_id = ?1
               AND cost_state IN ('priced', 'legacy_estimate')",
            [&id],
            |row| row.get(0),
        )?;
        let migrated_5h = percent_5h
            .zip(anchor_5h)
            .map(|baseline| effective_usage(0.0, Some(baseline), total_cost, limits.window_5h));
        let migrated_week = percent_week
            .zip(anchor_week)
            .map(|baseline| effective_usage(0.0, Some(baseline), total_cost, limits.window_week));
        let migrated_month = match percent_month.zip(anchor_month) {
            Some(baseline) => {
                let month_start = month_window_start_utc(&purchase_date)?.to_rfc3339();
                let actual_month_cost: f64 = tx.query_row(
                    "SELECT COALESCE(SUM(cost), 0) FROM forward_logs
                     WHERE account_id = ?1
                       AND cost_state IN ('priced', 'legacy_estimate')
                       AND timestamp >= ?2",
                    params![&id, month_start],
                    |row| row.get(0),
                )?;
                Some(
                    effective_usage(0.0, Some(baseline), total_cost, limits.window_month)
                        - actual_month_cost,
                )
            }
            None => None,
        };

        tx.execute(
            "UPDATE accounts SET
                usage_5h_window_started_at = CASE WHEN ?2 IS NULL THEN usage_5h_window_started_at ELSE ?1 END,
                usage_5h_window_cost_offset = COALESCE(?2, usage_5h_window_cost_offset),
                usage_week_window_started_at = CASE WHEN ?3 IS NULL THEN usage_week_window_started_at ELSE ?1 END,
                usage_week_window_cost_offset = COALESCE(?3, usage_week_window_cost_offset),
                usage_month_window_cost_offset = COALESCE(?4, usage_month_window_cost_offset),
                usage_5h_baseline_percent = NULL,
                usage_5h_anchor_success_cost = NULL,
                usage_week_baseline_percent = NULL,
                usage_week_anchor_success_cost = NULL,
                usage_month_baseline_percent = NULL,
                usage_month_anchor_success_cost = NULL
             WHERE id = ?5",
            params![
                &now_string,
                migrated_5h,
                migrated_week,
                migrated_month,
                &id
            ],
        )?;
    }
    Ok(())
}

impl Database {
    pub fn open(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("data.sqlite");
        let conn = Connection::open(db_path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY
            )",
            [],
        )?;

        let tx = self.conn.unchecked_transaction()?;
        let mut version: i32 = tx
            .query_row(
                "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // 修复：v1.4.2 -> v1.5.0 升级时，旧 v9 migration（HEAD 固定窗口）只添加了
        // usage_*_window_* 列，没有添加 upstream v9 的 cost_state 等 forward_logs 列。
        // 检测 cost_state 列是否存在，不存在则把 version 回退到 8，让 v9/v10/v11
        // 重跑（v9/v11 已改成幂等，不会因列已存在而报错）。
        let has_cost_state = {
            let mut stmt = tx.prepare("PRAGMA table_info(forward_logs)")?;
            let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
            columns
                .collect::<rusqlite::Result<Vec<_>>>()?
                .iter()
                .any(|existing| existing == "cost_state")
        };
        if !has_cost_state && version >= 9 {
            version = 8;
        }

        if version < 1 {
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS accounts (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    key_cipher TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    referral_code TEXT,
                    recharge_date TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS gateway_logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    level TEXT NOT NULL,
                    category TEXT NOT NULL,
                    message TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS forward_logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp TEXT NOT NULL,
                    model TEXT NOT NULL,
                    account_id TEXT NOT NULL,
                    account_name TEXT NOT NULL,
                    status TEXT NOT NULL,
                    http_status INTEGER,
                    prompt_tokens INTEGER NOT NULL DEFAULT 0,
                    completion_tokens INTEGER NOT NULL DEFAULT 0,
                    cached_tokens INTEGER NOT NULL DEFAULT 0,
                    cost REAL NOT NULL DEFAULT 0,
                    error_message TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_forward_logs_time ON forward_logs(timestamp);
                CREATE INDEX IF NOT EXISTS idx_forward_logs_account ON forward_logs(account_id);
                INSERT OR REPLACE INTO schema_version (version) VALUES (1);
            ",
            )?;
        }

        if version < 2 {
            // v2: per-account rate-limit cooldown (parsed from upstream 429 body).
            // Two nullable columns; no new table — account count is tiny, avoids a JOIN.
            tx.execute_batch(
                "ALTER TABLE accounts ADD COLUMN cooldown_until TEXT;
                ALTER TABLE accounts ADD COLUMN last_error TEXT;
                INSERT OR REPLACE INTO schema_version (version) VALUES (2);",
            )?;
        }

        if version < 3 {
            tx.execute_batch(
                "ALTER TABLE accounts ADD COLUMN username TEXT;
                ALTER TABLE accounts ADD COLUMN password_cipher TEXT;
                INSERT OR REPLACE INTO schema_version (version) VALUES (3);",
            )?;
        }

        if version < 4 {
            tx.execute_batch(
                "ALTER TABLE accounts ADD COLUMN usage_5h_baseline_percent REAL CHECK (usage_5h_baseline_percent BETWEEN 0 AND 100);
                ALTER TABLE accounts ADD COLUMN usage_5h_anchor_success_cost REAL CHECK (usage_5h_anchor_success_cost >= 0);
                ALTER TABLE accounts ADD COLUMN usage_week_baseline_percent REAL CHECK (usage_week_baseline_percent BETWEEN 0 AND 100);
                ALTER TABLE accounts ADD COLUMN usage_week_anchor_success_cost REAL CHECK (usage_week_anchor_success_cost >= 0);
                ALTER TABLE accounts ADD COLUMN usage_month_baseline_percent REAL CHECK (usage_month_baseline_percent BETWEEN 0 AND 100);
                ALTER TABLE accounts ADD COLUMN usage_month_anchor_success_cost REAL CHECK (usage_month_anchor_success_cost >= 0);
                INSERT OR REPLACE INTO schema_version (version) VALUES (4);",
            )?;
        }

        if version < 5 {
            tx.execute(
                "ALTER TABLE accounts ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
                [],
            )?;

            let accounts = {
                let mut stmt = tx.prepare(
                    "SELECT id, recharge_date, created_at
                     FROM accounts
                     ORDER BY created_at ASC, id ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };

            for (sort_order, (id, recharge_date, created_at)) in accounts.into_iter().enumerate() {
                let purchase_date = match recharge_date {
                    Some(value) if normalize_purchase_date(&value).is_ok() => value,
                    _ => migration_fallback_purchase_date(&created_at)?,
                };
                tx.execute(
                    "UPDATE accounts
                     SET recharge_date = ?1, sort_order = ?2
                     WHERE id = ?3",
                    params![purchase_date, sort_order as i64, id],
                )?;
            }

            tx.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (5)",
                [],
            )?;
        }

        if version < 6 {
            tx.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_forward_logs_model ON forward_logs(model);
                CREATE INDEX IF NOT EXISTS idx_forward_logs_status ON forward_logs(status);
                INSERT OR REPLACE INTO schema_version (version) VALUES (6)",
            )?;
        }

        if version < 7 {
            for column in [
                "cooldown_generic_until",
                "cooldown_5h_until",
                "cooldown_week_until",
                "cooldown_month_until",
            ] {
                ensure_column(&tx, "accounts", column, "TEXT")?;
            }
            tx.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_forward_logs_model ON forward_logs(model);
                CREATE INDEX IF NOT EXISTS idx_forward_logs_status ON forward_logs(status);
                CREATE INDEX IF NOT EXISTS idx_forward_logs_time_instant
                    ON forward_logs(julianday(timestamp));
                UPDATE accounts
                SET cooldown_generic_until = COALESCE(cooldown_generic_until, CASE
                        WHEN lower(COALESCE(last_error, '')) LIKE '%5-hour usage limit%'
                          OR lower(COALESCE(last_error, '')) LIKE '%5 hour usage limit%'
                          OR lower(COALESCE(last_error, '')) LIKE '%weekly usage limit%'
                          OR lower(COALESCE(last_error, '')) LIKE '%monthly usage limit%'
                        THEN NULL ELSE cooldown_until END),
                    cooldown_5h_until = COALESCE(cooldown_5h_until, CASE
                        WHEN lower(COALESCE(last_error, '')) LIKE '%5-hour usage limit%'
                          OR lower(COALESCE(last_error, '')) LIKE '%5 hour usage limit%'
                        THEN cooldown_until ELSE NULL END),
                    cooldown_week_until = COALESCE(cooldown_week_until, CASE
                        WHEN lower(COALESCE(last_error, '')) LIKE '%weekly usage limit%'
                        THEN cooldown_until ELSE NULL END),
                    cooldown_month_until = COALESCE(cooldown_month_until, CASE
                        WHEN lower(COALESCE(last_error, '')) LIKE '%monthly usage limit%'
                        THEN cooldown_until ELSE NULL END)
                WHERE cooldown_until IS NOT NULL;",
            )?;

            let account_ids = {
                let mut stmt = tx.prepare("SELECT id FROM accounts")?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            let now = Utc::now().to_rfc3339();
            for id in account_ids {
                let cooldown = Self::compute_cooldown_until(&tx, &id, &now)?;
                tx.execute(
                    "UPDATE accounts SET cooldown_until = ?2 WHERE id = ?1",
                    params![id, cooldown],
                )?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (7)",
                [],
            )?;
        }

        if version < 8 {
            // Older binaries can still write NULL or otherwise invalid purchase dates after the
            // v5 backfill has already run. Repair those rows so current account reads stay valid.
            let accounts = {
                let mut stmt = tx.prepare(
                    "SELECT id, recharge_date, created_at
                     FROM accounts",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };

            for (id, recharge_date, created_at) in accounts {
                let needs_repair = match recharge_date.as_deref() {
                    Some(value) => normalize_purchase_date(value).is_err(),
                    None => true,
                };
                if needs_repair {
                    let purchase_date = migration_fallback_purchase_date(&created_at)?;
                    tx.execute(
                        "UPDATE accounts SET recharge_date = ?1 WHERE id = ?2",
                        params![purchase_date, id],
                    )?;
                }
            }

            tx.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (8)",
                [],
            )?;
        }

        if version < 9 {
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS pricing_snapshots (
                    revision TEXT PRIMARY KEY,
                    activated_at TEXT NOT NULL,
                    document_updated_at TEXT NOT NULL,
                    source_url TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    snapshot_json TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_pricing_snapshots_activated
                    ON pricing_snapshots(activated_at DESC);",
            )?;
            ensure_column(&tx, "forward_logs", "pricing_revision_id", "TEXT")?;
            ensure_column(&tx, "forward_logs", "quota_multiplier", "REAL")?;
            ensure_column(&tx, "forward_logs", "local_adjustment_multiplier", "REAL")?;
            ensure_column(
                &tx,
                "forward_logs",
                "cache_creation_tokens",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(&tx, "forward_logs", "service_tier", "TEXT")?;
            ensure_column(
                &tx,
                "forward_logs",
                "cost_state",
                "TEXT NOT NULL DEFAULT 'not_applicable'",
            )?;
            tx.execute_batch(
                "UPDATE forward_logs SET cost_state = CASE
                    WHEN status = 'success' THEN 'legacy_estimate'
                    WHEN status = 'error' AND cost > 0 THEN 'legacy_estimate'
                    WHEN status = 'success_no_usage' THEN 'usage_missing'
                    WHEN status = 'success_unpriced' THEN 'unpriced'
                    WHEN status = 'outcome_unknown' THEN 'outcome_unknown'
                    ELSE 'not_applicable'
                END;
                INSERT OR REPLACE INTO schema_version (version) VALUES (9);",
            )?;
        }

        if version < 10 {
            // Repair databases that already ran the original v9 migration, which
            // classified charged response-conversion failures as not applicable.
            tx.execute_batch(
                "UPDATE forward_logs
                 SET cost_state = 'legacy_estimate'
                 WHERE status = 'error'
                   AND cost > 0
                   AND cost_state = 'not_applicable';
                 INSERT OR REPLACE INTO schema_version (version) VALUES (10);",
            )?;
        }

        if version < 11 {
            // v11: 用固定窗口替代滚动窗口 + baseline 机制。
            // 5h/周窗口记一条"窗口起点时间戳"和"起点用量偏移"（手动校准用）。
            // 月窗口无新列：起点 = purchase_date 00:00，终点 = purchase_expires_on(purchase_date) 00:00。
            // 旧的 6 个 baseline 列保留不读不写，避免 DROP COLUMN 迁移风险。
            ensure_column(&tx, "accounts", "usage_5h_window_started_at", "TEXT")?;
            ensure_column(
                &tx,
                "accounts",
                "usage_5h_window_cost_offset",
                "REAL NOT NULL DEFAULT 0 CHECK (usage_5h_window_cost_offset >= 0)",
            )?;
            ensure_column(&tx, "accounts", "usage_week_window_started_at", "TEXT")?;
            ensure_column(
                &tx,
                "accounts",
                "usage_week_window_cost_offset",
                "REAL NOT NULL DEFAULT 0 CHECK (usage_week_window_cost_offset >= 0)",
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (11)",
                [],
            )?;
        }

        if version < 12 {
            // v12:
            // - 重建 accounts 表去掉 usage_5h/week_window_cost_offset 的 CHECK (>= 0) 约束。
            //   SQLite 不支持 ALTER TABLE DROP CONSTRAINT，必须 rename + create + copy + drop。
            //   允许手动校准时 offset 为负数（target_cost < actual_cost 的情况），避免向左拉
            //   滑块时锁死在实际 cost 对应的百分比（Bug 1.5）。
            // - 新增 usage_month_window_cost_offset 列（无 CHECK），支持月窗口手动校准。
            let needs_rebuild: bool = {
                let sql: String = tx
                    .query_row(
                        "SELECT sql FROM sqlite_master WHERE type='table' AND name='accounts'",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or_default();
                sql.contains("usage_5h_window_cost_offset >= 0")
                    || sql.contains("usage_week_window_cost_offset >= 0")
                    || !sql.contains("usage_month_window_cost_offset")
            };
            if needs_rebuild {
                tx.execute_batch("PRAGMA foreign_keys=OFF;")?;
                tx.execute_batch("ALTER TABLE accounts RENAME TO accounts_v11_backup;")?;
                tx.execute_batch(
                    "CREATE TABLE accounts (
                        id TEXT PRIMARY KEY,
                        name TEXT NOT NULL,
                        username TEXT,
                        password_cipher TEXT,
                        key_cipher TEXT NOT NULL,
                        enabled INTEGER NOT NULL DEFAULT 1,
                        referral_code TEXT,
                        recharge_date TEXT NOT NULL,
                        cooldown_until TEXT,
                        cooldown_generic_until TEXT,
                        cooldown_5h_until TEXT,
                        cooldown_week_until TEXT,
                        cooldown_month_until TEXT,
                        last_error TEXT,
                        usage_5h_baseline_percent REAL,
                        usage_5h_anchor_success_cost REAL,
                        usage_week_baseline_percent REAL,
                        usage_week_anchor_success_cost REAL,
                        usage_month_baseline_percent REAL,
                        usage_month_anchor_success_cost REAL,
                        sort_order INTEGER NOT NULL DEFAULT 0,
                        usage_5h_window_started_at TEXT,
                        usage_5h_window_cost_offset REAL NOT NULL DEFAULT 0,
                        usage_week_window_started_at TEXT,
                        usage_week_window_cost_offset REAL NOT NULL DEFAULT 0,
                        usage_month_window_cost_offset REAL NOT NULL DEFAULT 0,
                        created_at TEXT NOT NULL,
                        updated_at TEXT NOT NULL
                    );",
                )?;
                // accounts_v11_backup 不含 usage_month_window_cost_offset 列，用字面量 0
                // 填充（NOT NULL DEFAULT 0 列拒绝显式 NULL，所以不能写 NULL）。
                tx.execute_batch(
                    "INSERT INTO accounts (
                        id, name, username, password_cipher, key_cipher, enabled, referral_code,
                        recharge_date, cooldown_until, cooldown_generic_until, cooldown_5h_until,
                        cooldown_week_until, cooldown_month_until, last_error,
                        usage_5h_baseline_percent, usage_5h_anchor_success_cost,
                        usage_week_baseline_percent, usage_week_anchor_success_cost,
                        usage_month_baseline_percent, usage_month_anchor_success_cost,
                        sort_order, usage_5h_window_started_at, usage_5h_window_cost_offset,
                        usage_week_window_started_at, usage_week_window_cost_offset,
                        usage_month_window_cost_offset, created_at, updated_at
                    )
                    SELECT
                        id, name, username, password_cipher, key_cipher, enabled, referral_code,
                        recharge_date, cooldown_until, cooldown_generic_until, cooldown_5h_until,
                        cooldown_week_until, cooldown_month_until, last_error,
                        usage_5h_baseline_percent, usage_5h_anchor_success_cost,
                        usage_week_baseline_percent, usage_week_anchor_success_cost,
                        usage_month_baseline_percent, usage_month_anchor_success_cost,
                        sort_order, usage_5h_window_started_at, usage_5h_window_cost_offset,
                        usage_week_window_started_at, usage_week_window_cost_offset,
                        0, created_at, updated_at
                    FROM accounts_v11_backup;
                    DROP TABLE accounts_v11_backup;
                    PRAGMA foreign_keys=ON;",
                )?;
            } else {
                // 已重建过的库只需补 usage_month_window_cost_offset 列。
                ensure_column(
                    &tx,
                    "accounts",
                    "usage_month_window_cost_offset",
                    "REAL NOT NULL DEFAULT 0",
                )?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (12)",
                [],
            )?;
        }

        if version < 13 {
            // v13 preserves manual calibrations from the old rolling-window
            // baseline model. Anchor fixed windows at the migration instant so
            // already-counted logs are not charged twice, then let new logs
            // accumulate normally from that point onward.
            let limits = tx
                .query_row(
                    "SELECT snapshot_json FROM pricing_snapshots
                     ORDER BY activated_at DESC LIMIT 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|json| serde_json::from_str::<PricingSnapshot>(&json))
                .transpose()?
                .map(|snapshot| snapshot.limits)
                .unwrap_or_else(|| crate::pricing::embedded_seed().limits);
            migrate_legacy_usage_baselines(&tx, &limits, Utc::now())?;
            tx.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (13)",
                [],
            )?;
        }

        if version < 14 {
            // Some early development databases (and their migration fixtures) did not
            // yet contain the optional runtime log table. Recreate its stable base shape
            // before adding diagnostic columns so upgrades remain repairable.
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS gateway_logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    level TEXT NOT NULL,
                    category TEXT NOT NULL,
                    message TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );",
            )?;
            for (table, columns) in [
                (
                    "forward_logs",
                    [
                        ("request_id", "TEXT"),
                        ("attempt", "INTEGER"),
                        ("error_source", "TEXT"),
                        ("error_stage", "TEXT"),
                        ("duration_ms", "INTEGER"),
                        ("diagnostic_json", "TEXT"),
                    ],
                ),
                (
                    "gateway_logs",
                    [
                        ("request_id", "TEXT"),
                        ("attempt", "INTEGER"),
                        ("error_source", "TEXT"),
                        ("error_stage", "TEXT"),
                        ("duration_ms", "INTEGER"),
                        ("diagnostic_json", "TEXT"),
                    ],
                ),
            ] {
                for (column, definition) in columns {
                    ensure_column(&tx, table, column, definition)?;
                }
            }
            tx.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_forward_logs_request_id
                    ON forward_logs(request_id);
                 CREATE INDEX IF NOT EXISTS idx_gateway_logs_request_id
                    ON gateway_logs(request_id);
                 INSERT OR REPLACE INTO schema_version (version) VALUES (14);",
            )?;
        }

        if version < 15 {
            // A 401 is account-specific and safe to fail over, but unlike a
            // quota cooldown it has no trustworthy reset time. Persist it in a
            // separate slot so routing can exclude the account without
            // conflating auth failure with a manual disable or rate limit.
            ensure_column(&tx, "accounts", "auth_error", "TEXT")?;
            tx.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (15)",
                [],
            )?;
        }

        // Detailed diagnostics are intentionally short-lived. Keep the base log row,
        // stable request id, source, stage, and original compact error indefinitely.
        tx.execute(
            "UPDATE forward_logs SET diagnostic_json = NULL
             WHERE diagnostic_json IS NOT NULL
               AND julianday(timestamp) < julianday('now', '-30 days')",
            [],
        )?;
        tx.execute(
            "UPDATE gateway_logs SET diagnostic_json = NULL
             WHERE diagnostic_json IS NOT NULL
               AND julianday(created_at) < julianday('now', '-30 days')",
            [],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn insert_pricing_snapshot(&self, snapshot: &PricingSnapshot) -> Result<()> {
        let snapshot_json = serde_json::to_string(snapshot)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO pricing_snapshots
             (revision, activated_at, document_updated_at, source_url, content_hash, snapshot_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                snapshot.revision,
                snapshot.activated_at,
                snapshot.document_updated_at,
                snapshot.source_url,
                snapshot.content_hash,
                snapshot_json,
            ],
        )?;
        Ok(())
    }

    pub fn latest_pricing_snapshot(&self) -> Result<Option<PricingSnapshot>> {
        let snapshot_json = self
            .conn
            .query_row(
                "SELECT snapshot_json FROM pricing_snapshots
                 ORDER BY datetime(activated_at) DESC, rowid DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        snapshot_json
            .map(|json| serde_json::from_str(&json).map_err(Into::into))
            .transpose()
    }

    // Accounts
    pub fn create_account(&self, account: &Account) -> Result<()> {
        let purchase_date = if account.purchase_date.trim().is_empty() {
            local_today()
        } else {
            normalize_purchase_date(&account.purchase_date)?
        };
        self.conn.execute(
            "INSERT INTO accounts (id, name, username, password_cipher, key_cipher, enabled, referral_code, recharge_date, sort_order, cooldown_until, cooldown_generic_until, cooldown_5h_until, cooldown_week_until, cooldown_month_until, last_error, auth_error, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM accounts), ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                account.id,
                account.name,
                account.username,
                account.password_cipher,
                account.key_cipher,
                account.enabled as i32,
                account.referral_code,
                purchase_date,
                account.cooldown_until.map(|t| t.to_rfc3339()),
                account.cooldown_generic_until.map(|t| t.to_rfc3339()),
                account.cooldown_5h_until.map(|t| t.to_rfc3339()),
                account.cooldown_week_until.map(|t| t.to_rfc3339()),
                account.cooldown_month_until.map(|t| t.to_rfc3339()),
                account.last_error,
                account.auth_error,
                account.created_at.to_rfc3339(),
                account.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn update_account(
        &self,
        id: &str,
        update: &AccountUpdate,
        key_cipher: Option<&str>,
        password_cipher: Option<&str>,
    ) -> Result<()> {
        let existing = self
            .get_account(id)?
            .ok_or_else(|| anyhow::anyhow!("account not found"))?;
        let name = update.name.as_ref().unwrap_or(&existing.name);
        let username = match &update.username {
            Some(s) if s.is_empty() => None,
            Some(s) => Some(s.clone()),
            None => existing.username.clone(),
        };
        let enabled = update.enabled.unwrap_or(existing.enabled);
        let referral_code = match &update.referral_code {
            Some(s) if s.is_empty() => None,        // explicitly cleared
            Some(s) => Some(s.clone()),             // set to new value
            None => existing.referral_code.clone(), // not provided, keep existing
        };
        let purchase_date = match &update.purchase_date {
            Some(value) => normalize_purchase_date(value)?,
            None => existing.purchase_date.clone(),
        };
        let purchase_date_changed = purchase_date != existing.purchase_date;
        let key = key_cipher.unwrap_or(&existing.key_cipher);
        let password = match password_cipher {
            Some("") => None,
            Some(s) => Some(s.to_string()),
            None => existing.password_cipher.clone(),
        };

        self.conn.execute(
            "UPDATE accounts SET name = ?1, username = ?2, password_cipher = ?3, key_cipher = ?4, enabled = ?5, referral_code = ?6, recharge_date = ?7,
             usage_month_window_cost_offset = CASE WHEN ?8 THEN 0 ELSE usage_month_window_cost_offset END,
             auth_error = CASE WHEN ?9 THEN NULL ELSE auth_error END,
             updated_at = ?10 WHERE id = ?11",
            params![
                name,
                username,
                password,
                key,
                enabled as i32,
                referral_code,
                purchase_date,
                purchase_date_changed,
                key_cipher.is_some(),
                Utc::now().to_rfc3339(),
                id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_account(&mut self, id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_account(&self, id: &str) -> Result<Option<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, username, password_cipher, key_cipher, enabled, referral_code, recharge_date, cooldown_until, cooldown_generic_until, cooldown_5h_until, cooldown_week_until, cooldown_month_until, last_error, created_at, updated_at, auth_error FROM accounts WHERE id = ?1"
        )?;
        let account = stmt.query_row([id], account_from_row).optional()?;
        Ok(account)
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, username, password_cipher, key_cipher, enabled, referral_code, recharge_date, cooldown_until, cooldown_generic_until, cooldown_5h_until, cooldown_week_until, cooldown_month_until, last_error, created_at, updated_at, auth_error FROM accounts ORDER BY sort_order ASC, created_at ASC, id ASC"
        )?;
        let rows = stmt.query_map([], account_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    pub fn reorder_accounts(
        &self,
        account_ids: &[String],
    ) -> std::result::Result<(), ReorderAccountsError> {
        let tx = self.conn.unchecked_transaction()?;
        let mut requested_ids = HashSet::with_capacity(account_ids.len());
        if account_ids
            .iter()
            .any(|id| !requested_ids.insert(id.as_str()))
        {
            return Err(ReorderAccountsError::DuplicateAccountId);
        }

        let current_ids = {
            let mut stmt = tx.prepare("SELECT id FROM accounts")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if current_ids.len() != account_ids.len()
            || current_ids
                .iter()
                .any(|id| !requested_ids.contains(id.as_str()))
        {
            return Err(ReorderAccountsError::AccountSetMismatch);
        }

        for (sort_order, id) in account_ids.iter().enumerate() {
            tx.execute(
                "UPDATE accounts SET sort_order = ?1 WHERE id = ?2",
                params![sort_order as i64, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // Settings
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            [key, value],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| e.into())
    }

    // Logging
    pub fn log_gateway(&self, level: &str, category: &str, message: &str) -> Result<()> {
        self.log_gateway_diagnostic(level, category, message, None, None, None, None, None, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn log_gateway_diagnostic(
        &self,
        level: &str,
        category: &str,
        message: &str,
        request_id: Option<&str>,
        attempt: Option<i64>,
        error_source: Option<&str>,
        error_stage: Option<&str>,
        duration_ms: Option<i64>,
        diagnostic_json: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO gateway_logs
             (level, category, message, created_at, request_id, attempt,
              error_source, error_stage, duration_ms, diagnostic_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                level,
                category,
                message,
                Utc::now().to_rfc3339(),
                request_id,
                attempt,
                error_source,
                error_stage,
                duration_ms,
                diagnostic_json,
            ],
        )?;
        Ok(())
    }

    /// Insert a forward_logs row. Returns the auto-assigned row id.
    pub fn log_forward(&self, log: &ForwardLog) -> Result<i64> {
        let diagnostic_json = log
            .diagnostic
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        self.conn.execute(
            "INSERT INTO forward_logs
             (timestamp, model, account_id, account_name, status, http_status,
              prompt_tokens, completion_tokens, cached_tokens, cache_creation_tokens, cost,
              pricing_revision_id, quota_multiplier, local_adjustment_multiplier,
              service_tier, cost_state, error_message, request_id, attempt,
              error_source, error_stage, duration_ms, diagnostic_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                     ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                log.timestamp.to_rfc3339(),
                log.model,
                log.account_id,
                log.account_name,
                log.status,
                log.http_status,
                log.prompt_tokens,
                log.completion_tokens,
                log.cached_tokens,
                log.cache_creation_tokens,
                log.cost.unwrap_or(0.0),
                log.pricing_revision_id,
                log.quota_multiplier,
                log.local_adjustment_multiplier,
                log.service_tier,
                log.cost_state,
                log.error_message,
                log.request_id,
                log.attempt,
                log.error_source,
                log.error_stage,
                log.duration_ms,
                diagnostic_json,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Finalize a forward_logs row once the upstream response ends. `http_status` and
    /// `error_message` may be `None` to leave them at their initial value. `id` is the
    /// primary key returned from the original `log_forward` insert.
    pub fn update_forward_log(
        &self,
        id: i64,
        status: &str,
        http_status: Option<i32>,
        metrics: ForwardMetrics,
        error_message: Option<&str>,
        diagnostic: Option<&ForwardLogDiagnosticUpdate<'_>>,
    ) -> Result<()> {
        let cost_state = match (metrics.cost_state, status) {
            ("not_applicable", "outcome_unknown") => "outcome_unknown",
            ("not_applicable", "success_no_usage") => "usage_missing",
            ("not_applicable", "success_unpriced") => "unpriced",
            (state, _) => state,
        };
        let stored_cost = if cost_state == "priced" {
            metrics.cost
        } else {
            0.0
        };
        self.conn.execute(
            "UPDATE forward_logs
             SET status = ?2,
                 http_status = COALESCE(?3, http_status),
                 prompt_tokens = ?4,
                 completion_tokens = ?5,
                 cached_tokens = ?6,
                 cache_creation_tokens = ?7,
                 cost = ?8,
                 pricing_revision_id = ?9,
                 quota_multiplier = ?10,
                 local_adjustment_multiplier = ?11,
                 service_tier = ?12,
                 cost_state = ?13,
                 error_message = COALESCE(?14, error_message),
                 error_source = COALESCE(?15, error_source),
                 error_stage = COALESCE(?16, error_stage),
                 duration_ms = COALESCE(?17, duration_ms),
                 diagnostic_json = COALESCE(?18, diagnostic_json)
             WHERE id = ?1",
            params![
                id,
                status,
                http_status,
                metrics.prompt_tokens,
                metrics.completion_tokens,
                metrics.cached_tokens,
                metrics.cache_creation_tokens,
                stored_cost,
                metrics.pricing_revision_id,
                metrics.quota_multiplier,
                metrics.local_adjustment_multiplier,
                metrics.service_tier,
                cost_state,
                error_message,
                diagnostic.map(|diagnostic| diagnostic.error_source),
                diagnostic.map(|diagnostic| diagnostic.error_stage),
                diagnostic.map(|diagnostic| diagnostic.duration_ms),
                diagnostic.map(|diagnostic| diagnostic.diagnostic_json),
            ],
        )?;
        Ok(())
    }

    pub fn list_gateway_logs(&self, limit: i64) -> Result<Vec<GatewayLog>> {
        self.query_gateway_logs(limit, None)
    }

    pub fn query_gateway_logs(
        &self,
        limit: i64,
        request_id: Option<&str>,
    ) -> Result<Vec<GatewayLog>> {
        let sql = if request_id.is_some() {
            "SELECT id, level, category, message, created_at, request_id, attempt,
                    error_source, error_stage, duration_ms, diagnostic_json
             FROM gateway_logs WHERE request_id = ?1 ORDER BY id DESC LIMIT ?2"
        } else {
            "SELECT id, level, category, message, created_at, request_id, attempt,
                    error_source, error_stage, duration_ms, diagnostic_json
             FROM gateway_logs ORDER BY id DESC LIMIT ?1"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let map = |row: &rusqlite::Row<'_>| {
            Ok(GatewayLog {
                id: row.get(0)?,
                level: row.get(1)?,
                category: row.get(2)?,
                message: row.get(3)?,
                created_at: parse_datetime(row.get::<_, String>(4)?),
                request_id: row.get(5)?,
                attempt: row.get(6)?,
                error_source: row.get(7)?,
                error_stage: row.get(8)?,
                duration_ms: row.get(9)?,
                diagnostic: row
                    .get::<_, Option<String>>(10)?
                    .and_then(|json| serde_json::from_str(&json).ok()),
            })
        };
        let rows = if let Some(request_id) = request_id {
            stmt.query_map(params![request_id, limit], map)?
        } else {
            stmt.query_map(params![limit], map)?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    pub fn latest_gateway_error(&self) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT message FROM gateway_logs
                 WHERE lower(level) = 'error' AND category = 'gateway'
                 ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.into())
    }

    pub fn latest_error_summary(&self) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT message FROM gateway_logs
                 WHERE lower(level) IN ('error', 'warn')
                 ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.into())
    }

    pub fn list_forward_logs(&self, limit: i64) -> Result<Vec<ForwardLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, model, account_id, account_name, status, http_status,
                    prompt_tokens, completion_tokens, cached_tokens, cache_creation_tokens, cost,
                    pricing_revision_id, quota_multiplier, local_adjustment_multiplier,
                    service_tier, cost_state, error_message, request_id, attempt,
                    error_source, error_stage, duration_ms, diagnostic_json
             FROM forward_logs ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], forward_log_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    pub fn query_forward_logs(
        &self,
        options: ForwardLogQueryOptions<'_>,
    ) -> Result<ForwardLogPage> {
        let limit = options.limit.clamp(1, 200);
        let offset = options.offset.max(0);
        let (filter, filter_params) = forward_log_filter(
            options.status,
            options.account_id,
            options.model,
            options.request_id,
            options.start_time,
            options.end_time,
        );
        let order_clause = forward_log_order(options.sort_by, options.sort_order);
        let summary_sql = format!(
            "SELECT COUNT(*),
                    COALESCE(SUM(prompt_tokens), 0),
                    COALESCE(SUM(completion_tokens), 0),
                    COALESCE(SUM(cached_tokens), 0),
                    COALESCE(SUM(cost), 0.0)
             FROM forward_logs{filter}"
        );
        let summary = self.conn.query_row(
            &summary_sql,
            params_from_iter(filter_params.iter()),
            |row| {
                Ok(ForwardLogSummary {
                    total_requests: row.get(0)?,
                    prompt_tokens: row.get(1)?,
                    completion_tokens: row.get(2)?,
                    cached_tokens: row.get(3)?,
                    cost: row.get(4)?,
                })
            },
        )?;

        let items_sql = format!(
            "SELECT id, timestamp, model, account_id, account_name, status, http_status,
                    prompt_tokens, completion_tokens, cached_tokens, cache_creation_tokens, cost,
                    pricing_revision_id, quota_multiplier, local_adjustment_multiplier,
                    service_tier, cost_state, error_message, request_id, attempt,
                    error_source, error_stage, duration_ms, diagnostic_json
             FROM forward_logs{filter}
             {order_clause}
             LIMIT ? OFFSET ?"
        );
        let mut item_params = filter_params;
        item_params.push(Value::Integer(limit));
        item_params.push(Value::Integer(offset));
        let mut stmt = self.conn.prepare(&items_sql)?;
        let items = stmt
            .query_map(params_from_iter(item_params.iter()), forward_log_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ForwardLogPage { items, summary })
    }

    pub fn list_forward_log_models(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT model FROM forward_logs ORDER BY model ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    // Cooldown
    /// Set or clear a per-account rate-limit cooldown.
    /// Pass `None` for both `until` and `err` to clear.
    pub fn set_account_cooldown(
        &self,
        id: &str,
        until: Option<DateTime<Utc>>,
        err: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let tx = self.conn.unchecked_transaction()?;
        if until.is_none() && err.is_none() {
            tx.execute(
                "UPDATE accounts
                 SET cooldown_until = NULL,
                     cooldown_generic_until = NULL,
                     cooldown_5h_until = NULL,
                     cooldown_week_until = NULL,
                     cooldown_month_until = NULL,
                     last_error = NULL,
                     updated_at = ?2
                 WHERE id = ?1",
                params![id, now],
            )?;
        } else {
            tx.execute(
                "UPDATE accounts
                 SET cooldown_generic_until = ?2, last_error = ?3, updated_at = ?4
                 WHERE id = ?1",
                params![id, until.map(|t| t.to_rfc3339()), err, now],
            )?;
            let new_cooldown = Self::compute_cooldown_until(&tx, id, &now)?;
            tx.execute(
                "UPDATE accounts SET cooldown_until = ?2 WHERE id = ?1",
                params![id, new_cooldown],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn clear_account_cooldown(&self, id: &str) -> Result<()> {
        self.set_account_cooldown(id, None, None)
    }

    /// Persist or clear an account-specific upstream 401. This state is kept
    /// separate from cooldowns because authentication failures do not carry a
    /// reset deadline and must not be reported as rate limits.
    pub fn set_account_auth_error(&self, id: &str, error: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE accounts SET auth_error = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, error, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Update auth state only when the stored credential is still the one that
    /// produced this upstream response. A late response from a replaced key
    /// must not break or recover the new credential.
    pub fn set_account_auth_error_if_key_matches(
        &self,
        id: &str,
        expected_key_cipher: &str,
        error: Option<&str>,
    ) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE accounts
             SET auth_error = ?3, updated_at = ?4
             WHERE id = ?1 AND key_cipher = ?2",
            params![id, expected_key_cipher, error, Utc::now().to_rfc3339()],
        )?;
        Ok(updated > 0)
    }

    /// Record a real upstream 429 and reset only the identified manual usage window.
    pub fn set_account_rate_limit(
        &self,
        id: &str,
        until: DateTime<Utc>,
        err: &str,
        window: Option<UsageWindowKind>,
    ) -> Result<()> {
        self.set_account_rate_limit_inner(id, None, until, err, window)?;
        Ok(())
    }

    /// Record a 429 only when the credential that produced it is still current.
    /// This prevents a delayed response from an old key from cooling down a
    /// replacement credential.
    pub fn set_account_rate_limit_if_key_matches(
        &self,
        id: &str,
        expected_key_cipher: &str,
        until: DateTime<Utc>,
        err: &str,
        window: Option<UsageWindowKind>,
    ) -> Result<bool> {
        self.set_account_rate_limit_inner(id, Some(expected_key_cipher), until, err, window)
    }

    fn set_account_rate_limit_inner(
        &self,
        id: &str,
        expected_key_cipher: Option<&str>,
        until: DateTime<Utc>,
        err: &str,
        window: Option<UsageWindowKind>,
    ) -> Result<bool> {
        let now = Utc::now();
        let now_rfc = now.to_rfc3339();
        let tx = self.conn.unchecked_transaction()?;

        // Unknown upstream rate limits need their own slot so a later known window
        // cannot overwrite a still-active generic cooldown.
        let column = match window {
            Some(UsageWindowKind::FiveHours) => "cooldown_5h_until",
            Some(UsageWindowKind::Week) => "cooldown_week_until",
            Some(UsageWindowKind::Month) => "cooldown_month_until",
            None => "cooldown_generic_until",
        };
        let updated = tx.execute(
            &format!(
                "UPDATE accounts SET {column} = ?2, last_error = ?3, updated_at = ?4
                 WHERE id = ?1 AND (?5 IS NULL OR key_cipher = ?5)"
            ),
            params![id, until.to_rfc3339(), err, now_rfc, expected_key_cipher],
        )?;
        if updated == 0 {
            return Ok(false);
        }

        // Legacy callers use cooldown_until as the time when this account is usable.
        let new_cooldown = Self::compute_cooldown_until(&tx, id, &now_rfc)?;
        tx.execute(
            "UPDATE accounts SET cooldown_until = ?2 WHERE id = ?1",
            params![id, new_cooldown],
        )?;

        // ponytail: 不再在 429 时设置 baseline。固定窗口的"重置"由 forward_logs 自然驱动；
        // 冷却到期后账号恢复可用，用量窗口照常计算。429 仅用于阻断选择器重试。
        // 旧 baseline 列保留不读不写，避免迁移风险。
        tx.commit()?;
        Ok(true)
    }

    fn compute_cooldown_until(
        tx: &rusqlite::Transaction,
        id: &str,
        now_rfc: &str,
    ) -> Result<Option<String>> {
        let max: Option<String> = tx.query_row(
            "SELECT MAX(until) FROM (
                SELECT cooldown_generic_until AS until FROM accounts WHERE id = ?1
                UNION ALL
                SELECT cooldown_5h_until FROM accounts WHERE id = ?1
                UNION ALL
                SELECT cooldown_week_until FROM accounts WHERE id = ?1
                UNION ALL
                SELECT cooldown_month_until FROM accounts WHERE id = ?1
            ) WHERE until IS NOT NULL AND until > ?2",
            params![id, now_rfc],
            |row| row.get(0),
        )?;
        Ok(max)
    }

    /// Among all enabled accounts, return the first time any account becomes usable.
    /// `None` means no account is in cooldown.
    pub fn soonest_cooldown_reset(&self) -> Result<Option<DateTime<Utc>>> {
        let now = Utc::now().to_rfc3339();
        let res: Option<String> = self
            .conn
            .query_row(
                "SELECT MIN(cooldown_until)
                 FROM accounts
                 WHERE enabled = 1
                   AND auth_error IS NULL
                   AND cooldown_until IS NOT NULL
                   AND cooldown_until > ?1",
                params![now],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(res.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }))
    }

    // Usage
    /// 手动校准一个固定窗口的"当前已用百分比"与"距上游重置还剩多久"。
    /// `percent` = 当前已用百分比（0-100），`resets_in_minutes` = 距上游重置还剩多少分钟
    /// （None 表示从 now 起算满窗口时长；月窗口忽略此参数——窗口由 purchase_date/expires_on 决定）。
    /// `limit` = 当前窗口的限额（从 PricingSnapshot 读取，避免硬编码）。
    pub fn calibrate_account_usage(
        &self,
        account_id: &str,
        window: UsageWindowKind,
        percent: f64,
        resets_in_minutes: Option<i64>,
        limit: f64,
    ) -> Result<bool> {
        let now = Utc::now();
        // (started_at, offset_col, started_col_or_empty)
        // started_col 为空字符串表示月窗口——不写 started_at 列（起点固定为 purchase_date）。
        let (started_at, started_col, offset_col): (Option<DateTime<Utc>>, &str, &str) =
            match window {
                UsageWindowKind::FiveHours => {
                    let window_len = Duration::hours(5);
                    let started_at =
                        calibrated_window_start(now, window_len, resets_in_minutes, "5-hour")?;
                    (
                        Some(started_at),
                        "usage_5h_window_started_at",
                        "usage_5h_window_cost_offset",
                    )
                }
                UsageWindowKind::Week => {
                    let window_len = Duration::days(7);
                    let started_at =
                        calibrated_window_start(now, window_len, resets_in_minutes, "weekly")?;
                    (
                        Some(started_at),
                        "usage_week_window_started_at",
                        "usage_week_window_cost_offset",
                    )
                }
                UsageWindowKind::Month => {
                    // 月窗口的起点/终点由 purchase_date 决定，不写 started_at 列。
                    // resets_in_minutes 被忽略——窗口已由账号购买日期固定。
                    (None, "", "usage_month_window_cost_offset")
                }
            };

        // 计算 actual_cost：窗口内已有 forward_logs 的 cost 总和。
        // 5h/周窗口的起点是刚算出的 started_at；月窗口的起点是 purchase_date 00:00 本地时区。
        let actual_cost: f64 = match started_at {
            Some(started) => self.conn.query_row(
                "SELECT COALESCE(SUM(cost), 0) FROM forward_logs
                 WHERE account_id = ?1
                   AND cost_state IN ('priced', 'legacy_estimate')
                   AND timestamp >= ?2",
                params![account_id, started.to_rfc3339()],
                |row| row.get(0),
            )?,
            None => {
                let purchase_date: String = self
                    .conn
                    .query_row(
                        "SELECT recharge_date FROM accounts WHERE id = ?1",
                        [account_id],
                        |row| row.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| anyhow::anyhow!("account not found"))?;
                let started = month_window_start_utc(&purchase_date)?;
                self.conn.query_row(
                    "SELECT COALESCE(SUM(cost), 0) FROM forward_logs
                     WHERE account_id = ?1
                       AND cost_state IN ('priced', 'legacy_estimate')
                       AND timestamp >= ?2",
                    params![account_id, started.to_rfc3339()],
                    |row| row.get(0),
                )?
            }
        };

        let target_cost = limit * percent / 100.0;
        // Bug 1.5 修复：去掉 max(0, ...) 钳制，允许负 offset。
        // 之前 max(0, target - actual) 配合 schema CHECK (offset >= 0) 让向左拉
        // 滑块时被锁死在实际 cost 对应的百分比。现在 offset 可以为负，
        // compute_fixed_window 返回 offset + actual = target_cost，与用户输入一致。
        let offset = target_cost - actual_cost;

        let changed = if started_col.is_empty() {
            // 月窗口：只更新 cost_offset（started_at 由 purchase_date 派生，不存储）
            self.conn.execute(
                "UPDATE accounts
                 SET usage_month_window_cost_offset = ?2,
                     updated_at = ?3
                 WHERE id = ?1",
                params![account_id, offset, now.to_rfc3339()],
            )?
        } else {
            let started = started_at.unwrap();
            self.conn.execute(
                &format!(
                    "UPDATE accounts
                     SET {started_col} = ?2,
                         {offset_col} = ?3,
                         updated_at = ?4
                     WHERE id = ?1"
                ),
                params![account_id, started.to_rfc3339(), offset, now.to_rfc3339()],
            )?
        };
        Ok(changed > 0)
    }

    pub fn account_usage(&self, account_id: &str) -> Result<UsageWindow> {
        let limits = self
            .latest_pricing_snapshot()?
            .unwrap_or_else(crate::pricing::embedded_seed)
            .limits;
        self.account_usage_with_limits(account_id, &limits)
    }

    pub fn account_usage_with_limits(
        &self,
        account_id: &str,
        limits: &PricingLimits,
    ) -> Result<UsageWindow> {
        let now = Utc::now();
        let row = self.conn.query_row(
            "SELECT usage_5h_window_started_at, usage_5h_window_cost_offset,
                    usage_week_window_started_at, usage_week_window_cost_offset,
                    usage_month_window_cost_offset,
                    recharge_date
             FROM accounts WHERE id = ?1",
            [account_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        );
        let (started_5h_str, offset_5h, started_week_str, offset_week, offset_month, purchase_date) =
            match row.optional()? {
                Some(v) => v,
                None => {
                    return Ok(UsageWindow {
                        account_id: account_id.to_string(),
                        window_5h: 0.0,
                        window_week: 0.0,
                        window_month: 0.0,
                        resets_in_5h: None,
                        resets_in_week: None,
                        resets_in_month: None,
                    });
                }
            };

        let (cost_5h, reset_5h) = compute_fixed_window(
            &self.conn,
            account_id,
            started_5h_str.as_deref(),
            offset_5h,
            limits.window_5h,
            now,
            FixedWindowSpec {
                length: Duration::hours(5),
                started_col: "usage_5h_window_started_at",
                offset_col: "usage_5h_window_cost_offset",
            },
        )?;
        let (cost_week, reset_week) = compute_fixed_window(
            &self.conn,
            account_id,
            started_week_str.as_deref(),
            offset_week,
            limits.window_week,
            now,
            FixedWindowSpec {
                length: Duration::days(7),
                started_col: "usage_week_window_started_at",
                offset_col: "usage_week_window_cost_offset",
            },
        )?;
        let (cost_month, reset_month) = compute_month_window(
            &self.conn,
            account_id,
            &purchase_date,
            offset_month,
            limits.window_month,
        )?;

        Ok(UsageWindow {
            account_id: account_id.to_string(),
            window_5h: cost_5h,
            window_week: cost_week,
            window_month: cost_month,
            resets_in_5h: reset_5h,
            resets_in_week: reset_week,
            resets_in_month: reset_month,
        })
    }

    pub fn total_usage(&self) -> Result<(f64, f64, f64)> {
        let now = Utc::now();
        let today_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339();
        let week_ago = (now - Duration::days(7)).to_rfc3339();
        let month_ago = (now - Duration::days(30)).to_rfc3339();

        let today: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE cost_state IN ('priced', 'legacy_estimate') AND timestamp > ?1",
            [&today_start],
            |row| row.get(0),
        )?;
        let week: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE cost_state IN ('priced', 'legacy_estimate') AND timestamp > ?1",
            [&week_ago],
            |row| row.get(0),
        )?;
        let month: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE cost_state IN ('priced', 'legacy_estimate') AND timestamp > ?1",
            [&month_ago],
            |row| row.get(0),
        )?;

        Ok((today, week, month))
    }

    /// Aggregate `forward_logs` into per-day, per-model cost buckets covering
    /// the last `days` calendar days (UTC). Rows with zero activity on a given
    /// day are omitted — the frontend synthesizes empty days so the x-axis
    /// never collapses. Priced rows and preserved legacy estimates count,
    /// including an upstream success whose local response conversion failed.
    pub fn daily_cost_by_model(&self, days: i64) -> Result<Vec<DailyModelCost>> {
        // Bone-simple SQLite date math: store timestamps as RFC3339 strings,
        // so group by `substr(timestamp, 1, 10)` to collapse to YYYY-MM-DD.
        // UTC-only is fine — the gateway runs local and the dashboard is a
        // single-user tool; a TZ-correct grouping would need a calendar table
        // or a strftime('%Y-%m-%d', ...) with proper epoch arg, which is more
        // machinery than this needs right now.
        let since = (Utc::now() - Duration::days(days - 1)).to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT substr(timestamp, 1, 10) AS day, model, COALESCE(SUM(cost), 0)
             FROM forward_logs
             WHERE cost_state IN ('priced', 'legacy_estimate') AND timestamp > ?1
             GROUP BY day, model
             ORDER BY day ASC, model ASC",
        )?;
        let rows = stmt.query_map([&since], |row| {
            Ok(DailyModelCost {
                date: row.get::<_, String>(0)?,
                model: row.get::<_, String>(1)?,
                cost: row.get::<_, f64>(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }
}

/// 计算固定窗口的当前用量与清零时刻。`started_at_str` 为 `None` 表示账号从未使用过该窗口；
/// 窗口已过期时从 `forward_logs` lazy 重建新起点。
struct FixedWindowSpec {
    length: Duration,
    started_col: &'static str,
    offset_col: &'static str,
}

fn compute_fixed_window(
    conn: &Connection,
    account_id: &str,
    started_at_str: Option<&str>,
    offset: f64,
    limit: f64,
    now: DateTime<Utc>,
    spec: FixedWindowSpec,
) -> Result<(f64, Option<DateTime<Utc>>)> {
    let mut started_at = match started_at_str {
        None => {
            // ponytail: lazy 初始化——查 forward_logs 第一条计费请求作为窗口起点。
            // 计费行 = cost_state IN ('priced', 'legacy_estimate')，
            // 与下方 SUM(cost) 的过滤保持一致，确保迁移后的 legacy error 也能触发窗口。
            let first: Option<String> = conn
                .query_row(
                    "SELECT MIN(timestamp) FROM forward_logs
                     WHERE account_id = ?1
                       AND cost_state IN ('priced', 'legacy_estimate')",
                    [account_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            match first {
                None => return Ok((0.0, None)), // 真的没用过
                Some(s) => {
                    conn.execute(
                        &format!(
                            "UPDATE accounts SET {} = ?2, {} = 0
                             WHERE id = ?1",
                            spec.started_col, spec.offset_col
                        ),
                        params![account_id, &s],
                    )?;
                    parse_rfc3339(&s)?
                }
            }
        }
        Some(s) => parse_rfc3339(s)?,
    };
    // 第一次进入循环时使用调用方传入的 offset（来自手动校准）；任何一次前进后，
    // offset 都被清零（`offset_col = 0` 已写入 DB），用 effective_offset 跟踪。
    let mut effective_offset = offset;

    loop {
        let ends_at = started_at + spec.length;
        if now < ends_at {
            // 窗口仍有效：用量 = effective_offset + SUM(cost WHERE ts >= started_at)
            let cost: f64 = conn.query_row(
                "SELECT COALESCE(SUM(cost), 0) FROM forward_logs
                 WHERE account_id = ?1
                   AND cost_state IN ('priced', 'legacy_estimate')
                   AND timestamp >= ?2",
                params![account_id, started_at.to_rfc3339()],
                |row| row.get(0),
            )?;
            return Ok(((effective_offset + cost).min(limit), Some(ends_at)));
        }

        // 窗口已过期：找 forward_logs 中第一条 timestamp >= ends_at 的计费请求作为新起点。
        // 关键修复：旧实现只前进一次就 return，遇到多条稀疏日志（间隔 > 5h）时每次刷新
        // 只前进一个窗口，造成前端可见的"用量从 60 → 30 → 13 → 5.8 → 0"递减幻觉；
        // 当 next=None 清空后下次刷新又 lazy-init 回最旧日志，循环重启。
        // 用 loop 在一次调用内连过所有过期窗口，直到落在有效窗口或彻底无新请求。
        let next: Option<String> = conn
            .query_row(
                "SELECT MIN(timestamp) FROM forward_logs
                 WHERE account_id = ?1
                   AND cost_state IN ('priced', 'legacy_estimate')
                   AND timestamp >= ?2",
                params![account_id, ends_at.to_rfc3339()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        match next {
            None => {
                // 过期后无新请求：清空窗口，等待下次请求触发新窗口。
                conn.execute(
                    &format!(
                        "UPDATE accounts SET {} = NULL, {} = 0
                         WHERE id = ?1",
                        spec.started_col, spec.offset_col
                    ),
                    [account_id],
                )?;
                return Ok((0.0, None));
            }
            Some(s) => {
                started_at = parse_rfc3339(&s)?;
                effective_offset = 0.0;
                conn.execute(
                    &format!(
                        "UPDATE accounts SET {} = ?2, {} = 0
                         WHERE id = ?1",
                        spec.started_col, spec.offset_col
                    ),
                    params![account_id, &s],
                )?;
                // 继续循环：新起点对应的窗口可能也已过期，需要再判一次。
            }
        }
    }
}

/// 月窗口：从 `purchase_date 00:00 本地时区` 累计到 `purchase_expires_on(purchase_date) 00:00 本地时区`，不重置。
/// `offset` = 手动校准时写入的 `usage_month_window_cost_offset`，与 `compute_fixed_window` 对齐：
/// 返回值 = `(offset + cost).min(limit)`，让月窗口支持手动校准。
fn compute_month_window(
    conn: &Connection,
    account_id: &str,
    purchase_date: &str,
    offset: f64,
    limit: f64,
) -> Result<(f64, Option<DateTime<Utc>>)> {
    if purchase_date.trim().is_empty() {
        return Ok((0.0, None));
    }
    let start = month_window_start_utc(purchase_date)?;
    let expires = purchase_expires_on(purchase_date)?;
    let end_naive = NaiveDate::parse_from_str(&expires, "%Y-%m-%d")?
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end: DateTime<Utc> = Local
        .from_local_datetime(&end_naive)
        .single()
        .ok_or_else(|| anyhow::anyhow!("ambiguous local datetime for expires_on"))?
        .with_timezone(&Utc);
    let cost: f64 = conn.query_row(
        "SELECT COALESCE(SUM(cost), 0) FROM forward_logs
         WHERE account_id = ?1
           AND cost_state IN ('priced', 'legacy_estimate')
           AND timestamp >= ?2",
        params![account_id, start.to_rfc3339()],
        |row| row.get(0),
    )?;
    // ponytail: 月窗口已过期也照常返回终点，前端按"已到期"显示。
    Ok(((offset + cost).min(limit), Some(end)))
}

fn calibrated_window_start(
    now: DateTime<Utc>,
    window_len: Duration,
    resets_in_minutes: Option<i64>,
    window_name: &str,
) -> Result<DateTime<Utc>> {
    let max_minutes = window_len.num_minutes();
    let remaining_minutes = resets_in_minutes.unwrap_or(max_minutes);
    if !(0..=max_minutes).contains(&remaining_minutes) {
        return Err(anyhow::anyhow!(
            "{window_name} resets_in_minutes must be between 0 and {max_minutes}"
        ));
    }
    let remaining = Duration::try_minutes(remaining_minutes)
        .ok_or_else(|| anyhow::anyhow!("resets_in_minutes is out of range"))?;
    let ends_at = now
        .checked_add_signed(remaining)
        .ok_or_else(|| anyhow::anyhow!("usage window end is out of range"))?;
    ends_at
        .checked_sub_signed(window_len)
        .ok_or_else(|| anyhow::anyhow!("usage window start is out of range"))
}

/// 把 `purchase_date`（YYYY-MM-DD）解释为本时区 00:00，转 UTC。
/// purchase_date 是 local_today() 写入的本地日期；转 UTC 时必须经过 Local 时区，
/// 否则本地早上的请求会被 UTC 午夜 cutoff 漏算。
fn month_window_start_utc(purchase_date: &str) -> Result<DateTime<Utc>> {
    let normalized = normalize_purchase_date(purchase_date)?;
    let start_naive = NaiveDate::parse_from_str(&normalized, "%Y-%m-%d")?
        .and_hms_opt(0, 0, 0)
        .unwrap();
    Ok(Local
        .from_local_datetime(&start_naive)
        .single()
        .ok_or_else(|| anyhow::anyhow!("ambiguous local datetime for purchase_date"))?
        .with_timezone(&Utc))
}

fn parse_rfc3339(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| anyhow::anyhow!("invalid RFC3339 timestamp: {e}"))
}

fn effective_usage(
    local_window_cost: f64,
    baseline: Option<(f64, f64)>,
    total_success_cost: f64,
    limit: f64,
) -> f64 {
    baseline.map_or(local_window_cost, |(percent, anchor)| {
        (limit * percent / 100.0 + (total_success_cost - anchor).max(0.0)).min(limit)
    })
}

fn forward_log_filter(
    status: Option<&str>,
    account_id: Option<&str>,
    model: Option<&str>,
    request_id: Option<&str>,
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> (String, Vec<Value>) {
    let mut filter = String::new();
    let mut params = Vec::new();
    for (clause, value) in [
        ("status = ?", status),
        ("account_id = ?", account_id),
        ("model = ?", model),
        ("request_id = ?", request_id),
        ("julianday(timestamp) >= julianday(?)", start_time),
        ("julianday(timestamp) <= julianday(?)", end_time),
    ] {
        if let Some(value) = value {
            filter.push_str(if params.is_empty() {
                " WHERE "
            } else {
                " AND "
            });
            filter.push_str(clause);
            params.push(Value::Text(value.to_owned()));
        }
    }
    (filter, params)
}

fn forward_log_order(sort_by: Option<&str>, sort_order: Option<&str>) -> String {
    let column = match sort_by {
        Some("timestamp") => "timestamp",
        Some("attempt") => "attempt",
        Some("prompt_tokens") => "prompt_tokens",
        Some("completion_tokens") => "completion_tokens",
        Some("cached_tokens") => "cached_tokens",
        Some("cost") => "cost",
        Some("model") => "model",
        Some("status") => "status",
        _ => "id",
    };
    let direction = if sort_order == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };
    format!("ORDER BY {column} {direction}, id DESC")
}

fn forward_log_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ForwardLog> {
    let raw_cost = row.get::<_, f64>(11)?;
    let cost_state = row.get::<_, String>(16)?;
    let cost = matches!(cost_state.as_str(), "priced" | "legacy_estimate").then_some(raw_cost);
    Ok(ForwardLog {
        id: row.get(0)?,
        timestamp: parse_datetime(row.get::<_, String>(1)?),
        model: row.get(2)?,
        account_id: row.get(3)?,
        account_name: row.get(4)?,
        status: row.get(5)?,
        http_status: row.get(6)?,
        prompt_tokens: row.get(7)?,
        completion_tokens: row.get(8)?,
        cached_tokens: row.get(9)?,
        cache_creation_tokens: row.get(10)?,
        cost,
        pricing_revision_id: row.get(12)?,
        quota_multiplier: row.get(13)?,
        local_adjustment_multiplier: row.get(14)?,
        service_tier: row.get(15)?,
        cost_state,
        error_message: row.get(17)?,
        request_id: row.get(18)?,
        attempt: row.get(19)?,
        error_source: row.get(20)?,
        error_stage: row.get(21)?,
        duration_ms: row.get(22)?,
        diagnostic: row
            .get::<_, Option<String>>(23)?
            .and_then(|json| serde_json::from_str(&json).ok()),
    })
}

fn account_from_row(row: &Row<'_>) -> rusqlite::Result<Account> {
    let created_at = row.get::<_, String>(14)?;
    let purchase_date = match row.get::<_, Option<String>>(7)? {
        Some(value) if normalize_purchase_date(&value).is_ok() => value,
        _ => migration_fallback_purchase_date(&created_at).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                14,
                Type::Text,
                Box::new(std::io::Error::other(error.to_string())),
            )
        })?,
    };
    let expires_on = purchase_expires_on(&purchase_date).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, Type::Text, Box::new(error))
    })?;
    Ok(Account {
        id: row.get(0)?,
        name: row.get(1)?,
        username: row.get(2)?,
        password_cipher: row.get(3)?,
        key_cipher: row.get(4)?,
        enabled: row.get::<_, i32>(5)? != 0,
        referral_code: row.get(6)?,
        purchase_date,
        expires_on,
        cooldown_until: row.get::<_, Option<String>>(8)?.map(parse_datetime),
        cooldown_generic_until: row.get::<_, Option<String>>(9)?.map(parse_datetime),
        cooldown_5h_until: row.get::<_, Option<String>>(10)?.map(parse_datetime),
        cooldown_week_until: row.get::<_, Option<String>>(11)?.map(parse_datetime),
        cooldown_month_until: row.get::<_, Option<String>>(12)?.map(parse_datetime),
        last_error: row.get(13)?,
        auth_error: row.get(16)?,
        created_at: parse_datetime(created_at),
        updated_at: parse_datetime(row.get::<_, String>(15)?),
    })
}

fn migration_fallback_purchase_date(created_at: &str) -> Result<String> {
    let created_at = DateTime::parse_from_rfc3339(created_at).map_err(|error| {
        anyhow::anyhow!(
            "invalid account created_at {created_at:?} while repairing purchase date: {error}"
        )
    })?;
    Ok(created_at
        .with_timezone(&Utc)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string())
}

fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|e| {
            eprintln!("error: failed to parse datetime '{}': {}, using now", s, e);
            Utc::now()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_data_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        dir.push(format!("ocg-db-test-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("test data dir should be created");
        dir
    }

    fn account(id: &str) -> Account {
        Account {
            id: id.into(),
            name: id.into(),
            username: None,
            password_cipher: None,
            key_cipher: "cipher".into(),
            enabled: true,
            referral_code: None,
            purchase_date: String::new(),
            expires_on: String::new(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            last_error: None,
            auth_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn forward_log(account_id: &str, status: &str, cost: f64) -> ForwardLog {
        ForwardLog {
            id: 0,
            timestamp: Utc::now(),
            model: "test".into(),
            account_id: account_id.into(),
            account_name: account_id.into(),
            status: status.into(),
            http_status: Some(200),
            prompt_tokens: 0,
            completion_tokens: 0,
            cached_tokens: 0,
            cache_creation_tokens: 0,
            cost: Some(cost),
            pricing_revision_id: None,
            quota_multiplier: None,
            local_adjustment_multiplier: None,
            service_tier: None,
            cost_state: "legacy_estimate".into(),
            error_message: None,
            request_id: None,
            attempt: None,
            error_source: None,
            error_stage: None,
            duration_ms: None,
            diagnostic: None,
        }
    }

    fn forward_log_at(
        account_id: &str,
        status: &str,
        cost: f64,
        timestamp: DateTime<Utc>,
    ) -> ForwardLog {
        let mut log = forward_log(account_id, status, cost);
        log.timestamp = timestamp;
        log
    }

    fn finalize_success(db: &Database, account_id: &str, cost: f64, timestamp: DateTime<Utc>) {
        let id = db
            .log_forward(&forward_log_at(account_id, "streaming", 0.0, timestamp))
            .expect("log should insert");
        db.update_forward_log(
            id,
            "success",
            None,
            ForwardMetrics {
                cost,
                cost_state: "priced",
                ..ForwardMetrics::default()
            },
            None,
            None,
        )
        .expect("stream should finalize");
    }

    fn assert_cost(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    fn create_v6_database(
        dir: &std::path::Path,
        extra_cooldown_columns: &str,
        extra_indexes: &str,
    ) -> Connection {
        let conn = Connection::open(dir.join("data.sqlite")).expect("v6 db should open");
        conn.execute_batch(&format!(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (6);
             CREATE TABLE accounts (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 key_cipher TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 referral_code TEXT,
                 recharge_date TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL,
                 cooldown_until TEXT,
                 last_error TEXT,
                 username TEXT,
                 password_cipher TEXT,
                 usage_5h_baseline_percent REAL,
                 usage_5h_anchor_success_cost REAL,
                 usage_week_baseline_percent REAL,
                 usage_week_anchor_success_cost REAL,
                 usage_month_baseline_percent REAL,
                 usage_month_anchor_success_cost REAL,
                 sort_order INTEGER NOT NULL DEFAULT 0
                 {extra_cooldown_columns}
             );
             CREATE TABLE forward_logs (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 timestamp TEXT NOT NULL,
                 model TEXT NOT NULL,
                 account_id TEXT NOT NULL,
                 account_name TEXT NOT NULL,
                 status TEXT NOT NULL,
                 http_status INTEGER,
                 prompt_tokens INTEGER NOT NULL DEFAULT 0,
                 completion_tokens INTEGER NOT NULL DEFAULT 0,
                 cached_tokens INTEGER NOT NULL DEFAULT 0,
                 cost REAL NOT NULL DEFAULT 0,
                 error_message TEXT
             );
             {extra_indexes}"
        ))
        .expect("v6 schema should be created");
        conn
    }

    #[test]
    fn v7_migration_repairs_pr11_pr12_and_combined_v6_databases() {
        let future = (Utc::now() + Duration::days(2)).to_rfc3339();
        for (label, extra_columns, extra_indexes, source_column, error) in [
            (
                "pr11-v6",
                "",
                "CREATE INDEX idx_forward_logs_model ON forward_logs(model);\nCREATE INDEX idx_forward_logs_status ON forward_logs(status);",
                "",
                "5 hour usage limit reached",
            ),
            (
                "pr12-v6",
                ", cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
                "",
                "cooldown_week_until",
                "weekly usage limit reached",
            ),
            (
                "combined-v6",
                ", cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
                "CREATE INDEX idx_forward_logs_model ON forward_logs(model);\nCREATE INDEX idx_forward_logs_status ON forward_logs(status);",
                "cooldown_month_until",
                "monthly usage limit reached",
            ),
            (
                "generic-dev-v6",
                ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
                "CREATE INDEX idx_forward_logs_model ON forward_logs(model);\nCREATE INDEX idx_forward_logs_status ON forward_logs(status);",
                "cooldown_generic_until",
                "unknown rate limit",
            ),
        ] {
            let dir = temp_data_dir(label);
            let conn = create_v6_database(&dir, extra_columns, extra_indexes);
            conn.execute(
                "INSERT INTO accounts
                 (id, name, key_cipher, recharge_date, created_at, updated_at, cooldown_until, last_error)
                 VALUES ('old', 'old', 'cipher', '2026-07-01', ?1, ?1, ?2, ?3)",
                params![Utc::now().to_rfc3339(), future, error],
            )
            .expect("v6 account should be inserted");
            if !source_column.is_empty() {
                conn.execute(
                    &format!("UPDATE accounts SET {source_column} = ?1 WHERE id = 'old'"),
                    [&future],
                )
                .expect("existing cooldown source should be set");
            }
            drop(conn);

            let db = Database::open(dir.clone()).expect("v6 database should migrate");
            let version: i32 = db
                .conn
                .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                    row.get(0)
                })
                .expect("schema version should load");
            assert_eq!(version, 15, "{label}");
            let account = db
                .get_account("old")
                .expect("account query should work")
                .expect("account should exist");
            assert!(account.cooldown_until.is_some(), "{label}");
            assert!(account.is_cooling_at(Utc::now()), "{label}");
            let indexes: i64 = db
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'index' AND name IN (
                         'idx_forward_logs_model',
                         'idx_forward_logs_status',
                         'idx_forward_logs_time_instant'
                     )",
                    [],
                    |row| row.get(0),
                )
                .expect("indexes should be queryable");
            assert_eq!(indexes, 3, "{label}");

            drop(db);
            fs::remove_dir_all(dir).expect("test data dir should be removed");
        }
    }

    #[test]
    fn v4_migration_preserves_uncalibrated_usage() {
        let dir = temp_data_dir("v4-migration");
        let conn = Connection::open(dir.join("data.sqlite")).expect("v3 db should open");
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (3);
             CREATE TABLE accounts (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 key_cipher TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 referral_code TEXT,
                 recharge_date TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL,
                 cooldown_until TEXT,
                 last_error TEXT,
                 username TEXT,
                 password_cipher TEXT
             );
             CREATE TABLE forward_logs (
                 timestamp TEXT NOT NULL,
                 model TEXT NOT NULL DEFAULT 'test',
                 account_id TEXT NOT NULL,
                 status TEXT NOT NULL,
                 cost REAL NOT NULL DEFAULT 0
             );",
        )
        .expect("v3 schema should be created");
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO accounts (id, name, key_cipher, created_at, updated_at) VALUES (?1, ?1, 'cipher', ?2, ?2)",
            params!["old", now],
        )
        .expect("v3 account should be inserted");
        conn.execute(
            "INSERT INTO forward_logs (timestamp, account_id, status, cost) VALUES (?1, 'old', 'success', 2.5)",
            [Utc::now().to_rfc3339()],
        )
        .expect("v3 usage should be inserted");
        drop(conn);

        let db = Database::open(dir.clone()).expect("v3 db should migrate");
        let version: i32 = db
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .expect("schema version should be readable");
        let usage = db.account_usage("old").expect("usage should load");
        assert_eq!(version, 15);
        assert_eq!(
            db.get_account("old")
                .expect("account should load")
                .expect("account should exist")
                .purchase_date,
            now[..10]
        );
        assert_cost(usage.window_5h, 2.5);
        assert_cost(usage.window_week, 2.5);
        assert_cost(usage.window_month, 2.5);

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn v5_migration_backfills_dates_and_stable_dense_order() {
        let dir = temp_data_dir("v5-migration");
        let conn = Connection::open(dir.join("data.sqlite")).expect("v4 db should open");
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (4);
             CREATE TABLE accounts (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 key_cipher TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 referral_code TEXT,
                 recharge_date TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL,
                 cooldown_until TEXT,
                 last_error TEXT,
                 username TEXT,
                 password_cipher TEXT,
                 usage_5h_baseline_percent REAL,
                 usage_5h_anchor_success_cost REAL,
                 usage_week_baseline_percent REAL,
                 usage_week_anchor_success_cost REAL,
                 usage_month_baseline_percent REAL,
                 usage_month_anchor_success_cost REAL
             );
             CREATE TABLE forward_logs (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 timestamp TEXT NOT NULL,
                 model TEXT NOT NULL,
                 account_id TEXT NOT NULL,
                 account_name TEXT NOT NULL,
                 status TEXT NOT NULL,
                 http_status INTEGER,
                 prompt_tokens INTEGER NOT NULL DEFAULT 0,
                 completion_tokens INTEGER NOT NULL DEFAULT 0,
                 cached_tokens INTEGER NOT NULL DEFAULT 0,
                 cost REAL NOT NULL DEFAULT 0,
                 error_message TEXT
             );",
        )
        .expect("v4 schema should be created");
        let shared_created_at = "2026-01-02T01:30:00+02:00";
        for (id, recharge_date, created_at) in [
            ("a", Some("2025-12-31"), shared_created_at),
            ("b", None, shared_created_at),
            ("c", Some(""), shared_created_at),
            ("d", Some("2026-2-3"), "2026-02-04T04:00:00Z"),
        ] {
            conn.execute(
                "INSERT INTO accounts
                 (id, name, key_cipher, recharge_date, created_at, updated_at)
                 VALUES (?1, ?1, 'cipher', ?2, ?3, ?3)",
                params![id, recharge_date, created_at],
            )
            .expect("v4 account should be inserted");
        }
        drop(conn);

        let db = Database::open(dir.clone()).expect("v4 db should migrate");
        let accounts = db.list_accounts().expect("migrated accounts should load");
        assert_eq!(
            accounts
                .iter()
                .map(|account| account.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c", "d"]
        );
        assert_eq!(accounts[0].purchase_date, "2025-12-31");
        assert_eq!(accounts[1].purchase_date, "2026-01-01");
        assert_eq!(accounts[2].purchase_date, "2026-01-01");
        assert_eq!(accounts[3].purchase_date, "2026-02-04");
        let sort_orders = db
            .conn
            .prepare("SELECT sort_order FROM accounts ORDER BY sort_order")
            .expect("sort query should prepare")
            .query_map([], |row| row.get::<_, i64>(0))
            .expect("sort query should run")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("sort orders should load");
        assert_eq!(sort_orders, [0, 1, 2, 3]);
        drop(db);

        let reopened = Database::open(dir.clone()).expect("migrated db should reopen");
        assert_eq!(
            reopened
                .list_accounts()
                .expect("reopened accounts should load")
                .iter()
                .map(|account| account.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c", "d"]
        );

        drop(reopened);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn v8_migration_repairs_purchase_dates_written_by_older_binaries() {
        let dir = temp_data_dir("v8-purchase-date-repair");
        let conn = create_v6_database(
            &dir,
            ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
            "",
        );
        conn.execute("INSERT INTO schema_version (version) VALUES (7)", [])
            .expect("v7 schema version should be recorded");

        let created_at = "2026-01-02T01:30:00+02:00";
        for (id, recharge_date) in [
            ("valid", Some("2025-12-31")),
            ("null", None),
            ("invalid", Some("2026-2-3")),
        ] {
            conn.execute(
                "INSERT INTO accounts
                 (id, name, key_cipher, recharge_date, created_at, updated_at)
                 VALUES (?1, ?1, 'cipher', ?2, ?3, ?3)",
                params![id, recharge_date, created_at],
            )
            .expect("legacy account should be inserted");
        }
        drop(conn);

        let db = Database::open(dir.clone()).expect("v7 database should migrate");
        let version: i32 = db
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .expect("schema version should load");
        assert_eq!(version, 15);
        assert_eq!(
            db.get_account("valid")
                .expect("valid account query should work")
                .expect("valid account should exist")
                .purchase_date,
            "2025-12-31"
        );
        for id in ["null", "invalid"] {
            assert_eq!(
                db.get_account(id)
                    .expect("repaired account query should work")
                    .expect("repaired account should exist")
                    .purchase_date,
                "2026-01-01",
                "{id}"
            );
        }

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn v9_migration_preserves_charged_legacy_errors() {
        let dir = temp_data_dir("v9-charged-error-cost");
        let conn = create_v6_database(
            &dir,
            ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
            "",
        );
        conn.execute("INSERT INTO schema_version (version) VALUES (7)", [])
            .expect("v7 schema version should be recorded");
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO accounts
             (id, name, key_cipher, recharge_date, created_at, updated_at)
             VALUES ('legacy', 'legacy', 'cipher', '2026-07-01', ?1, ?1)",
            [&now],
        )
        .expect("legacy account should be inserted");
        for (status, cost) in [("error", 1.25), ("error", 0.0), ("success", 2.0)] {
            conn.execute(
                "INSERT INTO forward_logs
                 (timestamp, model, account_id, account_name, status, http_status, cost)
                 VALUES (?1, 'glm-5.2', 'legacy', 'legacy', ?2, 200, ?3)",
                params![now, status, cost],
            )
            .expect("legacy forward log should be inserted");
        }
        drop(conn);

        let db = Database::open(dir.clone()).expect("v7 database should migrate through v10");
        let states = db
            .conn
            .prepare("SELECT status, cost, cost_state FROM forward_logs ORDER BY id")
            .expect("migrated logs should prepare")
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .expect("migrated logs should query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("migrated logs should load");
        assert_eq!(
            states,
            [
                ("error".to_string(), 1.25, "legacy_estimate".to_string()),
                ("error".to_string(), 0.0, "not_applicable".to_string()),
                ("success".to_string(), 2.0, "legacy_estimate".to_string()),
            ]
        );
        assert_cost(
            db.account_usage("legacy")
                .expect("legacy usage should load")
                .window_month,
            3.25,
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn v10_migration_repairs_charged_errors_from_original_v9() {
        let dir = temp_data_dir("v10-repair-v9-charged-error-cost");
        let conn = create_v6_database(
            &dir,
            ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
            "",
        );
        conn.execute_batch(
            "CREATE TABLE pricing_snapshots (
                 revision TEXT PRIMARY KEY,
                 activated_at TEXT NOT NULL,
                 document_updated_at TEXT NOT NULL,
                 source_url TEXT NOT NULL,
                 content_hash TEXT NOT NULL,
                 snapshot_json TEXT NOT NULL
             );
             CREATE INDEX idx_pricing_snapshots_activated
                 ON pricing_snapshots(activated_at DESC);
             ALTER TABLE forward_logs ADD COLUMN pricing_revision_id TEXT;
             ALTER TABLE forward_logs ADD COLUMN quota_multiplier REAL;
             ALTER TABLE forward_logs ADD COLUMN local_adjustment_multiplier REAL;
             ALTER TABLE forward_logs ADD COLUMN cache_creation_tokens INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE forward_logs ADD COLUMN service_tier TEXT;
             ALTER TABLE forward_logs ADD COLUMN cost_state TEXT NOT NULL DEFAULT 'not_applicable';
             INSERT INTO schema_version (version) VALUES (9);",
        )
        .expect("original v9 schema should be created");
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO accounts
             (id, name, key_cipher, recharge_date, created_at, updated_at)
             VALUES ('legacy', 'legacy', 'cipher', '2026-07-01', ?1, ?1)",
            [&now],
        )
        .expect("legacy account should be inserted");
        for (cost, cost_state) in [
            (1.25, "not_applicable"),
            (0.0, "not_applicable"),
            (4.0, "unpriced"),
        ] {
            conn.execute(
                "INSERT INTO forward_logs
                 (timestamp, model, account_id, account_name, status, http_status, cost, cost_state)
                 VALUES (?1, 'glm-5.2', 'legacy', 'legacy', 'error', 200, ?2, ?3)",
                params![now, cost, cost_state],
            )
            .expect("original v9 forward log should be inserted");
        }
        drop(conn);

        let db = Database::open(dir.clone()).expect("v9 database should migrate through v11");
        let version: i32 = db
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .expect("schema version should load");
        assert_eq!(version, 15);
        let states = db
            .conn
            .prepare("SELECT cost, cost_state FROM forward_logs ORDER BY id")
            .expect("migrated logs should prepare")
            .query_map([], |row| {
                Ok((row.get::<_, f64>(0)?, row.get::<_, String>(1)?))
            })
            .expect("migrated logs should query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("migrated logs should load");
        assert_eq!(
            states,
            [
                (1.25, "legacy_estimate".to_string()),
                (0.0, "not_applicable".to_string()),
                (4.0, "unpriced".to_string()),
            ]
        );
        assert_cost(
            db.account_usage("legacy")
                .expect("legacy usage should load")
                .window_month,
            1.25,
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn account_reads_fallback_after_v8_data_is_corrupted() {
        let dir = temp_data_dir("post-v8-purchase-date-corruption");
        let conn = create_v6_database(
            &dir,
            ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
            "",
        );
        conn.execute("INSERT INTO schema_version (version) VALUES (7)", [])
            .expect("v7 schema version should be recorded");
        drop(conn);

        let db = Database::open(dir.clone()).expect("database should open");
        let version: i32 = db
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .expect("schema version should load");
        assert_eq!(version, 15);

        let created_at = DateTime::parse_from_rfc3339("2026-01-02T01:30:00+02:00")
            .expect("fixed timestamp should parse")
            .with_timezone(&Utc);
        for id in ["null", "invalid"] {
            let mut legacy = account(id);
            legacy.purchase_date = "2025-12-31".to_string();
            legacy.created_at = created_at;
            legacy.updated_at = created_at;
            db.create_account(&legacy)
                .expect("account should be created before corruption");
        }
        // v12 重建 accounts 表后 recharge_date 是 NOT NULL（恢复 v1 原始约束），
        // 无法再被 UPDATE 成 NULL；只测试 invalid-text 这一支。
        db.conn
            .execute(
                "UPDATE accounts SET recharge_date = 'not-a-date' WHERE id = 'invalid'",
                [],
            )
            .expect("purchase date should be corrupted to invalid text");

        let accounts = db
            .list_accounts()
            .expect("one corrupt row must not break the account list");
        assert_eq!(accounts.len(), 2);
        // 仅 invalid 被破坏；null 仍持有原始 2025-12-31。
        let invalid_account = accounts
            .iter()
            .find(|a| a.id == "invalid")
            .expect("invalid account should be present");
        assert_eq!(
            invalid_account.purchase_date, "2026-01-01",
            "list_accounts should fall back to default date for corrupted rows"
        );
        let invalid = db
            .get_account("invalid")
            .expect("corrupt account query should work")
            .expect("corrupt account should exist");
        assert_eq!(invalid.purchase_date, "2026-01-01");
        assert_eq!(invalid.expires_on, "2026-02-01");
        let remains_invalid: bool = db
            .conn
            .query_row(
                "SELECT recharge_date = 'not-a-date' FROM accounts WHERE id = 'invalid'",
                [],
                |row| row.get(0),
            )
            .expect("raw purchase date should remain queryable");
        assert!(
            remains_invalid,
            "read fallback must not hide a migration rerun"
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn account_creation_defaults_dates_and_appends_to_saved_order() {
        let dir = temp_data_dir("create-order");
        let db = Database::open(dir.clone()).expect("db should open");
        let purchase_date_is_not_null: bool = db
            .conn
            .query_row(
                "SELECT [notnull]
                 FROM pragma_table_info('accounts')
                 WHERE name = 'recharge_date'",
                [],
                |row| row.get(0),
            )
            .expect("fresh account schema should expose purchase date constraints");
        assert!(purchase_date_is_not_null);
        let mut first = account("first");
        first.created_at = Utc::now() + Duration::days(1);
        db.create_account(&first)
            .expect("first account should save");
        let mut second = account("second");
        second.created_at = Utc::now() - Duration::days(1);
        second.purchase_date = "2024-01-31".to_string();
        db.create_account(&second)
            .expect("second account should save");

        let accounts = db.list_accounts().expect("accounts should load");
        assert_eq!(
            accounts
                .iter()
                .map(|account| account.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(accounts[0].purchase_date, local_today());
        assert_eq!(
            accounts[0].expires_on,
            purchase_expires_on(&accounts[0].purchase_date)
                .expect("default date should have an expiry")
        );
        assert_eq!(accounts[1].expires_on, "2024-02-29");

        let mut invalid = account("invalid");
        invalid.purchase_date = "2026-2-03".to_string();
        assert!(db.create_account(&invalid).is_err());
        assert!(
            db.get_account("invalid")
                .expect("invalid account lookup should work")
                .is_none()
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn reorder_accounts_validates_atomically_and_persists_dense_order() {
        let dir = temp_data_dir("reorder");
        let db = Database::open(dir.clone()).expect("db should open");
        for id in ["a", "b", "c"] {
            db.create_account(&account(id))
                .expect("account should be created");
        }

        db.reorder_accounts(&["c".into(), "a".into(), "b".into()])
            .expect("valid reorder should save");
        assert_eq!(account_ids(&db), ["c", "a", "b"]);

        let duplicate = db
            .reorder_accounts(&["c".into(), "c".into(), "b".into()])
            .expect_err("duplicates should fail");
        assert!(matches!(
            duplicate,
            ReorderAccountsError::DuplicateAccountId
        ));
        assert_eq!(account_ids(&db), ["c", "a", "b"]);

        for stale in [
            vec!["c".into(), "a".into()],
            vec!["c".into(), "a".into(), "missing".into()],
            Vec::<String>::new(),
        ] {
            let error = db
                .reorder_accounts(&stale)
                .expect_err("stale account set should fail");
            assert!(matches!(error, ReorderAccountsError::AccountSetMismatch));
            assert_eq!(account_ids(&db), ["c", "a", "b"]);
        }

        let sort_orders = db
            .conn
            .prepare("SELECT sort_order FROM accounts ORDER BY sort_order")
            .expect("sort query should prepare")
            .query_map([], |row| row.get::<_, i64>(0))
            .expect("sort query should run")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("sort orders should load");
        assert_eq!(sort_orders, [0, 1, 2]);
        drop(db);

        let reopened = Database::open(dir.clone()).expect("db should reopen");
        assert_eq!(account_ids(&reopened), ["c", "a", "b"]);
        drop(reopened);

        let empty_dir = temp_data_dir("reorder-empty");
        let empty = Database::open(empty_dir.clone()).expect("empty db should open");
        empty
            .reorder_accounts(&[])
            .expect("empty order should be valid for an empty database");
        drop(empty);

        fs::remove_dir_all(dir).expect("test data dir should be removed");
        fs::remove_dir_all(empty_dir).expect("empty test data dir should be removed");
    }

    #[test]
    fn reorder_accounts_rolls_back_when_an_update_fails_mid_transaction() {
        let dir = temp_data_dir("reorder-write-failure");
        let db = Database::open(dir.clone()).expect("db should open");
        for id in ["a", "b", "c"] {
            db.create_account(&account(id))
                .expect("account should be created");
        }
        db.conn
            .execute_batch(
                "CREATE TRIGGER reject_b_sort_update
                 BEFORE UPDATE OF sort_order ON accounts
                 WHEN NEW.id = 'b'
                 BEGIN
                     SELECT RAISE(ABORT, 'forced reorder failure');
                 END;",
            )
            .expect("failure trigger should be installed");

        let error = db
            .reorder_accounts(&["c".into(), "a".into(), "b".into()])
            .expect_err("the trigger should interrupt the reorder");
        assert!(matches!(error, ReorderAccountsError::Database(_)));
        assert_eq!(account_ids(&db), ["a", "b", "c"]);

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    fn account_ids(db: &Database) -> Vec<String> {
        db.list_accounts()
            .expect("accounts should load")
            .into_iter()
            .map(|account| account.id)
            .collect()
    }

    #[test]
    fn migrations_roll_back_partial_schema_changes() {
        let dir = temp_data_dir("atomic-migration");
        let conn = Connection::open(dir.join("data.sqlite")).expect("v3 db should open");
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (3);
             CREATE TABLE accounts (
                 id TEXT PRIMARY KEY,
                 usage_5h_anchor_success_cost REAL
             );",
        )
        .expect("conflicting v3 schema should be created");
        drop(conn);

        assert!(Database::open(dir.clone()).is_err());
        let conn = Connection::open(dir.join("data.sqlite")).expect("db should reopen");
        let columns = conn
            .prepare("PRAGMA table_info(accounts)")
            .expect("table info should prepare")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("table info should query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("columns should load");
        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .expect("schema version should load");
        assert!(
            !columns
                .iter()
                .any(|name| name == "usage_5h_baseline_percent")
        );
        assert!(
            columns
                .iter()
                .any(|name| name == "usage_5h_anchor_success_cost")
        );
        assert_eq!(version, 3);

        drop(conn);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn account_stays_cooling_until_all_windows_expire() {
        let dir = temp_data_dir("multi-window-cooldown");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("multi"))
            .expect("account should be created");

        let now = Utc::now();
        let past_5h = now - Duration::minutes(1);
        let future_week = now + Duration::days(2);
        db.set_account_rate_limit(
            "multi",
            past_5h,
            "5-hour usage limit reached. Resets in 13min.",
            Some(UsageWindowKind::FiveHours),
        )
        .expect("5h rate limit should save");
        db.set_account_rate_limit(
            "multi",
            future_week,
            "weekly usage limit reached. Resets in 4 days.",
            Some(UsageWindowKind::Week),
        )
        .expect("weekly rate limit should save");

        let account = db
            .get_account("multi")
            .expect("account should load")
            .expect("account should exist");
        assert!(account.cooldown_5h_until.is_some_and(|until| until <= now));
        assert!(account.cooldown_week_until.is_some_and(|until| until > now));
        assert!(
            account
                .cooldown_until
                .is_some_and(|until| (until - future_week).num_seconds().abs() < 2)
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn v13_migration_preserves_legacy_manual_usage_calibration() {
        let dir = temp_data_dir("v13-legacy-calibration");
        let db = Database::open(dir.clone()).expect("db should open");
        let mut acct = account("legacy-calibration");
        acct.purchase_date = local_today();
        db.create_account(&acct).expect("account should be created");
        finalize_success(&db, "legacy-calibration", 2.0, Utc::now());
        db.conn
            .execute(
                "UPDATE accounts SET
                    usage_5h_baseline_percent = 50,
                    usage_5h_anchor_success_cost = 2,
                    usage_week_baseline_percent = 40,
                    usage_week_anchor_success_cost = 2,
                    usage_month_baseline_percent = 25,
                    usage_month_anchor_success_cost = 2
                 WHERE id = 'legacy-calibration'",
                [],
            )
            .expect("legacy baselines should save");
        finalize_success(&db, "legacy-calibration", 1.0, Utc::now());
        db.conn
            .execute_batch(
                "DELETE FROM schema_version;
                 INSERT INTO schema_version (version) VALUES (10);",
            )
            .expect("legacy schema version should save");
        drop(db);

        let db = Database::open(dir.clone()).expect("legacy database should migrate");
        let usage = db
            .account_usage("legacy-calibration")
            .expect("migrated usage should load");
        // Old effective values: 50% * 12 + 1, 40% * 30 + 1,
        // and 25% * 60 + 1. The migration must preserve all three.
        assert_cost(usage.window_5h, 7.0);
        assert_cost(usage.window_week, 13.0);
        assert_cost(usage.window_month, 16.0);

        let (version, remaining_baselines): (i32, i64) = db
            .conn
            .query_row(
                "SELECT
                    (SELECT MAX(version) FROM schema_version),
                    COUNT(*)
                 FROM accounts
                 WHERE usage_5h_baseline_percent IS NOT NULL
                    OR usage_week_baseline_percent IS NOT NULL
                    OR usage_month_baseline_percent IS NOT NULL",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("migration state should load");
        assert_eq!(version, 15);
        assert_eq!(remaining_baselines, 0);

        finalize_success(&db, "legacy-calibration", 2.0, Utc::now());
        let usage = db
            .account_usage("legacy-calibration")
            .expect("new usage should accumulate after migration");
        assert_cost(usage.window_5h, 9.0);
        assert_cost(usage.window_week, 15.0);
        assert_cost(usage.window_month, 18.0);

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn v14_migrates_v13_logs_and_adds_request_id_indexes() {
        let dir = temp_data_dir("v14-log-diagnostics");
        let db = Database::open(dir.clone()).expect("db should open");
        db.conn
            .execute_batch(
                "DROP INDEX idx_forward_logs_request_id;
                 DROP INDEX idx_gateway_logs_request_id;
                 ALTER TABLE forward_logs DROP COLUMN request_id;
                 ALTER TABLE forward_logs DROP COLUMN attempt;
                 ALTER TABLE forward_logs DROP COLUMN error_source;
                 ALTER TABLE forward_logs DROP COLUMN error_stage;
                 ALTER TABLE forward_logs DROP COLUMN duration_ms;
                 ALTER TABLE forward_logs DROP COLUMN diagnostic_json;
                 ALTER TABLE gateway_logs DROP COLUMN request_id;
                 ALTER TABLE gateway_logs DROP COLUMN attempt;
                 ALTER TABLE gateway_logs DROP COLUMN error_source;
                 ALTER TABLE gateway_logs DROP COLUMN error_stage;
                 ALTER TABLE gateway_logs DROP COLUMN duration_ms;
                 ALTER TABLE gateway_logs DROP COLUMN diagnostic_json;
                 INSERT INTO forward_logs
                    (timestamp, model, account_id, account_name, status, error_message)
                 VALUES ('2026-07-01T00:00:00Z', 'legacy-model', 'legacy', 'Legacy',
                         'client_error', 'legacy error');
                 INSERT INTO gateway_logs (level, category, message, created_at)
                 VALUES ('warn', 'legacy', 'legacy gateway error', '2026-07-01T00:00:00Z');
                 DELETE FROM schema_version;
                 INSERT INTO schema_version (version) VALUES (13);",
            )
            .expect("v13 schema should be prepared");
        drop(db);

        let db = Database::open(dir.clone()).expect("v13 database should migrate");
        let version: i32 = db
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .expect("schema version should load");
        assert_eq!(version, 15);
        for index in ["idx_forward_logs_request_id", "idx_gateway_logs_request_id"] {
            let exists: bool = db
                .conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
                    [index],
                    |row| row.get(0),
                )
                .expect("index state should load");
            assert!(exists, "{index} should exist");
        }
        let forward = db
            .query_forward_logs(ForwardLogQueryOptions {
                limit: 10,
                offset: 0,
                status: None,
                account_id: None,
                model: None,
                request_id: None,
                start_time: None,
                end_time: None,
                sort_by: None,
                sort_order: None,
            })
            .expect("legacy forward log should load")
            .items
            .pop()
            .expect("legacy forward log should remain");
        assert_eq!(forward.error_message.as_deref(), Some("legacy error"));
        assert!(forward.request_id.is_none());
        assert!(forward.diagnostic.is_none());
        let gateway = db
            .list_gateway_logs(10)
            .expect("legacy gateway log should load")
            .pop()
            .expect("legacy gateway log should remain");
        assert_eq!(gateway.message, "legacy gateway error");
        assert!(gateway.request_id.is_none());
        assert!(gateway.diagnostic.is_none());

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn v15_migration_adds_nullable_auth_error() {
        let dir = temp_data_dir("v15-auth-error");
        let conn = Connection::open(dir.join("data.sqlite")).expect("legacy db should open");
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (14);
             CREATE TABLE accounts (id TEXT PRIMARY KEY);
             INSERT INTO accounts (id) VALUES ('legacy');
             CREATE TABLE forward_logs (
                 timestamp TEXT,
                 cost_state TEXT NOT NULL DEFAULT 'not_applicable',
                 diagnostic_json TEXT
             );
             CREATE TABLE gateway_logs (created_at TEXT, diagnostic_json TEXT);",
        )
        .expect("v14 fixture should be created");
        drop(conn);

        let db = Database::open(dir.clone()).expect("v14 database should migrate");
        let (version, auth_error): (i32, Option<String>) = db
            .conn
            .query_row(
                "SELECT (SELECT MAX(version) FROM schema_version), auth_error
                 FROM accounts WHERE id = 'legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("v15 migration state should load");
        assert_eq!(version, 15);
        assert!(auth_error.is_none());

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn diagnostic_retention_removes_only_old_json() {
        let dir = temp_data_dir("diagnostic-retention");
        let db = Database::open(dir.clone()).expect("db should open");
        db.conn
            .execute_batch(
                "INSERT INTO forward_logs
                    (timestamp, model, account_id, account_name, status, error_message,
                     request_id, attempt, error_source, error_stage, duration_ms, diagnostic_json)
                 VALUES
                    (datetime('now', '-31 days'), 'old', 'a', 'A', 'client_error', 'keep me',
                     'ocg-old', 1, 'upstream', 'upstream_http', 12, '{\"old\":true}'),
                    (datetime('now', '-29 days'), 'new', 'a', 'A', 'client_error', 'keep new',
                     'ocg-new', 1, 'upstream', 'upstream_http', 13, '{\"new\":true}');
                 INSERT INTO gateway_logs
                    (level, category, message, created_at, request_id, error_source,
                     error_stage, duration_ms, diagnostic_json)
                 VALUES
                    ('warn', 'gateway', 'old gateway', datetime('now', '-31 days'),
                     'ocg-gateway-old', 'client', 'parse', 5, '{\"old\":true}'),
                    ('warn', 'gateway', 'new gateway', datetime('now', '-29 days'),
                     'ocg-gateway-new', 'client', 'parse', 6, '{\"new\":true}');",
            )
            .expect("diagnostic rows should insert");
        drop(db);

        let db = Database::open(dir.clone()).expect("db reopen should apply retention");
        let (old_detail, old_id, old_error, old_source): (Option<String>, String, String, String) =
            db.conn
                .query_row(
                    "SELECT diagnostic_json, request_id, error_message, error_source
                 FROM forward_logs WHERE request_id='ocg-old'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .expect("old row should remain");
        assert!(old_detail.is_none());
        assert_eq!(old_id, "ocg-old");
        assert_eq!(old_error, "keep me");
        assert_eq!(old_source, "upstream");
        let new_detail: Option<String> = db
            .conn
            .query_row(
                "SELECT diagnostic_json FROM forward_logs WHERE request_id='ocg-new'",
                [],
                |row| row.get(0),
            )
            .expect("new detail should load");
        assert!(new_detail.is_some());
        let gateway_details: (Option<String>, Option<String>) = db
            .conn
            .query_row(
                "SELECT
                    (SELECT diagnostic_json FROM gateway_logs WHERE request_id='ocg-gateway-old'),
                    (SELECT diagnostic_json FROM gateway_logs WHERE request_id='ocg-gateway-new')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("gateway details should load");
        assert!(gateway_details.0.is_none());
        assert!(gateway_details.1.is_some());

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn fixed_window_5h_starts_at_first_success_and_expires_after_5h() {
        let dir = temp_data_dir("fixed-5h");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("fixed"))
            .expect("account should be created");

        // 第一条成功请求落在 4h 前：固定窗口起点 = 4h 前，倒计时 ≈ 1h
        let ts1 = Utc::now() - Duration::hours(4);
        finalize_success(&db, "fixed", 1.0, ts1);
        // 窗口内的第二条请求：累加
        let ts2 = ts1 + Duration::hours(1);
        finalize_success(&db, "fixed", 2.0, ts2);

        let usage = db.account_usage("fixed").expect("usage should load");
        assert_cost(usage.window_5h, 3.0);
        let reset = usage
            .resets_in_5h
            .expect("5h window reset should be set while window is active");
        let remaining_min = (reset - Utc::now()).num_minutes();
        assert!(
            (55..=65).contains(&remaining_min),
            "expected ~60min remaining, got {remaining_min}"
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn fixed_window_treats_exact_end_as_the_next_window_start() {
        let dir = temp_data_dir("fixed-boundary");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("boundary"))
            .expect("account should be created");

        let first = Utc::now() - Duration::hours(5) - Duration::minutes(1);
        let exact_end = first + Duration::hours(5);
        finalize_success(&db, "boundary", 10.0, first);
        finalize_success(&db, "boundary", 2.0, exact_end);

        let usage = db.account_usage("boundary").expect("usage should load");
        assert_cost(usage.window_5h, 2.0);
        assert!(usage.resets_in_5h.is_some());

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn fixed_window_5h_rebuilds_after_expiry_when_new_request_arrives() {
        let dir = temp_data_dir("fixed-5h-rebuild");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("rebuild"))
            .expect("account should be created");

        // 6h 前的第一条请求：窗口已过期
        let ts1 = Utc::now() - Duration::hours(6);
        finalize_success(&db, "rebuild", 10.0, ts1);
        // 1h 前的第二条请求：触发新窗口
        let ts2 = Utc::now() - Duration::hours(1);
        finalize_success(&db, "rebuild", 5.0, ts2);

        let usage = db.account_usage("rebuild").expect("usage should load");
        // 新窗口只包含 ts2 之后：10 已被丢弃，只剩 5
        assert_cost(usage.window_5h, 5.0);
        let reset = usage
            .resets_in_5h
            .expect("5h window reset should be set after rebuild");
        let remaining_min = (reset - Utc::now()).num_minutes();
        assert!(
            (235..=245).contains(&remaining_min),
            "expected ~240min remaining, got {remaining_min}"
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn fixed_window_5h_advances_through_multiple_expired_windows_in_one_call() {
        // 复现用户报告的"刷新递减"循环 bug：
        //   4 条间隔 6h 的计费日志（全部已过期）。
        // 旧实现每次刷新只前进一个窗口，前端可见 60→30→13→5.8→0→60+ 循环；
        // 修复后一次调用内连过 4 个过期窗口，next=None 时清空并返回 0，
        // 第二次刷新仍为 0，不再回到最旧日志。
        let dir = temp_data_dir("fixed-5h-multi-expired");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("cycle"))
            .expect("account should be created");

        // ts1 = -24h, ts2 = -18h, ts3 = -12h, ts4 = -6h：每条间隔 6h（> 5h 窗口长度）。
        let ts1 = Utc::now() - Duration::hours(24);
        let ts2 = ts1 + Duration::hours(6);
        let ts3 = ts2 + Duration::hours(6);
        let ts4 = ts3 + Duration::hours(6);
        finalize_success(&db, "cycle", 10.0, ts1);
        finalize_success(&db, "cycle", 5.0, ts2);
        finalize_success(&db, "cycle", 3.0, ts3);
        finalize_success(&db, "cycle", 2.0, ts4);

        // 第一次刷新：应直接走完所有过期窗口，返回 0（无新请求）。
        let usage = db.account_usage("cycle").expect("usage should load");
        assert_cost(usage.window_5h, 0.0);
        assert!(
            usage.resets_in_5h.is_none(),
            "no active window after all expired; resets_in_5h should be None"
        );

        // 第二次刷新：不应回到最旧日志循环重放，仍稳定为 0。
        let usage2 = db.account_usage("cycle").expect("usage should load again");
        assert_cost(usage2.window_5h, 0.0);
        assert!(usage2.resets_in_5h.is_none());

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn fixed_window_5h_finds_active_window_after_multiple_expired() {
        // 多条已过期日志后跟一条近期日志：修复后第一次刷新就应落在有效窗口上，
        // 而不是停在某个过期窗口里返回错误的中间值。
        let dir = temp_data_dir("fixed-5h-active-after-expired");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("active"))
            .expect("account should be created");

        // 三条过期日志间隔 6h，再加一条 1h 前的近期日志。
        let ts1 = Utc::now() - Duration::hours(19);
        let ts2 = ts1 + Duration::hours(6); // -13h
        let ts3 = ts2 + Duration::hours(6); // -7h，仍过期
        let ts4 = Utc::now() - Duration::hours(1); // 近期，落在有效窗口内
        finalize_success(&db, "active", 10.0, ts1);
        finalize_success(&db, "active", 5.0, ts2);
        finalize_success(&db, "active", 3.0, ts3);
        finalize_success(&db, "active", 2.0, ts4);

        // 第一次刷新：连过 3 个过期窗口，落在 ts4 上，只算 ts4 之后的 cost = 2.0。
        let usage = db.account_usage("active").expect("usage should load");
        assert_cost(usage.window_5h, 2.0);
        let reset = usage
            .resets_in_5h
            .expect("5h window reset should be anchored at ts4");
        let remaining_min = (reset - Utc::now()).num_minutes();
        assert!(
            (235..=245).contains(&remaining_min),
            "expected ~240min remaining (anchored at ts4 = now - 1h), got {remaining_min}"
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn fixed_window_5h_with_no_usage_returns_zero_and_full_window_remaining() {
        let dir = temp_data_dir("fixed-5h-empty");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("empty"))
            .expect("account should be created");

        let usage = db.account_usage("empty").expect("usage should load");
        assert_cost(usage.window_5h, 0.0);
        // 没用过：倒计时为 None（前端显示"5h0min"由默认值决定）
        assert!(usage.resets_in_5h.is_none());

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn month_window_accumulates_from_purchase_date_to_expires_on() {
        let dir = temp_data_dir("month-window");
        let db = Database::open(dir.clone()).expect("db should open");
        let mut acct = account("monthly");
        acct.purchase_date = "2026-07-01".into();
        db.create_account(&acct).expect("account should be created");

        // 模拟一条历史成功请求（任何时间都算，月窗口从 purchase_date 累计）
        finalize_success(&db, "monthly", 5.0, Utc::now());

        let usage = db.account_usage("monthly").expect("usage should load");
        assert_cost(usage.window_month, 5.0);
        let reset = usage
            .resets_in_month
            .expect("month window reset should be purchase_date + 1 month");
        // 2026-07-01 + 1 自然月 = 2026-08-01 00:00
        let expected = DateTime::parse_from_rfc3339("2026-08-01T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert!(
            (reset - expected).num_seconds().abs() < 86400,
            "expected ~2026-08-01, got {reset}"
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn manual_calibrate_5h_window_sets_started_at_and_cost_offset() {
        let dir = temp_data_dir("calibrate-5h");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("calib"))
            .expect("account should be created");

        // 用户在别处已用 50%，距上游重置还剩 3 小时
        db.calibrate_account_usage("calib", UsageWindowKind::FiveHours, 50.0, Some(180), 12.0)
            .expect("calibrate should save");

        let usage = db.account_usage("calib").expect("usage should load");
        // 5h 限额 12.0，50% = 6.0
        assert_cost(usage.window_5h, 6.0);
        let reset = usage
            .resets_in_5h
            .expect("5h window reset should be set after manual calibrate");
        let remaining_min = (reset - Utc::now()).num_minutes();
        assert!(
            (175..=185).contains(&remaining_min),
            "expected ~180min remaining, got {remaining_min}"
        );

        // 后续网关内的请求累加到偏移之上
        finalize_success(&db, "calib", 1.0, Utc::now());
        let usage = db.account_usage("calib").expect("usage should reload");
        assert_cost(usage.window_5h, 7.0);

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn calibrate_subtracts_existing_window_usage_from_offset() {
        // 回归测试：活跃账号（窗口内已有 forward_logs）校准时，
        // offset 必须 = target_cost - actual_cost，否则 compute_fixed_window
        // 返回 offset + actual_cost，显示百分比会高于用户输入。
        let dir = temp_data_dir("calibrate-with-usage");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("active"))
            .expect("account should be created");

        // 1 小时前已用 $3（落在 5h 窗口内）
        let ts = Utc::now() - Duration::hours(1);
        finalize_success(&db, "active", 3.0, ts);

        // 用户说"我在别处用到了 50%"（5h 限额 12.0 → target_cost = 6.0）
        // 期望：offset = 6.0 - 3.0 = 3.0，compute_fixed_window 返回 3.0 + 3.0 = 6.0 = 50%
        // 修复前 bug：offset = 6.0，compute_fixed_window 返回 6.0 + 3.0 = 9.0 = 75%
        // 用 resets_in_minutes=180 让新窗口的 started_at = now + 3h - 5h = now - 2h，
        // 把 1 小时前的 log 稳稳包含进窗口（避开 finalize 与 calibrate 之间的微秒级时序差）。
        db.calibrate_account_usage("active", UsageWindowKind::FiveHours, 50.0, Some(180), 12.0)
            .expect("calibrate should save with existing usage");
        let usage = db.account_usage("active").expect("usage should load");
        assert_cost(usage.window_5h, 6.0);

        // 后续请求继续累加：offset=3.0 + actual=3.0 + new=2.0 = 8.0
        finalize_success(&db, "active", 2.0, Utc::now());
        let usage = db.account_usage("active").expect("usage should reload");
        assert_cost(usage.window_5h, 8.0);

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn calibrate_below_actual_usage_allows_negative_offset() {
        // 回归测试（Bug 1.5）：用户校准的百分比低于窗口内实际 cost 时，offset 允许为负数，
        // 让 compute_fixed_window 返回 offset + actual = target_cost，与用户输入一致。
        // 之前 max(0, target - actual) 钳制 + schema CHECK (offset >= 0) 约束让向左拉
        // 滑块时被锁死在实际 cost 对应的百分比（9.0 / 12.0 * 100 = 75%，对应用户看到的 40.2%）。
        let dir = temp_data_dir("calibrate-below-usage");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("clamp"))
            .expect("account should be created");

        // 已用 $9
        let ts = Utc::now() - Duration::hours(1);
        finalize_success(&db, "clamp", 9.0, ts);

        // 用户校准到 20%（target_cost = 2.4，但实际已用 9.0）
        // offset = 2.4 - 9.0 = -6.6；compute_fixed_window 返回 -6.6 + 9.0 = 2.4 = 20%。
        // 用 resets_in_minutes=180 让新窗口的 started_at = now - 2h，把 1 小时前的
        // $9 log 稳稳包含进窗口（避开 finalize 与 calibrate 之间的微秒级时序差）。
        db.calibrate_account_usage("clamp", UsageWindowKind::FiveHours, 20.0, Some(180), 12.0)
            .expect("calibrate below actual usage should allow negative offset");
        let usage = db.account_usage("clamp").expect("usage should load");
        // 显示的 cost = offset(-6.6) + actual(9.0) = 2.4（用户输入的 20%）
        assert_cost(usage.window_5h, 2.4);

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn calibrate_month_window_writes_offset_without_started_at() {
        // 回归测试（Bug 2）：月窗口必须支持手动校准。
        // 月窗口不写 started_at 列（起点固定为 purchase_date），只更新 cost_offset。
        // resets_in_minutes 被忽略——窗口由 purchase_date/expires_on 决定。
        let dir = temp_data_dir("calibrate-month");
        let db = Database::open(dir.clone()).expect("db should open");
        let mut acct = account("monthly-calib");
        acct.purchase_date = "2026-07-01".into();
        db.create_account(&acct).expect("account should be created");

        // 已用 $5（落在月窗口内：purchase_date 00:00 起）
        finalize_success(&db, "monthly-calib", 5.0, Utc::now());

        // 用户校准到 50%（月限额 100.0 → target_cost = 50.0）
        // 期望：offset = 50.0 - 5.0 = 45.0；compute_month_window 返回 45.0 + 5.0 = 50.0 = 50%。
        db.calibrate_account_usage("monthly-calib", UsageWindowKind::Month, 50.0, None, 100.0)
            .expect("month window calibrate should save");
        let usage = db
            .account_usage("monthly-calib")
            .expect("usage should load");
        assert_cost(usage.window_month, 50.0);
        // resets_in_month 仍是 purchase_date + 1 自然月（不受 resets_in_minutes 影响）
        let reset = usage
            .resets_in_month
            .expect("month window reset should be purchase_date + 1 month");
        let expected = DateTime::parse_from_rfc3339("2026-08-01T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert!(
            (reset - expected).num_seconds().abs() < 86400,
            "expected ~2026-08-01, got {reset}"
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn changing_purchase_date_resets_month_calibration_offset() {
        let dir = temp_data_dir("month-renewal-reset");
        let db = Database::open(dir.clone()).expect("db should open");
        let new_purchase_date = local_today();
        let old_purchase_date = (Local::now().date_naive() - Duration::days(10))
            .format("%Y-%m-%d")
            .to_string();
        let mut acct = account("monthly-renewal");
        acct.purchase_date = old_purchase_date;
        db.create_account(&acct).expect("account should be created");

        finalize_success(&db, "monthly-renewal", 5.0, Utc::now() - Duration::days(2));
        db.calibrate_account_usage("monthly-renewal", UsageWindowKind::Month, 0.0, None, 100.0)
            .expect("month calibration should save a negative offset");
        assert_cost(
            db.account_usage("monthly-renewal")
                .expect("usage should load")
                .window_month,
            0.0,
        );

        db.update_account(
            "monthly-renewal",
            &AccountUpdate {
                name: None,
                username: None,
                password: None,
                key: None,
                enabled: None,
                referral_code: None,
                purchase_date: Some(new_purchase_date),
            },
            None,
            None,
        )
        .expect("purchase date should update");
        let offset: f64 = db
            .conn
            .query_row(
                "SELECT usage_month_window_cost_offset FROM accounts WHERE id = ?1",
                ["monthly-renewal"],
                |row| row.get(0),
            )
            .expect("month offset should load");
        assert_cost(offset, 0.0);
        assert_cost(
            db.account_usage("monthly-renewal")
                .expect("renewed usage should load")
                .window_month,
            0.0,
        );

        finalize_success(&db, "monthly-renewal", 2.0, Utc::now());
        assert_cost(
            db.account_usage("monthly-renewal")
                .expect("new cycle usage should load")
                .window_month,
            2.0,
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn replacing_key_clears_auth_error_but_other_updates_preserve_it() {
        let dir = temp_data_dir("auth-error-key-replacement");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("auth-failed"))
            .expect("account should be created");
        let old_key_cipher = db
            .get_account("auth-failed")
            .expect("account should load")
            .expect("account should exist")
            .key_cipher;
        db.set_account_auth_error("auth-failed", Some("upstream auth error 401"))
            .expect("auth error should save");

        let rename = AccountUpdate {
            name: Some("renamed".into()),
            username: None,
            password: None,
            key: None,
            enabled: None,
            referral_code: None,
            purchase_date: None,
        };
        db.update_account("auth-failed", &rename, None, None)
            .expect("non-key update should save");
        assert!(
            db.get_account("auth-failed")
                .expect("account should load")
                .expect("account should exist")
                .auth_error
                .is_some()
        );

        let no_fields = AccountUpdate {
            name: None,
            username: None,
            password: None,
            key: None,
            enabled: None,
            referral_code: None,
            purchase_date: None,
        };
        db.update_account("auth-failed", &no_fields, Some("replacement-cipher"), None)
            .expect("key replacement should save");
        assert!(
            db.get_account("auth-failed")
                .expect("account should load")
                .expect("account should exist")
                .auth_error
                .is_none()
        );

        assert!(
            !db.set_account_auth_error_if_key_matches(
                "auth-failed",
                &old_key_cipher,
                Some("late old-key 401"),
            )
            .expect("stale auth response should be ignored")
        );
        assert!(
            db.get_account("auth-failed")
                .expect("account should load")
                .expect("account should exist")
                .auth_error
                .is_none(),
            "a delayed 401 from the old key must not break its replacement"
        );

        assert!(
            db.set_account_auth_error_if_key_matches(
                "auth-failed",
                "replacement-cipher",
                Some("new-key auth error"),
            )
            .expect("current-key auth response should save")
        );
        assert!(
            !db.set_account_auth_error_if_key_matches("auth-failed", &old_key_cipher, None)
                .expect("stale success response should be ignored")
        );
        assert_eq!(
            db.get_account("auth-failed")
                .expect("account should load")
                .expect("account should exist")
                .auth_error
                .as_deref(),
            Some("new-key auth error"),
            "a delayed success from the old key must not recover its replacement"
        );
        assert!(
            db.set_account_auth_error_if_key_matches("auth-failed", "replacement-cipher", None)
                .expect("current-key success should clear auth state")
        );

        let stale_cooldown = Utc::now() + Duration::days(3);
        assert!(
            !db.set_account_rate_limit_if_key_matches(
                "auth-failed",
                &old_key_cipher,
                stale_cooldown,
                "late old-key 429",
                None,
            )
            .expect("stale rate limit should be ignored")
        );
        let stored = db
            .get_account("auth-failed")
            .expect("account should load")
            .expect("account should exist");
        assert!(stored.cooldown_until.is_none());
        assert!(stored.last_error.is_none());

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn calibrate_rejects_reset_outside_fixed_window_without_panicking() {
        let dir = temp_data_dir("calibrate-reset-bounds");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("reset-bounds"))
            .expect("account should be created");

        for (window, minutes) in [
            (UsageWindowKind::FiveHours, -1),
            (UsageWindowKind::FiveHours, 301),
            (UsageWindowKind::Week, 10_081),
            (UsageWindowKind::FiveHours, i64::MAX),
        ] {
            assert!(
                db.calibrate_account_usage("reset-bounds", window, 50.0, Some(minutes), 100.0,)
                    .is_err(),
                "{window:?} should reject {minutes} minutes"
            );
        }

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn soonest_reset_is_minimum_of_each_accounts_latest_active_cooldown() {
        let dir = temp_data_dir("soonest-account-reset");
        let db = Database::open(dir.clone()).expect("db should open");
        for id in ["first", "second"] {
            db.create_account(&account(id))
                .expect("account should be created");
        }

        let now = Utc::now();
        let first_early = now + Duration::hours(1);
        let first_latest = now + Duration::hours(4);
        let second_latest = now + Duration::hours(2);
        db.set_account_rate_limit(
            "first",
            first_early,
            "5-hour usage limit reached",
            Some(UsageWindowKind::FiveHours),
        )
        .expect("first short cooldown should save");
        db.set_account_rate_limit(
            "first",
            first_latest,
            "weekly usage limit reached",
            Some(UsageWindowKind::Week),
        )
        .expect("first long cooldown should save");
        db.set_account_rate_limit("second", second_latest, "unknown rate limit", None)
            .expect("second cooldown should save");

        let reset = db
            .soonest_cooldown_reset()
            .expect("reset query should work")
            .expect("a reset should exist");
        assert!((reset - second_latest).num_seconds().abs() < 2);

        db.set_account_auth_error("second", Some("upstream auth error 401"))
            .expect("auth breaker should save");
        let reset = db
            .soonest_cooldown_reset()
            .expect("reset query should work")
            .expect("an eligible reset should exist");
        assert!((reset - first_latest).num_seconds().abs() < 2);

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn forward_log_time_filter_compares_rfc3339_offsets_by_instant() {
        let dir = temp_data_dir("forward-log-offset-filter");
        let db = Database::open(dir.clone()).expect("db should open");
        db.conn
            .execute(
                "INSERT INTO forward_logs
                 (timestamp, model, account_id, account_name, status, cost)
                 VALUES (?1, 'inside', 'a', 'a', 'success', 1)",
                ["2026-07-17T04:15:00Z"],
            )
            .expect("inside log should save");
        db.conn
            .execute(
                "INSERT INTO forward_logs
                 (timestamp, model, account_id, account_name, status, cost)
                 VALUES (?1, 'outside', 'a', 'a', 'success', 2)",
                ["2026-07-17T03:30:00Z"],
            )
            .expect("outside log should save");

        let page = db
            .query_forward_logs(ForwardLogQueryOptions {
                limit: 20,
                offset: 0,
                status: None,
                account_id: None,
                model: None,
                request_id: None,
                start_time: Some("2026-07-17T12:00:00+08:00"),
                end_time: Some("2026-07-17T12:30:00+08:00"),
                sort_by: Some("cost"),
                sort_order: Some("asc"),
            })
            .expect("offset filter should query");
        assert_eq!(page.summary.total_requests, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].model, "inside");

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn forward_logs_can_sort_by_attempt() {
        let dir = temp_data_dir("forward-log-attempt-sort");
        let db = Database::open(dir.clone()).expect("db should open");
        for attempt in [2, 1] {
            db.conn
                .execute(
                    "INSERT INTO forward_logs
                     (timestamp, model, account_id, account_name, status, cost, attempt)
                     VALUES ('2026-07-23T00:00:00Z', ?1, 'a', 'a', 'client_error', 0, ?2)",
                    params![format!("attempt-{attempt}"), attempt],
                )
                .expect("forward log should save");
        }

        let page = db
            .query_forward_logs(ForwardLogQueryOptions {
                limit: 20,
                offset: 0,
                status: None,
                account_id: None,
                model: None,
                request_id: None,
                start_time: None,
                end_time: None,
                sort_by: Some("attempt"),
                sort_order: Some("asc"),
            })
            .expect("attempt sort should query");
        assert_eq!(
            page.items
                .iter()
                .filter_map(|log| log.attempt)
                .collect::<Vec<_>>(),
            [1, 2]
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }
}
