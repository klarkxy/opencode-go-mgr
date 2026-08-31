//! Pure immutable Provider/Offering catalog, adapter identities, capability
//! registry, and binding/enablement validation.
//!
//! Custom URL parsing (`reqwest`) and persistence-shaped quota, credit, pricing,
//! and usage-sync records stay in the host crate.

use crate::catalog::{
    CatalogParseError, CredentialKind, OPENCODE_GO_USAGE_URL, QuotaScope, UpstreamAuthScheme,
    UpstreamProtocolKind,
};
use crate::ids::{
    ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_PROVIDER_ID, CPA_ACCOUNT_ID, CPA_OFFERING_ID,
    CPA_PROVIDER_ID, CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID,
    KIMI_CN_OFFERING_ID, KIMI_PROVIDER_ID, MINIMAX_CN_OFFERING_ID, MINIMAX_PROVIDER_ID,
    OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID, ZEN_FREE_ACCOUNT_ID,
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Official Command Code Provider API v1 base. Production GOAT routes this
/// fixed origin; loopback substitutes exist only as a test seam.
pub const COMMAND_CODE_GOAT_BASE_URL: &str = "https://api.commandcode.ai/provider/v1";
pub const COMMAND_CODE_GOAT_HOST: &str = "api.commandcode.ai";
/// Relative to [`COMMAND_CODE_GOAT_BASE_URL`].
pub const COMMAND_CODE_GOAT_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
/// Relative to [`COMMAND_CODE_GOAT_BASE_URL`]. Anthropic models use this path;
/// OpenAI and open-source models use Chat Completions.
pub const COMMAND_CODE_GOAT_MESSAGES_PATH: &str = "/messages";
/// Official public GET `/models` discovery path used for Provider catalog refresh.
pub const COMMAND_CODE_GOAT_MODELS_PATH: &str = "/models";
pub const COMMAND_CODE_GOAT_MODELS_SOURCE: &str = "command_code_get_models";
pub const COMMAND_CODE_GOAT_MODEL_SOURCE: &str = "command_code_verified_models";
/// Public GOAT plan windows. OCG uses these only to project locally priced
/// request logs; Command Code does not expose a machine-readable usage API.
pub const COMMAND_CODE_GOAT_QUOTA_5H: f64 = 14.0;
pub const COMMAND_CODE_GOAT_QUOTA_WEEK: f64 = 35.0;
pub const COMMAND_CODE_GOAT_QUOTA_MONTH: f64 = 70.0;
pub const MAX_COMMAND_CODE_MODELS_CATALOG: usize = 1_000;

/// Official MiniMax CN Token Plan endpoints. The Plan Key is sent as Bearer
/// auth to all three surfaces; redirects stay disabled in the host adapter.
pub const MINIMAX_CN_BASE_URL: &str = "https://api.minimaxi.com/v1";
pub const MINIMAX_CN_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
pub const MINIMAX_CN_MODELS_PATH: &str = "/models";
pub const MINIMAX_CN_USAGE_URL: &str = "https://api.minimaxi.com/v1/token_plan/remains";
pub const MINIMAX_CN_MODEL_SOURCE: &str = "minimax_cn_get_models";
pub const MAX_MINIMAX_CN_MODELS_CATALOG: usize = 1_000;
pub const KIMI_CN_BASE_URL: &str = "https://api.kimi.com/coding/v1";
pub const KIMI_CN_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
pub const KIMI_CN_MODELS_PATH: &str = "/models";
pub const KIMI_CN_USAGE_URL: &str = "https://api.kimi.com/coding/v1/usages";
pub const KIMI_CN_MODEL_SOURCE: &str = "kimi_cn_get_models";
pub const MAX_KIMI_CN_MODELS_CATALOG: usize = 1_000;

/// Models included by the GOAT subscription page. These are the default-on
/// rows in the Provider model/protocol matrix. Models discovered beyond this
/// preset remain visible but default off until an administrator enables them.
pub const COMMAND_CODE_GOAT_INCLUDED_MODEL_IDS: &[&str] = &[
    "gpt-5.6-sol",
    "gpt-5.6-luna",
    "deepseek/deepseek-v4-pro",
    "deepseek/deepseek-v4-flash",
    "deepseek/deepseek-v4-flash-vision-exp",
    "moonshotai/Kimi-K3",
    "moonshotai/Kimi-K2.7-Code",
    "moonshotai/Kimi-K2.7-Code-Highspeed",
    "moonshotai/Kimi-K2.6",
    "moonshotai/Kimi-K2.5",
    "zai-org/GLM-5.3",
    "zai-org/GLM-5.2",
    "zai-org/GLM-5.2-Fast",
    "zai-org/GLM-5.1",
    "zai-org/GLM-5",
    "MiniMaxAI/MiniMax-M3",
    "MiniMaxAI/MiniMax-M2.7",
    "MiniMaxAI/MiniMax-M2.5",
    "xiaomi/mimo-v2.5-pro",
    "xiaomi/mimo-v2.5",
    "Qwen/Qwen3.8-Max",
    "Qwen/Qwen3.8-27B",
    "Qwen/Qwen3.7-Max",
    "Qwen/Qwen3.7-Plus",
    "Qwen/Qwen3.7-Flash",
    "Qwen/Qwen3.6-Max-Preview",
    "Qwen/Qwen3.6-Plus",
    "stepfun/Step-3.7-Flash",
    "stepfun/Step-3.5-Flash",
    "tencent/hy3-paid",
    "google/gemini-3.7-flash",
    "nvidia/nemotron-3-ultra-550b-a55b",
    "thinkingmachines/inkling",
    "thinkingmachines/inkling-small",
    "stealth/ox-alpha",
    "poolside/laguna-s-2.1-free",
    "meta/muse-spark-1.2",
    "meta/muse-spark-1.2-contributor",
    "xai/grok-4.5",
    "xai/grok-4.6",
];

pub fn command_code_goat_includes_model(model_id: &str) -> bool {
    let model_id = model_id.trim();
    !model_id.is_empty()
        && COMMAND_CODE_GOAT_INCLUDED_MODEL_IDS
            .iter()
            .any(|included| included.eq_ignore_ascii_case(model_id))
}

pub const QUOTA_WINDOW_FIVE_HOURS: &str = "five_hours";
pub const QUOTA_WINDOW_WEEK: &str = "week";
pub const QUOTA_WINDOW_MONTH: &str = "month";
pub const QUOTA_WINDOW_FREE: &str = "free";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinOffering {
    pub provider_id: &'static str,
    pub offering_id: &'static str,
    pub credential_kind: CredentialKind,
    pub quota_scope: QuotaScope,
    pub singleton_account_id: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreationAvailability {
    Available,
    Unavailable,
}

/// Product location for a sealed offering. Registry validation applies to both
/// surfaces; callers use this marker to keep external integrations out of the
/// Providers catalog and generic Add Account flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderProductSurface {
    Provider,
    ExternalIntegration,
}

impl ProviderProductSurface {
    pub const fn is_external_integration(self) -> bool {
        matches!(self, Self::ExternalIntegration)
    }
}

impl CreationAvailability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationPolicy {
    NotRequired,
    Required,
}

impl VerificationPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Required => "required",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionVerificationStatus {
    NotRequired,
    Pending,
    Verified,
    Failed,
}

impl ConnectionVerificationStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Pending => "pending",
            Self::Verified => "verified",
            Self::Failed => "failed",
        }
    }

    pub const fn allows_enablement(self) -> bool {
        matches!(self, Self::NotRequired | Self::Verified)
    }
}

impl TryFrom<&str> for ConnectionVerificationStatus {
    type Error = ProviderBindingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "not_required" => Ok(Self::NotRequired),
            "pending" => Ok(Self::Pending),
            "verified" => Ok(Self::Verified),
            "failed" => Ok(Self::Failed),
            _ => Err(ProviderBindingError::UnknownVerificationStatus(
                value.to_string(),
            )),
        }
    }
}

/// Deterministic fallback when the preferred upstream protocol is disabled.
pub const PROTOCOL_FALLBACK_CHAT_RESPONSES_MESSAGES: &[UpstreamProtocolKind] = &[
    UpstreamProtocolKind::ChatCompletions,
    UpstreamProtocolKind::Responses,
    UpstreamProtocolKind::Messages,
];

/// Protocols whose OpenCode Go / known Zen endpoint, materialization, and
/// auth path the adapter can construct. This is the probe safety ceiling,
/// not static verified support (`MODEL_PROTOCOLS`).
pub const OPENCODE_CONSTRUCTABLE_PROTOCOLS: &[UpstreamProtocolKind] =
    PROTOCOL_FALLBACK_CHAT_RESPONSES_MESSAGES;

