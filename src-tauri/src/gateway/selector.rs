use crate::db::Database;
use crate::gateway::circuit_breaker::CircuitBreaker;
use crate::models::{Account, CircuitState, SelectionStrategy};
use anyhow::Result;
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
        let mut available: Vec<(Account, CircuitState)> = Vec::new();
        for account in accounts {
            if !account.enabled {
                continue;
            }
            if let Some(excluded) = exclude_id {
                if account.id == excluded {
                    continue;
                }
            }
            let state = db.get_circuit_state(&account.id)?;
            if CircuitBreaker::is_available(&state) {
                available.push((account, state));
            }
        }

        if available.is_empty() {
            return Ok(None);
        }

        let selected = match self.strategy {
            SelectionStrategy::Sequential => available.into_iter().next().map(|(a, _)| a),
            SelectionStrategy::Random => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let idx = (seed % available.len() as u128) as usize;
                available.into_iter().nth(idx).map(|(a, _)| a)
            }
            SelectionStrategy::RoundRobin => {
                let idx = self.round_robin_index.fetch_add(1, Ordering::SeqCst) % available.len();
                available.into_iter().nth(idx).map(|(a, _)| a)
            }
        };

        Ok(selected)
    }
}
