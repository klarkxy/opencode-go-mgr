//! Integration tests for the `ocg-core` refactor.
//!
//! Covers the surfaces that the GUI/CLI split could have broken:
//! - `CoreState` persistence (cipher round-trip across reopen)
//! - `CoreState` config round-trip (gateway_key auto-generation, persistence)
//! - Cross-cipher incompatibility: an account encrypted with one cipher
//!   cannot be decrypted by another — the safety property the README warns about.

use chrono::{Duration, Utc};
use ocg_core::crypto::{KeyCipher, MachineBoundCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::models::{Account, ForwardLog, normalize_client_root_url};
use ocg_core::state::CoreStateInner;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn temp_data_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("ocg-core-test-{}-{}", label, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn core_state_persists_account_through_static_cipher() {
    let dir = temp_data_dir("persist");
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("secret-A"));
    let db = Database::open(dir.clone()).unwrap();
    let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

    let account = Account {
        id: "acct-1".into(),
        name: "main".into(),
        username: None,
        password_cipher: None,
        key_cipher: state.encrypt_key("sk-ocg-plaintext-123").unwrap(),
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
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    state.db.lock().create_account(&account).unwrap();

    // Reopen with the same cipher — must decrypt cleanly.
    let db2 = Database::open(dir.clone()).unwrap();
    let cipher2: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("secret-A"));
    let state2 = Arc::new(CoreStateInner::new(db2, dir, cipher2).unwrap());

    let stored = state2.db.lock().get_account("acct-1").unwrap().unwrap();
    let decrypted = state2.decrypt_key(&stored.key_cipher).unwrap();
    assert_eq!(decrypted, "sk-ocg-plaintext-123");
}

#[test]
fn core_state_with_wrong_cipher_cannot_decrypt_existing_account() {
    let dir = temp_data_dir("mismatch");

    // Write with cipher A.
    {
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("A"));
        let db = Database::open(dir.clone()).unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        let account = Account {
            id: "acct-2".into(),
            name: "alt".into(),
            username: None,
            password_cipher: None,
            key_cipher: state.encrypt_key("sk-ocg-plaintext-456").unwrap(),
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        state.db.lock().create_account(&account).unwrap();
    }

    // Read with cipher B — must fail or return garbage, NEVER the plaintext.
    let db = Database::open(dir.clone()).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("B"));
    let state = Arc::new(CoreStateInner::new(db, dir, cipher).unwrap());
    let stored = state.db.lock().get_account("acct-2").unwrap().unwrap();
    let result = state.decrypt_key(&stored.key_cipher);
    match result {
        Err(_) => {} // invalid utf-8 — fine
        Ok(s) => assert_ne!(s, "sk-ocg-plaintext-456"),
    }
}

#[test]
fn core_state_generates_gateway_key_on_first_run_and_persists() {
    let dir = temp_data_dir("gwkey");
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("k"));

    // First open — gateway_key should be auto-generated and look like ocg-word-word.
    let db = Database::open(dir.clone()).unwrap();
    let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
    let key1 = state.config().gateway_key;
    assert!(
        key1.starts_with("ocg-"),
        "expected auto-generated gateway key, got {:?}",
        key1
    );

    // Reopen — same key, persisted in settings table.
    let db2 = Database::open(dir).unwrap();
    let cipher2: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("k"));
    let state2 = Arc::new(CoreStateInner::new(db2, PathBuf::from("."), cipher2).unwrap());
    assert_eq!(state2.config().gateway_key, key1);
}

