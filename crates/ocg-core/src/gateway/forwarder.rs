use crate::db::Database;
use crate::gateway::cost::estimate_cost;
use crate::gateway::limit::parse_reset;
use crate::gateway::selector::AccountSelector;
use crate::models::{Account, ForwardLog};
use crate::state::CoreState;
use anyhow::Result;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{Duration, Utc};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration as StdDuration;

const UPSTREAM_TIMEOUT: StdDuration = StdDuration::from_secs(120);

pub struct ForwardResult {
    pub response: Response,
    pub account: Account,
    pub success: bool,
    pub error_message: Option<String>,
}

pub async fn forward_request(
    client: &Client,
    state: &CoreState,
    account: &Account,
    upstream_base_url: &str,
    upstream_path: &str,
    headers: HeaderMap,
    body_bytes: bytes::Bytes,
) -> Result<ForwardResult> {
    forward_request_impl(client, state, account, upstream_base_url, upstream_path, headers, body_bytes).await
}

async fn forward_request_impl(
    client: &Client,
    state: &CoreState,
    account: &Account,
    upstream_base_url: &str,
    upstream_path: &str,
    headers: HeaderMap,
    body_bytes: bytes::Bytes,
) -> Result<ForwardResult> {
    let key = state.decrypt_key(&account.key_cipher)?;
    let mut upstream_headers = reqwest::header::HeaderMap::new();

    // Forward client headers except Authorization (we use the account's key)
    for (name, value) in headers.iter() {
        if name.as_str().to_lowercase() != "authorization" {
            upstream_headers.insert(name.clone(), value.clone());
        }
    }
    // Ensure Content-Type and Authorization are set correctly
    upstream_headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    upstream_headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", key))?,
    );

    let url = format!("{}{}", upstream_base_url.trim_end_matches('/'), upstream_path);

    let request_body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    let model = request_body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let upstream_req = client
        .post(&url)
        .headers(upstream_headers)
        .body(body_bytes.to_vec());

    let upstream_resp = match upstream_req.send().await {
        Ok(resp) => resp,
        Err(e) => {
            let error_message = format!("upstream request failed: {}", e);
            {
                let db = state.db.lock();
                log_forward(&*db, account, &model, "error", None, 0, 0, 0, 0.0, Some(&error_message))?;
            }
            return Ok(ForwardResult {
                response: error_response(&error_message),
                account: account.clone(),
                success: false,
                error_message: Some(error_message),
            });
        }
    };

    let status = upstream_resp.status();
    let is_stream = upstream_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/event-stream"))
        .unwrap_or(false);

    if status.is_server_error() {
        let text = upstream_resp.text().await.unwrap_or_default();
        let error_message = format!("upstream error {}: {}", status.as_u16(), text);
        {
            let db = state.db.lock();
            log_forward(
                &*db,
                account,
                &model,
                "error",
                Some(status.as_u16() as i32),
                0, 0, 0, 0.0,
                Some(&error_message),
            )?;
        }
        return Ok(ForwardResult {
            response: error_response(&error_message),
            account: account.clone(),
            success: false,
            error_message: Some(error_message),
        });
    }

    if status.is_client_error() {
        let text = upstream_resp.text().await.unwrap_or_default();

        if status.as_u16() == 429 {
            // 429 from opencode-go carries the exact reset window ("Resets in 13 days" / "4 days" / "13min").
            // Parse it, cool the account down until then, and fail over to the next account
            // (success: false). 5xx/transport errors are environment-level — no cooldown, just failover.
            let cooldown = parse_reset(&text).unwrap_or_else(|| Duration::minutes(5));
            let until = Utc::now() + cooldown;
            let error_message = format!(
                "rate limited: {} (resets in {}s)",
                text.trim(),
                cooldown.num_seconds()
            );
            {
                let db = state.db.lock();
                log_forward(
                    &*db,
                    account,
                    &model,
                    "client_error",
                    Some(429),
                    0, 0, 0, 0.0,
                    Some(&text),
                )?;
                db.set_account_cooldown(&account.id, Some(until), Some(&text))?;
            }
            return Ok(ForwardResult {
                response: error_response(&error_message),
                account: account.clone(),
                success: false,
                error_message: Some(error_message),
            });
        }

        // Other 4xx (400/401/403/404...): client- or key-level error. Pass through, don't retry.
        {
            let db = state.db.lock();
            log_forward(&*db, account, &model, "client_error", Some(status.as_u16() as i32), 0, 0, 0, 0.0, Some(&text))?;
        }
        let mut response_headers = HeaderMap::new();
        response_headers.insert("content-type", HeaderValue::from_static("application/json"));
        return Ok(ForwardResult {
            response: (status, response_headers, text).into_response(),
            account: account.clone(),
            success: true,
            error_message: None,
        });
    }

    // Success path — for non-stream, record breaker success now.
    // For streams, don't pre-record success; the stream error handler
    // records errors, and we haven't proven success until the stream completes.

    if is_stream {
        let response_builder = Response::builder()
            .status(status)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("connection", "keep-alive");

        let stream = upstream_resp.bytes_stream();
        let state_clone = state.clone();
        let account_clone = account.clone();
        let model_clone = model.clone();

        let mapped = stream.map(move |result| {
            match result {
                Ok(chunk) => Ok(chunk),
                Err(e) => {
                    let _ = {
                        let db = state_clone.db.lock();
                        log_forward(
                            &*db,
                            &account_clone,
                            &model_clone,
                            "error",
                            None,
                            0,
                            0,
                            0,
                            0.0,
                            Some(&format!("stream error: {}", e)),
                        )
                    };
                    Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("stream error: {}", e),
                    ))
                }
            }
        });

        {
            let db = state.db.lock();
            log_forward(&*db, account, &model, "streaming", Some(status.as_u16() as i32), 0, 0, 0, 0.0, None)?;
        }
        Ok(ForwardResult {
            response: response_builder.body(Body::from_stream(mapped))?,
            account: account.clone(),
            success: true,
            error_message: None,
        })
    } else {
        let text = upstream_resp.text().await.unwrap_or_default();
        let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let usage = parsed.get("usage").cloned().unwrap_or(Value::Null);
        let cost = estimate_cost(&model, &usage);
        let prompt_tokens = usage
            .get("prompt_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let completion_tokens = usage
            .get("completion_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let cached_tokens = usage
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0);

        {
            let db = state.db.lock();
            log_forward(
                &*db,
                account,
                &model,
                "success",
                Some(status.as_u16() as i32),
                prompt_tokens,
                completion_tokens,
                cached_tokens,
                cost,
                None,
            )?;
        }

        let mut response_headers = HeaderMap::new();
        response_headers.insert("content-type", HeaderValue::from_static("application/json"));
        Ok(ForwardResult {
            response: (status, response_headers, text).into_response(),
            account: account.clone(),
            success: true,
            error_message: None,
        })
    }
}

