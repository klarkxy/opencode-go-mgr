use axum::Router;
use axum::routing::post;
use chrono::Utc;
use futures_util::{StreamExt, stream};
use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::models::Account;
use ocg_core::state::CoreStateInner;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SUCCESS_BODY: &str = r#"{"id":"bench","object":"chat.completion","model":"deepseek-v4-flash","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}"#;

async fn start_mock_upstream() -> (String, tokio::sync::oneshot::Sender<()>) {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async { ([("content-type", "application/json")], SUCCESS_BODY) }),
    );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });
    (format!("http://{address}"), shutdown_tx)
}

fn benchmark_state(base_url: String) -> (Arc<CoreStateInner>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ocg-success-bench-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("bench"));
    let db = Database::open(dir.clone()).unwrap();
    let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
    let mut config = state.config();
    config.gateway_key = "bench-gateway-key".to_string();
    config.upstream_base_url = base_url;
    state.set_config(config).unwrap();
    let now = Utc::now();
    state
        .db
        .lock()
        .create_account(&Account {
            id: "bench-account".to_string(),
            name: "Benchmark".to_string(),
            username: None,
            password_cipher: None,
            key_cipher: state.encrypt_key("bench-upstream-key").unwrap(),
            enabled: true,
            referral_code: None,
            purchase_date: String::new(),
            expires_on: String::new(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            last_error: None,
            auth_error: None,
            created_at: now,
            updated_at: now,
        })
        .unwrap();
    (state, dir)
}

async fn run_requests(client: &reqwest::Client, url: &str, count: usize) -> Vec<Duration> {
    stream::iter(0..count)
        .map(|_| async {
            let started = Instant::now();
            let response = client
                .post(url)
                .bearer_auth("bench-gateway-key")
                .json(&serde_json::json!({
                    "model": "deepseek-v4-flash",
                    "messages": [{"role": "user", "content": "benchmark payload"}],
                    "max_tokens": 3,
                    "stream": false
                }))
                .send()
                .await
                .unwrap();
            assert!(response.status().is_success());
            let _ = response.bytes().await.unwrap();
            started.elapsed()
        })
        .buffer_unordered(16)
        .collect()
        .await
}

/// Run manually with:
/// `cargo test -p ocg-core --test success_benchmark --release -- --ignored --nocapture`
#[tokio::test]
#[ignore = "manual release-mode performance comparison"]
async fn successful_gateway_requests_have_a_repeatable_release_benchmark() {
    let (upstream_url, stop_upstream) = start_mock_upstream().await;
    let (state, dir) = benchmark_state(upstream_url);
    let handle = gateway::start_gateway(state.clone(), 0).await.unwrap();
    let url = format!("http://127.0.0.1:{}/v1/chat/completions", handle.port);
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(32)
        .build()
        .unwrap();
    let _ = run_requests(&client, &url, 100).await;
    let label = std::env::var("OCG_BENCH_LABEL").unwrap_or_else(|_| "current".to_string());

    for round in 1..=3 {
        let started = Instant::now();
        let mut durations = run_requests(&client, &url, 600).await;
        let elapsed = started.elapsed();
        durations.sort_unstable();
        let p95 = durations[(durations.len() * 95 / 100).min(durations.len() - 1)];
        println!(
            "OCG_BENCH_RESULT {}",
            serde_json::json!({
                "label": label,
                "round": round,
                "requests": durations.len(),
                "throughput_rps": durations.len() as f64 / elapsed.as_secs_f64(),
                "p95_ms": p95.as_secs_f64() * 1000.0,
            })
        );
    }

    let logs = state.db.lock().list_forward_logs(2_000).unwrap();
    assert_eq!(logs.len(), 1_900);
    let _ = handle.shutdown.send(());
    handle.task.await.unwrap();
    let _ = stop_upstream.send(());
    drop(client);
    drop(state);
    fs::remove_dir_all(dir).unwrap();
}
