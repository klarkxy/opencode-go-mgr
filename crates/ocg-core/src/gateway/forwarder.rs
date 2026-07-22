use crate::db::{Database, ForwardLogDiagnosticUpdate};
use crate::gateway::diagnostics::{
    ErrorDiagnostic, RequestTrace, api_format_name, emit_failure, safe_upstream_headers,
    sanitize_upstream_error_value, serialize_diagnostic,
};
use crate::gateway::limit::{parse_reset, parse_usage_limit_window};
use crate::gateway::protocol::{
    ApiFormat, RequestPlan, UsageCounts, error_body, extract_usage, format_error,
    has_complete_usage, has_usage, merge_stream_usage, transform_response,
};
use crate::gateway::protocol_stream::StreamConverter;
use crate::gateway::selector::AccountSelector;
use crate::models::{Account, AppConfig, ForwardLog, ForwardMetrics};
use crate::pricing::PricingSnapshot;
use crate::state::CoreState;
use anyhow::Result;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::BytesMut;
use chrono::{Duration, Utc};
use futures_util::StreamExt;
use parking_lot::Mutex;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

const MAX_UPSTREAM_ERROR_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForwardAction {
    Return,
    RetrySameAccount,
    TryNextAccount,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct UpstreamPayloadTooLargeResponse;

pub struct ForwardResult {
    pub response: Response,
    pub(crate) action: ForwardAction,
    pub error_message: Option<String>,
}

#[derive(Clone)]
struct ForwardAttemptContext {
    trace: RequestTrace,
    client_body_bytes: usize,
    upstream_body_bytes: usize,
    attempt: u32,
    client_format: ApiFormat,
    upstream_format: ApiFormat,
    model: String,
    stream: bool,
}

impl ForwardAttemptContext {
    fn new(
        trace: &RequestTrace,
        client_body_bytes: usize,
        attempt: u32,
        plan: &RequestPlan,
    ) -> Self {
        Self {
            trace: trace.clone(),
            client_body_bytes,
            upstream_body_bytes: plan.body.len(),
            attempt,
            client_format: plan.client,
            upstream_format: plan.upstream,
            model: plan.model.clone(),
            stream: plan.stream,
        }
    }

    fn failure(&self, spec: FailureSpec<'_>) -> FailureRecord {
        let mut diagnostic = ErrorDiagnostic::new(
            &self.trace,
            self.attempt,
            spec.error_source,
            spec.error_stage,
            self.client_format,
        );
        diagnostic.upstream_format = Some(api_format_name(self.upstream_format).to_string());
        diagnostic.model = Some(self.model.clone());
        diagnostic.stream = Some(self.stream);
        diagnostic.client_body_bytes = Some(self.client_body_bytes);
        diagnostic.upstream_body_bytes = Some(self.upstream_body_bytes);
        diagnostic.upstream_wait_ms = spec.upstream_wait_ms;
        diagnostic.downstream_status = spec.downstream_status;
        diagnostic.upstream_status = spec.upstream_status;
        diagnostic.retry_action = spec.retry_action.map(str::to_string);
        if let Some(headers) = spec.upstream_headers {
            diagnostic.upstream_headers = safe_upstream_headers(headers);
        }
        if let Some(body) = spec.request_body {
            diagnostic = diagnostic.with_request_summary(body);
        }
        if let Some(error) = spec.upstream_error {
            diagnostic = diagnostic.with_upstream_error(error);
        }
        let duration_ms = diagnostic.duration_ms.min(i64::MAX as u64) as i64;
        let diagnostic_json = serialize_diagnostic(diagnostic);
        emit_failure(&diagnostic_json);
        FailureRecord {
            error_source: spec.error_source.to_string(),
            error_stage: spec.error_stage.to_string(),
            duration_ms,
            diagnostic_json,
        }
    }
}

struct FailureSpec<'a> {
    error_source: &'static str,
    error_stage: &'static str,
    downstream_status: Option<u16>,
    upstream_status: Option<u16>,
    upstream_wait_ms: Option<u64>,
    retry_action: Option<&'static str>,
    upstream_headers: Option<&'a HeaderMap>,
    upstream_error: Option<&'a str>,
    request_body: Option<&'a [u8]>,
}

struct FailureRecord {
    error_source: String,
    error_stage: String,
    duration_ms: i64,
    diagnostic_json: String,
}

