use std::fmt;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub password_cipher: Option<String>,
    pub key_cipher: String,
    pub enabled: bool,
    #[serde(default)]
    pub account_type: AccountType,
    #[serde(default)]
    pub setup_step: AccountSetupStep,
    pub referral_code: Option<String>,
    #[serde(alias = "recharge_date")]
    pub purchase_date: String,
    #[serde(default)]
    pub expires_on: String,
    /// Derived: when the account becomes usable after every active cooldown expires.
    /// Kept for backwards compatibility; `None` means currently available.
    pub cooldown_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cooldown_generic_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cooldown_5h_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cooldown_week_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cooldown_month_until: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// A persisted upstream 401 marker. Accounts with an auth error remain
    /// enabled for management purposes, but are excluded from gateway routing
    /// until their key is replaced or a direct ping proves the key works again.
    #[serde(default)]
    pub auth_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    #[default]
    Key,
    Managed,
}

impl AccountType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Managed => "managed",
        }
    }
}

impl TryFrom<&str> for AccountType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "key" => Ok(Self::Key),
            "managed" => Ok(Self::Managed),
            _ => Err(format!("unknown account type `{value}`")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccountSetupStep {
    GoogleAccount,
    OpencodeRegistration,
    Payment,
    KeyVerification,
    #[default]
    Ready,
}

impl AccountSetupStep {
    pub fn next(self) -> Option<Self> {
        match self {
            Self::GoogleAccount => Some(Self::OpencodeRegistration),
            Self::OpencodeRegistration => Some(Self::Payment),
            Self::Payment => Some(Self::KeyVerification),
            Self::KeyVerification | Self::Ready => None,
        }
    }

    pub fn is_ready(self) -> bool {
        self == Self::Ready
    }

    /// Wizard progress index for unfinished steps. `ready` is not part of the wizard.
    pub const fn wizard_index(self) -> Option<u8> {
        match self {
            Self::GoogleAccount => Some(0),
            Self::OpencodeRegistration => Some(1),
            Self::Payment => Some(2),
            Self::KeyVerification => Some(3),
            Self::Ready => None,
        }
    }

    /// Forward exactly one step, or rewind to any earlier unfinished step.
    pub fn can_transition_to(self, to: Self) -> bool {
        let Some(from_i) = self.wizard_index() else {
            return false;
        };
        let Some(to_i) = to.wizard_index() else {
            return false;
        };
        to_i == from_i + 1 || to_i < from_i
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GoogleAccount => "google_account",
            Self::OpencodeRegistration => "opencode_registration",
            Self::Payment => "payment",
            Self::KeyVerification => "key_verification",
            Self::Ready => "ready",
        }
    }
}

impl TryFrom<&str> for AccountSetupStep {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "google_account" => Ok(Self::GoogleAccount),
            "opencode_registration" => Ok(Self::OpencodeRegistration),
            "payment" => Ok(Self::Payment),
            "key_verification" => Ok(Self::KeyVerification),
            "ready" => Ok(Self::Ready),
            _ => Err(format!("unknown account setup step `{value}`")),
        }
    }
}

impl Account {
    pub fn cooldown_ends_at(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        [
            self.cooldown_generic_until,
            self.cooldown_5h_until,
            self.cooldown_week_until,
            self.cooldown_month_until,
        ]
        .into_iter()
        .flatten()
        .filter(|until| *until > now)
        .max()
    }

    pub fn is_cooling_at(&self, now: DateTime<Utc>) -> bool {
        self.cooldown_ends_at(now).is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInput {
    pub name: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub key: String,
    pub referral_code: Option<String>,
    #[serde(alias = "recharge_date")]
    pub purchase_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUpdate {
    pub name: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub key: Option<String>,
    pub enabled: Option<bool>,
    pub referral_code: Option<String>,
    #[serde(alias = "recharge_date")]
    pub purchase_date: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurchaseDateError;

impl fmt::Display for PurchaseDateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("purchase date must use the YYYY-MM-DD format")
    }
}

impl std::error::Error for PurchaseDateError {}

/// Returns the current calendar date in the process's local timezone.
pub fn local_today() -> String {
    format_date(Local::now().date_naive())
}

/// Validates a purchase date and returns its canonical `YYYY-MM-DD` representation.
pub fn normalize_purchase_date(value: &str) -> Result<String, PurchaseDateError> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| PurchaseDateError)?;
    let normalized = format_date(parsed);
    if normalized != value {
        return Err(PurchaseDateError);
    }
    Ok(normalized)
}

