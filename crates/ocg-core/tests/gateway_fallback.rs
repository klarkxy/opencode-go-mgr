use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use chrono::Utc;
use ocg_core::admin;
use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::models::{Account, ForwardLog};
use ocg_core::state::{CoreStateInner, GatewayHandle};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::net::TcpListener as StdTcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct MockReply {
    status: u16,
    body: &'static str,
}

#[derive(Clone)]
struct MockCall {
    key: String,
    accept_encoding: Option<String>,
}

#[derive(Clone)]
struct MockState {
    replies: Arc<Mutex<HashMap<String, VecDeque<MockReply>>>>,
    calls: Arc<Mutex<Vec<MockCall>>>,
}

const LIMITED_BODY: &str = r#"{"type":"error","error":{"type":"GoUsageLimitError","message":"Weekly usage limit reached. Resets in 3 days."}}"#;
const SUCCESS_BODY: &str = r#"{"id":"ok","object":"chat.completion","model":"deepseek-v4-flash","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}"#;

fn temp_data_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "ocg-gateway-test-{}-{}",
        label,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn free_port() -> u16 {
    let listener = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

async fn start_mock_upstream(
    replies: HashMap<String, VecDeque<MockReply>>,
) -> (
    String,
    Arc<Mutex<Vec<MockCall>>>,
    tokio::sync::oneshot::Sender<()>,
) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let state = MockState {
        replies: Arc::new(Mutex::new(replies)),
        calls: calls.clone(),
    };
    let app = Router::new()
        .route("/v1/chat/completions", post(mock_chat))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        let _ = server.await;
    });
    (format!("http://{}", addr), calls, shutdown_tx)
}