/// Command Code documented surfaces have no Responses path.
pub const PROTOCOL_FALLBACK_CHAT_MESSAGES: &[UpstreamProtocolKind] = &[
    UpstreamProtocolKind::ChatCompletions,
    UpstreamProtocolKind::Messages,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlanFormField {
    pub id: &'static str,
    pub kind: &'static str,
    pub required: bool,
    pub immutable_after_create: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinPlan {
    pub offering: BuiltinOffering,
    /// Stable identifier for the persisted Provider-contract scope. External
    /// integrations and account-configured Custom API intentionally have none.
    pub contract_scope_id: Option<&'static str>,
    pub display_name: &'static str,
    pub display_family: &'static str,
    pub product_surface: ProviderProductSurface,
    pub creation_availability: CreationAvailability,
    pub creation_unavailable_reason: Option<&'static str>,
    pub verification_policy: VerificationPolicy,
    pub verification_runtime_availability: &'static str,
    pub routable: bool,
    pub managed_registration: bool,
    pub pricing_availability: &'static str,
    pub usage_availability: &'static str,
    /// Whether the dashboard may persist a user-entered quota percentage for
    /// display when the provider exposes no machine-readable usage endpoint.
    pub manual_usage_calibration: bool,
    pub quota_unit: &'static str,
    pub model_source: &'static str,
    pub key_prefix: Option<&'static str>,
    pub auth_schemes: &'static [UpstreamAuthScheme],
    pub upstream_protocols: &'static [UpstreamProtocolKind],
    pub form_fields: &'static [PlanFormField],
}

const NAME_FIELD: PlanFormField = PlanFormField {
    id: "name",
    kind: "text",
    required: true,
    immutable_after_create: false,
};
const KEY_FIELD: PlanFormField = PlanFormField {
    id: "key",
    kind: "secret",
    required: true,
    immutable_after_create: false,
};
const PURCHASE_DATE_FIELD: PlanFormField = PlanFormField {
    id: "purchase_date",
    kind: "date",
    required: false,
    immutable_after_create: false,
};
const NOTES_FIELD: PlanFormField = PlanFormField {
    id: "notes",
    kind: "text",
    required: false,
    immutable_after_create: false,
};
const ENDPOINT_URL_FIELD: PlanFormField = PlanFormField {
    id: "endpoint_url",
    kind: "url",
    required: true,
    immutable_after_create: false,
};
const PROTOCOL_FIELD: PlanFormField = PlanFormField {
    id: "upstream_protocol",
    kind: "select",
    required: true,
    immutable_after_create: false,
};
const MODELS_FIELD: PlanFormField = PlanFormField {
    id: "model_capabilities",
    kind: "models",
    required: true,
    immutable_after_create: false,
};

const GO_FORM_FIELDS: [PlanFormField; 4] =
    [NAME_FIELD, KEY_FIELD, PURCHASE_DATE_FIELD, NOTES_FIELD];
const GOAT_FORM_FIELDS: [PlanFormField; 4] =
    [NAME_FIELD, KEY_FIELD, PURCHASE_DATE_FIELD, NOTES_FIELD];
const MINIMAX_CN_FORM_FIELDS: [PlanFormField; 4] =
    [NAME_FIELD, KEY_FIELD, PURCHASE_DATE_FIELD, NOTES_FIELD];
const KIMI_CN_FORM_FIELDS: [PlanFormField; 4] =
    [NAME_FIELD, KEY_FIELD, PURCHASE_DATE_FIELD, NOTES_FIELD];
const CUSTOM_FORM_FIELDS: [PlanFormField; 6] = [
    NAME_FIELD,
    KEY_FIELD,
    NOTES_FIELD,
    ENDPOINT_URL_FIELD,
    PROTOCOL_FIELD,
    MODELS_FIELD,
];

const BEARER_AUTH: [UpstreamAuthScheme; 1] = [UpstreamAuthScheme::Bearer];
const CUSTOM_AUTH: [UpstreamAuthScheme; 2] =
    [UpstreamAuthScheme::Bearer, UpstreamAuthScheme::XApiKey];
const GO_PROTOCOLS: [UpstreamProtocolKind; 3] = [
    UpstreamProtocolKind::ChatCompletions,
    UpstreamProtocolKind::Responses,
    UpstreamProtocolKind::Messages,
];
const GOAT_PROTOCOLS: [UpstreamProtocolKind; 2] = [
    UpstreamProtocolKind::ChatCompletions,
    UpstreamProtocolKind::Messages,
];
const CHAT_PROTOCOLS: [UpstreamProtocolKind; 1] = [UpstreamProtocolKind::ChatCompletions];
const CUSTOM_PROTOCOLS: [UpstreamProtocolKind; 3] = [
    UpstreamProtocolKind::ChatCompletions,
    UpstreamProtocolKind::Responses,
    UpstreamProtocolKind::Messages,
];

const fn key_offering(provider_id: &'static str, offering_id: &'static str) -> BuiltinOffering {
    BuiltinOffering {
        provider_id,
        offering_id,
        credential_kind: CredentialKind::ApiKey,
        quota_scope: QuotaScope::Key,
        singleton_account_id: None,
    }
}

pub const BUILTIN_PLANS: [BuiltinPlan; 7] = [
    BuiltinPlan {
        offering: key_offering(OPENCODE_PROVIDER_ID, GO_OFFERING_ID),
        contract_scope_id: Some(OPENCODE_PROVIDER_ID),
        display_name: "OpenCode Go",
        display_family: "OpenCode",
        product_surface: ProviderProductSurface::Provider,
        creation_availability: CreationAvailability::Available,
        creation_unavailable_reason: None,
        verification_policy: VerificationPolicy::NotRequired,
        verification_runtime_availability: "optional",
        routable: true,
        managed_registration: true,
        pricing_availability: "available",
        usage_availability: "available",
        manual_usage_calibration: false,
        quota_unit: "usd",
        model_source: "builtin_go_protocol_table",
        key_prefix: None,
        auth_schemes: &BEARER_AUTH,
        upstream_protocols: &GO_PROTOCOLS,
        form_fields: &GO_FORM_FIELDS,
    },
    BuiltinPlan {
        offering: BuiltinOffering {
            provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
            offering_id: ANONYMOUS_FREE_OFFERING_ID,
            credential_kind: CredentialKind::None,
            quota_scope: QuotaScope::EgressIp,
            singleton_account_id: Some(ZEN_FREE_ACCOUNT_ID),
        },
        contract_scope_id: Some(OPENCODE_ZEN_FREE_PROVIDER_ID),
        display_name: "OpenCode Zen Free",
        display_family: "OpenCode",
        product_surface: ProviderProductSurface::Provider,
        creation_availability: CreationAvailability::Unavailable,
        creation_unavailable_reason: Some(
            "Zen Free is a built-in singleton and cannot be created through the generic account API",
        ),
        verification_policy: VerificationPolicy::NotRequired,
        verification_runtime_availability: "not_applicable",
        routable: true,
        managed_registration: false,
        pricing_availability: "not_applicable",
        usage_availability: "local_state",
        manual_usage_calibration: false,
        quota_unit: "request",
        model_source: "builtin_zen_free_alias",
        key_prefix: None,
        auth_schemes: &[],
        upstream_protocols: &GO_PROTOCOLS,
        form_fields: &[],
    },
    BuiltinPlan {
        offering: key_offering(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID),
        contract_scope_id: Some(COMMAND_CODE_PROVIDER_ID),
        display_name: "Command Code GOAT",
        display_family: "Command Code",
        product_surface: ProviderProductSurface::Provider,
        creation_availability: CreationAvailability::Available,
        creation_unavailable_reason: None,
        verification_policy: VerificationPolicy::NotRequired,
        verification_runtime_availability: "not_applicable",
        routable: true,
        managed_registration: false,
        pricing_availability: "available",
        usage_availability: "local_state",
        manual_usage_calibration: true,
        quota_unit: "usd",
        model_source: COMMAND_CODE_GOAT_MODEL_SOURCE,
        key_prefix: None,
        auth_schemes: &BEARER_AUTH,
        upstream_protocols: &GOAT_PROTOCOLS,
        form_fields: &GOAT_FORM_FIELDS,
    },
    BuiltinPlan {
        offering: key_offering(MINIMAX_PROVIDER_ID, MINIMAX_CN_OFFERING_ID),
        contract_scope_id: Some(MINIMAX_PROVIDER_ID),
        display_name: "MiniMax CN Token Plan",
        display_family: "MiniMax",
        product_surface: ProviderProductSurface::Provider,
        creation_availability: CreationAvailability::Available,
        creation_unavailable_reason: None,
        verification_policy: VerificationPolicy::NotRequired,
        verification_runtime_availability: "not_applicable",
        routable: true,
        managed_registration: false,
        pricing_availability: "unpriced",
        usage_availability: "available",
        manual_usage_calibration: false,
        quota_unit: "request",
        model_source: MINIMAX_CN_MODEL_SOURCE,
        key_prefix: Some("sk-cp"),
        auth_schemes: &BEARER_AUTH,
        upstream_protocols: &CHAT_PROTOCOLS,
        form_fields: &MINIMAX_CN_FORM_FIELDS,
    },
    BuiltinPlan {
        offering: key_offering(KIMI_PROVIDER_ID, KIMI_CN_OFFERING_ID),
        contract_scope_id: Some(KIMI_PROVIDER_ID),
        display_name: "Kimi Code CN",
        display_family: "Kimi",
        product_surface: ProviderProductSurface::Provider,
        creation_availability: CreationAvailability::Available,
        creation_unavailable_reason: None,
        verification_policy: VerificationPolicy::NotRequired,
        verification_runtime_availability: "not_applicable",
        routable: true,
        managed_registration: false,
        pricing_availability: "unpriced",
        usage_availability: "available",
        manual_usage_calibration: false,
        quota_unit: "request",
        model_source: KIMI_CN_MODEL_SOURCE,
        key_prefix: Some("sk-ki"),
        auth_schemes: &BEARER_AUTH,
        upstream_protocols: &CHAT_PROTOCOLS,
        form_fields: &KIMI_CN_FORM_FIELDS,
    },
    BuiltinPlan {
        offering: key_offering(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID),
        contract_scope_id: None,
        display_name: "Custom API",
        display_family: "Custom",
        product_surface: ProviderProductSurface::Provider,
        creation_availability: CreationAvailability::Available,
        creation_unavailable_reason: None,
        verification_policy: VerificationPolicy::Required,
        verification_runtime_availability: "available",
        routable: true,
        managed_registration: false,
        pricing_availability: "unpriced",
        usage_availability: "unavailable",
        manual_usage_calibration: false,
        quota_unit: "token",
        model_source: "account_capabilities",
        key_prefix: None,
        auth_schemes: &CUSTOM_AUTH,
        upstream_protocols: &CUSTOM_PROTOCOLS,
        form_fields: &CUSTOM_FORM_FIELDS,
    },
    BuiltinPlan {
        offering: BuiltinOffering {
            provider_id: CPA_PROVIDER_ID,
            offering_id: CPA_OFFERING_ID,
            credential_kind: CredentialKind::ApiKey,
            quota_scope: QuotaScope::Key,
            singleton_account_id: Some(CPA_ACCOUNT_ID),
        },
        contract_scope_id: None,
        display_name: "CPA Subscription Pool",
        display_family: "CPA",
        product_surface: ProviderProductSurface::ExternalIntegration,
        creation_availability: CreationAvailability::Unavailable,
        creation_unavailable_reason: Some(
            "CPA is a local external integration and cannot be created through the generic account API",
        ),
        verification_policy: VerificationPolicy::Required,
        verification_runtime_availability: "external_integration",
        routable: true,
        managed_registration: false,
        pricing_availability: "unpriced",
        usage_availability: "unavailable",
        manual_usage_calibration: false,
        quota_unit: "request",
        model_source: "cpa_persisted_snapshot",
        key_prefix: None,
        auth_schemes: &BEARER_AUTH,
        upstream_protocols: &CUSTOM_PROTOCOLS,
        form_fields: &[],
    },
];

pub const BUILTIN_OFFERINGS: [BuiltinOffering; 7] = [
    BUILTIN_PLANS[0].offering,
    BUILTIN_PLANS[1].offering,
    BUILTIN_PLANS[2].offering,
    BUILTIN_PLANS[3].offering,
    BUILTIN_PLANS[4].offering,
    BUILTIN_PLANS[5].offering,
    BUILTIN_PLANS[6].offering,
];

pub fn default_provider_id() -> String {
    OPENCODE_PROVIDER_ID.to_string()
}

pub fn default_offering_id() -> String {
    GO_OFFERING_ID.to_string()
}

pub fn default_credential_kind() -> CredentialKind {
    CredentialKind::ApiKey
}

pub fn default_quota_scope() -> QuotaScope {
    QuotaScope::Key
}

pub fn builtin_offering(provider_id: &str, offering_id: &str) -> Option<BuiltinOffering> {
    builtin_plan(provider_id, offering_id).map(|plan| plan.offering)
}

pub fn builtin_plan(provider_id: &str, offering_id: &str) -> Option<BuiltinPlan> {
    BUILTIN_PLANS.iter().copied().find(|plan| {
        plan.offering.provider_id == provider_id && plan.offering.offering_id == offering_id
    })
}

/// Exhaustive, code-owned adapter identity. Not a plugin slot, JSON DSL, or
/// user-defined implementation. Custom API is [`Self::ConfigurableHttp`], not
/// a base class other adapters inherit from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderAdapterKind {
    OpenCodeGo,
    ZenFree,
    CommandCodeGoat,
    MiniMaxCn,
    KimiCn,
    ConfigurableHttp,
    Cpa,
}

impl ProviderAdapterKind {
    pub const ALL: [Self; 7] = [
        Self::OpenCodeGo,
        Self::ZenFree,
        Self::CommandCodeGoat,
        Self::MiniMaxCn,
        Self::KimiCn,
        Self::ConfigurableHttp,
        Self::Cpa,
    ];

    pub fn from_offering(provider_id: &str, offering_id: &str) -> Option<Self> {
        match (provider_id, offering_id) {
            (OPENCODE_PROVIDER_ID, GO_OFFERING_ID) => Some(Self::OpenCodeGo),
            (OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID) => Some(Self::ZenFree),
            (COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID) => Some(Self::CommandCodeGoat),
            (MINIMAX_PROVIDER_ID, MINIMAX_CN_OFFERING_ID) => Some(Self::MiniMaxCn),
            (KIMI_PROVIDER_ID, KIMI_CN_OFFERING_ID) => Some(Self::KimiCn),
            (CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID) => Some(Self::ConfigurableHttp),
            (CPA_PROVIDER_ID, CPA_OFFERING_ID) => Some(Self::Cpa),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenCodeGo => "opencode_go",
            Self::ZenFree => "zen_free",
            Self::CommandCodeGoat => "command_code_goat",
            Self::MiniMaxCn => "minimax_cn",
            Self::KimiCn => "kimi_cn",
            Self::ConfigurableHttp => "configurable_http",
            Self::Cpa => "cpa",
        }
    }

    pub const fn product_surface(self) -> ProviderProductSurface {
        match self {
            Self::Cpa => ProviderProductSurface::ExternalIntegration,
            Self::OpenCodeGo
            | Self::ZenFree
            | Self::CommandCodeGoat
            | Self::MiniMaxCn
            | Self::KimiCn
            | Self::ConfigurableHttp => ProviderProductSurface::Provider,
        }
    }
}

pub fn is_command_code_goat(provider_id: &str, offering_id: &str) -> bool {
    matches!(
        ProviderAdapterKind::from_offering(provider_id, offering_id),
        Some(ProviderAdapterKind::CommandCodeGoat)
    )
}

/// Normalize a GET `/models` JSON object into a de-duplicated, non-empty
/// Command Code catalog. Rejects arrays at the root, empty snapshots, and
/// oversized directories. First spelling of each case-insensitive ID wins.
pub fn parse_command_code_models_catalog(bytes: &[u8]) -> Result<Vec<String>, String> {
    parse_provider_models_catalog(bytes, "Command Code")
}

/// Normalize a Provider GET `/models` response while retaining the caller's
/// sealed Provider label in validation errors.
pub fn parse_provider_models_catalog(
    bytes: &[u8],
    provider_label: &str,
) -> Result<Vec<String>, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| format!("{provider_label} GET /models did not return JSON"))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("{provider_label} GET /models did not return a JSON object"))?;
    let items = object
        .get("data")
        .or_else(|| object.get("models"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!("{provider_label} GET /models did not include a data or models array")
        })?;
    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if models.len() >= MAX_COMMAND_CODE_MODELS_CATALOG {
            return Err(format!(
                "{provider_label} GET /models exceeded {MAX_COMMAND_CODE_MODELS_CATALOG} models"
            ));
        }
        let id = match item {
            serde_json::Value::String(value) => Some(value.as_str()),
            serde_json::Value::Object(object) => object
                .get("id")
                .and_then(|value| value.as_str())
                .or_else(|| object.get("model").and_then(|value| value.as_str())),
            _ => None,
        };
        let Some(id) = id else {
            continue;
        };
        let normalized = validate_custom_model_id(id).map_err(|error| error.to_string())?;
        let key = normalized.to_ascii_lowercase();
        if seen.insert(key) {
            models.push(normalized);
        }
    }
    if models.is_empty() {
        return Err(format!(
            "{provider_label} GET /models returned no usable model ids"
        ));
    }
    Ok(models)
}

