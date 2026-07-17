use axum::Router;
use axum::body::Body;
use axum::extract::{OriginalUri, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use chrono::{Duration, Utc};
use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::models::{Account, AccountUpdate};
use ocg_core::state::{CoreStateInner, GatewayHandle};
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::fs;
use std::net::TcpListener as StdTcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

#[derive(Clone)]
struct MockReply {
    status: u16,
    body: &'static str,
}

#[derive(Clone)]
struct MockCall {
    key: String,
    path: String,
    authorization: Option<String>,
    x_api_key: Option<String>,
    anthropic_version: Option<String>,
    body: String,
    accept_encoding: Option<String>,
}

#[derive(Clone)]
struct MockState {
    replies: Arc<Mutex<HashMap<String, VecDeque<MockReply>>>>,
    calls: Arc<Mutex<Vec<MockCall>>>,
}

#[derive(Clone)]
struct DelayedReply {
    content_type: &'static str,
    chunks: Vec<(StdDuration, &'static str)>,
    calls: Arc<AtomicUsize>,
}

const LIMITED_BODY: &str = r#"{"type":"error","error":{"type":"GoUsageLimitError","message":"Weekly usage limit reached. Resets in 3 days."}}"#;
const SUCCESS_BODY: &str = r#"{"id":"ok","object":"chat.completion","model":"deepseek-v4-flash","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}"#;
const MESSAGES_SUCCESS_BODY: &str = r#"{"id":"msg-ok","type":"message","role":"assistant","model":"minimax-m2.7","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":2,"cache_read_input_tokens":0}}"#;
const CHAT_STREAM_BODY: &str = concat!(
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"prompt_tokens_details\":{\"cached_tokens\":0}}}\n\n",
    "data: [DONE]\n\n"
);
const MESSAGES_STREAM_BODY: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-stream\",\"model\":\"minimax-m2.7\",\"usage\":{\"input_tokens\":6,\"cache_read_input_tokens\":4}}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
);
const MESSAGES_STREAM_HEAD: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-stream\",\"model\":\"minimax-m2.7\",\"usage\":{\"input_tokens\":6,\"cache_read_input_tokens\":4}}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n"
);
const MESSAGES_STREAM_TAIL: &str = concat!(
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
);
const CHAT_BAD_REQUEST_BODY: &str =
    r#"{"error":{"type":"invalid_request_error","message":"bad request"}}"#;

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
        .route("/v1/responses", post(mock_chat))
        .route("/v1/messages", post(mock_chat))
        .route("/v1/models", get(mock_chat))
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

