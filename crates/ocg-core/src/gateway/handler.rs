use crate::gateway::forwarder::{forward_get, forward_request};
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
    proxy_handler(state, headers, body, "/v1/chat/completions").await
}

pub async fn messages(
    State(state): State<CoreState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, headers, body, "/v1/messages").await
}

/// GET /v1/models — passthrough, any enabled account's key works.
pub async fn models(
    State(state): State<CoreState>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !check_auth(&headers, &state.config()) {
        return error_response(StatusCode::UNAUTHORIZED, "invalid gateway key");
    }

    let config = state.config();
    let client = &state.http_client;
    match forward_get(client, &state, &config.upstream_base_url, "/v1/models").await {
        Ok(resp) => resp,
        Err(e) => error_response(StatusCode::BAD_GATEWAY, &format!("models error: {}", e)),
    }
}

async fn proxy_handler(
    state: CoreState,
    headers: HeaderMap,
    body: Bytes,
    upstream_path: &str,
) -> axum::response::Response {
    let config = state.config();

    if !check_auth(&headers, &config) {
        return error_response(StatusCode::UNAUTHORIZED, "invalid gateway key");
    }

    let client = &state.http_client;
    let selector =
        AccountSelector::with_counter(config.selection_strategy, state.round_robin_counter.clone());

    let mut last_error: Option<String> = None;
    let mut failed_ids: Vec<String> = Vec::new();

    for _attempt in 0..5 {
        let account = {
            let db = state.db.lock();
            let excluded = failed_ids.iter().map(String::as_str).collect::<Vec<_>>();
            match selector.select_excluding(&*db, &excluded) {
                Ok(Some(a)) => a,
                Ok(None) => {
                    // No enabled, non-cooldown, non-excluded account left.
                    // If any enabled account is in cooldown, tell the client when the soonest resets.
                    let soonest = db.soonest_cooldown_reset().ok().flatten();
                    return match soonest {
                        Some(until) => rate_limited_response(until),
                        None => {
                            let msg =
                                last_error.unwrap_or_else(|| "no available accounts".to_string());
                            error_response(StatusCode::SERVICE_UNAVAILABLE, &msg)
                        }
                    };
                }
                Err(e) => {
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("failed to select account: {}", e),
                    );
                }
            }
        };

        match forward_request(
            &client,
            &state,
            &account,
            &config.upstream_base_url,
            upstream_path,
            headers.clone(),
            body.clone(),
        )
        .await
        {
            Ok(result) => {
                if result.success {
                    return result.response;
                } else {
                    last_error = result.error_message.clone();
                    failed_ids.push(account.id.clone());
                    let _ = {
                        let db = state.db.lock();
                        db.log_gateway(
                            "warn",
                            "gateway",
                            &format!(
                                "account {} failed, switching to next: {:?}",
                                account.name, result.error_message
                            ),
                        )
                    };
                }
            }
            Err(e) => {
                last_error = Some(format!("forward error: {}", e));
                failed_ids.push(account.id.clone());
            }
        }
    }

    error_response(
        StatusCode::BAD_GATEWAY,
        &last_error.unwrap_or_else(|| "all accounts failed".to_string()),
    )
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

fn error_response(status: StatusCode, message: &str) -> axum::response::Response {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "gateway_error"
        }
    });
    (status, axum::Json(body)).into_response()
}

fn rate_limited_response(resets_at: chrono::DateTime<chrono::Utc>) -> axum::response::Response {
    let body = serde_json::json!({
        "error": {
            "message": format!(
                "all accounts rate-limited, soonest resets at {}",
                resets_at.to_rfc3339()
            ),
            "type": "rate_limited",
            "resets_at": resets_at.to_rfc3339(),
        }
    });
    (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response()
}
