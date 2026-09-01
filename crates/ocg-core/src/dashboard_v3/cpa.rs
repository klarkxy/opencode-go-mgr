//! Typed Dashboard V3 control plane for one user-operated local CPA runtime.
//! Network operations are serialized and never hold SQLite or synchronous
//! state locks while awaiting CPA.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use chrono::Utc;
use serde::Deserialize;

use crate::cpa::{self, CpaClient};
use crate::models::{Account as ModelAccount, AccountSetupStep, AccountType};
use crate::provider::{
    CPA_ACCOUNT_ID, CPA_ACCOUNT_NAME, CPA_PROVIDER_ID, CredentialKind, QuotaScope,
};
use crate::state::CoreState;

use super::types::{
    CpaAccount, CpaAccountDelete, CpaAccountStatusUpdate, CpaAccounts, CpaConnectionReport,
    CpaIntegration, CpaIntegrationUpdate, CpaModels, CpaOAuthProvider, CpaOAuthSessionDelete,
    CpaOAuthStart, CpaOAuthStartRequest, CpaOAuthStatus, CpaQuotaReset, CpaTestRequest,
    MutationAck, MutationExpectation,
};
use super::{V3ApiError, check_expectation, parse_json, parse_mutation_json};

struct SavedCpa {
    base_url: String,
    management_key: String,
    inference_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OAuthStatusQuery {
    state: String,
}

pub(super) async fn get_integration(
    State(state): State<CoreState>,
) -> Result<Json<CpaIntegration>, V3ApiError> {
    integration_view(&state).map(Json)
}

pub(super) async fn put_integration(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<CpaIntegration>, V3ApiError> {
    let input = parse_mutation_json::<CpaIntegrationUpdate>(&body)?;
    let _operation = state.cpa_operations.lock().await;
    let _settings = state.settings_update.lock();
    check_expectation(&state, &input.expectation)?;

    let env_base = cpa::env_base_url().map_err(|error| map_cpa_error(&state, error))?;
    if env_base.is_some() && input.base_url.is_some() {
        return Err(V3ApiError::invalid_request_at(
            &state,
            "CPA base URL is controlled by OCG_CPA_BASE_URL in this runtime",
        ));
    }
    let (existing_record, existing_account) = {
        let db = state.db.lock();
        (
            db.cpa_integration().map_err(V3ApiError::internal)?,
            db.get_account(CPA_ACCOUNT_ID)
                .map_err(V3ApiError::internal)?,
        )
    };
    let base_url = env_base.unwrap_or_else(|| {
        input
            .base_url
            .clone()
            .or_else(|| {
                existing_record
                    .as_ref()
                    .map(|record| record.base_url.clone())
            })
            .unwrap_or_else(|| cpa::DEFAULT_CPA_BASE_URL.to_string())
    });
    let base_url = cpa::normalize_base_url(&base_url, false)
        .or_else(|error| {
            if std::env::var_os(cpa::CPA_BASE_URL_ENV).is_some() {
                cpa::normalize_base_url(&base_url, true)
            } else {
                Err(error)
            }
        })
        .map_err(|error| map_cpa_error(&state, error))?;

    let management_key_cipher = match clean_secret(input.management_key) {
        Some(value) => state.encrypt_key(&value).map_err(V3ApiError::internal)?,
        None => existing_record
            .as_ref()
            .map(|record| record.management_key_cipher.clone())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                V3ApiError::invalid_request_at(&state, "CPA Management Key is required")
            })?,
    };
    let inference_key_cipher = match clean_secret(input.inference_key) {
        Some(value) => state.encrypt_key(&value).map_err(V3ApiError::internal)?,
        None => existing_account
            .as_ref()
            .map(|account| account.key_cipher.clone())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                V3ApiError::invalid_request_at(&state, "CPA Inference Key is required")
            })?,
    };
    let now = Utc::now();
    let account = ModelAccount {
        id: CPA_ACCOUNT_ID.to_string(),
        provider_id: CPA_PROVIDER_ID.to_string(),

        credential_kind: CredentialKind::ApiKey,
        quota_scope: QuotaScope::Key,
        name: CPA_ACCOUNT_NAME.to_string(),
        username: None,
        password_cipher: None,
        key_cipher: inference_key_cipher,
        enabled: input
            .enabled
            .unwrap_or_else(|| existing_account.as_ref().is_some_and(|item| item.enabled)),
        account_type: AccountType::Key,
        setup_step: AccountSetupStep::Ready,
        referral_code: None,
        purchase_date: String::new(),
        expires_on: String::new(),
        cooldown_until: None,
        cooldown_generic_until: None,
        cooldown_5h_until: None,
        cooldown_week_until: None,
        cooldown_month_until: None,
        cooldown_free_until: None,
        last_error: None,
        auth_error: None,
        notes: None,
        created_at: existing_account
            .as_ref()
            .map_or(now, |item| item.created_at),
        updated_at: now,
    };
    state
        .db
        .lock()
        .upsert_cpa_integration(&account, &base_url, &management_key_cipher)
        .map_err(V3ApiError::internal)?;
    state.routing.reset();
    state.bump_settings_revision();
    integration_view(&state).map(Json)
}