/// Calculates the natural-month expiry date, clamping to the target month's last day.
pub fn purchase_expires_on(value: &str) -> Result<String, PurchaseDateError> {
    let normalized = normalize_purchase_date(value)?;
    let purchase =
        NaiveDate::parse_from_str(&normalized, "%Y-%m-%d").map_err(|_| PurchaseDateError)?;
    let (target_year, target_month) = next_month(purchase.year(), purchase.month())?;
    let (following_year, following_month) = next_month(target_year, target_month)?;
    let target_last_day = NaiveDate::from_ymd_opt(following_year, following_month, 1)
        .and_then(|date| date.pred_opt())
        .ok_or(PurchaseDateError)?
        .day();
    let expires = NaiveDate::from_ymd_opt(
        target_year,
        target_month,
        purchase.day().min(target_last_day),
    )
    .ok_or(PurchaseDateError)?;
    Ok(format_date(expires))
}

fn next_month(year: i32, month: u32) -> Result<(i32, u32), PurchaseDateError> {
    if month == 12 {
        Ok((year.checked_add(1).ok_or(PurchaseDateError)?, 1))
    } else {
        Ok((year, month + 1))
    }
}

fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingMode {
    #[default]
    StrictPriority,
    StickyGlobal,
    RoundRobin,
}

pub const DEFAULT_OPENCODE_INVITE_URL: &str = "https://opencode.ai/go?ref=68XPB6NP8V";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub gateway_port: u16,
    pub gateway_key: String,
    pub upstream_base_url: String,
    pub opencode_invite_url: String,
    pub client_root_url: String,
    pub auto_start: bool,
    pub show_dock_icon: bool,
    pub connect_timeout_secs: u64,
    pub non_stream_timeout_secs: u64,
    pub stream_idle_timeout_secs: u64,
    pub routing_mode: RoutingMode,
    pub conversation_sticky: bool,
    pub claude_desktop_models: ClaudeDesktopModels,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            gateway_port: 9042,
            gateway_key: String::new(),
            upstream_base_url: "https://opencode.ai/zen/go".to_string(),
            opencode_invite_url: DEFAULT_OPENCODE_INVITE_URL.to_string(),
            client_root_url: String::new(),
            auto_start: false,
            show_dock_icon: true,
            connect_timeout_secs: 30,
            non_stream_timeout_secs: 900,
            stream_idle_timeout_secs: 300,
            routing_mode: RoutingMode::StrictPriority,
            conversation_sticky: false,
            claude_desktop_models: ClaudeDesktopModels::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeDesktopModels {
    pub sonnet: String,
    pub opus: String,
    pub haiku: String,
}

impl Default for ClaudeDesktopModels {
    fn default() -> Self {
        Self {
            sonnet: "minimax-m3".to_string(),
            opus: String::new(),
            haiku: String::new(),
        }
    }
}

impl ClaudeDesktopModels {
    pub fn normalize(&mut self) {
        self.sonnet = self.sonnet.trim().to_string();
        self.opus = self.opus.trim().to_string();
        self.haiku = self.haiku.trim().to_string();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.first_configured().is_none() {
            return Err("at least one Claude Desktop model is required".to_string());
        }
        for (role, model) in [
            ("sonnet", self.sonnet.as_str()),
            ("opus", self.opus.as_str()),
            ("haiku", self.haiku.as_str()),
        ] {
            if !model.is_empty()
                && !crate::gateway::protocol::supported_model_ids()
                    .any(|supported| supported == model)
            {
                return Err(format!("unsupported Claude Desktop {role} model `{model}`"));
            }
        }
        Ok(())
    }

    pub fn resolved(&self) -> Self {
        let fallback = self.first_configured().unwrap_or_default();
        Self {
            sonnet: if self.sonnet.is_empty() {
                fallback.to_string()
            } else {
                self.sonnet.clone()
            },
            opus: if self.opus.is_empty() {
                fallback.to_string()
            } else {
                self.opus.clone()
            },
            haiku: if self.haiku.is_empty() {
                fallback.to_string()
            } else {
                self.haiku.clone()
            },
        }
    }

    pub(crate) fn model_for_alias(&self, alias: &str) -> Option<&str> {
        let configured = match alias {
            CLAUDE_DESKTOP_SONNET_ALIAS => self.sonnet.as_str(),
            CLAUDE_DESKTOP_OPUS_ALIAS => self.opus.as_str(),
            CLAUDE_DESKTOP_HAIKU_ALIAS => self.haiku.as_str(),
            _ => return None,
        };
        (!configured.is_empty())
            .then_some(configured)
            .or_else(|| self.first_configured())
    }

    fn first_configured(&self) -> Option<&str> {
        [
            self.sonnet.as_str(),
            self.opus.as_str(),
            self.haiku.as_str(),
        ]
        .into_iter()
        .find(|model| !model.is_empty())
    }
}

