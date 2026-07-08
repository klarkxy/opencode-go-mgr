//! Integration tests for the `ocg-core` refactor.
//!
//! Covers the surfaces that the GUI/CLI split could have broken:
//! - `CoreState` persistence (cipher round-trip across reopen)
//! - `CoreState` config round-trip (gateway_key auto-generation, persistence)
//! - Cross-cipher incompatibility: an account encrypted with one cipher
//!   cannot be decrypted by another — the safety property the README warns about.

use chrono::Utc;
use ocg_core::crypto::{KeyCipher, MachineBoundCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::models::Account;
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
        key_cipher: state.encrypt_key("sk-ocg-plaintext-123").unwrap(),
        enabled: true,
        referral_code: None,
        recharge_date: None,
        cooldown_until: None,
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
            key_cipher: state.encrypt_key("sk-ocg-plaintext-456").unwrap(),
            enabled: true,
            referral_code: None,
            recharge_date: None,
            cooldown_until: None,
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
    assert!(key1.starts_with("ocg-"), "expected auto-generated gateway key, got {:?}", key1);

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
    state.set_config(cfg).unwrap();

    // Reopen and verify.
    let db2 = Database::open(dir).unwrap();
    let cipher2: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("k"));
    let state2 = Arc::new(CoreStateInner::new(db2, PathBuf::from("."), cipher2).unwrap());
    assert_eq!(state2.config().gateway_port, 9999);
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
