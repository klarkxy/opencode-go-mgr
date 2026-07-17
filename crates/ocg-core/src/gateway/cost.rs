use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

const DEFAULT_PRICE: ModelPrice = ModelPrice {
    input: 1.0,
    output: 3.0,
    cache_read: 0.0,
    cache_write: 1.0,
};

const MINIMAX_M3_LONG_CONTEXT_PRICE: ModelPrice = ModelPrice {
    input: 0.60,
    output: 2.40,
    cache_read: 0.12,
    // M3 uses automatic caching, whose writes have no separate surcharge.
    cache_write: 0.60,
};

fn price_table_cell() -> &'static HashMap<String, ModelPrice> {
    static TABLE: OnceLock<HashMap<String, ModelPrice>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = HashMap::new();
        table.insert(
            "glm-5.2".to_string(),
            ModelPrice {
                input: 1.40,
                output: 4.40,
                cache_read: 0.26,
                cache_write: 1.40,
            },
        );
        table.insert(
            "glm-5.1".to_string(),
            ModelPrice {
                input: 1.40,
                output: 4.40,
                cache_read: 0.26,
                cache_write: 1.40,
            },
        );
        table.insert(
            "kimi-k2.7-code".to_string(),
            ModelPrice {
                input: 0.95,
                output: 4.00,
                cache_read: 0.19,
                cache_write: 0.95,
            },
        );
        table.insert(
            "kimi-k2.6".to_string(),
            ModelPrice {
                input: 0.95,
                output: 4.00,
                cache_read: 0.16,
                cache_write: 0.95,
            },
        );
        table.insert(
            "deepseek-v4-pro".to_string(),
            ModelPrice {
                input: 1.74,
                output: 3.48,
                cache_read: 0.0145,
                cache_write: 1.74,
            },
        );
        table.insert(
            "deepseek-v4-flash".to_string(),
            ModelPrice {
                input: 0.14,
                output: 0.28,
                cache_read: 0.0028,
                cache_write: 0.14,
            },
        );
        table.insert(
            "mimo-v2.5".to_string(),
            ModelPrice {
                input: 0.14,
                output: 0.28,
                cache_read: 0.0028,
                cache_write: 0.14,
            },
        );
        table.insert(
            "mimo-v2.5-pro".to_string(),
            ModelPrice {
                input: 1.74,
                output: 3.48,
                cache_read: 0.0145,
                cache_write: 1.74,
            },
        );
        table.insert(
            "minimax-m3".to_string(),
            ModelPrice {
                input: 0.30,
                output: 1.20,
                cache_read: 0.06,
                // Automatic cache writes are billed as ordinary new input.
                cache_write: 0.30,
            },
        );
        table.insert(
            "minimax-m2.7".to_string(),
            ModelPrice {
                input: 0.30,
                output: 1.20,
                cache_read: 0.06,
                cache_write: 0.375,
            },
        );
        table.insert(
            "minimax-m2.7-highspeed".to_string(),
            ModelPrice {
                input: 0.60,
                output: 2.40,
                cache_read: 0.06,
                cache_write: 0.375,
            },
        );
        table.insert(
            "minimax-m2.5".to_string(),
            ModelPrice {
                input: 0.30,
                output: 1.20,
                cache_read: 0.03,
                cache_write: 0.375,
            },
        );
        table.insert(
            "minimax-m2.5-highspeed".to_string(),
            ModelPrice {
                input: 0.60,
                output: 2.40,
                cache_read: 0.03,
                cache_write: 0.375,
            },
        );
        table
    })
}

pub fn normalize_model_name(name: &str) -> String {
    name.to_lowercase().replace([' ', '_', '/'], "-")
}

fn model_price(model: &str, input_tokens: f64, service_tier: Option<&str>) -> ModelPrice {
    let table = price_table_cell();
    let normalized = normalize_model_name(model);
    if normalized.is_empty() {
        return DEFAULT_PRICE;
    }
    let (matched_model, mut price) = table
        .get_key_value(&normalized)
        .or_else(|| {
            table
                .iter()
                .filter(|(key, _)| normalized.contains(key.as_str()))
                .max_by_key(|(key, _)| key.len())
        })
        .map(|(key, price)| (key.as_str(), *price))
        .unwrap_or(("", DEFAULT_PRICE));

    if matched_model == "minimax-m3" && input_tokens > 512_000.0 {
        price = MINIMAX_M3_LONG_CONTEXT_PRICE;
    }
    if matched_model == "minimax-m3"
        && service_tier.is_some_and(|tier| tier.eq_ignore_ascii_case("priority"))
    {
        price.input *= 1.5;
        price.output *= 1.5;
        price.cache_read *= 1.5;
        price.cache_write *= 1.5;
    }
    price
}

pub fn estimate_cost(model: &str, usage: &Value) -> f64 {
    estimate_cost_with_tier(model, usage, None)
}