async fn mock_chat(State(state): State<MockState>, headers: HeaderMap) -> impl IntoResponse {
    let key = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string();
    let accept_encoding = headers
        .get(axum::http::header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    state.calls.lock().unwrap().push(MockCall {
        key: key.clone(),
        accept_encoding,
    });

    let reply = {
        let mut replies = state.replies.lock().unwrap();
        let queue = replies.entry(key).or_insert_with(|| {
            VecDeque::from([MockReply {
                status: 500,
                body: r#"{"error":"unexpected key"}"#,
            }])
        });
        if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else {
            queue.front().unwrap().clone()
        }
    };

    (
        StatusCode::from_u16(reply.status).unwrap(),
        [("content-type", "application/json")],
        reply.body,
    )
}

fn build_state(base_url: String, keys: &[&str]) -> (Arc<CoreStateInner>, PathBuf) {
    let dir = temp_data_dir("state");
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
    let db = Database::open(dir.clone()).unwrap();
    let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
    let mut config = state.config();
    config.gateway_key = "gw-test".into();
    config.upstream_base_url = base_url;
    state.set_config(config).unwrap();

    let now = Utc::now();
    for (idx, key) in keys.iter().enumerate() {
        let account = Account {
            id: format!("acct-{}", idx + 1),
            name: format!("acct-{}", idx + 1),
            username: None,
            password_cipher: None,
            key_cipher: state.encrypt_key(key).unwrap(),
            enabled: true,
            referral_code: None,
            recharge_date: None,
            cooldown_until: None,
            last_error: None,
            created_at: now + chrono::Duration::seconds(idx as i64),
            updated_at: now + chrono::Duration::seconds(idx as i64),
        };
        state.db.lock().create_account(&account).unwrap();
    }

    (state, dir)
}

async fn start_gateway(state: Arc<CoreStateInner>) -> (u16, GatewayHandle) {
    let port = free_port();
    let handle = gateway::start_gateway(state, port).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (port, handle)
}

async fn chat(port: u16) -> (u16, String) {
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", port))
        .header(reqwest::header::AUTHORIZATION, "Bearer gw-test")
        .header(reqwest::header::ACCEPT_ENCODING, "gzip")
        .json(&serde_json::json!({
            "model": "deepseek-v4-flash",
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": false
        }))
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = response.text().await.unwrap();
    (status, body)
}

#[tokio::test]
async fn falls_back_past_five_limited_accounts_to_sixth_success() {
    let replies = (1..=6)
        .map(|i| {
            let reply = if i == 6 {
                MockReply {
                    status: 200,
                    body: SUCCESS_BODY,
                }
            } else {
                MockReply {
                    status: 429,
                    body: LIMITED_BODY,
                }
            };
            (format!("key-{}", i), VecDeque::from([reply]))
        })
        .collect();
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let keys = ["key-1", "key-2", "key-3", "key-4", "key-5", "key-6"];
    let (state, dir) = build_state(base_url, &keys);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = chat(port).await;
    assert_eq!(status, 200);

    let call_keys = calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        call_keys,
        keys.iter().map(|k| k.to_string()).collect::<Vec<_>>()
    );
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .all(|c| c.accept_encoding.as_deref() == Some("identity"))
    );

    let db = state.db.lock();
    let accounts = db.list_accounts().unwrap();
    assert_eq!(
        accounts
            .iter()
            .filter(|a| a.cooldown_until.is_some())
            .count(),
        5
    );
    let logs = db.list_forward_logs(20).unwrap();
    assert!(
        logs.iter()
            .any(|l| l.account_name == "acct-6" && l.status == "success")
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn retries_transient_failure_once_on_same_account_before_fallback() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([
                MockReply {
                    status: 500,
                    body: r#"{"error":"temporary"}"#,
                },
                MockReply {
                    status: 500,
                    body: r#"{"error":"still temporary"}"#,
                },
            ]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = chat(port).await;
    assert_eq!(status, 200);

    let call_keys = calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(call_keys, ["key-1", "key-1", "key-2"].map(str::to_string));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn keeps_same_account_when_transient_retry_succeeds() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([
                MockReply {
                    status: 500,
                    body: r#"{"error":"temporary"}"#,
                },
                MockReply {
                    status: 200,
                    body: SUCCESS_BODY,
                },
            ]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = chat(port).await;
    assert_eq!(status, 200);

    let call_keys = calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(call_keys, ["key-1", "key-1"].map(str::to_string));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn all_limited_accounts_return_429_with_soonest_reset() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 429,
                body: LIMITED_BODY,
            }]),
        ),
        (
            "key-2".to_string(),
            VecDeque::from([MockReply {
                status: 429,
                body: LIMITED_BODY,
            }]),
        ),
    ]);
    let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = chat(port).await;
    assert_eq!(status, 429);
    assert!(body.contains("resets_at"));
    assert_eq!(
        state
            .db
            .lock()
            .list_accounts()
            .unwrap()
            .iter()
            .filter(|a| a.cooldown_until.is_some())
            .count(),
        2
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn admin_health_works_with_bearer_token() {
    let (state, dir) = build_state("http://127.0.0.1:1".into(), &[]);
    let port = free_port();
    let handle = admin::start_admin(state, port, "admin-token".into())
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let response = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{}/admin/health", port))
        .header(reqwest::header::AUTHORIZATION, "Bearer admin-token")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.text().await.unwrap().contains("\"status\":\"ok\""));

    admin::stop_admin(handle);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn admin_status_requires_token_and_hides_secrets() {
    let (state, dir) = build_state(
        "http://127.0.0.1:1".into(),
        &["secret-key-1", "secret-key-2"],
    );
    let gateway_port = free_port();
    let mut config = state.config();
    config.gateway_port = gateway_port;
    state.set_config(config).unwrap();

    let gateway_handle = gateway::start_gateway(state.clone(), gateway_port)
        .await
        .unwrap();
    *state.gateway.lock() = Some(gateway_handle);
    state
        .db
        .lock()
        .set_account_cooldown(
            "acct-1",
            Some(Utc::now() + chrono::Duration::days(1)),
            Some("limit reached"),
        )
        .unwrap();
    state
        .db
        .lock()
        .log_forward(&ForwardLog {
            id: 0,
            timestamp: Utc::now(),
            model: "deepseek-v4-flash".into(),
            account_id: "acct-2".into(),
            account_name: "acct-2".into(),
            status: "success".into(),
            http_status: Some(200),
            prompt_tokens: 10,
            completion_tokens: 2,
            cached_tokens: 0,
            cost: 1.25,
            error_message: None,
        })
        .unwrap();
    state
        .db
        .lock()
        .log_gateway("warn", "gateway", "recent status warning")
        .unwrap();

    let admin_port = free_port();
    let admin_handle = admin::start_admin(state.clone(), admin_port, "admin-token".into())
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let client = reqwest::Client::new();

    let unauthorized = client
        .get(format!("http://127.0.0.1:{}/admin/status", admin_port))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let response = client
        .get(format!("http://127.0.0.1:{}/admin/status", admin_port))
        .header(reqwest::header::AUTHORIZATION, "Bearer admin-token")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(!body.contains("admin-token"));
    assert!(!body.contains("gw-test"));
    assert!(!body.contains("secret-key-1"));
    assert!(!body.contains("secret-key-2"));
    assert!(!body.contains("key_cipher"));

    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["gateway"]["running"].as_bool(), Some(true));
    assert_eq!(value["gateway"]["port"].as_u64(), Some(gateway_port as u64));
    assert_eq!(value["accounts"]["total"].as_u64(), Some(2));
    assert_eq!(value["accounts"]["enabled"].as_u64(), Some(2));
    assert_eq!(value["accounts"]["cooldown"].as_u64(), Some(1));
    assert_eq!(value["accounts"]["available"].as_u64(), Some(1));
    assert_eq!(value["usage"]["today_cost"].as_f64(), Some(1.25));
    assert_eq!(value["usage"]["week_cost"].as_f64(), Some(1.25));
    assert_eq!(value["usage"]["month_cost"].as_f64(), Some(1.25));
    assert_eq!(value["last_error"].as_str(), Some("recent status warning"));

    admin::stop_admin(admin_handle);
    if let Some(handle) = state.gateway.lock().take() {
        gateway::stop_gateway(handle);
    }
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn admin_keys_upsert_preserves_keys() {
    let (state, dir) = build_state("http://127.0.0.1:1".into(), &[]);
    let admin_port = free_port();
    let admin_handle = admin::start_admin(state.clone(), admin_port, "admin-token".into())
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let client = reqwest::Client::new();
    let created_at = (Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
    let first_updated_at = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();

    let create = client
        .post(format!("http://127.0.0.1:{}/admin/keys", admin_port))
        .header(reqwest::header::AUTHORIZATION, "Bearer admin-token")
        .json(&serde_json::json!({
            "id": "remote-1",
            "name": "remote",
            "key": "plain-secret",
            "enabled": true,
            "referral_code": "REF",
            "recharge_date": "15",
            "created_at": created_at,
            "updated_at": first_updated_at,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::NO_CONTENT);

    let stored = state.db.lock().get_account("remote-1").unwrap().unwrap();
    assert_ne!(stored.key_cipher, "plain-secret");
    assert_eq!(
        state.decrypt_key(&stored.key_cipher).unwrap(),
        "plain-secret"
    );

    let newer_updated_at = Utc::now().to_rfc3339();
    let update_without_key = client
        .post(format!("http://127.0.0.1:{}/admin/keys", admin_port))
        .header(reqwest::header::AUTHORIZATION, "Bearer admin-token")
        .json(&serde_json::json!({
            "id": "remote-1",
            "name": "renamed",
            "enabled": false,
            "referral_code": "NEW",
            "recharge_date": "20",
            "created_at": created_at,
            "updated_at": newer_updated_at,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(update_without_key.status(), StatusCode::NO_CONTENT);

    let stored = state.db.lock().get_account("remote-1").unwrap().unwrap();
    assert_eq!(stored.name, "renamed");
    assert!(!stored.enabled);
    assert_eq!(stored.referral_code.as_deref(), Some("NEW"));
    assert_eq!(stored.recharge_date.as_deref(), Some("20"));
    assert_eq!(
        state.decrypt_key(&stored.key_cipher).unwrap(),
        "plain-secret"
    );

    let stale_update = client
        .post(format!("http://127.0.0.1:{}/admin/keys", admin_port))
        .header(reqwest::header::AUTHORIZATION, "Bearer admin-token")
        .json(&serde_json::json!({
            "id": "remote-1",
            "name": "stale",
            "key": "wrong-secret",
            "enabled": true,
            "created_at": created_at,
            "updated_at": first_updated_at,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_update.status(), StatusCode::NO_CONTENT);

    let stored = state.db.lock().get_account("remote-1").unwrap().unwrap();
    assert_eq!(stored.name, "renamed");
    assert_eq!(
        state.decrypt_key(&stored.key_cipher).unwrap(),
        "plain-secret"
    );

    let listed = client
        .get(format!("http://127.0.0.1:{}/admin/keys", admin_port))
        .header(reqwest::header::AUTHORIZATION, "Bearer admin-token")
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::METHOD_NOT_ALLOWED);

    admin::stop_admin(admin_handle);
    let _ = fs::remove_dir_all(dir);
}
