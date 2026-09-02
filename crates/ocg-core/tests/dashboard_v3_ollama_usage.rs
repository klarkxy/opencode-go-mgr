//! Dashboard V3 integration regressions for the Ollama Cloud Cookie usage
//! capability: Cookie validation/storage, the bounded manual refresh, the
//! 30s throttle and fixed backoff ladder, the cooldown isolation contract,
//! lifecycle (clear/disable/delete), and the sanitized API/export surface.

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use chrono::Utc;
use ocg_core::models::{Account, AccountType};
use ocg_core::provider::{OLLAMA_CLOUD_OFFERING_ID, OLLAMA_PROVIDER_ID};
use reqwest::Method;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "fixtures/dashboard_v3/harness.rs"]
mod harness;

use harness::{V3Harness, start_loopback};

struct SettingsOrigin {
    url: String,
    calls: Arc<Mutex<Vec<RecordedCall>>>,
    _stop: tokio::sync::oneshot::Sender<()>,
}

#[derive(Clone, Debug)]
struct RecordedCall {
    path: String,
    cookie: Option<String>,
    authorization: Option<String>,
}

async fn start_settings_origin(status: StatusCode, body: &'static str) -> SettingsOrigin {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_handler = calls.clone();
    let app = Router::new().fallback(get(
        move |uri: axum::http::Uri, headers: axum::http::HeaderMap| {
            let calls = calls_for_handler.clone();
            async move {
                calls.lock().unwrap().push(RecordedCall {
                    path: uri.path().to_string(),
                    cookie: headers
                        .get(axum::http::header::COOKIE)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    authorization: headers
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                });
                (
                    status,
                    [(axum::http::header::CONTENT_TYPE, "text/html")],
                    body,
                )
            }
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown.await;
            })
            .await
            .ok();
    });
    SettingsOrigin {
        url: format!("http://{addr}"),
        calls,
        _stop: stop,
    }
}

const SETTINGS_PAGE: &str = concat!(
    r#"<html><body><span>Plan: Maker</span><span>Balance: $3.20</span>"#,
    r#"<div data-usage-track="5h" data-time="2026-09-01T00:00:00Z" data-used-percent="42"></div>"#,
    r#"<div data-usage-track="7d" data-used-percent="12.5"></div>"#,
    r#"<div data-model="gpt-oss:120b" data-requests="12" data-usage-window="5h"></div>"#,
    r#"<div data-model="gpt-oss:120b" data-requests="340" data-time="7d"></div>"#,
    r#"</body></html>"#,
);

const LOGIN_PAGE: &str =
    r#"<html><body><form action="/login">Sign in to continue</form></body></html>"#;

const BROKEN_PAGE: &str = r#"<html><body>no anchors at all</body></html>"#;

async fn send_json(
    harness: &V3Harness,
    method: Method,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = harness
        .client
        .request(method, format!("{}{}", harness.v3_base, path))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    (status, parsed)
}

