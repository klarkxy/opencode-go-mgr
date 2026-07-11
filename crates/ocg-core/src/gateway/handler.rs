use crate::gateway::forwarder::{forward_get, forward_request};
use crate::gateway::protocol::{ApiFormat, format_error, prepare_request};
use crate::gateway::selector::AccountSelector;
use crate::models::AppConfig;
use crate::state::CoreState;
use axum::body::Bytes;
use axum::extract::State;
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

    let config = state.config();
    let client = &state.http_client;
    match forward_get(client, &state, &config.upstream_base_url, "/v1/models").await {
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
    let config = state.config();

    if !check_auth(&headers, &config) {
        return protocol_error_response(
            client_format,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }

    let plan = match prepare_request(client_format, body) {
        Ok(plan) => plan,
        Err(error) => {
            return protocol_error_response(client_format, error.status, &error.message, None);
        }
    };

    let client = &state.http_client;
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
            match forward_request(
                client,
                &state,
                &account,
                &config.upstream_base_url,
                &plan,
                headers.clone(),
            )
            .await
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
    // Accept Authorization: Bearer <key> or x-api-key: <key> (Anthropic SDK compat)
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|auth| {
            let expected = format!("Bearer {}", config.gateway_key);
            auth.trim() == expected
        })
        .unwrap_or_else(|| {
            headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.trim() == config.gateway_key)
                .unwrap_or(false)
        })
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

fn rate_limited_response(
    format: ApiFormat,
    resets_at: chrono::DateTime<chrono::Utc>,
) -> axum::response::Response {
    let message = format!(
        "all accounts rate-limited, soonest resets at {}",
        resets_at.to_rfc3339()
    );
    let mut body = format_error(format, StatusCode::TOO_MANY_REQUESTS, &message, None);
    body["error"]["resets_at"] = serde_json::json!(resets_at.to_rfc3339());
    (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response()
}
