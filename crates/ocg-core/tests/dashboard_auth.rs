use chrono::Utc;
use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::models::ForwardLog;
use ocg_core::state::CoreStateInner;
use reqwest::StatusCode;
use serde_json::json;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

fn state(label: &str) -> Arc<CoreStateInner> {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("ocg-auth-test-{label}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    let db = Database::open(dir.clone()).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
    Arc::new(CoreStateInner::new(db, PathBuf::from(dir), cipher).unwrap())
}

#[tokio::test]
async fn public_dashboard_uses_first_registration_and_session_cookie() {
    let state = state("public");
    let handle = gateway::start_gateway_on(state, SocketAddr::from(([0, 0, 0, 0], 0)))
        .await
        .unwrap();
    let base = format!("http://127.0.0.1:{}/dashboard/api", handle.port);
    let client = reqwest::Client::new();

    let status = client
        .get(format!("{base}/auth/status"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(status["local"], false);
    assert_eq!(status["initialized"], false);
    assert_eq!(status["authenticated"], false);

    let response = client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "username": "admin", "password": "password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookie = response
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    assert_eq!(
        client
            .post(format!("{base}/auth/login"))
            .json(&json!({ "username": "admin", "password": "wrong-password" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(format!("{base}/auth/register"))
            .json(&json!({ "username": "other", "password": "password456" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_dashboard_skips_login() {
    let state = state("local");
    let handle = gateway::start_gateway_on(state, SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let base = format!("http://127.0.0.1:{}/dashboard/api", handle.port);
    let client = reqwest::Client::new();

    let status = client
        .get(format!("{base}/auth/status"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(status["local"], true);
    assert_eq!(status["authenticated"], true);
    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let forwarded_status = client
        .get(format!("{base}/auth/status"))
        .header("x-forwarded-for", "203.0.113.10")
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(forwarded_status["local"], false);
    assert_eq!(forwarded_status["authenticated"], false);
    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .header("x-forwarded-for", "203.0.113.10")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_forward_logs_apply_filters_before_pagination() {
    let state = state("forward-logs");
    for (account_id, prompt_tokens) in [("selected", 10), ("other", 100)] {
        state
            .db
            .lock()
            .log_forward(&ForwardLog {
                id: 0,
                timestamp: Utc::now(),
                model: "glm-5.2".into(),
                account_id: account_id.into(),
                account_name: account_id.into(),
                status: "success".into(),
                http_status: Some(200),
                prompt_tokens,
                completion_tokens: prompt_tokens * 2,
                cached_tokens: 0,
                cost: prompt_tokens as f64 / 100.0,
                error_message: None,
            })
            .unwrap();
    }

    let handle = gateway::start_gateway_on(state, SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let response = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{}/dashboard/api/logs/forward?limit=1&offset=0&status=success&account_id=selected",
            handle.port
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["account_id"], "selected");
    assert_eq!(body["summary"]["total_requests"], 1);
    assert_eq!(body["summary"]["prompt_tokens"], 10);
    assert_eq!(body["summary"]["completion_tokens"], 20);
    assert_eq!(body["summary"]["cost"], 0.1);

    gateway::stop_gateway(handle);
}