impl FailureRecord {
    fn update(&self) -> ForwardLogDiagnosticUpdate<'_> {
        ForwardLogDiagnosticUpdate {
            error_source: &self.error_source,
            error_stage: &self.error_stage,
            duration_ms: self.duration_ms,
            diagnostic_json: &self.diagnostic_json,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn forward_request(
    client: &Client,
    state: &CoreState,
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
    trace: &RequestTrace,
    client_body: &[u8],
    attempt: u32,
    allow_same_account_retry: bool,
    headers: HeaderMap,
    pricing_snapshot: Arc<PricingSnapshot>,
) -> Result<ForwardResult> {
    forward_request_impl(
        client,
        state,
        account,
        config,
        plan,
        trace,
        client_body,
        attempt,
        allow_same_account_retry,
        headers,
        pricing_snapshot,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn forward_request_impl(
    client: &Client,
    state: &CoreState,
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
    trace: &RequestTrace,
    client_body: &[u8],
    attempt: u32,
    allow_same_account_retry: bool,
    headers: HeaderMap,
    pricing_snapshot: Arc<PricingSnapshot>,
) -> Result<ForwardResult> {
    let attempt_context = ForwardAttemptContext::new(trace, client_body.len(), attempt, plan);
    ensure_safe_upstream_base_url(&config.upstream_base_url)?;
    let key = match state.decrypt_key(&account.key_cipher) {
        Ok(key) => key,
        Err(error) => {
            let message = format!("failed to decrypt account credentials: {error}");
            let failure = attempt_context.failure(FailureSpec {
                error_source: "gateway",
                error_stage: "credential",
                downstream_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                upstream_status: None,
                upstream_wait_ms: None,
                retry_action: Some("try_next_account"),
                upstream_headers: None,
                upstream_error: None,
                request_body: Some(client_body),
            });
            log_forward(
                &state.db.lock(),
                account,
                &plan.model,
                "error",
                None,
                metadata_metrics(
                    &pricing_snapshot,
                    plan.service_tier.as_deref(),
                    "not_applicable",
                ),
                Some(&message),
                &attempt_context,
                Some(failure),
            )?;
            return Ok(account_preflight_failure(plan, message));
        }
    };
    let mut upstream_headers = reqwest::header::HeaderMap::new();

    // Forward harmless client headers only. Auth and hop-by-hop/private headers
    // belong to the gateway/client boundary, not the upstream request.
    for (name, value) in headers.iter() {
        let header = name.as_str().to_ascii_lowercase();
        if !(matches!(
            header.as_str(),
            "authorization"
                | "x-api-key"
                | "x-goog-api-key"
                | "cookie"
                | "proxy-authorization"
                | "host"
                | "content-length"
                | "connection"
                | "transfer-encoding"
                | "accept-encoding"
        ) || (plan.upstream != ApiFormat::Messages
            && matches!(header.as_str(), "anthropic-version" | "anthropic-beta")))
        {
            upstream_headers.insert(name.clone(), value.clone());
        }
    }
    // Match the upstream protocol's authentication header.
    upstream_headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    if plan.upstream == ApiFormat::Messages {
        let key_header = match reqwest::header::HeaderValue::from_str(&key) {
            Ok(value) => value,
            Err(error) => {
                let message = format!("account key is not a valid upstream header value: {error}");
                let failure = attempt_context.failure(FailureSpec {
                    error_source: "gateway",
                    error_stage: "credential",
                    downstream_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                    upstream_status: None,
                    upstream_wait_ms: None,
                    retry_action: Some("try_next_account"),
                    upstream_headers: None,
                    upstream_error: None,
                    request_body: Some(client_body),
                });
                log_forward(
                    &state.db.lock(),
                    account,
                    &plan.model,
                    "error",
                    None,
                    metadata_metrics(
                        &pricing_snapshot,
                        plan.service_tier.as_deref(),
                        "not_applicable",
                    ),
                    Some(&message),
                    &attempt_context,
                    Some(failure),
                )?;
                return Ok(account_preflight_failure(plan, message));
            }
        };
        upstream_headers.insert("x-api-key", key_header);
        if !upstream_headers.contains_key("anthropic-version") {
            upstream_headers.insert(
                "anthropic-version",
                reqwest::header::HeaderValue::from_static("2023-06-01"),
            );
        }
    } else {
        let authorization = match reqwest::header::HeaderValue::from_str(&format!("Bearer {key}")) {
            Ok(value) => value,
            Err(error) => {
                let message = format!("account key is not a valid upstream header value: {error}");
                let failure = attempt_context.failure(FailureSpec {
                    error_source: "gateway",
                    error_stage: "credential",
                    downstream_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                    upstream_status: None,
                    upstream_wait_ms: None,
                    retry_action: Some("try_next_account"),
                    upstream_headers: None,
                    upstream_error: None,
                    request_body: Some(client_body),
                });
                log_forward(
                    &state.db.lock(),
                    account,
                    &plan.model,
                    "error",
                    None,
                    metadata_metrics(
                        &pricing_snapshot,
                        plan.service_tier.as_deref(),
                        "not_applicable",
                    ),
                    Some(&message),
                    &attempt_context,
                    Some(failure),
                )?;
                return Ok(account_preflight_failure(plan, message));
            }
        };
        upstream_headers.insert(reqwest::header::AUTHORIZATION, authorization);
    }
    upstream_headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        reqwest::header::HeaderValue::from_static("identity"),
    );

    let upstream_path = plan
        .upstream
        .upstream_path()
        .ok_or_else(|| anyhow::anyhow!("Gemini is a client-only protocol"))?;
    let url = format!(
        "{}{}",
        config.upstream_base_url.trim_end_matches('/'),
        upstream_path
    );

    let model = plan.model.clone();
    let upstream_req = client
        .post(&url)
        .headers(upstream_headers)
        .body(plan.body.clone());
    let upstream_started = Instant::now();
    let upstream_resp = if plan.stream {
        match tokio::time::timeout(
            StdDuration::from_secs(config.stream_idle_timeout_secs),
            upstream_req.send(),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                let detail = format!(
                    "upstream did not return response headers within {}s",
                    config.stream_idle_timeout_secs
                );
                let error_message = outcome_unknown_message(&detail);
                let upstream_wait_ms = upstream_started.elapsed().as_millis() as u64;
                let failure = attempt_context.failure(FailureSpec {
                    error_source: "transport",
                    error_stage: "response_headers",
                    downstream_status: Some(StatusCode::GATEWAY_TIMEOUT.as_u16()),
                    upstream_status: None,
                    upstream_wait_ms: Some(upstream_wait_ms),
                    retry_action: Some("return"),
                    upstream_headers: None,
                    upstream_error: Some(&detail),
                    request_body: Some(client_body),
                });
                {
                    let db = state.db.lock();
                    log_forward(
                        &db,
                        account,
                        &model,
                        "outcome_unknown",
                        None,
                        metadata_metrics(
                            &pricing_snapshot,
                            plan.service_tier.as_deref(),
                            "outcome_unknown",
                        ),
                        Some(&error_message),
                        &attempt_context,
                        Some(failure),
                    )?;
                }
                return Ok(ForwardResult {
                    response: outcome_unknown_response(
                        plan.client,
                        StatusCode::GATEWAY_TIMEOUT,
                        &detail,
                    ),
                    action: ForwardAction::Return,
                    error_message: Some(error_message),
                });
            }
        }
    } else {
        upstream_req
            .timeout(StdDuration::from_secs(config.non_stream_timeout_secs))
            .send()
            .await
    };

    let upstream_resp = match upstream_resp {
        Ok(resp) => resp,
        Err(e) => {
            let upstream_wait_ms = upstream_started.elapsed().as_millis() as u64;
            let connect_failure = e.is_connect();
            let outcome_unknown = !connect_failure;
            let detail = if e.is_timeout() {
                format!("upstream request timed out: {e}")
            } else {
                format!("upstream request failed: {e}")
            };
            let error_message = if outcome_unknown {
                outcome_unknown_message(&detail)
            } else {
                detail.clone()
            };
            let status = if e.is_timeout() {
                StatusCode::GATEWAY_TIMEOUT
            } else {
                StatusCode::BAD_GATEWAY
            };
            let failure = attempt_context.failure(FailureSpec {
                error_source: "transport",
                error_stage: if connect_failure {
                    "connect"
                } else {
                    "response_headers"
                },
                downstream_status: Some(status.as_u16()),
                upstream_status: None,
                upstream_wait_ms: Some(upstream_wait_ms),
                retry_action: Some(if connect_failure && allow_same_account_retry {
                    "retry_same_account"
                } else {
                    "return"
                }),
                upstream_headers: None,
                upstream_error: Some(&detail),
                request_body: Some(client_body),
            });
            {
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    if outcome_unknown {
                        "outcome_unknown"
                    } else {
                        "error"
                    },
                    None,
                    metadata_metrics(
                        &pricing_snapshot,
                        plan.service_tier.as_deref(),
                        if outcome_unknown {
                            "outcome_unknown"
                        } else {
                            "not_applicable"
                        },
                    ),
                    Some(&error_message),
                    &attempt_context,
                    Some(failure),
                )?;
            }
            return Ok(ForwardResult {
                response: if outcome_unknown {
                    outcome_unknown_response(plan.client, status, &detail)
                } else {
                    error_response(plan.client, &error_message, None)
                },
                action: if connect_failure && allow_same_account_retry {
                    ForwardAction::RetrySameAccount
                } else {
                    ForwardAction::Return
                },
                error_message: Some(error_message),
            });
        }
    };

    let upstream_wait_ms = upstream_started.elapsed().as_millis() as u64;

    let status = upstream_resp.status();
    let is_stream = upstream_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/event-stream"))
        .unwrap_or(false);

    let body_timeout = plan
        .stream
        .then(|| StdDuration::from_secs(config.stream_idle_timeout_secs));

    if status.is_server_error() {
        // A response status is authoritative even if its error body stalls. Keep
        // that status and never replay the request; the bounded read only affects
        // how much safe diagnostic text we can return.
        let error_headers = upstream_resp.headers().clone();
        let text = response_text_with_timeout(
            upstream_resp,
            body_timeout,
            Some(MAX_UPSTREAM_ERROR_BODY_BYTES),
        )
        .await
        .unwrap_or_else(ResponseBodyFailure::into_detail);
        let error_message = format!(
            "upstream error {}: {}",
            status.as_u16(),
            sanitize_upstream_error(&text)
        );
        let failure = attempt_context.failure(FailureSpec {
            error_source: "upstream",
            error_stage: "upstream_http",
            downstream_status: Some(status.as_u16()),
            upstream_status: Some(status.as_u16()),
            upstream_wait_ms: Some(upstream_wait_ms),
            retry_action: Some("return"),
            upstream_headers: Some(&error_headers),
            upstream_error: Some(&text),
            request_body: Some(client_body),
        });
        {
            let db = state.db.lock();
            log_forward(
                &db,
                account,
                &model,
                "error",
                Some(status.as_u16() as i32),
                metadata_metrics(
                    &pricing_snapshot,
                    plan.service_tier.as_deref(),
                    "not_applicable",
                ),
                Some(&error_message),
                &attempt_context,
                Some(failure),
            )?;
        }
        return Ok(ForwardResult {
            response: protocol_status_error_response(plan.client, status, &error_message, None),
            action: ForwardAction::Return,
            error_message: Some(error_message),
        });
    }

    if status.is_client_error() {
        // As above, a known 4xx proves the upstream rejected the request. Body
        // read failures must not turn into a replay or account fallback except
        // for the explicit 401/403/429 status policy below.
        let error_headers = upstream_resp.headers().clone();
        let text = response_text_with_timeout(
            upstream_resp,
            body_timeout,
            Some(MAX_UPSTREAM_ERROR_BODY_BYTES),
        )
        .await
        .unwrap_or_else(ResponseBodyFailure::into_detail);

        if status.as_u16() == 429 {
            // 429 from opencode-go carries the exact reset window ("Resets in 13 days" / "4 days" / "13min").
            // Parse it, cool the account down until then, and fail over to the next account.
            // Unlike a rejected 429, ambiguous transport failures and 5xx responses are not replayed.
            let cooldown = parse_reset(&text).unwrap_or_else(|| Duration::minutes(5));
            let until = Utc::now() + cooldown;
            let sanitized = sanitize_upstream_error(&text);
            let error_message = format!(
                "rate limited: {} (resets in {}s)",
                sanitized,
                cooldown.num_seconds()
            );
            let failure = attempt_context.failure(FailureSpec {
                error_source: "upstream",
                error_stage: "upstream_http",
                downstream_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                upstream_status: Some(status.as_u16()),
                upstream_wait_ms: Some(upstream_wait_ms),
                retry_action: Some("try_next_account"),
                upstream_headers: Some(&error_headers),
                upstream_error: Some(&text),
                request_body: Some(client_body),
            });
            {
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "client_error",
                    Some(429),
                    metadata_metrics(
                        &pricing_snapshot,
                        plan.service_tier.as_deref(),
                        "not_applicable",
                    ),
                    Some(&sanitized),
                    &attempt_context,
                    Some(failure),
                )?;
                db.set_account_rate_limit_if_key_matches(
                    &account.id,
                    &account.key_cipher,
                    until,
                    &sanitized,
                    parse_usage_limit_window(&text),
                )?;
            }
            return Ok(ForwardResult {
                response: error_response(plan.client, &error_message, None),
                action: ForwardAction::TryNextAccount,
                error_message: Some(error_message),
            });
        }

        if status.as_u16() == 408 {
            let detail = format!("upstream returned 408: {}", sanitize_upstream_error(&text));
            let error_message = outcome_unknown_message(&detail);
            let failure = attempt_context.failure(FailureSpec {
                error_source: "upstream",
                error_stage: "upstream_http",
                downstream_status: Some(StatusCode::GATEWAY_TIMEOUT.as_u16()),
                upstream_status: Some(status.as_u16()),
                upstream_wait_ms: Some(upstream_wait_ms),
                retry_action: Some("return"),
                upstream_headers: Some(&error_headers),
                upstream_error: Some(&text),
                request_body: Some(client_body),
            });
            {
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "outcome_unknown",
                    Some(408),
                    metadata_metrics(
                        &pricing_snapshot,
                        plan.service_tier.as_deref(),
                        "outcome_unknown",
                    ),
                    Some(&error_message),
                    &attempt_context,
                    Some(failure),
                )?;
            }
            return Ok(ForwardResult {
                response: outcome_unknown_response(
                    plan.client,
                    StatusCode::GATEWAY_TIMEOUT,
                    &detail,
                ),
                action: ForwardAction::Return,
                error_message: Some(error_message),
            });
        }

