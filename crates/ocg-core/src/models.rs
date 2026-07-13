use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub password_cipher: Option<String>,
    pub key_cipher: String,
    pub enabled: bool,
    pub referral_code: Option<String>,
    pub recharge_date: Option<String>,
    pub cooldown_until: Option<DateTime<Utc>>, // None = 可用
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInput {
    pub name: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub key: String,
    pub referral_code: Option<String>,
    pub recharge_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUpdate {
    pub name: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub key: Option<String>,
    pub enabled: Option<bool>,
    pub referral_code: Option<String>,
    pub recharge_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub gateway_port: u16,
    pub gateway_key: String,
    pub upstream_base_url: String,
    pub client_root_url: String,
    pub auto_start: bool,
    pub connect_timeout_secs: u64,
    pub non_stream_timeout_secs: u64,
    pub stream_idle_timeout_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            gateway_port: 9042,
            gateway_key: String::new(),
            upstream_base_url: "https://opencode.ai/zen/go".to_string(),
            client_root_url: String::new(),
            auto_start: false,
            connect_timeout_secs: 30,
            non_stream_timeout_secs: 120,
            stream_idle_timeout_secs: 300,
        }
    }
}

/// Validates and canonicalizes the optional URL shown to downstream clients.
pub fn normalize_client_root_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let lower = value.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err("client root URL must be an absolute http:// or https:// URL".to_string());
    }

    let mut url =
        reqwest::Url::parse(value).map_err(|error| format!("invalid client root URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("client root URL must use http or https".to_string());
    }
    if url.host_str().is_none() {
        return Err("client root URL must include a host".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("client root URL must not include credentials".to_string());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("client root URL must not include a query or fragment".to_string());
    }

    let mut path = url.path().trim_end_matches('/').to_string();
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if let Some(index) = segments
        .iter()
        .position(|segment| segment.eq_ignore_ascii_case("v1"))
    {
        if index + 1 != segments.len() {
            return Err("client root URL must not include an endpoint after /v1".to_string());
        }
        path.truncate(path.len() - "/v1".len());
        path.truncate(path.trim_end_matches('/').len());
    }

    url.set_path(if path.is_empty() { "/" } else { &path });
    Ok(url.as_str().trim_end_matches('/').to_string())
}

impl AppConfig {
    pub fn validate_timeouts(&self) -> Result<(), String> {
        for (name, value, max) in [
            ("connect_timeout_secs", self.connect_timeout_secs, 300),
            (
                "non_stream_timeout_secs",
                self.non_stream_timeout_secs,
                3600,
            ),
            (
                "stream_idle_timeout_secs",
                self.stream_idle_timeout_secs,
                3600,
            ),
        ] {
            if !(1..=max).contains(&value) {
                return Err(format!("{name} must be between 1 and {max}"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayLog {
    pub id: i64,
    pub level: String,
    pub category: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardLog {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub account_id: String,
    pub account_name: String,
    pub status: String,
    pub http_status: Option<i32>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub cost: f64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ForwardMetrics {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardLogSummary {
    pub total_requests: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardLogPage {
    pub items: Vec<ForwardLog>,
    pub summary: ForwardLogSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageWindow {
    pub account_id: String,
    pub window_5h: f64,
    pub window_week: f64,
    pub window_month: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStatus {
    pub running: bool,
    pub port: u16,
    pub key: String,
    pub upstream_base_url: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub total_accounts: usize,
    pub available_accounts: usize,
    pub gateway_running: bool,
    pub today_cost: f64,
    pub week_cost: f64,
    pub month_cost: f64,
}

/// One row of "daily cost per model" aggregation for the dashboard chart.
/// `date` is `YYYY-MM-DD` (UTC). The frontend groups rows by date into a
/// stacked bar for each day.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyModelCost {
    pub date: String,
    pub model: String,
    pub cost: f64,
}
