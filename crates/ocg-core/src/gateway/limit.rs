pub use crate::upstream_limit::{parse_reset, parse_usage_limit_window};

/// Parse free-promo reset hints (`Resets in` / `retrying in`). Falls back to default.
pub fn parse_free_reset_or_default(text: &str) -> chrono::Duration {
    if let Some(duration) = parse_reset(text) {
        return duration;
    }
    // Also accept "retrying in 17h 42m"
    if let Some(idx) = text.to_ascii_lowercase().find("retrying in")
        && let Some(duration) =
            parse_reset(&format!("Resets in {}", &text[idx + "retrying in".len()..]))
    {
        return duration;
    }
    chrono::Duration::minutes(crate::gateway::free_models::DEFAULT_FREE_COOLDOWN_MINUTES)
}
