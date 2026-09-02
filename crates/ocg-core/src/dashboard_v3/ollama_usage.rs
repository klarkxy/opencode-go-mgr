//! Ollama Cloud Cookie usage endpoints.
//!
//! Three account-scoped surfaces: a sanitized status read, a CAS Cookie
//! write/clear, and a manual refresh. The refresh runs the bounded scrape in
//! [`crate::ollama_usage`] through the process-wide outbound client, applies
//! the 30-second manual throttle plus the fixed failure-backoff ladder, and
//! deduplicates concurrent refreshes. Usage failures never write inference
//! cooldowns and never change account enablement or routing eligibility —
//! that isolation is locked by a source-scan test at the bottom of this file.

use super::{
    MutationExpectation, OllamaCookieUpdate, OllamaUsageStatus, OllamaUsageThrottleError,
    V3ApiError, parse_mutation_json,
};
use crate::ollama_usage::{
    self, ParseOutcome, failure_backoff, manual_next_allowed_at, normalize_cookie_header,
    sanitize_error_text,
};
use crate::provider::{OLLAMA_CLOUD_OFFERING_ID, OLLAMA_PROVIDER_ID};
use crate::state::CoreState;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};

/// Throttled refreshes answer HTTP 429 with the absolute retry instant and
/// a `Retry-After` header, mirroring the official usage refresh envelope.
pub(super) enum OllamaUsageApiError {
    Api(V3ApiError),
    Throttled {
        body: OllamaUsageThrottleError,
        retry_after_secs: u64,
    },
}

impl From<V3ApiError> for OllamaUsageApiError {
    fn from(error: V3ApiError) -> Self {
        Self::Api(error)
    }
}

impl IntoResponse for OllamaUsageApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Api(error) => error.into_response(),
            Self::Throttled {
                body,
                retry_after_secs,
            } => {
                let mut response = (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response();
                if let Ok(header_value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                    response
                        .headers_mut()
                        .insert(header::RETRY_AFTER, header_value);
                }
                response
            }
        }
    }
}

pub(super) async fn get_ollama_usage(
    State(state): State<CoreState>,
    Path(id): Path<String>,
) -> Result<Json<OllamaUsageStatus>, V3ApiError> {
    ollama_usage_status_locked(&state, &id).map(Json)
}

/// PUT `/accounts/{id}/ollama-cookie`. `cookie: null` clears the stored web
/// session (and with it the snapshot and refresh state); a string is
/// validated as a Cookie request header and stored with the same obfuscation
/// facility as account keys.
pub(super) async fn put_ollama_cookie(
    State(state): State<CoreState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<OllamaUsageStatus>, V3ApiError> {
    let input = parse_mutation_json::<OllamaCookieUpdate>(&body)?;
    let _settings_update = state.settings_update.lock();
    super::check_expectation(&state, &input.expectation)?;
    let account = load_ollama_account(&state, &id)?;
    match input.cookie.as_deref().map(str::trim) {
        None | Some("") => {
            state
                .db
                .lock()
                .clear_ollama_cloud_cookie(&account.id)
                .map_err(V3ApiError::internal)?;
        }
        Some(raw) => {
            let normalized = normalize_cookie_header(raw)
                .map_err(|message| V3ApiError::invalid_request_at(&state, message))?;
            let cipher = state
                .encrypt_key(&normalized)
                .map_err(V3ApiError::internal)?;
            state
                .db
                .lock()
                .set_ollama_cloud_cookie(&account.id, &cipher)
                .map_err(V3ApiError::internal)?;
        }
    }
    state.bump_settings_revision();
    state.log_runtime_event(
        "info",
        "ollama_usage",
        &format!(
            "event=ollama_cookie_{} account_id={}",
            if input
                .cookie
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                "cleared"
            } else {
                "configured"
            },
            account.id
        ),
    );
    ollama_usage_status_locked(&state, &id).map(Json)
}

