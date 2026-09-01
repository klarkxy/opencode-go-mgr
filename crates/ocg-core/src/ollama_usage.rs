//! Ollama Cloud Cookie usage: validation, bounded scrape, DOM parsing.
//!
//! Ollama publishes no JSON usage API. The only usage surface is the signed-in
//! `https://ollama.com/settings` page, so this capability is opt-in per
//! account: the administrator pastes a browser Cookie request header, and a
//! manual refresh scrapes that one fixed URL. Boundary rules (spec-frozen):
//!
//! - the Cookie is stored with the same `.encryption-key`-derived obfuscation
//!   as account keys (explicitly not AEAD) and never appears in any API
//!   response, log, or export payload;
//! - the fetch uses the exact settings URL, never follows redirects, waits at
//!   most 15s, reads at most 512 KiB, and goes out through the process-wide
//!   outbound proxy default leg (`http_client::configured_builder`);
//! - parsing anchors on `data-usage-track` / `data-usage-segment` /
//!   `data-model` / `data-requests` / `data-time` / `data-usage-window`;
//! - failures (HTTP, parse, expired session) only move status columns.
//!   They never write inference cooldowns or change routing eligibility.
//!
//! The refresh state machine (30s manual throttle) lives in
//! [`crate::dashboard_v3`]; this module stays I/O-shaped but decision-free.

use crate::models::AppConfig;
use chrono::{DateTime, Utc};
use std::time::Duration;

/// Hard upper bound for the whole Cookie header (spec: ≤16KB).
pub const MAX_COOKIE_HEADER_BYTES: usize = 16 * 1024;
/// Single-request budget for the settings scrape.
pub const SETTINGS_FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// Response body cap: the settings page is far smaller; anything bigger is
/// treated as a failure instead of being buffered.
pub const MAX_SETTINGS_BODY_BYTES: usize = 512 * 1024;
/// Manual refresh throttle window (applies to successes and failures alike).
pub const MANUAL_REFRESH_THROTTLE: Duration = Duration::from_secs(30);
/// Upper bound for persisted error text (chars).
pub const MAX_ERROR_TEXT_CHARS: usize = 256;

const SET_COOKIE_ATTRIBUTE_NAMES: &[&str] = &[
    "path",
    "domain",
    "expires",
    "max-age",
    "samesite",
    "same-site",
    "secure",
    "httponly",
    "partitioned",
    "priority",
];