/// Simple GET forward for endpoints like /v1/models — uses configured selection strategy.
pub async fn forward_get(
    client: &Client,
    state: &CoreState,
    upstream_base_url: &str,
    upstream_path: &str,
) -> Result<Response> {
    let config = state.config();
    let selector = AccountSelector::with_counter(config.selection_strategy, state.round_robin_counter.clone());
    let account = {
        let db = state.db.lock();
        selector.select(&*db, None)?
            .ok_or_else(|| anyhow::anyhow!("no enabled accounts available"))
    }?;

    let key = state.decrypt_key(&account.key_cipher)?;
    let url = format!("{}{}", upstream_base_url.trim_end_matches('/'), upstream_path);

    let resp = match client
        .get(&url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", key))
        .timeout(UPSTREAM_TIMEOUT)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return Err(e.into()),
    };

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    {
        let db = state.db.lock();
        let category = if status.is_server_error() {
            "error"
        } else if status.is_client_error() {
            "client_error"
        } else {
            "success"
        };
        log_forward(&*db, &account, "", category, Some(status.as_u16() as i32), 0, 0, 0, 0.0, Some(&body))?;
        if status.as_u16() == 429 {
            // 429 cooldown: parse the reset window so the next request skips this account.
            let cooldown = parse_reset(&body).unwrap_or_else(|| Duration::minutes(5));
            db.set_account_cooldown(&account.id, Some(Utc::now() + cooldown), Some(&body))?;
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("application/json"));

    Ok((status, headers, body).into_response())
}

fn error_response(message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "gateway_error"
        }
    });
    (StatusCode::BAD_GATEWAY, axum::Json(body)).into_response()
}

fn log_forward(
    db: &Database,
    account: &Account,
    model: &str,
    status: &str,
    http_status: Option<i32>,
    prompt_tokens: i64,
    completion_tokens: i64,
    cached_tokens: i64,
    cost: f64,
    error_message: Option<&str>,
) -> Result<()> {
    db.log_forward(&ForwardLog {
        id: 0,
        timestamp: Utc::now(),
        model: model.to_string(),
        account_id: account.id.clone(),
        account_name: account.name.clone(),
        status: status.to_string(),
        http_status,
        prompt_tokens,
        completion_tokens,
        cached_tokens,
        cost,
        error_message: error_message.map(|s| s.to_string()),
    })
}