        // Key-level auth failures may be isolated to this account; fail over.
        if matches!(status.as_u16(), 401 | 403) {
            let error_message = format!(
                "upstream auth error {}: {}",
                status.as_u16(),
                sanitize_upstream_error(&text)
            );
            let sanitized = sanitize_upstream_error(&text);
            let failure = attempt_context.failure(FailureSpec {
                error_source: "upstream",
                error_stage: "upstream_http",
                downstream_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                upstream_status: Some(status.as_u16()),
                upstream_wait_ms: Some(upstream_wait_ms),
                retry_action: Some("try_next_account"),
                upstream_headers: Some(&error_headers),
                upstream_error: Some(&text),
                request_body: Some(client_body),
            });
            {
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "client_error",
                    Some(status.as_u16() as i32),
                    metadata_metrics(
                        &pricing_snapshot,
                        plan.service_tier.as_deref(),
                        "not_applicable",
                    ),
                    Some(&sanitized),
                    &attempt_context,
                    Some(failure),
                )?;
                if status == StatusCode::UNAUTHORIZED {
                    db.set_account_auth_error_if_key_matches(
                        &account.id,
                        &account.key_cipher,
                        Some(&error_message),
                    )?;
                }
            }
            return Ok(ForwardResult {
                response: error_response(plan.client, &error_message, None),
                action: ForwardAction::TryNextAccount,
                error_message: Some(error_message),
            });
        }

        // Other 4xx: request-level error. Convert its envelope for the caller,
        // but don't retry another account for the same invalid request.
        let sanitized = sanitize_upstream_error(&text);
        let failure = attempt_context.failure(FailureSpec {
            error_source: "upstream",
            error_stage: "upstream_http",
            downstream_status: Some(status.as_u16()),
            upstream_status: Some(status.as_u16()),
            upstream_wait_ms: Some(upstream_wait_ms),
            retry_action: Some("return"),
            upstream_headers: Some(&error_headers),
            upstream_error: Some(&text),
            request_body: Some(client_body),
        });
        {
            let db = state.db.lock();
            log_forward(
                &db,
                account,
                &model,
                "client_error",
                Some(status.as_u16() as i32),
                metadata_metrics(
                    &pricing_snapshot,
                    plan.service_tier.as_deref(),
                    "not_applicable",
                ),
                Some(&sanitized),
                &attempt_context,
                Some(failure),
            )?;
        }
        let upstream_error = serde_json::from_str::<Value>(&text).ok();
        let message = sanitized;
        let body = format_error(plan.client, status, &message, upstream_error.as_ref());
        let mut response = (status, axum::Json(body)).into_response();
        if status == StatusCode::PAYLOAD_TOO_LARGE {
            response
                .extensions_mut()
                .insert(UpstreamPayloadTooLargeResponse);
        }
        return Ok(ForwardResult {
            response,
            action: ForwardAction::Return,
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

        // Insert the "streaming" row up front so a process crash mid-stream still
        // leaves a record. The finalizer updates it once the stream ends. The error
        // path also updates this row (instead of inserting a duplicate) so every
        // request maps to exactly one row in forward_logs.
        let initial_id: i64 = {
            let db = state.db.lock();
            log_forward(
                &db,
                account,
                &model,
                "streaming",
                Some(status.as_u16() as i32),
                metadata_metrics(
                    &pricing_snapshot,
                    plan.service_tier.as_deref(),
                    "not_applicable",
                ),
                None,
                &attempt_context,
                None,
            )?
        };

        let stream_idle_timeout = StdDuration::from_secs(config.stream_idle_timeout_secs);
        let stream = futures_util::stream::unfold(
            (Box::pin(upstream_resp.bytes_stream()), false),
            move |(mut stream, finished)| async move {
                if finished {
                    return None;
                }
                match tokio::time::timeout(stream_idle_timeout, stream.next()).await {
                    Ok(Some(Ok(chunk))) => Some((StreamRead::Chunk(chunk), (stream, false))),
                    Ok(Some(Err(error))) => Some((StreamRead::Failed(error), (stream, true))),
                    Ok(None) => None,
                    Err(_) => Some((StreamRead::IdleTimeout, (stream, true))),
                }
            },
        );
        let state_h = state.clone();
        let st = Arc::new(Mutex::new(StreamState::default()));
        let converter = Arc::new(Mutex::new(StreamConverter::new(plan)));
        let upstream_format = plan.upstream;
        let stream_idle_timeout_secs = config.stream_idle_timeout_secs;

        let st_map = st.clone();
        let converter_map = converter.clone();
        let model_for_stream = model.clone();
        let pricing_map = pricing_snapshot.clone();
        let service_tier_map = plan.service_tier.clone();
        let attempt_map = attempt_context.clone();

        let mapped = stream
            .flat_map(move |result| {
                let (chunks, stop) = match result {
                    StreamRead::Chunk(chunk) => {
                        let stopped = {
                            let state = st_map.lock();
                            state.error || state.terminal
                        } || converter_map.lock().is_terminal();
                        if stopped {
                            (Vec::new(), true)
                        } else {
                            process_chunk_for_usage(
                                &mut st_map.lock(),
                                upstream_format,
                                &chunk,
                                Some(&model_for_stream),
                            );
                            let converted = converter_map.lock().process_chunk(chunk);
                            match converted {
                                Ok(chunks) => (chunks, false),
                                Err(error) => {
                                    let detail =
                                        format!("stream conversion failed: {}", error.message);
                                    let msg = outcome_unknown_message(&detail);
                                    {
                                        let mut state = st_map.lock();
                                        state.error = true;
                                        state.outcome_unknown = true;
                                        state.error_message = Some(msg.clone());
                                        state.diagnostic_recorded = true;
                                    }
                                    let chunks = converter_map.lock().outcome_unknown_event(&msg);
                                    let failure = attempt_map.failure(FailureSpec {
                                        error_source: "gateway",
                                        error_stage: "response_transform",
                                        downstream_status: Some(status.as_u16()),
                                        upstream_status: Some(status.as_u16()),
                                        upstream_wait_ms: Some(upstream_wait_ms),
                                        retry_action: Some("return"),
                                        upstream_headers: None,
                                        upstream_error: Some(&detail),
                                        request_body: None,
                                    });
                                    let diagnostic = failure.update();
                                    let db = state_h.db.lock();
                                    let _ = db.update_forward_log(
                                        initial_id,
                                        "outcome_unknown",
                                        None,
                                        metadata_metrics(
                                            &pricing_map,
                                            service_tier_map.as_deref(),
                                            "outcome_unknown",
                                        ),
                                        Some(&msg),
                                        Some(&diagnostic),
                                    );
                                    (chunks, true)
                                }
                            }
                        }
                    }
                    StreamRead::Failed(error) => {
                        if converter_map.lock().is_terminal() {
                            (Vec::new(), true)
                        } else {
                            let detail = format!("upstream stream interrupted: {error}");
                            let msg = outcome_unknown_message(&detail);
                            {
                                let mut state = st_map.lock();
                                state.error = true;
                                state.outcome_unknown = true;
                                state.error_message = Some(msg.clone());
                                state.diagnostic_recorded = true;
                            }
                            let chunks = converter_map.lock().outcome_unknown_event(&msg);
                            let failure = attempt_map.failure(FailureSpec {
                                error_source: "transport",
                                error_stage: "stream",
                                downstream_status: Some(status.as_u16()),
                                upstream_status: Some(status.as_u16()),
                                upstream_wait_ms: Some(upstream_wait_ms),
                                retry_action: Some("return"),
                                upstream_headers: None,
                                upstream_error: Some(&detail),
                                request_body: None,
                            });
                            let diagnostic = failure.update();
                            let db = state_h.db.lock();
                            let _ = db.update_forward_log(
                                initial_id,
                                "outcome_unknown",
                                None,
                                metadata_metrics(
                                    &pricing_map,
                                    service_tier_map.as_deref(),
                                    "outcome_unknown",
                                ),
                                Some(&msg),
                                Some(&diagnostic),
                            );
                            (chunks, true)
                        }
                    }
                    StreamRead::IdleTimeout => {
                        if converter_map.lock().is_terminal() {
                            (Vec::new(), true)
                        } else {
                            let detail = format!(
                                "upstream stream idle timeout after {}s",
                                stream_idle_timeout_secs
                            );
                            let msg = outcome_unknown_message(&detail);
                            {
                                let mut state = st_map.lock();
                                state.error = true;
                                state.outcome_unknown = true;
                                state.error_message = Some(msg.clone());
                                state.diagnostic_recorded = true;
                            }
                            let chunks = converter_map.lock().outcome_unknown_event(&msg);
                            let failure = attempt_map.failure(FailureSpec {
                                error_source: "transport",
                                error_stage: "stream",
                                downstream_status: Some(status.as_u16()),
                                upstream_status: Some(status.as_u16()),
                                upstream_wait_ms: Some(upstream_wait_ms),
                                retry_action: Some("return"),
                                upstream_headers: None,
                                upstream_error: Some(&detail),
                                request_body: None,
                            });
                            let diagnostic = failure.update();
                            let db = state_h.db.lock();
                            let _ = db.update_forward_log(
                                initial_id,
                                "outcome_unknown",
                                None,
                                metadata_metrics(
                                    &pricing_map,
                                    service_tier_map.as_deref(),
                                    "outcome_unknown",
                                ),
                                Some(&msg),
                                Some(&diagnostic),
                            );
                            (chunks, true)
                        }
                    }
                };
                let mut items = chunks
                    .into_iter()
                    .map(|chunk| Some(Ok::<bytes::Bytes, std::io::Error>(chunk)))
                    .collect::<Vec<_>>();
                if stop {
                    // The sentinel lets flat_map drain every generated error chunk, then
                    // stops without polling the stalled upstream body for another item.
                    items.push(None);
                }
                futures_util::stream::iter(items)
            })
            .take_while(|item| futures_util::future::ready(item.is_some()))
            .map(|item| item.expect("stream stop sentinel should be filtered"));

        // Finalizer runs once, after the real stream is fully drained. It updates
        // the streaming row with final token counts and cost (or marks
        // success_no_usage if the upstream never sent a usage chunk).
        let finalizer = {
            let db_h = state.clone();
            let st_f = st.clone();
            let converter_f = converter.clone();
            let mdl = model.clone();
            let service_tier_f = plan.service_tier.clone();
            let pricing_f = pricing_snapshot.clone();
            let attempt_f = attempt_context.clone();
            let stream_guard = StreamOutcomeGuard::new(
                state.clone(),
                initial_id,
                st.clone(),
                model.clone(),
                pricing_snapshot.clone(),
                plan.service_tier.clone(),
                attempt_context.clone(),
                status.as_u16(),
                upstream_wait_ms,
            );
            // `unfold` is a clean "run once, then end" stream. The DB write is the
            // unfold's state transition, the body emits a single empty chunk, and
            // the stream then terminates — no need for once() + flatten gymnastics.
            futures_util::stream::unfold(
                FinalizerState::Init {
                    db_h,
                    st_f,
                    converter_f,
                    mdl,
                    initial_id,
                    guard: Box::new(stream_guard),
                },
                move |state| {
                    let service_tier = service_tier_f.clone();
                    let pricing = pricing_f.clone();
                    let attempt = attempt_f.clone();
                    async move {
                        let (db_h, st_f, converter_f, mdl, initial_id, mut guard) = match state {
                            FinalizerState::Init {
                                db_h,
                                st_f,
                                converter_f,
                                mdl,
                                initial_id,
                                guard,
                            } => (db_h, st_f, converter_f, mdl, initial_id, guard),
                            FinalizerState::Done => return None,
                        };
                        let (output, finish_error) = if st_f.lock().error {
                            (bytes::Bytes::new(), None)
                        } else {
                            let mut converter = converter_f.lock();
                            match converter.finish() {
                                Ok(chunks) => (join_chunks(chunks), None),
                                Err(error) => {
                                    let detail = format!(
                                        "upstream stream ended before a complete response: {}",
                                        error.message
                                    );
                                    let message = outcome_unknown_message(&detail);
                                    {
                                        let mut state = st_f.lock();
                                        state.error = true;
                                        state.outcome_unknown = true;
                                        state.error_message = Some(message.clone());
                                    }
                                    let chunks = converter.outcome_unknown_event(&message);
                                    (join_chunks(chunks), Some(message))
                                }
                            }
                        };
                        let stream_error = st_f.lock().error_message.clone();
                        let diagnostic_recorded = st_f.lock().diagnostic_recorded;
                        let (status_str, metrics) = {
                            let g = st_f.lock();
                            if g.error {
                                let status = if g.outcome_unknown {
                                    "outcome_unknown"
                                } else {
                                    "error"
                                };
                                (
                                    status.to_string(),
                                    metadata_metrics(
                                        &pricing,
                                        service_tier.as_deref(),
                                        if g.outcome_unknown {
                                            "outcome_unknown"
                                        } else {
                                            "not_applicable"
                                        },
                                    ),
                                )
                            } else if g.has_usage {
                                let (p, c, cached, cache_creation) = token_counts(g.usage);
                                let metrics = pricing_metrics(
                                    &pricing,
                                    &mdl,
                                    p,
                                    c,
                                    cached,
                                    cache_creation,
                                    service_tier.as_deref(),
                                );
                                let status = if metrics.cost_state == "priced" {
                                    "success"
                                } else {
                                    "success_unpriced"
                                };
                                (status.to_string(), metrics)
                            } else {
                                (
                                    "success_no_usage".to_string(),
                                    metadata_metrics(
                                        &pricing,
                                        service_tier.as_deref(),
                                        "usage_missing",
                                    ),
                                )
                            }
                        };
                        let failure = if diagnostic_recorded {
                            None
                        } else if let Some(error) = finish_error.as_deref() {
                            Some(attempt.failure(FailureSpec {
                                error_source: "gateway",
                                error_stage: "response_transform",
                                downstream_status: Some(status.as_u16()),
                                upstream_status: Some(status.as_u16()),
                                upstream_wait_ms: Some(upstream_wait_ms),
                                retry_action: Some("return"),
                                upstream_headers: None,
                                upstream_error: Some(error),
                                request_body: None,
                            }))
                        } else {
                            stream_error.as_deref().map(|error| {
                                attempt.failure(FailureSpec {
                                    error_source: "upstream",
                                    error_stage: "stream",
                                    downstream_status: Some(status.as_u16()),
                                    upstream_status: Some(status.as_u16()),
                                    upstream_wait_ms: Some(upstream_wait_ms),
                                    retry_action: Some("return"),
                                    upstream_headers: None,
                                    upstream_error: Some(error),
                                    request_body: None,
                                })
                            })
                        };
                        let diagnostic = failure.as_ref().map(FailureRecord::update);
                        let db = db_h.db.lock();
                        if let Err(e) = db.update_forward_log(
                            initial_id,
                            &status_str,
                            None,
                            metrics,
                            finish_error.as_deref().or(stream_error.as_deref()),
                            diagnostic.as_ref(),
                        ) {
                            let _ = db.log_gateway(
                                "warn",
                                "forwarder",
                                &format!("failed to finalize streaming row {}: {}", initial_id, e),
                            );
                        }
                        guard.disarm();
                        Some((
                            Ok::<bytes::Bytes, std::io::Error>(output),
                            FinalizerState::Done,
                        ))
                    }
                },
            )
        };

        Ok(ForwardResult {
            response: response_builder.body(Body::from_stream(mapped.chain(finalizer)))?,
            action: ForwardAction::Return,
            error_message: None,
        })
    } else {
        let text = match response_text_with_timeout(upstream_resp, body_timeout, None).await {
            Ok(text) => text,
            Err(error) => {
                let downstream_status = if error.is_timeout() {
                    StatusCode::GATEWAY_TIMEOUT
                } else {
                    StatusCode::BAD_GATEWAY
                };
                let detail = error.into_detail();
                let error_message = outcome_unknown_message(&detail);
                let failure = attempt_context.failure(FailureSpec {
                    error_source: "transport",
                    error_stage: "response_body",
                    downstream_status: Some(downstream_status.as_u16()),
                    upstream_status: Some(status.as_u16()),
                    upstream_wait_ms: Some(upstream_wait_ms),
                    retry_action: Some("return"),
                    upstream_headers: None,
                    upstream_error: Some(&detail),
                    request_body: Some(client_body),
                });
                {
                    let db = state.db.lock();
                    log_forward(
                        &db,
                        account,
                        &model,
                        "outcome_unknown",
                        Some(status.as_u16() as i32),
                        metadata_metrics(
                            &pricing_snapshot,
                            plan.service_tier.as_deref(),
                            "outcome_unknown",
                        ),
                        Some(&error_message),
                        &attempt_context,
                        Some(failure),
                    )?;
                }
                return Ok(ForwardResult {
                    response: outcome_unknown_response(plan.client, downstream_status, &detail),
                    action: ForwardAction::Return,
                    error_message: Some(error_message),
                });
            }
        };
        let upstream_json = match serde_json::from_str::<Value>(&text) {
            Ok(value) => value,
            Err(_) => {
                let message = "upstream returned invalid JSON";
                let failure = attempt_context.failure(FailureSpec {
                    error_source: "upstream",
                    error_stage: "response_body",
                    downstream_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                    upstream_status: Some(status.as_u16()),
                    upstream_wait_ms: Some(upstream_wait_ms),
                    retry_action: Some("return"),
                    upstream_headers: None,
                    upstream_error: Some(&text),
                    request_body: Some(client_body),
                });
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "error",
                    Some(status.as_u16() as i32),
                    metadata_metrics(
                        &pricing_snapshot,
                        plan.service_tier.as_deref(),
                        "not_applicable",
                    ),
                    Some(message),
                    &attempt_context,
                    Some(failure),
                )?;
                return Ok(ForwardResult {
                    response: error_response(plan.client, message, None),
                    action: ForwardAction::Return,
                    error_message: Some(message.to_string()),
                });
            }
        };

        let metrics = if has_complete_usage(plan.upstream, &upstream_json) {
            let usage = extract_usage(plan.upstream, &upstream_json, Some(&model));
            let (prompt_tokens, completion_tokens, cached_tokens, cache_creation_tokens) =
                token_counts(usage);
            pricing_metrics(
                &pricing_snapshot,
                &model,
                prompt_tokens,
                completion_tokens,
                cached_tokens,
                cache_creation_tokens,
                plan.service_tier.as_deref(),
            )
        } else {
            metadata_metrics(
                &pricing_snapshot,
                plan.service_tier.as_deref(),
                "usage_missing",
            )
        };
        let response_json = match transform_response(plan, &upstream_json) {
            Ok(value) => value,
            Err(error) => {
                let message = format!("response conversion failed: {}", error.message);
                let failure = attempt_context.failure(FailureSpec {
                    error_source: "gateway",
                    error_stage: "response_transform",
                    downstream_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                    upstream_status: Some(status.as_u16()),
                    upstream_wait_ms: Some(upstream_wait_ms),
                    retry_action: Some("return"),
                    upstream_headers: None,
                    upstream_error: Some(&message),
                    request_body: Some(client_body),
                });
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "error",
                    Some(status.as_u16() as i32),
                    metrics.clone(),
                    Some(&message),
                    &attempt_context,
                    Some(failure),
                )?;
                return Ok(ForwardResult {
                    response: error_response(plan.client, &message, Some(&upstream_json)),
                    action: ForwardAction::Return,
                    error_message: Some(message),
                });
            }
        };

        {
            let db = state.db.lock();
            log_forward(
                &db,
                account,
                &model,
                match metrics.cost_state {
                    "priced" => "success",
                    "usage_missing" => "success_no_usage",
                    _ => "success_unpriced",
                },
                Some(status.as_u16() as i32),
                metrics,
                None,
                &attempt_context,
                None,
            )?;
        }

        Ok(ForwardResult {
            response: (status, axum::Json(response_json)).into_response(),
            action: ForwardAction::Return,
            error_message: None,
        })
    }
}

