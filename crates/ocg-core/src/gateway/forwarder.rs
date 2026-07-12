use crate::db::Database;
use crate::gateway::cost::cost_from_counts;
use crate::gateway::limit::parse_reset;
use crate::gateway::protocol::{
    ApiFormat, RequestPlan, UsageCounts, extract_usage, format_error, merge_stream_usage,
    transform_response,
};
use crate::gateway::protocol_stream::StreamConverter;
use crate::gateway::selector::AccountSelector;
use crate::models::{Account, AppConfig, ForwardLog};
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
use std::time::Duration as StdDuration;

pub struct ForwardResult {
    pub response: Response,
    pub success: bool,
    pub retryable: bool,
    pub error_message: Option<String>,
}

pub async fn forward_request(
    client: &Client,
    state: &CoreState,
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
    headers: HeaderMap,
) -> Result<ForwardResult> {
    forward_request_impl(client, state, account, config, plan, headers).await
}

async fn forward_request_impl(
    client: &Client,
    state: &CoreState,
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
    headers: HeaderMap,
) -> Result<ForwardResult> {
    ensure_safe_upstream_base_url(&config.upstream_base_url)?;
    let key = state.decrypt_key(&account.key_cipher)?;
    let mut upstream_headers = reqwest::header::HeaderMap::new();

    // Forward harmless client headers only. Auth and hop-by-hop/private headers
    // belong to the gateway/client boundary, not the upstream request.
    for (name, value) in headers.iter() {
        let header = name.as_str().to_ascii_lowercase();
        if !(matches!(
            header.as_str(),
            "authorization"
                | "x-api-key"
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
        upstream_headers.insert("x-api-key", reqwest::header::HeaderValue::from_str(&key)?);
        if !upstream_headers.contains_key("anthropic-version") {
            upstream_headers.insert(
                "anthropic-version",
                reqwest::header::HeaderValue::from_static("2023-06-01"),
            );
        }
    } else {
        upstream_headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", key))?,
        );
    }
    upstream_headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        reqwest::header::HeaderValue::from_static("identity"),
    );

    let url = format!(
        "{}{}",
        config.upstream_base_url.trim_end_matches('/'),
        plan.upstream.path()
    );

    let model = plan.model.clone();

    let upstream_req = client
        .post(&url)
        .headers(upstream_headers)
        .body(plan.body.clone());
    let upstream_req = if plan.stream {
        upstream_req
    } else {
        upstream_req.timeout(StdDuration::from_secs(config.non_stream_timeout_secs))
    };

    let upstream_resp = match upstream_req.send().await {
        Ok(resp) => resp,
        Err(e) => {
            let error_message = format!("upstream request failed: {}", e);
            {
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "error",
                    None,
                    0,
                    0,
                    0,
                    0.0,
                    Some(&error_message),
                )?;
            }
            return Ok(ForwardResult {
                response: error_response(plan.client, &error_message, None),
                success: false,
                retryable: true,
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
        let text = upstream_resp
            .text()
            .await
            .unwrap_or_else(|error| response_body_error(&error));
        let error_message = format!(
            "upstream error {}: {}",
            status.as_u16(),
            sanitize_upstream_error(&text)
        );
        {
            let db = state.db.lock();
            log_forward(
                &db,
                account,
                &model,
                "error",
                Some(status.as_u16() as i32),
                0,
                0,
                0,
                0.0,
                Some(&error_message),
            )?;
        }
        return Ok(ForwardResult {
            response: error_response(plan.client, &error_message, None),
            success: false,
            retryable: true,
            error_message: Some(error_message),
        });
    }

    if status.is_client_error() {
        let text = upstream_resp
            .text()
            .await
            .unwrap_or_else(|error| response_body_error(&error));

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
                    &db,
                    account,
                    &model,
                    "client_error",
                    Some(429),
                    0,
                    0,
                    0,
                    0.0,
                    Some(&text),
                )?;
                db.set_account_cooldown(&account.id, Some(until), Some(&text))?;
            }
            return Ok(ForwardResult {
                response: error_response(plan.client, &error_message, None),
                success: false,
                retryable: false,
                error_message: Some(error_message),
            });
        }

        if status.as_u16() == 408 {
            let error_message = format!("upstream timeout 408: {}", sanitize_upstream_error(&text));
            {
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "client_error",
                    Some(408),
                    0,
                    0,
                    0,
                    0.0,
                    Some(&error_message),
                )?;
            }
            return Ok(ForwardResult {
                response: error_response(plan.client, &error_message, None),
                success: false,
                retryable: true,
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
            {
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "client_error",
                    Some(status.as_u16() as i32),
                    0,
                    0,
                    0,
                    0.0,
                    Some(&sanitize_upstream_error(&text)),
                )?;
            }
            return Ok(ForwardResult {
                response: error_response(plan.client, &error_message, None),
                success: false,
                retryable: false,
                error_message: Some(error_message),
            });
        }

        // Other 4xx: request-level error. Convert its envelope for the caller,
        // but don't retry another account for the same invalid request.
        {
            let db = state.db.lock();
            log_forward(
                &db,
                account,
                &model,
                "client_error",
                Some(status.as_u16() as i32),
                0,
                0,
                0,
                0.0,
                Some(&sanitize_upstream_error(&text)),
            )?;
        }
        let upstream_error = serde_json::from_str::<Value>(&text).ok();
        let message = sanitize_upstream_error(&text);
        let body = format_error(plan.client, status, &message, upstream_error.as_ref());
        return Ok(ForwardResult {
            response: (status, axum::Json(body)).into_response(),
            success: true,
            retryable: false,
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
                0,
                0,
                0,
                0.0,
                None,
            )?
        };

        let stream = upstream_resp.bytes_stream();
        let state_h = state.clone();
        let st = Arc::new(Mutex::new(StreamState::default()));
        let converter = Arc::new(Mutex::new(StreamConverter::new(plan)));
        let upstream_format = plan.upstream;
        let stream_idle_timeout_secs = config.stream_idle_timeout_secs;

        let st_map = st.clone();
        let converter_map = converter.clone();

        let mapped = stream.flat_map(move |result| {
            let chunks = match result {
                Ok(chunk) => {
                    let stopped = {
                        let state = st_map.lock();
                        state.error || state.terminal
                    } || converter_map.lock().is_terminal();
                    if stopped {
                        Vec::new()
                    } else {
                        process_chunk_for_usage(&mut st_map.lock(), upstream_format, &chunk);
                        let converted = converter_map.lock().process_chunk(chunk);
                        match converted {
                            Ok(chunks) => chunks,
                            Err(error) => {
                                let msg = format!("stream conversion failed: {}", error.message);
                                {
                                    let mut state = st_map.lock();
                                    state.error = true;
                                    state.error_message = Some(msg.clone());
                                }
                                let chunks = converter_map.lock().error_event(&msg);
                                let db = state_h.db.lock();
                                let _ = db.update_forward_log(
                                    initial_id,
                                    "error",
                                    None,
                                    0,
                                    0,
                                    0,
                                    0.0,
                                    Some(&msg),
                                );
                                chunks
                            }
                        }
                    }
                }
                Err(e) => {
                    if converter_map.lock().is_terminal() {
                        return futures_util::stream::iter(Vec::new());
                    }
                    // Update the streaming row to "error" rather than inserting a new
                    // row, then report the failure in the caller's SSE protocol.
                    let msg = if e.is_timeout() {
                        format!(
                            "stream error: upstream stream idle timeout after {}s",
                            stream_idle_timeout_secs
                        )
                    } else {
                        format!("stream error: {e}")
                    };
                    {
                        let mut state = st_map.lock();
                        state.error = true;
                        state.error_message = Some(msg.clone());
                    }
                    let chunks = converter_map.lock().error_event(&msg);
                    let db = state_h.db.lock();
                    let _ =
                        db.update_forward_log(initial_id, "error", None, 0, 0, 0, 0.0, Some(&msg));
                    chunks
                }
            };
            futures_util::stream::iter(
                chunks
                    .into_iter()
                    .map(Ok::<bytes::Bytes, std::io::Error>)
                    .collect::<Vec<_>>(),
            )
        });

        // Finalizer runs once, after the real stream is fully drained. It updates
        // the streaming row with final token counts and cost (or marks
        // success_no_usage if the upstream never sent a usage chunk).
        let finalizer = {
            let db_h = state.clone();
            let st_f = st.clone();
            let converter_f = converter.clone();
            let mdl = model.clone();
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
                },
                |state| async move {
                    let (db_h, st_f, converter_f, mdl, initial_id) = match state {
                        FinalizerState::Init {
                            db_h,
                            st_f,
                            converter_f,
                            mdl,
                            initial_id,
                        } => (db_h, st_f, converter_f, mdl, initial_id),
                        FinalizerState::Done => return None,
                    };
                    let (output, finish_error) = if st_f.lock().error {
                        (bytes::Bytes::new(), None)
                    } else {
                        let mut converter = converter_f.lock();
                        match converter.finish() {
                            Ok(chunks) => (join_chunks(chunks), None),
                            Err(error) => {
                                let message =
                                    format!("stream conversion failed: {}", error.message);
                                {
                                    let mut state = st_f.lock();
                                    state.error = true;
                                    state.error_message = Some(message.clone());
                                }
                                let chunks = converter.error_event(&message);
                                (join_chunks(chunks), Some(message))
                            }
                        }
                    };
                    let stream_error = st_f.lock().error_message.clone();
                    let (status_str, prompt, completion, cached, cost) = {
                        let g = st_f.lock();
                        if g.error {
                            // ponytail: the mapped Err arm already wrote the
                            // 'error' row. Don't overwrite it back to success.
                            ("error".to_string(), 0, 0, 0, 0.0)
                        } else if g.has_usage {
                            let (p, c, cached) = token_counts(g.usage);
                            (
                                "success".to_string(),
                                p,
                                c,
                                cached,
                                cost_from_counts(&mdl, p, c, cached),
                            )
                        } else {
                            ("success_no_usage".to_string(), 0, 0, 0, 0.0)
                        }
                    };
                    let db = db_h.db.lock();
                    if let Err(e) = db.update_forward_log(
                        initial_id,
                        &status_str,
                        None,
                        prompt,
                        completion,
                        cached,
                        cost,
                        finish_error.as_deref().or(stream_error.as_deref()),
                    ) {
                        let _ = db.log_gateway(
                            "warn",
                            "forwarder",
                            &format!("failed to finalize streaming row {}: {}", initial_id, e),
                        );
                    }
                    Some((
                        Ok::<bytes::Bytes, std::io::Error>(output),
                        FinalizerState::Done,
                    ))
                },
            )
        };

        Ok(ForwardResult {
            response: response_builder.body(Body::from_stream(mapped.chain(finalizer)))?,
            success: true,
            retryable: false,
            error_message: None,
        })
    } else {
        let text = match upstream_resp.text().await {
            Ok(text) => text,
            Err(error) => {
                let error_message = response_body_error(&error);
                {
                    let db = state.db.lock();
                    log_forward(
                        &db,
                        account,
                        &model,
                        "error",
                        Some(status.as_u16() as i32),
                        0,
                        0,
                        0,
                        0.0,
                        Some(&error_message),
                    )?;
                }
                return Ok(ForwardResult {
                    response: error_response(plan.client, &error_message, None),
                    success: false,
                    retryable: true,
                    error_message: Some(error_message),
                });
            }
        };
        let upstream_json = match serde_json::from_str::<Value>(&text) {
            Ok(value) => value,
            Err(_) => {
                let message = "upstream returned invalid JSON";
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "error",
                    Some(status.as_u16() as i32),
                    0,
                    0,
                    0,
                    0.0,
                    Some(message),
                )?;
                return Ok(ForwardResult {
                    response: error_response(plan.client, message, None),
                    success: true,
                    retryable: false,
                    error_message: Some(message.to_string()),
                });
            }
        };

        let (prompt_tokens, completion_tokens, cached_tokens) =
            token_counts(extract_usage(plan.upstream, &upstream_json));
        let cost = cost_from_counts(&model, prompt_tokens, completion_tokens, cached_tokens);
        let response_json = match transform_response(plan, &upstream_json) {
            Ok(value) => value,
            Err(error) => {
                let message = format!("response conversion failed: {}", error.message);
                let db = state.db.lock();
                log_forward(
                    &db,
                    account,
                    &model,
                    "error",
                    Some(status.as_u16() as i32),
                    prompt_tokens,
                    completion_tokens,
                    cached_tokens,
                    cost,
                    Some(&message),
                )?;
                return Ok(ForwardResult {
                    response: error_response(plan.client, &message, Some(&upstream_json)),
                    success: true,
                    retryable: false,
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
                "success",
                Some(status.as_u16() as i32),
                prompt_tokens,
                completion_tokens,
                cached_tokens,
                cost,
                None,
            )?;
        }

        Ok(ForwardResult {
            response: (status, axum::Json(response_json)).into_response(),
            success: true,
            retryable: false,
            error_message: None,
        })
    }
}