pub fn is_custom_api(provider_id: &str, offering_id: &str) -> bool {
    matches!(
        ProviderAdapterKind::from_offering(provider_id, offering_id),
        Some(ProviderAdapterKind::ConfigurableHttp)
    )
}

pub fn is_cpa_external_integration(provider_id: &str, offering_id: &str) -> bool {
    matches!(
        ProviderAdapterKind::from_offering(provider_id, offering_id),
        Some(ProviderAdapterKind::Cpa)
    )
}

/// Static code-owned registry of built-in provider offerings. Lookup is by
/// `(provider_id, offering_id)`; unknown pairs fail closed.
pub struct ProviderRegistry;

impl ProviderRegistry {
    pub fn get(provider_id: &str, offering_id: &str) -> Option<ProviderDescriptor> {
        let plan = builtin_plan(provider_id, offering_id)?;
        let kind = ProviderAdapterKind::from_offering(provider_id, offering_id)?;
        Some(ProviderDescriptor::from_plan(kind, plan))
    }

    pub fn iter() -> impl Iterator<Item = ProviderDescriptor> {
        BUILTIN_PLANS
            .iter()
            .filter_map(|plan| Self::get(plan.offering.provider_id, plan.offering.offering_id))
    }
}

/// Composed capability records selected by the sealed adapter kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProviderCapabilities {
    pub model_catalog: ModelCatalogDescriptor,
    pub inference: InferenceRoutingDescriptor,
    pub protocol_probe: ProtocolProbeDescriptor,
    pub verification: VerificationDescriptor,
    pub usage: UsageDescriptor,
    pub pricing: PricingDescriptor,
    pub card_actions: CardActionsDescriptor,
}

/// Composed capability surfaces for one catalog offering. These are facts for
/// later persistence/UI; this slice does not change dashboard DTOs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub kind: ProviderAdapterKind,
    pub provider_id: &'static str,
    pub offering_id: &'static str,
    pub contract_scope_id: Option<&'static str>,
    pub product_surface: ProviderProductSurface,
    pub model_catalog: ModelCatalogDescriptor,
    pub inference: InferenceRoutingDescriptor,
    pub protocol_probe: ProtocolProbeDescriptor,
    pub verification: VerificationDescriptor,
    pub usage: UsageDescriptor,
    pub pricing: PricingDescriptor,
    pub card_actions: CardActionsDescriptor,
}

impl ProviderDescriptor {
    fn from_plan(kind: ProviderAdapterKind, plan: BuiltinPlan) -> Self {
        Self::from_capabilities(kind, plan, kind.capabilities(plan))
    }