pub const CLAUDE_DESKTOP_SONNET_ALIAS: &str = "claude-sonnet-4-6";
pub const CLAUDE_DESKTOP_OPUS_ALIAS: &str = "claude-opus-4-6";
pub const CLAUDE_DESKTOP_HAIKU_ALIAS: &str = "claude-haiku-4-5-20251001";

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
    pub fn validate(&self) -> Result<(), String> {
        self.validate_timeouts()?;
        normalize_opencode_invite_url(&self.opencode_invite_url)?;
        // routing_mode is validated by serde enum decoding; unknown values never reach here.
        self.claude_desktop_models.validate()
    }

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

pub fn normalize_opencode_invite_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    if value.len() > 2048 {
        return Err("OpenCode invite URL is too long".to_string());
    }
    let parsed = reqwest::Url::parse(value)
        .map_err(|error| format!("invalid OpenCode invite URL: {error}"))?;
    if parsed.scheme() != "https" {
        return Err("OpenCode invite URL must use https".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("OpenCode invite URL must not contain credentials".to_string());
    }
    match parsed.host_str() {
        Some("opencode.ai" | "console.opencode.ai") => {}
        _ => {
            return Err(
                "OpenCode invite URL host must be opencode.ai or console.opencode.ai".to_string(),
            );
        }
    }
    Ok(parsed.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayLog {
    pub id: i64,
    pub level: String,
    pub category: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub request_id: Option<String>,
    pub attempt: Option<i64>,
    pub error_source: Option<String>,
    pub error_stage: Option<String>,
    pub duration_ms: Option<i64>,
    pub diagnostic: Option<Value>,
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
    pub cache_creation_tokens: i64,
    pub cost: Option<f64>,
    pub pricing_revision_id: Option<String>,
    pub quota_multiplier: Option<f64>,
    pub local_adjustment_multiplier: Option<f64>,
    pub service_tier: Option<String>,
    pub cost_state: String,
    pub error_message: Option<String>,
    pub request_id: Option<String>,
    pub attempt: Option<i64>,
    pub error_source: Option<String>,
    pub error_stage: Option<String>,
    pub duration_ms: Option<i64>,
    pub diagnostic: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct ForwardMetrics {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost: f64,
    pub pricing_revision_id: Option<String>,
    pub quota_multiplier: Option<f64>,
    pub local_adjustment_multiplier: Option<f64>,
    pub service_tier: Option<String>,
    pub cost_state: &'static str,
}

impl Default for ForwardMetrics {
    fn default() -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: 0,
            cached_tokens: 0,
            cache_creation_tokens: 0,
            cost: 0.0,
            pricing_revision_id: None,
            quota_multiplier: None,
            local_adjustment_multiplier: None,
            service_tier: None,
            cost_state: "not_applicable",
        }
    }
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
    /// 当 5h 固定窗口仍有效时，表示该窗口的清零时刻；`None` 表示窗口尚未开始（无成功请求）。
    #[serde(default)]
    pub resets_in_5h: Option<DateTime<Utc>>,
    /// 当周固定窗口的清零时刻；`None` 表示窗口尚未开始。
    #[serde(default)]
    pub resets_in_week: Option<DateTime<Utc>>,
    /// 月窗口的到期时刻，固定为 `purchase_expires_on(purchase_date) 00:00`；`None` 表示账号无购买日期。
    #[serde(default)]
    pub resets_in_month: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageWindowKind {
    FiveHours,
    Week,
    Month,
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

#[cfg(test)]
mod tests {
    use super::{
        AccountInput, AppConfig, CLAUDE_DESKTOP_HAIKU_ALIAS, CLAUDE_DESKTOP_OPUS_ALIAS,
        CLAUDE_DESKTOP_SONNET_ALIAS, ClaudeDesktopModels, DEFAULT_OPENCODE_INVITE_URL, RoutingMode,
        normalize_opencode_invite_url, normalize_purchase_date, purchase_expires_on,
    };

    #[test]
    fn claude_desktop_models_map_aliases_and_inherit_by_role_priority() {
        let models = ClaudeDesktopModels {
            sonnet: String::new(),
            opus: "glm-5.2".to_string(),
            haiku: "mimo-v2.5".to_string(),
        };

        assert_eq!(
            models.model_for_alias(CLAUDE_DESKTOP_SONNET_ALIAS),
            Some("glm-5.2")
        );
        assert_eq!(
            models.model_for_alias(CLAUDE_DESKTOP_OPUS_ALIAS),
            Some("glm-5.2")
        );
        assert_eq!(
            models.model_for_alias(CLAUDE_DESKTOP_HAIKU_ALIAS),
            Some("mimo-v2.5")
        );
        assert_eq!(models.model_for_alias("claude-unknown"), None);
    }

    #[test]
    fn claude_desktop_models_reject_unknown_and_all_empty_values() {
        let empty = ClaudeDesktopModels {
            sonnet: String::new(),
            opus: String::new(),
            haiku: String::new(),
        };
        assert!(empty.validate().is_err());

        let unknown = ClaudeDesktopModels {
            sonnet: "not-a-supported-model".to_string(),
            ..ClaudeDesktopModels::default()
        };
        assert!(unknown.validate().is_err());
        assert!(ClaudeDesktopModels::default().validate().is_ok());
    }

    #[test]
    fn purchase_dates_require_canonical_calendar_dates() {
        assert_eq!(
            normalize_purchase_date("2026-07-15").expect("valid date should normalize"),
            "2026-07-15"
        );
        for invalid in ["2026-7-15", " 2026-07-15", "2026-07-15 ", "2026-02-29", ""] {
            assert!(
                normalize_purchase_date(invalid).is_err(),
                "{invalid:?} should be rejected"
            );
        }
    }

    #[test]
    fn purchase_expiry_uses_the_next_natural_month() {
        for (purchase, expected) in [
            ("2026-01-15", "2026-02-15"),
            ("2026-01-31", "2026-02-28"),
            ("2024-01-31", "2024-02-29"),
            ("2024-02-29", "2024-03-29"),
            ("2026-12-31", "2027-01-31"),
        ] {
            assert_eq!(
                purchase_expires_on(purchase).expect("valid date should have an expiry"),
                expected
            );
        }
    }

    #[test]
    fn account_input_accepts_legacy_recharge_date_but_serializes_the_new_name() {
        let input: AccountInput = serde_json::from_value(serde_json::json!({
            "name": "legacy",
            "key": "key",
            "recharge_date": "2026-07-15"
        }))
        .expect("legacy input should deserialize");
        assert_eq!(input.purchase_date.as_deref(), Some("2026-07-15"));

        let json = serde_json::to_value(input).expect("input should serialize");
        assert_eq!(json["purchase_date"], "2026-07-15");
        assert!(json.get("recharge_date").is_none());
    }

    #[test]
    fn routing_mode_defaults_and_rejects_unknown_values() {
        let missing: AppConfig = serde_json::from_value(serde_json::json!({
            "gateway_key": "k"
        }))
        .expect("missing routing fields should default");
        assert_eq!(missing.routing_mode, RoutingMode::StrictPriority);
        assert!(!missing.conversation_sticky);

        for mode in [
            RoutingMode::StrictPriority,
            RoutingMode::StickyGlobal,
            RoutingMode::RoundRobin,
        ] {
            let config = AppConfig {
                routing_mode: mode,
                conversation_sticky: true,
                gateway_key: "k".into(),
                ..AppConfig::default()
            };
            config.validate().expect("valid routing config");
            let encoded = serde_json::to_value(&config).expect("serialize");
            let decoded: AppConfig =
                serde_json::from_value(encoded).expect("round-trip routing config");
            assert_eq!(decoded.routing_mode, mode);
            assert!(decoded.conversation_sticky);
        }

        assert!(
            serde_json::from_value::<AppConfig>(serde_json::json!({
                "gateway_key": "k",
                "routing_mode": "weighted"
            }))
            .is_err()
        );
    }

    #[test]
    fn default_opencode_invite_url_is_allowlisted() {
        assert_eq!(
            normalize_opencode_invite_url(DEFAULT_OPENCODE_INVITE_URL).unwrap(),
            DEFAULT_OPENCODE_INVITE_URL
        );
        assert_eq!(
            AppConfig::default().opencode_invite_url,
            DEFAULT_OPENCODE_INVITE_URL
        );
    }

    #[test]
    fn opencode_invite_url_is_https_and_host_allowlisted() {
        assert_eq!(normalize_opencode_invite_url("  ").unwrap(), "");
        assert_eq!(
            normalize_opencode_invite_url("https://opencode.ai/invite/test").unwrap(),
            "https://opencode.ai/invite/test"
        );
        assert!(normalize_opencode_invite_url("https://console.opencode.ai/invite?id=1").is_ok());
        for invalid in [
            "http://opencode.ai/invite/test",
            "https://opencode.ai.evil.test/invite",
            "https://user:pass@opencode.ai/invite",
            "https://example.com/invite",
            "not-a-url",
        ] {
            assert!(
                normalize_opencode_invite_url(invalid).is_err(),
                "accepted unsafe invite URL {invalid:?}"
            );
        }
        assert!(
            normalize_opencode_invite_url(&format!("https://opencode.ai/{}", "x".repeat(2049)))
                .is_err()
        );
    }
}