// ponytail: `unfold` with an Init/Done state is the simplest "run once, then
// end" stream. The DB write is the unfold's transition; one empty chunk is
// yielded so the chain's last poll has something to send; Done terminates.
enum FinalizerState {
    Init {
        db_h: CoreState,
        st_f: Arc<Mutex<StreamState>>,
        converter_f: Arc<Mutex<StreamConverter>>,
        mdl: String,
        initial_id: i64,
    },
    Done,
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
    let account = {
        let db = state.db.lock();
        selector
            .select(&db, None)?
            .ok_or_else(|| anyhow::anyhow!("no enabled accounts available"))
    }?;

    let key = state.decrypt_key(&account.key_cipher)?;
    let url = format!(
        "{}{}",
        config.upstream_base_url.trim_end_matches('/'),
        upstream_path
    );

    let resp = match client
        .get(&url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", key))
        .timeout(StdDuration::from_secs(config.non_stream_timeout_secs))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return Err(e.into()),
    };

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|error| anyhow::anyhow!(response_body_error(&error)))?;

    {
        let db = state.db.lock();
        let category = if status.is_server_error() {
            "error"
        } else if status.is_client_error() {
            "client_error"
        } else {
            "success"
        };
        log_forward(
            &db,
            &account,
            "",
            category,
            Some(status.as_u16() as i32),
            0,
            0,
            0,
            0.0,
            Some(&body),
        )?;
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
    let mut out = String::new();
    for token in text.split_whitespace().take(40) {
        if token.starts_with("sk-") || token.to_ascii_lowercase().contains("bearer") {
            out.push_str("<redacted> ");
        } else {
            out.push_str(token);
            out.push(' ');
        }
    }
    out.trim_end().chars().take(500).collect()
}

