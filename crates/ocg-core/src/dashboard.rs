use crate::models::*;
use crate::state::CoreState;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, Request, State},
    http::{Response as HttpResponse, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path as FsPath, PathBuf};

pub fn api_router(state: CoreState) -> Router<CoreState> {
    Router::new()
        .route("/accounts", get(list_accounts).post(create_account))
        .route(
            "/accounts/{id}",
            patch(update_account).delete(delete_account),
        )
        .route("/accounts/{id}/toggle", post(toggle_account))
        .route("/accounts/{id}/test", post(test_account))
        .route("/accounts/{id}/usage", get(account_usage))
        .route(
            "/accounts/{id}/reset-cooldown",
            post(reset_account_cooldown),
        )
        .route("/settings", get(get_settings).post(update_settings))
        .route(
            "/settings/regenerate-gateway-key",
            post(regenerate_gateway_key),
        )
        .route("/gateway/status", get(gateway_status))
        .route("/logs/gateway", get(gateway_logs))
        .route("/logs/forward", get(forward_logs))
        .route("/dashboard/summary", get(dashboard_summary))
        .route("/dashboard/daily-cost-by-model", get(daily_cost_by_model))
        .route("/remote/test", post(test_remote))
        .route("/remote/status", get(remote_status))
        .route("/remote/push", post(push_local_to_remote))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_dashboard_token,
        ))
}

pub fn dashboard_dir(state: &CoreState) -> PathBuf {
    if let Some(dir) = state.dashboard_dir() {
        return dir;
    }
    if let Ok(dir) = std::env::var("OCG_DASHBOARD_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.join("dist");
        }
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("dist")
}

pub async fn serve_index(State(state): State<CoreState>) -> impl IntoResponse {
    serve_file(dashboard_dir(&state).join("index.html")).await
}

pub async fn serve_asset(
    State(state): State<CoreState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    match asset_path(&dashboard_dir(&state), &path) {
        Some(path) => serve_file(path).await,
        None => StatusCode::BAD_REQUEST.into_response(),
    }
}

fn asset_path(dashboard_dir: &FsPath, raw: &str) -> Option<PathBuf> {
    if raw.contains('\\') || raw.contains(':') {
        return None;
    }
    let mut path = dashboard_dir.join("assets");
    for component in FsPath::new(raw).components() {
        match component {
            Component::Normal(part) => path.push(part),
            _ => return None,
        }
    }
    Some(path)
}

async fn serve_file(path: PathBuf) -> Response {
    match tokio::fs::read(&path).await {
        Ok(bytes) => HttpResponse::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                content_type(path.extension().and_then(|s| s.to_str())),
            )
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!("dashboard file not found: {}", path.display()),
        )
            .into_response(),
    }
}

