//! Neutral SQLite statement helpers for forward and gateway logs.
//!
//! Each helper runs exactly one explicit v26 statement on a caller-owned
//! connection and returns the raw rusqlite result. Callers own timestamps,
//! diagnostics serialization, cost policy, redaction, and transactions.

use rusqlite::{Connection, params};

const INSERT_FORWARD_LOG_SQL: &str = "INSERT INTO forward_logs
             (timestamp, model, account_id, account_name, client_key_id, client_key_name,
              route_account_id, provider_id, offering_id, credential_account_id,
              status, http_status, route,
              prompt_tokens, completion_tokens, cached_tokens, cache_creation_tokens, cost,
              raw_cost_usd, quota_debit, effective_paid_cost_usd,
              pricing_revision_id, quota_multiplier, local_adjustment_multiplier,
              service_tier, cost_state, error_message, request_id, attempt,
              error_source, error_stage, duration_ms, diagnostic_json,
              requested_model, resolved_alias, upstream_model,
              native_cost_value, native_cost_unit, native_cost_currency)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
                     ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39)";

const UPDATE_FORWARD_LOG_SQL: &str = "UPDATE forward_logs
             SET status = ?2,
                 http_status = COALESCE(?3, http_status),
                 prompt_tokens = ?4,
                 completion_tokens = ?5,
                 cached_tokens = ?6,
                 cache_creation_tokens = ?7,
                 cost = ?8,
                 raw_cost_usd = ?9,
                 quota_debit = ?10,
                 effective_paid_cost_usd = ?11,
                 pricing_revision_id = ?12,
                 quota_multiplier = ?13,
                 local_adjustment_multiplier = ?14,
                 service_tier = ?15,
                 cost_state = ?16,
                 error_message = COALESCE(?17, error_message),
                 error_source = COALESCE(?18, error_source),
                 error_stage = COALESCE(?19, error_stage),
                 duration_ms = COALESCE(?20, duration_ms),
                 diagnostic_json = COALESCE(?21, diagnostic_json),
                 native_cost_value = ?22,
                 native_cost_unit = ?23,
                 native_cost_currency = ?24
             WHERE id = ?1";

const PATCH_FORWARD_LOG_IDENTITY_SQL: &str = "UPDATE forward_logs SET
                requested_model = ?2,
                resolved_alias = ?3,
                upstream_model = ?4,
                native_cost_value = ?5,
                native_cost_unit = ?6,
                native_cost_currency = ?7
             WHERE id = ?1";

const INSERT_GATEWAY_LOG_SQL: &str = "INSERT INTO gateway_logs
             (level, category, message, created_at, request_id, attempt,
              error_source, error_stage, duration_ms, diagnostic_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

/// Borrowed v26 `forward_logs` insert payload. Field order matches the
/// 39-column statement binding order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForwardLogInsertRow<'a> {
    pub timestamp: &'a str,
    pub model: &'a str,
    pub account_id: &'a str,
    pub account_name: &'a str,
    pub client_key_id: Option<&'a str>,
    pub client_key_name: Option<&'a str>,
    pub route_account_id: Option<&'a str>,
    pub provider_id: Option<&'a str>,
    pub offering_id: Option<&'a str>,
    pub credential_account_id: Option<&'a str>,
    pub status: &'a str,
    pub http_status: Option<i32>,
    pub route: &'a str,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost: f64,
    pub raw_cost_usd: Option<f64>,
    pub quota_debit: Option<f64>,
    pub effective_paid_cost_usd: Option<f64>,
    pub pricing_revision_id: Option<&'a str>,
    pub quota_multiplier: Option<f64>,
    pub local_adjustment_multiplier: Option<f64>,
    pub service_tier: Option<&'a str>,
    pub cost_state: &'a str,
    pub error_message: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub attempt: Option<i64>,
    pub error_source: Option<&'a str>,
    pub error_stage: Option<&'a str>,
    pub duration_ms: Option<i64>,
    pub diagnostic_json: Option<&'a str>,
    pub requested_model: Option<&'a str>,
    pub resolved_alias: Option<&'a str>,
    pub upstream_model: Option<&'a str>,
    pub native_cost_value: Option<f64>,
    pub native_cost_unit: Option<&'a str>,
    pub native_cost_currency: Option<&'a str>,
}

/// Borrowed v26 `forward_logs` finalize payload. `None` http/error/diagnostic
/// fields leave the stored value unchanged via SQL `COALESCE`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForwardLogUpdateRow<'a> {
    pub id: i64,
    pub status: &'a str,
    pub http_status: Option<i32>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost: f64,
    pub raw_cost_usd: Option<f64>,
    pub quota_debit: Option<f64>,
    pub effective_paid_cost_usd: Option<f64>,
    pub pricing_revision_id: Option<&'a str>,
    pub quota_multiplier: Option<f64>,
    pub local_adjustment_multiplier: Option<f64>,
    pub service_tier: Option<&'a str>,
    pub cost_state: &'a str,
    pub error_message: Option<&'a str>,
    pub error_source: Option<&'a str>,
    pub error_stage: Option<&'a str>,
    pub duration_ms: Option<i64>,
    pub diagnostic_json: Option<&'a str>,
    pub native_cost_value: Option<f64>,
    pub native_cost_unit: Option<&'a str>,
    pub native_cost_currency: Option<&'a str>,
}

