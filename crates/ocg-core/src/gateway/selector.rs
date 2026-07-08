use crate::db::Database;
use crate::models::{Account, SelectionStrategy};
use anyhow::Result;
use chrono::Utc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct AccountSelector {
    strategy: SelectionStrategy,
    round_robin_index: Arc<AtomicUsize>,
}

impl AccountSelector {
    pub fn new(strategy: SelectionStrategy) -> Self {
        Self {
            strategy,
            round_robin_index: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn with_counter(strategy: SelectionStrategy, counter: Arc<AtomicUsize>) -> Self {
        Self {
            strategy,
            round_robin_index: counter,
        }
    }

    pub fn select(&self, db: &Database, exclude_id: Option<&str>) -> Result<Option<Account>> {
        let accounts = db.list_accounts()?;
        let now = Utc::now();
        let mut available: Vec<Account> = Vec::new();
        for account in accounts {
            if !account.enabled {
                continue;
            }
            if let Some(excluded) = exclude_id {
                if account.id == excluded {
                    continue;
                }
            }
            // ponytail: cooldown check piggybacks on list_accounts (no extra query).
            // Add a per-row cache or index when account count exceeds ~100.
            if let Some(until) = account.cooldown_until {
                if until > now {
                    continue;
                }
            }
            available.push(account);
        }

        if available.is_empty() {
            return Ok(None);
        }

        let selected = match self.strategy {
            SelectionStrategy::Sequential => available.into_iter().next(),
            SelectionStrategy::Random => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let idx = (seed % available.len() as u128) as usize;
                available.into_iter().nth(idx)
            }
            SelectionStrategy::RoundRobin => {
                let idx = self.round_robin_index.fetch_add(1, Ordering::SeqCst) % available.len();
                available.into_iter().nth(idx)
            }
        };

        Ok(selected)
    }
}