/// Normalize a pasted `Cookie:` request header value.
///
/// Accepts `name=value` pairs separated by `;`. Rejects Set-Cookie attribute
/// forms (`Path=/`, bare `HttpOnly`), `$`-prefixed names, empty names or
/// values, duplicate names, and headers over [`MAX_COOKIE_HEADER_BYTES`].
/// Returns the canonical `name=value; name2=value2` form actually sent
/// upstream.
pub fn normalize_cookie_header(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Cookie header is required".to_string());
    }
    if trimmed.len() > MAX_COOKIE_HEADER_BYTES {
        return Err(format!(
            "Cookie header exceeds the {MAX_COOKIE_HEADER_BYTES}-byte limit"
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut pairs = Vec::new();
    for part in trimmed.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((name, value)) = part.split_once('=') else {
            return Err(
                "paste the Cookie request header (name=value pairs), not a Set-Cookie header"
                    .to_string(),
            );
        };
        let name = name.trim();
        let value = value.trim();
        if SET_COOKIE_ATTRIBUTE_NAMES.contains(&name.to_ascii_lowercase().as_str()) {
            return Err(
                "paste the Cookie request header (name=value pairs), not a Set-Cookie header"
                    .to_string(),
            );
        }
        if name.starts_with('$') {
            return Err(format!("cookie name `{name}` uses the reserved `$` prefix"));
        }
        if name.is_empty() || value.is_empty() {
            return Err("every cookie pair must be a non-empty name=value".to_string());
        }
        if value.contains('"') || value.contains(',') || value.contains(';') {
            return Err(format!(
                "cookie value for `{name}` contains forbidden characters"
            ));
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(format!("cookie name `{name}` appears more than once"));
        }
        pairs.push(format!("{name}={value}"));
    }
    if pairs.is_empty() {
        return Err("Cookie header is required".to_string());
    }
    Ok(pairs.join("; "))
}

/// Shortest allowed instant for the next manual attempt after one at
/// `last_attempt`. Successes and failures both count.
pub fn manual_next_allowed_at(last_attempt: DateTime<Utc>) -> DateTime<Utc> {
    last_attempt + MANUAL_REFRESH_THROTTLE
}

/// Trim and sanitize an error string for persistence: bounded length, no HTML
/// fragments, no URL query strings.
pub fn sanitize_error_text(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    // Drop anything that looks like markup or a query string wholesale; the
    // scrape deals in HTML and redirect URLs, both leak-prone.
    while let Some(start) = text.find('<') {
        let end = text[start..].find('>').map(|offset| start + offset + 1);
        match end {
            Some(end) => text.replace_range(start..end, ""),
            None => {
                text.truncate(start);
                break;
            }
        }
    }
    // A '?' inside scraped text is a URL query string: drop it and
    // everything after rather than trying to reassemble the sentence.
    if let Some(start) = text.find('?') {
        text.truncate(start);
        text = text.trim_end().to_string();
    }
    let bounded: String = text.chars().take(MAX_ERROR_TEXT_CHARS).collect();
    bounded.trim().to_string()
}

/// The sanitized snapshot persisted and served to the dashboard. Constructed
/// only by [`parse_settings_usage`]; contains no HTML, Cookie, or session
/// data by construction.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OllamaUsageSnapshot {
    pub windows: Vec<OllamaUsageWindow>,
    pub models: Vec<OllamaModelRequests>,
    pub plan: Option<String>,
    pub balance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OllamaUsageWindow {
    /// `5h` or `7d`.
    pub window: String,
    pub used_percent: Option<f64>,
    pub reset_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OllamaModelRequests {
    pub model: String,
    pub requests_5h: Option<u64>,
    pub requests_7d: Option<u64>,
}

/// Parse outcome: a successful snapshot, an expired session, or a failure
/// message (already sanitized for persistence).
#[derive(Debug, Clone, PartialEq)]
pub enum ParseOutcome {
    Snapshot(OllamaUsageSnapshot),
    Unauthorized,
    Failed(String),
}

/// A fetched settings page: bounded HTML plus the final URL actually reached
/// (equal to the fixed settings URL — redirects never happen on success).
pub struct FetchedSettingsPage {
    pub final_url: String,
    pub body: String,
}

/// Build the settings URL for one origin. Production always passes
/// [`crate::provider::OLLAMA_CLOUD_BASE_URL`]; tests pass the debug-only
/// loopback seam origin.
pub fn settings_url_for_origin(origin: &str) -> String {
    format!("{}/settings", origin.trim_end_matches('/'))
}

/// True for an HTTP loopback origin (the debug-only test seam).
pub fn is_loopback_http_url(origin: &str) -> bool {
    match reqwest::Url::parse(origin) {
        Ok(parsed) => {
            parsed.scheme() == "http"
                && matches!(
                    parsed.host_str(),
                    Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
                )
        }
        Err(_) => false,
    }
}

/// Exact-URL gate for the scrape target: scheme, host, and path must equal
/// the canonical settings URL.
pub fn is_exact_settings_url(url: &str) -> bool {
    match reqwest::Url::parse(url) {
        Ok(parsed) => {
            parsed.scheme() == "https"
                && parsed.host_str()
                    == Some(crate::provider::OLLAMA_CLOUD_BASE_URL.trim_start_matches("https://"))
                && parsed.path() == "/settings"
                && parsed.query().is_none()
                && parsed.fragment().is_none()
        }
        Err(_) => false,
    }
}

/// Fetch the fixed settings page with the account Cookie. Redirects are
/// disabled at the client; any redirect status is a failure (the session is
/// expired or the page moved — neither is followed).
pub async fn fetch_settings_page(
    config: &AppConfig,
    cookie: &str,
    origin: &str,
) -> Result<FetchedSettingsPage, String> {
    let url = settings_url_for_origin(origin);
    let origin_allowed =
        is_exact_settings_url(&url) || (cfg!(debug_assertions) && is_loopback_http_url(origin));
    if !origin_allowed {
        return Err("usage scrape target does not match the fixed settings URL".to_string());
    }
    let client = crate::http_client::configured_builder(config)
        .and_then(|builder| {
            builder
                .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
                .redirect(crate::http_client::no_redirect_policy())
                .build()
                .map_err(Into::into)
        })
        .map_err(|error| format!("failed to build the usage client: {error}"))?;
    let response = client
        .get(&url)
        .header(reqwest::header::COOKIE, cookie)
        .header(reqwest::header::ACCEPT, "text/html,application/json")
        .timeout(SETTINGS_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("settings request failed: {error}"))?;
    let status = response.status();
    if status.is_redirection() {
        return Err("the settings page redirected; the session is no longer valid".to_string());
    }
    if !status.is_success() {
        return Err(format!("settings page returned HTTP {}", status.as_u16()));
    }
    let final_url = response.url().to_string();
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("settings body read failed: {error}"))?;
        if body.len() + chunk.len() > MAX_SETTINGS_BODY_BYTES {
            return Err("settings page exceeded the 512KB read limit".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&body).to_string();
    Ok(FetchedSettingsPage { final_url, body })
}

/// Parse the bounded settings HTML into a snapshot.
pub fn parse_settings_usage(html: &str) -> ParseOutcome {
    if html_looks_like_login_page(html) {
        return ParseOutcome::Unauthorized;
    }
    let tracks = collect_tags_with_attribute(html, "data-usage-track");
    let segments = collect_tags_with_attribute(html, "data-usage-segment");
    let model_rows = collect_tags_with_attribute(html, "data-model");
    if tracks.is_empty() && model_rows.is_empty() {
        return ParseOutcome::Failed("usage anchors were not found on the settings page".into());
    }

    let mut windows: Vec<OllamaUsageWindow> = Vec::new();
    for (index, track) in tracks.iter().enumerate() {
        let window = track
            .get("data-usage-window")
            .or_else(|| track.get("data-usage-track"))
            .map(|value| normalize_window_key(value))
            .unwrap_or_else(|| fallback_window_key(index));
        let reset_at = track.get("data-time").cloned();
        let used_percent = track
            .get("data-used-percent")
            .and_then(|value| value.trim().trim_end_matches('%').parse::<f64>().ok());
        windows.push(OllamaUsageWindow {
            window,
            used_percent,
            reset_at,
        });
    }
    // Gauge segments without a model carry the window's used percent when the
    // track element itself does not.
    for segment in &segments {
        if segment.contains_key("data-model") {
            continue;
        }
        let Some(percent) = segment
            .get("data-usage-segment")
            .and_then(|value| value.trim().trim_end_matches('%').parse::<f64>().ok())
        else {
            continue;
        };
        let window = segment
            .get("data-usage-window")
            .map(|value| normalize_window_key(value))
            .or_else(|| windows.first().map(|window| window.window.clone()));
        let existing_index = window
            .as_ref()
            .and_then(|key| windows.iter().position(|item| &item.window == key));
        match existing_index {
            Some(index) => {
                if windows[index].used_percent.is_none() {
                    windows[index].used_percent = Some(percent);
                }
            }
            None => {
                if !windows.is_empty() && window.is_none() {
                    // Unknown gauge segment: attribute it to the first track.
                    if windows[0].used_percent.is_none() {
                        windows[0].used_percent = Some(percent);
                    }
                } else {
                    windows.push(OllamaUsageWindow {
                        window: window.unwrap_or_else(|| "5h".to_string()),
                        used_percent: Some(percent),
                        reset_at: segment.get("data-time").cloned(),
                    });
                }
            }
        }
    }

    let mut models: Vec<OllamaModelRequests> = Vec::new();
    for row in &model_rows {
        let model = match row.get("data-model") {
            Some(model) if !model.is_empty() => model.clone(),
            _ => continue,
        };
        let requests = row
            .get("data-requests")
            .and_then(|value| value.trim().parse::<u64>().ok());
        let window = row
            .get("data-usage-window")
            .or_else(|| row.get("data-time"))
            .map(|value| normalize_window_key(value));
        let entry_index = models
            .iter()
            .position(|item| item.model.eq_ignore_ascii_case(&model))
            .unwrap_or_else(|| {
                models.push(OllamaModelRequests {
                    model: model.clone(),
                    requests_5h: None,
                    requests_7d: None,
                });
                models.len() - 1
            });
        let entry = &mut models[entry_index];
        match window.as_deref() {
            Some("7d") => entry.requests_7d = requests.or(entry.requests_7d),
            Some("5h") => entry.requests_5h = requests.or(entry.requests_5h),
            _ => {}
        }
    }

    let plan = extract_labeled_value(html, &["Plan", "套餐"]);
    let balance = extract_labeled_value(html, &["Balance", "余额"]);

    if windows.is_empty() && models.is_empty() {
        return ParseOutcome::Failed("usage anchors were not found on the settings page".into());
    }
    ParseOutcome::Snapshot(OllamaUsageSnapshot {
        windows,
        models,
        plan,
        balance,
    })
}

fn normalize_window_key(raw: &str) -> String {
    let folded = raw.trim().to_ascii_lowercase();
    match folded.as_str() {
        "5h" | "5hour" | "five_hours" | "fivehours" | "5-hour" => "5h".to_string(),
        "7d" | "7day" | "week" | "7-day" | "weekly" => "7d".to_string(),
        other => other.to_string(),
    }
}

fn fallback_window_key(index: usize) -> String {
    if index == 0 {
        "5h".to_string()
    } else {
        "7d".to_string()
    }
}

fn html_looks_like_login_page(html: &str) -> bool {
    let lowered = html.to_ascii_lowercase();
    let has_login_marker = lowered.contains("sign in")
        || lowered.contains("log in")
        || lowered.contains("login")
        || lowered.contains("data-login")
        || lowered.contains("auth/form");
    let has_usage_anchor = lowered.contains("data-usage-track")
        || lowered.contains("data-usage-segment")
        || lowered.contains("data-model");
    has_login_marker && !has_usage_anchor
}

fn extract_labeled_value(html: &str, labels: &[&str]) -> Option<String> {
    let lowered = html.to_ascii_lowercase();
    for label in labels {
        let needle = label.to_ascii_lowercase();
        let mut search_from = 0;
        while let Some(found) = lowered[search_from..].find(&needle) {
            let start = search_from + found + needle.len();
            let tail = &html[start..];
            let mut chars = tail.char_indices();
            // Skip one separator run (":" / "：" / whitespace) then read the
            // value up to the next tag or newline, bounded to 64 chars.
            let mut value_start = None;
            let mut value_end = None;
            for (offset, ch) in chars.by_ref() {
                if value_start.is_none() {
                    if ch.is_whitespace() || ch == ':' || ch == '：' {
                        continue;
                    }
                    value_start = Some(offset);
                }
                if ch == '<' || ch == '\n' || offset > 96 {
                    value_end = Some(offset);
                    break;
                }
            }
            if let (Some(value_start), Some(value_end)) = (value_start, value_end) {
                let value = html[start + value_start..start + value_end].trim();
                if !value.is_empty() && value.len() <= 64 {
                    return Some(value.to_string());
                }
            }
            search_from = start;
        }
    }
    None
}

/// Extract one tag's attributes as a flat map. Attributes without a value
/// map to an empty string. Bounded to the tag text; no nesting.
fn collect_tags_with_attribute(
    html: &str,
    attribute: &str,
) -> Vec<std::collections::BTreeMap<String, String>> {
    let needle = format!("{}=", attribute);
    let mut tags = Vec::new();
    let mut search_from = 0;
    while let Some(found) = html[search_from..].find(&needle) {
        let attribute_at = search_from + found;
        let tag_start = html[..attribute_at].rfind('<').unwrap_or(0);
        let Some(tag_end_rel) = html[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + tag_end_rel;
        let tag = &html[tag_start..tag_end];
        if tag.starts_with('<') && !tag.starts_with("<!--") {
            tags.push(parse_tag_attributes(tag));
        }
        search_from = tag_end + 1;
    }
    tags
}

fn parse_tag_attributes(tag: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    let mut rest = tag.trim_start_matches('<').trim_end_matches('>');
    // Drop the element name (first token) and any leading slash.
    rest = rest.trim_start_matches('/');
    let name_end = rest
        .find(|ch: char| ch.is_whitespace())
        .unwrap_or(rest.len());
    rest = &rest[name_end..];
    let bytes = rest.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let name_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        let name = rest[name_start..index].to_ascii_lowercase();
        if index < bytes.len() && bytes[index] == b'=' {
            index += 1;
            let value = if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
                let quote = bytes[index];
                index += 1;
                let value_start = index;
                while index < bytes.len() && bytes[index] != quote {
                    index += 1;
                }
                let value = rest[value_start..index.min(rest.len())].to_string();
                if index < bytes.len() {
                    index += 1;
                }
                value
            } else {
                let value_start = index;
                while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                rest[value_start..index].to_string()
            };
            map.insert(name, decode_html_entities(&value));
        } else if !name.is_empty() {
            map.insert(name, String::new());
        }
    }
    map
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_header_normalization_accepts_pairs_and_rejects_set_cookie_forms() {
        assert_eq!(
            normalize_cookie_header("session=abc; theme=dark").unwrap(),
            "session=abc; theme=dark"
        );
        assert_eq!(
            normalize_cookie_header("  a=1 ;  b=2  ").unwrap(),
            "a=1; b=2"
        );

        let set_cookie = normalize_cookie_header("session=abc; Path=/; HttpOnly");
        assert!(set_cookie.unwrap_err().contains("not a Set-Cookie header"));
        assert!(
            normalize_cookie_header("session=abc; SameSite=Lax")
                .unwrap_err()
                .contains("not a Set-Cookie header")
        );
        assert!(
            normalize_cookie_header("session=abc; Expires=Wed, 21 Oct 2015 07:28:00 GMT")
                .unwrap_err()
                .contains("not a Set-Cookie header")
        );
        assert!(normalize_cookie_header("HttpOnly").is_err());
        assert!(
            normalize_cookie_header("$version=1; session=abc")
                .unwrap_err()
                .contains("$")
        );
        assert!(
            normalize_cookie_header("session=abc; session=def")
                .unwrap_err()
                .contains("more than once")
        );
        assert!(normalize_cookie_header("session=").is_err());
        assert!(normalize_cookie_header("=value").is_err());
        assert!(normalize_cookie_header("").is_err());
        let oversized = format!("{}={}", "n".repeat(64), "v".repeat(MAX_COOKIE_HEADER_BYTES));
        assert!(
            normalize_cookie_header(&oversized)
                .unwrap_err()
                .contains("byte limit")
        );
    }

    #[test]
    fn error_sanitizer_strips_html_and_query_strings_and_bounds_length() {
        assert_eq!(sanitize_error_text("plain failure"), "plain failure");
        assert_eq!(
            sanitize_error_text("bad <script>alert(1)</script> token"),
            "bad alert(1) token"
        );
        assert_eq!(sanitize_error_text("open <tag"), "open");
        assert_eq!(
            sanitize_error_text("see https://x.example/path?session=tok value"),
            "see https://x.example/path"
        );
        let long = sanitize_error_text(&"x".repeat(5_000));
        assert!(long.chars().count() <= MAX_ERROR_TEXT_CHARS);
    }

    #[test]
    fn login_pages_are_unauthorized_and_missing_anchors_fail() {
        assert_eq!(
            parse_settings_usage(
                r#"<html><body><form action="/login">Sign in</form></body></html>"#
            ),
            ParseOutcome::Unauthorized
        );
        assert!(matches!(
            parse_settings_usage("<html><body>nothing here</body></html>"),
            ParseOutcome::Failed(message) if message.contains("anchors")
        ));
    }

    #[test]
    fn settings_page_anchors_parse_into_sanitized_windows_and_models() {
        let page = concat!(
            r#"<div class="wrap"><span>Plan: Maker</span> <span>Balance: $3.20</span>"#,
            r#"<div data-usage-track="5h" data-time="2026-09-01T00:00:00Z" data-used-percent="42"></div>"#,
            r#"<div data-usage-track="7d" data-segment data-used-percent="12.5"></div>"#,
            r#"<div data-model="gpt-oss:120b" data-requests="12" data-usage-window="5h"></div>"#,
            r#"<div data-model="gpt-oss:120b" data-requests="340" data-time="7d"></div>"#,
            r#"<div data-model="deepseek-v4-flash:0915" data-requests="5" data-time="5h"></div>"#,
            "</div>"
        );
        let ParseOutcome::Snapshot(snapshot) = parse_settings_usage(page) else {
            panic!("expected a snapshot");
        };
        assert_eq!(snapshot.plan.as_deref(), Some("Maker"));
        assert_eq!(snapshot.balance.as_deref(), Some("$3.20"));
        assert_eq!(
            snapshot.windows,
            vec![
                OllamaUsageWindow {
                    window: "5h".into(),
                    used_percent: Some(42.0),
                    reset_at: Some("2026-09-01T00:00:00Z".into()),
                },
                OllamaUsageWindow {
                    window: "7d".into(),
                    used_percent: Some(12.5),
                    reset_at: None,
                },
            ]
        );
        assert_eq!(snapshot.models.len(), 2);
        let gpt = snapshot
            .models
            .iter()
            .find(|model| model.model == "gpt-oss:120b")
            .unwrap();
        assert_eq!(gpt.requests_5h, Some(12));
        assert_eq!(gpt.requests_7d, Some(340));
        let flash = snapshot
            .models
            .iter()
            .find(|model| model.model == "deepseek-v4-flash:0915")
            .unwrap();
        assert_eq!(flash.requests_5h, Some(5));
        assert!(flash.requests_7d.is_none());
        // Sanitized shape: serializing the snapshot carries no markup.
        let encoded = serde_json::to_string(&snapshot).unwrap();
        assert!(!encoded.contains('<') && !encoded.contains("data-"));
    }

    #[test]
    fn usage_segment_percent_fills_windows_without_track_values() {
        let page = concat!(
            r#"<div data-usage-track="5h"></div>"#,
            r#"<div data-usage-track="7d"></div>"#,
            r#"<div data-usage-segment="77" data-usage-window="5h"></div>"#,
            r#"<div data-usage-segment="10" data-usage-window="7d"></div>"#,
            r#"<div data-model="m" data-requests="1" data-time="5h"></div>"#
        );
        let ParseOutcome::Snapshot(snapshot) = parse_settings_usage(page) else {
            panic!("expected a snapshot");
        };
        assert_eq!(snapshot.windows[0].used_percent, Some(77.0));
        assert_eq!(snapshot.windows[1].used_percent, Some(10.0));
    }

    #[test]
    fn settings_url_gate_is_exact() {
        assert!(is_exact_settings_url("https://ollama.com/settings"));
        assert!(!is_exact_settings_url("https://ollama.com/settings/"));
        assert!(!is_exact_settings_url("https://ollama.com/settings?x=1"));
        assert!(!is_exact_settings_url("https://evil.example/settings"));
        assert!(!is_exact_settings_url("http://ollama.com/settings"));
    }
}