pub(super) async fn delete_integration(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<MutationAck>, V3ApiError> {
    let expectation = parse_mutation_json::<MutationExpectation>(&body)?;
    let _operation = state.cpa_operations.lock().await;
    let _settings = state.settings_update.lock();
    check_expectation(&state, &expectation)?;
    state
        .disconnect_cpa_integration()
        .map_err(V3ApiError::internal)?;
    Ok(Json(committed_ack(&state)))
}

pub(super) async fn test_connection(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<CpaConnectionReport>, V3ApiError> {
    let input = parse_json::<CpaTestRequest>(&body)?;
    let _operation = state.cpa_operations.lock().await;
    let saved = load_saved(&state).ok();
    let env_base = cpa::env_base_url().map_err(|error| map_cpa_error(&state, error))?;
    let base_url = env_base
        .or(input.base_url)
        .or_else(|| saved.as_ref().map(|item| item.base_url.clone()))
        .unwrap_or_else(|| cpa::DEFAULT_CPA_BASE_URL.to_string());
    let management_key = clean_secret(input.management_key)
        .or_else(|| saved.as_ref().map(|item| item.management_key.clone()))
        .ok_or_else(|| V3ApiError::invalid_request_at(&state, "CPA Management Key is required"))?;
    let inference_key = clean_secret(input.inference_key)
        .or_else(|| saved.as_ref().map(|item| item.inference_key.clone()))
        .ok_or_else(|| V3ApiError::invalid_request_at(&state, "CPA Inference Key is required"))?;
    let client = CpaClient::new(
        &state.config(),
        &base_url,
        management_key,
        inference_key,
        std::env::var_os(cpa::CPA_BASE_URL_ENV).is_some(),
    )
    .map_err(|error| map_cpa_error(&state, error))?;
    let report = client
        .test()
        .await
        .map_err(|error| map_cpa_error(&state, error))?;
    Ok(Json(CpaConnectionReport {
        reachable: report.reachable,
        management_ready: report.management_ready,
        inference_ready: report.inference_ready,
        version: report.version.as_ref().map(|item| item.version.clone()),
        commit: report.version.as_ref().and_then(|item| item.commit.clone()),
        build_date: report
            .version
            .as_ref()
            .and_then(|item| item.build_date.clone()),
        model_count: report.model_count,
        management_error: report.management_error,
        inference_error: report.inference_error,
        revision: state.settings_revision(),
        process_generation: state.process_generation(),
    }))
}

pub(super) async fn refresh_models(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<CpaModels>, V3ApiError> {
    let expectation = parse_mutation_json::<MutationExpectation>(&body)?;
    let _operation = state.cpa_operations.lock().await;
    {
        let _settings = state.settings_update.lock();
        check_expectation(&state, &expectation)?;
    }
    let (client, base_url) = saved_client(&state)?;
    let models = client
        .models()
        .await
        .map_err(|error| map_cpa_error(&state, error))?;
    let _settings = state.settings_update.lock();
    check_expectation(&state, &expectation)?;
    let refreshed_at = Utc::now();
    state
        .activate_cpa_model_catalog(models.clone(), &base_url, refreshed_at)
        .map_err(V3ApiError::internal)?;
    let revision = state.bump_settings_revision();
    Ok(Json(CpaModels {
        models,
        refreshed_at: Some(refreshed_at.to_rfc3339()),
        revision,
        process_generation: state.process_generation(),
    }))
}

pub(super) async fn list_accounts(
    State(state): State<CoreState>,
) -> Result<Json<CpaAccounts>, V3ApiError> {
    let _operation = state.cpa_operations.lock().await;
    let (client, _) = saved_client(&state)?;
    let (version, accounts) = client
        .accounts()
        .await
        .map_err(|error| map_cpa_error(&state, error))?;
    Ok(Json(CpaAccounts {
        accounts: accounts.into_iter().map(account_view).collect(),
        version: version.version,
        revision: state.settings_revision(),
        process_generation: state.process_generation(),
    }))
}

pub(super) async fn set_account_status(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<MutationAck>, V3ApiError> {
    let input = parse_mutation_json::<CpaAccountStatusUpdate>(&body)?;
    let _operation = state.cpa_operations.lock().await;
    check_before_external_write(&state, &input.expectation)?;
    let (client, _) = saved_client(&state)?;
    client
        .set_account_disabled(&input.name, &input.auth_index, input.disabled)
        .await
        .map_err(|error| map_cpa_error(&state, error))?;
    Ok(Json(current_ack(&state)))
}

pub(super) async fn delete_account(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<MutationAck>, V3ApiError> {
    let input = parse_mutation_json::<CpaAccountDelete>(&body)?;
    let _operation = state.cpa_operations.lock().await;
    check_before_external_write(&state, &input.expectation)?;
    let (client, _) = saved_client(&state)?;
    client
        .delete_account(&input.name, &input.auth_index)
        .await
        .map_err(|error| map_cpa_error(&state, error))?;
    Ok(Json(current_ack(&state)))
}

pub(super) async fn reset_quota(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<MutationAck>, V3ApiError> {
    let input = parse_mutation_json::<CpaQuotaReset>(&body)?;
    let _operation = state.cpa_operations.lock().await;
    check_before_external_write(&state, &input.expectation)?;
    let (client, _) = saved_client(&state)?;
    client
        .reset_quota(&input.name, &input.auth_index)
        .await
        .map_err(|error| map_cpa_error(&state, error))?;
    Ok(Json(current_ack(&state)))
}

pub(super) async fn start_oauth(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<CpaOAuthStart>, V3ApiError> {
    let input = parse_mutation_json::<CpaOAuthStartRequest>(&body)?;
    let _operation = state.cpa_operations.lock().await;
    check_before_external_write(&state, &input.expectation)?;
    let (client, _) = saved_client(&state)?;
    let provider = cpa_provider(input.provider);
    let started = client
        .start_oauth(provider)
        .await
        .map_err(|error| map_cpa_error(&state, error))?;
    Ok(Json(CpaOAuthStart {
        provider: input.provider,
        state: started.state,
        url: started.url,
        flow: started.flow,
        user_code: started.user_code,
        expires_in: started.expires_in,
        revision: state.settings_revision(),
        process_generation: state.process_generation(),
    }))
}

pub(super) async fn oauth_status(
    State(state): State<CoreState>,
    Query(query): Query<OAuthStatusQuery>,
) -> Result<Json<CpaOAuthStatus>, V3ApiError> {
    let _operation = state.cpa_operations.lock().await;
    let (client, _) = saved_client(&state)?;
    let status = client
        .oauth_status(&query.state)
        .await
        .map_err(|error| map_cpa_error(&state, error))?;
    Ok(Json(CpaOAuthStatus {
        state: query.state,
        status: status.status,
        error: status.error,
        revision: state.settings_revision(),
        process_generation: state.process_generation(),
    }))
}

pub(super) async fn cancel_oauth(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<MutationAck>, V3ApiError> {
    let input = parse_mutation_json::<CpaOAuthSessionDelete>(&body)?;
    let _operation = state.cpa_operations.lock().await;
    check_before_external_write(&state, &input.expectation)?;
    let (client, _) = saved_client(&state)?;
    client
        .cancel_oauth(&input.state)
        .await
        .map_err(|error| map_cpa_error(&state, error))?;
    Ok(Json(current_ack(&state)))
}

fn integration_view(state: &CoreState) -> Result<CpaIntegration, V3ApiError> {
    let env_base = cpa::env_base_url().map_err(|error| map_cpa_error(state, error))?;
    let (record, account, catalog) = {
        let db = state.db.lock();
        (
            db.cpa_integration().map_err(V3ApiError::internal)?,
            db.get_account(CPA_ACCOUNT_ID)
                .map_err(V3ApiError::internal)?,
            db.cpa_model_catalog().map_err(V3ApiError::internal)?,
        )
    };
    Ok(CpaIntegration {
        configured: record.is_some() && account.is_some(),
        base_url: env_base
            .clone()
            .or_else(|| record.as_ref().map(|item| item.base_url.clone()))
            .unwrap_or_else(|| cpa::DEFAULT_CPA_BASE_URL.to_string()),
        base_url_read_only: env_base.is_some(),
        management_key_configured: record
            .as_ref()
            .is_some_and(|item| !item.management_key_cipher.is_empty()),
        inference_key_configured: account
            .as_ref()
            .is_some_and(|item| !item.key_cipher.is_empty()),
        enabled: account.as_ref().is_some_and(|item| item.enabled),
        account_id: account.map(|item| item.id),
        model_count: catalog.as_ref().map_or(0, |item| item.models.len()),
        models_refreshed_at: catalog
            .and_then(|item| item.refreshed_at)
            .map(|value| value.to_rfc3339()),
        revision: state.settings_revision(),
        process_generation: state.process_generation(),
    })
}

fn load_saved(state: &CoreState) -> Result<SavedCpa, V3ApiError> {
    let (record, account) = {
        let db = state.db.lock();
        (
            db.cpa_integration().map_err(V3ApiError::internal)?,
            db.get_account(CPA_ACCOUNT_ID)
                .map_err(V3ApiError::internal)?,
        )
    };
    let record =
        record.ok_or_else(|| V3ApiError::precondition_failed_at(state, "CPA is not configured"))?;
    let account = account.ok_or_else(|| {
        V3ApiError::precondition_failed_at(state, "CPA singleton account is missing")
    })?;
    let base_url = cpa::env_base_url()
        .map_err(|error| map_cpa_error(state, error))?
        .unwrap_or(record.base_url);
    Ok(SavedCpa {
        base_url,
        management_key: state
            .decrypt_key(&record.management_key_cipher)
            .map_err(V3ApiError::internal)?,
        inference_key: state
            .decrypt_key(&account.key_cipher)
            .map_err(V3ApiError::internal)?,
    })
}

fn saved_client(state: &CoreState) -> Result<(CpaClient, String), V3ApiError> {
    let saved = load_saved(state)?;
    let base_url = saved.base_url.clone();
    let client = CpaClient::new(
        &state.config(),
        &saved.base_url,
        saved.management_key,
        saved.inference_key,
        std::env::var_os(cpa::CPA_BASE_URL_ENV).is_some(),
    )
    .map_err(|error| map_cpa_error(state, error))?;
    Ok((client, base_url))
}

fn account_view(item: cpa::CpaAccountView) -> CpaAccount {
    CpaAccount {
        name: item.name,
        auth_index: item.auth_index,
        provider: item.provider,
        label: item.label,
        status: item.status,
        status_message: item.status_message,
        disabled: item.disabled,
        unavailable: item.unavailable,
        runtime_only: item.runtime_only,
        mutable: item.mutable,
        email: item.email,
        quota: item.quota,
    }
}

fn cpa_provider(provider: CpaOAuthProvider) -> cpa::CpaOAuthProvider {
    match provider {
        CpaOAuthProvider::Codex => cpa::CpaOAuthProvider::Codex,
        CpaOAuthProvider::Anthropic => cpa::CpaOAuthProvider::Anthropic,
        CpaOAuthProvider::Antigravity => cpa::CpaOAuthProvider::Antigravity,
        CpaOAuthProvider::Kimi => cpa::CpaOAuthProvider::Kimi,
        CpaOAuthProvider::Xai => cpa::CpaOAuthProvider::Xai,
    }
}

fn clean_secret(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn check_before_external_write(
    state: &CoreState,
    expectation: &MutationExpectation,
) -> Result<(), V3ApiError> {
    let _settings = state.settings_update.lock();
    check_expectation(state, expectation)
}

fn current_ack(state: &CoreState) -> MutationAck {
    MutationAck {
        revision: state.settings_revision(),
        process_generation: state.process_generation(),
    }
}

fn committed_ack(state: &CoreState) -> MutationAck {
    let revision = state.bump_settings_revision();
    MutationAck {
        revision,
        process_generation: state.process_generation(),
    }
}

fn map_cpa_error(state: &CoreState, error: cpa::CpaError) -> V3ApiError {
    match error {
        cpa::CpaError::Invalid(message) => V3ApiError::invalid_request_at(state, message),
        cpa::CpaError::Unreachable(message) => {
            V3ApiError::service_unavailable(state, format!("CPA is unreachable: {message}"))
        }
        cpa::CpaError::Http { status, message } => {
            V3ApiError::outbound_failed(state, format!("CPA returned HTTP {status}: {message}"))
        }
        cpa::CpaError::Response(message) | cpa::CpaError::Incompatible(message) => {
            V3ApiError::outbound_failed(state, message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::db::Database;
    use crate::state::CoreStateInner;
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;

    fn test_state(label: &str) -> (std::path::PathBuf, CoreState) {
        let dir = std::env::temp_dir().join(format!("ocg-v3-cpa-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> =
            Arc::new(StaticKeyCipher::new("v3-cpa-test"));
        let state = Arc::new(
            CoreStateInner::new(Database::open(dir.clone()).unwrap(), dir.clone(), cipher).unwrap(),
        );
        (dir, state)
    }

    #[tokio::test]
    async fn config_is_singleton_cas_encrypted_secret_free_and_disconnectable() {
        let (dir, state) = test_state("lifecycle");
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "expectedRevision": state.settings_revision(),
                "processGeneration": state.process_generation(),
                "managementKey": "management-secret",
                "inferenceKey": "inference-secret",
                "enabled": true
            }))
            .unwrap(),
        );
        let Ok(Json(view)) = put_integration(State(state.clone()), body).await else {
            panic!("CPA configuration should save");
        };
        assert!(view.configured);
        assert!(view.management_key_configured);
        assert!(view.inference_key_configured);
        assert!(!view.enabled);
        let encoded = serde_json::to_string(&view).unwrap();
        assert!(!encoded.contains("management-secret"));
        assert!(!encoded.contains("inference-secret"));
        assert!(!encoded.contains("cipher"));

        let (record, account) = {
            let db = state.db.lock();
            (
                db.cpa_integration().unwrap().unwrap(),
                db.get_account(CPA_ACCOUNT_ID).unwrap().unwrap(),
            )
        };
        assert_eq!(
            state.decrypt_key(&record.management_key_cipher).unwrap(),
            "management-secret"
        );
        assert_eq!(
            state.decrypt_key(&account.key_cipher).unwrap(),
            "inference-secret"
        );

        let delete = Bytes::from(
            serde_json::to_vec(&json!({
                "expectedRevision": state.settings_revision(),
                "processGeneration": state.process_generation()
            }))
            .unwrap(),
        );
        assert!(
            delete_integration(State(state.clone()), delete)
                .await
                .is_ok()
        );
        assert!(state.db.lock().cpa_integration().unwrap().is_none());
        assert!(
            state
                .db
                .lock()
                .get_account(CPA_ACCOUNT_ID)
                .unwrap()
                .is_none()
        );
        drop(state);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn stale_config_write_is_rejected_before_persistence() {
        let (dir, state) = test_state("cas");
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "expectedRevision": state.settings_revision().wrapping_sub(1),
                "processGeneration": state.process_generation(),
                "managementKey": "management-secret",
                "inferenceKey": "inference-secret"
            }))
            .unwrap(),
        );
        assert!(put_integration(State(state.clone()), body).await.is_err());
        assert!(state.db.lock().cpa_integration().unwrap().is_none());
        drop(state);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn queued_external_write_rechecks_cas_after_serialization() {
        let (dir, state) = test_state("queued-cas");
        let configure = Bytes::from(
            serde_json::to_vec(&json!({
                "expectedRevision": state.settings_revision(),
                "processGeneration": state.process_generation(),
                "managementKey": "management-secret",
                "inferenceKey": "inference-secret"
            }))
            .unwrap(),
        );
        assert!(
            put_integration(State(state.clone()), configure)
                .await
                .is_ok(),
            "CPA configuration should save"
        );

        let queued_revision = state.settings_revision();
        let operation = state.cpa_operations.lock().await;
        let request = Bytes::from(
            serde_json::to_vec(&json!({
                "expectedRevision": queued_revision,
                "processGeneration": state.process_generation(),
                "provider": "codex"
            }))
            .unwrap(),
        );
        let queued_state = state.clone();
        let queued = tokio::spawn(async move { start_oauth(State(queued_state), request).await });
        tokio::task::yield_now().await;
        state.bump_settings_revision();
        drop(operation);

        let error = queued
            .await
            .expect("queued handler should finish")
            .expect_err("stale queued write must be rejected before CPA network I/O");
        assert_eq!(error.status, StatusCode::CONFLICT);

        drop(state);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
