use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub key_cipher: String,
    pub enabled: bool,
    pub referral_code: Option<String>,
    pub recharge_date: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInput {
    pub name: String,
    pub key: String,
    pub referral_code: Option<String>,
    pub recharge_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUpdate {
    pub name: Option<String>,
    pub key: Option<String>,
    pub enabled: Option<bool>,
    pub referral_code: Option<String>,
    pub recharge_date: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectionStrategy {
    Sequential,
    Random,
    RoundRobin,
}

impl Default for SelectionStrategy {
    fn default() -> Self {
        SelectionStrategy::Sequential
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub gateway_port: u16,
    pub gateway_key: String,
    pub selection_strategy: SelectionStrategy,
    pub upstream_base_url: String,
    pub auto_start: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            gateway_port: 9042,
            gateway_key: String::new(),
            selection_strategy: SelectionStrategy::default(),
            upstream_base_url: "https://api.opencode.ai".to_string(),
            auto_start: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitState {
    pub account_id: String,
    pub consecutive_errors: i32,
    pub first_error_at: Option<DateTime<Utc>>,
    pub last_error_at: Option<DateTime<Utc>>,
    pub cooldown_until: Option<DateTime<Utc>>,
    pub level: CircuitLevel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CircuitLevel {
    Normal,
    Cooldown5m,
    Cooldown1h,
    Cooldown1d,
    MonthlyBlown,
}

impl Default for CircuitLevel {
    fn default() -> Self {
        CircuitLevel::Normal
    }
}

impl CircuitLevel {
    pub fn cooldown_seconds(&self) -> i64 {
        match self {
            CircuitLevel::Normal => 0,
            CircuitLevel::Cooldown5m => 5 * 60,
            CircuitLevel::Cooldown1h => 60 * 60,
            CircuitLevel::Cooldown1d => 24 * 60 * 60,
            CircuitLevel::MonthlyBlown => i64::MAX,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CircuitLevel::Normal => "normal",
            CircuitLevel::Cooldown5m => "cooldown5m",
            CircuitLevel::Cooldown1h => "cooldown1h",
            CircuitLevel::Cooldown1d => "cooldown1d",
            CircuitLevel::MonthlyBlown => "monthly_blown",
        }
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