async fn start_delayed_messages_upstream(
    content_type: &'static str,
    chunks: Vec<(StdDuration, &'static str)>,
) -> (String, Arc<AtomicUsize>, tokio::sync::oneshot::Sender<()>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/v1/messages", post(delayed_reply))
        .with_state(DelayedReply {
            content_type,
            chunks,
            calls: calls.clone(),
        });
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

async fn delayed_reply(State(state): State<DelayedReply>) -> Response {
    state.calls.fetch_add(1, Ordering::Relaxed);
    let stream =
        futures_util::stream::unfold(VecDeque::from(state.chunks), |mut chunks| async move {
            let (delay, chunk) = chunks.pop_front()?;
            tokio::time::sleep(delay).await;
            Some((
                Ok::<_, Infallible>(bytes::Bytes::from_static(chunk.as_bytes())),
                chunks,
            ))
        });
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", state.content_type)
        .body(Body::from_stream(stream))
        .unwrap()
}

async fn mock_chat(
    State(state): State<MockState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let x_api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let anthropic_version = headers
        .get("anthropic-version")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let key = authorization
        .as_deref()
        .and_then(|v| v.strip_prefix("Bearer "))
        .or(x_api_key.as_deref())
        .unwrap_or("")
        .to_string();
    let accept_encoding = headers
        .get(axum::http::header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    state.calls.lock().unwrap().push(MockCall {
        key: key.clone(),
        path: uri.path().to_string(),
        authorization,
        x_api_key,
        anthropic_version,
        body,
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

    let content_type = if reply.body.starts_with("data:") || reply.body.starts_with("event:") {
        "text/event-stream"
    } else {
        "application/json"
    };
    (
        StatusCode::from_u16(reply.status).unwrap(),
        [("content-type", content_type)],
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
            purchase_date: String::new(),
            expires_on: String::new(),
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

async fn models(port: u16) -> (StatusCode, String) {
    let response = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/v1/models"))
        .header(reqwest::header::AUTHORIZATION, "Bearer gw-test")
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    (status, body)
}

async fn protocol_call(port: u16, path: &str, model: &str) -> (StatusCode, serde_json::Value) {
    let body = match path {
        "/v1/chat/completions" => serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": false
        }),
        "/v1/responses" => serde_json::json!({
            "model": model,
            "input": "ping",
            "store": false,
            "max_output_tokens": 3,
            "stream": false
        }),
        "/v1/messages" => serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": false
        }),
        _ => panic!("unsupported test path: {path}"),
    };
    let client = reqwest::Client::new();
    let request = client
        .post(format!("http://127.0.0.1:{port}{path}"))
        .json(&body);
    let request = if path == "/v1/messages" {
        request
            .header("x-api-key", "gw-test")
            .header("anthropic-version", "2023-06-01")
    } else {
        request.header(reqwest::header::AUTHORIZATION, "Bearer gw-test")
    };
    let response = request.send().await.unwrap();
    let status = response.status();
    let body = response.json().await.unwrap();
    (status, body)
}

async fn protocol_stream_call(port: u16, path: &str, model: &str) -> (StatusCode, String) {
    let body = match path {
        "/v1/chat/completions" => serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": true
        }),
        "/v1/responses" => serde_json::json!({
            "model": model,
            "input": "ping",
            "store": false,
            "max_output_tokens": 3,
            "stream": true
        }),
        "/v1/messages" => serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": true
        }),
        _ => panic!("unsupported test path: {path}"),
    };
    let client = reqwest::Client::new();
    let request = client
        .post(format!("http://127.0.0.1:{port}{path}"))
        .json(&body);
    let request = if path == "/v1/messages" {
        request.header("x-api-key", "gw-test")
    } else {
        request.header(reqwest::header::AUTHORIZATION, "Bearer gw-test")
    };
    let response = request.send().await.unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    (status, body)
}