struct StreamOutcomeGuard {
    state: CoreState,
    log_id: i64,
    stream_state: Arc<Mutex<StreamState>>,
    model: String,
    pricing: Arc<PricingSnapshot>,
    service_tier: Option<String>,
    attempt_context: ForwardAttemptContext,
    upstream_status: u16,
    upstream_wait_ms: u64,
    armed: bool,
}

impl StreamOutcomeGuard {
    #[allow(clippy::too_many_arguments)]
    fn new(
        state: CoreState,
        log_id: i64,
        stream_state: Arc<Mutex<StreamState>>,
        model: String,
        pricing: Arc<PricingSnapshot>,
        service_tier: Option<String>,
        attempt_context: ForwardAttemptContext,
        upstream_status: u16,
        upstream_wait_ms: u64,
    ) -> Self {
        Self {
            state,
            log_id,
            stream_state,
            model,
            pricing,
            service_tier,
            attempt_context,
            upstream_status,
            upstream_wait_ms,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StreamOutcomeGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }

        let (status, metrics, error_message, failure) = {
            let stream = self.stream_state.lock();
            if !stream.terminal && !stream.error {
                let message = outcome_unknown_message(
                    "downstream disconnected before the upstream stream outcome was confirmed",
                );
                let failure = self.attempt_context.failure(FailureSpec {
                    error_source: "downstream",
                    error_stage: "downstream_disconnect",
                    downstream_status: Some(self.upstream_status),
                    upstream_status: Some(self.upstream_status),
                    upstream_wait_ms: Some(self.upstream_wait_ms),
                    retry_action: Some("return"),
                    upstream_headers: None,
                    upstream_error: Some(&message),
                    request_body: None,
                });
                (
                    "outcome_unknown",
                    metadata_metrics(
                        &self.pricing,
                        self.service_tier.as_deref(),
                        "outcome_unknown",
                    ),
                    Some(message),
                    Some(failure),
                )
            } else if stream.error {
                let status = if stream.outcome_unknown {
                    "outcome_unknown"
                } else {
                    "error"
                };
                let failure = (!stream.diagnostic_recorded).then(|| {
                    let error = stream
                        .error_message
                        .as_deref()
                        .unwrap_or("upstream stream error");
                    self.attempt_context.failure(FailureSpec {
                        error_source: "upstream",
                        error_stage: "stream",
                        downstream_status: Some(self.upstream_status),
                        upstream_status: Some(self.upstream_status),
                        upstream_wait_ms: Some(self.upstream_wait_ms),
                        retry_action: Some("return"),
                        upstream_headers: None,
                        upstream_error: Some(error),
                        request_body: None,
                    })
                });
                (
                    status,
                    metadata_metrics(
                        &self.pricing,
                        self.service_tier.as_deref(),
                        if stream.outcome_unknown {
                            "outcome_unknown"
                        } else {
                            "not_applicable"
                        },
                    ),
                    stream.error_message.clone(),
                    failure,
                )
            } else if stream.has_usage {
                let (prompt, completion, cached, cache_creation) = token_counts(stream.usage);
                let metrics = pricing_metrics(
                    &self.pricing,
                    &self.model,
                    prompt,
                    completion,
                    cached,
                    cache_creation,
                    self.service_tier.as_deref(),
                );
                let status = if metrics.cost_state == "priced" {
                    "success"
                } else {
                    "success_unpriced"
                };
                (status, metrics, None, None)
            } else {
                (
                    "success_no_usage",
                    metadata_metrics(&self.pricing, self.service_tier.as_deref(), "usage_missing"),
                    None,
                    None,
                )
            }
        };

        let diagnostic = failure.as_ref().map(FailureRecord::update);
        let db = self.state.db.lock();
        if let Err(error) = db.update_forward_log(
            self.log_id,
            status,
            None,
            metrics,
            error_message.as_deref(),
            diagnostic.as_ref(),
        ) {
            let _ = db.log_gateway(
                "warn",
                "forwarder",
                &format!(
                    "failed to finalize dropped streaming row {}: {}",
                    self.log_id, error
                ),
            );
        }
    }
}

