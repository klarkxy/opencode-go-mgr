use crate::gateway::forwarder::{forward_get, forward_request, rate_limited_response};
use crate::gateway::protocol::{
    ApiFormat, ProtocolError, RequestPlan, format_error, prepare_gemini_request, prepare_request,
};
use crate::gateway::selector::AccountSelector;
use crate::models::{
    AppConfig, CLAUDE_DESKTOP_HAIKU_ALIAS, CLAUDE_DESKTOP_OPUS_ALIAS, CLAUDE_DESKTOP_SONNET_ALIAS,
    ClaudeDesktopModels,
};
use crate::state::CoreState;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

pub async fn chat_completions(
    State(state): State<CoreState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, headers, body, ApiFormat::ChatCompletions).await
}

pub async fn responses(
    State(state): State<CoreState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, headers, body, ApiFormat::Responses).await
}

pub async fn messages(
    State(state): State<CoreState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, headers, body, ApiFormat::Messages).await
}

pub async fn claude_desktop_messages(
    State(state): State<CoreState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler_inner(state, headers, body, ApiFormat::Messages, true).await
}

pub async fn claude_desktop_models(
    State(state): State<CoreState>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !check_auth(&headers, &state.config()) {
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
    Path(model_action): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let Some((model, action)) = model_action.rsplit_once(':') else {
        return gemini_error(
            &state,
            &headers,
            StatusCode::NOT_FOUND,
            "Gemini model action is required",
        );
    };
    if model.is_empty() {
        return gemini_error(
            &state,
            &headers,
            StatusCode::BAD_REQUEST,
            "Gemini model is required",
        );
    }
    match action {
        "generateContent" => {
            gemini_proxy_handler(state, headers, body, model.to_string(), false).await
        }
        "streamGenerateContent" => {
            gemini_proxy_handler(state, headers, body, model.to_string(), true).await
        }
        "countTokens" => gemini_error(
            &state,
            &headers,
            StatusCode::NOT_IMPLEMENTED,
            "Gemini countTokens is not available; Gemini CLI falls back to local estimation",
        ),
        "embedContent" => gemini_error(
            &state,
            &headers,
            StatusCode::NOT_IMPLEMENTED,
            "Gemini embeddings are not supported by this gateway",
        ),
        _ => gemini_error(
            &state,
            &headers,
            StatusCode::NOT_FOUND,
            "unknown Gemini model action",
        ),
    }
}

/// GET /v1/models — passthrough, any enabled account's key works.
pub async fn models(
    State(state): State<CoreState>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !check_auth(&headers, &state.config()) {
        return protocol_error_response(
            ApiFormat::ChatCompletions,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }

    let (config, client) = state.upstream_context();
    match forward_get(&client, &state, &config, "/v1/models").await {
        Ok(resp) => resp,
        Err(e) => protocol_error_response(
            ApiFormat::ChatCompletions,
            StatusCode::BAD_GATEWAY,
            &format!("models error: {}", e),
            None,
        ),
    }
}

async fn proxy_handler(
    state: CoreState,
    headers: HeaderMap,
    body: Bytes,
    client_format: ApiFormat,
) -> axum::response::Response {
    proxy_handler_inner(state, headers, body, client_format, false).await
}

async fn proxy_handler_inner(
    state: CoreState,
    headers: HeaderMap,
    body: Bytes,
    client_format: ApiFormat,
    claude_desktop: bool,
) -> axum::response::Response {
    let (config, client) = state.upstream_context();

    if !check_auth(&headers, &config) {
        return protocol_error_response(
            client_format,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }

    let body = if claude_desktop {
        match rewrite_claude_desktop_model(body, &config.claude_desktop_models) {
            Ok(body) => body,
            Err(error) => {
                return protocol_error_response(
                    ApiFormat::Messages,
                    error.status,
                    &error.message,
                    None,
                );
            }
        }
    } else {
        body
    };
    let plan = match prepare_request(client_format, body) {
        Ok(plan) => plan,
        Err(error) => {
            return protocol_error_response(client_format, error.status, &error.message, None);
        }
    };

    execute_plan(state, headers, client_format, plan, config, client).await
}

fn rewrite_claude_desktop_model(
    body: Bytes,
    models: &ClaudeDesktopModels,
) -> Result<Bytes, ProtocolError> {
    let mut request: serde_json::Value = serde_json::from_slice(&body)
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
    headers: HeaderMap,
    body: Bytes,
    model: String,
    stream: bool,
) -> axum::response::Response {
    let (config, client) = state.upstream_context();
    if !check_auth(&headers, &config) {
        return protocol_error_response(
            ApiFormat::Gemini,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }
    let plan = match prepare_gemini_request(model, stream, body) {
        Ok(plan) => plan,
        Err(error) => {
            return protocol_error_response(ApiFormat::Gemini, error.status, &error.message, None);
        }
    };
    execute_plan(state, headers, ApiFormat::Gemini, plan, config, client).await
}

async fn execute_plan(
    state: CoreState,
    headers: HeaderMap,
    client_format: ApiFormat,
    plan: RequestPlan,
    config: AppConfig,
    client: reqwest::Client,
) -> axum::response::Response {
    let selector = AccountSelector::new();
    let is_stream_request = plan.stream;

    let mut last_error: Option<String> = None;
    let mut failed_ids: Vec<String> = Vec::new();

    loop {
        let account = {
            let db = state.db.lock();
            let excluded = failed_ids.iter().map(String::as_str).collect::<Vec<_>>();
            match selector.select_excluding(&db, &excluded) {
                Ok(Some(a)) => a,
                Ok(None) => {
                    // No enabled, non-cooldown, non-excluded account left.
                    // If any enabled account is in cooldown, tell the client when the soonest resets.
                    let soonest = db.soonest_cooldown_reset().ok().flatten();
                    return match soonest {
                        Some(until) => rate_limited_response(client_format, until),
                        None => {
                            let msg =
                                last_error.unwrap_or_else(|| "no available accounts".to_string());
                            protocol_error_response(
                                client_format,
                                StatusCode::SERVICE_UNAVAILABLE,
                                &msg,
                                None,
                            )
                        }
                    };
                }
                Err(e) => {
                    return protocol_error_response(
                        client_format,
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("failed to select account: {}", e),
                        None,
                    );
                }
            }
        };

        let mut retried_same_account = false;
        loop {
            match forward_request(&client, &state, &account, &config, &plan, headers.clone()).await
            {
                Ok(result) => {
                    if result.success {
                        return result.response;
                    }
                    last_error = result.error_message.clone();
                    if result.retryable && !is_stream_request && !retried_same_account {
                        retried_same_account = true;
                        let _ = state.db.lock().log_gateway(
                            "warn",
                            "gateway",
                            &format!(
                                "account {} transient failure, retrying once: {:?}",
                                account.name, result.error_message
                            ),
                        );
                        continue;
                    }
                    failed_ids.push(account.id.clone());
                    let _ = state.db.lock().log_gateway(
                        "warn",
                        "gateway",
                        &format!(
                            "account {} failed, switching to next: {:?}",
                            account.name, result.error_message
                        ),
                    );
                    break;
                }
                Err(e) => {
                    last_error = Some(format!("forward error: {}", e));
                    if !is_stream_request && !retried_same_account {
                        retried_same_account = true;
                        continue;
                    }
                    failed_ids.push(account.id.clone());
                    break;
                }
            }
        }
    }
}

fn check_auth(headers: &HeaderMap, config: &AppConfig) -> bool {
    // Accept the standard bearer token plus Anthropic and Gemini SDK headers.
    let bearer_matches = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|auth| {
            let expected = format!("Bearer {}", config.gateway_key);
            auth.trim() == expected
        })
        .unwrap_or(false);
    bearer_matches
        || ["x-api-key", "x-goog-api-key"].iter().any(|name| {
            headers
                .get(*name)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|value| value.trim() == config.gateway_key)
        })
}

fn gemini_error(
    state: &CoreState,
    headers: &HeaderMap,
    status: StatusCode,
    message: &str,
) -> axum::response::Response {
    if !check_auth(headers, &state.config()) {
        return protocol_error_response(
            ApiFormat::Gemini,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }
    protocol_error_response(ApiFormat::Gemini, status, message, None)
}

fn protocol_error_response(
    format: ApiFormat,
    status: StatusCode,
    message: &str,
    upstream: Option<&serde_json::Value>,
) -> axum::response::Response {
    (
        status,
        axum::Json(format_error(format, status, message, upstream)),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{check_auth, rewrite_claude_desktop_model};
    use crate::gateway::protocol::{ApiFormat, prepare_request};
    use crate::models::{AppConfig, CLAUDE_DESKTOP_OPUS_ALIAS, ClaudeDesktopModels};
    use axum::body::Bytes;
    use axum::http::{HeaderMap, HeaderValue};
    use serde_json::json;

    #[test]
    fn gemini_api_key_header_is_accepted_and_wrong_key_is_rejected() {
        let config = AppConfig {
            gateway_key: "gateway-test-key".to_string(),
            ..AppConfig::default()
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-goog-api-key",
            HeaderValue::from_static("gateway-test-key"),
        );
        assert!(check_auth(&headers, &config));

        headers.insert("x-goog-api-key", HeaderValue::from_static("wrong-key"));
        assert!(!check_auth(&headers, &config));
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
            rewrite_claude_desktop_model(body, &models).expect("known alias should be rewritten");
        let plan = prepare_request(ApiFormat::Messages, rewritten)
            .expect("rewritten request should use the existing preparation path");

        assert_eq!(plan.model, "glm-5.2");
        assert_eq!(plan.upstream, ApiFormat::ChatCompletions);
    }
}