fn cas(harness: &V3Harness, patch: Value) -> Value {
    let mut body = match patch {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    body.insert(
        "expectedRevision".into(),
        json!(harness.state.settings_revision()),
    );
    body.insert(
        "processGeneration".into(),
        json!(harness.state.process_generation()),
    );
    Value::Object(body)
}

fn base_ollama_account(id: &str) -> Account {
    let now = Utc::now();
    Account {
        id: id.into(),
        provider_id: OLLAMA_PROVIDER_ID.into(),
        offering_id: OLLAMA_CLOUD_OFFERING_ID.into(),
        credential_kind: ocg_core::provider::default_credential_kind(),
        quota_scope: ocg_core::provider::default_quota_scope(),
        name: id.into(),
        username: None,
        password_cipher: None,
        key_cipher: String::new(),
        enabled: true,
        account_type: AccountType::Key,
        setup_step: ocg_core::models::AccountSetupStep::Ready,
        referral_code: None,
        purchase_date: String::new(),
        expires_on: String::new(),
        cooldown_until: None,
        cooldown_generic_until: None,
        cooldown_5h_until: None,
        cooldown_week_until: None,
        cooldown_month_until: None,
        cooldown_free_until: None,
        last_error: None,
        auth_error: None,
        notes: None,
        created_at: now,
        updated_at: now,
    }
}

/// The offering admits persisted enablement; disabled-account gating is
/// exercised separately by `ollama_usage_refresh_requires_enabled_account_and_configured_cookie`.

#[tokio::test]
async fn ollama_cookie_roundtrip_validates_rejects_set_cookie_and_never_echoes() {
    let harness = start_loopback("ollama-cookie").await;
    let account = base_ollama_account("ollama-cookie-1");
    harness.state.db.lock().create_account(&account).unwrap();

    let (status, body) = send_json(
        &harness,
        Method::PUT,
        "/accounts/ollama-cookie-1/ollama-cookie",
        cas(
            &harness,
            json!({ "cookie": "session=abc; Path=/; HttpOnly" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| message.contains("Set-Cookie")),
        "{body}"
    );

    let (status, body) = send_json(
        &harness,
        Method::PUT,
        "/accounts/ollama-cookie-1/ollama-cookie",
        cas(&harness, json!({ "cookie": "session=abc; theme=dark" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["cookieConfigured"].as_bool().unwrap());
    assert_eq!(body["status"], "unconfigured");
    let encoded = serde_json::to_string(&body).unwrap();
    assert!(
        !encoded.contains("session=abc"),
        "the API never echoes the Cookie plaintext: {encoded}"
    );

    // Clearing returns the capability to the unconfigured state.
    let (status, body) = send_json(
        &harness,
        Method::PUT,
        "/accounts/ollama-cookie-1/ollama-cookie",
        cas(&harness, json!({ "cookie": null })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body["cookieConfigured"].as_bool().unwrap());
    assert!(
        harness
            .state
            .db
            .lock()
            .ollama_cloud_usage_state("ollama-cookie-1")
            .unwrap()
            .is_none(),
        "the state row is removed with the Cookie"
    );

    // Non-Ollama accounts are rejected outright.
    let (status, _body) = send_json(
        &harness,
        Method::PUT,
        "/accounts/00000000-0000-0000-0000-000000000002/ollama-cookie",
        cas(&harness, json!({ "cookie": "a=1" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    harness.stop();
}

#[tokio::test]
async fn ollama_usage_refresh_scrapes_sanitizes_and_throttles() {
    let harness = start_loopback("ollama-refresh").await;
    let account = base_ollama_account("ollama-refresh-1");
    harness.state.db.lock().create_account(&account).unwrap();
    let origin = start_settings_origin(StatusCode::OK, SETTINGS_PAGE).await;
    let _guard = ocg_core::goat::install_ollama_models_origin_for_test(
        harness.state.process_generation(),
        origin.url.clone(),
    )
    .unwrap();

    let (status, body) = send_json(
        &harness,
        Method::PUT,
        "/accounts/ollama-refresh-1/ollama-cookie",
        cas(&harness, json!({ "cookie": "session=abc; theme=dark" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = send_json(
        &harness,
        Method::POST,
        "/accounts/ollama-refresh-1/ollama-usage/refresh",
        cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["snapshot"]["plan"], "Maker");
    assert_eq!(body["snapshot"]["balance"], "$3.20");
    assert_eq!(body["snapshot"]["windows"][0]["window"], "5h");
    assert_eq!(body["snapshot"]["windows"][0]["used_percent"], 42.0);
    assert_eq!(body["snapshot"]["models"][0]["model"], "gpt-oss:120b");
    let encoded = serde_json::to_string(&body).unwrap();
    assert!(
        !encoded.contains("session=abc")
            && !encoded.contains("data-usage")
            && !encoded.contains('<'),
        "the response is free of Cookie plaintext and HTML: {encoded}"
    );

    // The scrape carried only the account Cookie: no Authorization, exact path.
    // Clone-and-drop the guard: clippy forbids holding a std MutexGuard
    // across the awaits inside the surrounding async test.
    let calls: Vec<RecordedCall> = origin.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/settings");
    assert_eq!(calls[0].cookie.as_deref(), Some("session=abc; theme=dark"));
    assert!(
        calls[0].authorization.is_none(),
        "no dashboard or upstream API key may ride the scrape"
    );

    // 30-second manual throttle (successes count) surfaces as 429 with the
    // absolute retry instant.
    let (status, body) = send_json(
        &harness,
        Method::POST,
        "/accounts/ollama-refresh-1/ollama-usage/refresh",
        cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "throttled");
    assert!(body["nextAllowedAt"].as_str().is_some());
    let cooldowns_unchanged = harness
        .state
        .db
        .lock()
        .get_account("ollama-refresh-1")
        .unwrap()
        .unwrap();
    assert!(cooldowns_unchanged.cooldown_until.is_none());
    assert!(cooldowns_unchanged.cooldown_generic_until.is_none());
    assert!(cooldowns_unchanged.cooldown_5h_until.is_none());
    assert!(
        cooldowns_unchanged.last_error.is_none() && cooldowns_unchanged.auth_error.is_none(),
        "usage paths never write inference cooldown or account error state"
    );

    // A later read still serves the last successful snapshot.
    let (status, body) = send_json(
        &harness,
        Method::GET,
        "/accounts/ollama-refresh-1/ollama-usage",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["snapshot"]["plan"], "Maker");

    harness.stop();
}

#[tokio::test]
async fn ollama_usage_failures_keep_the_last_snapshot_and_enter_backoff() {
    let harness = start_loopback("ollama-failure").await;
    let account = base_ollama_account("ollama-failure-1");
    harness.state.db.lock().create_account(&account).unwrap();

    // Seed a successful snapshot directly, then fail twice.
    let now = Utc::now();
    let cipher = harness.state.encrypt_key("session=abc").unwrap();
    {
        let db = harness.state.db.lock();
        db.set_ollama_cloud_cookie("ollama-failure-1", &cipher)
            .unwrap();
        db.commit_ollama_cloud_usage_success(
            "ollama-failure-1",
            r#"{"windows":[{"window":"5h","used_percent":10.0,"reset_at":null}],"models":[],"plan":"Maker","balance":null}"#,
            now - chrono::Duration::hours(2),
            Some(now - chrono::Duration::hours(2)),
        )
        .unwrap();
    }
    harness.state.reload_provider_contracts().unwrap();

    let broken = start_settings_origin(StatusCode::OK, BROKEN_PAGE).await;
    let _guard = ocg_core::goat::install_ollama_models_origin_for_test(
        harness.state.process_generation(),
        broken.url.clone(),
    )
    .unwrap();
    let (status, body) = send_json(
        &harness,
        Method::POST,
        "/accounts/ollama-failure-1/ollama-usage/refresh",
        cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "failed");
    // The failed attempt must not clear the last successful snapshot.
    assert_eq!(
        body["snapshot"]["windows"][0]["used_percent"], 10.0,
        "failures preserve the last successful snapshot"
    );
    assert_eq!(body["failureStreak"], 1);
    let next_eligible = body["nextEligibleAt"].as_str().unwrap().to_string();
    let eligible = chrono::DateTime::parse_from_rfc3339(&next_eligible)
        .unwrap()
        .with_timezone(&Utc);
    let in_five_minutes = (eligible - Utc::now()).num_seconds();
    assert!(
        (4 * 60..=6 * 60).contains(&in_five_minutes),
        "first failure backs off five minutes, got {in_five_minutes}s"
    );
    // The failure left no cooldown on the account.
    let account_after = harness
        .state
        .db
        .lock()
        .get_account("ollama-failure-1")
        .unwrap()
        .unwrap();
    assert!(account_after.cooldown_until.is_none());
    assert!(account_after.enabled);

    // Unauthorized pages flip the status without touching the snapshot.
    let login = start_settings_origin(StatusCode::OK, LOGIN_PAGE).await;
    let _login_guard = ocg_core::goat::install_ollama_models_origin_for_test(
        harness.state.process_generation(),
        login.url.clone(),
    )
    .unwrap();
    // Expire the backoff so the manual window is the only gate.
    {
        let db = harness.state.db.lock();
        db.record_ollama_cloud_usage_failure(
            "ollama-failure-1",
            "failed",
            None,
            now - chrono::Duration::hours(1),
            None,
            1,
        )
        .unwrap();
    }
    let (status, body) = send_json(
        &harness,
        Method::POST,
        "/accounts/ollama-failure-1/ollama-usage/refresh",
        cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "unauthorized");
    assert!(
        body["snapshot"]["windows"][0]["used_percent"] == 10.0,
        "unauthorized also preserves the snapshot"
    );

    harness.stop();
}

#[tokio::test]
async fn ollama_usage_redirect_is_a_failure_that_enters_backoff() {
    let harness = start_loopback("ollama-redirect").await;
    let account = base_ollama_account("ollama-redirect-1");
    harness.state.db.lock().create_account(&account).unwrap();
    // 302 Found: the scrape must treat any redirect as a failure and never
    // follow it (spec: 重定向视为失败).
    let origin = start_settings_origin(StatusCode::FOUND, LOGIN_PAGE).await;
    let _guard = ocg_core::goat::install_ollama_models_origin_for_test(
        harness.state.process_generation(),
        origin.url.clone(),
    )
    .unwrap();
    let cipher = harness.state.encrypt_key("session=abc").unwrap();
    harness
        .state
        .db
        .lock()
        .set_ollama_cloud_cookie("ollama-redirect-1", &cipher)
        .unwrap();
    let (status, body) = send_json(
        &harness,
        Method::POST,
        "/accounts/ollama-redirect-1/ollama-usage/refresh",
        cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "failed");
    assert_eq!(body["failureStreak"], 1);
    // The redirect must not have been followed: still exactly one call.
    assert_eq!(origin.calls.lock().unwrap().len(), 1);
    let next_eligible = body["nextEligibleAt"].as_str().unwrap().to_string();
    let eligible = chrono::DateTime::parse_from_rfc3339(&next_eligible)
        .unwrap()
        .with_timezone(&Utc);
    let wait = (eligible - Utc::now()).num_seconds();
    assert!(
        (4 * 60..=6 * 60).contains(&wait),
        "the redirect failure enters the fixed backoff ladder, got {wait}s"
    );
    // The sanitized reason is persisted and served without URLs or HTML.
    let last_error = body["lastError"].as_str().unwrap_or("");
    assert!(
        !last_error.contains('?') && !last_error.contains('<'),
        "{last_error}"
    );
    let account_after = harness
        .state
        .db
        .lock()
        .get_account("ollama-redirect-1")
        .unwrap()
        .unwrap();
    assert!(
        account_after.cooldown_until.is_none(),
        "no inference cooldown"
    );
    assert!(account_after.enabled, "routing eligibility untouched");

    harness.stop();
}

#[tokio::test]
async fn ollama_usage_refresh_requires_enabled_account_and_configured_cookie() {
    let harness = start_loopback("ollama-gates").await;
    let mut disabled = base_ollama_account("ollama-gate-1");
    disabled.enabled = false;
    harness.state.db.lock().create_account(&disabled).unwrap();

    let (status, body) = send_json(
        &harness,
        Method::POST,
        "/accounts/ollama-gate-1/ollama-usage/refresh",
        cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| message.contains("Cookie")),
        "an unconfigured Cookie is rejected before any outbound call: {body}"
    );

    let cipher = harness.state.encrypt_key("session=abc").unwrap();
    harness
        .state
        .db
        .lock()
        .set_ollama_cloud_cookie("ollama-gate-1", &cipher)
        .unwrap();
    let (status, body) = send_json(
        &harness,
        Method::POST,
        "/accounts/ollama-gate-1/ollama-usage/refresh",
        cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| message.contains("disabled")),
        "a disabled account keeps the usage entry unavailable: {body}"
    );

    harness.stop();
}

#[tokio::test]
async fn ollama_usage_and_cookies_stay_out_of_export_payloads() {
    let harness = start_loopback("ollama-export").await;
    let account = base_ollama_account("ollama-export-1");
    harness.state.db.lock().create_account(&account).unwrap();
    // Export decrypts every account credential; give the draft a Key via
    // the same loopback SQLite poke the GOAT fixtures use.
    let conn_key = rusqlite::Connection::open(harness.dir.join("data.sqlite")).unwrap();
    conn_key.busy_timeout(Duration::from_millis(5_000)).unwrap();
    let key_cipher = harness.state.encrypt_key("sk-export-test").unwrap();
    conn_key
        .execute(
            "UPDATE accounts SET key_cipher = ?2 WHERE id = ?1",
            rusqlite::params!["ollama-export-1", key_cipher],
        )
        .unwrap();
    let cipher = harness.state.encrypt_key("session=supersecret").unwrap();
    {
        let db = harness.state.db.lock();
        db.set_ollama_cloud_cookie("ollama-export-1", &cipher)
            .unwrap();
        db.commit_ollama_cloud_usage_success(
            "ollama-export-1",
            r#"{"windows":[{"window":"7d","used_percent":3.0,"reset_at":null}],"models":[],"plan":"Maker","balance":"$1"}"#,
            Utc::now(),
            Some(Utc::now()),
        )
        .unwrap();
    }

    let (status, body) = send_json(
        &harness,
        Method::POST,
        "/accounts/transfer/export",
        json!({ "bundlePassword": "export-passphrase-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let encoded = serde_json::to_string(&body).unwrap();
    assert!(
        !encoded.contains("supersecret"),
        "the encrypted export bundle never carries the Cookie plaintext"
    );
    assert!(
        !encoded.contains("usage_state") && !encoded.contains("usageState"),
        "the export payload structure stays unchanged"
    );
    // The row itself survives locally; only the payload omits it.
    assert!(
        harness
            .state
            .db
            .lock()
            .ollama_cloud_usage_state("ollama-export-1")
            .unwrap()
            .is_some()
    );

    harness.stop();
}
