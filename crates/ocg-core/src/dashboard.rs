use crate::auth;
use crate::db::{ForwardLogQueryOptions, ReorderAccountsError};
use crate::gateway::{
    forwarder::forward_get,
    limit::{parse_reset, parse_usage_limit_window},
    protocol::supported_model_ids,
};
use crate::models::*;
use crate::pricing::{PricingSnapshot, fetch_official_snapshot};
use crate::state::{CoreState, DesktopUpdateStartError, DesktopUpdateStatus};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, Response as HttpResponse, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post, put},
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path as FsPath, PathBuf};

pub fn api_router(state: CoreState) -> Router<CoreState> {
    let protected = Router::new()
        .route("/accounts", get(list_accounts).post(create_account))
        .route("/accounts/order", put(reorder_accounts))
        .route(
            "/accounts/{id}",
            patch(update_account).delete(delete_account),
        )
        .route("/accounts/{id}/toggle", post(toggle_account))
        .route("/accounts/{id}/test", post(test_account))
        .route(
            "/accounts/{id}/usage",
            get(account_usage).patch(update_account_usage),
        )
        .route(
            "/accounts/{id}/reset-cooldown",
            post(reset_account_cooldown),
        )
        .route("/settings", get(get_settings).post(update_settings))
        .route(
            "/claude-desktop/models",
            get(get_claude_desktop_models).put(update_claude_desktop_models),
        )
        .route("/settings/check-update", get(check_update))
        .route("/settings/update-status", get(get_update_status))
        .route("/settings/install-update", post(install_update))
        .route("/pricing", get(get_pricing))
        .route("/pricing/refresh", post(refresh_pricing))
        .route(
            "/settings/regenerate-gateway-key",
            post(regenerate_gateway_key),
        )
        .route("/gateway/status", get(gateway_status))
        .route("/application-models", get(application_models))
        .route("/logs/gateway", get(gateway_logs))
        .route("/logs/forward", get(forward_logs))
        .route("/logs/forward/models", get(forward_log_models))
        .route("/dashboard/summary", get(dashboard_summary))
        .route("/dashboard/daily-cost-by-model", get(daily_cost_by_model))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_dashboard_session,
        ));

    Router::new()
        .route("/auth/status", get(auth_status))
        .route("/auth/register", post(register_admin))
        .route("/auth/login", post(login_admin))
        .route("/auth/logout", post(logout_admin))
        .merge(protected)
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

const SESSION_COOKIE: &str = "ocg_dashboard_session";

#[derive(Serialize)]
struct AuthStatus {
    local: bool,
    initialized: bool,
    authenticated: bool,
}

#[derive(Deserialize)]
struct AdminCredentials {
    username: String,
    password: String,
}

async fn auth_status(
    State(state): State<CoreState>,
    headers: HeaderMap,
) -> Result<Json<AuthStatus>, ApiError> {
    let local = is_local_dashboard_request(&state, &headers);
    let initialized = {
        let db = state.db.lock();
        auth::load_admin(&db).map_err(ApiError::internal)?.is_some()
    };
    Ok(Json(AuthStatus {
        local,
        initialized,
        authenticated: local || has_dashboard_session(&state, &headers),
    }))
}

async fn register_admin(
    State(state): State<CoreState>,
    headers: HeaderMap,
    Json(input): Json<AdminCredentials>,
) -> Result<Response, ApiError> {
    let admin = auth::build_admin(&input.username, &input.password)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    {
        let db = state.db.lock();
        if auth::load_admin(&db).map_err(ApiError::internal)?.is_some() {
            return Err(ApiError::status(
                StatusCode::CONFLICT,
                "管理员已经创建，请直接登录",
            ));
        }
        auth::save_admin(&db, &admin).map_err(ApiError::internal)?;
    }
    session_response(&state, &headers, StatusCode::CREATED)
}

async fn login_admin(
    State(state): State<CoreState>,
    headers: HeaderMap,
    Json(input): Json<AdminCredentials>,
) -> Result<Response, ApiError> {
    let admin = {
        let db = state.db.lock();
        auth::load_admin(&db).map_err(ApiError::internal)?
    };
    let valid = admin
        .as_ref()
        .map(|admin| auth::verify_admin(admin, &input.username, &input.password))
        .unwrap_or(false);
    if !valid {
        return Err(ApiError::status(
            StatusCode::UNAUTHORIZED,
            "用户名或密码错误",
        ));
    }
    session_response(&state, &headers, StatusCode::OK)
}

async fn logout_admin(headers: HeaderMap) -> Result<Response, ApiError> {
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, cookie_header("", &headers, true)?);
    Ok(response)
}

async fn require_dashboard_session(
    State(state): State<CoreState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if is_local_dashboard_request(&state, req.headers())
        || has_dashboard_session(&state, req.headers())
    {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn is_local_dashboard_request(state: &CoreState, headers: &HeaderMap) -> bool {
    state.dashboard_local_mode()
        && [
            "forwarded",
            "x-forwarded-for",
            "x-forwarded-proto",
            "x-real-ip",
        ]
        .iter()
        .all(|name| !headers.contains_key(*name))
}

fn has_dashboard_session(state: &CoreState, headers: &HeaderMap) -> bool {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == SESSION_COOKIE).then_some(value)
            })
        })
        .map(|value| value == state.dashboard_session_token)
        .unwrap_or(false)
}