fn content_type(ext: Option<&str>) -> &'static str {
    match ext.unwrap_or_default() {
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

async fn require_dashboard_token(
    State(state): State<CoreState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let header_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let ok = header_token
        .map(|token| token == state.dashboard_token)
        .unwrap_or(false);
    if ok {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct DashboardAccount {
    id: String,
    name: String,
    username: String,
    password: String,
    key: String,
    enabled: bool,
    cooldown_until: Option<String>,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
}

fn dashboard_account(state: &CoreState, account: Account) -> Result<DashboardAccount, ApiError> {
    let key = state
        .decrypt_key(&account.key_cipher)
        .map_err(ApiError::internal)?;
    let password = match account.password_cipher.as_deref() {
        Some(cipher) => state.decrypt_key(cipher).map_err(ApiError::internal)?,
        None => String::new(),
    };
    Ok(DashboardAccount {
        id: account.id,
        name: account.name,
        username: account.username.unwrap_or_default(),
        password,
        key,
        enabled: account.enabled,
        cooldown_until: account.cooldown_until.map(|t| t.to_rfc3339()),
        last_error: account.last_error,
        created_at: account.created_at.to_rfc3339(),
        updated_at: account.updated_at.to_rfc3339(),
    })
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn encrypted_optional(
    state: &CoreState,
    value: &Option<String>,
) -> Result<Option<String>, ApiError> {
    match value.as_deref().map(str::trim) {
        Some("") | None => Ok(None),
        Some(v) => state.encrypt_key(v).map(Some).map_err(ApiError::internal),
    }
}

async fn list_accounts(
    State(state): State<CoreState>,
) -> Result<Json<Vec<DashboardAccount>>, ApiError> {
    let accounts = state
        .db
        .lock()
        .list_accounts()
        .map_err(ApiError::internal)?;
    accounts
        .into_iter()
        .map(|account| dashboard_account(&state, account))
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

async fn create_account(
    State(state): State<CoreState>,
    Json(input): Json<AccountInput>,
) -> Result<Json<DashboardAccount>, ApiError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }
    if input.key.trim().is_empty() {
        return Err(ApiError::bad_request("key is required"));
    }
    let now = Utc::now();
    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        username: clean_optional(input.username),
        password_cipher: encrypted_optional(&state, &input.password)?,
        key_cipher: state
            .encrypt_key(input.key.trim())
            .map_err(ApiError::internal)?,
        enabled: true,
        referral_code: clean_optional(input.referral_code),
        recharge_date: clean_optional(input.recharge_date),
        cooldown_until: None,
        last_error: None,
        created_at: now,
        updated_at: now,
    };
    {
        let db = state.db.lock();
        db.create_account(&account).map_err(ApiError::internal)?;
        let _ = db.log_gateway(
            "info",
            "account",
            &format!("created account {}", account.name),
        );
    }
    dashboard_account(&state, account).map(Json)
}

async fn update_account(
    State(state): State<CoreState>,
    Path(id): Path<String>,
    Json(update): Json<AccountUpdate>,
) -> Result<Json<DashboardAccount>, ApiError> {
    let key_cipher = match update.key.as_deref().map(str::trim) {
        Some("") | None => None,
        Some(key) => Some(state.encrypt_key(key).map_err(ApiError::internal)?),
    };
    let password_cipher = match update.password.as_deref().map(str::trim) {
        Some("") => Some(String::new()),
        None => None,
        Some(password) => Some(state.encrypt_key(password).map_err(ApiError::internal)?),
    };
    {
        let db = state.db.lock();
        db.update_account(
            &id,
            &update,
            key_cipher.as_deref(),
            password_cipher.as_deref(),
        )
        .map_err(ApiError::internal)?;
        let _ = db.log_gateway("info", "account", &format!("updated account {}", id));
    }
    let account = state
        .db
        .lock()
        .get_account(&id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("account not found"))?;
    dashboard_account(&state, account).map(Json)
}

async fn delete_account(
    State(state): State<CoreState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut db = state.db.lock();
    db.delete_account(&id).map_err(ApiError::internal)?;
    let _ = db.log_gateway("info", "account", &format!("deleted account {}", id));
    Ok(StatusCode::NO_CONTENT)
}

async fn toggle_account(
    State(state): State<CoreState>,
    Path(id): Path<String>,
) -> Result<Json<DashboardAccount>, ApiError> {
    let account = state
        .db
        .lock()
        .get_account(&id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("account not found"))?;
    let update = AccountUpdate {
        name: None,
        username: None,
        password: None,
        key: None,
        enabled: Some(!account.enabled),
        referral_code: None,
        recharge_date: None,
    };
    {
        let db = state.db.lock();
        db.update_account(&id, &update, None, None)
            .map_err(ApiError::internal)?;
    }
    let account = state
        .db
        .lock()
        .get_account(&id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("account not found"))?;
    dashboard_account(&state, account).map(Json)
}

async fn test_account(
    State(state): State<CoreState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let account = state
        .db
        .lock()
        .get_account(&id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("account not found"))?;
    let key = state
        .decrypt_key(&account.key_cipher)
        .map_err(ApiError::internal)?;
    let masked = if key.len() > 8 && key.is_char_boundary(4) && key.is_char_boundary(key.len() - 4)
    {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else {
        "***".to_string()
    };
    Ok(Json(serde_json::json!({
        "message": format!("account {} key looks valid ({})", account.name, masked)
    })))
}

async fn account_usage(
    State(state): State<CoreState>,
    Path(id): Path<String>,
) -> Result<Json<UsageWindow>, ApiError> {
    state
        .db
        .lock()
        .account_usage(&id)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn reset_account_cooldown(
    State(state): State<CoreState>,
    Path(id): Path<String>,
) -> Result<Json<DashboardAccount>, ApiError> {
    {
        let db = state.db.lock();
        db.clear_account_cooldown(&id).map_err(ApiError::internal)?;
    }
    let account = state
        .db
        .lock()
        .get_account(&id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("account not found"))?;
    dashboard_account(&state, account).map(Json)
}

async fn get_settings(State(state): State<CoreState>) -> Json<AppConfig> {
    Json(state.config())
}

async fn update_settings(
    State(state): State<CoreState>,
    Json(config): Json<AppConfig>,
) -> Result<Json<GatewayStatus>, ApiError> {
    validate_upstream_url(&config.upstream_base_url)?;
    validate_remote_url(&config.remote.url)?;
    let running = state.gateway.lock().is_some();
    if running && config.gateway_port != state.config().gateway_port {
        return Err(ApiError::bad_request(
            "changing gateway_port from the dashboard requires restarting the app",
        ));
    }
    state.set_config(config).map_err(ApiError::internal)?;
    Ok(Json(status_from_state(&state)))
}

async fn regenerate_gateway_key(
    State(state): State<CoreState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut config = state.config();
    config.gateway_key = format!(
        "ocg-{}-{}",
        crate::state::random_word(),
        crate::state::random_word()
    );
    state
        .set_config(config.clone())
        .map_err(ApiError::internal)?;
    Ok(Json(serde_json::json!({ "key": config.gateway_key })))
}

async fn gateway_status(State(state): State<CoreState>) -> Json<GatewayStatus> {
    Json(status_from_state(&state))
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
    days: Option<i64>,
}

async fn gateway_logs(
    State(state): State<CoreState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Vec<GatewayLog>>, ApiError> {
    state
        .db
        .lock()
        .list_gateway_logs(q.limit.unwrap_or(100))
        .map(Json)
        .map_err(ApiError::internal)
}

async fn forward_logs(
    State(state): State<CoreState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Vec<ForwardLog>>, ApiError> {
    state
        .db
        .lock()
        .list_forward_logs(q.limit.unwrap_or(100))
        .map(Json)
        .map_err(ApiError::internal)
}

async fn dashboard_summary(
    State(state): State<CoreState>,
) -> Result<Json<DashboardSummary>, ApiError> {
    let db = state.db.lock();
    let accounts = db.list_accounts().map_err(ApiError::internal)?;
    let total_accounts = accounts.len();
    let available_accounts = accounts
        .iter()
        .filter(|a| {
            a.enabled
                && a.cooldown_until
                    .map(|until| until <= Utc::now())
                    .unwrap_or(true)
        })
        .count();
    let (today_cost, week_cost, month_cost) = db.total_usage().map_err(ApiError::internal)?;
    Ok(Json(DashboardSummary {
        total_accounts,
        available_accounts,
        gateway_running: state.gateway.lock().is_some(),
        today_cost,
        week_cost,
        month_cost,
    }))
}

async fn daily_cost_by_model(
    State(state): State<CoreState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Vec<DailyModelCost>>, ApiError> {
    state
        .db
        .lock()
        .daily_cost_by_model(q.days.unwrap_or(30))
        .map(Json)
        .map_err(ApiError::internal)
}

#[derive(Deserialize)]
struct RemoteTestInput {
    url: String,
    token: String,
}

#[derive(Serialize)]
struct RemoteTestResult {
    ok: bool,
    message: String,
}

#[derive(Serialize)]
struct RemoteSyncResult {
    pushed: usize,
    message: String,
}

#[derive(Serialize)]
struct RemotePushPayload {
    id: String,
    name: String,
    username: Option<String>,
    password: Option<String>,
    password_cipher: Option<String>,
    key: String,
    key_cipher: String,
    enabled: bool,
    referral_code: Option<String>,
    recharge_date: Option<String>,
    created_at: String,
    updated_at: String,
}

async fn test_remote(
    State(state): State<CoreState>,
    Json(input): Json<RemoteTestInput>,
) -> Json<RemoteTestResult> {
    let base = input.url.trim().trim_end_matches('/').to_string();
    if let Err(e) = validate_remote_url(&base) {
        return Json(RemoteTestResult {
            ok: false,
            message: e.message,
        });
    }
    let result = state
        .http_client
        .get(format!("{}/admin/health", base))
        .bearer_auth(input.token.trim())
        .send()
        .await;
    match result {
        Ok(r) if r.status().is_success() => Json(RemoteTestResult {
            ok: true,
            message: format!("{} OK", r.status()),
        }),
        Ok(r) => Json(RemoteTestResult {
            ok: false,
            message: format!("server replied {}", r.status()),
        }),
        Err(e) => Json(RemoteTestResult {
            ok: false,
            message: format!("connection failed: {}", e),
        }),
    }
}

async fn remote_status(
    State(state): State<CoreState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (base, token) = configured_remote(&state)?;
    let response = state
        .http_client
        .get(format!("{}/admin/status", base))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(ApiError::internal)?;
    if !response.status().is_success() {
        return Err(ApiError::bad_request(format!(
            "remote status returned {}",
            response.status()
        )));
    }
    let mut value = response
        .json::<serde_json::Value>()
        .await
        .map_err(ApiError::internal)?;
    if let serde_json::Value::Object(map) = &mut value {
        map.insert("url".to_string(), serde_json::Value::String(base));
    }
    Ok(Json(value))
}

async fn push_local_to_remote(
    State(state): State<CoreState>,
) -> Result<Json<RemoteSyncResult>, ApiError> {
    sync_local_to_remote(&state).await.map(Json)
}

async fn sync_local_to_remote(state: &CoreState) -> Result<RemoteSyncResult, ApiError> {
    let (base, token) = configured_remote(state)?;
    let payloads = remote_payloads(state)?;

    let mut pushed = 0usize;
    for payload in &payloads {
        upsert_remote_key(state, &base, &token, payload).await?;
        pushed += 1;
    }

    let message = format!("推送完成：推送 {} 个账号，未删除远端账号", pushed);
    let _ = state.db.lock().log_gateway("info", "remote_sync", &message);

    Ok(RemoteSyncResult { pushed, message })
}

fn configured_remote(state: &CoreState) -> Result<(String, String), ApiError> {
    let cfg = state.config();
    let base = cfg.remote.url.trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Err(ApiError::bad_request("请先填写并保存远端 URL"));
    }
    validate_remote_url(&base)?;
    let token = cfg.remote.token.trim().to_string();
    if token.is_empty() {
        return Err(ApiError::bad_request("请先填写并保存远端 token"));
    }
    Ok((base, token))
}

fn remote_payloads(state: &CoreState) -> Result<Vec<RemotePushPayload>, ApiError> {
    let accounts = state
        .db
        .lock()
        .list_accounts()
        .map_err(ApiError::internal)?;
    accounts
        .iter()
        .map(|account| remote_payload_from_account(state, account))
        .collect()
}

fn remote_payload_from_account(
    state: &CoreState,
    account: &Account,
) -> Result<RemotePushPayload, ApiError> {
    Ok(RemotePushPayload {
        id: account.id.clone(),
        name: account.name.clone(),
        username: account.username.clone(),
        password: match account.password_cipher.as_deref() {
            Some(cipher) => Some(state.decrypt_key(cipher).map_err(ApiError::internal)?),
            None => None,
        },
        password_cipher: account.password_cipher.clone(),
        key: state
            .decrypt_key(&account.key_cipher)
            .map_err(ApiError::internal)?,
        key_cipher: account.key_cipher.clone(),
        enabled: account.enabled,
        referral_code: account.referral_code.clone(),
        recharge_date: account.recharge_date.clone(),
        created_at: account.created_at.to_rfc3339(),
        updated_at: account.updated_at.to_rfc3339(),
    })
}

async fn upsert_remote_key(
    state: &CoreState,
    base: &str,
    token: &str,
    payload: &RemotePushPayload,
) -> Result<(), ApiError> {
    let response = state
        .http_client
        .post(format!("{}/admin/keys", base))
        .bearer_auth(token)
        .json(payload)
        .send()
        .await
        .map_err(ApiError::internal)?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(ApiError::bad_request(format!(
            "push account {} returned {}",
            payload.name,
            response.status()
        )))
    }
}

fn status_from_state(state: &CoreState) -> GatewayStatus {
    let config = state.config();
    let running = state.gateway.lock().is_some();
    let last_error = if running {
        None
    } else {
        state.db.lock().latest_gateway_error().ok().flatten()
    };
    GatewayStatus {
        running,
        port: config.gateway_port,
        key: config.gateway_key,
        upstream_base_url: config.upstream_base_url,
        last_error,
    }
}

fn validate_upstream_url(url: &str) -> Result<(), ApiError> {
    validate_http_url(url, "upstream", false)
}

fn validate_remote_url(url: &str) -> Result<(), ApiError> {
    validate_http_url(url, "remote sync", true)
}

fn validate_http_url(url: &str, label: &str, allow_empty: bool) -> Result<(), ApiError> {
    if allow_empty && url.trim().is_empty() {
        return Ok(());
    }
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| ApiError::bad_request(format!("invalid {} URL: {}", label, e)))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(&parsed) => Ok(()),
        _ => Err(ApiError::bad_request(format!(
            "{} must use https, except loopback http",
            label
        ))),
    }
}

fn is_loopback(url: &reqwest::Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
    )
}

#[cfg(test)]
mod tests {
    use super::asset_path;
    use std::path::Path;

    #[test]
    fn asset_path_rejects_escape_components() {
        let root = Path::new("dist");

        assert_eq!(
            asset_path(root, "index.js").unwrap(),
            root.join("assets").join("index.js")
        );
        assert_eq!(
            asset_path(root, "nested/index.js").unwrap(),
            root.join("assets").join("nested").join("index.js")
        );

        assert!(asset_path(root, "../secret.txt").is_none());
        assert!(asset_path(root, "/secret.txt").is_none());
        assert!(asset_path(root, r"nested\secret.txt").is_none());
        assert!(asset_path(root, "C:/secret.txt").is_none());
    }
}
