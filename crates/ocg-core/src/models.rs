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
            auto_start: false,
            connect_timeout_secs: 30,
            non_stream_timeout_secs: 120,
            stream_idle_timeout_secs: 300,
        }
    }
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
