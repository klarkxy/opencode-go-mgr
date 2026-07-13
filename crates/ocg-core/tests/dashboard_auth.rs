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
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

static AUTO_START_SYNCED: AtomicBool = AtomicBool::new(false);
static AUTO_START_FAIL: AtomicBool = AtomicBool::new(false);

fn test_auto_start_sync(enabled: bool) -> anyhow::Result<()> {
    if enabled && AUTO_START_FAIL.load(Ordering::Relaxed) {
        anyhow::bail!("test auto-start failure");
    }
    AUTO_START_SYNCED.store(enabled, Ordering::Relaxed);
    Ok(())
}

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
    Arc::new(CoreStateInner::new(db, dir, cipher).unwrap())
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
            .get(format!("{base}/settings/check-update"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{base}/application-models"))
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
            .get(format!("{base}/application-models"))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_GATEWAY
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
async fn loopback_settings_trim_and_require_gateway_key() {
    let state = state("settings-key");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let url = format!("http://127.0.0.1:{}/dashboard/api/settings", handle.port);
    let client = reqwest::Client::new();

    let mut config = state.config();
    config.gateway_key = "  trimmed-key  ".into();
    config.client_root_url = "  http://192.168.1.20:9042/proxy/v1/  ".into();
    config.connect_timeout_secs = 12;
    config.non_stream_timeout_secs = 345;
    config.stream_idle_timeout_secs = 678;
    assert_eq!(
        client
            .post(&url)
            .json(&config)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let saved = state.config();
    assert_eq!(saved.gateway_key, "trimmed-key");
    assert_eq!(saved.client_root_url, "http://192.168.1.20:9042/proxy");
    assert_eq!(saved.connect_timeout_secs, 12);
    assert_eq!(saved.non_stream_timeout_secs, 345);
    assert_eq!(saved.stream_idle_timeout_secs, 678);
    let roundtrip = client
        .get(&url)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(roundtrip["connect_timeout_secs"], 12);
    assert_eq!(roundtrip["non_stream_timeout_secs"], 345);
    assert_eq!(roundtrip["stream_idle_timeout_secs"], 678);
    assert_eq!(
        roundtrip["client_root_url"],
        "http://192.168.1.20:9042/proxy"
    );
    assert_eq!(roundtrip["auto_start_supported"], false);
    assert_eq!(roundtrip["client_root_url_from_env"], false);

    config.gateway_key = "   ".into();
    assert_eq!(
        client
            .post(&url)
            .json(&config)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(state.config().gateway_key, "trimmed-key");

    for client_root_url in [
        "ocg.example.com",
        "ftp://ocg.example.com",
        "https://user:secret@ocg.example.com",
        "https://ocg.example.com?node=one",
        "https://ocg.example.com#settings",
        "https://ocg.example.com/v1/chat/completions",
    ] {
        let mut invalid = state.config();
        invalid.client_root_url = client_root_url.into();
        assert_eq!(
            client
                .post(&url)
                .json(&invalid)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST,
            "{client_root_url}"
        );
        assert_eq!(
            state.config().client_root_url,
            "http://192.168.1.20:9042/proxy"
        );
    }

    for (field, value) in [
        ("connect_timeout_secs", 0),
        ("connect_timeout_secs", 301),
        ("non_stream_timeout_secs", 0),
        ("non_stream_timeout_secs", 3_601),
        ("stream_idle_timeout_secs", 0),
        ("stream_idle_timeout_secs", 3_601),
    ] {
        let mut invalid = state.config();
        match field {
            "connect_timeout_secs" => invalid.connect_timeout_secs = value,
            "non_stream_timeout_secs" => invalid.non_stream_timeout_secs = value,
            "stream_idle_timeout_secs" => invalid.stream_idle_timeout_secs = value,
            _ => unreachable!(),
        }
        assert_eq!(
            client
                .post(&url)
                .json(&invalid)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST,
            "{field}={value}"
        );
        let unchanged = state.config();
        assert_eq!(unchanged.connect_timeout_secs, 12);
        assert_eq!(unchanged.non_stream_timeout_secs, 345);
        assert_eq!(unchanged.stream_idle_timeout_secs, 678);
    }

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_settings_gate_and_sync_auto_start() {
    AUTO_START_SYNCED.store(false, Ordering::Relaxed);
    AUTO_START_FAIL.store(false, Ordering::Relaxed);
    let unsupported_state = state("settings-auto-start-unsupported");
    let unsupported_handle = gateway::start_gateway_on(
        unsupported_state.clone(),
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )
    .await
    .unwrap();
    let unsupported_url = format!(
        "http://127.0.0.1:{}/dashboard/api/settings",
        unsupported_handle.port
    );
    let client = reqwest::Client::new();
    let mut unsupported_config = unsupported_state.config();
    unsupported_config.auto_start = true;
    assert_eq!(
        client
            .post(&unsupported_url)
            .json(&unsupported_config)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(!unsupported_state.config().auto_start);

    let mut preserved_config = unsupported_state.config();
    preserved_config.auto_start = true;
    unsupported_state
        .set_config(preserved_config.clone())
        .unwrap();
    let roundtrip = client
        .get(&unsupported_url)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(roundtrip["auto_start_supported"], false);
    assert_eq!(roundtrip["auto_start"], true);
    preserved_config.connect_timeout_secs = 31;
    assert_eq!(
        client
            .post(&unsupported_url)
            .json(&preserved_config)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(unsupported_state.config().auto_start);
    preserved_config.auto_start = false;
    assert_eq!(
        client
            .post(&unsupported_url)
            .json(&preserved_config)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(unsupported_state.config().auto_start);
    gateway::stop_gateway(unsupported_handle);

    let supported_state = state("settings-auto-start-supported");
    supported_state.set_auto_start_sync(test_auto_start_sync);
    let supported_handle = gateway::start_gateway_on(
        supported_state.clone(),
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )
    .await
    .unwrap();
    let supported_url = format!(
        "http://127.0.0.1:{}/dashboard/api/settings",
        supported_handle.port
    );
    let mut supported_config = supported_state.config();
    supported_config.auto_start = true;
    AUTO_START_FAIL.store(true, Ordering::Relaxed);
    assert_eq!(
        client
            .post(&supported_url)
            .json(&supported_config)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(!supported_state.config().auto_start);
    let persisted = supported_state
        .db
        .lock()
        .get_setting("config")
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&persisted).unwrap()["auto_start"],
        false
    );

    AUTO_START_FAIL.store(false, Ordering::Relaxed);
    assert_eq!(
        client
            .post(&supported_url)
            .json(&supported_config)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(supported_state.config().auto_start);
    assert!(AUTO_START_SYNCED.load(Ordering::Relaxed));
    let roundtrip = client
        .get(&supported_url)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(roundtrip["auto_start_supported"], true);
    assert_eq!(roundtrip["auto_start"], true);

    gateway::stop_gateway(supported_handle);
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