// `unfold` with an Init/Done state runs the normal finalizer once. The guard
// handles the complementary path where Hyper drops the body because the
// downstream client disconnected before polling that finalizer.
enum FinalizerState {
    Init {
        db_h: CoreState,
        st_f: Arc<Mutex<StreamState>>,
        converter_f: Arc<Mutex<StreamConverter>>,
        mdl: String,
        initial_id: i64,
        guard: Box<StreamOutcomeGuard>,
    },
    Done,
}

enum StreamRead {
    Chunk(bytes::Bytes),
    Failed(reqwest::Error),
    IdleTimeout,
}

fn join_chunks(chunks: Vec<bytes::Bytes>) -> bytes::Bytes {
    let capacity = chunks.iter().map(bytes::Bytes::len).sum();
    let mut joined = BytesMut::with_capacity(capacity);
    for chunk in chunks {
        joined.extend_from_slice(&chunk);
    }
    joined.freeze()
}

/// Simple GET forward for endpoints like /v1/models — uses configured selection strategy.
pub async fn forward_get(
    client: &Client,
    state: &CoreState,
    config: &AppConfig,
    upstream_path: &str,
) -> Result<Response> {
    ensure_safe_upstream_base_url(&config.upstream_base_url)?;
    let selector = AccountSelector::new();
    let mut failed_ids = Vec::new();
    let mut last_http_error = None;
    let mut last_transport_error = None;

    loop {
        let account = {
            let db = state.db.lock();
            let excluded = failed_ids.iter().map(String::as_str).collect::<Vec<_>>();
            selector.select_excluding(&db, &excluded)?
        };
        let Some(account) = account else {
            if let Some(until) = state.db.lock().soonest_cooldown_reset()? {
                return Ok(rate_limited_response(ApiFormat::ChatCompletions, until));
            }
            if let Some((status, body)) = last_http_error {
                let mut headers = HeaderMap::new();
                headers.insert("content-type", HeaderValue::from_static("application/json"));
                return Ok((status, headers, body).into_response());
            }
            if let Some(error) = last_transport_error {
                return Err(error);
            }
            anyhow::bail!("no enabled accounts available");
        };

        let key = match state.decrypt_key(&account.key_cipher) {
            Ok(key) => key,
            Err(error) => {
                last_transport_error = Some(error);
                failed_ids.push(account.id);
                continue;
            }
        };
        let authorization = match HeaderValue::from_str(&format!("Bearer {key}")) {
            Ok(value) => value,
            Err(error) => {
                last_transport_error = Some(anyhow::anyhow!(
                    "account key is not a valid upstream header value: {error}"
                ));
                failed_ids.push(account.id);
                continue;
            }
        };
        let url = format!(
            "{}{}",
            config.upstream_base_url.trim_end_matches('/'),
            upstream_path
        );
        let mut retried_same_account = false;

        loop {
            let resp = match client
                .get(&url)
                .header(reqwest::header::AUTHORIZATION, authorization.clone())
                .timeout(StdDuration::from_secs(config.non_stream_timeout_secs))
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    // A connect error occurs before an HTTP response exists and
                    // is the only transport failure safe to repeat. Retry this
                    // account once, then return the failure without trying a
                    // different account. Any post-connect failure is ambiguous.
                    if error.is_connect() && !retried_same_account {
                        retried_same_account = true;
                        continue;
                    }
                    return Err(error.into());
                }
            };

            let status = resp.status();
            let body = match resp.text().await {
                Ok(body) => body,
                Err(error) => {
                    // Headers may already represent a completed upstream call;
                    // never replay when reading the response body fails.
                    return Err(anyhow::anyhow!(response_body_error(&error)));
                }
            };

            if status.as_u16() == 429 {
                let db = state.db.lock();
                let cooldown = parse_reset(&body).unwrap_or_else(|| Duration::minutes(5));
                db.set_account_rate_limit_if_key_matches(
                    &account.id,
                    &account.key_cipher,
                    Utc::now() + cooldown,
                    &body,
                    parse_usage_limit_window(&body),
                )?;
            }
            if status == StatusCode::UNAUTHORIZED {
                let auth_error = format!(
                    "upstream auth error 401: {}",
                    sanitize_upstream_error(&body)
                );
                state.db.lock().set_account_auth_error_if_key_matches(
                    &account.id,
                    &account.key_cipher,
                    Some(&auth_error),
                )?;
            }
            if matches!(status.as_u16(), 401 | 403 | 429) {
                last_http_error = Some((status, body));
                failed_ids.push(account.id.clone());
                break;
            }

            let mut headers = HeaderMap::new();
            headers.insert("content-type", HeaderValue::from_static("application/json"));
            let mut response = (status, headers, body).into_response();
            if status == StatusCode::PAYLOAD_TOO_LARGE {
                response
                    .extensions_mut()
                    .insert(UpstreamPayloadTooLargeResponse);
            }
            return Ok(response);
        }
    }
}

