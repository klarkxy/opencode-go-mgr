//! OpenCode Zen free-model allowlist, mapping, and route resolution.

use crate::models::UpstreamChannel;
use bytes::Bytes;
use serde_json::Value;

pub use crate::kernel::ids::is_free_model;

/// Default free-usage cooldown when upstream omits a reset hint.
pub const DEFAULT_FREE_COOLDOWN_MINUTES: i64 = 30;

/// Derive the Zen free base URL from the configured Go/Zen upstream.
///
/// - `…/zen/go` → `…/zen`
/// - `…/zen` → unchanged
/// - anything else → `None` (free channel unavailable)
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

pub fn resolve_upstream_base(channel: UpstreamChannel, go_base: &str) -> Result<String, String> {
    match channel {
        UpstreamChannel::Go => Ok(go_base.trim_end_matches('/').to_string()),
        UpstreamChannel::Free => derive_free_upstream_base(go_base).ok_or_else(|| {
            "Zen free models require an OpenCode Zen upstream (…/zen or …/zen/go); custom upstream cannot serve free models".to_string()
        }) }
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
    fn detects_free_allowlist_and_mappings() {
        assert!(is_free_model("mimo-v2.5-free"));
        assert!(!is_free_model("big-pickle"));
        assert!(is_free_model("hy3-free"));
        assert!(!is_free_model("ox-alpha-free"));
        assert!(is_free_model("x-preview-f-free"));
        assert!(is_free_model("brand-new-promo-free"));
        assert!(!is_free_model("deepseek-v4-flash"));
    }

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
}
