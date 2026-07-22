use crate::db::Database;
use crate::models::Account;
use anyhow::Result;
use chrono::Utc;

#[derive(Default)]
pub struct AccountSelector;

impl AccountSelector {
    pub fn new() -> Self {
        Self
    }

    pub fn select(&self, db: &Database, exclude_id: Option<&str>) -> Result<Option<Account>> {
        let excluded = exclude_id.into_iter().collect::<Vec<_>>();
        self.select_excluding(db, &excluded)
    }

    pub fn select_excluding(&self, db: &Database, exclude_ids: &[&str]) -> Result<Option<Account>> {
        let now = Utc::now();
        for account in db.list_accounts()? {
            if !account.enabled {
                continue;
            }
            if exclude_ids.iter().any(|excluded| account.id == *excluded) {
                continue;
            }
            if account.is_cooling_at(now) {
                continue;
            }
            return Ok(Some(account));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
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
            name: id.into(),
            username: None,
            password_cipher: None,
            key_cipher: cipher.encrypt(id).unwrap(),
            enabled,
            referral_code: None,
            purchase_date: String::new(),
            expires_on: String::new(),
            cooldown_until: cooldown,
            cooldown_generic_until: cooldown,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            last_error: None,
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
        db.reorder_accounts(&["second".into(), "first".into()])
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
            status: "success".into(),
            http_status: Some(200),
            prompt_tokens: 0,
            completion_tokens: 0,
            cached_tokens: 0,
            cache_creation_tokens: 0,
            cost: Some(1000.0),
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
}