fn response_body_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "upstream response body timed out".to_string()
    } else {
        format!("upstream response body failed: {error}")
    }
}

fn error_response(format: ApiFormat, message: &str, upstream: Option<&Value>) -> Response {
    let body = format_error(format, StatusCode::BAD_GATEWAY, message, upstream);
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
) -> Result<i64> {
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
    error_message: Option<String>,
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
fn process_chunk_for_usage(st: &mut StreamState, format: ApiFormat, chunk: &bytes::Bytes) {
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
                    merge_stream_usage(format, &v, &mut st.usage);
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

fn has_usage(format: ApiFormat, payload: &Value) -> bool {
    match format {
        ApiFormat::ChatCompletions => payload.get("usage"),
        ApiFormat::Messages => payload
            .get("usage")
            .or_else(|| payload.pointer("/message/usage")),
        ApiFormat::Responses => payload
            .get("usage")
            .or_else(|| payload.pointer("/response/usage")),
    }
    .is_some_and(Value::is_object)
}

fn token_counts(usage: UsageCounts) -> (i64, i64, i64) {
    let to_i64 = |value: u64| value.min(i64::MAX as u64) as i64;
    (
        to_i64(usage.input_tokens),
        to_i64(usage.output_tokens),
        to_i64(usage.cached_tokens),
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
        process_chunk_for_usage(&mut st, ApiFormat::ChatCompletions, &chunk);
        assert!(st.has_usage, "usage should be set");
        let (p, c, cached) = token_counts(st.usage);
        assert_eq!(p, 10);
        assert_eq!(c, 20);
        assert_eq!(cached, 5);
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
        );
        process_chunk_for_usage(
            &mut st,
            ApiFormat::ChatCompletions,
            &Bytes::copy_from_slice(b),
        );
        process_chunk_for_usage(
            &mut st,
            ApiFormat::ChatCompletions,
            &Bytes::copy_from_slice(c),
        );

        assert!(st.has_usage, "usage should be set after boundary");
        let (p, c, cached) = token_counts(st.usage);
        assert_eq!((p, c, cached), (10, 20, 5));
        assert!(st.buf.is_empty(), "buffer should be empty after all chunks");
    }

    #[test]
    fn no_usage_event_yields_none() {
        let mut st = StreamState::default();
        let payload =
            b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: [DONE]\n\n".to_vec();
        process_chunk_for_usage(&mut st, ApiFormat::ChatCompletions, &Bytes::from(payload));
        assert!(!st.has_usage, "no usage field means no usage");
        assert!(st.buf.is_empty());
    }

    #[test]
    fn last_non_null_usage_wins() {
        let mut st = StreamState::default();
        let first = b"data: {\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2}}\n\n".to_vec();
        let second = b"data: {\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":200,\"prompt_tokens_details\":{\"cached_tokens\":50}}}\n\n".to_vec();
        process_chunk_for_usage(&mut st, ApiFormat::ChatCompletions, &Bytes::from(first));
        process_chunk_for_usage(&mut st, ApiFormat::ChatCompletions, &Bytes::from(second));
        assert!(st.has_usage, "usage set");
        let (p, c, cached) = token_counts(st.usage);
        assert_eq!((p, c, cached), (100, 200, 50));
    }

    #[test]
    fn messages_stream_merges_start_and_delta_usage() {
        let mut st = StreamState::default();
        let start = Bytes::from_static(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":6,\"cache_read_input_tokens\":4}}}\n\n",
        );
        let delta = Bytes::from_static(
            b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":7}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &start);
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &delta);
        assert!(st.has_usage);
        assert_eq!(token_counts(st.usage), (10, 7, 4));
    }

    #[test]
    fn upstream_stream_error_marks_log_state() {
        let mut st = StreamState::default();
        let event = Bytes::from_static(
            b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"boom\"}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Messages, &event);
        assert!(st.error);
        assert_eq!(st.error_message.as_deref(), Some("boom"));

        let mut responses = StreamState::default();
        let event = Bytes::from_static(
            b"event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"server_error\",\"message\":\"codex boom\"}}}\n\n",
        );
        process_chunk_for_usage(&mut responses, ApiFormat::Responses, &event);
        assert!(responses.error);
        assert_eq!(responses.error_message.as_deref(), Some("codex boom"));
    }

    #[test]
    fn terminal_usage_ignores_late_stream_errors() {
        let mut st = StreamState::default();
        let chunk = Bytes::from_static(
            b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":7,\"output_tokens\":2}}}\n\nevent: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"late\"}}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Responses, &chunk);
        assert!(st.terminal);
        assert!(!st.error);
        assert_eq!(token_counts(st.usage), (7, 2, 0));

        let later = Bytes::from_static(
            b"event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"later\"}}}\n\n",
        );
        process_chunk_for_usage(&mut st, ApiFormat::Responses, &later);
        assert!(!st.error);
        assert_eq!(token_counts(st.usage), (7, 2, 0));
    }

    #[test]
    fn crlf_event_boundary_is_detected() {
        // \r\n\r\n-terminated event must be split out, not accumulated.
        let mut st = StreamState::default();
        let payload =
            b"data: {\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":11}}\r\n\r\n".to_vec();
        process_chunk_for_usage(&mut st, ApiFormat::ChatCompletions, &Bytes::from(payload));
        assert!(st.has_usage, "CRLF usage should be parsed");
        let (p, c, _) = token_counts(st.usage);
        assert_eq!((p, c), (7, 11));
        assert!(st.buf.is_empty());
    }

    #[test]
    fn buffer_bound_clears_on_oversize() {
        let mut st = StreamState::default();
        // Single chunk larger than MAX_SSE_BUF — must be dropped, not allocated.
        let big = vec![b'x'; MAX_SSE_BUF + 1];
        process_chunk_for_usage(&mut st, ApiFormat::ChatCompletions, &Bytes::from(big));
        assert!(st.buf.is_empty(), "oversize chunks are dropped");
        assert!(!st.has_usage);
    }
}
