//! OpenCode Zen free-model allowlist, mapping, and route resolution.

use crate::models::UpstreamChannel;
use bytes::Bytes;
use serde_json::{Value, json};

pub use crate::kernel::ids::is_free_model;

/// Default free-usage cooldown when upstream omits a reset hint.
pub const DEFAULT_FREE_COOLDOWN_MINUTES: i64 = 30;

#[derive(Debug, Clone, Copy)]
struct FreeModelProfile {
    id: &'static str,
}

const FREE_MODELS: &[FreeModelProfile] = &[
    FreeModelProfile {
        id: "deepseek-v4-flash-free",
    },
    FreeModelProfile {
        id: "mimo-v2.5-free",
    },
    FreeModelProfile { id: "hy3-free" },
    FreeModelProfile {
        id: "laguna-s-2.1-free",
    },
    FreeModelProfile {
        id: "nemotron-3-ultra-free",
    },
    FreeModelProfile {
        id: "nemotron-3.5-lightning-free",
    },
    FreeModelProfile {
        id: "muse-spark-1.2-contributor-free",
    },
    FreeModelProfile {
        id: "x-preview-f-free",
    },
];

pub fn free_model_ids() -> impl Iterator<Item = &'static str> {
    FREE_MODELS.iter().map(|profile| profile.id)
}

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

/// Append known Zen free model ids to an OpenAI-style `/v1/models` payload.
///
/// Go's catalog omits Zen-only promo models such as `big-pickle`. Clients that
/// discover models from the gateway would otherwise never see them. Go-named
/// free ids like `ox-alpha-free` already appear in the Go catalog and must not
/// be injected as Zen.
pub fn merge_free_models_into_list(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut value: Value =
        serde_json::from_slice(body).map_err(|error| format!("invalid models list: {error}"))?;
    let data = value
        .get_mut("data")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "models list data is missing".to_string())?;
    let existing = data
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_string))
        .collect::<Vec<_>>();
    for id in free_model_ids() {
        if existing.iter().any(|have| have == id) {
            continue;
        }
        data.push(json!({
            "id": id,
            "object": "model" }));
    }
    serde_json::to_vec(&value).map_err(|error| format!("failed to encode models list: {error}"))
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
    fn merge_appends_missing_free_ids() {
        let merged = merge_free_models_into_list(
            br#"{"object":"list","data":[{"id":"deepseek-v4-flash","object":"model"}]}"#,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&merged).unwrap();
        let ids = value["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(ids.contains(&"deepseek-v4-flash"));
        assert!(!ids.contains(&"big-pickle"));
        assert!(ids.contains(&"hy3-free"));
        assert!(ids.contains(&"muse-spark-1.2-contributor-free"));
        assert!(!ids.contains(&"ox-alpha-free"));
        assert!(ids.contains(&"x-preview-f-free"));
        assert!(ids.contains(&"deepseek-v4-flash-free"));
        assert_eq!(
            ids.iter().filter(|id| **id == "deepseek-v4-flash").count(),
            1
        );
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