fn session_response(
    state: &CoreState,
    headers: &HeaderMap,
    status: StatusCode,
) -> Result<Response, ApiError> {
    let mut response = (status, Json(serde_json::json!({ "ok": true }))).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie_header(&state.dashboard_session_token, headers, false)?,
    );
    Ok(response)
}

fn cookie_header(
    value: &str,
    request_headers: &HeaderMap,
    clear: bool,
) -> Result<HeaderValue, ApiError> {
    let mut cookie =
        format!("{SESSION_COOKIE}={value}; HttpOnly; SameSite=Strict; Path=/dashboard");
    if clear {
        cookie.push_str("; Max-Age=0");
    }
    if request_headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("https"))
    {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie).map_err(ApiError::internal)
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn status(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

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

#[derive(Debug, Serialize)]
struct DashboardAccount {
    id: String,
    name: String,
    username: String,
    password: String,
    key: String,
    enabled: bool,
    purchase_date: String,
    expires_on: String,
    cooldown_until: Option<String>,
    cooldown_generic_until: Option<String>,
    cooldown_5h_until: Option<String>,
    cooldown_week_until: Option<String>,
    cooldown_month_until: Option<String>,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
}

fn dashboard_account(account: Account) -> DashboardAccount {
    DashboardAccount {
        id: account.id,
        name: account.name,
        username: account.username.unwrap_or_default(),
        password: String::new(),
        key: String::new(),
        enabled: account.enabled,
        purchase_date: account.purchase_date,
        expires_on: account.expires_on,
        cooldown_until: account.cooldown_until.map(|t| t.to_rfc3339()),
        cooldown_generic_until: account.cooldown_generic_until.map(|t| t.to_rfc3339()),
        cooldown_5h_until: account.cooldown_5h_until.map(|t| t.to_rfc3339()),
        cooldown_week_until: account.cooldown_week_until.map(|t| t.to_rfc3339()),
        cooldown_month_until: account.cooldown_month_until.map(|t| t.to_rfc3339()),
        last_error: account.last_error,
        created_at: account.created_at.to_rfc3339(),
        updated_at: account.updated_at.to_rfc3339(),
    }
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

async fn get_pricing(State(state): State<CoreState>) -> Result<Json<PricingSnapshot>, ApiError> {
    Ok(Json(state.pricing_snapshot().as_ref().clone()))
}

#[derive(Debug, Serialize)]
struct PricingRefreshResponse {
    #[serde(flatten)]
    snapshot: PricingSnapshot,
    refresh_status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn refresh_pricing(
    State(state): State<CoreState>,
) -> Result<Json<PricingRefreshResponse>, ApiError> {
    let _guard = state.pricing_refresh.try_lock().map_err(|_| {
        ApiError::status(
            StatusCode::CONFLICT,
            "OpenCode Go pricing refresh is already running",
        )
    })?;

    apply_pricing_refresh(&state, fetch_official_snapshot().await).map(Json)
}

fn apply_pricing_refresh(
    state: &CoreState,
    result: crate::Result<PricingSnapshot>,
) -> Result<PricingRefreshResponse, ApiError> {
    match result {
        Ok(snapshot) => {
            let active = state.pricing_snapshot();
            let snapshot = if active.content_hash == snapshot.content_hash
                && active.adjustment_policy_version == snapshot.adjustment_policy_version
            {
                active.as_ref().clone()
            } else {
                state
                    .activate_pricing_snapshot(snapshot.clone())
                    .map_err(ApiError::internal)?;
                snapshot
            };
            let _ = state.db.lock().log_gateway(
                "info",
                "pricing",
                &format!("activated OpenCode Go pricing {}", snapshot.revision),
            );
            Ok(PricingRefreshResponse {
                snapshot,
                refresh_status: "success",
                error: None,
            })
        }
        Err(error) => {
            let message = error.to_string();
            let _ = state.db.lock().log_gateway(
                "warn",
                "pricing",
                &format!("OpenCode Go pricing refresh failed: {message}"),
            );
            Ok(PricingRefreshResponse {
                snapshot: state.pricing_snapshot().as_ref().clone(),
                refresh_status: "failed_no_change",
                error: Some(message),
            })
        }
    }
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
    Ok(Json(
        accounts
            .into_iter()
            .map(dashboard_account)
            .collect::<Vec<_>>(),
    ))
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
    let purchase_date = match input.purchase_date {
        Some(value) if !value.trim().is_empty() => normalize_purchase_date(&value)
            .map_err(|error| ApiError::bad_request(error.to_string()))?,
        _ => String::new(),
    };
    let now = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();
    let account = Account {
        id: id.clone(),
        name,
        username: clean_optional(input.username),
        password_cipher: encrypted_optional(&state, &input.password)?,
        key_cipher: state
            .encrypt_key(input.key.trim())
            .map_err(ApiError::internal)?,
        enabled: true,
        referral_code: clean_optional(input.referral_code),
        purchase_date,
        expires_on: String::new(),
        cooldown_until: None,
        cooldown_generic_until: None,
        cooldown_5h_until: None,
        cooldown_week_until: None,
        cooldown_month_until: None,
        last_error: None,
        created_at: now,
        updated_at: now,
    };
    let account = {
        let db = state.db.lock();
        db.create_account(&account).map_err(ApiError::internal)?;
        let _ = db.log_gateway(
            "info",
            "account",
            &format!("created account {}", account.name),
        );
        db.get_account(&id)
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::internal("created account not found"))?
    };
    Ok(Json(dashboard_account(account)))
}

async fn update_account(
    State(state): State<CoreState>,
    Path(id): Path<String>,
    Json(mut update): Json<AccountUpdate>,
) -> Result<Json<DashboardAccount>, ApiError> {
    if let Some(value) = update.purchase_date.take() {
        update.purchase_date = Some(
            normalize_purchase_date(&value)
                .map_err(|error| ApiError::bad_request(error.to_string()))?,
        );
    }
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
    Ok(Json(dashboard_account(account)))
}

#[derive(Deserialize)]
struct AccountOrderInput {
    account_ids: Vec<String>,
}

async fn reorder_accounts(
    State(state): State<CoreState>,
    Json(input): Json<AccountOrderInput>,
) -> Result<Json<Vec<DashboardAccount>>, ApiError> {
    let accounts = {
        let db = state.db.lock();
        db.reorder_accounts(&input.account_ids)
            .map_err(|error| match error {
                ReorderAccountsError::DuplicateAccountId => {
                    ApiError::bad_request("account_ids contains duplicates")
                }
                ReorderAccountsError::AccountSetMismatch => ApiError::status(
                    StatusCode::CONFLICT,
                    "account list changed; reload accounts and try again",
                ),
                ReorderAccountsError::Database(error) => ApiError::internal(error),
            })?;
        db.list_accounts().map_err(ApiError::internal)?
    };
    Ok(Json(
        accounts
            .into_iter()
            .map(dashboard_account)
            .collect::<Vec<_>>(),
    ))
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
        purchase_date: None,
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
    Ok(Json(dashboard_account(account)))
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
    let (config, client) = state.upstream_context();
    validate_upstream_url(&config.upstream_base_url)?;
    let response = client
        .post(format!(
            "{}/v1/chat/completions",
            config.upstream_base_url.trim_end_matches('/')
        ))
        .bearer_auth(&key)
        .json(&account_ping_payload())
        .timeout(std::time::Duration::from_secs(
            config.non_stream_timeout_secs,
        ))
        .send()
        .await
        .map_err(ApiError::internal)?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        if error.is_timeout() {
            ApiError::internal("upstream response body timed out")
        } else {
            ApiError::internal(error)
        }
    })?;
    if status == StatusCode::TOO_MANY_REQUESTS {
        let cooldown = parse_reset(&body).unwrap_or_else(|| Duration::minutes(5));
        let until = Utc::now() + cooldown;
        {
            let db = state.db.lock();
            db.set_account_rate_limit(&account.id, until, &body, parse_usage_limit_window(&body))
                .map_err(ApiError::internal)?;
            let _ = db.log_gateway(
                "warn",
                "account",
                &format!("ping quota reached for account {}", account.name),
            );
        }
        return Err(ApiError::status(
            StatusCode::TOO_MANY_REQUESTS,
            format!(
                "Ping 到达额度或限流，已熔断到 {}",
                until.format("%Y-%m-%d %H:%M:%S UTC")
            ),
        ));
    }
    if !status.is_success() {
        return Err(ApiError::bad_request(format!(
            "Ping failed: upstream returned {}: {}",
            status,
            short_body(&body)
        )));
    }
    let masked = if key.len() > 8 && key.is_char_boundary(4) && key.is_char_boundary(key.len() - 4)
    {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else {
        "***".to_string()
    };
    Ok(Json(serde_json::json!({
        "message": format!("Ping OK: {} ({})", account.name, masked)
    })))
}

fn account_ping_payload() -> serde_json::Value {
    serde_json::json!({
        "model": "deepseek-v4-flash",
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 1,
        "stream": false
    })
}

fn short_body(body: &str) -> String {
    body.split_whitespace()
        .take(40)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(300)
        .collect()
}

async fn account_usage(
    State(state): State<CoreState>,
    Path(id): Path<String>,
) -> Result<Json<UsageWindow>, ApiError> {
    let limits = state.pricing_snapshot().limits.clone();
    state
        .db
        .lock()
        .account_usage_with_limits(&id, &limits)
        .map(Json)
        .map_err(ApiError::internal)
}

#[derive(Deserialize)]
struct AccountUsageUpdate {
    window: String,
    percent: f64,
}

async fn update_account_usage(
    State(state): State<CoreState>,
    Path(id): Path<String>,
    Json(update): Json<AccountUsageUpdate>,
) -> Result<Json<UsageWindow>, ApiError> {
    let window = match update.window.as_str() {
        "window_5h" => UsageWindowKind::FiveHours,
        "window_week" => UsageWindowKind::Week,
        "window_month" => UsageWindowKind::Month,
        _ => return Err(ApiError::bad_request("invalid usage window")),
    };
    if !update.percent.is_finite() || !(0.0..=100.0).contains(&update.percent) {
        return Err(ApiError::bad_request(
            "usage percent must be between 0 and 100",
        ));
    }
    let percent = (update.percent * 10.0).round() / 10.0;

    let limits = state.pricing_snapshot().limits.clone();
    let db = state.db.lock();
    if !db
        .set_account_usage_baseline(&id, window, percent)
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::not_found("account not found"));
    }
    db.account_usage_with_limits(&id, &limits)
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
    Ok(Json(dashboard_account(account)))
}