fn ensure_safe_upstream_base_url(base: &str) -> Result<()> {
    let url = reqwest::Url::parse(base)?;
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_loopback_host(&url) => Ok(()),
        scheme => anyhow::bail!("unsafe upstream scheme or host: {}", scheme),
    }
}

fn is_loopback_host(url: &reqwest::Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
    )
}

fn sanitize_upstream_error(text: &str) -> String {
    let safe = sanitize_upstream_error_value(text);
    safe.get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| safe.to_string())
        .chars()
        .take(500)
        .collect()
}

fn response_body_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "upstream response body timed out".to_string()
    } else {
        format!("upstream response body failed: {error}")
    }
}

#[derive(Debug)]
enum ResponseBodyFailure {
    IdleTimeout(StdDuration),
    Transport(reqwest::Error),
}

impl ResponseBodyFailure {
    fn is_timeout(&self) -> bool {
        match self {
            Self::IdleTimeout(_) => true,
            Self::Transport(error) => error.is_timeout(),
        }
    }

    fn into_detail(self) -> String {
        match self {
            Self::IdleTimeout(timeout) => format!(
                "upstream response body timed out after {}s",
                timeout.as_secs()
            ),
            Self::Transport(error) => response_body_error(&error),
        }
    }
}

async fn response_text_with_timeout(
    response: reqwest::Response,
    timeout: Option<StdDuration>,
    max_bytes: Option<usize>,
) -> std::result::Result<String, ResponseBodyFailure> {
    let read = response_text(response, max_bytes);
    match timeout {
        Some(timeout) => match tokio::time::timeout(timeout, read).await {
            Ok(result) => result.map_err(ResponseBodyFailure::Transport),
            Err(_) => Err(ResponseBodyFailure::IdleTimeout(timeout)),
        },
        None => read.await.map_err(ResponseBodyFailure::Transport),
    }
}

async fn response_text(
    response: reqwest::Response,
    max_bytes: Option<usize>,
) -> std::result::Result<String, reqwest::Error> {
    let Some(max_bytes) = max_bytes else {
        return response.text().await;
    };

    let read_limit = max_bytes.saturating_add(1);
    let capacity = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .map_or(read_limit, |length| length.min(read_limit));
    let mut body = Vec::with_capacity(capacity);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let remaining = read_limit.saturating_sub(body.len());
        if remaining == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if body.len() == read_limit {
            break;
        }
    }

    let truncated = body.len() > max_bytes;
    body.truncate(max_bytes);
    let mut text = String::from_utf8_lossy(&body).into_owned();
    if truncated {
        text.push_str("\n<upstream error body truncated>");
    }
    Ok(text)
}

fn error_response(format: ApiFormat, message: &str, upstream: Option<&Value>) -> Response {
    let body = format_error(format, StatusCode::BAD_GATEWAY, message, upstream);
    (StatusCode::BAD_GATEWAY, axum::Json(body)).into_response()
}