    fn from_capabilities(
        kind: ProviderAdapterKind,
        plan: BuiltinPlan,
        capabilities: ProviderCapabilities,
    ) -> Self {
        Self {
            kind,
            provider_id: plan.offering.provider_id,
            offering_id: plan.offering.offering_id,
            contract_scope_id: plan.contract_scope_id,
            product_surface: plan.product_surface,
            model_catalog: capabilities.model_catalog,
            inference: capabilities.inference,
            protocol_probe: capabilities.protocol_probe,
            verification: capabilities.verification,
            usage: capabilities.usage,
            pricing: capabilities.pricing,
            card_actions: capabilities.card_actions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogKind {
    BuiltinGoProtocolTable,
    ZenFreePersistedSnapshot,
    BuiltinCommandCodeProtocolTable,
    ProviderPersistedSnapshot,
    AccountDeclaredCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCatalogDescriptor {
    pub kind: ModelCatalogKind,
    pub catalog_source: &'static str,
    pub publishes_client_aliases: bool,
    pub admin_explicit_refresh: bool,
    pub overlays_declared_ids: bool,
    pub snapshot_is_adapter_input_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceChannelKind {
    Go,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceOriginKind {
    ConfigUpstreamBase,
    DerivedZenBase,
    OfficialFixed,
    AccountConfigured,
    LocalExternalIntegration,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceAuthDescriptor {
    OpenCodeProtocolDefault,
    Bearer,
    None,
    ProtocolDerivedBearerOrXApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InferenceRoutingDescriptor {
    pub catalog_routable: bool,
    pub production_inference: bool,
    pub channel: Option<InferenceChannelKind>,
    pub credential_kind: CredentialKind,
    pub quota_scope: QuotaScope,
    pub auth: InferenceAuthDescriptor,
    pub follow_redirects: bool,
    pub origin: InferenceOriginKind,
    pub loopback_test_seam_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolMatrixKind {
    OpenCodeModelProtocols,
    CommandCodeNative,
    FixedChatCompletions,
    AccountDeclaredProtocol,
    FixedStandardProtocols,
}

/// Immutable adapter ceiling for explicit protocol probes. Distinct from
/// static/preset verified support, which still begins from `MODEL_PROTOCOLS`
/// (OpenCode/Zen), Command Code native rows, or the Custom account's declared
/// protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralProbeCeiling {
    /// GOAT: request-path probes are unavailable. GOAT production uses saved
    /// GET `/models` facts plus hard-coded family rules.
    Unavailable,
    /// Known OpenCode Go models: Chat Completions, Responses, and Messages
    /// all have constructable `/v1/...` paths and OpenCode auth.
    OpenCodeConstructable,
    /// Known Zen models share OpenCode constructable paths. Unknown `-free`
    /// IDs stay Chat-only. Anything else is empty.
    ZenFreeConstructable,
    /// Configurable HTTP: only the account's immutable declared protocol.
    AccountDeclared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolProbeDescriptor {
    pub request_path_may_trial: bool,
    pub matrix: ProtocolMatrixKind,
    pub unknown_zen_free_defaults_to_chat: bool,
    pub fallback_priority: &'static [UpstreamProtocolKind],
    /// Dedicated admin probe surface. Request paths must stay false.
    pub explicit_probe: bool,
    pub structural_ceiling: StructuralProbeCeiling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationDescriptor {
    pub policy: VerificationPolicy,
    pub runtime_availability: &'static str,
    pub never_auto_enable: bool,
    pub probe_first_declared_model: bool,
    pub uses_get_models: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageContractKind {
    Authoritative,
    LocalState,
    ExperimentalUnavailable,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageDescriptor {
    pub catalog_availability: &'static str,
    pub contract: UsageContractKind,
    pub endpoint: Option<&'static str>,
    pub experimental: bool,
    pub automatic_sync: bool,
    pub authoritative_for_quota: bool,
    pub affects_inference_eligibility: bool,
    pub publishes_capability: bool,
    pub manual_calibration: bool,
    /// When true, dashboard usage projects the process-wide Zen Free
    /// egress-IP cooldown window. Only Zen Free sets this.
    pub egress_ip_shared_cooldown_window: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PricingDescriptor {
    pub availability: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardVerifyAction {
    NotApplicable,
    Optional,
    UnavailableNotImplemented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardActionsDescriptor {
    pub persisted_enable_allowed: bool,
    pub enable_requires_verification: bool,
    pub managed_registration: bool,
    pub fetch_zen_models: bool,
    pub discover_models: bool,
    pub usage_refresh: bool,
    pub manual_usage_calibration: bool,
    pub connection_verify: CardVerifyAction,
    pub protocol_and_auth_immutable_after_create: bool,
    pub protocol_probe: bool,
    pub catalog_refresh: bool,
}

impl ProviderAdapterKind {
    /// Single sealed construction path for each adapter kind. This stays
    /// private so callers consume the immutable [`ProviderDescriptor`].
    fn capabilities(self, plan: BuiltinPlan) -> ProviderCapabilities {
        match self {
            Self::OpenCodeGo => open_code_go_capabilities(plan),
            Self::ZenFree => zen_free_capabilities(plan),
            Self::CommandCodeGoat => command_code_goat_capabilities(plan),
            Self::MiniMaxCn => minimax_cn_capabilities(plan),
            Self::KimiCn => kimi_cn_capabilities(plan),
            Self::ConfigurableHttp => configurable_http_capabilities(plan),
            Self::Cpa => cpa_capabilities(plan),
        }
    }
}

fn catalog_pricing(plan: BuiltinPlan) -> PricingDescriptor {
    PricingDescriptor {
        availability: plan.pricing_availability,
    }
}

fn open_code_go_capabilities(plan: BuiltinPlan) -> ProviderCapabilities {
    ProviderCapabilities {
        model_catalog: ModelCatalogDescriptor {
            kind: ModelCatalogKind::BuiltinGoProtocolTable,
            catalog_source: plan.model_source,
            publishes_client_aliases: true,
            admin_explicit_refresh: true,
            overlays_declared_ids: false,
            snapshot_is_adapter_input_only: false,
        },
        inference: InferenceRoutingDescriptor {
            catalog_routable: plan.routable,
            production_inference: true,
            channel: Some(InferenceChannelKind::Go),
            credential_kind: plan.offering.credential_kind,
            quota_scope: plan.offering.quota_scope,
            auth: InferenceAuthDescriptor::OpenCodeProtocolDefault,
            follow_redirects: true,
            origin: InferenceOriginKind::ConfigUpstreamBase,
            loopback_test_seam_only: false,
        },
        protocol_probe: ProtocolProbeDescriptor {
            request_path_may_trial: false,
            matrix: ProtocolMatrixKind::OpenCodeModelProtocols,
            unknown_zen_free_defaults_to_chat: false,
            fallback_priority: PROTOCOL_FALLBACK_CHAT_RESPONSES_MESSAGES,
            explicit_probe: true,
            structural_ceiling: StructuralProbeCeiling::OpenCodeConstructable,
        },
        verification: VerificationDescriptor {
            policy: plan.verification_policy,
            runtime_availability: plan.verification_runtime_availability,
            never_auto_enable: false,
            probe_first_declared_model: false,
            uses_get_models: false,
        },
        usage: UsageDescriptor {
            catalog_availability: plan.usage_availability,
            contract: UsageContractKind::Authoritative,
            endpoint: Some(OPENCODE_GO_USAGE_URL),
            experimental: false,
            automatic_sync: true,
            authoritative_for_quota: true,
            affects_inference_eligibility: false,
            publishes_capability: true,
            manual_calibration: plan.manual_usage_calibration,
            egress_ip_shared_cooldown_window: false,
        },
        pricing: catalog_pricing(plan),
        card_actions: CardActionsDescriptor {
            persisted_enable_allowed: plan.routable,
            enable_requires_verification: false,
            managed_registration: plan.managed_registration,
            fetch_zen_models: false,
            discover_models: false,
            usage_refresh: true,
            manual_usage_calibration: plan.manual_usage_calibration,
            connection_verify: CardVerifyAction::Optional,
            protocol_and_auth_immutable_after_create: false,
            protocol_probe: true,
            catalog_refresh: true,
        },
    }
}

fn zen_free_capabilities(plan: BuiltinPlan) -> ProviderCapabilities {
    ProviderCapabilities {
        model_catalog: ModelCatalogDescriptor {
            kind: ModelCatalogKind::ZenFreePersistedSnapshot,
            catalog_source: plan.model_source,
            publishes_client_aliases: true,
            admin_explicit_refresh: true,
            overlays_declared_ids: false,
            snapshot_is_adapter_input_only: false,
        },
        inference: InferenceRoutingDescriptor {
            catalog_routable: plan.routable,
            production_inference: true,
            channel: Some(InferenceChannelKind::Free),
            credential_kind: plan.offering.credential_kind,
            quota_scope: plan.offering.quota_scope,
            auth: InferenceAuthDescriptor::None,
            follow_redirects: true,
            origin: InferenceOriginKind::DerivedZenBase,
            loopback_test_seam_only: false,
        },
        protocol_probe: ProtocolProbeDescriptor {
            request_path_may_trial: false,
            matrix: ProtocolMatrixKind::OpenCodeModelProtocols,
            unknown_zen_free_defaults_to_chat: true,
            fallback_priority: PROTOCOL_FALLBACK_CHAT_RESPONSES_MESSAGES,
            explicit_probe: true,
            structural_ceiling: StructuralProbeCeiling::ZenFreeConstructable,
        },
        verification: VerificationDescriptor {
            policy: plan.verification_policy,
            runtime_availability: plan.verification_runtime_availability,
            never_auto_enable: false,
            probe_first_declared_model: false,
            uses_get_models: false,
        },
        usage: UsageDescriptor {
            catalog_availability: plan.usage_availability,
            contract: UsageContractKind::Unavailable,
            endpoint: None,
            experimental: false,
            automatic_sync: false,
            authoritative_for_quota: false,
            affects_inference_eligibility: false,
            publishes_capability: false,
            manual_calibration: plan.manual_usage_calibration,
            egress_ip_shared_cooldown_window: true,
        },
        pricing: catalog_pricing(plan),
        card_actions: CardActionsDescriptor {
            persisted_enable_allowed: plan.routable,
            enable_requires_verification: false,
            managed_registration: plan.managed_registration,
            fetch_zen_models: true,
            discover_models: false,
            usage_refresh: false,
            manual_usage_calibration: plan.manual_usage_calibration,
            connection_verify: CardVerifyAction::NotApplicable,
            protocol_and_auth_immutable_after_create: false,
            protocol_probe: true,
            catalog_refresh: true,
        },
    }
}

fn command_code_goat_capabilities(plan: BuiltinPlan) -> ProviderCapabilities {
    ProviderCapabilities {
        model_catalog: ModelCatalogDescriptor {
            kind: ModelCatalogKind::BuiltinCommandCodeProtocolTable,
            catalog_source: plan.model_source,
            publishes_client_aliases: true,
            admin_explicit_refresh: true,
            overlays_declared_ids: false,
            snapshot_is_adapter_input_only: false,
        },
        inference: InferenceRoutingDescriptor {
            catalog_routable: plan.routable,
            production_inference: true,
            channel: Some(InferenceChannelKind::Go),
            credential_kind: plan.offering.credential_kind,
            quota_scope: plan.offering.quota_scope,
            auth: InferenceAuthDescriptor::Bearer,
            follow_redirects: false,
            origin: InferenceOriginKind::OfficialFixed,
            loopback_test_seam_only: false,
        },
        protocol_probe: ProtocolProbeDescriptor {
            request_path_may_trial: false,
            matrix: ProtocolMatrixKind::CommandCodeNative,
            unknown_zen_free_defaults_to_chat: false,
            fallback_priority: PROTOCOL_FALLBACK_CHAT_MESSAGES,
            explicit_probe: false,
            structural_ceiling: StructuralProbeCeiling::Unavailable,
        },
        verification: VerificationDescriptor {
            policy: plan.verification_policy,
            runtime_availability: plan.verification_runtime_availability,
            never_auto_enable: false,
            probe_first_declared_model: false,
            uses_get_models: false,
        },
        usage: UsageDescriptor {
            catalog_availability: plan.usage_availability,
            contract: UsageContractKind::LocalState,
            endpoint: None,
            experimental: false,
            automatic_sync: false,
            authoritative_for_quota: false,
            affects_inference_eligibility: false,
            publishes_capability: true,
            manual_calibration: plan.manual_usage_calibration,
            egress_ip_shared_cooldown_window: false,
        },
        pricing: catalog_pricing(plan),
        card_actions: CardActionsDescriptor {
            persisted_enable_allowed: plan.routable,
            enable_requires_verification: false,
            managed_registration: plan.managed_registration,
            fetch_zen_models: false,
            discover_models: false,
            usage_refresh: false,
            manual_usage_calibration: plan.manual_usage_calibration,
            connection_verify: CardVerifyAction::NotApplicable,
            protocol_and_auth_immutable_after_create: false,
            protocol_probe: false,
            catalog_refresh: true,
        },
    }
}

fn minimax_cn_capabilities(plan: BuiltinPlan) -> ProviderCapabilities {
    ProviderCapabilities {
        model_catalog: ModelCatalogDescriptor {
            kind: ModelCatalogKind::ProviderPersistedSnapshot,
            catalog_source: plan.model_source,
            publishes_client_aliases: true,
            admin_explicit_refresh: true,
            overlays_declared_ids: false,
            snapshot_is_adapter_input_only: false,
        },
        inference: InferenceRoutingDescriptor {
            catalog_routable: plan.routable,
            production_inference: true,
            channel: Some(InferenceChannelKind::Go),
            credential_kind: plan.offering.credential_kind,
            quota_scope: plan.offering.quota_scope,
            auth: InferenceAuthDescriptor::Bearer,
            follow_redirects: false,
            origin: InferenceOriginKind::OfficialFixed,
            loopback_test_seam_only: false,
        },
        protocol_probe: ProtocolProbeDescriptor {
            request_path_may_trial: false,
            matrix: ProtocolMatrixKind::FixedChatCompletions,
            unknown_zen_free_defaults_to_chat: false,
            fallback_priority: &CHAT_PROTOCOLS,
            explicit_probe: false,
            structural_ceiling: StructuralProbeCeiling::Unavailable,
        },
        verification: VerificationDescriptor {
            policy: plan.verification_policy,
            runtime_availability: plan.verification_runtime_availability,
            never_auto_enable: false,
            probe_first_declared_model: false,
            uses_get_models: false,
        },
        usage: UsageDescriptor {
            catalog_availability: plan.usage_availability,
            contract: UsageContractKind::Authoritative,
            endpoint: Some(MINIMAX_CN_USAGE_URL),
            experimental: false,
            automatic_sync: false,
            authoritative_for_quota: false,
            affects_inference_eligibility: false,
            publishes_capability: true,
            manual_calibration: false,
            egress_ip_shared_cooldown_window: false,
        },
        pricing: catalog_pricing(plan),
        card_actions: CardActionsDescriptor {
            persisted_enable_allowed: plan.routable,
            enable_requires_verification: false,
            managed_registration: false,
            fetch_zen_models: false,
            discover_models: false,
            usage_refresh: true,
            manual_usage_calibration: false,
            connection_verify: CardVerifyAction::NotApplicable,
            protocol_and_auth_immutable_after_create: false,
            protocol_probe: false,
            catalog_refresh: true,
        },
    }
}

fn kimi_cn_capabilities(plan: BuiltinPlan) -> ProviderCapabilities {
    ProviderCapabilities {
        model_catalog: ModelCatalogDescriptor {
            kind: ModelCatalogKind::ProviderPersistedSnapshot,
            catalog_source: plan.model_source,
            publishes_client_aliases: true,
            admin_explicit_refresh: true,
            overlays_declared_ids: false,
            snapshot_is_adapter_input_only: false,
        },
        inference: InferenceRoutingDescriptor {
            catalog_routable: plan.routable,
            production_inference: true,
            channel: Some(InferenceChannelKind::Go),
            credential_kind: plan.offering.credential_kind,
            quota_scope: plan.offering.quota_scope,
            auth: InferenceAuthDescriptor::Bearer,
            follow_redirects: false,
            origin: InferenceOriginKind::OfficialFixed,
            loopback_test_seam_only: false,
        },
        protocol_probe: ProtocolProbeDescriptor {
            request_path_may_trial: false,
            matrix: ProtocolMatrixKind::FixedChatCompletions,
            unknown_zen_free_defaults_to_chat: false,
            fallback_priority: &CHAT_PROTOCOLS,
            explicit_probe: false,
            structural_ceiling: StructuralProbeCeiling::Unavailable,
        },
        verification: VerificationDescriptor {
            policy: plan.verification_policy,
            runtime_availability: plan.verification_runtime_availability,
            never_auto_enable: false,
            probe_first_declared_model: false,
            uses_get_models: false,
        },
        usage: UsageDescriptor {
            catalog_availability: plan.usage_availability,
            contract: UsageContractKind::Authoritative,
            endpoint: Some(KIMI_CN_USAGE_URL),
            experimental: false,
            automatic_sync: false,
            authoritative_for_quota: false,
            affects_inference_eligibility: false,
            publishes_capability: true,
            manual_calibration: false,
            egress_ip_shared_cooldown_window: false,
        },
        pricing: catalog_pricing(plan),
        card_actions: CardActionsDescriptor {
            persisted_enable_allowed: plan.routable,
            enable_requires_verification: false,
            managed_registration: false,
            fetch_zen_models: false,
            discover_models: false,
            usage_refresh: true,
            manual_usage_calibration: false,
            connection_verify: CardVerifyAction::NotApplicable,
            protocol_and_auth_immutable_after_create: false,
            protocol_probe: false,
            catalog_refresh: true,
        },
    }
}

fn configurable_http_capabilities(plan: BuiltinPlan) -> ProviderCapabilities {
    ProviderCapabilities {
        model_catalog: ModelCatalogDescriptor {
            kind: ModelCatalogKind::AccountDeclaredCapabilities,
            catalog_source: plan.model_source,
            publishes_client_aliases: false,
            admin_explicit_refresh: false,
            overlays_declared_ids: true,
            snapshot_is_adapter_input_only: false,
        },
        inference: InferenceRoutingDescriptor {
            catalog_routable: plan.routable,
            production_inference: true,
            channel: Some(InferenceChannelKind::Go),
            credential_kind: plan.offering.credential_kind,
            quota_scope: plan.offering.quota_scope,
            auth: InferenceAuthDescriptor::ProtocolDerivedBearerOrXApiKey,
            follow_redirects: false,
            origin: InferenceOriginKind::AccountConfigured,
            loopback_test_seam_only: false,
        },
        protocol_probe: ProtocolProbeDescriptor {
            request_path_may_trial: false,
            matrix: ProtocolMatrixKind::AccountDeclaredProtocol,
            unknown_zen_free_defaults_to_chat: false,
            fallback_priority: PROTOCOL_FALLBACK_CHAT_RESPONSES_MESSAGES,
            explicit_probe: true,
            structural_ceiling: StructuralProbeCeiling::AccountDeclared,
        },
        verification: VerificationDescriptor {
            policy: plan.verification_policy,
            runtime_availability: plan.verification_runtime_availability,
            never_auto_enable: true,
            probe_first_declared_model: true,
            uses_get_models: false,
        },
        usage: UsageDescriptor {
            catalog_availability: plan.usage_availability,
            contract: UsageContractKind::Unavailable,
            endpoint: None,
            experimental: false,
            automatic_sync: false,
            authoritative_for_quota: false,
            affects_inference_eligibility: false,
            publishes_capability: false,
            manual_calibration: plan.manual_usage_calibration,
            egress_ip_shared_cooldown_window: false,
        },
        pricing: catalog_pricing(plan),
        card_actions: CardActionsDescriptor {
            persisted_enable_allowed: plan.routable,
            enable_requires_verification: false,
            managed_registration: plan.managed_registration,
            fetch_zen_models: false,
            discover_models: true,
            usage_refresh: false,
            manual_usage_calibration: plan.manual_usage_calibration,
            connection_verify: CardVerifyAction::Optional,
            protocol_and_auth_immutable_after_create: false,
            protocol_probe: true,
            catalog_refresh: false,
        },
    }
}

fn cpa_capabilities(plan: BuiltinPlan) -> ProviderCapabilities {
    ProviderCapabilities {
        model_catalog: ModelCatalogDescriptor {
            kind: ModelCatalogKind::ProviderPersistedSnapshot,
            catalog_source: plan.model_source,
            publishes_client_aliases: true,
            admin_explicit_refresh: true,
            overlays_declared_ids: false,
            snapshot_is_adapter_input_only: false,
        },
        inference: InferenceRoutingDescriptor {
            catalog_routable: plan.routable,
            production_inference: true,
            // CPA participates in ordinary keyed account selection rather than
            // the Zen egress-IP special channel.
            channel: Some(InferenceChannelKind::Go),
            credential_kind: plan.offering.credential_kind,
            quota_scope: plan.offering.quota_scope,
            auth: InferenceAuthDescriptor::Bearer,
            follow_redirects: false,
            origin: InferenceOriginKind::LocalExternalIntegration,
            loopback_test_seam_only: false,
        },
        protocol_probe: ProtocolProbeDescriptor {
            request_path_may_trial: false,
            matrix: ProtocolMatrixKind::FixedStandardProtocols,
            unknown_zen_free_defaults_to_chat: false,
            fallback_priority: PROTOCOL_FALLBACK_CHAT_RESPONSES_MESSAGES,
            explicit_probe: false,
            structural_ceiling: StructuralProbeCeiling::Unavailable,
        },
        verification: VerificationDescriptor {
            policy: plan.verification_policy,
            runtime_availability: plan.verification_runtime_availability,
            never_auto_enable: true,
            probe_first_declared_model: false,
            uses_get_models: true,
        },
        usage: UsageDescriptor {
            catalog_availability: plan.usage_availability,
            contract: UsageContractKind::Unavailable,
            endpoint: None,
            experimental: false,
            automatic_sync: false,
            authoritative_for_quota: false,
            affects_inference_eligibility: false,
            publishes_capability: false,
            manual_calibration: false,
            egress_ip_shared_cooldown_window: false,
        },
        pricing: catalog_pricing(plan),
        card_actions: CardActionsDescriptor {
            persisted_enable_allowed: plan.routable,
            enable_requires_verification: false,
            managed_registration: false,
            fetch_zen_models: false,
            discover_models: false,
            usage_refresh: false,
            manual_usage_calibration: false,
            connection_verify: CardVerifyAction::NotApplicable,
            protocol_and_auth_immutable_after_create: true,
            protocol_probe: false,
            catalog_refresh: true,
        },
    }
}

pub fn default_verification_status(plan: BuiltinPlan) -> ConnectionVerificationStatus {
    match plan.verification_policy {
        VerificationPolicy::NotRequired => ConnectionVerificationStatus::NotRequired,
        VerificationPolicy::Required => ConnectionVerificationStatus::Pending,
    }
}

/// Catalog-backed enablement capability. Only `routable` offerings may persist
/// `enabled=true`. Unknown offerings fail closed.
pub const fn plan_allows_enablement(plan: BuiltinPlan) -> bool {
    plan.routable
}

pub fn offering_allows_enablement(provider_id: &str, offering_id: &str) -> bool {
    builtin_plan(provider_id, offering_id).is_some_and(plan_allows_enablement)
}

/// Reject `enabled=true` for catalogued-but-unroutable offerings. Disabled
/// drafts skip the check so they can still be created and edited.
pub fn ensure_enabled_offering_is_routable(
    provider_id: &str,
    offering_id: &str,
    enabled: bool,
) -> Result<(), ProviderBindingError> {
    if !enabled {
        return Ok(());
    }
    ensure_offering_can_enable(provider_id, offering_id)
}

pub fn ensure_offering_can_enable(
    provider_id: &str,
    offering_id: &str,
) -> Result<(), ProviderBindingError> {
    match builtin_plan(provider_id, offering_id) {
        Some(plan) if plan_allows_enablement(plan) => Ok(()),
        Some(plan) => Err(ProviderBindingError::EnablementNotRoutable {
            provider_id: plan.offering.provider_id,
            offering_id: plan.offering.offering_id,
            display_name: plan.display_name,
        }),
        None => Err(ProviderBindingError::UnknownOffering {
            provider_id: provider_id.to_string(),
            offering_id: offering_id.to_string(),
        }),
    }
}

pub fn plan_requires_custom_config(plan: BuiltinPlan) -> bool {
    matches!(
        ProviderAdapterKind::from_offering(plan.offering.provider_id, plan.offering.offering_id),
        Some(ProviderAdapterKind::ConfigurableHttp)
    )
}

pub fn validate_plan_key(plan: BuiltinPlan, key: &str) -> Result<(), ProviderBindingError> {
    if plan.offering.credential_kind == CredentialKind::None {
        return Ok(());
    }
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(ProviderBindingError::KeyRequired);
    }
    if let Some(prefix) = plan.key_prefix {
        if !trimmed.starts_with(prefix) {
            return Err(ProviderBindingError::KeyPrefixMismatch {
                provider_id: plan.offering.provider_id.to_string(),
                offering_id: plan.offering.offering_id.to_string(),
                prefix: prefix.to_string(),
            });
        }
    }
    Ok(())
}

pub fn validate_custom_model_id(model_id: &str) -> Result<String, ProviderBindingError> {
    let trimmed = model_id.trim();
    if trimmed.is_empty() {
        return Err(ProviderBindingError::InvalidModelId(
            "model id is required".to_string(),
        ));
    }
    if trimmed.chars().count() > 200 {
        return Err(ProviderBindingError::InvalidModelId(
            "model id is too long".to_string(),
        ));
    }
    if trimmed.contains('\0') || trimmed.chars().any(char::is_control) {
        return Err(ProviderBindingError::InvalidModelId(
            "model id must not contain control characters".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_account_binding(
    account_id: &str,
    provider_id: &str,
    offering_id: &str,
    credential_kind: CredentialKind,
    quota_scope: QuotaScope,
) -> Result<(), ProviderBindingError> {
    let offering = builtin_offering(provider_id, offering_id).ok_or_else(|| {
        ProviderBindingError::UnknownOffering {
            provider_id: provider_id.to_string(),
            offering_id: offering_id.to_string(),
        }
    })?;
    if offering.credential_kind != credential_kind || offering.quota_scope != quota_scope {
        return Err(ProviderBindingError::BindingMismatch {
            provider_id: provider_id.to_string(),
            offering_id: offering_id.to_string(),
        });
    }
    match offering.singleton_account_id {
        Some(singleton) if account_id != singleton => {
            Err(ProviderBindingError::SingletonAccountRequired(singleton))
        }
        None if account_id == ZEN_FREE_ACCOUNT_ID || account_id == CPA_ACCOUNT_ID => Err(
            ProviderBindingError::ReservedAccountId(if account_id == CPA_ACCOUNT_ID {
                CPA_ACCOUNT_ID
            } else {
                ZEN_FREE_ACCOUNT_ID
            }),
        ),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderBindingError {
    UnknownOffering {
        provider_id: String,
        offering_id: String,
    },
    UnknownCredentialKind(String),
    UnknownQuotaScope(String),
    BindingMismatch {
        provider_id: String,
        offering_id: String,
    },
    SingletonAccountRequired(&'static str),
    ReservedAccountId(&'static str),
    UnknownVerificationStatus(String),
    UnknownUpstreamProtocol(String),
    UnknownAuthScheme(String),
    KeyRequired,
    KeyPrefixMismatch {
        provider_id: String,
        offering_id: String,
        prefix: String,
    },
    InvalidCustomBaseUrl(String),
    InvalidModelId(String),
    EnablementNotRoutable {
        provider_id: &'static str,
        offering_id: &'static str,
        display_name: &'static str,
    },
}

impl fmt::Display for ProviderBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOffering {
                provider_id,
                offering_id,
            } => write!(f, "unknown provider offering `{provider_id}/{offering_id}`"),
            Self::UnknownCredentialKind(value) => {
                write!(f, "unknown credential kind `{value}`")
            }
            Self::UnknownQuotaScope(value) => write!(f, "unknown quota scope `{value}`"),
            Self::BindingMismatch {
                provider_id,
                offering_id,
            } => write!(
                f,
                "provider binding does not match `{provider_id}/{offering_id}`"
            ),
            Self::SingletonAccountRequired(id) => {
                write!(f, "provider offering requires singleton account `{id}`")
            }
            Self::ReservedAccountId(id) => write!(f, "account id `{id}` is reserved"),
            Self::UnknownVerificationStatus(value) => {
                write!(f, "unknown verification status `{value}`")
            }
            Self::UnknownUpstreamProtocol(value) => {
                write!(f, "unknown upstream protocol `{value}`")
            }
            Self::UnknownAuthScheme(value) => write!(f, "unknown auth scheme `{value}`"),
            Self::KeyRequired => write!(f, "key is required"),
            Self::KeyPrefixMismatch {
                provider_id,
                offering_id,
                prefix,
            } => write!(
                f,
                "provider offering `{provider_id}/{offering_id}` requires key prefix `{prefix}`"
            ),
            Self::InvalidCustomBaseUrl(message) | Self::InvalidModelId(message) => {
                f.write_str(message)
            }
            Self::EnablementNotRoutable { display_name, .. } => write!(
                f,
                "{display_name} is catalogued but is not routable in this release"
            ),
        }
    }
}

impl std::error::Error for ProviderBindingError {}

impl From<CatalogParseError> for ProviderBindingError {
    fn from(error: CatalogParseError) -> Self {
        match error {
            CatalogParseError::UnknownCredentialKind(value) => Self::UnknownCredentialKind(value),
            CatalogParseError::UnknownQuotaScope(value) => Self::UnknownQuotaScope(value),
            CatalogParseError::UnknownUpstreamProtocol(value) => {
                Self::UnknownUpstreamProtocol(value)
            }
            CatalogParseError::UnknownAuthScheme(value) => Self::UnknownAuthScheme(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{
        CatalogParseError, CredentialKind, OPENCODE_GO_USAGE_URL, QuotaScope, UpstreamAuthScheme,
        UpstreamProtocolKind,
    };
    use crate::ids::{
        ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
        COMMAND_CODE_PROVIDER_ID, CPA_ACCOUNT_ID, CPA_OFFERING_ID, CPA_PROVIDER_ID,
        CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID,
        OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID, ZEN_FREE_ACCOUNT_ID,
    };

    #[test]
    fn catalog_parse_errors_map_to_provider_binding_errors() {
        assert!(matches!(
            ProviderBindingError::from(CredentialKind::try_from("cookie").unwrap_err()),
            ProviderBindingError::UnknownCredentialKind(value) if value == "cookie"
        ));
        assert!(matches!(
            ProviderBindingError::from(QuotaScope::try_from("account").unwrap_err()),
            ProviderBindingError::UnknownQuotaScope(value) if value == "account"
        ));
        assert!(matches!(
            ProviderBindingError::from(UpstreamProtocolKind::try_from("gemini").unwrap_err()),
            ProviderBindingError::UnknownUpstreamProtocol(value) if value == "gemini"
        ));
        assert!(matches!(
            ProviderBindingError::from(UpstreamAuthScheme::try_from("basic").unwrap_err()),
            ProviderBindingError::UnknownAuthScheme(value) if value == "basic"
        ));
    }

    #[test]
    fn builtin_pairs_derive_credential_and_quota_scope() {
        let goat = builtin_offering(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert_eq!(goat.credential_kind, CredentialKind::ApiKey);
        assert_eq!(goat.quota_scope, QuotaScope::Key);

        let free =
            builtin_offering(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID).unwrap();
        assert_eq!(free.credential_kind, CredentialKind::None);
        assert_eq!(free.quota_scope, QuotaScope::EgressIp);
        assert_eq!(free.singleton_account_id, Some(ZEN_FREE_ACCOUNT_ID));

        let cpa = builtin_offering(CPA_PROVIDER_ID, CPA_OFFERING_ID).unwrap();
        assert_eq!(cpa.credential_kind, CredentialKind::ApiKey);
        assert_eq!(cpa.quota_scope, QuotaScope::Key);
        assert_eq!(cpa.singleton_account_id, Some(CPA_ACCOUNT_ID));
    }

    #[test]
    fn goat_included_model_set_is_exact_unique_and_mode_gated() {
        assert_eq!(COMMAND_CODE_GOAT_INCLUDED_MODEL_IDS.len(), 40);
        let mut unique = std::collections::HashSet::new();
        for model in COMMAND_CODE_GOAT_INCLUDED_MODEL_IDS {
            assert!(
                unique.insert(model.to_ascii_lowercase()),
                "duplicate GOAT model {model}"
            );
        }
        assert!(command_code_goat_includes_model(
            "deepseek/deepseek-v4-flash"
        ));
        assert!(command_code_goat_includes_model("XAI/GROK-4.6"));
        assert!(!command_code_goat_includes_model(
            "anthropic/claude-opus-4.1"
        ));
    }

    #[test]
    fn singleton_and_pair_validation_is_fail_closed() {
        assert!(
            validate_account_binding(
                "account-1",
                OPENCODE_PROVIDER_ID,
                GO_OFFERING_ID,
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_ok()
        );
        assert!(
            validate_account_binding(
                "account-1",
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID,
                CredentialKind::None,
                QuotaScope::EgressIp,
            )
            .is_err()
        );
        assert!(
            validate_account_binding(
                CPA_ACCOUNT_ID,
                CPA_PROVIDER_ID,
                CPA_OFFERING_ID,
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_ok()
        );
        assert!(
            validate_account_binding(
                "account-1",
                CPA_PROVIDER_ID,
                CPA_OFFERING_ID,
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_err()
        );
        assert!(
            validate_account_binding(
                ZEN_FREE_ACCOUNT_ID,
                OPENCODE_PROVIDER_ID,
                GO_OFFERING_ID,
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_err()
        );
        assert!(
            validate_account_binding(
                "account-1",
                "unknown-provider",
                "unknown-offering",
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_err()
        );
    }

    #[test]
    fn catalog_hardcodes_plans_and_keeps_unverified_offerings_unroutable() {
        assert_eq!(BUILTIN_PLANS.len(), 7);
        let goat = builtin_plan(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert!(goat.routable);
        assert_eq!(goat.verification_policy, VerificationPolicy::NotRequired);
        assert_eq!(goat.verification_runtime_availability, "not_applicable");
        assert_eq!(goat.creation_availability, CreationAvailability::Available);
        assert_eq!(goat.pricing_availability, "available");
        assert_eq!(goat.usage_availability, "local_state");
        assert!(goat.manual_usage_calibration);
        assert_eq!(goat.auth_schemes, &BEARER_AUTH);
        assert_eq!(goat.upstream_protocols, &GOAT_PROTOCOLS);
        assert!(
            !goat
                .upstream_protocols
                .contains(&UpstreamProtocolKind::Responses)
        );
        assert_eq!(goat.model_source, COMMAND_CODE_GOAT_MODEL_SOURCE);
        assert_eq!(
            COMMAND_CODE_GOAT_BASE_URL,
            "https://api.commandcode.ai/provider/v1"
        );
        assert_eq!(COMMAND_CODE_GOAT_HOST, "api.commandcode.ai");
        assert_eq!(COMMAND_CODE_GOAT_CHAT_COMPLETIONS_PATH, "/chat/completions");
        assert_eq!(COMMAND_CODE_GOAT_MESSAGES_PATH, "/messages");
        assert_eq!(COMMAND_CODE_GOAT_MODELS_PATH, "/models");
        assert_eq!(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            "deepseek/deepseek-v4-flash"
        );
        assert!(is_command_code_goat(
            COMMAND_CODE_PROVIDER_ID,
            GOAT_OFFERING_ID
        ));
        assert!(!is_command_code_goat(OPENCODE_PROVIDER_ID, GO_OFFERING_ID));

        let custom = builtin_plan(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).unwrap();
        assert!(custom.routable);
        assert_eq!(custom.verification_runtime_availability, "available");
        assert_eq!(custom.verification_policy, VerificationPolicy::Required);
        assert_eq!(custom.pricing_availability, "unpriced");
        assert_eq!(custom.usage_availability, "unavailable");
        assert!(plan_requires_custom_config(custom));
        assert!(is_custom_api(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID));
        assert!(!is_custom_api(OPENCODE_PROVIDER_ID, GO_OFFERING_ID));

        let cpa = builtin_plan(CPA_PROVIDER_ID, CPA_OFFERING_ID).unwrap();
        assert!(cpa.routable);
        assert_eq!(
            cpa.product_surface,
            ProviderProductSurface::ExternalIntegration
        );
        assert!(cpa.product_surface.is_external_integration());
        assert_eq!(cpa.creation_availability, CreationAvailability::Unavailable);
        assert_eq!(cpa.offering.singleton_account_id, Some(CPA_ACCOUNT_ID));
        assert_eq!(cpa.auth_schemes, &BEARER_AUTH);
        assert_eq!(cpa.upstream_protocols, &CUSTOM_PROTOCOLS);
        assert!(cpa.form_fields.is_empty());
        assert!(is_cpa_external_integration(
            CPA_PROVIDER_ID,
            CPA_OFFERING_ID
        ));
        assert!(!is_cpa_external_integration(
            OPENCODE_PROVIDER_ID,
            GO_OFFERING_ID
        ));

        let go = builtin_plan(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert!(go.routable);
        assert_eq!(
            default_verification_status(go),
            ConnectionVerificationStatus::NotRequired
        );

        for (provider_id, offering_id) in [
            (OPENCODE_PROVIDER_ID, GO_OFFERING_ID),
            (COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID),
            (MINIMAX_PROVIDER_ID, MINIMAX_CN_OFFERING_ID),
            (KIMI_PROVIDER_ID, KIMI_CN_OFFERING_ID),
        ] {
            let plan = builtin_plan(provider_id, offering_id).unwrap();
            assert!(
                plan.form_fields
                    .iter()
                    .any(|field| field.id == "purchase_date"),
                "{provider_id}/{offering_id} must collect its subscription purchase date"
            );
        }
        assert!(
            !custom
                .form_fields
                .iter()
                .any(|field| field.id == "purchase_date")
        );
    }

    #[test]
    fn catalog_enablement_gate_is_fail_closed_for_unroutable_plans() {
        for plan in BUILTIN_PLANS {
            let provider_id = plan.offering.provider_id;
            let offering_id = plan.offering.offering_id;
            assert_eq!(
                plan_allows_enablement(plan),
                plan.routable,
                "{provider_id}/{offering_id}"
            );
            assert_eq!(
                offering_allows_enablement(provider_id, offering_id),
                plan.routable,
                "{provider_id}/{offering_id}"
            );
            assert!(
                ensure_enabled_offering_is_routable(provider_id, offering_id, false).is_ok(),
                "disabled drafts must stay writable: {provider_id}/{offering_id}"
            );
            let enabled = ensure_enabled_offering_is_routable(provider_id, offering_id, true);
            if plan.routable {
                enabled.expect("routable offerings may enable");
                ensure_offering_can_enable(provider_id, offering_id).unwrap();
            } else {
                let error = enabled.expect_err("unroutable offerings must reject enabled=true");
                assert!(
                    matches!(
                        error,
                        ProviderBindingError::EnablementNotRoutable {
                            provider_id: rejected_provider,
                            offering_id: rejected_offering,
                            display_name,
                        } if rejected_provider == provider_id
                            && rejected_offering == offering_id
                            && display_name == plan.display_name
                    ),
                    "{error:?}"
                );
                assert!(error.to_string().contains("not routable"), "{}", error);
            }
        }
        assert!(!offering_allows_enablement(
            "unknown-provider",
            "unknown-offering"
        ));
        assert!(matches!(
            ensure_offering_can_enable("unknown-provider", "unknown-offering"),
            Err(ProviderBindingError::UnknownOffering { .. })
        ));
        let zen = builtin_plan(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID).unwrap();
        assert!(plan_allows_enablement(zen));
        let go = builtin_plan(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert!(plan_allows_enablement(go));
    }

    #[test]
    fn custom_model_ids_stay_stable() {
        assert_eq!(
            validate_custom_model_id("deepseek/deepseek-v4-flash").unwrap(),
            "deepseek/deepseek-v4-flash"
        );
        assert_eq!(validate_custom_model_id("  glm-5.2  ").unwrap(), "glm-5.2");
        assert!(matches!(
            validate_custom_model_id(""),
            Err(ProviderBindingError::InvalidModelId(message)) if message == "model id is required"
        ));
        assert!(matches!(
            validate_custom_model_id("   "),
            Err(ProviderBindingError::InvalidModelId(message)) if message == "model id is required"
        ));
        assert!(matches!(
            validate_custom_model_id(&"a".repeat(201)),
            Err(ProviderBindingError::InvalidModelId(message)) if message == "model id is too long"
        ));
        assert_eq!(
            validate_custom_model_id(&"a".repeat(200)).unwrap().len(),
            200
        );
        assert!(matches!(
            validate_custom_model_id("bad\0id"),
            Err(ProviderBindingError::InvalidModelId(message))
                if message == "model id must not contain control characters"
        ));
        assert!(matches!(
            validate_custom_model_id("bad\nid"),
            Err(ProviderBindingError::InvalidModelId(message))
                if message == "model id must not contain control characters"
        ));
    }

    #[test]
    fn provider_registry_is_exhaustive_for_plans_and_adapter_kinds() {
        let mut seen = std::collections::HashSet::new();
        assert_eq!(ProviderRegistry::iter().count(), BUILTIN_PLANS.len());
        for plan in BUILTIN_PLANS {
            let kind = ProviderAdapterKind::from_offering(
                plan.offering.provider_id,
                plan.offering.offering_id,
            )
            .expect("every catalog plan has an adapter kind");
            seen.insert(kind);
            let descriptor =
                ProviderRegistry::get(plan.offering.provider_id, plan.offering.offering_id)
                    .expect("every catalog plan has a composed descriptor");
            assert_eq!(descriptor.kind, kind);
            assert_eq!(descriptor.provider_id, plan.offering.provider_id);
            assert_eq!(descriptor.offering_id, plan.offering.offering_id);
            assert_eq!(descriptor.inference.catalog_routable, plan.routable);
            assert_eq!(
                descriptor.inference.credential_kind,
                plan.offering.credential_kind
            );
            assert_eq!(descriptor.inference.quota_scope, plan.offering.quota_scope);
            assert_eq!(descriptor.verification.policy, plan.verification_policy);
            assert_eq!(
                descriptor.verification.runtime_availability,
                plan.verification_runtime_availability
            );
            assert_eq!(descriptor.pricing.availability, plan.pricing_availability);
            assert_eq!(
                descriptor.usage.catalog_availability,
                plan.usage_availability
            );
            assert_eq!(
                descriptor.usage.manual_calibration,
                plan.manual_usage_calibration
            );
            assert_eq!(descriptor.model_catalog.catalog_source, plan.model_source);
            assert_eq!(
                descriptor.card_actions.managed_registration,
                plan.managed_registration
            );
            assert_eq!(
                descriptor.card_actions.persisted_enable_allowed,
                plan.routable
            );
            assert!(!descriptor.protocol_probe.request_path_may_trial);
            assert!(!descriptor.protocol_probe.fallback_priority.is_empty());
            assert_eq!(
                descriptor.protocol_probe.explicit_probe,
                descriptor.card_actions.protocol_probe
            );
            assert_eq!(
                descriptor.card_actions.catalog_refresh,
                matches!(
                    kind,
                    ProviderAdapterKind::OpenCodeGo
                        | ProviderAdapterKind::ZenFree
                        | ProviderAdapterKind::CommandCodeGoat
                        | ProviderAdapterKind::MiniMaxCn
                        | ProviderAdapterKind::KimiCn
                        | ProviderAdapterKind::Cpa
                )
            );
            assert_eq!(
                descriptor.card_actions.protocol_probe,
                matches!(
                    kind,
                    ProviderAdapterKind::OpenCodeGo
                        | ProviderAdapterKind::ZenFree
                        | ProviderAdapterKind::ConfigurableHttp
                )
            );
            assert_eq!(
                descriptor.verification.uses_get_models,
                kind == ProviderAdapterKind::Cpa
            );
            assert_eq!(
                descriptor.usage.egress_ip_shared_cooldown_window,
                kind == ProviderAdapterKind::ZenFree
            );
            match kind {
                ProviderAdapterKind::OpenCodeGo
                | ProviderAdapterKind::ZenFree
                | ProviderAdapterKind::CommandCodeGoat
                | ProviderAdapterKind::MiniMaxCn
                | ProviderAdapterKind::KimiCn
                | ProviderAdapterKind::ConfigurableHttp
                | ProviderAdapterKind::Cpa => {
                    assert!(descriptor.inference.production_inference);
                    assert!(descriptor.inference.catalog_routable);
                }
            }
        }
        for kind in ProviderAdapterKind::ALL {
            assert!(
                seen.contains(&kind),
                "{kind:?} must be wired to at least one catalog offering"
            );
        }
        assert_eq!(seen.len(), ProviderAdapterKind::ALL.len());
        assert!(ProviderAdapterKind::from_offering("unknown", "unknown").is_none());
        assert!(ProviderRegistry::get("unknown", "unknown").is_none());
        assert_eq!(ProviderAdapterKind::ALL.len(), 7);
    }

    #[test]
    fn adapter_descriptors_preserve_current_capability_decisions() {
        let go = ProviderRegistry::get(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert_eq!(go.kind, ProviderAdapterKind::OpenCodeGo);
        assert_eq!(
            go.inference.auth,
            InferenceAuthDescriptor::OpenCodeProtocolDefault
        );
        assert!(go.inference.follow_redirects);
        assert_eq!(go.inference.origin, InferenceOriginKind::ConfigUpstreamBase);
        assert!(go.usage.automatic_sync);
        assert!(go.usage.authoritative_for_quota);
        assert_eq!(go.usage.endpoint, Some(OPENCODE_GO_USAGE_URL));
        assert_eq!(OPENCODE_GO_USAGE_URL, "https://opencode.ai/zen/go/v1/usage");
        assert_eq!(go.usage.contract, UsageContractKind::Authoritative);
        assert!(go.usage.publishes_capability);
        assert!(!go.usage.egress_ip_shared_cooldown_window);
        assert_eq!(
            go.protocol_probe.matrix,
            ProtocolMatrixKind::OpenCodeModelProtocols
        );
        assert!(go.protocol_probe.explicit_probe);
        assert_eq!(
            go.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::OpenCodeConstructable
        );
        assert_eq!(
            go.card_actions.connection_verify,
            CardVerifyAction::Optional
        );
        assert!(go.card_actions.usage_refresh);
        assert!(go.card_actions.protocol_probe);
        assert!(go.card_actions.catalog_refresh);

        let zen = ProviderRegistry::get(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID)
            .unwrap();
        assert_eq!(zen.kind, ProviderAdapterKind::ZenFree);
        assert_eq!(zen.inference.auth, InferenceAuthDescriptor::None);
        assert_eq!(zen.inference.credential_kind, CredentialKind::None);
        assert_eq!(zen.inference.quota_scope, QuotaScope::EgressIp);
        assert_eq!(zen.inference.channel, Some(InferenceChannelKind::Free));
        assert!(zen.inference.follow_redirects);
        assert!(zen.model_catalog.admin_explicit_refresh);
        assert!(zen.protocol_probe.unknown_zen_free_defaults_to_chat);
        assert!(zen.protocol_probe.explicit_probe);
        assert_eq!(
            zen.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::ZenFreeConstructable
        );
        assert!(!zen.usage.experimental);
        assert!(zen.usage.egress_ip_shared_cooldown_window);
        assert!(zen.card_actions.fetch_zen_models);
        assert!(zen.card_actions.protocol_probe);
        assert!(zen.card_actions.catalog_refresh);
        assert_eq!(
            zen.card_actions.connection_verify,
            CardVerifyAction::NotApplicable
        );

        let goat = ProviderRegistry::get(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert_eq!(goat.kind, ProviderAdapterKind::CommandCodeGoat);
        assert!(!goat.inference.loopback_test_seam_only);
        assert!(goat.inference.production_inference);
        assert!(goat.inference.catalog_routable);
        assert!(!goat.inference.follow_redirects);
        assert_eq!(goat.inference.auth, InferenceAuthDescriptor::Bearer);
        assert!(!goat.usage.experimental);
        assert!(goat.usage.publishes_capability);
        assert_eq!(goat.usage.contract, UsageContractKind::LocalState);
        assert!(goat.usage.manual_calibration);
        assert!(!goat.usage.egress_ip_shared_cooldown_window);
        assert_eq!(
            goat.protocol_probe.matrix,
            ProtocolMatrixKind::CommandCodeNative
        );
        assert!(!goat.protocol_probe.explicit_probe);
        assert_eq!(
            goat.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::Unavailable
        );
        assert!(!goat.card_actions.protocol_probe);
        assert!(goat.card_actions.catalog_refresh);
        assert_eq!(
            goat.card_actions.connection_verify,
            CardVerifyAction::NotApplicable
        );
        assert!(!goat.verification.uses_get_models);
        assert!(!goat.verification.never_auto_enable);

        let custom = ProviderRegistry::get(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).unwrap();
        assert_eq!(custom.kind, ProviderAdapterKind::ConfigurableHttp);
        assert_eq!(
            custom.inference.auth,
            InferenceAuthDescriptor::ProtocolDerivedBearerOrXApiKey
        );
        assert!(!custom.inference.follow_redirects);
        assert_eq!(
            custom.inference.origin,
            InferenceOriginKind::AccountConfigured
        );
        assert!(custom.model_catalog.overlays_declared_ids);
        assert!(custom.verification.never_auto_enable);
        assert!(custom.verification.probe_first_declared_model);
        assert!(!custom.usage.publishes_capability);
        assert_eq!(
            custom.card_actions.connection_verify,
            CardVerifyAction::Optional
        );
        assert!(!custom.card_actions.protocol_and_auth_immutable_after_create);
        assert!(!custom.card_actions.enable_requires_verification);
        assert!(custom.card_actions.discover_models);
        assert!(custom.card_actions.protocol_probe);
        assert!(!custom.card_actions.catalog_refresh);
        assert_eq!(
            custom.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::AccountDeclared
        );
        assert!(!custom.usage.egress_ip_shared_cooldown_window);

        assert_ne!(go.kind, ProviderAdapterKind::ConfigurableHttp);
        assert_ne!(zen.kind, ProviderAdapterKind::ConfigurableHttp);
        assert_ne!(goat.kind, ProviderAdapterKind::ConfigurableHttp);
        assert_eq!(
            ProviderAdapterKind::from_offering(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID),
            Some(ProviderAdapterKind::ConfigurableHttp)
        );

        let cpa = ProviderRegistry::get(CPA_PROVIDER_ID, CPA_OFFERING_ID).unwrap();
        assert_eq!(cpa.kind, ProviderAdapterKind::Cpa);
        assert_eq!(
            cpa.product_surface,
            ProviderProductSurface::ExternalIntegration
        );
        assert_eq!(cpa.inference.auth, InferenceAuthDescriptor::Bearer);
        assert_eq!(
            cpa.inference.origin,
            InferenceOriginKind::LocalExternalIntegration
        );
        assert_eq!(
            cpa.model_catalog.kind,
            ModelCatalogKind::ProviderPersistedSnapshot
        );
        assert_eq!(
            cpa.protocol_probe.matrix,
            ProtocolMatrixKind::FixedStandardProtocols
        );
        assert!(!cpa.protocol_probe.explicit_probe);
        assert_eq!(
            cpa.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::Unavailable
        );
        assert!(cpa.verification.never_auto_enable);
        assert!(cpa.verification.uses_get_models);
        assert_eq!(cpa.usage.contract, UsageContractKind::Unavailable);
        assert!(!cpa.usage.publishes_capability);
        assert!(cpa.card_actions.catalog_refresh);
        assert!(!cpa.card_actions.protocol_probe);
    }

    #[test]
    fn descriptor_capabilities_are_built_once_from_the_sealed_kind() {
        for plan in BUILTIN_PLANS {
            let kind = ProviderAdapterKind::from_offering(
                plan.offering.provider_id,
                plan.offering.offering_id,
            )
            .expect("every catalog plan has an adapter kind");
            let descriptor =
                ProviderRegistry::get(plan.offering.provider_id, plan.offering.offering_id)
                    .expect("every catalog plan has a composed descriptor");
            assert_eq!(descriptor.kind, kind);
            assert!(!descriptor.protocol_probe.request_path_may_trial);
            assert!(!descriptor.protocol_probe.fallback_priority.is_empty());
            assert_eq!(descriptor.inference.catalog_routable, plan.routable);
            assert_eq!(descriptor.pricing.availability, plan.pricing_availability);
        }
    }

    #[test]
    fn contract_scopes_are_unique_and_limited_to_ordinary_providers() {
        let mut scopes = std::collections::HashSet::new();
        for plan in BUILTIN_PLANS {
            let descriptor =
                ProviderRegistry::get(plan.offering.provider_id, plan.offering.offering_id)
                    .expect("every built-in plan has a descriptor");
            assert_eq!(descriptor.contract_scope_id, plan.contract_scope_id);
            match plan.product_surface {
                ProviderProductSurface::Provider
                    if is_custom_api(plan.offering.provider_id, plan.offering.offering_id) =>
                {
                    assert_eq!(plan.contract_scope_id, None)
                }
                ProviderProductSurface::Provider => {
                    let scope = plan.contract_scope_id.expect("ordinary Provider scope");
                    assert!(!scope.is_empty());
                    assert!(scopes.insert(scope), "duplicate contract scope `{scope}`");
                    assert_eq!(scope, plan.offering.provider_id);
                }
                ProviderProductSurface::ExternalIntegration => {
                    assert_eq!(plan.contract_scope_id, None)
                }
            }
        }
    }

    #[test]
    fn defaults_verification_status_and_binding_error_messages_stay_stable() {
        assert_eq!(default_provider_id(), OPENCODE_PROVIDER_ID);
        assert_eq!(default_offering_id(), GO_OFFERING_ID);
        assert_eq!(default_credential_kind(), CredentialKind::ApiKey);
        assert_eq!(default_quota_scope(), QuotaScope::Key);
        assert_eq!(CreationAvailability::Available.as_str(), "available");
        assert_eq!(CreationAvailability::Unavailable.as_str(), "unavailable");
        assert_eq!(VerificationPolicy::NotRequired.as_str(), "not_required");
        assert_eq!(VerificationPolicy::Required.as_str(), "required");
        assert_eq!(
            ConnectionVerificationStatus::NotRequired.as_str(),
            "not_required"
        );
        assert!(ConnectionVerificationStatus::NotRequired.allows_enablement());
        assert!(ConnectionVerificationStatus::Verified.allows_enablement());
        assert!(!ConnectionVerificationStatus::Pending.allows_enablement());
        assert!(!ConnectionVerificationStatus::Failed.allows_enablement());
        assert_eq!(
            ConnectionVerificationStatus::try_from("verified").unwrap(),
            ConnectionVerificationStatus::Verified
        );
        assert!(matches!(
            ConnectionVerificationStatus::try_from("unknown"),
            Err(ProviderBindingError::UnknownVerificationStatus(value)) if value == "unknown"
        ));

        let unknown = ProviderBindingError::UnknownOffering {
            provider_id: "p".into(),
            offering_id: "o".into(),
        };
        assert_eq!(unknown.to_string(), "unknown provider offering `p/o`");
        assert_eq!(
            ProviderBindingError::UnknownCredentialKind("cookie".into()).to_string(),
            "unknown credential kind `cookie`"
        );
        assert_eq!(
            ProviderBindingError::UnknownQuotaScope("account".into()).to_string(),
            "unknown quota scope `account`"
        );
        assert_eq!(
            ProviderBindingError::BindingMismatch {
                provider_id: "p".into(),
                offering_id: "o".into(),
            }
            .to_string(),
            "provider binding does not match `p/o`"
        );
        assert_eq!(
            ProviderBindingError::SingletonAccountRequired(ZEN_FREE_ACCOUNT_ID).to_string(),
            format!("provider offering requires singleton account `{ZEN_FREE_ACCOUNT_ID}`")
        );
        assert_eq!(
            ProviderBindingError::ReservedAccountId(ZEN_FREE_ACCOUNT_ID).to_string(),
            format!("account id `{ZEN_FREE_ACCOUNT_ID}` is reserved")
        );
        assert_eq!(
            ProviderBindingError::UnknownVerificationStatus("bogus".into()).to_string(),
            "unknown verification status `bogus`"
        );
        assert_eq!(
            ProviderBindingError::UnknownUpstreamProtocol("gemini".into()).to_string(),
            "unknown upstream protocol `gemini`"
        );
        assert_eq!(
            ProviderBindingError::UnknownAuthScheme("basic".into()).to_string(),
            "unknown auth scheme `basic`"
        );
        assert_eq!(
            ProviderBindingError::KeyRequired.to_string(),
            "key is required"
        );
        assert_eq!(
            ProviderBindingError::KeyPrefixMismatch {
                provider_id: "custom".into(),
                offering_id: "api".into(),
                prefix: "x-".into(),
            }
            .to_string(),
            "provider offering `custom/api` requires key prefix `x-`"
        );
        assert_eq!(
            ProviderBindingError::InvalidCustomBaseUrl("base URL is required".into()).to_string(),
            "base URL is required"
        );
        assert_eq!(
            ProviderBindingError::InvalidModelId("model id is required".into()).to_string(),
            "model id is required"
        );
        assert_eq!(
            ProviderBindingError::EnablementNotRoutable {
                provider_id: COMMAND_CODE_PROVIDER_ID,
                offering_id: GOAT_OFFERING_ID,
                display_name: "Command Code GOAT",
            }
            .to_string(),
            "Command Code GOAT is catalogued but is not routable in this release"
        );
        assert!(matches!(
            ProviderBindingError::from(CatalogParseError::UnknownCredentialKind("cookie".into())),
            ProviderBindingError::UnknownCredentialKind(value) if value == "cookie"
        ));
        assert!(matches!(
            ProviderBindingError::from(CatalogParseError::UnknownQuotaScope("account".into())),
            ProviderBindingError::UnknownQuotaScope(value) if value == "account"
        ));
        assert!(matches!(
            ProviderBindingError::from(CatalogParseError::UnknownUpstreamProtocol("gemini".into())),
            ProviderBindingError::UnknownUpstreamProtocol(value) if value == "gemini"
        ));
        assert!(matches!(
            ProviderBindingError::from(CatalogParseError::UnknownAuthScheme("basic".into())),
            ProviderBindingError::UnknownAuthScheme(value) if value == "basic"
        ));
    }

    #[test]
    fn command_code_models_catalog_parses_openai_list_and_rejects_empty() {
        let parsed = parse_command_code_models_catalog(
            br#"{"object":"list","data":[{"id":"deepseek/deepseek-v4-flash"},{"id":"claude-sonnet-4-6"},{"id":"deepseek/deepseek-v4-flash"}]}"#,
        )
        .unwrap();
        assert_eq!(
            parsed,
            vec![
                "deepseek/deepseek-v4-flash".to_string(),
                "claude-sonnet-4-6".to_string()
            ]
        );
        assert!(parse_command_code_models_catalog(br#"["id"]"#).is_err());
        assert!(parse_command_code_models_catalog(br#"{"data":[]}"#).is_err());
        assert!(
            parse_command_code_models_catalog(br#"{"models":[{"model":"gpt-5.4"}]}"#)
                .is_ok_and(|models| models == ["gpt-5.4"])
        );
        assert!(ensure_offering_can_enable(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).is_ok());
    }

    #[test]
    fn zen_free_key_validation_skips_empty_secret() {
        let zen = builtin_plan(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID).unwrap();
        assert_eq!(zen.offering.credential_kind, CredentialKind::None);
        assert!(validate_plan_key(zen, "").is_ok());
        assert!(validate_plan_key(zen, "   ").is_ok());
        let go = builtin_plan(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert!(matches!(
            validate_plan_key(go, "   "),
            Err(ProviderBindingError::KeyRequired)
        ));
    }
}
