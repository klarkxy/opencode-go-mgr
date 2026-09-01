use super::*;
use crate::crypto::{KeyCipher, StaticKeyCipher};
use crate::models::{Account, ForwardLog};
use chrono::{Duration, Utc};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn temp_data_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("ocg-selector-test-{}-{}", label, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn account(id: &str, enabled: bool, cooldown: Option<chrono::DateTime<Utc>>) -> Account {
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
    Account {
        id: id.into(),
        provider_id: crate::provider::default_provider_id(),

        credential_kind: crate::provider::default_credential_kind(),
        quota_scope: crate::provider::default_quota_scope(),
        name: id.into(),
        username: None,
        password_cipher: None,
        key_cipher: cipher.encrypt(id).unwrap(),
        enabled,
        account_type: crate::models::AccountType::Key,
        setup_step: crate::models::AccountSetupStep::Ready,
        referral_code: None,
        purchase_date: String::new(),
        expires_on: String::new(),
        cooldown_until: cooldown,
        cooldown_generic_until: cooldown,
        cooldown_5h_until: None,
        cooldown_week_until: None,
        cooldown_month_until: None,
        cooldown_free_until: None,
        last_error: None,
        auth_error: None,
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn skips_disabled_cooldown_and_excluded_accounts_in_order() {
    let dir = temp_data_dir("skip");
    let db = Database::open(dir.clone()).unwrap();
    db.create_account(&account("disabled", false, None))
        .unwrap();
    db.create_account(&account(
        "cooldown",
        true,
        Some(Utc::now() + Duration::hours(1)),
    ))
    .unwrap();
    let mut auth_failed = account("auth-failed", true, None);
    auth_failed.auth_error = Some("upstream auth error 401".into());
    db.create_account(&auth_failed).unwrap();
    db.create_account(&account("failed", true, None)).unwrap();
    db.create_account(&account("next", true, None)).unwrap();

    let selected = AccountSelector::new()
        .select_excluding(&db, &["failed"])
        .unwrap()
        .unwrap();
    assert_eq!(selected.id, "next");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn selects_accounts_in_the_saved_manual_order() {
    let dir = temp_data_dir("manual-order");
    let db = Database::open(dir.clone()).unwrap();
    db.create_account(&account("first", true, None)).unwrap();
    db.create_account(&account("second", true, None)).unwrap();
    db.reorder_accounts(&[
        "second".into(),
        "first".into(),
        crate::provider::ZEN_FREE_ACCOUNT_ID.into(),
    ])
    .unwrap();

    let selected = AccountSelector::new().select(&db, None).unwrap().unwrap();
    assert_eq!(selected.id, "second");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn account_with_any_future_window_cooling_is_skipped() {
    let dir = temp_data_dir("per-window-cooling");
    let db = Database::open(dir.clone()).unwrap();
    let mut expired_5h = account("expired-5h", true, None);
    expired_5h.cooldown_5h_until = Some(Utc::now() - Duration::hours(1));
    expired_5h.cooldown_week_until = Some(Utc::now() + Duration::hours(1));
    db.create_account(&expired_5h).unwrap();
    db.create_account(&account("next", true, None)).unwrap();

    let selected = AccountSelector::new().select(&db, None).unwrap().unwrap();
    assert_eq!(selected.id, "next");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pending_or_empty_key_accounts_are_never_selected() {
    let dir = temp_data_dir("managed-pending");
    let db = Database::open(dir.clone()).unwrap();
    let mut pending = account("pending", true, None);
    pending.account_type = crate::models::AccountType::Managed;
    pending.setup_step = crate::models::AccountSetupStep::KeyVerification;
    db.create_account(&pending).unwrap();
    let mut empty = account("empty", true, None);
    empty.key_cipher.clear();
    db.create_account(&empty).unwrap();
    db.create_account(&account("ready", true, None)).unwrap();

    let selected = AccountSelector::new().select(&db, None).unwrap().unwrap();
    assert_eq!(selected.id, "ready");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_usage_does_not_exclude_only_account() {
    let dir = temp_data_dir("local-usage");
    let db = Database::open(dir.clone()).unwrap();
    db.create_account(&account("estimated-full", true, None))
        .unwrap();
    db.log_forward(&ForwardLog {
        id: 0,
        timestamp: Utc::now(),
        model: "test".into(),
        account_id: "estimated-full".into(),
        account_name: "estimated-full".into(),
        route_account_id: None,
        provider_id: None,

        credential_account_id: None,
        client_key_id: None,
        client_key_name: None,
        status: "success".into(),
        http_status: Some(200),
        route: String::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
        cached_tokens: 0,
        cache_creation_tokens: 0,
        cost: Some(1000.0),
        raw_cost_usd: None,
        quota_debit: None,
        effective_paid_cost_usd: None,
        pricing_revision_id: None,
        quota_multiplier: None,
        local_adjustment_multiplier: None,
        service_tier: None,
        cost_state: "legacy_estimate".into(),
        error_message: None,
        request_id: None,
        attempt: None,
        error_source: None,
        error_stage: None,
        duration_ms: None,
        diagnostic: None,
    })
    .unwrap();

    // 月用量已远超 60 上限（被 compute_month_window 钳到 60.0），但因为没有别的账号可选，
    // selector 仍然返回它。
    assert!(db.account_usage("estimated-full").unwrap().window_month >= 60.0);
    assert_eq!(
        AccountSelector::new()
            .select(&db, None)
            .unwrap()
            .unwrap()
            .id,
        "estimated-full"
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn one_free_cooldown_exhausts_the_whole_free_channel() {
    let mut cooled = account(crate::provider::ZEN_FREE_ACCOUNT_ID, false, None);
    cooled.provider_id = crate::provider::OPENCODE_ZEN_FREE_PROVIDER_ID.to_string();
    cooled.credential_kind = crate::provider::CredentialKind::None;
    cooled.quota_scope = crate::provider::QuotaScope::EgressIp;
    cooled.key_cipher.clear();
    cooled.cooldown_free_until = Some(Utc::now() + Duration::hours(1));
    let next = account("next", true, None);
    let accounts = vec![cooled, next];

    assert!(AccountSelector::free_channel_exhausted(&accounts));
    assert!(AccountSelector::first_available_for(&accounts, UpstreamChannel::Free, &[]).is_none());
    assert_eq!(
        AccountSelector::first_available_for(&accounts, UpstreamChannel::Go, &[])
            .unwrap()
            .id,
        "next"
    );
}

fn frozen_wall() -> chrono::DateTime<Utc> {
    chrono::DateTime::from_naive_utc_and_offset(
        chrono::NaiveDate::from_ymd_opt(2024, 1, 2)
            .unwrap()
            .and_hms_opt(3, 4, 5)
            .unwrap(),
        Utc,
    )
}

#[test]
fn selection_cooldown_uses_injected_wall() {
    let wall = frozen_wall();
    let mut cooled = account("cooled", true, Some(wall + Duration::hours(1)));
    cooled.cooldown_generic_until = Some(wall + Duration::hours(1));
    let next = account("next", true, None);
    let accounts = vec![cooled, next];

    assert!(
        AccountSelector::first_available_at(&accounts, &[], wall)
            .is_some_and(|account| account.id == "next")
    );
    assert!(
        AccountSelector::first_available_at(
            &accounts,
            &[],
            wall + Duration::hours(1) + Duration::seconds(1),
        )
        .is_some_and(|account| account.id == "cooled")
    );
}

#[test]
fn free_channel_exhaustion_uses_injected_wall() {
    let wall = frozen_wall();
    let mut cooled = account(crate::provider::ZEN_FREE_ACCOUNT_ID, false, None);
    cooled.provider_id = crate::provider::OPENCODE_ZEN_FREE_PROVIDER_ID.to_string();
    cooled.credential_kind = crate::provider::CredentialKind::None;
    cooled.quota_scope = crate::provider::QuotaScope::EgressIp;
    cooled.key_cipher.clear();
    cooled.cooldown_free_until = Some(wall + Duration::hours(1));
    let accounts = vec![cooled, account("next", true, None)];

    assert!(AccountSelector::free_channel_exhausted_at(&accounts, wall));
    assert!(
        AccountSelector::first_available_for_at(&accounts, UpstreamChannel::Free, &[], wall)
            .is_none()
    );
    assert!(!AccountSelector::free_channel_exhausted_at(
        &accounts,
        wall + Duration::hours(1) + Duration::seconds(1),
    ));
}

#[test]
fn candidate_cooldown_at_exact_deadline_is_available() {
    let wall = frozen_wall();
    let mut cooled = account("cooled", true, Some(wall));
    cooled.cooldown_generic_until = Some(wall);
    let next = account("next", true, None);

    assert!(
        AccountSelector::is_available_at(&cooled, &[], wall),
        "until == now must be expired/available"
    );
    assert!(
        !AccountSelector::is_available_at(&cooled, &[], wall - Duration::seconds(1)),
        "until > now must remain cooling"
    );
    assert!(
        AccountSelector::first_available_at(&[cooled, next], &[], wall)
            .is_some_and(|account| account.id == "cooled")
    );
}

#[test]
fn disabled_zen_free_exhaustion_expires_at_exact_deadline() {
    let wall = frozen_wall();
    let mut cooled = account(crate::provider::ZEN_FREE_ACCOUNT_ID, false, None);
    cooled.provider_id = crate::provider::OPENCODE_ZEN_FREE_PROVIDER_ID.to_string();
    cooled.credential_kind = crate::provider::CredentialKind::None;
    cooled.quota_scope = crate::provider::QuotaScope::EgressIp;
    cooled.key_cipher.clear();
    cooled.cooldown_free_until = Some(wall);
    let accounts = vec![cooled];

    assert!(
        AccountSelector::free_channel_exhausted_at(&accounts, wall - Duration::seconds(1)),
        "until > now must exhaust the Free channel even on a disabled row"
    );
    assert!(
        !AccountSelector::free_channel_exhausted_at(&accounts, wall),
        "until == now must expire Free exhaustion on a disabled Zen row"
    );
}
