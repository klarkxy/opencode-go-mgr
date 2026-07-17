use crate::models::*;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{
    Connection, OptionalExtension, Row, params, params_from_iter,
    types::{Type, Value},
};
use std::{collections::HashSet, fmt, path::PathBuf};

pub struct Database {
    conn: Connection,
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
        let version: i32 = tx
            .query_row(
                "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if version < 1 {
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS accounts (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    key_cipher TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    referral_code TEXT,
                    recharge_date TEXT,
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

        tx.commit()?;
        Ok(())
    }

    // Accounts
    pub fn create_account(&self, account: &Account) -> Result<()> {
        let purchase_date = if account.purchase_date.trim().is_empty() {
            local_today()
        } else {
            normalize_purchase_date(&account.purchase_date)?
        };
        self.conn.execute(
            "INSERT INTO accounts (id, name, username, password_cipher, key_cipher, enabled, referral_code, recharge_date, sort_order, cooldown_until, last_error, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM accounts), ?9, ?10, ?11, ?12)",
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
                account.last_error,
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
        let key = key_cipher.unwrap_or(&existing.key_cipher);
        let password = match password_cipher {
            Some("") => None,
            Some(s) => Some(s.to_string()),
            None => existing.password_cipher.clone(),
        };

        self.conn.execute(
            "UPDATE accounts SET name = ?1, username = ?2, password_cipher = ?3, key_cipher = ?4, enabled = ?5, referral_code = ?6, recharge_date = ?7, updated_at = ?8
             WHERE id = ?9",
            params![
                name,
                username,
                password,
                key,
                enabled as i32,
                referral_code,
                purchase_date,
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
            "SELECT id, name, username, password_cipher, key_cipher, enabled, referral_code, recharge_date, cooldown_until, last_error, created_at, updated_at FROM accounts WHERE id = ?1"
        )?;
        let account = stmt.query_row([id], account_from_row).optional()?;
        Ok(account)
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, username, password_cipher, key_cipher, enabled, referral_code, recharge_date, cooldown_until, last_error, created_at, updated_at FROM accounts ORDER BY sort_order ASC, created_at ASC, id ASC"
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
        self.conn.execute(
            "INSERT INTO gateway_logs (level, category, message, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![level, category, message, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Insert a forward_logs row. Returns the auto-assigned row id.
    pub fn log_forward(&self, log: &ForwardLog) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO forward_logs
             (timestamp, model, account_id, account_name, status, http_status, prompt_tokens, completion_tokens, cached_tokens, cost, error_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                log.cost,
                log.error_message,
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
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE forward_logs
             SET status = ?2,
                 http_status = COALESCE(?3, http_status),
                 prompt_tokens = ?4,
                 completion_tokens = ?5,
                 cached_tokens = ?6,
                 cost = ?7,
                 error_message = COALESCE(?8, error_message)
             WHERE id = ?1",
            params![
                id,
                status,
                http_status,
                metrics.prompt_tokens,
                metrics.completion_tokens,
                metrics.cached_tokens,
                metrics.cost,
                error_message
            ],
        )?;
        Ok(())
    }

    pub fn list_gateway_logs(&self, limit: i64) -> Result<Vec<GatewayLog>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, level, category, message, created_at FROM gateway_logs ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit], |row| {
            Ok(GatewayLog {
                id: row.get(0)?,
                level: row.get(1)?,
                category: row.get(2)?,
                message: row.get(3)?,
                created_at: parse_datetime(row.get::<_, String>(4)?),
            })
        })?;
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
            "SELECT id, timestamp, model, account_id, account_name, status, http_status, prompt_tokens, completion_tokens, cached_tokens, cost, error_message
             FROM forward_logs ORDER BY id DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map([limit], forward_log_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    pub fn query_forward_logs(
        &self,
        limit: i64,
        offset: i64,
        status: Option<&str>,
        account_id: Option<&str>,
        model: Option<&str>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
    ) -> Result<ForwardLogPage> {
        let limit = limit.clamp(1, 200);
        let offset = offset.max(0);
        let (filter, filter_params) =
            forward_log_filter(status, account_id, model, start_time, end_time);
        let order_clause = forward_log_order(sort_by, sort_order);
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
            "SELECT id, timestamp, model, account_id, account_name, status, http_status, prompt_tokens, completion_tokens, cached_tokens, cost, error_message
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
        self.conn.execute(
            "UPDATE accounts SET cooldown_until = ?2, last_error = ?3, updated_at = ?4 WHERE id = ?1",
            params![
                id,
                until.map(|t| t.to_rfc3339()),
                err,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn clear_account_cooldown(&self, id: &str) -> Result<()> {
        self.set_account_cooldown(id, None, None)
    }

    /// Record a real upstream 429 and reset only the identified manual usage window.
    pub fn set_account_rate_limit(
        &self,
        id: &str,
        until: DateTime<Utc>,
        err: &str,
        window: Option<UsageWindowKind>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE accounts SET cooldown_until = ?2, last_error = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, until.to_rfc3339(), err, now],
        )?;
        if changed > 0 {
            if let Some(window) = window {
                tx.execute(usage_baseline_update_sql(window), params![id, 0.0, now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Among all enabled accounts, return the earliest `cooldown_until` in the future.
    /// `None` means no account is in cooldown.
    pub fn soonest_cooldown_reset(&self) -> Result<Option<DateTime<Utc>>> {
        let now = Utc::now().to_rfc3339();
        let res: Option<String> = self
            .conn
            .query_row(
                "SELECT MIN(cooldown_until) FROM accounts WHERE enabled = 1 AND cooldown_until IS NOT NULL AND cooldown_until > ?1",
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
    /// Calibrate one usage window and atomically snapshot all successful cost so far.
    /// Returns `false` when the account no longer exists.
    pub fn set_account_usage_baseline(
        &self,
        account_id: &str,
        window: UsageWindowKind,
        percent: f64,
    ) -> Result<bool> {
        let changed = self.conn.execute(
            usage_baseline_update_sql(window),
            params![account_id, percent, Utc::now().to_rfc3339()],
        )?;
        Ok(changed > 0)
    }

    pub fn account_usage(&self, account_id: &str) -> Result<UsageWindow> {
        let now = Utc::now();
        let five_hours_ago = (now - Duration::hours(5)).to_rfc3339();
        let week_ago = (now - Duration::days(7)).to_rfc3339();
        let month_ago = (now - Duration::days(30)).to_rfc3339();

        let baselines: [Option<(f64, f64)>; 3] = self
            .conn
            .query_row(
                "SELECT usage_5h_baseline_percent, usage_5h_anchor_success_cost,
                        usage_week_baseline_percent, usage_week_anchor_success_cost,
                        usage_month_baseline_percent, usage_month_anchor_success_cost
                 FROM accounts WHERE id = ?1",
                [account_id],
                |row| {
                    Ok([
                        row.get::<_, Option<f64>>(0)?.zip(row.get(1)?),
                        row.get::<_, Option<f64>>(2)?.zip(row.get(3)?),
                        row.get::<_, Option<f64>>(4)?.zip(row.get(5)?),
                    ])
                },
            )
            .optional()?
            .unwrap_or([None; 3]);

        let total_success_cost = if baselines.iter().any(Option::is_some) {
            self.conn.query_row(
                "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE account_id = ?1 AND status = 'success'",
                [account_id],
                |row| row.get(0),
            )?
        } else {
            0.0
        };

        let window_5h: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE account_id = ?1 AND status = 'success' AND timestamp > ?2",
                params![account_id, five_hours_ago],
                |row| row.get(0),
            )?;
        let window_week: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE account_id = ?1 AND status = 'success' AND timestamp > ?2",
                params![account_id, week_ago],
                |row| row.get(0),
            )?;
        let window_month: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE account_id = ?1 AND status = 'success' AND timestamp > ?2",
                params![account_id, month_ago],
                |row| row.get(0),
            )?;

        Ok(UsageWindow {
            account_id: account_id.to_string(),
            window_5h: effective_usage(window_5h, baselines[0], total_success_cost, 12.0),
            window_week: effective_usage(window_week, baselines[1], total_success_cost, 30.0),
            window_month: effective_usage(window_month, baselines[2], total_success_cost, 60.0),
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
            "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE status = 'success' AND timestamp > ?1",
            [&today_start],
            |row| row.get(0),
        )?;
        let week: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE status = 'success' AND timestamp > ?1",
            [&week_ago],
            |row| row.get(0),
        )?;
        let month: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE status = 'success' AND timestamp > ?1",
            [&month_ago],
            |row| row.get(0),
        )?;

        Ok((today, week, month))
    }

    /// Aggregate `forward_logs` into per-day, per-model cost buckets covering
    /// the last `days` calendar days (UTC). Rows with zero activity on a given
    /// day are omitted — the frontend synthesizes empty days so the x-axis
    /// never collapses. Only `status = 'success'` rows count, matching the
    /// convention of `total_usage` / `usage_for_account`.
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
             WHERE status = 'success' AND timestamp > ?1
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

fn usage_baseline_update_sql(window: UsageWindowKind) -> &'static str {
    match window {
        UsageWindowKind::FiveHours => {
            "UPDATE accounts
             SET usage_5h_baseline_percent = ?2,
                 usage_5h_anchor_success_cost = (SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE account_id = ?1 AND status = 'success'),
                 updated_at = ?3
             WHERE id = ?1"
        }
        UsageWindowKind::Week => {
            "UPDATE accounts
             SET usage_week_baseline_percent = ?2,
                 usage_week_anchor_success_cost = (SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE account_id = ?1 AND status = 'success'),
                 updated_at = ?3
             WHERE id = ?1"
        }
        UsageWindowKind::Month => {
            "UPDATE accounts
             SET usage_month_baseline_percent = ?2,
                 usage_month_anchor_success_cost = (SELECT COALESCE(SUM(cost), 0) FROM forward_logs WHERE account_id = ?1 AND status = 'success'),
                 updated_at = ?3
             WHERE id = ?1"
        }
    }
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
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> (String, Vec<Value>) {
    let mut filter = String::new();
    let mut params = Vec::new();
    for (clause, value) in [
        ("status = ?", status),
        ("account_id = ?", account_id),
        ("model = ?", model),
        ("timestamp >= ?", start_time),
        ("timestamp <= ?", end_time),
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
        Some("prompt_tokens") => "prompt_tokens",
        Some("completion_tokens") => "completion_tokens",
        Some("cached_tokens") => "cached_tokens",
        Some("cost") => "cost",
        Some("model") => "model",
        Some("status") => "status",
        _ => "id",
    };
    let direction = if sort_order == Some("asc") { "ASC" } else { "DESC" };
    format!("ORDER BY {column} {direction}, id DESC")
}

fn forward_log_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ForwardLog> {
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
        cost: row.get(10)?,
        error_message: row.get(11)?,
    })
}

fn account_from_row(row: &Row<'_>) -> rusqlite::Result<Account> {
    let purchase_date = row.get::<_, String>(7)?;
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
        last_error: row.get(9)?,
        created_at: parse_datetime(row.get::<_, String>(10)?),
        updated_at: parse_datetime(row.get::<_, String>(11)?),
    })
}

fn migration_fallback_purchase_date(created_at: &str) -> Result<String> {
    let created_at = DateTime::parse_from_rfc3339(created_at).map_err(|error| {
        anyhow::anyhow!("invalid account created_at {created_at:?} during v5 migration: {error}")
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
            last_error: None,
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
            cost,
            error_message: None,
        }
    }

    fn assert_cost(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
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
        assert_eq!(version, 5);
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
    fn account_creation_defaults_dates_and_appends_to_saved_order() {
        let dir = temp_data_dir("create-order");
        let db = Database::open(dir.clone()).expect("db should open");
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
    fn manual_baselines_add_only_later_success_cost_per_window() {
        let dir = temp_data_dir("manual-baseline");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("manual"))
            .expect("account should be created");
        db.log_forward(&forward_log("manual", "success", 2.0))
            .expect("initial cost should be logged");
        let streaming_id = db
            .log_forward(&forward_log("manual", "streaming", 0.0))
            .expect("stream should be logged");

        assert!(
            db.set_account_usage_baseline("manual", UsageWindowKind::FiveHours, 50.0)
                .expect("5h baseline should save")
        );
        assert!(
            db.set_account_usage_baseline("manual", UsageWindowKind::Week, 25.0)
                .expect("week baseline should save")
        );
        db.update_forward_log(
            streaming_id,
            "success",
            None,
            ForwardMetrics {
                cost: 1.0,
                ..ForwardMetrics::default()
            },
            None,
        )
        .expect("stream should finalize");

        let usage = db.account_usage("manual").expect("usage should load");
        assert_cost(usage.window_5h, 7.0);
        assert_cost(usage.window_week, 8.5);
        assert_cost(usage.window_month, 3.0);

        db.set_account_usage_baseline("manual", UsageWindowKind::FiveHours, 100.0)
            .expect("5h baseline should update");
        db.log_forward(&forward_log("manual", "success", 5.0))
            .expect("later cost should be logged");
        let usage = db.account_usage("manual").expect("usage should reload");
        assert_cost(usage.window_5h, 12.0);
        assert_cost(usage.window_week, 13.5);
        assert_cost(usage.window_month, 8.0);
        assert!(
            !db.set_account_usage_baseline("missing", UsageWindowKind::Month, 50.0)
                .expect("missing account should not be an SQL error")
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }

    #[test]
    fn known_rate_limit_resets_only_its_window() {
        let dir = temp_data_dir("rate-limit-window");
        let db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("limited"))
            .expect("account should be created");
        db.set_account_usage_baseline("limited", UsageWindowKind::FiveHours, 40.0)
            .expect("5h baseline should save");
        db.set_account_usage_baseline("limited", UsageWindowKind::Week, 70.0)
            .expect("week baseline should save");

        db.set_account_rate_limit(
            "limited",
            Utc::now() + Duration::hours(1),
            "weekly quota",
            Some(UsageWindowKind::Week),
        )
        .expect("known rate limit should save");
        let usage = db.account_usage("limited").expect("usage should load");
        assert_cost(usage.window_5h, 4.8);
        assert_cost(usage.window_week, 0.0);

        db.set_account_usage_baseline("limited", UsageWindowKind::Month, 50.0)
            .expect("month baseline should save");
        let before = db.account_usage("limited").expect("usage should load");
        db.set_account_rate_limit(
            "limited",
            Utc::now() + Duration::minutes(5),
            "unknown rate limit",
            None,
        )
        .expect("unknown rate limit should still save cooldown");
        let after = db.account_usage("limited").expect("usage should reload");
        assert_cost(after.window_5h, before.window_5h);
        assert_cost(after.window_week, before.window_week);
        assert_cost(after.window_month, before.window_month);
        assert!(
            db.get_account("limited")
                .expect("account should load")
                .expect("account should exist")
                .cooldown_until
                .is_some()
        );

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }
}