fn estimate_cost_with_tier(model: &str, usage: &Value, service_tier: Option<&str>) -> f64 {
    let prompt = usage
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0) as f64;
    let completion = usage
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0) as f64;
    let cached = usage
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0) as f64;
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .or_else(|| usage.pointer("/prompt_tokens_details/cache_creation_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0) as f64;

    let cached = cached.min(prompt);
    let cache_creation = cache_creation.min(prompt - cached);
    let uncached_prompt = prompt - cached - cache_creation;
    let price = model_price(model, prompt, service_tier);

    uncached_prompt / 1_000_000.0 * price.input
        + completion / 1_000_000.0 * price.output
        + cached / 1_000_000.0 * price.cache_read
        + cache_creation / 1_000_000.0 * price.cache_write
}

pub fn cost_from_counts(
    model: &str,
    prompt: i64,
    completion: i64,
    cached: i64,
    cache_creation: i64,
) -> f64 {
    cost_from_counts_with_tier(model, prompt, completion, cached, cache_creation, None)
}

pub fn cost_from_counts_with_tier(
    model: &str,
    prompt: i64,
    completion: i64,
    cached: i64,
    cache_creation: i64,
    service_tier: Option<&str>,
) -> f64 {
    let usage = serde_json::json!({
        "prompt_tokens": prompt,
        "completion_tokens": completion,
        "prompt_tokens_details": {
            "cached_tokens": cached,
            "cache_creation_tokens": cache_creation
        }
    });
    estimate_cost_with_tier(model, &usage, service_tier)
}

#[cfg(test)]
mod tests {
    use super::{cost_from_counts, cost_from_counts_with_tier};

    #[test]
    fn cached_tokens_are_not_charged_twice() {
        let cost = cost_from_counts("glm-5.2", 100, 10, 80, 0);
        let expected = (20.0 * 1.40 + 10.0 * 4.40 + 80.0 * 0.26) / 1_000_000.0;
        assert!((cost - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn minimax_m3_uses_configured_price_table() {
        let cost = cost_from_counts("minimax-m3", 1000, 200, 400, 0);
        let expected = (600.0 * 0.30 + 200.0 * 1.20 + 400.0 * 0.06) / 1_000_000.0;
        assert!((cost - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn minimax_m3_cache_read_is_not_free() {
        // Regression: before the price table included minimax-m3, cache hits
        // fell back to cache_read=0.0 and the request was heavily undercharged.
        let cost = cost_from_counts("minimax-m3", 1000, 0, 1000, 0);
        let expected = 1000.0 * 0.06 / 1_000_000.0;
        assert!((cost - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn minimax_m2_uses_configured_price_table() {
        let cost_m2_7 = cost_from_counts("minimax-m2.7", 1000, 200, 400, 0);
        let expected_m2_7 = (600.0 * 0.30 + 200.0 * 1.20 + 400.0 * 0.06) / 1_000_000.0;
        assert!((cost_m2_7 - expected_m2_7).abs() < f64::EPSILON);

        let cost_m2_5 = cost_from_counts("minimax-m2.5", 1000, 200, 400, 0);
        let expected_m2_5 = (600.0 * 0.30 + 200.0 * 1.20 + 400.0 * 0.03) / 1_000_000.0;
        assert!((cost_m2_5 - expected_m2_5).abs() < f64::EPSILON);
    }

    #[test]
    fn minimax_m3_uses_long_context_price_after_512k_total_input() {
        let at_boundary = cost_from_counts("minimax-m3", 512_000, 10, 500_000, 0);
        let expected_boundary = (12_000.0 * 0.30 + 10.0 * 1.20 + 500_000.0 * 0.06) / 1_000_000.0;
        assert!((at_boundary - expected_boundary).abs() < f64::EPSILON);

        let over_boundary = cost_from_counts("minimax-m3", 512_001, 10, 500_000, 0);
        let expected_over = (12_001.0 * 0.60 + 10.0 * 2.40 + 500_000.0 * 0.12) / 1_000_000.0;
        assert!((over_boundary - expected_over).abs() < f64::EPSILON);
    }

    #[test]
    fn minimax_m3_priority_tier_costs_one_and_a_half_times_standard() {
        let standard = cost_from_counts("minimax-m3", 1000, 200, 400, 0);
        let priority =
            cost_from_counts_with_tier("minimax-m3", 1000, 200, 400, 0, Some("priority"));
        assert!((priority - standard * 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn minimax_explicit_cache_write_and_highspeed_are_priced_separately() {
        let normal = cost_from_counts("minimax-m2.7", 1000, 100, 400, 300);
        let expected_normal =
            (300.0 * 0.30 + 100.0 * 1.20 + 400.0 * 0.06 + 300.0 * 0.375) / 1_000_000.0;
        assert!((normal - expected_normal).abs() < f64::EPSILON);

        let highspeed = cost_from_counts("MiniMax-M2.7-highspeed", 1000, 100, 400, 300);
        let expected_highspeed =
            (300.0 * 0.60 + 100.0 * 2.40 + 400.0 * 0.06 + 300.0 * 0.375) / 1_000_000.0;
        assert!((highspeed - expected_highspeed).abs() < f64::EPSILON);
    }
}