#[test]
fn core_state_set_config_persists() {
    let dir = temp_data_dir("cfg");
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("k"));
    let db = Database::open(dir.clone()).unwrap();
    let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

    let mut cfg = state.config();
    cfg.gateway_port = 9999;
    cfg.client_root_url = "https://gateway.example.com/ocg".into();
    cfg.connect_timeout_secs = 12;
    cfg.non_stream_timeout_secs = 345;
    cfg.stream_idle_timeout_secs = 678;
    state.set_config(cfg).unwrap();

    // Reopen and verify.
    let db2 = Database::open(dir).unwrap();
    let cipher2: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("k"));
    let state2 = Arc::new(CoreStateInner::new(db2, PathBuf::from("."), cipher2).unwrap());
    let persisted = state2.config();
    assert_eq!(persisted.gateway_port, 9999);
    assert_eq!(persisted.client_root_url, "https://gateway.example.com/ocg");
    assert_eq!(persisted.connect_timeout_secs, 12);
    assert_eq!(persisted.non_stream_timeout_secs, 345);
    assert_eq!(persisted.stream_idle_timeout_secs, 678);
}

#[test]
fn core_state_scrubs_removed_config_fields() {
    let dir = temp_data_dir("removed-config");
    let db = Database::open(dir.clone()).unwrap();
    db.set_setting(
        "config",
        r#"{"gateway_port":9042,"gateway_key":"gw","upstream_base_url":"https://example.com","auto_start":false,"remote":{"url":"https://old.example.com","token":"remote-secret"}}"#,
    )
    .unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("k"));
    let state = Arc::new(CoreStateInner::new(db, dir, cipher).unwrap());

    let config = state.config();
    assert_eq!(config.client_root_url, "");
    assert_eq!(config.connect_timeout_secs, 30);
    assert_eq!(config.non_stream_timeout_secs, 120);
    assert_eq!(config.stream_idle_timeout_secs, 300);

    let persisted = state.db.lock().get_setting("config").unwrap().unwrap();
    assert!(!persisted.contains("remote"));
    assert!(!persisted.contains("remote-secret"));
    let persisted: serde_json::Value = serde_json::from_str(&persisted).unwrap();
    assert_eq!(persisted["client_root_url"], "");
    assert_eq!(persisted["connect_timeout_secs"], 30);
    assert_eq!(persisted["non_stream_timeout_secs"], 120);
    assert_eq!(persisted["stream_idle_timeout_secs"], 300);
}

#[test]
fn client_root_url_normalizes_and_rejects_endpoints() {
    for (input, expected) in [
        ("", ""),
        ("  https://ocg.example.com///  ", "https://ocg.example.com"),
        (
            "http://192.168.1.20:9042/proxy/v1/",
            "http://192.168.1.20:9042/proxy",
        ),
        (
            "https://ocg.example.com/proxy/V1",
            "https://ocg.example.com/proxy",
        ),
        (
            "https://ocg.example.com/reverse/proxy",
            "https://ocg.example.com/reverse/proxy",
        ),
    ] {
        assert_eq!(normalize_client_root_url(input).unwrap(), expected);
    }

    for input in [
        "ocg.example.com",
        "http:example.com/",
        "http:/example.com/",
        "ftp://ocg.example.com",
        "https://user:secret@ocg.example.com",
        "https://ocg.example.com?node=one",
        "https://ocg.example.com#settings",
        "https://ocg.example.com/v1/chat/completions",
        "https://ocg.example.com/proxy/v1/responses",
    ] {
        assert!(
            normalize_client_root_url(input).is_err(),
            "expected {input:?} to be rejected"
        );
    }
}

#[test]
fn machine_bound_cipher_roundtrip_through_core_state() {
    // Sanity check that the GUI's default cipher flows through CoreState correctly.
    let dir = temp_data_dir("machine");
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(MachineBoundCipher::new());
    let db = Database::open(dir).unwrap();
    let state = Arc::new(CoreStateInner::new(db, PathBuf::from("."), cipher).unwrap());
    let enc = state.encrypt_key("sk-ocg-machine-bound").unwrap();
    let dec = state.decrypt_key(&enc).unwrap();
    assert_eq!(dec, "sk-ocg-machine-bound");
}

