use crate::models::*;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;

pub struct Database {
    conn: Connection,
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

        let version: i32 = self
            .conn
            .query_row(
                "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if version < 1 {
            self.conn.execute_batch(
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
            self.conn.execute_batch(
                "ALTER TABLE accounts ADD COLUMN cooldown_until TEXT;
                ALTER TABLE accounts ADD COLUMN last_error TEXT;
                INSERT OR REPLACE INTO schema_version (version) VALUES (2);",
            )?;
        }

        if version < 3 {
            self.conn.execute_batch(
                "ALTER TABLE accounts ADD COLUMN username TEXT;
                ALTER TABLE accounts ADD COLUMN password_cipher TEXT;
                INSERT OR REPLACE INTO schema_version (version) VALUES (3);",
            )?;
        }

        Ok(())
    }

    // Accounts
    pub fn create_account(&self, account: &Account) -> Result<()> {
        self.conn.execute(
            "INSERT INTO accounts (id, name, username, password_cipher, key_cipher, enabled, referral_code, recharge_date, cooldown_until, last_error, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                account.id,
                account.name,
                account.username,
                account.password_cipher,
                account.key_cipher,
                account.enabled as i32,
                account.referral_code,
                account.recharge_date,
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
        let recharge_date = match &update.recharge_date {
            Some(s) if s.is_empty() => None,
            Some(s) => Some(s.clone()),
            None => existing.recharge_date.clone(),
        };
        let key = key_cipher.unwrap_or(&existing.key_cipher);
        let password = match password_cipher {
            Some(s) if s.is_empty() => None,
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
                recharge_date,
                Utc::now().to_rfc3339(),
                id,
            ],
        )?;
        Ok(())
    }

    /// LWW-aware variant: only overwrite fields when the wire's `updated_at` is
    /// strictly newer than the local row's. This is the merge used by
    /// admin::upsert_key and manual remote sync — both treat the wire payload
    /// as a snapshot from a peer machine and refuse to clobber a fresher local
    /// edit. Pass `wire_updated_at = None` to fall back to the unconditional
    /// update (legacy callers, e.g. local GUI mutations).
    pub fn merge_account_from_remote(
        &self,
        id: &str,
        wire: &crate::models::Account,
        wire_updated_at: chrono::DateTime<Utc>,
    ) -> Result<bool> {
        let existing = match self.get_account(id)? {
            Some(a) => a,
            None => {
                self.create_account(wire)?;
                return Ok(true);
            }
        };
        if existing.updated_at >= wire_updated_at {
            // ponytail: local is at least as new — preserve it. The wire's
            // payload is discarded wholesale. LWW-by-timestamp is enforced here
            // for both the admin and manual sync paths.
            return Ok(false);
        }
        // Wire is newer — apply its fields. New sync payloads carry plaintext
        // keys that callers re-encrypt locally before calling this helper.
        // Empty key_cipher means "metadata only; keep the local key".
        let upd = crate::models::AccountUpdate {
            name: Some(wire.name.clone()),
            username: wire.username.clone(),
            password: None,
            key: None,
            enabled: Some(wire.enabled),
            referral_code: wire.referral_code.clone(),
            recharge_date: wire.recharge_date.clone(),
        };
        // ponytail: optional fields are Option<String> in the DB; unwrap with
        // a fallback to the local value when the wire didn't send them.
        let key_cipher = if wire.key_cipher.is_empty() {
            existing.key_cipher.clone()
        } else {
            wire.key_cipher.clone()
        };
        let password_cipher = match &wire.password_cipher {
            Some(s) if s.is_empty() => existing.password_cipher.clone(),
            Some(s) => Some(s.clone()),
            None => existing.password_cipher.clone(),
        };
        self.conn.execute(
            "UPDATE accounts SET name = ?1, username = ?2, password_cipher = ?3, key_cipher = ?4, enabled = ?5, referral_code = ?6, recharge_date = ?7, updated_at = ?8
             WHERE id = ?9",
            params![
                upd.name.as_deref().unwrap_or(&existing.name),
                upd.username.or_else(|| existing.username.clone()),
                password_cipher,
                key_cipher,
                upd.enabled.unwrap_or(existing.enabled) as i32,
                upd.referral_code.or_else(|| existing.referral_code.clone()),
                upd.recharge_date.or_else(|| existing.recharge_date.clone()),
                wire_updated_at.to_rfc3339(),
                id,
            ],
        )?;
        Ok(true)
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
        let account = stmt
            .query_row([id], |row| {
                Ok(Account {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    username: row.get(2)?,
                    password_cipher: row.get(3)?,
                    key_cipher: row.get(4)?,
                    enabled: row.get::<_, i32>(5)? != 0,
                    referral_code: row.get(6)?,
                    recharge_date: row.get(7)?,
                    cooldown_until: row.get::<_, Option<String>>(8)?.map(parse_datetime),
                    last_error: row.get(9)?,
                    created_at: parse_datetime(row.get::<_, String>(10)?),
                    updated_at: parse_datetime(row.get::<_, String>(11)?),
                })
            })
            .optional()?;
        Ok(account)
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, username, password_cipher, key_cipher, enabled, referral_code, recharge_date, cooldown_until, last_error, created_at, updated_at FROM accounts ORDER BY created_at"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Account {
                id: row.get(0)?,
                name: row.get(1)?,
                username: row.get(2)?,
                password_cipher: row.get(3)?,
                key_cipher: row.get(4)?,
                enabled: row.get::<_, i32>(5)? != 0,
                referral_code: row.get(6)?,
                recharge_date: row.get(7)?,
                cooldown_until: row.get::<_, Option<String>>(8)?.map(parse_datetime),
                last_error: row.get(9)?,
                created_at: parse_datetime(row.get::<_, String>(10)?),
                updated_at: parse_datetime(row.get::<_, String>(11)?),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
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
        prompt_tokens: i64,
        completion_tokens: i64,
        cached_tokens: i64,
        cost: f64,
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
                prompt_tokens,
                completion_tokens,
                cached_tokens,
                cost,
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
        let rows = stmt.query_map([limit], |row| {
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
        })?;
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
    pub fn account_usage(&self, account_id: &str) -> Result<UsageWindow> {
        let now = Utc::now();
        let five_hours_ago = (now - Duration::hours(5)).to_rfc3339();
        let week_ago = (now - Duration::days(7)).to_rfc3339();
        let month_ago = (now - Duration::days(30)).to_rfc3339();

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
            window_5h,
            window_week,
            window_month,
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

fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|e| {
            eprintln!("error: failed to parse datetime '{}': {}, using now", s, e);
            Utc::now()
        })
}
