//! HTTP admin API: `/admin/*`. Binds to 127.0.0.1 only, gated by a single
//! Bearer token. Used by the Windows GUI to push account keys to a headless
//! Linux daemon, and by the daemon to surface them to its gateway.
//!
//! ponytail: 4 handlers, 1 middleware, no retries, no metrics, no audit log.
//! Reuses `state.core.encrypt_key` / `state.core.db` — never bypasses them.

use crate::models::Account;
use crate::state::CoreState;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::sync::oneshot;

#[derive(Clone)]
struct AdminToken(String);

async fn require_bearer(
    State(token): State<AdminToken>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == token.0)
        .unwrap_or(false);
    if !ok {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}

pub fn build_admin_router(state: CoreState, token: String) -> Router {
    Router::new()
        .route("/admin/health", get(health))
        .route("/admin/status", get(status))
        .route("/admin/keys", post(upsert_key))
        .route_layer(middleware::from_fn_with_state(
            AdminToken(token),
            require_bearer,
        ))
        .with_state(state)
    // ponytail: /admin/health is also behind the token — one less code path,
    // the GUI tests the same endpoint with the same Authorization header.
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

#[derive(Serialize)]
struct AdminStatus {
    version: &'static str,
    gateway: AdminGatewayStatus,
    accounts: AdminAccountStatus,
    usage: AdminUsageStatus,
    last_error: Option<String>,
}

#[derive(Serialize)]
struct AdminGatewayStatus {
    running: bool,
    port: u16,
    upstream_base_url: String,
    last_error: Option<String>,
}

#[derive(Serialize)]
struct AdminAccountStatus {
    total: usize,
    enabled: usize,
    disabled: usize,
    cooldown: usize,
    available: usize,
}

#[derive(Serialize)]
struct AdminUsageStatus {
    today_cost: f64,
    week_cost: f64,
    month_cost: f64,
}

async fn status(State(state): State<CoreState>) -> Result<Json<AdminStatus>, StatusCode> {
    let config = state.config();
    let running = state.gateway.lock().is_some();
    let now = Utc::now();
    let db = state.db.lock();
    let accounts = db
        .list_accounts()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (today_cost, week_cost, month_cost) = db
        .total_usage()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let last_gateway_error = db
        .latest_gateway_error()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let last_error = db
        .latest_error_summary()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    drop(db);

    let enabled = accounts.iter().filter(|a| a.enabled).count();
    let cooldown = accounts
        .iter()
        .filter(|a| a.enabled && a.cooldown_until.map(|until| until > now).unwrap_or(false))
        .count();
    let disabled = accounts.len().saturating_sub(enabled);
    let available = accounts
        .iter()
        .filter(|a| a.enabled && a.cooldown_until.map(|until| until <= now).unwrap_or(true))
        .count();

    Ok(Json(AdminStatus {
        version: env!("CARGO_PKG_VERSION"),
        gateway: AdminGatewayStatus {
            running,
            port: config.gateway_port,
            upstream_base_url: config.upstream_base_url,
            last_error: if running { None } else { last_gateway_error },
        },
        accounts: AdminAccountStatus {
            total: accounts.len(),
            enabled,
            disabled,
            cooldown,
            available,
        },
        usage: AdminUsageStatus {
            today_cost,
            week_cost,
            month_cost,
        },
        last_error,
    }))
}

#[derive(Deserialize)]
struct UpsertKeyDto {
    id: String,
    name: String,
    username: Option<String>,
    password: Option<String>,
    password_cipher: Option<String>,
    /// Preferred sync path: receiver encrypts plaintext with its local cipher.
    key: Option<String>,
    /// Legacy fallback for older peers. Existing local rows preserve their key
    /// when plaintext `key` is absent.
    key_cipher: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    referral_code: Option<String>,
    recharge_date: Option<String>,
    created_at: String,
    updated_at: String,
}

fn default_true() -> bool {
    true
}

async fn upsert_key(
    State(state): State<CoreState>,
    Json(input): Json<UpsertKeyDto>,
) -> Result<StatusCode, StatusCode> {
    let created_at = parse_rfc3339(&input.created_at).unwrap_or_else(chrono::Utc::now);
    let updated_at = parse_rfc3339(&input.updated_at).unwrap_or_else(chrono::Utc::now);
    let existing = {
        let db = state.db.lock();
        db.get_account(&input.id)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };
    let key_cipher = match input.key.as_deref() {
        Some(key) => state
            .encrypt_key(key)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        None if existing.is_none() => input.key_cipher.clone().ok_or(StatusCode::BAD_REQUEST)?,
        None => String::new(),
    };
    let password_cipher = match input.password.as_deref() {
        Some(password) if !password.is_empty() => Some(
            state
                .encrypt_key(password)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        ),
        Some(_) => None,
        None if existing.is_none() => input.password_cipher.clone(),
        None => Some(String::new()),
    };
    let account = Account {
        id: input.id.clone(),
        name: input.name,
        username: input.username,
        password_cipher,
        key_cipher,
        enabled: input.enabled,
        referral_code: input.referral_code,
        recharge_date: input.recharge_date,
        cooldown_until: None,
        last_error: None,
        created_at,
        updated_at,
    };
    let db = state.db.lock();
    let result = if existing.is_some() {
        db.merge_account_from_remote(&account.id, &account, updated_at)
            .map(|_| ())
    } else {
        db.create_account(&account)
    };
    match result {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn parse_rfc3339(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

/// Public handle the CLI holds to stop the admin server on shutdown.
pub struct AdminHandle {
    pub port: u16,
    shutdown: Option<oneshot::Sender<()>>,
}

pub async fn start_admin(
    state: CoreState,
    port: u16,
    token: String,
) -> anyhow::Result<AdminHandle> {
    let app = build_admin_router(state, token);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        if let Err(e) = server.await {
            eprintln!("admin server error: {}", e);
        }
    });
    Ok(AdminHandle {
        port,
        shutdown: Some(shutdown_tx),
    })
}

pub fn stop_admin(mut handle: AdminHandle) {
    if let Some(tx) = handle.shutdown.take() {
        let _ = tx.send(());
    }
}

/// Generate a 32-byte random bearer token, base64-encoded.
/// Stable across restarts when the caller persists the value to disk first.
pub fn generate_admin_token() -> String {
    let mut bytes = [0u8; 32];
    for chunk in bytes.chunks_mut(16) {
        let uuid = uuid::Uuid::new_v4();
        let src = uuid.as_bytes();
        let n = chunk.len().min(src.len());
        chunk[..n].copy_from_slice(&src[..n]);
    }
    B64.encode(bytes)
}
