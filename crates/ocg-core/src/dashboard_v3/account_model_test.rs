//! Exact-account operational model test for the Accounts page.
//!
//! This intentionally differs from provider protocol probes: it never selects
//! a sibling account, writes protocol evidence, changes account state, or
//! requires the account to be enabled/available. It only verifies that the
//! requested account can currently serve one admitted model.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use std::time::Instant;

use crate::models::Account as ModelAccount;
use crate::provider::{
    ProviderAdapterKind, UpstreamProtocolKind, builtin_provider, plan_requires_custom_config,
};
use crate::provider_contracts::ContractScope;
use crate::state::CoreState;

use super::accounts::load_model_account;
use super::types::{AccountModelTestRequest, AccountModelTestResponse, AccountUpstreamProtocol};
use super::{V3ApiError, parse_json};

pub(super) async fn test_account_model(
    State(state): State<CoreState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<AccountModelTestResponse>, V3ApiError> {
    let input = parse_json::<AccountModelTestRequest>(&body)?;
    let prepared = prepare_account_model_test(&state, &id, input)?;
    let started = Instant::now();
    let (success, http_status, error) = match crate::protocol_probe::execute_account_model_test(
        &state,
        &prepared.config,
        &prepared.account,
        prepared.adapter,
        &prepared.upstream_model,
        prepared.protocol,
        prepared.custom_endpoint_url.as_deref(),
    )
    .await
    {
        Ok(status) => (true, Some(status), None),
        Err((status, message)) => (false, status, Some(message)),
    };
    Ok(Json(AccountModelTestResponse {
        account_id: prepared.account.id,
        model_id: prepared.public_model,
        protocol: AccountUpstreamProtocol::from(prepared.protocol),
        success,
        http_status,
        duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        error,
    }))
}

struct PreparedAccountModelTest {
    account: ModelAccount,
    config: crate::models::AppConfig,
    adapter: ProviderAdapterKind,
    public_model: String,
    upstream_model: String,
    protocol: UpstreamProtocolKind,
    custom_endpoint_url: Option<String>,
}

fn prepare_account_model_test(
    state: &CoreState,
    id: &str,
    input: AccountModelTestRequest,
) -> Result<PreparedAccountModelTest, V3ApiError> {
    let account = load_model_account(state, id)?;
    if !account.setup_step.is_ready() {
        return Err(V3ApiError::precondition_failed_at(
            state,
            "finish account setup before testing a model",
        ));
    }
    let model_id = input.model_id.trim();
    if model_id.is_empty() {
        return Err(V3ApiError::invalid_request_at(state, "modelId is required"));
    }
    let plan = builtin_provider(&account.provider_id)
        .ok_or_else(|| V3ApiError::invalid_request_at(state, "unknown provider offering"))?;
    let adapter = ProviderAdapterKind::from_provider_id(&account.provider_id)
        .ok_or_else(|| V3ApiError::invalid_request_at(state, "unknown provider offering"))?;

    let (protocol, custom_endpoint_url, upstream_model) = if plan_requires_custom_config(plan) {
        let contract = state
            .db
            .lock()
            .load_account_contract(&account.id)
            .map_err(V3ApiError::internal)?;
        let config = contract.custom_config.ok_or_else(|| {
            V3ApiError::invalid_request_at(
                state,
                "Custom API accounts require a persisted endpoint URL and upstream protocol",
            )
        })?;
        let capability = contract
            .model_capabilities
            .iter()
            .find(|capability| {
                crate::custom::custom_model_id_matches(&capability.public_model, model_id)
            })
            .ok_or_else(|| {
                V3ApiError::invalid_request_at(state, "model is not declared for this account")
            })?;
        if capability.protocol != config.upstream_protocol {
            return Err(V3ApiError::invalid_request_at(
                state,
                "Custom API model capability protocol does not match this account",
            ));
        }
        (
            capability.protocol,
            Some(config.endpoint_url),
            capability.upstream_model.clone(),
        )
    } else {
        let scope = ContractScope::from_account(&account)
            .ok_or_else(|| V3ApiError::invalid_request_at(state, "unknown provider offering"))?;
        let contracts = state.provider_contracts();
        let protocol = contracts
            .scope(&scope)
            .and_then(|scope| scope.model(model_id))
            .filter(|model| model.routable)
            .ok_or_else(|| {
                V3ApiError::invalid_request_at(state, "model is not routable for this provider")
            })?;
        (protocol.preferred_protocol, None, model_id.to_string())
    };

    Ok(PreparedAccountModelTest {
        account,
        config: state.config(),
        adapter,
        public_model: model_id.to_string(),
        upstream_model,
        protocol,
        custom_endpoint_url,
    })
}
