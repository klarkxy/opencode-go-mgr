use crate::auth;
use crate::gateway::{
    forwarder::forward_get,
    limit::{parse_reset, parse_usage_limit_window},
    protocol::supported_model_ids,
};
use crate::models::*;
use crate::state::CoreState;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, Response as HttpResponse, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path as FsPath, PathBuf};

pub fn api_router(state: CoreState) -> Router<CoreState> {
    let protected = Router::new()
        .route("/accounts", get(list_accounts).post(create_account))
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
        .route("/settings/check-update", get(check_update))
        .route(
            "/settings/regenerate-gateway-key",
            post(regenerate_gateway_key),
        )
        .route("/gateway/status", get(gateway_status))
        .route("/application-models", get(application_models))
        .route("/logs/gateway", get(gateway_logs))
        .route("/logs/forward", get(forward_logs))
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

fn dashboard_account(account: Account) -> DashboardAccount {
    DashboardAccount {
        id: account.id,
        name: account.name,
        username: account.username.unwrap_or_default(),
        password: String::new(),
        key: String::new(),
        enabled: account.enabled,
        cooldown_until: account.cooldown_until.map(|t| t.to_rfc3339()),
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
    Ok(Json(dashboard_account(account)))
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
    Ok(Json(dashboard_account(account)))
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
    state
        .db
        .lock()
        .account_usage(&id)
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

    let db = state.db.lock();
    if !db
        .set_account_usage_baseline(&id, window, percent)
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::not_found("account not found"));
    }
    db.account_usage(&id).map(Json).map_err(ApiError::internal)
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
    auto_start_supported: bool,
    client_root_url_from_env: bool,
}

async fn get_settings(State(state): State<CoreState>) -> Json<SettingsResponse> {
    let auto_start_supported = state.auto_start_supported();
    Json(SettingsResponse {
        config: state.settings_config(),
        auto_start_supported,
        client_root_url_from_env: state.client_root_url_from_env(),
    })
}

const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/klarkxy/opencode-go-mgr/releases/latest";
const GITHUB_LATEST_RELEASE_URL: &str =
    "https://github.com/klarkxy/opencode-go-mgr/releases/latest";

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
        .timeout(std::time::Duration::from_secs(10))
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
    }))
}

fn update_check_error(error: reqwest::Error) -> ApiError {
    ApiError::status(
        StatusCode::BAD_GATEWAY,
        format!("failed to check GitHub releases: {error}"),
    )
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

async fn update_settings(
    State(state): State<CoreState>,
    Json(mut config): Json<AppConfig>,
) -> Result<Json<GatewayStatus>, ApiError> {
    let _settings_update = state.settings_update.lock();
    config.gateway_key = config.gateway_key.trim().to_string();
    if config.gateway_key.is_empty() {
        return Err(ApiError::bad_request("gateway key is required"));
    }
    config.validate_timeouts().map_err(ApiError::bad_request)?;
    validate_upstream_url(&config.upstream_base_url)?;
    config.client_root_url =
        normalize_client_root_url(&config.client_root_url).map_err(ApiError::bad_request)?;
    let previous_config = state.config();
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
    Ok(Json(status_from_state(&state)))
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
    Ok(Json(serde_json::json!({ "key": config.gateway_key })))
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

#[derive(Deserialize)]
struct ForwardLogQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    status: Option<String>,
    account_id: Option<String>,
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
    state
        .db
        .lock()
        .query_forward_logs(
            q.limit.unwrap_or(100),
            q.offset.unwrap_or(0),
            q.status.as_deref(),
            q.account_id.as_deref(),
        )
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
        AccountUsageUpdate, asset_path, dashboard_account, dashboard_summary, is_update_available,
        parse_stable_version, update_account_usage,
    };
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::db::Database;
    use crate::models::Account;
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
            recharge_date: None,
            cooldown_until: None,
            last_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let dto = dashboard_account(account);

        assert_eq!(dto.username, "user");
        assert!(dto.password.is_empty());
        assert!(dto.key.is_empty());
        let _ = fs::remove_dir_all(dir);
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
            recharge_date: None,
            cooldown_until: None,
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

    fn version_parts(version: &str) -> [u64; 3] {
        parse_stable_version(version)
            .expect("test version should be valid")
            .0
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