#[derive(Serialize)]
struct SettingsResponse {
    #[serde(flatten)]
    config: AppConfig,
    revision: u64,
    auto_start_supported: bool,
    client_root_url_from_env: bool,
}

async fn get_settings(State(state): State<CoreState>) -> Json<SettingsResponse> {
    let _settings_update = state.settings_update.lock();
    let auto_start_supported = state.auto_start_supported();
    Json(SettingsResponse {
        config: state.settings_config(),
        revision: state.settings_revision(),
        auto_start_supported,
        client_root_url_from_env: state.client_root_url_from_env(),
    })
}

async fn get_claude_desktop_models(State(state): State<CoreState>) -> Json<ClaudeDesktopModels> {
    Json(state.config().claude_desktop_models.resolved())
}

async fn update_claude_desktop_models(
    State(state): State<CoreState>,
    Json(mut models): Json<ClaudeDesktopModels>,
) -> Result<Json<ClaudeDesktopModels>, ApiError> {
    let _settings_update = state.settings_update.lock();
    models.normalize();
    models.validate().map_err(ApiError::bad_request)?;
    let response = models.resolved();
    let mut config = state.config();
    config.claude_desktop_models = models;
    state.set_config(config).map_err(ApiError::internal)?;
    Ok(Json(response))
}

