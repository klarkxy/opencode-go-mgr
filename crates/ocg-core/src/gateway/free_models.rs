//! OpenCode Zen free-model allowlist, mapping, and route resolution.

use crate::models::UpstreamChannel;
use crate::provider::{OPENCODE_GO_BASE_URL, OPENCODE_ZEN_BASE_URL};
use bytes::Bytes;
use serde_json::Value;

pub use crate::kernel::ids::is_free_model;

/// Default free-usage cooldown when upstream omits a reset hint.
pub const DEFAULT_FREE_COOLDOWN_MINUTES: i64 = 30;

/// Derive the Zen free base URL from a Go/Zen origin.
///
/// Used only for loopback test seams:
/// - `…/zen/go` → `…/zen`
/// - `…/zen` → unchanged
/// - anything else → `None` (free channel unavailable on that origin)
pub fn derive_free_upstream_base(go_base: &str) -> Option<String> {
    let trimmed = go_base.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with("/zen/go") {
        Some(trimmed[..trimmed.len() - "/go".len()].to_string())
    } else if lower.ends_with("/zen") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// Production OpenCode Go origin, or a loopback substitute from tests.
pub fn opencode_go_base_url(configured: &str) -> String {
    resolve_upstream_base(UpstreamChannel::Go, configured)
        .unwrap_or_else(|_| OPENCODE_GO_BASE_URL.to_string())
}

/// Resolve the sealed OpenCode origin for a channel.
///
/// Non-loopback values, including saved custom HTTPS URLs, use the official
/// constants. Loopback HTTP(S) remains a test seam and still derives Zen by
/// stripping `/go` when the configured base ends in `/zen/go` or `/zen`.
pub fn resolve_upstream_base(channel: UpstreamChannel, go_base: &str) -> Result<String, String> {
    let trimmed = go_base.trim().trim_end_matches('/');
    if is_loopback_http_origin(trimmed) {
        return match channel {
            UpstreamChannel::Go => Ok(trimmed.to_string()),
            UpstreamChannel::Free => derive_free_upstream_base(trimmed).ok_or_else(|| {
                "Zen free models require an OpenCode Zen upstream (…/zen or …/zen/go); custom upstream cannot serve free models".to_string()
            }),
        };
    }
    Ok(match channel {
        UpstreamChannel::Go => OPENCODE_GO_BASE_URL.to_string(),
        UpstreamChannel::Free => OPENCODE_ZEN_BASE_URL.to_string(),
    })
}

fn is_loopback_http_origin(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    matches!(
        parsed.host_str(),
        Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
    )
}

pub fn rewrite_body_model(body: &Bytes, model: &str) -> Result<Bytes, String> {
    let mut value: Value =
        serde_json::from_slice(body).map_err(|error| format!("invalid JSON request: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "request must be a JSON object".to_string())?;
    object.insert("model".to_string(), Value::String(model.to_string()));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| format!("failed to encode request: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_free_base_from_go_or_zen() {
        assert_eq!(
            derive_free_upstream_base("https://opencode.ai/zen/go"),
            Some("https://opencode.ai/zen".into())
        );
        assert_eq!(
            derive_free_upstream_base("https://opencode.ai/zen/go/"),
            Some("https://opencode.ai/zen".into())
        );
        assert_eq!(
            derive_free_upstream_base("https://opencode.ai/zen"),
            Some("https://opencode.ai/zen".into())
        );
        assert_eq!(derive_free_upstream_base("https://example.com/v1"), None);
    }

    #[test]
    fn production_bases_are_sealed_and_loopback_is_the_test_seam() {
        assert_eq!(
            resolve_upstream_base(UpstreamChannel::Go, "https://mirror.example/zen/go").unwrap(),
            OPENCODE_GO_BASE_URL
        );
        assert_eq!(
            resolve_upstream_base(UpstreamChannel::Free, "https://mirror.example/zen/go").unwrap(),
            OPENCODE_ZEN_BASE_URL
        );
        assert_eq!(opencode_go_base_url(""), OPENCODE_GO_BASE_URL);
        assert_eq!(
            resolve_upstream_base(UpstreamChannel::Go, "http://127.0.0.1:9/zen/go").unwrap(),
            "http://127.0.0.1:9/zen/go"
        );
        assert_eq!(
            resolve_upstream_base(UpstreamChannel::Free, "http://127.0.0.1:9/zen/go").unwrap(),
            "http://127.0.0.1:9/zen"
        );
        assert!(resolve_upstream_base(UpstreamChannel::Free, "http://127.0.0.1:9/v1").is_err());
    }
}
