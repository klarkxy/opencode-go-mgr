use crate::gateway::protocol::ApiFormat;
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::Instant;
use uuid::Uuid;

pub const REQUEST_ID_HEADER: &str = "x-ocg-request-id";
pub const DIAGNOSTIC_VERSION: u8 = 1;
pub const MAX_REQUEST_SUMMARY_BYTES: usize = 2 * 1024;
pub const MAX_UPSTREAM_ERROR_BYTES: usize = 2 * 1024;
pub const MAX_DIAGNOSTIC_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone)]
pub struct RequestTrace {
    pub request_id: String,
    started_at: Instant,
}

impl RequestTrace {
    pub fn new() -> Self {
        Self {
            request_id: format!("ocg-{}", Uuid::new_v4()),
            started_at: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorDiagnostic {
    pub version: u8,
    pub request_id: String,
    pub attempt: u32,
    pub error_source: String,
    pub error_stage: String,
    pub client_format: String,
    pub upstream_format: Option<String>,
    pub model: Option<String>,
    pub stream: Option<bool>,
    pub client_body_bytes: Option<usize>,
    pub upstream_body_bytes: Option<usize>,
    pub duration_ms: u64,
    pub upstream_wait_ms: Option<u64>,
    pub downstream_status: Option<u16>,
    pub upstream_status: Option<u16>,
    pub retry_action: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub upstream_headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_summary: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_error: Option<Value>,
    pub truncated: bool,
}

impl ErrorDiagnostic {
    pub fn new(
        trace: &RequestTrace,
        attempt: u32,
        error_source: &str,
        error_stage: &str,
        client_format: ApiFormat,
    ) -> Self {
        Self {
            version: DIAGNOSTIC_VERSION,
            request_id: trace.request_id.clone(),
            attempt,
            error_source: error_source.to_string(),
            error_stage: error_stage.to_string(),
            client_format: api_format_name(client_format).to_string(),
            upstream_format: None,
            model: None,
            stream: None,
            client_body_bytes: None,
            upstream_body_bytes: None,
            duration_ms: trace.elapsed_ms(),
            upstream_wait_ms: None,
            downstream_status: None,
            upstream_status: None,
            retry_action: None,
            upstream_headers: BTreeMap::new(),
            request_summary: None,
            request_fingerprint: None,
            upstream_error: None,
            truncated: false,
        }
    }

    pub fn with_request_summary(mut self, body: &[u8]) -> Self {
        let (summary, fingerprint) = summarize_request(body);
        self.request_summary = Some(summary);
        self.request_fingerprint = Some(fingerprint);
        self
    }

    pub fn with_upstream_error(mut self, text: &str) -> Self {
        self.upstream_error = Some(sanitize_upstream_error_value(text));
        self
    }
}

pub fn api_format_name(format: ApiFormat) -> &'static str {
    match format {
        ApiFormat::ChatCompletions => "chat_completions",
        ApiFormat::Responses => "responses",
        ApiFormat::Messages => "messages",
        ApiFormat::Gemini => "gemini",
    }
}

pub fn safe_upstream_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    const ALLOWED: &[&str] = &[
        "x-request-id",
        "request-id",
        "x-trace-id",
        "x-amzn-trace-id",
        "traceparent",
        "cf-ray",
        "retry-after",
        "content-type",
    ];
    let mut safe = BTreeMap::new();
    for name in ALLOWED {
        if let Some(value) = headers.get(*name).and_then(|value| value.to_str().ok()) {
            safe.insert((*name).to_string(), truncate_text(&redact_text(value), 256));
        }
    }
    safe
}

pub fn serialize_diagnostic(mut diagnostic: ErrorDiagnostic) -> String {
    let mut encoded = serde_json::to_string(&diagnostic).unwrap_or_else(|_| {
        format!(
            "{{\"version\":1,\"request_id\":{},\"error_source\":\"gateway\",\"error_stage\":\"internal\",\"truncated\":true}}",
            serde_json::to_string(&diagnostic.request_id).unwrap_or_else(|_| "\"unknown\"".into())
        )
    });
    if encoded.len() <= MAX_DIAGNOSTIC_BYTES {
        return encoded;
    }

    diagnostic.truncated = true;
    diagnostic.request_summary = diagnostic.request_summary.as_ref().map(|value| {
        json!({
            "summary": truncate_text(&value.to_string(), 768),
            "truncated": true
        })
    });
    diagnostic.upstream_error = diagnostic.upstream_error.as_ref().map(|value| {
        json!({
            "summary": truncate_text(&value.to_string(), 768),
            "truncated": true
        })
    });
    encoded = serde_json::to_string(&diagnostic).unwrap_or_default();
    if encoded.len() <= MAX_DIAGNOSTIC_BYTES {
        return encoded;
    }

    diagnostic.upstream_headers.clear();
    diagnostic.request_summary = None;
    diagnostic.upstream_error = None;
    diagnostic.model = None;
    diagnostic.retry_action = None;
    serde_json::to_string(&diagnostic).unwrap_or_else(|_| {
        format!(
            "{{\"version\":1,\"request_id\":{},\"truncated\":true}}",
            serde_json::to_string(&diagnostic.request_id).unwrap_or_else(|_| "\"unknown\"".into())
        )
    })
}

pub fn emit_failure(diagnostic_json: &str) {
    eprintln!("OCG_REQUEST_ERROR {diagnostic_json}");
}

fn summarize_request(body: &[u8]) -> (Value, String) {
    let fingerprint = sha256_hex(body);
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return (
            json!({
                "kind": "invalid_json",
                "bytes": body.len(),
                "sha256": fingerprint,
            }),
            fingerprint,
        );
    };

