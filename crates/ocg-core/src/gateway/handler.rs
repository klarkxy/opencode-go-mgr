use crate::alias;
use crate::gateway::diagnostics::{
    ErrorDiagnostic, REQUEST_ID_HEADER, RequestTrace, emit_failure, log_request_failure,
    serialize_diagnostic,
};
use crate::gateway::executor::GatewayExecutor;
use crate::gateway::forwarder::UpstreamPayloadTooLargeResponse;
use crate::gateway::materialize::protocol_error_from_resolve;
use crate::gateway::protocol::{ProtocolError, parse_client_request, parse_gemini_request};
use crate::gateway::response::{
    local_protocol_failure, protocol_error_from, protocol_error_response,
};
use crate::kernel::protocol::ApiFormat;
use crate::models::{
    CLAUDE_DESKTOP_HAIKU_ALIAS, CLAUDE_DESKTOP_OPUS_ALIAS, CLAUDE_DESKTOP_SONNET_ALIAS,
};
use crate::state::CoreState;
use axum::body::{Body, Bytes};
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub async fn request_trace_middleware(
    State(state): State<CoreState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let credential = extract_client_key(request.headers(), &state);
    let trace = credential
        .as_ref()
        .map(|entry| RequestTrace::new().with_client_key(entry.id.clone(), entry.name.clone()))
        .unwrap_or_default();
    let path = request.uri().path().to_string();
    let client_body_bytes = request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    let authenticated = credential.is_some();
    request.extensions_mut().insert(trace.clone());
    let mut response = next.run(request).await;

    if response.status() == StatusCode::PAYLOAD_TOO_LARGE
        && authenticated
        && response
            .extensions()
            .get::<UpstreamPayloadTooLargeResponse>()
            .is_none()
    {
        let mut diagnostic = ErrorDiagnostic::new(
            &trace,
            1,
            "client",
            "body_limit",
            client_format_for_path(&path),
        );
        diagnostic.client_body_bytes = client_body_bytes;
        diagnostic.downstream_status = Some(StatusCode::PAYLOAD_TOO_LARGE.as_u16());
        let encoded = serialize_diagnostic(diagnostic.clone());
        log_request_failure(
            &state.db.lock(),
            &trace,
            &diagnostic,
            &encoded,
            "gateway request body exceeded the configured limit",
        );
        emit_failure(&encoded);
    }

    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&trace.request_id)
            .expect("generated request id must be a valid header value"),
    );
    response
}

fn client_format_for_path(path: &str) -> ApiFormat {
    if path.ends_with("/responses") {
        ApiFormat::Responses
    } else if path.ends_with("/messages") {
        ApiFormat::Messages
    } else if path.starts_with("/v1beta/models/")
        || (path.starts_with("/v1/models/") && path.contains(':'))
    {
        ApiFormat::Gemini
    } else {
        ApiFormat::ChatCompletions
    }
}

pub async fn chat_completions(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, trace, headers, body, ApiFormat::ChatCompletions).await
}

pub async fn responses(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, trace, headers, body, ApiFormat::Responses).await
}

pub async fn messages(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, trace, headers, body, ApiFormat::Messages).await
}

pub async fn claude_desktop_messages(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler_inner(state, trace, headers, body, ApiFormat::Messages, true).await
}