const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/klarkxy/opencode-go-mgr/releases/latest";
const GITHUB_LATEST_RELEASE_URL: &str =
    "https://github.com/klarkxy/opencode-go-mgr/releases/latest";
const UPDATE_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
}

#[derive(Serialize)]
struct UpdateCheckResponse {
    current_version: String,
    latest_version: String,
    update_available: bool,
    release_url: &'static str,
    install_supported: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallUpdateRequest {
    expected_version: String,
}

async fn check_update(
    State(state): State<CoreState>,
) -> Result<Json<UpdateCheckResponse>, ApiError> {
    let (_, client) = state.upstream_context();
    let release = client
        .get(GITHUB_LATEST_RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(
            reqwest::header::USER_AGENT,
            concat!("ocg-manager/", env!("CARGO_PKG_VERSION")),
        )
        .timeout(UPDATE_CHECK_TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(update_check_error)?
        .json::<GithubRelease>()
        .await
        .map_err(update_check_error)?;

    let current_version = env!("CARGO_PKG_VERSION");
    let (current_parts, current_version) = parse_stable_version(current_version)
        .ok_or_else(|| ApiError::internal("application version is not stable X.Y.Z"))?;
    let (latest_parts, latest_version) =
        parse_stable_version(&release.tag_name).ok_or_else(|| {
            ApiError::status(
                StatusCode::BAD_GATEWAY,
                "GitHub latest release has an invalid stable version tag",
            )
        })?;

    Ok(Json(UpdateCheckResponse {
        current_version: current_version.to_string(),
        latest_version: latest_version.to_string(),
        update_available: is_update_available(current_parts, latest_parts),
        release_url: GITHUB_LATEST_RELEASE_URL,
        install_supported: state.desktop_update_supported(),
    }))
}

async fn get_update_status(State(state): State<CoreState>) -> Json<DesktopUpdateStatus> {
    Json(state.desktop_update_status())
}

async fn install_update(
    State(state): State<CoreState>,
    Json(input): Json<InstallUpdateRequest>,
) -> Result<(StatusCode, Json<DesktopUpdateStatus>), ApiError> {
    let status = state.desktop_update_status();
    let (current_parts, _) = parse_stable_version(&status.current_version)
        .ok_or_else(|| ApiError::internal("application version is not stable X.Y.Z"))?;
    let (expected_parts, expected_version) = parse_stable_version(&input.expected_version)
        .ok_or_else(|| ApiError::bad_request("expected_version must be a stable X.Y.Z version"))?;
    if !is_update_available(current_parts, expected_parts) {
        return Err(ApiError::bad_request(
            "expected_version must be newer than the current version",
        ));
    }

    match state.start_desktop_update(expected_version.to_string()) {
        Ok(()) => Ok((StatusCode::ACCEPTED, Json(state.desktop_update_status()))),
        Err(DesktopUpdateStartError::Unsupported) => Err(ApiError::bad_request(
            "desktop update installation is unavailable in this runtime",
        )),
        Err(DesktopUpdateStartError::Busy) => Err(ApiError::status(
            StatusCode::CONFLICT,
            "a desktop update is already in progress",
        )),
        Err(DesktopUpdateStartError::Starter(error)) => Err(ApiError::internal(error)),
    }
}

fn update_check_error(error: reqwest::Error) -> ApiError {
    let category = if error.is_timeout() {
        format!(
            "request timed out after {} seconds",
            UPDATE_CHECK_TIMEOUT.as_secs()
        )
    } else if error.is_connect() {
        "connection failed".to_string()
    } else if let Some(status) = error.status() {
        format!("GitHub returned HTTP {status}")
    } else if error.is_decode() {
        "GitHub returned an invalid response".to_string()
    } else {
        "request failed".to_string()
    };
    ApiError::status(
        StatusCode::BAD_GATEWAY,
        format!(
            "failed to check GitHub releases ({category}): {}",
            format_error_chain(&error)
        ),
    )
}

fn format_error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

fn parse_stable_version(version: &str) -> Option<([u64; 3], &str)> {
    let version = version.strip_prefix('v').unwrap_or(version);
    let mut parts = version.split('.');
    let parse_part = |part: &str| {
        (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    };
    let parsed = [
        parse_part(parts.next()?)?,
        parse_part(parts.next()?)?,
        parse_part(parts.next()?)?,
    ];
    parts.next().is_none().then_some((parsed, version))
}

fn is_update_available(current: [u64; 3], latest: [u64; 3]) -> bool {
    latest > current
}

#[derive(Deserialize)]
struct SettingsUpdateRequest {
    #[serde(flatten)]
    config: AppConfig,
    expected_revision: u64,
}

#[derive(Serialize)]
struct SettingsRevisionResponse {
    revision: u64,
}

async fn update_settings(
    State(state): State<CoreState>,
    Json(input): Json<SettingsUpdateRequest>,
) -> Result<Json<SettingsRevisionResponse>, ApiError> {
    let _settings_update = state.settings_update.lock();
    if input.expected_revision != state.settings_revision() {
        return Err(ApiError::status(
            StatusCode::CONFLICT,
            "settings changed since they were loaded; reload and try again",
        ));
    }
    let mut config = input.config;
    config.gateway_key = config.gateway_key.trim().to_string();
    if config.gateway_key.is_empty() {
        return Err(ApiError::bad_request("gateway key is required"));
    }
    let previous_config = state.config();
    config.claude_desktop_models = previous_config.claude_desktop_models.clone();
    config.validate().map_err(ApiError::bad_request)?;
    validate_upstream_url(&config.upstream_base_url)?;
    config.client_root_url =
        normalize_client_root_url(&config.client_root_url).map_err(ApiError::bad_request)?;
    let next_auto_start = config.auto_start;
    let auto_start_supported = state.auto_start_supported();
    if !auto_start_supported && next_auto_start != previous_config.auto_start {
        return Err(ApiError::bad_request(
            "auto-start is unavailable in this runtime",
        ));
    }
    state.set_config(config).map_err(ApiError::internal)?;
    if auto_start_supported {
        if let Err(sync_error) = state.sync_auto_start(next_auto_start) {
            let config_rollback_error = state.set_config(previous_config.clone()).err();
            let auto_start_rollback_error = state.sync_auto_start(previous_config.auto_start).err();
            let mut message = format!("failed to synchronize auto-start: {sync_error}");
            if let Some(error) = config_rollback_error {
                message.push_str(&format!("; failed to restore settings: {error}"));
            }
            if let Some(error) = auto_start_rollback_error {
                message.push_str(&format!("; failed to restore auto-start state: {error}"));
            }
            return Err(ApiError::internal(message));
        }
    }
    Ok(Json(SettingsRevisionResponse {
        revision: state.settings_revision(),
    }))
}

async fn regenerate_gateway_key(
    State(state): State<CoreState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _settings_update = state.settings_update.lock();
    let mut config = state.config();
    config.gateway_key = format!(
        "ocg-{}-{}",
        crate::state::random_word(),
        crate::state::random_word()
    );
    state
        .set_config(config.clone())
        .map_err(ApiError::internal)?;
    Ok(Json(serde_json::json!({
        "key": config.gateway_key,
        "revision": state.settings_revision(),
    })))
}

async fn gateway_status(State(state): State<CoreState>) -> Json<GatewayStatus> {
    Json(status_from_state(&state))
}

async fn application_models(State(state): State<CoreState>) -> Result<Json<Vec<String>>, ApiError> {
    let (config, client) = state.upstream_context();
    let response = forward_get(&client, &state, &config, "/v1/models")
        .await
        .map_err(|_| {
            ApiError::status(
                StatusCode::BAD_GATEWAY,
                "failed to load upstream model list",
            )
        })?;
    if !response.status().is_success() {
        return Err(ApiError::status(
            StatusCode::BAD_GATEWAY,
            "upstream model discovery failed",
        ));
    }
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .map_err(|_| ApiError::status(StatusCode::BAD_GATEWAY, "upstream model list is invalid"))?;
    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::status(StatusCode::BAD_GATEWAY, "upstream model list is invalid"))?;
    let data = payload
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            ApiError::status(StatusCode::BAD_GATEWAY, "upstream model list is invalid")
        })?;
    let supported = supported_model_ids().collect::<Vec<_>>();
    let mut models = Vec::new();
    for id in data
        .iter()
        .filter_map(|model| model.get("id").and_then(serde_json::Value::as_str))
    {
        if supported.contains(&id) && !models.iter().any(|model| model == id) {
            models.push(id.to_string());
        }
    }
    Ok(Json(models))
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
    days: Option<i64>,
}

#[derive(Default, Deserialize)]
struct ForwardLogQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    status: Option<String>,
    account_id: Option<String>,
    model: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
}