    let mut roles = BTreeMap::<String, usize>::new();
    let mut content_types = BTreeMap::<String, usize>::new();
    let mut parts = Vec::<Value>::new();
    let mut total_strings = 0usize;
    let mut total_string_bytes = 0usize;
    collect_shape(
        &value,
        None,
        0,
        &mut roles,
        &mut content_types,
        &mut parts,
        &mut total_strings,
        &mut total_string_bytes,
    );

    let object = value.as_object();
    let mut parameters = Map::new();
    if let Some(object) = object {
        for key in [
            "stream",
            "max_tokens",
            "max_output_tokens",
            "temperature",
            "top_p",
            "top_k",
            "parallel_tool_calls",
        ] {
            if let Some(value) = object
                .get(key)
                .filter(|value| value.is_boolean() || value.is_number())
            {
                parameters.insert(key.to_string(), value.clone());
            }
        }
        for key in ["reasoning_effort", "service_tier"] {
            if let Some(value) = object
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| value.len() <= 32 && value.chars().all(is_safe_label_char))
            {
                parameters.insert(key.to_string(), Value::String(value.to_string()));
            }
        }
    }

    let summary = json!({
        "bytes": body.len(),
        "object": value.is_object(),
        "messages": array_len(object, "messages"),
        "input_items": array_len(object, "input"),
        "tools": array_len(object, "tools"),
        "roles": roles,
        "content_types": content_types,
        "strings": total_strings,
        "string_bytes": total_string_bytes,
        "content_parts": parts,
        "parameters": parameters,
        "sha256": fingerprint,
    });
    if summary.to_string().len() <= MAX_REQUEST_SUMMARY_BYTES {
        (summary, fingerprint)
    } else {
        (
            json!({
                "bytes": body.len(),
                "messages": array_len(object, "messages"),
                "input_items": array_len(object, "input"),
                "tools": array_len(object, "tools"),
                "strings": total_strings,
                "string_bytes": total_string_bytes,
                "sha256": fingerprint,
                "truncated": true,
            }),
            fingerprint,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_shape(
    value: &Value,
    key_hint: Option<&str>,
    depth: usize,
    roles: &mut BTreeMap<String, usize>,
    content_types: &mut BTreeMap<String, usize>,
    parts: &mut Vec<Value>,
    total_strings: &mut usize,
    total_string_bytes: &mut usize,
) {
    if depth > 12 {
        return;
    }
    match value {
        Value::String(text) => {
            *total_strings += 1;
            *total_string_bytes = total_string_bytes.saturating_add(text.len());
            let key = key_hint.unwrap_or_default().to_ascii_lowercase();
            if key == "role" {
                let role = match text.as_str() {
                    "system" | "developer" | "user" | "assistant" | "tool" => text.as_str(),
                    _ => "other",
                };
                *roles.entry(role.to_string()).or_default() += 1;
                return;
            }
            if key == "type" {
                let kind = safe_content_type(text);
                *content_types.entry(kind.to_string()).or_default() += 1;
                return;
            }
            if matches!(key.as_str(), "model" | "service_tier" | "reasoning_effort") {
                return;
            }
            if parts.len() < 24 {
                parts.push(json!({
                    "kind": content_kind(&key),
                    "bytes": text.len(),
                    "sha256": &sha256_hex(text.as_bytes())[..12],
                }));
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_shape(
                    value,
                    key_hint,
                    depth + 1,
                    roles,
                    content_types,
                    parts,
                    total_strings,
                    total_string_bytes,
                );
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                collect_shape(
                    value,
                    Some(key),
                    depth + 1,
                    roles,
                    content_types,
                    parts,
                    total_strings,
                    total_string_bytes,
                );
            }
        }
        _ => {}
    }
}

fn array_len(object: Option<&Map<String, Value>>, key: &str) -> usize {
    object
        .and_then(|object| object.get(key))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn content_kind(key: &str) -> &'static str {
    if key.contains("argument") || key.contains("tool") {
        "tool_data"
    } else if key.contains("url")
        || key.contains("image")
        || key.contains("audio")
        || key.contains("file")
    {
        "resource"
    } else if key.contains("data") {
        "data"
    } else {
        "text"
    }
}

fn safe_content_type(value: &str) -> &'static str {
    match value {
        "text" | "input_text" | "output_text" => "text",
        "image" | "input_image" | "image_url" => "image",
        "audio" | "input_audio" => "audio",
        "video" | "input_video" => "video",
        "tool_use" | "tool_call" | "function_call" => "tool_call",
        "tool_result" | "function_call_output" => "tool_result",
        _ => "other",
    }
}

fn is_safe_label_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')
}

