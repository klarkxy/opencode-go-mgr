use crate::db::Database;
use crate::models::Account;
use anyhow::Result;
use chrono::Utc;

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
            if let Some(until) = account.cooldown_until {
                if until > now {
                    continue;
                }
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
            recharge_date: None,
            cooldown_until: cooldown,
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
    fn local_usage_does_not_exclude_account() {
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
            cost: 1000.0,
            error_message: None,
        })
        .unwrap();

        assert!(db.account_usage("estimated-full").unwrap().window_month > 60.0);
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