fn validate_forward_log_query(
    query: &ForwardLogQuery,
) -> Result<(Option<String>, Option<String>), ApiError> {
    if query.sort_by.as_deref().is_some_and(|value| {
        !matches!(
            value,
            "timestamp"
                | "prompt_tokens"
                | "completion_tokens"
                | "cached_tokens"
                | "cost"
                | "model"
                | "status"
        )
    }) {
        return Err(ApiError::bad_request("invalid sort_by"));
    }
    if query
        .sort_order
        .as_deref()
        .is_some_and(|value| !matches!(value, "asc" | "desc"))
    {
        return Err(ApiError::bad_request("invalid sort_order"));
    }

    let parse_time = |value: Option<&str>, name: &str| -> Result<_, ApiError> {
        value
            .map(|value| {
                DateTime::parse_from_rfc3339(value)
                    .map(|time| {
                        time.with_timezone(&Utc)
                            .to_rfc3339_opts(SecondsFormat::Millis, true)
                    })
                    .map_err(|_| ApiError::bad_request(format!("invalid {name}")))
            })
            .transpose()
    };
    let start_time = parse_time(query.start_time.as_deref(), "start_time")?;
    let end_time = parse_time(query.end_time.as_deref(), "end_time")?;
    if start_time
        .as_ref()
        .zip(end_time.as_ref())
        .is_some_and(|(start, end)| start > end)
    {
        return Err(ApiError::bad_request(
            "start_time must not be after end_time",
        ));
    }
    Ok((start_time, end_time))
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
    Query(q): Query<ForwardLogQuery>,
) -> Result<Json<ForwardLogPage>, ApiError> {
    let (start_time, end_time) = validate_forward_log_query(&q)?;
    state
        .db
        .lock()
        .query_forward_logs(ForwardLogQueryOptions {
            limit: q.limit.unwrap_or(100),
            offset: q.offset.unwrap_or(0),
            status: q.status.as_deref(),
            account_id: q.account_id.as_deref(),
            model: q.model.as_deref(),
            start_time: start_time.as_deref(),
            end_time: end_time.as_deref(),
            sort_by: q.sort_by.as_deref(),
            sort_order: q.sort_order.as_deref(),
        })
        .map(Json)
        .map_err(ApiError::internal)
}