pub(crate) fn sanitize_upstream_error_value(text: &str) -> Value {
    if let Ok(mut value) = serde_json::from_str::<Value>(text) {
        redact_value(&mut value, None);
        let encoded = value.to_string();
        if encoded.len() <= MAX_UPSTREAM_ERROR_BYTES {
            return value;
        }
        return json!({
            "summary": truncate_text(&encoded, MAX_UPSTREAM_ERROR_BYTES.saturating_sub(64)),
            "truncated": true,
        });
    }
    let redacted = redact_text(text);
    json!({
        "text": truncate_text(&redacted, MAX_UPSTREAM_ERROR_BYTES.saturating_sub(64)),
        "truncated": redacted.len() > MAX_UPSTREAM_ERROR_BYTES.saturating_sub(64),
    })
}

fn redact_value(value: &mut Value, key_hint: Option<&str>) {
    if key_hint.is_some_and(is_sensitive_key) {
        *value = Value::String("<redacted>".to_string());
        return;
    }
    if key_hint.is_some_and(is_content_key) {
        let encoded = match value {
            Value::String(text) => text.as_bytes().to_vec(),
            _ => serde_json::to_vec(value).unwrap_or_default(),
        };
        *value = json!({
            "bytes": encoded.len(),
            "sha256": &sha256_hex(&encoded)[..12],
        });
        return;
    }
    match value {
        Value::String(text) => *text = redact_text(text),
        Value::Array(values) => {
            for value in values {
                redact_value(value, key_hint);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                redact_value(value, Some(key));
            }
        }
        _ => {}
    }
}

fn is_content_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('-', "_");
    matches!(
        normalized.as_str(),
        "prompt"
            | "input"
            | "text"
            | "input_text"
            | "output_text"
            | "instructions"
            | "system"
            | "content"
            | "arguments"
            | "tool_arguments"
            | "tool_name"
            | "url"
            | "image_url"
            | "file"
            | "file_data"
            | "base64"
            | "data"
            | "body"
    )
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    let normalized = lower.replace(['-', '_', '.', ' '], "");
    if matches!(
        normalized.as_str(),
        "authorization"
            | "apikey"
            | "token"
            | "cookie"
            | "password"
            | "passwd"
            | "secret"
            | "credential"
            | "bearer"
            | "privatekey"
    ) || [
        "authorization",
        "apikey",
        "password",
        "passwd",
        "secret",
        "secretkey",
        "secretaccesskey",
        "accesskeyid",
        "credential",
        "privatekey",
        "token",
        "accesstoken",
        "refreshtoken",
        "sessiontoken",
        "idtoken",
        "authtoken",
        "bearertoken",
        "apitoken",
        "clienttoken",
    ]
    .iter()
    .any(|suffix| normalized.ends_with(suffix))
    {
        return true;
    }

    let parts = lower
        .split(['-', '_', '.', ' '])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.iter().any(|part| {
        matches!(
            *part,
            "authorization" | "cookie" | "password" | "passwd" | "secret" | "credential" | "bearer"
        )
    }) {
        return true;
    }
    parts.windows(2).any(|pair| {
        matches!(
            pair,
            ["api", "key"]
                | ["private", "key"]
                | ["access", "token"]
                | ["refresh", "token"]
                | ["session", "token"]
                | ["id", "token"]
                | ["auth", "token"]
                | ["bearer", "token"]
                | ["client", "token"]
                | ["token", "value"]
                | ["token", "key"]
        )
    })
}