#[tokio::test]
async fn model_discovery_does_not_create_inference_logs() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: r#"{"object":"list","data":[{"id":"deepseek-v4-flash"}]}"#,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = models(port).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("deepseek-v4-flash"));
    assert_eq!(calls.lock().unwrap()[0].path, "/v1/models");
    let logs = state
        .db
        .lock()
        .query_forward_logs(10, 0, None, None, None, None, None, None, None)
        .unwrap();
    assert!(logs.items.is_empty());
    assert_eq!(logs.summary.total_requests, 0);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn model_discovery_keeps_rate_limit_cooldown_without_logging() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 429,
            body: LIMITED_BODY,
        }]),
    )]);
    let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, _) = models(port).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let stored = state.db.lock().get_account("acct-1").unwrap().unwrap();
    let remaining = stored.cooldown_until.unwrap() - Utc::now();
    assert!(remaining > Duration::days(2) && remaining <= Duration::days(3));
    let logs = state
        .db
        .lock()
        .query_forward_logs(10, 0, None, None, None, None, None, None, None)
        .unwrap();
    assert_eq!(logs.summary.total_requests, 0);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn application_models_falls_back_after_rate_limit() {
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
                status: 500,
                body: r#"{"error":"temporary failure"}"#,
            }]),
        ),
        (
            "key-3".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: r#"{"object":"list","data":[{"id":"deepseek-v4-flash"}]}"#,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2", "key-3"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let response = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{port}/dashboard/api/application-models"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!(["deepseek-v4-flash"])
    );
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-1", "key-2", "key-2", "key-3"]
    );
    assert!(
        state
            .db
            .lock()
            .get_account("acct-1")
            .unwrap()
            .unwrap()
            .cooldown_until
            .is_some()
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn application_models_skips_an_account_with_a_broken_key() {
    let replies = HashMap::from([(
        "key-2".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: r#"{"object":"list","data":[{"id":"deepseek-v4-flash"}]}"#,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2"]);
    state
        .db
        .lock()
        .update_account(
            "acct-1",
            &AccountUpdate {
                name: None,
                username: None,
                password: None,
                key: None,
                enabled: None,
                referral_code: None,
                purchase_date: None,
            },
            Some("not-a-valid-ciphertext"),
            None,
        )
        .unwrap();
    let (port, gateway_handle) = start_gateway(state).await;

    let response = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{port}/dashboard/api/application-models"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(calls.lock().unwrap()[0].key, "key-2");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn application_models_intersects_upstream_models_in_upstream_order() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: r#"{"object":"list","data":[{"id":"unknown"},{"id":"minimax-m2.7"},{"id":"deepseek-v4-flash"},{"id":"minimax-m2.7"},{"id":"qwen3.7-plus"}]}"#,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let response = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{port}/dashboard/api/application-models"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!(["minimax-m2.7", "deepseek-v4-flash", "qwen3.7-plus"])
    );
    assert_eq!(calls.lock().unwrap()[0].path, "/v1/models");
    assert_eq!(
        state
            .db
            .lock()
            .query_forward_logs(10, 0, None, None, None, None, None, None, None)
            .unwrap()
            .summary
            .total_requests,
        0
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn application_models_maps_upstream_failure_to_bad_gateway() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 500,
            body: r#"{"error":"upstream unavailable"}"#,
        }]),
    )]);
    let (base_url, _calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state).await;

    let response = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{port}/dashboard/api/application-models"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn routes_all_client_formats_to_each_models_native_protocol() {
    struct Case {
        client_path: &'static str,
        model: &'static str,
        upstream_path: &'static str,
        upstream_body: &'static str,
    }

    let cases = [
        Case {
            client_path: "/v1/chat/completions",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/chat/completions",
            upstream_body: SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/chat/completions",
            model: "minimax-m2.7",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/responses",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/chat/completions",
            upstream_body: SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/responses",
            model: "minimax-m2.7",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/messages",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/chat/completions",
            upstream_body: SUCCESS_BODY,
        },
        Case {
            client_path: "/v1/messages",
            model: "minimax-m2.7",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_SUCCESS_BODY,
        },
    ];

    for case in cases {
        let replies = HashMap::from([(
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: case.upstream_body,
            }]),
        )]);
        let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
        let (state, dir) = build_state(base_url, &["key-1"]);
        let (port, gateway_handle) = start_gateway(state.clone()).await;

        let (status, response) = protocol_call(port, case.client_path, case.model).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{} {}",
            case.client_path,
            case.model
        );

        let call = calls.lock().unwrap()[0].clone();
        assert_eq!(call.path, case.upstream_path);
        if case.upstream_path == "/v1/messages" {
            assert_eq!(call.x_api_key.as_deref(), Some("key-1"));
            assert!(call.authorization.is_none());
            assert_eq!(call.anthropic_version.as_deref(), Some("2023-06-01"));
        } else {
            assert_eq!(call.authorization.as_deref(), Some("Bearer key-1"));
            assert!(call.x_api_key.is_none());
            assert!(call.anthropic_version.is_none());
        }
        let upstream_request: serde_json::Value = serde_json::from_str(&call.body).unwrap();
        assert_eq!(upstream_request["model"], case.model);
        assert!(upstream_request["messages"].is_array());

        match case.client_path {
            "/v1/chat/completions" => {
                assert_eq!(response["object"], "chat.completion");
                assert_eq!(response["choices"][0]["message"]["content"], "ok");
            }
            "/v1/responses" => {
                assert_eq!(response["object"], "response");
                assert_eq!(response["output"][0]["content"][0]["text"], "ok");
            }
            "/v1/messages" => {
                assert_eq!(response["type"], "message");
                assert_eq!(response["content"][0]["text"], "ok");
            }
            _ => unreachable!(),
        }
        let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
        assert_eq!((log.prompt_tokens, log.completion_tokens), (10, 2));
        assert_eq!(log.status, "success");

        gateway::stop_gateway(gateway_handle);
        let _ = stop_mock.send(());
        let _ = fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn converts_streams_across_chat_messages_and_responses() {
    struct Case {
        client_path: &'static str,
        model: &'static str,
        upstream_path: &'static str,
        upstream_body: &'static str,
        expected_events: &'static [&'static str],
    }

    let cases = [
        Case {
            client_path: "/v1/messages",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/chat/completions",
            upstream_body: CHAT_STREAM_BODY,
            expected_events: &["event: message_start", "text_delta", "event: message_stop"],
        },
        Case {
            client_path: "/v1/responses",
            model: "deepseek-v4-flash",
            upstream_path: "/v1/chat/completions",
            upstream_body: CHAT_STREAM_BODY,
            expected_events: &[
                "event: response.created",
                "response.output_text.delta",
                "event: response.completed",
            ],
        },
        Case {
            client_path: "/v1/chat/completions",
            model: "minimax-m2.7",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_STREAM_BODY,
            expected_events: &[
                "chat.completion.chunk",
                "\"content\":\"ok\"",
                "data: [DONE]",
            ],
        },
        Case {
            client_path: "/v1/responses",
            model: "minimax-m2.7",
            upstream_path: "/v1/messages",
            upstream_body: MESSAGES_STREAM_BODY,
            expected_events: &[
                "event: response.created",
                "response.output_text.delta",
                "event: response.completed",
            ],
        },
    ];

    for case in cases {
        let replies = HashMap::from([(
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: case.upstream_body,
            }]),
        )]);
        let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
        let (state, dir) = build_state(base_url, &["key-1"]);
        let (port, gateway_handle) = start_gateway(state.clone()).await;

        let (status, body) = protocol_stream_call(port, case.client_path, case.model).await;
        assert_eq!(status, StatusCode::OK);
        for expected in case.expected_events {
            assert!(
                body.contains(expected),
                "{} {} missing {expected}: {body}",
                case.client_path,
                case.model
            );
        }
        assert_eq!(calls.lock().unwrap()[0].path, case.upstream_path);
        let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
        assert_eq!((log.prompt_tokens, log.completion_tokens), (10, 2));
        assert_eq!(log.status, "success");

        gateway::stop_gateway(gateway_handle);
        let _ = stop_mock.send(());
        let _ = fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn stream_can_outlive_non_stream_timeout() {
    let (base_url, calls, stop_mock) = start_delayed_messages_upstream(
        "text/event-stream",
        vec![
            (StdDuration::ZERO, MESSAGES_STREAM_HEAD),
            (StdDuration::from_millis(1_200), MESSAGES_STREAM_TAIL),
        ],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let mut config = state.config();
    config.non_stream_timeout_secs = 1;
    config.stream_idle_timeout_secs = 2;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(4),
        protocol_stream_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("stream should finish before the test watchdog");
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("event: message_stop"), "{body}");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "success");
    assert_eq!((log.prompt_tokens, log.completion_tokens), (10, 2));

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn stream_idle_timeout_emits_protocol_error_and_updates_log() {
    let (base_url, calls, stop_mock) = start_delayed_messages_upstream(
        "text/event-stream",
        vec![
            (StdDuration::ZERO, MESSAGES_STREAM_HEAD),
            (StdDuration::from_millis(1_200), MESSAGES_STREAM_TAIL),
        ],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let mut config = state.config();
    config.stream_idle_timeout_secs = 1;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(4),
        protocol_stream_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("idle timeout should finish before the test watchdog");
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("event: error"), "{body}");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "error");
    assert!(log.error_message.is_some());

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn non_stream_body_timeout_is_retryable() {
    let (base_url, calls, stop_mock) = start_delayed_messages_upstream(
        "application/json",
        vec![(StdDuration::from_millis(1_200), MESSAGES_SUCCESS_BODY)],
    )
    .await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let mut config = state.config();
    config.non_stream_timeout_secs = 1;
    state.set_config(config).unwrap();
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = tokio::time::timeout(
        StdDuration::from_secs(5),
        protocol_call(port, "/v1/messages", "minimax-m2.7"),
    )
    .await
    .expect("non-stream timeout should finish before the test watchdog");
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let message = body["error"]["message"].as_str().unwrap_or_default();
    let message = message.to_ascii_lowercase();
    assert!(
        message.contains("timeout") || message.contains("timed out"),
        "{body}"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    let log = state.db.lock().list_forward_logs(1).unwrap().remove(0);
    assert_eq!(log.status, "error");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn messages_forwards_account_key_as_x_api_key() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 200,
            body: MESSAGES_SUCCESS_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state).await;

    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/v1/messages", port))
        .header("x-api-key", "gw-test")
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "minimax-m2.7",
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": false
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].x_api_key.as_deref(), Some("key-1"));
    assert!(calls[0].authorization.is_none());

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn converted_messages_request_keeps_retry_and_account_fallback() {
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
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, body) = protocol_call(port, "/v1/messages", "deepseek-v4-flash").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["type"], "message");
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-1", "key-1", "key-2"]
    );
    assert!(calls.iter().all(|call| call.path == "/v1/chat/completions"));
    drop(calls);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn manual_order_drives_fallback_while_ineligible_accounts_are_skipped() {
    let replies = HashMap::from([
        (
            "key-2".to_string(),
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
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 200,
                body: SUCCESS_BODY,
            }]),
        ),
    ]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1", "key-2", "key-3", "key-4"]);
    {
        let db = state.db.lock();
        db.reorder_accounts(&[
            "acct-4".into(),
            "acct-3".into(),
            "acct-2".into(),
            "acct-1".into(),
        ])
        .unwrap();
        db.update_account(
            "acct-4",
            &AccountUpdate {
                name: None,
                username: None,
                password: None,
                key: None,
                enabled: Some(false),
                referral_code: None,
                purchase_date: None,
            },
            None,
            None,
        )
        .unwrap();
        db.set_account_cooldown(
            "acct-3",
            Some(Utc::now() + Duration::hours(1)),
            Some("test cooldown"),
        )
        .unwrap();
    }
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, _) = chat(port).await;
    assert_eq!(status, 200);
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.key.as_str())
            .collect::<Vec<_>>(),
        ["key-2", "key-2", "key-1"]
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn converted_request_error_uses_callers_envelope_without_fallback() {
    let replies = HashMap::from([
        (
            "key-1".to_string(),
            VecDeque::from([MockReply {
                status: 400,
                body: CHAT_BAD_REQUEST_BODY,
            }]),
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
    let (port, gateway_handle) = start_gateway(state).await;

    let (status, body) = protocol_call(port, "/v1/messages", "deepseek-v4-flash").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["type"], "error");
    assert_eq!(body["error"]["type"], "invalid_request_error");
    assert_eq!(body["error"]["message"], "bad request");
    assert_eq!(calls.lock().unwrap().len(), 1);

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
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
async fn dashboard_ping_marks_quota_cooldown() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([MockReply {
            status: 429,
            body: LIMITED_BODY,
        }]),
    )]);
    let (base_url, calls, stop_mock) = start_mock_upstream(replies).await;
    let (state, dir) = build_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let response = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{}/dashboard/api/accounts/acct-1/test",
            port
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("额度"));

    let stored = state.db.lock().get_account("acct-1").unwrap().unwrap();
    let remaining = stored.cooldown_until.unwrap() - Utc::now();
    assert!(remaining > Duration::days(2) && remaining <= Duration::days(3));
    assert!(stored.last_error.unwrap().contains("Weekly usage limit"));

    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].key, "key-1");
    let payload: serde_json::Value = serde_json::from_str(&calls[0].body).unwrap();
    assert_eq!(payload["model"], "deepseek-v4-flash");
    assert_eq!(payload["messages"][0]["content"], "ping");

    gateway::stop_gateway(gateway_handle);
    let _ = stop_mock.send(());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn dashboard_port_change_is_saved_for_next_restart() {
    let (state, dir) = build_state("http://127.0.0.1:1".into(), &[]);
    let current_port = free_port();
    let handle = gateway::start_gateway(state.clone(), current_port)
        .await
        .unwrap();
    *state.gateway.lock() = Some(handle);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requested_port = free_port();
    let mut config = state.config();
    config.gateway_port = requested_port;
    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "http://127.0.0.1:{}/dashboard/api/settings",
            current_port
        ))
        .json(&config)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let status: serde_json::Value = response.json().await.unwrap();
    assert_eq!(status["port"].as_u64(), Some(current_port as u64));
    assert_eq!(state.config().gateway_port, requested_port);
    assert_eq!(state.active_gateway_port(), current_port);

    let status_response = client
        .get(format!(
            "http://127.0.0.1:{}/dashboard/api/gateway/status",
            current_port
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);

    gateway::stop_gateway(state.gateway.lock().take().unwrap());
    let _ = fs::remove_dir_all(dir);
}
