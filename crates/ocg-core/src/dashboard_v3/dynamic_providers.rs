//! Dynamic Provider control plane. Distinct from Custom API.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use chrono::Utc;
use ocg_domain::dynamic::{
    DynamicAuthKind, DynamicModelMapping, DynamicProviderDefinition, normalize_dynamic_mappings,
    normalize_dynamic_provider_name,
};
use ocg_domain::provider::{ProviderRegistry, builtin_provider};

use crate::custom;
use crate::custom::validate_custom_endpoint_url;
use crate::dynamic::{DynamicProviderRuntime, collides_with_known_id, validate_definition};
use crate::models::{
    Account as ModelAccount, AccountCustomConfig, AccountCustomConfigInput, AccountModelCapability,
    AccountType, normalize_account_notes,
};
use crate::redaction::redact_known_secret;
use crate::state::CoreState;

use super::types::{
    ControlRevision, DynamicProvider, DynamicProviderCreate, DynamicProviderDiscoverRequest,
    DynamicProviderDiscoverResponse, DynamicProviderModel, DynamicProviderMutation,
    DynamicProviderTestRequest, DynamicProviderTestResponse, DynamicProviderUpdate, MutationAck,
    MutationExpectation,
};
use super::{V3ApiError, check_expectation, parse_json, parse_mutation_json};