fn account_preflight_failure(plan: &RequestPlan, message: String) -> ForwardResult {
    ForwardResult {
        response: error_response(plan.client, &message, None),
        action: ForwardAction::TryNextAccount,
        error_message: Some(message),
    }
}

fn protocol_status_error_response(
    format: ApiFormat,
    status: StatusCode,
    message: &str,
    upstream: Option<&Value>,
) -> Response {
    let body = format_error(format, status, message, upstream);
    (status, axum::Json(body)).into_response()
}

fn outcome_unknown_message(detail: &str) -> String {
    format!(
        "upstream outcome is unknown: {detail}; the request may have completed and consumed quota; the gateway did not retry it"
    )
}

fn outcome_unknown_response(format: ApiFormat, status: StatusCode, detail: &str) -> Response {
    let message = outcome_unknown_message(detail);
    let mut body = error_body(format, "upstream_outcome_unknown", &message);
    if format == ApiFormat::Gemini {
        body["error"]["code"] = serde_json::json!(status.as_u16());
        body["error"]["status"] = serde_json::json!("UPSTREAM_OUTCOME_UNKNOWN");
    }
    (status, axum::Json(body)).into_response()
}

pub(crate) fn rate_limited_response(
    format: ApiFormat,
    resets_at: chrono::DateTime<Utc>,
) -> Response {
    let message = format!(
        "all accounts rate-limited, soonest resets at {}",
        resets_at.to_rfc3339()
    );
    let mut body = format_error(format, StatusCode::TOO_MANY_REQUESTS, &message, None);
    body["error"]["resets_at"] = serde_json::json!(resets_at.to_rfc3339());
    (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response()
}

#[allow(clippy::too_many_arguments)]
fn log_forward(
    db: &Database,
    account: &Account,
    model: &str,
    status: &str,
    http_status: Option<i32>,
    metrics: ForwardMetrics,
    error_message: Option<&str>,
    context: &ForwardAttemptContext,
    failure: Option<FailureRecord>,
) -> Result<i64> {
    let cost_state = match (metrics.cost_state, status) {
        ("not_applicable", "outcome_unknown") => "outcome_unknown",
        ("not_applicable", "success_no_usage") => "usage_missing",
        ("not_applicable", "success_unpriced") => "unpriced",
        (state, _) => state,
    };
    let failure_value = failure
        .as_ref()
        .and_then(|failure| serde_json::from_str(&failure.diagnostic_json).ok());
    db.log_forward(&ForwardLog {
        id: 0,
        timestamp: Utc::now(),
        model: model.to_string(),
        account_id: account.id.clone(),
        account_name: account.name.clone(),
        status: status.to_string(),
        http_status,
        prompt_tokens: metrics.prompt_tokens,
        completion_tokens: metrics.completion_tokens,
        cached_tokens: metrics.cached_tokens,
        cache_creation_tokens: metrics.cache_creation_tokens,
        cost: (cost_state == "priced").then_some(metrics.cost),
        pricing_revision_id: metrics.pricing_revision_id,
        quota_multiplier: metrics.quota_multiplier,
        local_adjustment_multiplier: metrics.local_adjustment_multiplier,
        service_tier: metrics.service_tier,
        cost_state: cost_state.to_string(),
        error_message: error_message.map(|s| s.to_string()),
        request_id: Some(context.trace.request_id.clone()),
        attempt: Some(context.attempt as i64),
        error_source: failure.as_ref().map(|failure| failure.error_source.clone()),
        error_stage: failure.as_ref().map(|failure| failure.error_stage.clone()),
        duration_ms: failure.as_ref().map(|failure| failure.duration_ms),
        diagnostic: failure_value,
    })
}

fn pricing_metrics(
    snapshot: &PricingSnapshot,
    model: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
    cached_tokens: i64,
    cache_creation_tokens: i64,
    service_tier: Option<&str>,
) -> ForwardMetrics {
    let estimate = snapshot.estimate(
        model,
        prompt_tokens,
        completion_tokens,
        cached_tokens,
        cache_creation_tokens,
        service_tier,
    );
    ForwardMetrics {
        prompt_tokens,
        completion_tokens,
        cached_tokens,
        cache_creation_tokens,
        cost: estimate.cost.unwrap_or(0.0),
        pricing_revision_id: estimate.pricing_revision_id,
        quota_multiplier: estimate.quota_multiplier,
        local_adjustment_multiplier: estimate.local_adjustment_multiplier,
        service_tier: service_tier.map(str::to_string),
        cost_state: estimate.cost_state,
    }
}

fn metadata_metrics(
    snapshot: &PricingSnapshot,
    service_tier: Option<&str>,
    cost_state: &'static str,
) -> ForwardMetrics {
    ForwardMetrics {
        pricing_revision_id: Some(snapshot.revision.clone()),
        service_tier: service_tier.map(str::to_string),
        cost_state,
        ..ForwardMetrics::default()
    }
}

// ----- SSE usage accumulation -----

// ponytail: single Mutex<StreamState> instead of 3 separate Arc<Mutex<>>/
// AtomicBool. Lock is held for a single chunk's processing (microseconds);
// upgrade to per-chunk allocator if cross-stream contention ever shows up.
#[derive(Default)]
struct StreamState {
    buf: BytesMut,
    usage: UsageCounts,
    has_usage: bool,
    terminal: bool,
    /// Set by the mapped Err arm so the finalizer can skip its status overwrite.
    error: bool,
    outcome_unknown: bool,
    error_message: Option<String>,
    diagnostic_recorded: bool,
}

const MAX_SSE_BUF: usize = 64 * 1024;

// ponytail: SSE spec allows \n\n OR \r\n\r\n as event boundaries. Match both
// so Windows-origin / proxy-CRLF upstreams don't accumulate buffer forever.
fn find_event_boundary(buf: &[u8]) -> Option<usize> {
    // \n\n
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some(i);
        }
    }
    // \r\n\r\n
    for i in 0..buf.len().saturating_sub(3) {
        if &buf[i..i + 4] == b"\r\n\r\n" {
            return Some(i);
        }
    }
    None
}

fn event_boundary_len(buf: &[u8], start: usize) -> usize {
    if start + 3 < buf.len() && &buf[start..start + 4] == b"\r\n\r\n" {
        4
    } else {
        2
    }
}