/// POST `/accounts/{id}/ollama-usage/refresh`. Manual-only: opt-in scrape of
/// the fixed settings page with the account Cookie. Throttled to one attempt
/// per 30 seconds (success or failure) and gated by the fixed backoff ladder.
pub(super) async fn refresh_ollama_usage(
    State(state): State<CoreState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<OllamaUsageStatus>, OllamaUsageApiError> {
    let expectation = parse_mutation_json::<MutationExpectation>(&body)?;
    let _refresh = state
        .ollama_usage_refresh
        .try_lock()
        .map_err(|_| V3ApiError::conflict_at(&state, "Ollama usage refresh is already running"))?;

    let (account, cookie_cipher) = {
        let _settings_update = state.settings_update.lock();
        super::check_expectation(&state, &expectation)?;
        let account = load_ollama_account(&state, &id)?;
        // Configuration check first: an unconfigured Cookie is the actionable
        // message for a staged account, and the disabled gate below must not
        // mask which half of the capability is missing.
        let cookie_cipher = state
            .db
            .lock()
            .ollama_cloud_cookie_cipher(&account.id)
            .map_err(V3ApiError::internal)?
            .ok_or_else(|| {
                V3ApiError::invalid_request_at(
                    &state,
                    "configure the account's web-session Cookie before refreshing usage",
                )
            })?;
        if !account.enabled {
            return Err(V3ApiError::invalid_request_at(
                &state,
                "the account is disabled; usage refresh is unavailable",
            )
            .into());
        }
        let existing = state
            .db
            .lock()
            .ollama_cloud_usage_state_for_cookie(&account.id, &cookie_cipher)
            .map_err(V3ApiError::internal)?
            .ok_or_else(|| {
                V3ApiError::invalid_request_at(
                    &state,
                    "configure the account's web-session Cookie before refreshing usage",
                )
            })?;
        if let Some(throttle) = refresh_throttle(&existing, Utc::now()) {
            return Err(throttled(throttle));
        }
        (account, cookie_cipher)
    };

    let config = state.config();
    let cookie = state
        .decrypt_key(&cookie_cipher)
        .map_err(V3ApiError::internal)?;
    let origin = {
        #[cfg(debug_assertions)]
        {
            crate::goat::ollama_cloud_models_base_url(Some(state.process_generation()))
        }
        #[cfg(not(debug_assertions))]
        {
            crate::provider::OLLAMA_CLOUD_BASE_URL.to_string()
        }
    };
    let outcome = match ollama_usage::fetch_settings_page(&config, &cookie, &origin).await {
        Ok(page) => ollama_usage::parse_settings_usage(&page.body),
        Err(message) => ParseOutcome::Failed(sanitize_error_text(&message)),
    };

    let now = Utc::now();
    let log_event = {
        let _settings_update = state.settings_update.lock();
        super::check_expectation(&state, &expectation)?;
        // The Cookie may have been rotated or cleared while the scrape ran.
        let current_cipher = state
            .db
            .lock()
            .ollama_cloud_cookie_cipher(&account.id)
            .map_err(V3ApiError::internal)?;
        if current_cipher.as_deref() != Some(cookie_cipher.as_str()) {
            return Err(V3ApiError::conflict_at(
                &state,
                "the account's Cookie changed while usage was refreshing",
            )
            .into());
        }
        let db = state.db.lock();
        // Runtime-event logging happens AFTER the db guard is dropped
        // (log_runtime_event takes the db lock itself; parking_lot is not
        // reentrant and logging under the guard deadlocks the host thread).
        let log_event = match outcome {
            ParseOutcome::Snapshot(snapshot) => {
                let snapshot_json =
                    serde_json::to_string(&snapshot).map_err(V3ApiError::internal)?;
                // Success keeps the manual throttle window as the next
                // eligible instant; the backoff ladder does not apply.
                db.commit_ollama_cloud_usage_success(
                    &account.id,
                    &snapshot_json,
                    now,
                    Some(manual_next_allowed_at(now)),
                )
                .map_err(V3ApiError::internal)?;
                (
                    "info",
                    format!(
                        "event=ollama_usage_refresh_succeeded account_id={} provider=ollama",
                        account.id
                    ),
                )
            }
            ParseOutcome::Unauthorized => {
                record_failure(
                    &db,
                    &account.id,
                    "unauthorized",
                    Some("the web session expired; reconfigure the Cookie"),
                    now,
                )?;
                (
                    "warn",
                    format!(
                        "event=ollama_usage_refresh_unauthorized account_id={} provider=ollama",
                        account.id
                    ),
                )
            }
            ParseOutcome::Failed(message) => {
                // Already sanitized at construction (fetch failures pass
                // through sanitize_error_text there).
                record_failure(&db, &account.id, "failed", Some(&message), now)?;
                (
                    "warn",
                    format!(
                        "event=ollama_usage_refresh_failed account_id={} provider=ollama",
                        account.id
                    ),
                )
            }
        };
        Some(log_event)
    };
    if let Some((level, message)) = log_event {
        state.log_runtime_event(level, "ollama_usage", &message);
    }
    ollama_usage_status_locked(&state, &id)
        .map(Json)
        .map_err(Into::into)
}

fn record_failure(
    db: &crate::db::Database,
    account_id: &str,
    status: &str,
    message: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), V3ApiError> {
    // A missing row starts the ladder at 1; a read/write ERROR must surface
    // instead of silently dropping the throttle and backoff state — reporting
    // a recorded failure that was never persisted would let unthrottled
    // retries hit the upstream.
    let streak = db
        .ollama_cloud_usage_state(account_id)
        .map_err(V3ApiError::internal)?
        .map(|state| state.failure_streak + 1)
        .unwrap_or(1);
    let next_eligible = now + failure_backoff(streak - 1);
    db.record_ollama_cloud_usage_failure(
        account_id,
        status,
        message,
        now,
        Some(next_eligible),
        streak,
    )
    .map_err(V3ApiError::internal)
}

/// Throttle decision for one stored state at `now`: the later of the manual
/// window after the last attempt and any active backoff eligibility.
fn refresh_throttle(
    state: &crate::models::OllamaCloudUsageState,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let manual = state
        .last_attempt_at
        .map(manual_next_allowed_at)
        .filter(|allowed| *allowed > now);
    let backoff = state.next_eligible_at.filter(|eligible| *eligible > now);
    match (manual, backoff) {
        (Some(manual), Some(backoff)) => Some(manual.max(backoff)),
        (manual, backoff) => manual.or(backoff),
    }
}

fn throttled(next_allowed_at: DateTime<Utc>) -> OllamaUsageApiError {
    let retry_after_secs = (next_allowed_at - Utc::now())
        .num_seconds()
        .clamp(1, 6 * 60 * 60) as u64;
    OllamaUsageApiError::Throttled {
        body: OllamaUsageThrottleError {
            code: super::ERROR_THROTTLED.to_string(),
            message: "Ollama usage refresh is temporarily throttled; retry later".to_string(),
            next_allowed_at: next_allowed_at.to_rfc3339(),
        },
        retry_after_secs,
    }
}

fn load_ollama_account(state: &CoreState, id: &str) -> Result<crate::models::Account, V3ApiError> {
    let account = state
        .db
        .lock()
        .get_account(id)
        .map_err(V3ApiError::internal)?
        .ok_or_else(|| V3ApiError::not_found_at(state, "account not found"))?;
    if account.provider_id != OLLAMA_PROVIDER_ID || account.offering_id != OLLAMA_CLOUD_OFFERING_ID
    {
        return Err(V3ApiError::invalid_request_at(
            state,
            "usage scraping is only available for Ollama Cloud accounts",
        ));
    }
    Ok(account)
}

fn ollama_usage_status_locked(
    state: &CoreState,
    id: &str,
) -> Result<OllamaUsageStatus, V3ApiError> {
    let account = load_ollama_account(state, id)?;
    let row = state
        .db
        .lock()
        .ollama_cloud_usage_state(&account.id)
        .map_err(V3ApiError::internal)?;
    let revision = super::ControlRevision::from_state(state);
    let (
        cookie_configured,
        status,
        snapshot,
        last_error,
        last_success_at,
        last_attempt_at,
        next_eligible_at,
        failure_streak,
    ) = match row {
        Some(row) => (
            row.cookie_configured,
            row.status,
            row.snapshot
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok()),
            row.last_error,
            row.last_success_at.map(|at| at.to_rfc3339()),
            row.last_attempt_at.map(|at| at.to_rfc3339()),
            row.next_eligible_at.map(|at| at.to_rfc3339()),
            row.failure_streak,
        ),
        // A never-configured account is the unconfigured state, not an
        // empty-string status that would violate the frozen contract.
        None => (
            false,
            "unconfigured".to_string(),
            None,
            None,
            None,
            None,
            None,
            0,
        ),
    };
    Ok(OllamaUsageStatus {
        account_id: account.id,
        cookie_configured,
        status,
        snapshot,
        last_error,
        last_success_at,
        last_attempt_at,
        next_eligible_at,
        failure_streak,
        revision: revision.revision,
        process_generation: revision.process_generation,
    })
}