async fn forward_log_models(State(state): State<CoreState>) -> Result<Json<Vec<String>>, ApiError> {
    state
        .db
        .lock()
        .list_forward_log_models()
        .map(Json)
        .map_err(ApiError::internal)
}

async fn dashboard_summary(
    State(state): State<CoreState>,
) -> Result<Json<DashboardSummary>, ApiError> {
    let db = state.db.lock();
    let accounts = db.list_accounts().map_err(ApiError::internal)?;
    let total_accounts = accounts.len();
    let now = Utc::now();
    let available_accounts = accounts
        .iter()
        .filter(|a| a.enabled && !a.is_cooling_at(now))
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
        port: state.active_gateway_port(),
        key: config.gateway_key,
        upstream_base_url: config.upstream_base_url,
        last_error,
    }
}

fn validate_upstream_url(url: &str) -> Result<(), ApiError> {
    validate_http_url(url, "upstream")
}

fn validate_http_url(url: &str, label: &str) -> Result<(), ApiError> {
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
    use super::{
        AccountOrderInput, AccountUsageUpdate, ForwardLogQuery, SettingsUpdateRequest,
        UpdateCheckResponse, apply_pricing_refresh, asset_path, create_account, dashboard_account,
        dashboard_summary, format_error_chain, is_update_available, parse_stable_version,
        reorder_accounts, update_account, update_account_usage, update_settings,
        validate_forward_log_query,
    };
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::db::Database;
    use crate::models::{
        Account, AccountInput, AccountUpdate, AppConfig, ClaudeDesktopModels,
        normalize_purchase_date, purchase_expires_on,
    };
    use crate::state::CoreStateInner;
    use axum::Json;
    use axum::extract::{Path as AxumPath, State};
    use axum::http::StatusCode;
    use chrono::Utc;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn temp_data_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("ocg-dashboard-test-{}-{}", label, nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_account(id: &str) -> Account {
        let now = Utc::now();
        Account {
            id: id.into(),
            name: id.into(),
            username: None,
            password_cipher: None,
            key_cipher: format!("cipher-{id}"),
            enabled: true,
            referral_code: None,
            purchase_date: "2026-06-15".into(),
            expires_on: "2026-07-15".into(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            last_error: None,
            created_at: now,
            updated_at: now,
        }
    }

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

    #[test]
    fn failed_pricing_refresh_preserves_last_known_good_snapshot() {
        let dir = temp_data_dir("pricing-lkg");
        let cipher: Arc<dyn KeyCipher + Send + Sync> =
            Arc::new(StaticKeyCipher::new("pricing-test"));
        let state = Arc::new(
            CoreStateInner::new(Database::open(dir.clone()).unwrap(), dir.clone(), cipher).unwrap(),
        );
        let before = state.pricing_snapshot();

        let response = apply_pricing_refresh(
            &state,
            Err(anyhow::anyhow!("fixture parser rejected the document")),
        )
        .unwrap();

        assert_eq!(response.refresh_status, "failed_no_change");
        assert_eq!(response.snapshot.revision, before.revision);
        assert_eq!(state.pricing_snapshot().revision, before.revision);
        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn forward_log_query_normalizes_offsets_and_rejects_invalid_ordering() {
        let query = ForwardLogQuery {
            start_time: Some("2026-07-17T12:00:00+08:00".into()),
            end_time: Some("2026-07-17T05:00:00Z".into()),
            sort_by: Some("cost".into()),
            sort_order: Some("asc".into()),
            ..ForwardLogQuery::default()
        };
        let (start, end) = validate_forward_log_query(&query).expect("valid query");
        assert_eq!(start.as_deref(), Some("2026-07-17T04:00:00.000Z"));
        assert_eq!(end.as_deref(), Some("2026-07-17T05:00:00.000Z"));

        for invalid in [
            ForwardLogQuery {
                sort_by: Some("costt".into()),
                ..ForwardLogQuery::default()
            },
            ForwardLogQuery {
                sort_order: Some("sideways".into()),
                ..ForwardLogQuery::default()
            },
            ForwardLogQuery {
                start_time: Some("not-a-time".into()),
                ..ForwardLogQuery::default()
            },
            ForwardLogQuery {
                start_time: Some("2026-07-17T06:00:00Z".into()),
                end_time: Some("2026-07-17T05:00:00Z".into()),
                ..ForwardLogQuery::default()
            },
        ] {
            assert!(validate_forward_log_query(&invalid).is_err());
        }
    }

    #[test]
    fn dashboard_account_does_not_export_secrets() {
        let dir = temp_data_dir("secret-list");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        let account = Account {
            id: "acct-1".into(),
            name: "main".into(),
            username: Some("user".into()),
            password_cipher: Some(state.encrypt_key("password-secret").unwrap()),
            key_cipher: state.encrypt_key("sk-secret").unwrap(),
            enabled: true,
            referral_code: None,
            purchase_date: "2026-01-31".into(),
            expires_on: "2026-02-28".into(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            last_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let dto = dashboard_account(account);

        assert_eq!(dto.username, "user");
        assert!(dto.password.is_empty());
        assert!(dto.key.is_empty());
        assert_eq!(dto.purchase_date, "2026-01-31");
        assert_eq!(dto.expires_on, "2026-02-28");
        let json = serde_json::to_value(dto).expect("dashboard account should serialize");
        assert!(json.get("recharge_date").is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn create_account_defaults_purchase_date_and_returns_persisted_expiry() {
        let dir = temp_data_dir("create-default-purchase-date");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).expect("test database should open");
        let state = Arc::new(
            CoreStateInner::new(db, dir.clone(), cipher).expect("test state should initialize"),
        );

        let account = create_account(
            State(state.clone()),
            Json(AccountInput {
                name: "main".into(),
                username: None,
                password: None,
                key: "sk-test".into(),
                referral_code: None,
                purchase_date: None,
            }),
        )
        .await
        .expect("account should be created")
        .0;

        assert_eq!(
            normalize_purchase_date(&account.purchase_date)
                .expect("persisted purchase date should be valid"),
            account.purchase_date
        );
        assert_eq!(
            account.expires_on,
            purchase_expires_on(&account.purchase_date)
                .expect("persisted purchase date should have an expiry")
        );
        let persisted = state
            .db
            .lock()
            .get_account(&account.id)
            .expect("created account lookup should succeed")
            .expect("created account should exist");
        assert_eq!(persisted.purchase_date, account.purchase_date);
        assert_eq!(persisted.expires_on, account.expires_on);

        drop(state);
        fs::remove_dir_all(dir).expect("test directory should be removable");
    }

    #[tokio::test]
    async fn update_account_rejects_invalid_purchase_date_as_bad_request() {
        let dir = temp_data_dir("invalid-purchase-date");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).expect("test database should open");
        db.create_account(&test_account("acct-1"))
            .expect("test account should be created");
        let state = Arc::new(
            CoreStateInner::new(db, dir.clone(), cipher).expect("test state should initialize"),
        );

        let error = update_account(
            State(state.clone()),
            AxumPath("acct-1".into()),
            Json(AccountUpdate {
                name: None,
                username: None,
                password: None,
                key: None,
                enabled: None,
                referral_code: None,
                purchase_date: Some("2026-02-30".into()),
            }),
        )
        .await
        .expect_err("invalid purchase date should fail");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        let persisted = state
            .db
            .lock()
            .get_account("acct-1")
            .expect("account lookup should succeed")
            .expect("account should still exist");
        assert_eq!(persisted.purchase_date, "2026-06-15");

        drop(state);
        fs::remove_dir_all(dir).expect("test directory should be removable");
    }

    #[tokio::test]
    async fn reorder_accounts_maps_validation_errors_and_returns_saved_order() {
        let dir = temp_data_dir("reorder-accounts");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).expect("test database should open");
        for id in ["acct-1", "acct-2", "acct-3"] {
            db.create_account(&test_account(id))
                .expect("test account should be created");
        }
        let state = Arc::new(
            CoreStateInner::new(db, dir.clone(), cipher).expect("test state should initialize"),
        );

        let duplicate = reorder_accounts(
            State(state.clone()),
            Json(AccountOrderInput {
                account_ids: vec!["acct-1".into(), "acct-1".into(), "acct-3".into()],
            }),
        )
        .await
        .expect_err("duplicate ids should fail");
        assert_eq!(duplicate.status, StatusCode::BAD_REQUEST);

        for stale_ids in [
            vec!["acct-1".into(), "acct-2".into()],
            vec!["acct-1".into(), "acct-2".into(), "missing".into()],
            Vec::new(),
        ] {
            let stale = reorder_accounts(
                State(state.clone()),
                Json(AccountOrderInput {
                    account_ids: stale_ids,
                }),
            )
            .await
            .expect_err("stale account set should fail");
            assert_eq!(stale.status, StatusCode::CONFLICT);
        }

        let unchanged = state
            .db
            .lock()
            .list_accounts()
            .expect("account order should load")
            .into_iter()
            .map(|account| account.id)
            .collect::<Vec<_>>();
        assert_eq!(unchanged, ["acct-1", "acct-2", "acct-3"]);

        let reordered = reorder_accounts(
            State(state.clone()),
            Json(AccountOrderInput {
                account_ids: vec!["acct-3".into(), "acct-1".into(), "acct-2".into()],
            }),
        )
        .await
        .expect("complete account set should be reordered")
        .0;
        assert_eq!(
            reordered
                .into_iter()
                .map(|account| account.id)
                .collect::<Vec<_>>(),
            ["acct-3", "acct-1", "acct-2"]
        );

        drop(state);
        fs::remove_dir_all(dir).expect("test directory should be removable");
    }

    #[tokio::test]
    async fn reorder_accounts_accepts_empty_order_for_empty_database() {
        let dir = temp_data_dir("reorder-empty-accounts");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).expect("test database should open");
        let state = Arc::new(
            CoreStateInner::new(db, dir.clone(), cipher).expect("test state should initialize"),
        );

        let accounts = reorder_accounts(
            State(state.clone()),
            Json(AccountOrderInput {
                account_ids: Vec::new(),
            }),
        )
        .await
        .expect("empty account set should accept an empty order")
        .0;
        assert!(accounts.is_empty());

        drop(state);
        fs::remove_dir_all(dir).expect("test directory should be removable");
    }

    #[tokio::test]
    async fn manual_usage_update_validates_persists_and_keeps_account_available() {
        let dir = temp_data_dir("manual-usage");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        db.create_account(&Account {
            id: "acct-usage".into(),
            name: "usage".into(),
            username: None,
            password_cipher: None,
            key_cipher: cipher.encrypt("sk-test").unwrap(),
            enabled: true,
            referral_code: None,
            purchase_date: "2026-01-31".into(),
            expires_on: "2026-02-28".into(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            last_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

        let invalid = update_account_usage(
            State(state.clone()),
            AxumPath("acct-usage".into()),
            Json(AccountUsageUpdate {
                window: "invalid".into(),
                percent: 50.0,
            }),
        )
        .await
        .expect_err("invalid window should fail");
        assert_eq!(invalid.status, StatusCode::BAD_REQUEST);

        let invalid = update_account_usage(
            State(state.clone()),
            AxumPath("acct-usage".into()),
            Json(AccountUsageUpdate {
                window: "window_5h".into(),
                percent: -0.1,
            }),
        )
        .await
        .expect_err("invalid percent should fail");
        assert_eq!(invalid.status, StatusCode::BAD_REQUEST);

        let missing = update_account_usage(
            State(state.clone()),
            AxumPath("missing".into()),
            Json(AccountUsageUpdate {
                window: "window_5h".into(),
                percent: 50.0,
            }),
        )
        .await
        .expect_err("missing account should fail");
        assert_eq!(missing.status, StatusCode::NOT_FOUND);

        let usage = update_account_usage(
            State(state.clone()),
            AxumPath("acct-usage".into()),
            Json(AccountUsageUpdate {
                window: "window_5h".into(),
                percent: 50.04,
            }),
        )
        .await
        .expect("valid baseline should save")
        .0;
        assert!((usage.window_5h - 6.0).abs() < 1e-9);

        let _ = update_account_usage(
            State(state.clone()),
            AxumPath("acct-usage".into()),
            Json(AccountUsageUpdate {
                window: "window_month".into(),
                percent: 100.0,
            }),
        )
        .await
        .expect("100 percent baseline should save");
        let summary = dashboard_summary(State(state))
            .await
            .expect("summary should load")
            .0;
        assert_eq!(summary.available_accounts, 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn regular_settings_update_preserves_claude_desktop_models() {
        let dir = temp_data_dir("preserve-claude-desktop-models");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        let configured = ClaudeDesktopModels {
            sonnet: "glm-5.2".to_string(),
            opus: String::new(),
            haiku: "mimo-v2.5".to_string(),
        };
        let mut persisted = state.config();
        persisted.claude_desktop_models = configured.clone();
        state.set_config(persisted).unwrap();

        let _ = update_settings(
            State(state.clone()),
            Json(SettingsUpdateRequest {
                config: AppConfig {
                    gateway_key: "updated-gateway-key".to_string(),
                    connect_timeout_secs: 45,
                    ..AppConfig::default()
                },
                expected_revision: state.settings_revision(),
            }),
        )
        .await
        .expect("regular settings should save");

        assert_eq!(state.config().claude_desktop_models, configured);
        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    fn version_parts(version: &str) -> [u64; 3] {
        parse_stable_version(version)
            .expect("test version should be valid")
            .0
    }

    #[test]
    fn error_chain_includes_transport_root_cause() {
        let error = anyhow::Error::msg("root cause").context("outer error");
        assert_eq!(
            format_error_chain(error.as_ref()),
            "outer error: root cause"
        );
    }

    #[test]
    fn update_check_response_reports_install_capability() {
        let response = UpdateCheckResponse {
            current_version: "1.0.0".to_string(),
            latest_version: "2.0.0".to_string(),
            update_available: true,
            release_url: "https://example.com/release",
            install_supported: true,
        };
        let json = serde_json::to_value(response).expect("response should serialize");
        assert_eq!(json["install_supported"], true);
    }

    #[test]
    fn stable_version_comparison_detects_newer_release() {
        assert!(is_update_available(
            version_parts("1.0.0"),
            version_parts("1.1.0")
        ));
    }

    #[test]
    fn stable_version_comparison_treats_equal_as_current() {
        assert!(!is_update_available(
            version_parts("1.1.0"),
            version_parts("1.1.0")
        ));
    }

    #[test]
    fn stable_version_comparison_treats_current_ahead_as_current() {
        assert!(!is_update_available(
            version_parts("2.0.0"),
            version_parts("1.9.9")
        ));
    }

    #[test]
    fn stable_version_parser_strips_v_prefix() {
        assert_eq!(
            parse_stable_version("v1.2.3").map(|(_, value)| value),
            Some("1.2.3")
        );
    }

    #[test]
    fn stable_version_parser_rejects_non_stable_tag() {
        assert!(parse_stable_version("v1.1.0-beta.1").is_none());
    }
}
