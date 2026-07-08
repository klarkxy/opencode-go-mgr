use crate::db::Database;
use crate::models::{CircuitLevel, CircuitState};
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};

const ERROR_THRESHOLD: i32 = 6;
const MONTHLY_THRESHOLD: i32 = 7;

pub struct CircuitBreaker;

impl CircuitBreaker {
    pub fn record_error(db: &Database, account_id: &str) -> Result<CircuitState> {
        let mut state = db.get_circuit_state(account_id)?;
        let now = Utc::now();
        state.consecutive_errors += 1;
        if state.first_error_at.is_none() {
            state.first_error_at = Some(now);
        }
        state.last_error_at = Some(now);

        state.level = Self::evaluate_level(&state, now);
        state.cooldown_until = match state.level {
            CircuitLevel::Normal => None,
            CircuitLevel::Cooldown5m => Some(now + Duration::minutes(5)),
            CircuitLevel::Cooldown1h => Some(now + Duration::hours(1)),
            CircuitLevel::Cooldown1d => Some(now + Duration::days(1)),
            CircuitLevel::MonthlyBlown => Some(now + Duration::days(30)),
        };

        db.save_circuit_state(&state)?;
        Ok(state)
    }

    pub fn record_success(db: &Database, account_id: &str) -> Result<CircuitState> {
        let mut state = db.get_circuit_state(account_id)?;
        if state.consecutive_errors > 0 {
            state.consecutive_errors = 0;
            state.first_error_at = None;
            state.last_error_at = None;
            state.cooldown_until = None;
            state.level = CircuitLevel::Normal;
            db.save_circuit_state(&state)?;
        }
        Ok(state)
    }

    pub fn is_available(state: &CircuitState) -> bool {
        match state.cooldown_until {
            Some(until) if until > Utc::now() => false,
            _ => state.level != CircuitLevel::MonthlyBlown,
        }
    }

    fn evaluate_level(state: &CircuitState, now: DateTime<Utc>) -> CircuitLevel {
        if state.consecutive_errors < ERROR_THRESHOLD {
            return CircuitLevel::Normal;
        }

        let first = state.first_error_at.unwrap_or(now);
        let window = now - first;

        if window <= Duration::minutes(30) {
            CircuitLevel::Cooldown5m
        } else if window <= Duration::hours(5) {
            CircuitLevel::Cooldown1h
        } else if window <= Duration::hours(24) {
            CircuitLevel::Cooldown1d
        } else if state.consecutive_errors >= MONTHLY_THRESHOLD && window <= Duration::days(7) {
            CircuitLevel::MonthlyBlown
        } else {
            // ponytail: old burst of errors spread over a long window →
            // reset escalation. Consecutive_errors stays counted but we let
            // Normal through; the next error will be the 1st in a new window.
            CircuitLevel::Normal
        }
    }
}