/// Borrowed six-column native identity patch for `forward_logs`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForwardLogIdentityPatch<'a> {
    pub id: i64,
    pub requested_model: Option<&'a str>,
    pub resolved_alias: Option<&'a str>,
    pub upstream_model: Option<&'a str>,
    pub native_cost_value: Option<f64>,
    pub native_cost_unit: Option<&'a str>,
    pub native_cost_currency: Option<&'a str>,
}

/// Borrowed v26 `gateway_logs` insert payload, including optional diagnostics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatewayLogInsertRow<'a> {
    pub level: &'a str,
    pub category: &'a str,
    pub message: &'a str,
    pub created_at: &'a str,
    pub request_id: Option<&'a str>,
    pub attempt: Option<i64>,
    pub error_source: Option<&'a str>,
    pub error_stage: Option<&'a str>,
    pub duration_ms: Option<i64>,
    pub diagnostic_json: Option<&'a str>,
}

/// Insert one `forward_logs` row and return its auto-assigned id.
pub fn insert_forward_log(
    conn: &Connection,
    row: &ForwardLogInsertRow<'_>,
) -> rusqlite::Result<i64> {
    conn.execute(
        INSERT_FORWARD_LOG_SQL,
        params![
            row.timestamp,
            row.model,
            row.account_id,
            row.account_name,
            row.client_key_id,
            row.client_key_name,
            row.route_account_id,
            row.provider_id,
            row.offering_id,
            row.credential_account_id,
            row.status,
            row.http_status,
            row.route,
            row.prompt_tokens,
            row.completion_tokens,
            row.cached_tokens,
            row.cache_creation_tokens,
            row.cost,
            row.raw_cost_usd,
            row.quota_debit,
            row.effective_paid_cost_usd,
            row.pricing_revision_id,
            row.quota_multiplier,
            row.local_adjustment_multiplier,
            row.service_tier,
            row.cost_state,
            row.error_message,
            row.request_id,
            row.attempt,
            row.error_source,
            row.error_stage,
            row.duration_ms,
            row.diagnostic_json,
            row.requested_model,
            row.resolved_alias,
            row.upstream_model,
            row.native_cost_value,
            row.native_cost_unit,
            row.native_cost_currency,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Finalize one `forward_logs` row. Returns the number of affected rows.
pub fn update_forward_log(
    conn: &Connection,
    row: &ForwardLogUpdateRow<'_>,
) -> rusqlite::Result<usize> {
    conn.execute(
        UPDATE_FORWARD_LOG_SQL,
        params![
            row.id,
            row.status,
            row.http_status,
            row.prompt_tokens,
            row.completion_tokens,
            row.cached_tokens,
            row.cache_creation_tokens,
            row.cost,
            row.raw_cost_usd,
            row.quota_debit,
            row.effective_paid_cost_usd,
            row.pricing_revision_id,
            row.quota_multiplier,
            row.local_adjustment_multiplier,
            row.service_tier,
            row.cost_state,
            row.error_message,
            row.error_source,
            row.error_stage,
            row.duration_ms,
            row.diagnostic_json,
            row.native_cost_value,
            row.native_cost_unit,
            row.native_cost_currency,
        ],
    )
}

/// Patch the six native identity columns. Returns the number of affected rows.
pub fn patch_forward_log_identity(
    conn: &Connection,
    row: &ForwardLogIdentityPatch<'_>,
) -> rusqlite::Result<usize> {
    conn.execute(
        PATCH_FORWARD_LOG_IDENTITY_SQL,
        params![
            row.id,
            row.requested_model,
            row.resolved_alias,
            row.upstream_model,
            row.native_cost_value,
            row.native_cost_unit,
            row.native_cost_currency,
        ],
    )
}

/// Insert one `gateway_logs` row. Returns the number of affected rows.
pub fn insert_gateway_log(
    conn: &Connection,
    row: &GatewayLogInsertRow<'_>,
) -> rusqlite::Result<usize> {
    conn.execute(
        INSERT_GATEWAY_LOG_SQL,
        params![
            row.level,
            row.category,
            row.message,
            row.created_at,
            row.request_id,
            row.attempt,
            row.error_source,
            row.error_stage,
            row.duration_ms,
            row.diagnostic_json,
        ],
    )
}

#[cfg(test)]
mod tests;