#[cfg(test)]
mod tests {
    use super::super::OllamaUsageSnapshot;
    use super::*;

    #[test]
    fn persisted_snapshot_round_trips_into_the_contract_dto() {
        // The contract DTO must keep accepting exactly the JSON that
        // `crate::ollama_usage` persists, byte-for-byte in both directions.
        let domain = crate::ollama_usage::OllamaUsageSnapshot {
            windows: vec![crate::ollama_usage::OllamaUsageWindow {
                window: "5h".into(),
                used_percent: Some(42.5),
                reset_at: Some("2026-09-02T12:00:00Z".into()),
            }],
            models: vec![crate::ollama_usage::OllamaModelRequests {
                model: "deepseek-v4-flash:0731".into(),
                requests_5h: Some(11),
                requests_7d: None,
            }],
            plan: Some("web".into()),
            balance: None,
        };
        let json = serde_json::to_string(&domain).unwrap();
        let contract: OllamaUsageSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(contract.windows[0].used_percent, Some(42.5));
        assert_eq!(contract.models[0].requests_7d, None);
        assert_eq!(contract.plan.as_deref(), Some("web"));
        assert!(contract.balance.is_none());
        assert_eq!(serde_json::to_string(&contract).unwrap(), json);
    }

    #[test]
    fn usage_paths_never_write_inference_cooldowns_or_account_state() {
        let source = include_str!("ollama_usage.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        for needle in [
            "set_account_rate_limit",
            "set_account_rate_limit_if_key_matches",
            "set_account_cooldown",
            "set_account_auth_error",
            "upsert_free_channel_cooldown",
            "update_account(",
            "cooldown_until",
        ] {
            assert!(
                !production.contains(needle),
                "usage paths must never name `{needle}`"
            );
        }
    }

    #[test]
    fn throttle_picks_the_later_of_manual_window_and_backoff() {
        let now = Utc::now();
        let mut state = crate::models::OllamaCloudUsageState {
            account_id: "a".into(),
            cookie_configured: true,
            status: "ok".into(),
            snapshot: None,
            last_error: None,
            last_success_at: None,
            last_attempt_at: None,
            next_eligible_at: None,
            failure_streak: 0,
        };
        assert!(refresh_throttle(&state, now).is_none());

        state.last_attempt_at = Some(now - chrono::Duration::seconds(10));
        let manual = refresh_throttle(&state, now).expect("manual window applies");
        assert_eq!(
            manual,
            manual_next_allowed_at(now - chrono::Duration::seconds(10))
        );

        state.next_eligible_at = Some(now + chrono::Duration::hours(2));
        let later = refresh_throttle(&state, now).expect("backoff wins when later");
        assert_eq!(later, now + chrono::Duration::hours(2));

        state.last_attempt_at = Some(now - chrono::Duration::hours(3));
        assert_eq!(
            refresh_throttle(&state, now),
            Some(now + chrono::Duration::hours(2)),
            "stale attempts do not extend an active backoff"
        );
    }
}
