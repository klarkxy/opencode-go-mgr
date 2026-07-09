use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
}

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
            },
        );
        table.insert(
            "glm-5.1".to_string(),
            ModelPrice {
                input: 1.40,
                output: 4.40,
                cache_read: 0.26,
            },
        );
        table.insert(
            "kimi-k2.7-code".to_string(),
            ModelPrice {
                input: 0.95,
                output: 4.00,
                cache_read: 0.19,
            },
        );
        table.insert(
            "kimi-k2.6".to_string(),
            ModelPrice {
                input: 0.95,
                output: 4.00,
                cache_read: 0.16,
            },
        );
        table.insert(
            "deepseek-v4-pro".to_string(),
            ModelPrice {
                input: 1.74,
                output: 3.48,
                cache_read: 0.0145,
            },
        );
        table.insert(
            "deepseek-v4-flash".to_string(),
            ModelPrice {
                input: 0.14,
                output: 0.28,
                cache_read: 0.0028,
            },
        );
        table.insert(
            "mimo-v2.5".to_string(),
            ModelPrice {
                input: 0.14,
                output: 0.28,
                cache_read: 0.0028,
            },
        );
        table.insert(
            "mimo-v2.5-pro".to_string(),
            ModelPrice {
                input: 1.74,
                output: 3.48,
                cache_read: 0.0145,
            },
        );
        table
    })
}

#[deprecated(note = "use price_table_cell() instead")]
pub fn price_table() -> HashMap<String, ModelPrice> {
    price_table_cell().clone()
}

pub fn normalize_model_name(name: &str) -> String {
    name.to_lowercase().replace([' ', '_', '/'], "-")
}

pub fn estimate_cost(model: &str, usage: &Value) -> f64 {
    let table = price_table_cell();
    let normalized = normalize_model_name(model);
    let price = table.get(&normalized).unwrap_or_else(|| {
        table
            .iter()
            .find(|(k, _)| normalized.contains(*k) || k.contains(&normalized))
            .map(|(_, v)| v)
            .unwrap_or(&ModelPrice {
                input: 1.0,
                output: 3.0,
                cache_read: 0.0,
            })
    });

    let prompt = usage
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0) as f64;
    let completion = usage
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0) as f64;
    let cached = usage
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0) as f64;

    let uncached_prompt = (prompt - cached).max(0.0);
    uncached_prompt / 1_000_000.0 * price.input
        + completion / 1_000_000.0 * price.output
        + cached / 1_000_000.0 * price.cache_read
}

pub fn cost_from_counts(model: &str, prompt: i64, completion: i64, cached: i64) -> f64 {
    let usage = serde_json::json!({
        "prompt_tokens": prompt,
        "completion_tokens": completion,
        "prompt_tokens_details": { "cached_tokens": cached }
    });
    estimate_cost(model, &usage)
}

#[cfg(test)]
mod tests {
    use super::cost_from_counts;

    #[test]
    fn cached_tokens_are_not_charged_twice() {
        let cost = cost_from_counts("glm-5.2", 100, 10, 80);
        let expected = (20.0 * 1.40 + 10.0 * 4.40 + 80.0 * 0.26) / 1_000_000.0;
        assert!((cost - expected).abs() < f64::EPSILON);
    }
}