pub async fn claude_desktop_models(
    State(state): State<CoreState>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !check_auth(&headers, &state) {
        return protocol_error_response(
            ApiFormat::Messages,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }

    axum::Json(serde_json::json!({
        "data": [
            {
                "type": "model",
                "id": CLAUDE_DESKTOP_SONNET_ALIAS,
                "display_name": "Claude Sonnet 4.6",
                "created_at": "2026-02-17T00:00:00Z"
            },
            {
                "type": "model",
                "id": CLAUDE_DESKTOP_OPUS_ALIAS,
                "display_name": "Claude Opus 4.6",
                "created_at": "2026-02-05T00:00:00Z"
            },
            {
                "type": "model",
                "id": CLAUDE_DESKTOP_HAIKU_ALIAS,
                "display_name": "Claude Haiku 4.5",
                "created_at": "2025-10-01T00:00:00Z"
            }
        ],
        "has_more": false,
        "first_id": CLAUDE_DESKTOP_SONNET_ALIAS,
        "last_id": CLAUDE_DESKTOP_HAIKU_ALIAS
    }))
    .into_response()
}

pub async fn gemini_model_action(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    Path(model_action): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let client_body_bytes = body.len();
    let Some((model, action)) = model_action.rsplit_once(':') else {
        return gemini_error(
            &state,
            &trace,
            &headers,
            StatusCode::NOT_FOUND,
            "Gemini model action is required",
            Some(client_body_bytes),
        );
    };
    if model.is_empty() {
        return gemini_error(
            &state,
            &trace,
            &headers,
            StatusCode::BAD_REQUEST,
            "Gemini model is required",
            Some(client_body_bytes),
        );
    }
    match action {
        "generateContent" => {
            gemini_proxy_handler(state, trace, headers, body, model.to_string(), false).await
        }
        "streamGenerateContent" => {
            gemini_proxy_handler(state, trace, headers, body, model.to_string(), true).await
        }
        "countTokens" => gemini_expected_fallback(
            &state,
            &headers,
            StatusCode::NOT_IMPLEMENTED,
            "Gemini countTokens is not available; Gemini CLI falls back to local estimation",
        ),
        "embedContent" => gemini_error(
            &state,
            &trace,
            &headers,
            StatusCode::NOT_IMPLEMENTED,
            "Gemini embeddings are not supported by this gateway",
            Some(client_body_bytes),
        ),
        _ => gemini_error(
            &state,
            &trace,
            &headers,
            StatusCode::NOT_FOUND,
            "unknown Gemini model action",
            Some(client_body_bytes),
        ),
    }
}

/// GET /v1/models — authenticated local Alias registry list.
///
/// Returns OpenAI list JSON for routeable code-owned aliases, then eligible
/// Custom capability IDs, de-duplicated and in deterministic order. Refreshed
/// built-in catalogs can activate sealed names or add an exact raw pin, but
/// cannot create arbitrary aliases. It never calls upstream.
pub async fn models(
    State(state): State<CoreState>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !check_auth(&headers, &state) {
        return protocol_error_response(
            ApiFormat::ChatCompletions,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }
    published_alias_models_response(&state)
}

fn published_alias_models_response(state: &CoreState) -> axum::response::Response {
    let zen_catalog = state.zen_free_model_catalog();
    let contracts = state.provider_contracts();
    let go_ids = provider_catalog_model_ids(&contracts, crate::provider::OPENCODE_PROVIDER_ID);
    let goat_ids =
        provider_catalog_model_ids(&contracts, crate::provider::COMMAND_CODE_PROVIDER_ID);
    let minimax_ids = provider_catalog_model_ids(&contracts, crate::provider::MINIMAX_PROVIDER_ID);
    let kimi_ids = provider_catalog_model_ids(&contracts, crate::provider::KIMI_PROVIDER_ID);
    let cpa_ids = active_cpa_model_ids(state);
    let custom_ids = eligible_custom_public_models(state, &contracts);
    let dynamics = state.dynamic_providers();
    let extra: Vec<_> = dynamics
        .iter()
        .map(crate::dynamic::DynamicProviderRuntime::alias_catalog)
        .collect();
    let catalogs = crate::alias::RuntimeCatalogs {
        go: &go_ids,
        zen_free: &zen_catalog.models,
        custom: &custom_ids,
        command_code: &goat_ids,
        minimax: &minimax_ids,
        kimi: &kimi_ids,
        cpa: &cpa_ids,
        extra: &extra,
    };
    let published = crate::alias::published_routeable_aliases_with_runtime_catalogs(catalogs);
    let mut data: Vec<serde_json::Value> = published
        .iter()
        .filter(|item| published_alias_has_enabled_protocol(item, catalogs, &contracts, &dynamics))
        .map(|item| {
            serde_json::json!({
                "id": item.alias,
                "object": "model",
                "created": 0,
                "owned_by": item.owned_by
            })
        })
        .collect();
    for id in &custom_ids {
        let routeable_custom_alias = matches!(
            crate::alias::resolve_with_runtime_catalogs(id, catalogs),
            Ok(crate::alias::ResolvedModel::Alias { mappings, .. })
                if mappings.iter().any(|mapping| mapping.is_custom_api() && mapping.routeable)
        );
        if !routeable_custom_alias {
            continue;
        }
        if data.iter().any(|item| {
            item.get("id")
                .and_then(|value| value.as_str())
                .is_some_and(|existing| crate::custom::custom_model_id_matches(existing, id))
        }) {
            continue;
        }
        data.push(serde_json::json!({
            "id": id,
            "object": "model",
            "created": 0,
            "owned_by": crate::provider::CUSTOM_PROVIDER_ID
        }));
    }
    for id in cpa_ids.iter() {
        let exact_cpa_raw = matches!(
            crate::alias::resolve_with_runtime_catalogs(id, catalogs),
            Ok(crate::alias::ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.provider_id == crate::provider::CPA_PROVIDER_ID && mapping.routeable
        );
        if exact_cpa_raw
            && !data
                .iter()
                .any(|item| item.get("id").and_then(|value| value.as_str()) == Some(id))
        {
            data.push(serde_json::json!({
                "id": id,
                "object": "model",
                "created": 0,
                "owned_by": crate::provider::CPA_PROVIDER_ID
            }));
        }
    }
    axum::Json(serde_json::json!({
        "object": "list",
        "data": data
    }))
    .into_response()
}

fn published_alias_has_enabled_protocol(
    item: &alias::PublishedAlias,
    catalogs: alias::RuntimeCatalogs<'_>,
    contracts: &crate::provider_contracts::EffectiveContractSet,
    dynamics: &[crate::dynamic::DynamicProviderRuntime],
) -> bool {
    model_has_enabled_protocol(
        &item.alias,
        alias::RuntimeCatalogs {
            custom: &[],
            ..catalogs
        },
        contracts,
        dynamics,
    )
}

fn model_has_enabled_protocol(
    model: &str,
    catalogs: alias::RuntimeCatalogs<'_>,
    contracts: &crate::provider_contracts::EffectiveContractSet,
    dynamics: &[crate::dynamic::DynamicProviderRuntime],
) -> bool {
    match crate::alias::resolve_with_runtime_catalogs(model, catalogs) {
        Ok(alias::ResolvedModel::Alias { mappings, .. }) => mappings.iter().any(|mapping| {
            mapping.routeable
                && (mapping.provider_id == crate::provider::CPA_PROVIDER_ID
                    || crate::dynamic::find_runtime(dynamics, &mapping.provider_id).is_some()
                    || contracts.mapping_has_enabled_protocol(mapping))
        }),
        Ok(alias::ResolvedModel::PinnedRaw { mapping, .. }) => {
            mapping.routeable
                && (mapping.provider_id == crate::provider::CPA_PROVIDER_ID
                    || crate::dynamic::find_runtime(dynamics, &mapping.provider_id).is_some()
                    || contracts.mapping_has_enabled_protocol(&mapping))
        }
        Err(_) => false,
    }
}

fn provider_catalog_model_ids(
    contracts: &crate::provider_contracts::EffectiveContractSet,
    provider_id: &str,
) -> Vec<String> {
    contracts
        .provider_offering(provider_id)
        .filter(|scope| {
            provider_id != crate::provider::OPENCODE_PROVIDER_ID
                || scope.catalog.source == crate::provider_contracts::CATALOG_SOURCE_OPENCODE_MODELS
        })
        .map(|scope| scope.catalog.models.clone())
        .unwrap_or_default()
}

fn eligible_custom_public_models(
    state: &CoreState,
    contracts: &crate::provider_contracts::EffectiveContractSet,
) -> Vec<String> {
    let Ok(runtimes) = state.db.lock().list_custom_account_runtimes() else {
        return Vec::new();
    };
    crate::custom::eligible_custom_public_models(&runtimes)
        .into_iter()
        .filter(|id| {
            runtimes.iter().any(|runtime| {
                runtime.eligible()
                    && runtime.capability_matching_public(id).is_some()
                    && contracts
                        .scope(&crate::provider_contracts::ContractScope::custom_endpoint(
                            &runtime.account_id,
                        ))
                        .is_some_and(|contract| contract.model_has_enabled_protocol(id))
            })
        })
        .collect()
}

/// A disabled, cooling, auth-failed, or disconnected CPA must not inject raw
/// identities before the ordinary selector can fall back to existing routes.
fn active_cpa_model_ids(state: &CoreState) -> std::sync::Arc<Vec<String>> {
    let active = {
        let db = state.db.lock();
        db.cpa_integration().ok().flatten().is_some_and(|_| {
            db.get_account(crate::provider::CPA_ACCOUNT_ID)
                .ok()
                .flatten()
                .is_some_and(|account| {
                    crate::routing_runtime::account_is_available_for(
                        &account,
                        crate::models::UpstreamChannel::Go,
                        &[],
                    )
                })
        })
    };
    if active {
        state.cpa_model_catalog()
    } else {
        std::sync::Arc::new(Vec::new())
    }
}

async fn proxy_handler(
    state: CoreState,
    trace: RequestTrace,
    headers: HeaderMap,
    body: Bytes,
    client_format: ApiFormat,
) -> axum::response::Response {
    proxy_handler_inner(state, trace, headers, body, client_format, false).await
}

async fn proxy_handler_inner(
    state: CoreState,
    trace: RequestTrace,
    headers: HeaderMap,
    body: Bytes,
    client_format: ApiFormat,
    claude_desktop: bool,
) -> axum::response::Response {
    let config = state.config();
    let client_body_bytes = body.len();

    let Some(client_key_id) = extract_client_key_id(&headers, &state) else {
        return protocol_error_response(
            client_format,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    };

    let client_body = body.clone();
    let parsed = match parse_client_request(client_format, body) {
        Ok(parsed) => parsed,
        Err(error) => {
            return local_protocol_failure(
                &state,
                &trace,
                client_format,
                error,
                Some(client_body_bytes),
                Some(&client_body),
            );
        }
    };
    let client_model = parsed.requested_model.clone();
    let routing_model = if claude_desktop {
        match config
            .claude_desktop_models
            .model_for_alias(&parsed.requested_model)
        {
            Some(model) => model.to_string(),
            None => {
                return local_protocol_failure(
                    &state,
                    &trace,
                    ApiFormat::Messages,
                    ProtocolError::new(format!(
                        "unsupported Claude Desktop model alias `{}`",
                        parsed.requested_model
                    )),
                    Some(client_body_bytes),
                    Some(&client_body),
                );
            }
        }
    } else {
        parsed.requested_model.clone()
    };
    let contracts = state.provider_contracts();
    let go_model_ids =
        provider_catalog_model_ids(&contracts, crate::provider::OPENCODE_PROVIDER_ID);
    let custom_model_ids = eligible_custom_public_models(&state, &contracts);
    let goat_model_ids =
        provider_catalog_model_ids(&contracts, crate::provider::COMMAND_CODE_PROVIDER_ID);
    let minimax_model_ids =
        provider_catalog_model_ids(&contracts, crate::provider::MINIMAX_PROVIDER_ID);
    let kimi_model_ids = provider_catalog_model_ids(&contracts, crate::provider::KIMI_PROVIDER_ID);
    let cpa_model_ids = active_cpa_model_ids(&state);
    let zen_catalog = state.zen_free_model_catalog();
    let dynamics = state.dynamic_providers();
    let extra: Vec<_> = dynamics
        .iter()
        .map(crate::dynamic::DynamicProviderRuntime::alias_catalog)
        .collect();
    let catalogs = crate::alias::RuntimeCatalogs {
        go: &go_model_ids,
        zen_free: &zen_catalog.models,
        custom: &custom_model_ids,
        command_code: &goat_model_ids,
        minimax: &minimax_model_ids,
        kimi: &kimi_model_ids,
        cpa: &cpa_model_ids,
        extra: &extra,
    };
    let resolved = match crate::alias::resolve_with_runtime_catalogs(&routing_model, catalogs) {
        Ok(resolved) => resolved,
        Err(error) => {
            return local_protocol_failure(
                &state,
                &trace,
                client_format,
                protocol_error_from_resolve(error),
                Some(client_body_bytes),
                Some(&client_body),
            );
        }
    };

    GatewayExecutor::run(
        state,
        trace,
        client_body,
        headers,
        client_format,
        parsed,
        resolved,
        client_model,
        routing_model,
        config,
        Some(client_key_id),
        contracts,
        dynamics,
    )
    .await
}

#[cfg(test)]
fn rewrite_claude_desktop_model(
    body: &Bytes,
    models: &crate::models::ClaudeDesktopModels,
) -> Result<Bytes, ProtocolError> {
    let mut request: serde_json::Value = serde_json::from_slice(body)
        .map_err(|error| ProtocolError::new(format!("invalid JSON request: {error}")))?;
    let object = request
        .as_object_mut()
        .ok_or_else(|| ProtocolError::new("request must be a JSON object"))?;
    let alias = object
        .get("model")
        .and_then(serde_json::Value::as_str)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| ProtocolError::new("request model is required"))?;
    let model = models
        .model_for_alias(alias)
        .ok_or_else(|| {
            ProtocolError::new(format!("unsupported Claude Desktop model alias `{alias}`"))
        })?
        .to_string();
    object.insert("model".to_string(), serde_json::Value::String(model));
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(|error| ProtocolError::new(format!("failed to encode request: {error}")))
}

async fn gemini_proxy_handler(
    state: CoreState,
    trace: RequestTrace,
    headers: HeaderMap,
    body: Bytes,
    model: String,
    stream: bool,
) -> axum::response::Response {
    let config = state.config();
    let client_body_bytes = body.len();
    let Some(client_key_id) = extract_client_key_id(&headers, &state) else {
        return protocol_error_response(
            ApiFormat::Gemini,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    };
    let parsed = match parse_gemini_request(model, stream, body.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return local_protocol_failure(
                &state,
                &trace,
                ApiFormat::Gemini,
                error,
                Some(client_body_bytes),
                Some(&body),
            );
        }
    };
    let client_model = parsed.requested_model.clone();
    let routing_model = parsed.requested_model.clone();
    let contracts = state.provider_contracts();
    let go_model_ids =
        provider_catalog_model_ids(&contracts, crate::provider::OPENCODE_PROVIDER_ID);
    let custom_model_ids = eligible_custom_public_models(&state, &contracts);
    let goat_model_ids =
        provider_catalog_model_ids(&contracts, crate::provider::COMMAND_CODE_PROVIDER_ID);
    let minimax_model_ids =
        provider_catalog_model_ids(&contracts, crate::provider::MINIMAX_PROVIDER_ID);
    let kimi_model_ids = provider_catalog_model_ids(&contracts, crate::provider::KIMI_PROVIDER_ID);
    let cpa_model_ids = active_cpa_model_ids(&state);
    let zen_catalog = state.zen_free_model_catalog();
    let dynamics = state.dynamic_providers();
    let extra: Vec<_> = dynamics
        .iter()
        .map(crate::dynamic::DynamicProviderRuntime::alias_catalog)
        .collect();
    let catalogs = crate::alias::RuntimeCatalogs {
        go: &go_model_ids,
        zen_free: &zen_catalog.models,
        custom: &custom_model_ids,
        command_code: &goat_model_ids,
        minimax: &minimax_model_ids,
        kimi: &kimi_model_ids,
        cpa: &cpa_model_ids,
        extra: &extra,
    };
    let resolved = match crate::alias::resolve_with_runtime_catalogs(&routing_model, catalogs) {
        Ok(resolved) => resolved,
        Err(error) => {
            return local_protocol_failure(
                &state,
                &trace,
                ApiFormat::Gemini,
                protocol_error_from_resolve(error),
                Some(client_body_bytes),
                Some(&body),
            );
        }
    };
    GatewayExecutor::run(
        state,
        trace,
        body,
        headers,
        ApiFormat::Gemini,
        parsed,
        resolved,
        client_model,
        routing_model,
        config,
        Some(client_key_id),
        contracts,
        dynamics,
    )
    .await
}

/// Candidate credential values a client may present, in fixed priority
/// order: the Bearer token, then `x-api-key`, then `x-goog-api-key`. Every
/// non-empty candidate is an independent credential claim; a wrong value
/// alongside a correct one never downgrades the request.
fn candidate_key_values(headers: &HeaderMap) -> Vec<&str> {
    let mut candidates = Vec::with_capacity(3);
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|auth| auth.trim().strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = bearer {
        candidates.push(value);
    }
    for name in ["x-api-key", "x-goog-api-key"] {
        let value = headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(value) = value {
            candidates.push(value);
        }
    }
    candidates
}

/// Extracts the id of the credential that authenticates this request.
/// Authentication succeeds when ANY non-empty candidate header matches ANY
/// currently valid credential (the primary key value or an enabled,
/// non-deleted sub key) in the credential snapshot; the first candidate hit,
/// in header order, attributes the request (the primary key resolves to the
/// fixed `PRIMARY_KEY_ID`).
pub(crate) fn extract_client_key_id(headers: &HeaderMap, state: &CoreState) -> Option<String> {
    extract_client_key(headers, state).map(|entry| entry.id)
}

fn extract_client_key(
    headers: &HeaderMap,
    state: &CoreState,
) -> Option<crate::gateway_keys::CredentialEntry> {
    candidate_key_values(headers)
        .into_iter()
        .find_map(|value| state.credential_entry_for_value(value))
}

fn check_auth(headers: &HeaderMap, state: &CoreState) -> bool {
    extract_client_key_id(headers, state).is_some()
}

fn gemini_error(
    state: &CoreState,
    trace: &RequestTrace,
    headers: &HeaderMap,
    status: StatusCode,
    message: &str,
    client_body_bytes: Option<usize>,
) -> axum::response::Response {
    if !check_auth(headers, state) {
        return protocol_error_response(
            ApiFormat::Gemini,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }
    local_failure_response(
        state,
        trace,
        ApiFormat::Gemini,
        status,
        message,
        "client",
        "validation",
        client_body_bytes,
        None,
    )
}

fn gemini_expected_fallback(
    state: &CoreState,
    headers: &HeaderMap,
    status: StatusCode,
    message: &str,
) -> axum::response::Response {
    if !check_auth(headers, state) {
        return protocol_error_response(
            ApiFormat::Gemini,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }
    protocol_error_response(ApiFormat::Gemini, status, message, None)
}

#[allow(clippy::too_many_arguments)]
fn local_failure_response(
    state: &CoreState,
    trace: &RequestTrace,
    format: ApiFormat,
    status: StatusCode,
    message: &str,
    error_source: &str,
    error_stage: &str,
    client_body_bytes: Option<usize>,
    summary_body: Option<&[u8]>,
) -> axum::response::Response {
    let mut diagnostic = ErrorDiagnostic::new(trace, 1, error_source, error_stage, format);
    diagnostic.client_body_bytes = client_body_bytes;
    diagnostic.downstream_status = Some(status.as_u16());
    if let Some(body) = summary_body {
        diagnostic = diagnostic.with_request_summary(body);
    }
    let encoded = serialize_diagnostic(diagnostic.clone());
    log_request_failure(&state.db.lock(), trace, &diagnostic, &encoded, message);
    emit_failure(&encoded);
    protocol_error_from(
        format,
        ProtocolError::with_status(status, message.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        active_cpa_model_ids, check_auth, extract_client_key_id, rewrite_claude_desktop_model,
    };
    use crate::gateway::materialize::resolved_alias_from_model;
    use crate::gateway::protocol::{
        ApiFormat, MaterializeSpec, materialize_parsed_request, parse_client_request,
        prepare_request,
    };
    use crate::gateway_keys::{CredentialEntry, CredentialSnapshot, PRIMARY_KEY_ID};
    use crate::models::{AppConfig, CLAUDE_DESKTOP_OPUS_ALIAS, ClaudeDesktopModels};
    use crate::state::{CoreState, CoreStateInner};
    use axum::body::Bytes;
    use axum::http::{HeaderMap, HeaderValue};
    use serde_json::json;
    use std::collections::HashMap;

    /// Owns the temp data dir and releases the SQLite connection (and thus
    /// the open database file) before removing the directory on Windows.
    struct StateDir {
        state: Option<CoreState>,
        dir: Option<std::path::PathBuf>,
    }

    impl std::ops::Deref for StateDir {
        type Target = CoreState;
        fn deref(&self) -> &CoreState {
            self.state.as_ref().expect("state present during use")
        }
    }

    impl Drop for StateDir {
        fn drop(&mut self) {
            self.state.take();
            if let Some(dir) = self.dir.take() {
                std::fs::remove_dir_all(dir).ok();
            }
        }
    }

    fn state_with_snapshot() -> StateDir {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        dir.push(format!("ocg-auth-matrix-{nanos}"));
        std::fs::create_dir_all(&dir).expect("test data directory should be created");
        let db = crate::db::Database::open(dir.clone()).expect("test database should open");
        let cipher: std::sync::Arc<dyn crate::crypto::KeyCipher + Send + Sync> =
            std::sync::Arc::new(crate::crypto::StaticKeyCipher::new("test"));
        let state = CoreStateInner::new(db, dir.clone(), cipher).expect("state should initialize");
        StateDir {
            state: Some(std::sync::Arc::new(state)),
            dir: Some(dir),
        }
    }

    fn entry(id: &str, name: &str) -> CredentialEntry {
        CredentialEntry {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    fn snapshot() -> CredentialSnapshot {
        HashMap::from([
            ("ocg-primary".to_string(), entry(PRIMARY_KEY_ID, "Primary")),
            ("ocg-laptop".to_string(), entry("laptop", "Laptop")),
        ])
    }

    #[test]
    fn auth_matrix_across_headers_credentials_and_states() {
        let state = state_with_snapshot();
        *state.credential_snapshot.write() = snapshot();
        let cases = [
            // (header name, presented value, expected key id)
            ("authorization", "Bearer ocg-primary", PRIMARY_KEY_ID),
            ("authorization", "Bearer ocg-laptop", "laptop"),
            ("x-api-key", "ocg-laptop", "laptop"),
            ("x-goog-api-key", "ocg-primary", PRIMARY_KEY_ID),
            ("authorization", "Bearer wrong-key", ""),
            ("authorization", "Bearer ", ""),
            ("x-api-key", "", ""),
            ("x-goog-api-key", "   ", ""),
        ];
        for (header, presented, expected) in cases {
            let mut headers = HeaderMap::new();
            headers.insert(
                axum::http::HeaderName::from_static(header),
                HeaderValue::from_str(presented).expect("test header value should be valid"),
            );
            let matched = extract_client_key_id(&headers, &state);
            if expected.is_empty() {
                assert!(
                    matched.is_none(),
                    "{header}: {presented} should not authenticate"
                );
            } else {
                assert_eq!(
                    matched.as_deref(),
                    Some(expected),
                    "{header}: {presented} should match {expected}"
                );
            }
        }

        let no_headers = HeaderMap::new();
        assert!(extract_client_key_id(&no_headers, &state).is_none());
    }

    #[test]
    fn wrong_x_api_key_alongside_correct_x_goog_api_key_passes() {
        let state = state_with_snapshot();
        *state.credential_snapshot.write() = snapshot();
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("wrong-key"));
        headers.insert("x-goog-api-key", HeaderValue::from_static("ocg-laptop"));
        assert!(check_auth(&headers, &state));
        assert_eq!(
            extract_client_key_id(&headers, &state).as_deref(),
            Some("laptop")
        );

        // Bearer wins attribution when several candidates hit: it comes first
        // in the fixed candidate order.
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer ocg-primary"),
        );
        assert_eq!(
            extract_client_key_id(&headers, &state).as_deref(),
            Some(PRIMARY_KEY_ID)
        );

        // Two wrong candidates still fail.
        let mut wrong = HeaderMap::new();
        wrong.insert("x-api-key", HeaderValue::from_static("wrong-key"));
        wrong.insert("x-goog-api-key", HeaderValue::from_static("also-wrong"));
        assert!(!check_auth(&wrong, &state));
    }

    #[test]
    fn bearer_without_prefix_falls_back_to_api_key_headers() {
        let state = state_with_snapshot();
        *state.credential_snapshot.write() = snapshot();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("ocg-primary"));
        assert!(extract_client_key_id(&headers, &state).is_none());
        headers.insert("x-api-key", HeaderValue::from_static("ocg-laptop"));
        assert_eq!(
            extract_client_key_id(&headers, &state).as_deref(),
            Some("laptop")
        );
    }

    #[test]
    fn disabled_and_deleted_sub_keys_leave_the_snapshot() {
        // The snapshot only ever contains the primary value and enabled
        // non-deleted sub keys; disabling or soft-deleting removes the entry
        // (covered end to end by the key lifecycle integration tests).
        let state = state_with_snapshot();
        let mut snapshot = snapshot();
        assert!(snapshot.remove("ocg-laptop").is_some());
        *state.credential_snapshot.write() = snapshot;
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("ocg-laptop"));
        assert!(!check_auth(&headers, &state));
    }

    #[test]
    fn claude_desktop_alias_is_rewritten_before_messages_preparation() {
        let models = ClaudeDesktopModels {
            sonnet: "glm-5.2".to_string(),
            opus: String::new(),
            haiku: String::new(),
        };
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": CLAUDE_DESKTOP_OPUS_ALIAS,
                "max_tokens": 1,
                "messages": [{"role":"user","content":"hi"}]
            }))
            .expect("test request should serialize"),
        );

        let rewritten =
            rewrite_claude_desktop_model(&body, &models).expect("known alias should be rewritten");
        let plan = prepare_request(ApiFormat::Messages, rewritten)
            .expect("rewritten request should use the existing preparation path");

        assert_eq!(plan.model, "glm-5.2");
        // The current protocol snapshot exposes glm-5.2 through Chat only, so
        // the Claude Desktop Messages request is converted after alias rewrite.
        assert_eq!(plan.upstream, ApiFormat::ChatCompletions);

        let parsed = parse_client_request(ApiFormat::Messages, body).expect("parse once");
        assert_eq!(parsed.requested_model, CLAUDE_DESKTOP_OPUS_ALIAS);
        let mapped = models
            .model_for_alias(&parsed.requested_model)
            .expect("opus inherits sonnet");
        let resolved = crate::alias::resolve(mapped).expect("mapped Go alias");
        assert!(matches!(
            resolved,
            crate::alias::ResolvedModel::Alias { ref alias, .. } if alias == "glm-5.2"
        ));
        let plan = materialize_parsed_request(
            &parsed,
            &MaterializeSpec {
                client_model: parsed.requested_model.clone(),
                upstream_model: mapped.to_string(),
                resolved_alias: resolved_alias_from_model(&resolved),
                channel: crate::models::UpstreamChannel::Go,
                upstream_base_override: None,
                original_model: None,
                allow_go_fallback: false,
                forced_upstream: None,
                custom_route: None,
            },
        )
        .expect("Claude Desktop keeps the original alias as client_model");
        assert_eq!(plan.model, "glm-5.2");
        assert_eq!(plan.client_model, CLAUDE_DESKTOP_OPUS_ALIAS);
        assert_eq!(
            crate::gateway::materialize::native_log_identity(&plan)
                .resolved_alias
                .as_deref(),
            Some("glm-5.2")
        );
        assert_eq!(plan.upstream, ApiFormat::ChatCompletions);
    }

    #[test]
    fn app_config_still_compiles_without_a_key_list() {
        // Compile-time guard: the config shape no longer embeds key entries.
        let config = AppConfig {
            gateway_key: "k".into(),
            ..AppConfig::default()
        };
        config.validate().expect("scalar-key config validates");
    }

    #[test]
    fn disabled_cpa_catalog_does_not_enter_request_alias_resolution() {
        let state = state_with_snapshot();
        let now = chrono::Utc::now();
        let account = crate::models::Account {
            id: crate::provider::CPA_ACCOUNT_ID.to_string(),
            provider_id: crate::provider::CPA_PROVIDER_ID.to_string(),

            credential_kind: crate::provider::CredentialKind::ApiKey,
            quota_scope: crate::provider::QuotaScope::Key,
            name: crate::provider::CPA_ACCOUNT_NAME.to_string(),
            username: None,
            password_cipher: None,
            key_cipher: state.encrypt_key("inference").unwrap(),
            enabled: true,
            account_type: crate::models::AccountType::Key,
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
            notes: None,
            created_at: now,
            updated_at: now,
        };
        let management = state.encrypt_key("management").unwrap();
        state
            .db
            .lock()
            .upsert_cpa_integration(&account, crate::cpa::DEFAULT_CPA_BASE_URL, &management)
            .unwrap();
        state
            .activate_cpa_model_catalog(
                vec!["grok-4.5".into()],
                crate::cpa::DEFAULT_CPA_BASE_URL,
                now,
            )
            .unwrap();
        assert_eq!(active_cpa_model_ids(&state).as_slice(), ["grok-4.5"]);

        state
            .db
            .lock()
            .update_account(
                crate::provider::CPA_ACCOUNT_ID,
                &crate::models::AccountUpdate {
                    enabled: Some(false),
                    ..Default::default()
                },
                None,
                None,
            )
            .unwrap();
        assert!(active_cpa_model_ids(&state).is_empty());
    }
}