#[test]
fn list_forward_logs_binds_limit_parameter() {
    let dir = temp_data_dir("logs");
    let db = Database::open(dir).unwrap();
    db.log_forward(&ForwardLog {
        id: 0,
        timestamp: Utc::now(),
        model: "glm-5.2".into(),
        account_id: "acct".into(),
        account_name: "main".into(),
        status: "success".into(),
        http_status: Some(200),
        prompt_tokens: 1,
        completion_tokens: 2,
        cached_tokens: 0,
        cost: 0.01,
        error_message: None,
    })
    .unwrap();

    let rows = db.list_forward_logs(1).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].model, "glm-5.2");
}

#[test]
fn query_forward_logs_filters_before_limit_and_summarizes_all_matches() {
    let dir = temp_data_dir("filtered-logs");
    let db = Database::open(dir).unwrap();

    for (status, prompt, completion, cached, cost) in [
        ("success", 10, 20, 3, 1.0),
        ("success", 30, 40, 5, 2.0),
        ("error", 90, 90, 90, 9.0),
    ] {
        db.log_forward(&ForwardLog {
            id: 0,
            timestamp: Utc::now(),
            model: "glm-5.2".into(),
            account_id: "selected".into(),
            account_name: "selected".into(),
            status: status.into(),
            http_status: Some(200),
            prompt_tokens: prompt,
            completion_tokens: completion,
            cached_tokens: cached,
            cost,
            error_message: None,
        })
        .unwrap();
    }

    // Push every matching row beyond the old global top-200 window.
    for index in 0..200 {
        db.log_forward(&ForwardLog {
            id: 0,
            timestamp: Utc::now(),
            model: "other".into(),
            account_id: "busy".into(),
            account_name: format!("busy-{index}"),
            status: "success".into(),
            http_status: Some(200),
            prompt_tokens: 1_000,
            completion_tokens: 1_000,
            cached_tokens: 1_000,
            cost: 100.0,
            error_message: None,
        })
        .unwrap();
    }

    let first = db
        .query_forward_logs(
            1,
            0,
            Some("success"),
            Some("selected"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].prompt_tokens, 30);
    assert_eq!(first.summary.total_requests, 2);
    assert_eq!(first.summary.prompt_tokens, 40);
    assert_eq!(first.summary.completion_tokens, 60);
    assert_eq!(first.summary.cached_tokens, 8);
    assert!((first.summary.cost - 3.0).abs() < f64::EPSILON);

    let second = db
        .query_forward_logs(
            1,
            1,
            Some("success"),
            Some("selected"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].prompt_tokens, 10);
    assert_eq!(second.summary.total_requests, 2);

    let bounded = db
        .query_forward_logs(999, -1, None, None, None, None, None, None, None)
        .unwrap();
    assert_eq!(bounded.items.len(), 200);
    assert_eq!(bounded.summary.total_requests, 203);
}

#[test]
fn daily_cost_by_model_groups_success_rows_only() {
    let dir = temp_data_dir("daily");
    let db = Database::open(dir).unwrap();
    let today = Utc::now();
    for (model, status, cost, offset) in [
        ("glm-5.2", "success", 1.0, 0),
        ("glm-5.2", "success", 2.0, 0),
        ("kimi-k2.7-code", "success", 3.0, 1),
        ("glm-5.2", "error", 9.0, 0),
    ] {
        db.log_forward(&ForwardLog {
            id: 0,
            timestamp: today - Duration::days(offset),
            model: model.into(),
            account_id: "acct".into(),
            account_name: "main".into(),
            status: status.into(),
            http_status: Some(200),
            prompt_tokens: 0,
            completion_tokens: 0,
            cached_tokens: 0,
            cost,
            error_message: None,
        })
        .unwrap();
    }

    let rows = db.daily_cost_by_model(3).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|row| row.model == "glm-5.2" && (row.cost - 3.0).abs() < f64::EPSILON)
    );
    assert!(
        rows.iter()
            .any(|row| row.model == "kimi-k2.7-code" && (row.cost - 3.0).abs() < f64::EPSILON)
    );
}