fn redact_text(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.contains("-----begin") && lower.contains("private key-----") {
        return "<redacted private key>".to_string();
    }

    let mut redact_next_line = false;
    text.lines()
        .map(|line| {
            if redact_next_line {
                if line.trim().is_empty() {
                    return line.to_string();
                }
                redact_next_line = false;
                return "<redacted>".to_string();
            }

            if let Some(index) = sensitive_assignment_start(line) {
                redact_next_line = sensitive_assignment_value_is_empty(line, index);
            }
            redact_text_line(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_text_line(text: &str) -> String {
    if let Some(index) = sensitive_assignment_start(text) {
        return format!("{}<redacted>", &text[..index]);
    }

    let mut output = Vec::new();
    let mut redact_next = false;
    for token in text.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        let label =
            lower.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-'));
        if redact_next {
            if label.is_empty() || matches!(label, "bearer" | "basic") {
                output.push(token.to_string());
            } else {
                output.push("<redacted>".to_string());
                redact_next = false;
            }
        } else if lower.contains("sk-") {
            output.push("<redacted>".to_string());
        } else if let Some(value) = sensitive_assignment_value(&lower) {
            output.push("<redacted>".to_string());
            let value = value
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-'));
            redact_next = value.is_empty() || matches!(value, "bearer" | "basic");
        } else if matches!(label, "bearer" | "basic") {
            output.push(token.to_string());
            redact_next = true;
        } else if is_standalone_sensitive_label(label) {
            output.push(token.to_string());
            output.push("<redacted>".to_string());
            break;
        } else {
            output.push(token.to_string());
        }
    }
    output.join(" ")
}

fn sensitive_assignment_start(text: &str) -> Option<usize> {
    const LABELS: &[&str] = &[
        "proxy-authorization",
        "authorization",
        "set-cookie",
        "cookie",
        "x-api-key",
        "x_api_key",
        "api-key",
        "api_key",
        "api.key",
        "api key",
        "access-token",
        "access_token",
        "access token",
        "refresh-token",
        "refresh_token",
        "refresh token",
        "client-secret",
        "client_secret",
        "client secret",
        "private-key",
        "private_key",
        "private key",
        "password",
        "passwd",
        "credential",
        "secret",
        "token",
    ];

    let lower = text.to_ascii_lowercase();
    let mut earliest = generic_sensitive_assignment_start(&lower);
    for label in LABELS {
        for (index, _) in lower.match_indices(label) {
            let boundary_before = index == 0
                || lower[..index]
                    .chars()
                    .next_back()
                    .is_none_or(|ch| !is_label_char(ch));
            if !boundary_before {
                continue;
            }
            let value_start = index + label.len();
            let remainder = lower[value_start..]
                .trim_start()
                .trim_start_matches(['\"', '\''])
                .trim_start();
            let separator = remainder.chars().next();
            if matches!(separator, Some('=' | ':')) {
                earliest = Some(earliest.map_or(index, |current: usize| current.min(index)));
            }
        }
    }
    earliest
}

fn generic_sensitive_assignment_start(text: &str) -> Option<usize> {
    text.char_indices()
        .filter(|(_, ch)| matches!(ch, '=' | ':'))
        .filter_map(|(separator_index, _)| {
            let prefix = text[..separator_index].trim_end();
            let prefix = prefix.trim_end_matches(['\"', '\'']).trim_end();
            let key_end = prefix.len();
            let key_start = prefix[..key_end]
                .char_indices()
                .rev()
                .find(|(_, ch)| !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-' | '.'))
                .map_or(0, |(index, ch)| index + ch.len_utf8());
            let key = &prefix[key_start..key_end];
            (!key.is_empty() && is_sensitive_key(key)).then_some(key_start)
        })
        .min()
}

fn sensitive_assignment_value_is_empty(text: &str, assignment_start: usize) -> bool {
    text[assignment_start..]
        .char_indices()
        .find(|(_, ch)| matches!(ch, '=' | ':'))
        .is_some_and(|(index, ch)| {
            text[assignment_start + index + ch.len_utf8()..]
                .trim()
                .is_empty()
        })
}

fn is_label_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')
}

fn sensitive_assignment_value(text: &str) -> Option<&str> {
    text.char_indices()
        .filter(|(_, ch)| matches!(ch, '=' | ':'))
        .find_map(|(index, ch)| {
            let key = &text[..index];
            is_sensitive_key(key).then(|| &text[index + ch.len_utf8()..])
        })
}

fn is_standalone_sensitive_label(label: &str) -> bool {
    !label.is_empty() && is_sensitive_key(label)
}

fn truncate_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    text[..end].to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_summary_never_keeps_content_or_tool_arguments() {
        let secret = "private prompt sk-super-secret";
        let body = serde_json::to_vec(&json!({
            "model": "kimi-k2.7-code",
            "stream": true,
            "messages": [{"role": "user", "content": secret}],
            "tools": [{"type": "function", "function": {"name": "private_tool", "arguments": {"password": "hunter2"}}}],
            "image_url": "https://secret.example/token"
        }))
        .unwrap();
        let (summary, fingerprint) = summarize_request(&body);
        let encoded = summary.to_string();
        assert!(!encoded.contains(secret));
        assert!(!encoded.contains("private_tool"));
        assert!(!encoded.contains("hunter2"));
        assert!(!encoded.contains("secret.example"));
        assert_eq!(fingerprint.len(), 64);
        assert!(encoded.len() <= MAX_REQUEST_SUMMARY_BYTES);
        let (again, again_fingerprint) = summarize_request(&body);
        assert_eq!(summary, again);
        assert_eq!(fingerprint, again_fingerprint);
    }

    #[test]
    fn upstream_error_redacts_nested_secrets_and_is_bounded() {
        let value = sanitize_upstream_error_value(
            &json!({
                "error": {
                    "message": "Bearer abc sk-secret",
                    "api_key": "top-secret",
                    "input": "private prompt",
                    "tool_name": "private_tool"
                },
                "password": "hunter2",
                "padding": "x".repeat(8_000)
            })
            .to_string(),
        );
        let encoded = value.to_string();
        assert!(!encoded.contains("abc"));
        assert!(!encoded.contains("sk-secret"));
        assert!(!encoded.contains("top-secret"));
        assert!(!encoded.contains("hunter2"));
        assert!(!encoded.contains("private prompt"));
        assert!(!encoded.contains("private_tool"));
        assert!(encoded.len() <= MAX_UPSTREAM_ERROR_BYTES + 128);

        let plain = sanitize_upstream_error_value(
            "authorization: Bearer abc token=def password=hunter2 cookie: yum sk-last",
        )
        .to_string();
        for secret in ["abc", "def", "hunter2", "yum", "sk-last"] {
            assert!(!plain.contains(secret), "plain error leaked {secret}");
        }
    }

    #[test]
    fn upstream_error_redacts_common_plain_text_secret_boundaries() {
        let cases = [
            (
                "authorization=Bearer bearer-inline-value",
                "bearer-inline-value",
            ),
            (
                "authorization=Basic basic-inline-value",
                "basic-inline-value",
            ),
            (
                "Authorization:Bearer compact-bearer-value",
                "compact-bearer-value",
            ),
            ("prefix (sk-parenthesized-value)", "sk-parenthesized-value"),
            ("\"sk-quoted-value\"", "sk-quoted-value"),
            ("prefix=sk-assignment-value", "sk-assignment-value"),
            ("api_key = separated-key-value", "separated-key-value"),
            (
                "https://example.invalid/?code=bad&api_key=url-query-value",
                "url-query-value",
            ),
            (
                "password=\"correct horse battery staple\"",
                "correct horse battery staple",
            ),
            (
                "Cookie: sid=cookie-session-value; refresh=cookie-refresh-value",
                "cookie-session-value",
            ),
            (
                "Cookie: sid=cookie-session-value; refresh=cookie-refresh-value",
                "cookie-refresh-value",
            ),
            ("private_key=pk-live-private-value", "pk-live-private-value"),
            (
                "database_password=\"correct horse battery staple\"",
                "correct horse battery staple",
            ),
            ("api.key=plain-api-value", "plain-api-value"),
            ("api key: natural-api-value", "natural-api-value"),
            (
                "The password is \"correct horse battery staple\"",
                "correct horse battery staple",
            ),
            (
                "database_password value is correct horse battery staple",
                "correct horse battery staple",
            ),
            (
                "github_token=\"plain credential value\"",
                "plain credential value",
            ),
            ("secretKey=\"plain secret value\"", "plain secret value"),
            (
                "awsSecretAccessKey=\"plain aws secret value\"",
                "plain aws secret value",
            ),
            (
                "password (string): correct horse battery staple",
                "correct horse battery staple",
            ),
        ];
        for (text, secret) in cases {
            let encoded = sanitize_upstream_error_value(text).to_string();
            assert!(
                !encoded.contains(secret),
                "plain error leaked {secret}: {encoded}"
            );
        }

        let encoded = sanitize_upstream_error_value(
            r#"{"message":"authorization=Bearer json-message-value"}"#,
        )
        .to_string();
        assert!(!encoded.contains("json-message-value"));

        let encoded = sanitize_upstream_error_value(
            r#"{"message":"database_password=\"correct horse battery staple\""}"#,
        )
        .to_string();
        assert!(!encoded.contains("correct horse battery staple"));

        let encoded =
            sanitize_upstream_error_value(r#"payload={"password":"correct horse battery staple"}"#)
                .to_string();
        assert!(!encoded.contains("correct horse battery staple"));

        let multiline =
            sanitize_upstream_error_value("password:\ncorrect horse battery staple").to_string();
        assert!(!multiline.contains("correct horse battery staple"));

        let tokenizer =
            sanitize_upstream_error_value("tokenizer failed to encode input").to_string();
        assert!(tokenizer.contains("tokenizer failed to encode input"));

        let token_limit = sanitize_upstream_error_value(
            r#"{"error":{"message":"max_tokens must be <= 4096","max_tokens":8192}}"#,
        );
        assert_eq!(token_limit["error"]["max_tokens"], 8192);
        assert_eq!(
            token_limit["error"]["message"],
            "max_tokens must be <= 4096"
        );

        let echoed_prompt = sanitize_upstream_error_value(
            r#"{"error":{"message":"invalid input","text":"customer SSN 123-45-6789","instructions":"private system prompt"}}"#,
        );
        let encoded = echoed_prompt.to_string();
        assert!(!encoded.contains("123-45-6789"));
        assert!(!encoded.contains("private system prompt"));
        assert_eq!(echoed_prompt["error"]["text"]["bytes"], 24);

        let private_key = sanitize_upstream_error_value(
            "-----BEGIN PRIVATE KEY-----\nprivate-material\n-----END PRIVATE KEY-----",
        )
        .to_string();
        assert!(!private_key.contains("private-material"));
    }

    #[test]
    fn serialized_diagnostic_is_valid_json_and_bounded() {
        let trace = RequestTrace::new();
        let mut diagnostic =
            ErrorDiagnostic::new(&trace, 1, "upstream", "upstream_http", ApiFormat::Responses);
        diagnostic.request_summary = Some(json!({"padding": "x".repeat(10_000)}));
        diagnostic.upstream_error = Some(json!({"padding": "y".repeat(10_000)}));
        let encoded = serialize_diagnostic(diagnostic);
        assert!(encoded.len() <= MAX_DIAGNOSTIC_BYTES);
        let parsed: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(parsed["truncated"], true);
    }

    #[test]
    fn upstream_header_capture_uses_an_explicit_allowlist() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "provider-123".parse().unwrap());
        headers.insert("cf-ray", "ray-456".parse().unwrap());
        headers.insert("authorization", "Bearer secret".parse().unwrap());
        headers.insert("set-cookie", "session=secret".parse().unwrap());
        let safe = safe_upstream_headers(&headers);
        assert_eq!(
            safe.get("x-request-id").map(String::as_str),
            Some("provider-123")
        );
        assert_eq!(safe.get("cf-ray").map(String::as_str), Some("ray-456"));
        assert!(!safe.contains_key("authorization"));
        assert!(!safe.contains_key("set-cookie"));
    }
}
