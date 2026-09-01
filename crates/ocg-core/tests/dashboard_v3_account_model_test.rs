//! Dashboard V3 exact-account operational model tests.

use axum::Router;
use axum::body::Bytes;
use axum::extract::OriginalUri;
use axum::http::{HeaderMap, Method as HttpMethod};
use axum::routing::any;
use chrono::Utc;
use ocg_core::models::ProxyMode;
use ocg_core::provider::CUSTOM_PROVIDER_ID;
use reqwest::{Method, StatusCode};
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};

#[path = "fixtures/dashboard_v3/harness.rs"]
mod harness;

use harness::{V3Harness, start_loopback};

const TARGET_KEY: &str = "sk-account-model-target-secret";
const SIBLING_KEY: &str = "sk-account-model-sibling-secret";
const CUSTOM_KEY: &str = "custom-account-model-secret";

#[derive(Clone, Debug)]
struct CapturedCall {
    path: String,
    authorization: Option<String>,
    x_api_key: Option<String>,
    body: Value,
}

struct ProbeOrigin {
    url: String,
    calls: Arc<Mutex<Vec<CapturedCall>>>,
    _stop: tokio::sync::oneshot::Sender<()>,
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

async fn start_origin() -> ProbeOrigin {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_handler = calls.clone();
    let target_auth = format!("Bearer {TARGET_KEY}");
    let app = Router::new().fallback(any(
        move |_method: HttpMethod, uri: OriginalUri, headers: HeaderMap, body: Bytes| {
            let calls = calls_for_handler.clone();
            let target_auth = target_auth.clone();
            async move {
                let authorization = header_value(&headers, "authorization");
                let x_api_key = header_value(&headers, "x-api-key");
                calls.lock().unwrap().push(CapturedCall {
                    path: uri.0.path().to_string(),
                    authorization: authorization.clone(),
                    x_api_key,
                    body: serde_json::from_slice(&body).unwrap_or(Value::Null),
                });
                if authorization.as_deref() == Some(target_auth.as_str()) {
                    (
                        StatusCode::UNAUTHORIZED,
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        format!(r#"{{"error":"rejected {TARGET_KEY}"}}"#),
                    )
                } else {
                    (
                        StatusCode::OK,
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        r#"{"id":"ok","object":"response"}"#.to_string(),
                    )
                }
            }
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown.await;
            })
            .await
            .ok();
    });
    ProbeOrigin {
        url: format!("http://{address}"),
        calls,
        _stop: stop,
    }
}

fn cas(harness: &V3Harness, patch: Value) -> Value {
    let mut body = match patch {
        Value::Object(map) => map,
        _ => Map::new(),
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

async fn send_json(
    harness: &V3Harness,
    method: Method,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = harness
        .client
        .request(method, format!("{}{path}", harness.v3_base))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

async fn create_go_account(harness: &V3Harness, name: &str, key: &str) -> String {
    let (status, body) = send_json(
        harness,
        Method::POST,
        "/accounts",
        cas(harness, json!({"name": name, "key": key})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["account"]["id"].as_str().unwrap().to_string()
}

fn point_upstream(harness: &V3Harness, origin: &ProbeOrigin) {
    let mut config = harness.state.config();
    config.upstream_base_url = origin.url.clone();
    config.proxy_mode = ProxyMode::Direct;
    config.non_stream_timeout_secs = 5;
    harness.state.set_config(config).unwrap();
}

#[tokio::test]
async fn model_test_locks_the_requested_disabled_cooling_account_and_does_not_mutate_it() {
    let harness = start_loopback("account-model-test-locked").await;
    let origin = start_origin().await;
    point_upstream(&harness, &origin);
    let target = create_go_account(&harness, "Target", TARGET_KEY).await;
    let _sibling = create_go_account(&harness, "Healthy sibling", SIBLING_KEY).await;
    harness
        .state
        .db
        .lock()
        .set_account_cooldown(
            &target,
            Some(Utc::now() + chrono::Duration::hours(1)),
            Some("pre-existing cooldown"),
        )
        .unwrap();
    let (disabled_status, disabled) = send_json(
        &harness,
        Method::PATCH,
        &format!("/accounts/{target}"),
        cas(&harness, json!({"enabled": false})),
    )
    .await;
    assert_eq!(disabled_status, StatusCode::OK, "{disabled}");
    let before_revision = harness.state.settings_revision();
    let before = harness
        .state
        .db
        .lock()
        .get_account(&target)
        .unwrap()
        .unwrap();

    let (status, body) = send_json(
        &harness,
        Method::POST,
        &format!("/accounts/{target}/model-tests"),
        json!({"modelId":"grok-4.5"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["accountId"], target);
    assert_eq!(body["modelId"], "grok-4.5");
    assert_eq!(body["success"], false);
    assert_eq!(body["httpStatus"], 401);
    assert!(body["durationMs"].as_u64().is_some());
    assert!(!body.to_string().contains(TARGET_KEY), "{body}");
    assert!(!body.to_string().contains("rejected"), "{body}");
    assert_eq!(harness.state.settings_revision(), before_revision);
    let after = harness
        .state
        .db
        .lock()
        .get_account(&target)
        .unwrap()
        .unwrap();
    assert!(!after.enabled);
    assert_eq!(after.cooldown_until, before.cooldown_until);
    let calls = origin.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "{calls:?}");
    let target_authorization = format!("Bearer {TARGET_KEY}");
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some(target_authorization.as_str())
    );
    assert_eq!(calls[0].path, "/v1/responses");
    harness.stop();
}

#[tokio::test]
async fn model_test_rejects_unknown_models_before_outbound() {
    let harness = start_loopback("account-model-test-unknown").await;
    let origin = start_origin().await;
    point_upstream(&harness, &origin);
    let account = create_go_account(&harness, "Known", SIBLING_KEY).await;
    let (status, body) = send_json(
        &harness,
        Method::POST,
        &format!("/accounts/{account}/model-tests"),
        json!({"modelId":"not-a-current-model"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "invalidRequest");
    assert!(origin.calls.lock().unwrap().is_empty());
    harness.stop();
}

#[tokio::test]
async fn custom_model_test_uses_the_declared_protocol_and_route_without_secrets() {
    let harness = start_loopback("account-model-test-custom").await;
    let origin = start_origin().await;
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/accounts",
        cas(
            &harness,
            json!({
                "name":"Custom",
                "key": CUSTOM_KEY,
                "providerId": CUSTOM_PROVIDER_ID,
                "customConfig": {
                    "endpointUrl": origin.url,
                    "upstreamProtocol": "responses"
                },
                "modelCapabilities": [{
                    "publicModel":"declared-custom-model",
                    "upstreamModel":"upstream-custom-model:latest",
                    "protocol":"responses"
                }]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let account = created["account"]["id"].as_str().unwrap();
    let before_revision = harness.state.settings_revision();
    let (status, body) = send_json(
        &harness,
        Method::POST,
        &format!("/accounts/{account}/model-tests"),
        json!({"modelId":"declared-custom-model"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["protocol"], "responses");
    assert_eq!(body["modelId"], "declared-custom-model");
    assert_eq!(body["success"], true);
    assert_eq!(body["httpStatus"], 200);
    assert!(!body.to_string().contains(CUSTOM_KEY), "{body}");
    assert_eq!(harness.state.settings_revision(), before_revision);
    let calls = origin.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].path, "/v1/responses");
    assert_eq!(calls[0].body["model"], "upstream-custom-model:latest");
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer custom-account-model-secret")
    );
    assert_eq!(calls[0].x_api_key, None);
    harness.stop();
}