pub(super) async fn create_provider(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<DynamicProviderMutation>, V3ApiError> {
    let input = parse_mutation_json::<DynamicProviderCreate>(&body)?;
    create_locked(&state, input).map(Json)
}

pub(super) async fn get_provider(
    State(state): State<CoreState>,
    Path(provider_id): Path<String>,
) -> Result<Json<DynamicProvider>, V3ApiError> {
    let captured = ControlRevision::from_state(&state);
    let runtime = state
        .db
        .lock()
        .get_dynamic_provider(&provider_id)
        .map_err(V3ApiError::internal)?
        .ok_or_else(|| V3ApiError::not_found_at(&state, "provider not found"))?;
    Ok(Json(to_wire(
        runtime,
        captured.revision,
        captured.process_generation,
    )))
}

pub(super) async fn update_provider(
    State(state): State<CoreState>,
    Path(provider_id): Path<String>,
    body: Bytes,
) -> Result<Json<DynamicProviderMutation>, V3ApiError> {
    let input = parse_mutation_json::<DynamicProviderUpdate>(&body)?;
    update_locked(&state, &provider_id, input).map(Json)
}

pub(super) async fn delete_provider(
    State(state): State<CoreState>,
    Path(provider_id): Path<String>,
    body: Bytes,
) -> Result<Json<MutationAck>, V3ApiError> {
    let expectation = parse_mutation_json::<MutationExpectation>(&body)?;
    delete_locked(&state, &provider_id, &expectation).map(Json)
}

pub(super) async fn discover_models(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<DynamicProviderDiscoverResponse>, V3ApiError> {
    let input = parse_json::<DynamicProviderDiscoverRequest>(&body)?;
    let captured = ControlRevision::from_state(&state);
    let config = state.config();
    let endpoint = validate_custom_endpoint_url(&input.endpoint_url)
        .map_err(|error| V3ApiError::invalid_request_at(&state, error.to_string()))?;
    let auth_kind = DynamicAuthKind::from(input.auth_kind);
    let key = required_probe_key(&state, auth_kind, input.key.as_deref())?;
    let custom_config = AccountCustomConfigInput {
        endpoint_url: endpoint,
        upstream_protocol: input.upstream_protocol.into(),
    };
    let discovery =
        custom::discover_models_with_auth(&config, &custom_config, auth_kind.upstream_auth(), &key)
            .await
            .map_err(|failure| map_probe_failure(&state, &key, failure.message))?;
    Ok(Json(DynamicProviderDiscoverResponse {
        models: models_without_key(discovery.models, &key),
        truncated: discovery.truncated,
        revision: captured.revision,
        process_generation: captured.process_generation,
    }))
}

pub(super) async fn test_provider(
    State(state): State<CoreState>,
    body: Bytes,
) -> Result<Json<DynamicProviderTestResponse>, V3ApiError> {
    let input = parse_json::<DynamicProviderTestRequest>(&body)?;
    let captured = ControlRevision::from_state(&state);
    let config = state.config();
    let endpoint = validate_custom_endpoint_url(&input.endpoint_url)
        .map_err(|error| V3ApiError::invalid_request_at(&state, error.to_string()))?;
    let auth_kind = DynamicAuthKind::from(input.auth_kind);
    let key = required_probe_key(&state, auth_kind, input.key.as_deref())?;
    let public_model = ocg_domain::provider::validate_custom_model_id(&input.public_model)
        .map_err(|error| V3ApiError::invalid_request_at(&state, error.to_string()))?;
    let upstream_model = ocg_domain::provider::validate_custom_model_id(&input.upstream_model)
        .map_err(|error| V3ApiError::invalid_request_at(&state, error.to_string()))?;
    let custom_config = AccountCustomConfig {
        account_id: String::new(),
        endpoint_url: endpoint,
        upstream_protocol: input.upstream_protocol.into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let capability = AccountModelCapability {
        account_id: String::new(),
        public_model,
        protocol: input.upstream_protocol.into(),
        verified_at: None,
        source: "manual".into(),
        upstream_model,
    };
    let result = custom::probe_connection_with_auth(
        &config,
        &custom_config,
        &capability,
        auth_kind.upstream_auth(),
        &key,
    )
    .await;
    let (ok, error) = match result {
        Ok(()) => (true, None),
        Err(failure) => (false, Some(redact_known_secret(&failure.message, &key))),
    };
    Ok(Json(DynamicProviderTestResponse {
        ok,
        error,
        revision: captured.revision,
        process_generation: captured.process_generation,
    }))
}

fn create_locked(
    state: &CoreState,
    input: DynamicProviderCreate,
) -> Result<DynamicProviderMutation, V3ApiError> {
    let _settings_update = state.settings_update.lock();
    check_expectation(state, &input.expectation)?;
    let now = Utc::now();
    let auth_kind = DynamicAuthKind::from(input.auth_kind);
    let definition = validate_wire_definition(
        uuid::Uuid::new_v4().to_string(),
        input.name,
        input.endpoint_url,
        input.upstream_protocol,
        auth_kind,
        input.models,
    )
    .map_err(|error| V3ApiError::invalid_request_at(state, error.to_string()))?;
    let existing = state.dynamic_providers();
    if collides_with_known_id(&definition.id, &existing) {
        return Err(V3ApiError::conflict_at(
            state,
            "generated provider id collided; retry",
        ));
    }
    let key_cipher = first_account_key(state, auth_kind, input.key.as_deref())?;
    let account_name = input
        .account_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(definition.name.as_str())
        .to_string();
    let notes = match input.notes.as_deref() {
        Some(value) => normalize_account_notes(value)
            .map_err(|error| V3ApiError::invalid_request_at(state, error.to_string()))?,
        None => None,
    };
    let account = ModelAccount {
        id: uuid::Uuid::new_v4().to_string(),
        provider_id: definition.id.clone(),
        credential_kind: auth_kind.credential_kind(),
        quota_scope: auth_kind.quota_scope(),
        name: account_name,
        username: None,
        password_cipher: None,
        key_cipher,
        enabled: true,
        account_type: AccountType::Key,
        setup_step: crate::models::AccountSetupStep::Ready,
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
        notes,
        created_at: now,
        updated_at: now,
    };
    let runtime = runtime_from_definition(definition, now, now);
    {
        let db = state.db.lock();
        db.create_dynamic_provider(&runtime, &account)
            .map_err(|error| V3ApiError::invalid_request_at(state, error.to_string()))?;
        state
            .reload_dynamic_providers_locked(&db)
            .map_err(V3ApiError::internal)?;
    }
    let revision = state.bump_settings_revision();
    Ok(provider_mutation(state, runtime, revision))
}

fn update_locked(
    state: &CoreState,
    provider_id: &str,
    input: DynamicProviderUpdate,
) -> Result<DynamicProviderMutation, V3ApiError> {
    let _settings_update = state.settings_update.lock();
    check_expectation(state, &input.expectation)?;
    reject_builtin_id(state, provider_id)?;
    let existing = state
        .db
        .lock()
        .get_dynamic_provider(provider_id)
        .map_err(V3ApiError::internal)?
        .ok_or_else(|| V3ApiError::not_found_at(state, "provider not found"))?;
    let auth_kind = DynamicAuthKind::from(input.auth_kind);
    let definition = validate_wire_definition(
        existing.id.clone(),
        input.name,
        input.endpoint_url,
        input.upstream_protocol,
        auth_kind,
        input.models,
    )
    .map_err(|error| V3ApiError::invalid_request_at(state, error.to_string()))?;
    let account_count = state
        .db
        .lock()
        .count_accounts_for_provider(&existing.id)
        .map_err(V3ApiError::internal)?;
    if auth_kind.is_singleton() && account_count > 1 {
        return Err(V3ApiError::invalid_request_at(
            state,
            "no-auth provider requires a singleton account",
        ));
    }
    let changing_to_none = !existing.auth_kind.is_singleton() && auth_kind.is_singleton();
    let changing_from_none = existing.auth_kind.is_singleton() && !auth_kind.is_singleton();
    let replacement_key = if changing_from_none {
        Some(first_account_key(state, auth_kind, input.key.as_deref())?)
    } else if changing_to_none {
        None
    } else {
        input
            .key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|key| state.encrypt_key(key).map_err(V3ApiError::internal))
            .transpose()?
    };
    if changing_from_none && replacement_key.is_none() {
        return Err(V3ApiError::invalid_request_at(
            state,
            "changing from none-auth to keyed auth requires a replacement Key",
        ));
    }
    let now = Utc::now();
    let runtime = runtime_from_definition(definition, existing.created_at, now);
    let substantive = existing.endpoint_url != runtime.endpoint_url
        || existing.upstream_protocol != runtime.upstream_protocol
        || existing.auth_kind != runtime.auth_kind
        || existing.mappings != runtime.mappings;
    {
        let db = state.db.lock();
        db.replace_dynamic_provider(
            &runtime,
            substantive,
            changing_to_none,
            replacement_key.as_deref(),
        )
        .map_err(|error| V3ApiError::invalid_request_at(state, error.to_string()))?;
        state
            .reload_dynamic_providers_locked(&db)
            .map_err(V3ApiError::internal)?;
    }
    let revision = state.bump_settings_revision();
    Ok(provider_mutation(state, runtime, revision))
}

fn delete_locked(
    state: &CoreState,
    provider_id: &str,
    expectation: &MutationExpectation,
) -> Result<MutationAck, V3ApiError> {
    let _settings_update = state.settings_update.lock();
    check_expectation(state, expectation)?;
    reject_builtin_id(state, provider_id)?;
    {
        let db = state.db.lock();
        db.delete_dynamic_provider(provider_id)
            .map_err(|error| map_delete_error(state, error))?;
        state
            .reload_dynamic_providers_locked(&db)
            .map_err(V3ApiError::internal)?;
    }
    let revision = state.bump_settings_revision();
    Ok(MutationAck {
        revision,
        process_generation: state.process_generation(),
    })
}

fn reject_builtin_id(state: &CoreState, provider_id: &str) -> Result<(), V3ApiError> {
    if ProviderRegistry::get(provider_id).is_some() || builtin_provider(provider_id).is_some() {
        return Err(V3ApiError::invalid_request_at(
            state,
            "built-in providers cannot be deleted or replaced through this route",
        ));
    }
    Ok(())
}

fn validate_wire_definition(
    id: String,
    name: String,
    endpoint_url: String,
    protocol: super::types::AccountUpstreamProtocol,
    auth_kind: DynamicAuthKind,
    models: Vec<DynamicProviderModel>,
) -> Result<DynamicProviderDefinition, ocg_domain::provider::ProviderBindingError> {
    let endpoint_url = validate_custom_endpoint_url(&endpoint_url)?;
    let mappings = models
        .into_iter()
        .map(|model| DynamicModelMapping {
            public_model: model.public_model,
            upstream_model: model.upstream_model,
        })
        .collect::<Vec<_>>();
    validate_definition(DynamicProviderDefinition {
        id,
        name: normalize_dynamic_provider_name(&name)?,
        endpoint_url,
        upstream_protocol: protocol.into(),
        auth_kind,
        mappings: normalize_dynamic_mappings(&mappings)?,
    })
}

fn runtime_from_definition(
    definition: DynamicProviderDefinition,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
) -> DynamicProviderRuntime {
    DynamicProviderRuntime {
        id: definition.id,
        name: definition.name,
        endpoint_url: definition.endpoint_url,
        upstream_protocol: definition.upstream_protocol,
        auth_kind: definition.auth_kind,
        mappings: definition.mappings,
        created_at,
        updated_at,
    }
}

fn first_account_key(
    state: &CoreState,
    auth_kind: DynamicAuthKind,
    key: Option<&str>,
) -> Result<String, V3ApiError> {
    if !auth_kind.requires_key() {
        return Ok(String::new());
    }
    let trimmed = key.map(str::trim).unwrap_or("");
    if trimmed.is_empty() {
        return Err(V3ApiError::invalid_request_at(state, "key is required"));
    }
    state.encrypt_key(trimmed).map_err(V3ApiError::internal)
}

fn required_probe_key(
    state: &CoreState,
    auth_kind: DynamicAuthKind,
    key: Option<&str>,
) -> Result<String, V3ApiError> {
    if !auth_kind.requires_key() {
        return Ok(String::new());
    }
    let trimmed = key.map(str::trim).unwrap_or("");
    if trimmed.is_empty() {
        return Err(V3ApiError::invalid_request_at(state, "key is required"));
    }
    Ok(trimmed.to_string())
}

fn models_without_key(models: Vec<String>, key: &str) -> Vec<String> {
    if key.is_empty() {
        return models;
    }
    models
        .into_iter()
        .map(|model| redact_known_secret(&model, key))
        .collect()
}

fn map_probe_failure(state: &CoreState, key: &str, message: String) -> V3ApiError {
    V3ApiError::outbound_failed(state, redact_known_secret(&message, key))
}

fn map_delete_error(state: &CoreState, error: anyhow::Error) -> V3ApiError {
    let message = error.to_string();
    if message.contains("still has") {
        V3ApiError::conflict_at(state, message)
    } else if message.contains("unknown provider") {
        V3ApiError::not_found_at(state, message)
    } else {
        V3ApiError::invalid_request_at(state, message)
    }
}

fn provider_mutation(
    state: &CoreState,
    runtime: DynamicProviderRuntime,
    revision: u64,
) -> DynamicProviderMutation {
    DynamicProviderMutation {
        provider: to_wire(runtime, revision, state.process_generation()),
        revision,
        process_generation: state.process_generation(),
    }
}

fn to_wire(
    runtime: DynamicProviderRuntime,
    revision: u64,
    process_generation: u64,
) -> DynamicProvider {
    DynamicProvider {
        id: runtime.id,
        name: runtime.name,
        endpoint_url: runtime.endpoint_url,
        upstream_protocol: runtime.upstream_protocol.into(),
        auth_kind: runtime.auth_kind.into(),
        models: runtime
            .mappings
            .into_iter()
            .map(|mapping| DynamicProviderModel {
                public_model: mapping.public_model,
                upstream_model: mapping.upstream_model,
            })
            .collect(),
        created_at: runtime.created_at.to_rfc3339(),
        updated_at: runtime.updated_at.to_rfc3339(),
        revision,
        process_generation,
    }
}