fn extract_data_payload(event: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(event).ok()?;
    let mut parts: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = line.strip_prefix("data:") {
            parts.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

// ponytail: ignore_err on JSON parse — SSE frames may be comments or keep-alive
// heartbeats. Silent skip; the last non-null usage frame still wins.
// ponytail: bounded buffer — if the upstream never sends a complete event
// (malformed stream, CRLF-only chunks, dropped keep-alive framing), drop the
// garbage so memory can't grow unbounded.
fn process_chunk_for_usage(
    st: &mut StreamState,
    format: ApiFormat,
    chunk: &bytes::Bytes,
    model_hint: Option<&str>,
) {
    if st.terminal {
        return;
    }
    if st.buf.len() + chunk.len() > MAX_SSE_BUF {
        st.buf.clear();
        return;
    }
    st.buf.extend_from_slice(chunk);
    loop {
        let bytes = st.buf.as_ref();
        let Some(idx) = find_event_boundary(bytes) else {
            break;
        };
        let take = event_boundary_len(bytes, idx);
        let event = st.buf.split_to(idx + take);
        if let Some(payload) = extract_data_payload(&event) {
            let payload = payload.trim();
            if payload == "[DONE]" {
                st.terminal = true;
                st.buf.clear();
                break;
            }
            if payload.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(payload) {
                let is_error = matches!(
                    v.get("type").and_then(Value::as_str),
                    Some("error" | "response.failed")
                ) || v.get("error").is_some_and(|error| !error.is_null());
                if is_error {
                    st.error = true;
                    st.error_message = Some(
                        v.pointer("/response/error/message")
                            .or_else(|| v.pointer("/error/message"))
                            .or_else(|| v.get("message"))
                            .and_then(Value::as_str)
                            .unwrap_or("upstream stream error")
                            .to_string(),
                    );
                }
                if has_usage(format, &v) {
                    // Always retain the request model as the hint. Some compatible
                    // upstreams rewrite the response model to a generic alias, and
                    // extract_usage already combines that response model with this
                    // original hint when applying model-specific normalization.
                    merge_stream_usage(format, &v, &mut st.usage, model_hint);
                    st.has_usage = true;
                }
                let event_type = v.get("type").and_then(Value::as_str);
                let is_terminal = is_error
                    || match format {
                        ApiFormat::ChatCompletions => false,
                        ApiFormat::Messages => event_type == Some("message_stop"),
                        ApiFormat::Responses => matches!(
                            event_type,
                            Some("response.completed" | "response.incomplete")
                        ),
                        ApiFormat::Gemini => false,
                    };
                if is_terminal {
                    st.terminal = true;
                    st.buf.clear();
                    break;
                }
            }
        }
    }
}

fn token_counts(usage: UsageCounts) -> (i64, i64, i64, i64) {
    let to_i64 = |value: u64| value.min(i64::MAX as u64) as i64;
    (
        to_i64(usage.input_tokens),
        to_i64(usage.output_tokens),
        to_i64(usage.cached_tokens),
        to_i64(usage.cache_creation_tokens),
    )
}

#[cfg(test)]
mod stream_usage_tests {
    use super::*;
    use bytes::Bytes;

    fn usage_event() -> Vec<u8> {
        b"data: {\"id\":\"x\",\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":20,\"total_tokens\":30,\"prompt_tokens_details\":{\"cached_tokens\":5}}}\n\ndata: [DONE]\n\n".to_vec()
    }

    #[test]
    fn single_chunk_extracts_usage() {
        let mut st = StreamState::default();
        let chunk = Bytes::from(usage_event());
        process_chunk_for_usage(&mut st, ApiFormat::ChatCompletions, &chunk, None);
        assert!(st.has_usage, "usage should be set");
        let (p, c, cached, cache_creation) = token_counts(st.usage);
        assert_eq!(p, 10);
        assert_eq!(c, 20);
        assert_eq!(cached, 5);
        assert_eq!(cache_creation, 0);
        assert!(st.buf.is_empty(), "buffer should drain on full events");
    }

    #[test]
    fn chunk_boundary_handling() {
        let full = usage_event();
        let a = &full[..20];
        let b = &full[20..full.len() - 5];
        let c = &full[full.len() - 5..];

        let mut st = StreamState::default();
        process_chunk_for_usage(
            &mut st,
            ApiFormat::ChatCompletions,
            &Bytes::copy_from_slice(a),
            None,
        );
        process_chunk_for_usage(
            &mut st,
            ApiFormat::ChatCompletions,
            &Bytes::copy_from_slice(b),
            None,
        );
        process_chunk_for_usage(
            &mut st,
            ApiFormat::ChatCompletions,
            &Bytes::copy_from_slice(c),
            None,
        );

        assert!(st.has_usage, "usage should be set after boundary");
        let (p, c, cached, cache_creation) = token_counts(st.usage);
        assert_eq!((p, c, cached, cache_creation), (10, 20, 5, 0));
        assert!(st.buf.is_empty(), "buffer should be empty after all chunks");
    }

    #[test]
    fn no_usage_event_yields_none() {
        let mut st = StreamState::default();
        let payload =
            b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: [DONE]\n\n".to_vec();
        process_chunk_for_usage(
            &mut st,
            ApiFormat::ChatCompletions,
            &Bytes::from(payload),
            None,
        );
        assert!(!st.has_usage, "no usage field means no usage");
        assert!(st.buf.is_empty());
    }

    #[test]
    fn last_non_null_usage_wins() {
        let mut st = StreamState::default();
        let first = b"data: {\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2}}\n\n".to_vec();
        let second = b"data: {\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":200,\"prompt_tokens_details\":{\"cached_tokens\":50}}}\n\n".to_vec();
        process_chunk_for_usage(
            &mut st,
            ApiFormat::ChatCompletions,
            &Bytes::from(first),
            None,
        );
        process_chunk_for_usage(
            &mut st,
            ApiFormat::ChatCompletions,
            &Bytes::from(second),
            None,
        );
        assert!(st.has_usage, "usage set");
        let (p, c, cached, cache_creation) = token_counts(st.usage);
        assert_eq!((p, c, cached, cache_creation), (100, 200, 50, 0));
    }

    #[test]
    fn messages_stream_merges_start_and_delta_usage() {
        let mut st = StreamState::default();
        let start = Bytes::from_static(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":6,\"cache_read_input_tokens\":4,\"cache_creation_input_tokens\":2}}}\n\n",
        );
        let delta = Bytes::from_static(
            b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":7}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &start, None);
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &delta, None);
        assert!(st.has_usage);
        assert_eq!(token_counts(st.usage), (12, 7, 4, 2));
    }

    #[test]
    fn messages_stream_sanitizes_minimax_with_model_hint() {
        // Upstream may omit the model field in message_start; the request plan's
        // model must still be used to sanitize bogus all-cache usage.
        let mut st = StreamState::default();
        let start = Bytes::from_static(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":0,\"output_tokens\":5,\"cache_read_input_tokens\":40500}}}\n\n",
        );
        let delta = Bytes::from_static(
            b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":5}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &start, Some("minimax-m3"));
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &delta, Some("minimax-m3"));
        assert!(st.has_usage);
        let (p, c, cached, _) = token_counts(st.usage);
        assert_eq!(p, 40500, "bogus cache read should be moved back to input");
        assert_eq!(c, 5);
        assert_eq!(cached, 0);
    }

    #[test]
    fn messages_stream_keeps_minimax_request_hint_when_upstream_rewrites_model() {
        let mut st = StreamState::default();
        let start = Bytes::from_static(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"ocg-generic\",\"usage\":{\"input_tokens\":0,\"output_tokens\":5,\"cache_read_input_tokens\":40500}}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &start, Some("minimax-m3"));
        assert!(st.has_usage);
        let (input, output, cached, _) = token_counts(st.usage);
        assert_eq!((input, output, cached), (40500, 5, 0));
    }

    #[test]
    fn messages_stream_sanitizes_minimax_with_mixed_case_model_hint() {
        // OpenCode Go / Qwen Cloud expose MiniMax IDs as "MiniMax-M3". The stream
        // sanitizer must recognize the family regardless of capitalization.
        let mut st = StreamState::default();
        let start = Bytes::from_static(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":0,\"output_tokens\":5,\"cache_read_input_tokens\":40500}}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &start, Some("MiniMax-M3"));
        assert!(st.has_usage);
        let (p, _, cached, _) = token_counts(st.usage);
        assert_eq!(p, 40500, "bogus cache read should be moved back to input");
        assert_eq!(cached, 0);
    }

    #[test]
    fn upstream_stream_error_marks_log_state() {
        let mut st = StreamState::default();
        let event = Bytes::from_static(
            b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"boom\"}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &event, None);
        assert!(st.error);
        assert_eq!(st.error_message.as_deref(), Some("boom"));

        let mut responses = StreamState::default();
        let event = Bytes::from_static(
            b"event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"server_error\",\"message\":\"codex boom\"}}}\n\n",
        );
        process_chunk_for_usage(&mut responses, ApiFormat::Responses, &event, None);
        assert!(responses.error);
        assert_eq!(responses.error_message.as_deref(), Some("codex boom"));
    }

    #[test]
    fn terminal_usage_ignores_late_stream_errors() {
        let mut st = StreamState::default();
        let chunk = Bytes::from_static(
            b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":7,\"output_tokens\":2}}}\n\nevent: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"late\"}}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Responses, &chunk, None);
        assert!(st.terminal);
        assert!(!st.error);
        assert_eq!(token_counts(st.usage), (7, 2, 0, 0));

        let later = Bytes::from_static(
            b"event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"later\"}}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Responses, &later, None);
        assert!(!st.error);
        assert_eq!(token_counts(st.usage), (7, 2, 0, 0));
    }

    #[test]
    fn crlf_event_boundary_is_detected() {
        // \r\n\r\n-terminated event must be split out, not accumulated.
        let mut st = StreamState::default();
        let payload =
            b"data: {\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":11}}\r\n\r\n".to_vec();
        process_chunk_for_usage(
            &mut st,
            ApiFormat::ChatCompletions,
            &Bytes::from(payload),
            None,
        );
        assert!(st.has_usage, "CRLF usage should be parsed");
        let (p, c, _, _) = token_counts(st.usage);
        assert_eq!((p, c), (7, 11));
        assert!(st.buf.is_empty());
    }

    #[test]
    fn buffer_bound_clears_on_oversize() {
        let mut st = StreamState::default();
        // Single chunk larger than MAX_SSE_BUF — must be dropped, not allocated.
        let big = vec![b'x'; MAX_SSE_BUF + 1];
        process_chunk_for_usage(&mut st, ApiFormat::ChatCompletions, &Bytes::from(big), None);
        assert!(st.buf.is_empty(), "oversize chunks are dropped");
        assert!(!st.has_usage);
    }
}
