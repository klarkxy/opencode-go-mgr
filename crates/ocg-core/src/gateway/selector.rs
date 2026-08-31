use crate::db::Database;
use crate::models::{Account, UpstreamChannel};
use anyhow::Result;
use chrono::{DateTime, Utc};

#[derive(Default)]
pub struct AccountSelector;

impl AccountSelector {
    pub fn new() -> Self {
        Self
    }

    pub fn select(&self, db: &Database, exclude_id: Option<&str>) -> Result<Option<Account>> {
        self.select_at(db, exclude_id, Utc::now())
    }

    pub fn select_at(
        &self,
        db: &Database,
        exclude_id: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Option<Account>> {
        let excluded = exclude_id.into_iter().collect::<Vec<_>>();
        self.select_excluding_at(db, &excluded, now)
    }

    pub fn select_excluding(&self, db: &Database, exclude_ids: &[&str]) -> Result<Option<Account>> {
        self.select_excluding_at(db, exclude_ids, Utc::now())
    }

    pub fn select_excluding_at(
        &self,
        db: &Database,
        exclude_ids: &[&str],
        now: DateTime<Utc>,
    ) -> Result<Option<Account>> {
        self.select_excluding_for_at(db, UpstreamChannel::Go, exclude_ids, now)
    }

    pub fn select_excluding_for(
        &self,
        db: &Database,
        channel: UpstreamChannel,
        exclude_ids: &[&str],
    ) -> Result<Option<Account>> {
        self.select_excluding_for_at(db, channel, exclude_ids, Utc::now())
    }

    pub fn select_excluding_for_at(
        &self,
        db: &Database,
        channel: UpstreamChannel,
        exclude_ids: &[&str],
        now: DateTime<Utc>,
    ) -> Result<Option<Account>> {
        let accounts = db.list_accounts()?;
        Ok(Self::first_available_for_at(
            &accounts,
            channel,
            exclude_ids,
            now,
        ))
    }

    pub fn is_available(account: &Account, exclude_ids: &[&str]) -> bool {
        Self::is_available_at(account, exclude_ids, Utc::now())
    }

    pub fn is_available_at(account: &Account, exclude_ids: &[&str], now: DateTime<Utc>) -> bool {
        Self::is_available_for_at(account, UpstreamChannel::Go, exclude_ids, now)
    }

    pub fn is_available_for(
        account: &Account,
        channel: UpstreamChannel,
        exclude_ids: &[&str],
    ) -> bool {
        crate::routing_runtime::account_is_available_for(account, channel, exclude_ids)
    }

    pub fn is_available_for_at(
        account: &Account,
        channel: UpstreamChannel,
        exclude_ids: &[&str],
        now: DateTime<Utc>,
    ) -> bool {
        crate::routing_runtime::account_is_available_for_at(account, channel, exclude_ids, now)
    }

    pub fn first_available(accounts: &[Account], exclude_ids: &[&str]) -> Option<Account> {
        Self::first_available_at(accounts, exclude_ids, Utc::now())
    }

    pub fn first_available_at(
        accounts: &[Account],
        exclude_ids: &[&str],
        now: DateTime<Utc>,
    ) -> Option<Account> {
        Self::first_available_for_at(accounts, UpstreamChannel::Go, exclude_ids, now)
    }

    /// Account-row compatibility guard for the IP-shared Zen free cooldown.
    /// The durable global gate is checked by the request handler; do not filter
    /// disabled or unfinished rows here because changing account lifecycle state
    /// cannot restore an egress-IP quota.
    pub fn free_channel_exhausted(accounts: &[Account]) -> bool {
        Self::free_channel_exhausted_at(accounts, Utc::now())
    }

    pub fn free_channel_exhausted_at(accounts: &[Account], now: DateTime<Utc>) -> bool {
        crate::routing_runtime::free_channel_is_exhausted_at(accounts, now)
    }

    pub fn first_available_for(
        accounts: &[Account],
        channel: UpstreamChannel,
        exclude_ids: &[&str],
    ) -> Option<Account> {
        Self::first_available_for_at(accounts, channel, exclude_ids, Utc::now())
    }

    pub fn first_available_for_at(
        accounts: &[Account],
        channel: UpstreamChannel,
        exclude_ids: &[&str],
        now: DateTime<Utc>,
    ) -> Option<Account> {
        if channel == UpstreamChannel::Free && Self::free_channel_exhausted_at(accounts, now) {
            return None;
        }
        accounts
            .iter()
            .find(|account| Self::is_available_for_at(account, channel, exclude_ids, now))
            .cloned()
    }

    pub fn find_available(
        accounts: &[Account],
        account_id: &str,
        exclude_ids: &[&str],
    ) -> Option<Account> {
        Self::find_available_at(accounts, account_id, exclude_ids, Utc::now())
    }

    pub fn find_available_at(
        accounts: &[Account],
        account_id: &str,
        exclude_ids: &[&str],
        now: DateTime<Utc>,
    ) -> Option<Account> {
        Self::find_available_for_at(accounts, UpstreamChannel::Go, account_id, exclude_ids, now)
    }

    pub fn find_available_for(
        accounts: &[Account],
        channel: UpstreamChannel,
        account_id: &str,
        exclude_ids: &[&str],
    ) -> Option<Account> {
        Self::find_available_for_at(accounts, channel, account_id, exclude_ids, Utc::now())
    }

    pub fn find_available_for_at(
        accounts: &[Account],
        channel: UpstreamChannel,
        account_id: &str,
        exclude_ids: &[&str],
        now: DateTime<Utc>,
    ) -> Option<Account> {
        if channel == UpstreamChannel::Free && Self::free_channel_exhausted_at(accounts, now) {
            return None;
        }
        accounts
            .iter()
            .find(|account| {
                account.id == account_id
                    && Self::is_available_for_at(account, channel, exclude_ids, now)
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests;
