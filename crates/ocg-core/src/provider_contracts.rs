//! Effective provider/custom-endpoint contracts: merge, selection, and views.
//!
//! Persistence lives in [`crate::db`]. This module is the only merge/selection
//! seam: dashboard, materialize, and `/v1/models` read an immutable snapshot
//! captured at request entry. Request paths never discover or probe.

use crate::alias::ProviderMapping;
use crate::custom::CustomAccountRuntime;
use crate::kernel::ids::{
    COMMAND_CODE_PROVIDER_ID, CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, KIMI_PROVIDER_ID,
    MINIMAX_PROVIDER_ID, OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID,
    custom_model_id_matches, normalize_model_name,
};
use crate::kernel::protocol::{ApiFormat, is_known_model, supported_model_protocol_profiles};
use crate::kernel::zen::ZenFreeModelCatalog;
use crate::models::Account;
use crate::provider::{
    COMMAND_CODE_GOAT_BASE_URL, COMMAND_CODE_GOAT_INCLUDED_MODEL_IDS,
    OPENCODE_CONSTRUCTABLE_PROTOCOLS, ProtocolProbeDescriptor, ProviderAdapterKind,
    ProviderRegistry, StructuralProbeCeiling, UpstreamProtocolKind,
    command_code_goat_includes_model,
};
use crate::redaction::sanitize_upstream_error_value_with_known_secret;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

pub const SCOPE_KIND_PROVIDER: &str = "provider";
pub const SCOPE_KIND_CUSTOM_ENDPOINT: &str = "custom_endpoint";

pub const CATALOG_SOURCE_STATIC: &str = "static";
pub const CATALOG_SOURCE_OFFICIAL_ZEN: &str = "official_zen";
pub const CATALOG_SOURCE_CUSTOM_DISCOVERY: &str = "custom_discovery";
pub const CATALOG_SOURCE_DECLARED: &str = "account_declared";
pub const CATALOG_SOURCE_COMMAND_CODE_MODELS: &str = "command_code_get_models";
pub const CATALOG_SOURCE_OPENCODE_MODELS: &str = "opencode_get_models";
pub const CATALOG_SOURCE_MINIMAX_CN_MODELS: &str = "minimax_cn_get_models";
pub const CATALOG_SOURCE_KIMI_CN_MODELS: &str = "kimi_cn_get_models";

pub const NO_ENABLED_UPSTREAM_PROTOCOL: &str =
    "no enabled upstream protocol is available for this model";

const MAX_PROBE_ERROR_CHARS: usize = 500;

pub fn static_protocol_snapshot_date(scope_id: &str) -> Option<&'static str> {
    let provider_id = provider_scope_descriptor(scope_id)?.provider_id;
    match provider_id {
        OPENCODE_PROVIDER_ID => {
            Some(crate::kernel::protocol::OPENCODE_GO_STATIC_PROTOCOL_SNAPSHOT_DATE)
        }
        OPENCODE_ZEN_FREE_PROVIDER_ID => {
            Some(crate::kernel::protocol::ZEN_FREE_STATIC_PROTOCOL_SNAPSHOT_DATE)
        }
        COMMAND_CODE_PROVIDER_ID => {
            Some(crate::kernel::protocol::COMMAND_CODE_GOAT_STATIC_PROTOCOL_SNAPSHOT_DATE)
        }
        MINIMAX_PROVIDER_ID | KIMI_PROVIDER_ID => Some("2026-08-27"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractScopeKind {
    Provider,
    CustomEndpoint,
}

impl ContractScopeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provider => SCOPE_KIND_PROVIDER,
            Self::CustomEndpoint => SCOPE_KIND_CUSTOM_ENDPOINT,
        }
    }
}

impl TryFrom<&str> for ContractScopeKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            SCOPE_KIND_PROVIDER => Ok(Self::Provider),
            SCOPE_KIND_CUSTOM_ENDPOINT => Ok(Self::CustomEndpoint),
            other => Err(format!("unknown contract scope kind `{other}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContractScope {
    Provider(String),
    CustomEndpoint(String),
}

impl ContractScope {
    pub fn provider(provider_id: impl Into<String>) -> Self {
        Self::Provider(provider_id.into())
    }

    pub fn custom_endpoint(account_id: impl Into<String>) -> Self {
        Self::CustomEndpoint(account_id.into())
    }

    pub fn kind(&self) -> ContractScopeKind {
        match self {
            Self::Provider(_) => ContractScopeKind::Provider,
            Self::CustomEndpoint(_) => ContractScopeKind::CustomEndpoint,
        }
    }

    pub fn kind_str(&self) -> &'static str {
        self.kind().as_str()
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Provider(id) | Self::CustomEndpoint(id) => id,
        }
    }

    pub fn parse(kind: &str, id: &str) -> Result<Self, String> {
        let id = id.trim();
        if id.is_empty() {
            return Err("contract scope id is required".to_string());
        }
        match ContractScopeKind::try_from(kind)? {
            ContractScopeKind::Provider => provider_scope_descriptor(id)
                .map(|_| Self::provider(id))
                .ok_or_else(|| format!("unknown provider contract scope `{id}`")),
            ContractScopeKind::CustomEndpoint => Ok(Self::custom_endpoint(id)),
        }
    }

    pub fn from_account(account: &crate::models::Account) -> Option<Self> {
        Self::from_offering(
            &account.provider_id,
            &account.offering_id,
            Some(&account.id),
        )
    }

    pub fn from_mapping(mapping: &ProviderMapping) -> Option<Self> {
        Self::from_offering(mapping.provider_id, mapping.offering_id, None)
    }

    pub fn from_offering(
        provider_id: &str,
        offering_id: &str,
        account_id: Option<&str>,
    ) -> Option<Self> {
        let descriptor = ProviderRegistry::get(provider_id, offering_id)?;
        match descriptor.kind {
            ProviderAdapterKind::ConfigurableHttp => account_id
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(Self::custom_endpoint),
            _ => descriptor.contract_scope_id.map(Self::provider),
        }
    }
}

pub fn builtin_provider_scope_ids() -> Vec<&'static str> {
    ProviderRegistry::iter()
        .filter_map(|descriptor| descriptor.contract_scope_id)
        .collect()
}

/// Resolve one exact, statically declared Provider contract scope. The opaque
/// scope id is deliberately distinct from Provider identity so a future second
/// Offering can declare its own scope without changing persistence or V3 wire
/// shapes.
pub fn provider_scope_descriptor(scope_id: &str) -> Option<crate::provider::ProviderDescriptor> {
    ProviderRegistry::iter().find(|descriptor| descriptor.contract_scope_id == Some(scope_id))
}

pub fn parse_upstream_protocol(value: &str) -> Result<UpstreamProtocolKind, String> {
    UpstreamProtocolKind::try_from(value).map_err(|_| {
        format!(
            "unknown upstream protocol `{value}`; expected chat_completions, responses, or messages"
        )
    })
}

pub fn protocol_from_api(format: ApiFormat) -> Option<UpstreamProtocolKind> {
    match format {
        ApiFormat::ChatCompletions => Some(UpstreamProtocolKind::ChatCompletions),
        ApiFormat::Responses => Some(UpstreamProtocolKind::Responses),
        ApiFormat::Messages => Some(UpstreamProtocolKind::Messages),
        ApiFormat::Gemini => None,
    }
}

pub fn protocol_to_api(protocol: UpstreamProtocolKind) -> ApiFormat {
    match protocol {
        UpstreamProtocolKind::ChatCompletions => ApiFormat::ChatCompletions,
        UpstreamProtocolKind::Responses => ApiFormat::Responses,
        UpstreamProtocolKind::Messages => ApiFormat::Messages,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractEvidenceSource {
    Static,
    Preset,
    ProbeConfirmed,
    ProbeObserved,
}

impl ContractEvidenceSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Preset => "preset",
            Self::ProbeConfirmed => "probe_confirmed",
            Self::ProbeObserved => "probe_observed",
        }
    }

    pub const fn confers_support(self) -> bool {
        matches!(self, Self::Static | Self::Preset | Self::ProbeConfirmed)
    }
}

impl TryFrom<&str> for ContractEvidenceSource {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "static" => Ok(Self::Static),
            "preset" => Ok(Self::Preset),
            "probe_confirmed" => Ok(Self::ProbeConfirmed),
            "probe_observed" => Ok(Self::ProbeObserved),
            other => Err(format!("unknown contract evidence source `{other}`")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeResultKind {
    Success,
    Failure,
}

impl ProbeResultKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }
}

impl TryFrom<&str> for ProbeResultKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "success" => Ok(Self::Success),
            "failure" => Ok(Self::Failure),
            other => Err(format!("unknown probe result `{other}`")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolOverrideState {
    /// Follow persisted evidence and adapter safety ceiling.
    #[default]
    Auto,
    /// Request protocol enablement. Adapter-specific safety ceilings may still
    /// reject or suppress protocols the adapter cannot legally route.
    ForceOn,
    /// Disable the protocol regardless of evidence.
    ForceOff,
}

impl ProtocolOverrideState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::ForceOn => "force_on",
            Self::ForceOff => "force_off",
        }
    }
}

impl TryFrom<&str> for ProtocolOverrideState {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "auto" => Ok(Self::Auto),
            "force_on" => Ok(Self::ForceOn),
            "force_off" => Ok(Self::ForceOff),
            other => Err(format!("unknown protocol override state `{other}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedScopeRow {
    pub scope: ContractScope,
    pub catalog_models: Vec<String>,
    pub catalog_refreshed_at: Option<DateTime<Utc>>,
    pub catalog_source: String,
    pub catalog_source_url: String,
    pub revision: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedModelProtocol {
    pub scope: ContractScope,
    pub model_id: String,
    pub protocol: UpstreamProtocolKind,
    pub source: ContractEvidenceSource,
    pub verified_at: Option<DateTime<Utc>>,
    pub observed_at: Option<DateTime<Utc>>,
    pub last_probe_result: Option<ProbeResultKind>,
    pub last_probe_at: Option<DateTime<Utc>>,
    pub last_probe_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedModelProtocolOverride {
    pub scope: ContractScope,
    pub model_id: String,
    pub protocol: UpstreamProtocolKind,
    pub state: ProtocolOverrideState,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersistedContracts {
    pub scopes: HashMap<ContractScope, PersistedScopeRow>,
    pub evidence: HashMap<ContractScope, Vec<PersistedModelProtocol>>,
    pub overrides: HashMap<ContractScope, Vec<PersistedModelProtocolOverride>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveCatalog {
    pub source: String,
    pub source_url: String,
    pub refreshed_at: Option<DateTime<Utc>>,
    pub models: Vec<String>,
    pub refresh_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveProtocolEvidence {
    pub protocol: UpstreamProtocolKind,
    pub available: bool,
    pub enabled: bool,
    pub source: ContractEvidenceSource,
    pub verified_at: Option<DateTime<Utc>>,
    pub observed_at: Option<DateTime<Utc>>,
    pub last_probe_result: Option<ProbeResultKind>,
    pub last_probe_at: Option<DateTime<Utc>>,
    pub last_probe_error: Option<String>,
    #[serde(rename = "override")]
    pub r#override: ProtocolOverrideState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveModelContract {
    pub model_id: String,
    pub preferred_protocol: UpstreamProtocolKind,
    pub protocols: BTreeMap<String, EffectiveProtocolEvidence>,
    pub routable: bool,
    pub disabled_reasons: Vec<String>,
}

impl EffectiveModelContract {
    pub fn enabled_protocols(&self) -> Vec<UpstreamProtocolKind> {
        self.protocols
            .values()
            .filter(|row| row.enabled)
            .map(|row| row.protocol)
            .collect()
    }

    pub fn has_enabled_protocol(&self) -> bool {
        self.protocols.values().any(|row| row.enabled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveScopeContract {
    pub scope: ContractScope,
    pub provider_id: String,
    pub offering_id: String,
    pub adapter_kind: ProviderAdapterKind,
    pub catalog_routable: bool,
    pub production_inference: bool,
    pub catalog: EffectiveCatalog,
    pub models: BTreeMap<String, EffectiveModelContract>,
    pub revision: u64,
    pub fallback_priority: &'static [UpstreamProtocolKind],
    pub disabled_reasons: Vec<String>,
}

impl EffectiveScopeContract {
    pub fn model(&self, model_id: &str) -> Option<&EffectiveModelContract> {
        let normalized = normalize_model_name(model_id);
        self.models
            .get(model_id)
            .or_else(|| self.models.get(&normalized))
            .or_else(|| {
                self.models
                    .values()
                    .find(|model| custom_or_case_match(&model.model_id, model_id))
            })
    }

    pub fn model_has_enabled_protocol(&self, model_id: &str) -> bool {
        self.model(model_id)
            .is_some_and(EffectiveModelContract::has_enabled_protocol)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectiveContractSet {
    pub providers: BTreeMap<String, EffectiveScopeContract>,
    pub custom_endpoints: BTreeMap<String, EffectiveScopeContract>,
}

impl EffectiveContractSet {
    pub fn scope(&self, scope: &ContractScope) -> Option<&EffectiveScopeContract> {
        match scope {
            ContractScope::Provider(id) => self.providers.get(id),
            ContractScope::CustomEndpoint(id) => self.custom_endpoints.get(id),
        }
    }

    pub fn provider_offering(
        &self,
        provider_id: &str,
        offering_id: &str,
    ) -> Option<&EffectiveScopeContract> {
        let scope = ContractScope::from_offering(provider_id, offering_id, None)?;
        self.scope(&scope)
    }

    pub fn mapping_has_enabled_protocol(&self, mapping: &ProviderMapping) -> bool {
        let Some(scope) = ContractScope::from_mapping(mapping) else {
            return false;
        };
        self.scope(&scope)
            .is_some_and(|contract| contract.model_has_enabled_protocol(&mapping.upstream_model))
    }

    pub fn production_protocol_allowed(
        &self,
        account: &Account,
        model_id: &str,
        protocol: UpstreamProtocolKind,
    ) -> bool {
        let Some(scope) = ContractScope::from_account(account) else {
            return false;
        };
        self.scope(&scope)
            .and_then(|contract| contract.model(model_id))
            .and_then(|model| model.protocols.get(protocol.as_str()))
            .is_some_and(|row| row.available && row.enabled)
    }

    pub fn select_for_mapping(
        &self,
        mapping: &ProviderMapping,
        client: ApiFormat,
        model_id: &str,
    ) -> Result<ApiFormat, ProtocolSelectError> {
        let scope = ContractScope::from_mapping(mapping).ok_or_else(|| {
            ProtocolSelectError::new(format!(
                "no contract scope for `{}/{}`",
                mapping.provider_id, mapping.offering_id
            ))
        })?;
        self.select_upstream(&scope, client, model_id)
    }

    pub fn select_upstream(
        &self,
        scope: &ContractScope,
        client: ApiFormat,
        model_id: &str,
    ) -> Result<ApiFormat, ProtocolSelectError> {
        let contract = self.scope(scope).ok_or_else(|| {
            ProtocolSelectError::new(format!(
                "no effective contract for {} `{}`",
                scope.kind_str(),
                scope.id()
            ))
        })?;
        select_upstream_protocol(contract, client, model_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolSelectError {
    pub message: String,
}

impl ProtocolSelectError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ProtocolSelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProtocolSelectError {}

pub fn select_upstream_protocol(
    contract: &EffectiveScopeContract,
    client: ApiFormat,
    model_id: &str,
) -> Result<ApiFormat, ProtocolSelectError> {
    let model = contract.model(model_id).ok_or_else(|| {
        ProtocolSelectError::new(format!(
            "model `{model_id}` is not in the effective contract"
        ))
    })?;
    let available = model.enabled_protocols();
    if available.is_empty() {
        return Err(ProtocolSelectError::new(NO_ENABLED_UPSTREAM_PROTOCOL));
    }
    let preferred = model.preferred_protocol;
    if available.contains(&preferred) {
        return Ok(match protocol_from_api(client) {
            Some(client_protocol) if available.contains(&client_protocol) => {
                protocol_to_api(client_protocol)
            }
            _ => protocol_to_api(preferred),
        });
    }
    if let Some(client_protocol) = protocol_from_api(client)
        && available.contains(&client_protocol)
    {
        return Ok(protocol_to_api(client_protocol));
    }
    for protocol in contract.fallback_priority {
        if available.contains(protocol) {
            return Ok(protocol_to_api(*protocol));
        }
    }
    Err(ProtocolSelectError::new(NO_ENABLED_UPSTREAM_PROTOCOL))
}

pub fn safety_ceiling_protocols(
    probe: ProtocolProbeDescriptor,
    model_id: &str,
    declared: &[(String, UpstreamProtocolKind)],
) -> Vec<UpstreamProtocolKind> {
    match probe.structural_ceiling {
        StructuralProbeCeiling::Unavailable => Vec::new(),
        StructuralProbeCeiling::CommandCodeConstructable => {
            ocg_domain::protocol::command_code_supported_formats(model_id)
                .iter()
                .copied()
                .filter_map(protocol_from_api)
                .collect()
        }
        StructuralProbeCeiling::FixedChatCompletions => {
            vec![UpstreamProtocolKind::ChatCompletions]
        }
        StructuralProbeCeiling::OpenCodeConstructable => {
            if model_id.trim().is_empty() {
                Vec::new()
            } else {
                OPENCODE_CONSTRUCTABLE_PROTOCOLS.to_vec()
            }
        }
        StructuralProbeCeiling::ZenFreeConstructable => {
            if is_known_model(model_id) {
                OPENCODE_CONSTRUCTABLE_PROTOCOLS.to_vec()
            } else if crate::kernel::ids::is_free_model(model_id) {
                vec![UpstreamProtocolKind::ChatCompletions]
            } else {
                Vec::new()
            }
        }
        StructuralProbeCeiling::AccountDeclared => declared
            .iter()
            .filter(|(id, _)| custom_model_id_matches(id, model_id))
            .map(|(_, protocol)| *protocol)
            .collect(),
    }
}

pub fn static_verified_protocols(
    adapter: ProviderAdapterKind,
    model_id: &str,
    declared: &[(String, UpstreamProtocolKind)],
) -> Vec<UpstreamProtocolKind> {
    if adapter == ProviderAdapterKind::ConfigurableHttp {
        return declared
            .iter()
            .filter(|(id, _)| custom_model_id_matches(id, model_id))
            .map(|(_, protocol)| *protocol)
            .collect();
    }
    match adapter {
        ProviderAdapterKind::OpenCodeGo => {
            crate::kernel::protocol::snapshot_protocols(OPENCODE_PROVIDER_ID, model_id)
        }
        ProviderAdapterKind::ZenFree => {
            crate::kernel::protocol::snapshot_protocols(OPENCODE_ZEN_FREE_PROVIDER_ID, model_id)
        }
        ProviderAdapterKind::CommandCodeGoat => {
            crate::kernel::protocol::snapshot_protocols(COMMAND_CODE_PROVIDER_ID, model_id)
        }
        ProviderAdapterKind::MiniMaxCn | ProviderAdapterKind::KimiCn => {
            return vec![UpstreamProtocolKind::ChatCompletions];
        }
        ProviderAdapterKind::Cpa => {
            return vec![
                UpstreamProtocolKind::ChatCompletions,
                UpstreamProtocolKind::Responses,
                UpstreamProtocolKind::Messages,
            ];
        }
        ProviderAdapterKind::ConfigurableHttp => unreachable!("handled above"),
    }
    .into_iter()
    .filter_map(protocol_from_api)
    .collect()
}

fn opencode_profile(model_id: &str) -> Option<(ApiFormat, &'static [ApiFormat])> {
    let normalized = normalize_model_name(model_id);
    supported_model_protocol_profiles()
        .find(|(id, _, _)| *id == normalized)
        .map(|(_, preferred, supported)| (preferred, supported))
}

pub fn probe_may_add(
    probe: ProtocolProbeDescriptor,
    model_id: &str,
    protocol: UpstreamProtocolKind,
    declared: &[(String, UpstreamProtocolKind)],
) -> bool {
    probe.explicit_probe && safety_ceiling_protocols(probe, model_id, declared).contains(&protocol)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_probe_observation(
    existing: Option<&PersistedModelProtocol>,
    scope: ContractScope,
    model_id: &str,
    protocol: UpstreamProtocolKind,
    success: bool,
    error: Option<String>,
    now: DateTime<Utc>,
    inside_ceiling: bool,
) -> Result<PersistedModelProtocol, String> {
    if success && !inside_ceiling {
        return Err(
            "probe success cannot add a model/protocol combination outside the adapter safety ceiling"
                .to_string(),
        );
    }
    let sanitized = error.map(|value| sanitize_probe_error(&value, None));
    if let Some(row) = existing {
        let mut next = row.clone();
        next.observed_at = Some(now);
        next.last_probe_at = Some(now);
        next.last_probe_result = Some(if success {
            ProbeResultKind::Success
        } else {
            ProbeResultKind::Failure
        });
        next.last_probe_error = if success { None } else { sanitized };
        if success {
            if next.verified_at.is_none() {
                next.verified_at = Some(now);
            }
            if !next.source.confers_support() {
                next.source = ContractEvidenceSource::ProbeConfirmed;
            }
        }
        return Ok(next);
    }
    if success {
        return Ok(PersistedModelProtocol {
            scope,
            model_id: model_id.to_string(),
            protocol,
            source: ContractEvidenceSource::ProbeConfirmed,
            verified_at: Some(now),
            observed_at: Some(now),
            last_probe_result: Some(ProbeResultKind::Success),
            last_probe_at: Some(now),
            last_probe_error: None,
        });
    }
    Ok(PersistedModelProtocol {
        scope,
        model_id: model_id.to_string(),
        protocol,
        source: ContractEvidenceSource::ProbeObserved,
        verified_at: None,
        observed_at: Some(now),
        last_probe_result: Some(ProbeResultKind::Failure),
        last_probe_at: Some(now),
        last_probe_error: sanitized,
    })
}

pub fn sanitize_probe_error(raw: &str, secret: Option<&str>) -> String {
    let value = secret.map_or_else(
        || sanitize_upstream_error_value_with_known_secret(raw, "").to_string(),
        |secret| sanitize_upstream_error_value_with_known_secret(raw, secret).to_string(),
    );
    truncate_chars(&strip_credential_urls(&value), MAX_PROBE_ERROR_CHARS)
}

fn strip_credential_urls(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(scheme_at) = rest.find("://") {
        let prefix_start = rest[..scheme_at]
            .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '+' || ch == '.' || ch == '-'))
            .map(|index| index + 1)
            .unwrap_or(0);
        output.push_str(&rest[..prefix_start]);
        let after_scheme = &rest[scheme_at + 3..];
        if let Some(at) = after_scheme.find('@') {
            let host_end = after_scheme[at + 1..]
                .find(|ch: char| ch == '/' || ch == '?' || ch == '#' || ch.is_whitespace())
                .map(|index| at + 1 + index)
                .unwrap_or(after_scheme.len());
            output.push_str(&rest[prefix_start..scheme_at + 3]);
            output.push_str(&after_scheme[at + 1..host_end]);
            rest = &after_scheme[host_end..];
        } else {
            output.push_str(&rest[prefix_start..scheme_at + 3]);
            rest = after_scheme;
        }
    }
    output.push_str(rest);
    output
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let count = input.chars().count();
    if count <= max_chars {
        return input.to_string();
    }
    let mut truncated: String = input.chars().take(max_chars).collect();
    truncated.push('…');
    truncated
}

pub fn build_effective_contracts(
    zen_catalog: &ZenFreeModelCatalog,
    custom_runtimes: &[CustomAccountRuntime],
    persisted: PersistedContracts,
) -> EffectiveContractSet {
    let mut set = EffectiveContractSet::default();
    for scope_id in builtin_provider_scope_ids() {
        let scope = ContractScope::provider(scope_id);
        let descriptor = provider_scope_descriptor(scope_id)
            .expect("builtin provider scopes map to exact descriptors");
        let persisted_scope = persisted.scopes.get(&scope);
        let evidence = persisted.evidence.get(&scope).cloned().unwrap_or_default();
        let overrides = persisted.overrides.get(&scope).cloned().unwrap_or_default();
        let contract = merge_provider_scope(
            descriptor,
            zen_catalog,
            persisted_scope,
            &evidence,
            &overrides,
        );
        set.providers.insert(scope_id.to_string(), contract);
    }
    for runtime in custom_runtimes {
        let scope = ContractScope::custom_endpoint(&runtime.account_id);
        let persisted_scope = persisted.scopes.get(&scope);
        let evidence = persisted.evidence.get(&scope).cloned().unwrap_or_default();
        let overrides = persisted.overrides.get(&scope).cloned().unwrap_or_default();
        let contract = merge_custom_scope(runtime, persisted_scope, &evidence, &overrides);
        set.custom_endpoints
            .insert(runtime.account_id.clone(), contract);
    }
    set
}

fn merge_provider_scope(
    descriptor: crate::provider::ProviderDescriptor,
    zen_catalog: &ZenFreeModelCatalog,
    persisted: Option<&PersistedScopeRow>,
    evidence: &[PersistedModelProtocol],
    overrides: &[PersistedModelProtocolOverride],
) -> EffectiveScopeContract {
    let adapter = descriptor.kind;
    let scope_id = descriptor
        .contract_scope_id
        .expect("provider contract descriptor must declare a scope id");
    let revision = persisted.map(|row| row.revision).unwrap_or(1);
    let (catalog, static_models) = match adapter {
        ProviderAdapterKind::OpenCodeGo => {
            let models: Vec<String> = persisted
                .filter(|row| !row.catalog_models.is_empty())
                .map(|row| row.catalog_models.clone())
                .unwrap_or_else(|| {
                    supported_model_protocol_profiles()
                        .map(|(id, _, _)| id.to_string())
                        .collect()
                });
            (
                EffectiveCatalog {
                    source: persisted
                        .map(|row| row.catalog_source.clone())
                        .filter(|source| !source.is_empty())
                        .unwrap_or_else(|| CATALOG_SOURCE_STATIC.to_string()),
                    source_url: persisted
                        .map(|row| row.catalog_source_url.clone())
                        .unwrap_or_default(),
                    refreshed_at: persisted.and_then(|row| row.catalog_refreshed_at),
                    models: models.clone(),
                    refresh_supported: true,
                },
                models,
            )
        }
        ProviderAdapterKind::ZenFree => {
            let has_persisted_catalog = persisted.is_some_and(|row| !row.catalog_models.is_empty());
            let models = persisted
                .filter(|row| !row.catalog_models.is_empty())
                .map(|row| row.catalog_models.clone())
                .unwrap_or_else(|| zen_catalog.models.clone());
            (
                EffectiveCatalog {
                    source: if has_persisted_catalog {
                        persisted
                            .map(|row| row.catalog_source.clone())
                            .filter(|source| !source.is_empty())
                            .unwrap_or_else(|| CATALOG_SOURCE_OFFICIAL_ZEN.to_string())
                    } else {
                        CATALOG_SOURCE_STATIC.to_string()
                    },
                    source_url: if has_persisted_catalog {
                        persisted
                            .map(|row| row.catalog_source_url.clone())
                            .filter(|url| !url.is_empty())
                            .unwrap_or_else(|| zen_catalog.source_url.clone())
                    } else {
                        String::new()
                    },
                    refreshed_at: if has_persisted_catalog {
                        persisted
                            .and_then(|row| row.catalog_refreshed_at)
                            .or(zen_catalog.refreshed_at)
                    } else {
                        None
                    },
                    models: models.clone(),
                    refresh_supported: true,
                },
                models,
            )
        }
        ProviderAdapterKind::CommandCodeGoat => {
            let models: Vec<String> = persisted
                .filter(|row| !row.catalog_models.is_empty())
                .map(|row| row.catalog_models.clone())
                .unwrap_or_else(|| {
                    COMMAND_CODE_GOAT_INCLUDED_MODEL_IDS
                        .iter()
                        .map(|model| (*model).to_string())
                        .collect()
                });
            (
                EffectiveCatalog {
                    source: persisted
                        .map(|row| row.catalog_source.clone())
                        .filter(|source| !source.is_empty())
                        .unwrap_or_else(|| CATALOG_SOURCE_COMMAND_CODE_MODELS.to_string()),
                    source_url: persisted
                        .map(|row| row.catalog_source_url.clone())
                        .filter(|url| !url.is_empty())
                        .unwrap_or_else(|| COMMAND_CODE_GOAT_BASE_URL.to_string()),
                    refreshed_at: persisted.and_then(|row| row.catalog_refreshed_at),
                    models: models.clone(),
                    refresh_supported: true,
                },
                models,
            )
        }
        ProviderAdapterKind::MiniMaxCn | ProviderAdapterKind::KimiCn => {
            let (fallback, source, source_url): (&[&str], _, _) =
                if adapter == ProviderAdapterKind::MiniMaxCn {
                    (
                        &["MiniMax-M3"],
                        CATALOG_SOURCE_MINIMAX_CN_MODELS,
                        crate::provider::MINIMAX_CN_BASE_URL,
                    )
                } else {
                    (
                        &["kimi-for-coding", "kimi-k3"],
                        CATALOG_SOURCE_KIMI_CN_MODELS,
                        crate::provider::KIMI_CN_BASE_URL,
                    )
                };
            let models = persisted
                .filter(|row| !row.catalog_models.is_empty())
                .map(|row| row.catalog_models.clone())
                .unwrap_or_else(|| fallback.iter().map(|model| (*model).to_string()).collect());
            (
                EffectiveCatalog {
                    source: persisted
                        .map(|row| row.catalog_source.clone())
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| source.to_string()),
                    source_url: persisted
                        .map(|row| row.catalog_source_url.clone())
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| source_url.to_string()),
                    refreshed_at: persisted.and_then(|row| row.catalog_refreshed_at),
                    models: models.clone(),
                    refresh_supported: true,
                },
                models,
            )
        }
        ProviderAdapterKind::Cpa => {
            unreachable!("CPA is an external integration without a Provider contract scope")
        }
        ProviderAdapterKind::ConfigurableHttp => unreachable!("custom uses merge_custom_scope"),
    };

    let mut models = BTreeMap::new();
    for model_id in &static_models {
        let default_source = if adapter == ProviderAdapterKind::CommandCodeGoat
            && command_code_goat_includes_model(model_id)
        {
            ContractEvidenceSource::Preset
        } else {
            ContractEvidenceSource::Static
        };
        models.insert(
            model_id.clone(),
            merge_model_contract(
                adapter,
                descriptor.protocol_probe,
                model_id,
                &[],
                default_source,
                evidence,
                overrides,
                descriptor.inference.catalog_routable && descriptor.inference.production_inference,
            ),
        );
    }
    let mut disabled_reasons = Vec::new();
    if !descriptor.inference.catalog_routable {
        disabled_reasons.push("catalog offering is not routable".to_string());
    }
    if !descriptor.inference.production_inference {
        disabled_reasons.push("production inference is disabled".to_string());
    }

    EffectiveScopeContract {
        scope: ContractScope::provider(scope_id),
        provider_id: descriptor.provider_id.to_string(),
        offering_id: descriptor.offering_id.to_string(),
        adapter_kind: adapter,
        catalog_routable: descriptor.inference.catalog_routable,
        production_inference: descriptor.inference.production_inference,
        catalog,
        models,
        revision,
        fallback_priority: descriptor.protocol_probe.fallback_priority,
        disabled_reasons,
    }
}

fn merge_custom_scope(
    runtime: &CustomAccountRuntime,
    persisted: Option<&PersistedScopeRow>,
    evidence: &[PersistedModelProtocol],
    overrides: &[PersistedModelProtocolOverride],
) -> EffectiveScopeContract {
    let descriptor = ProviderRegistry::get(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID)
        .expect("custom offering is registered");
    let revision = persisted.map(|row| row.revision).unwrap_or(1);
    let declared: Vec<(String, UpstreamProtocolKind)> = runtime
        .capabilities
        .iter()
        .map(|capability| (capability.public_model.clone(), capability.protocol))
        .collect();
    let mut catalog_models = Vec::new();
    let mut catalog_seen = HashSet::new();
    for (model_id, _) in &declared {
        if catalog_seen.insert(model_id.to_ascii_lowercase()) {
            catalog_models.push(model_id.clone());
        }
    }
    let catalog = EffectiveCatalog {
        source: CATALOG_SOURCE_DECLARED.to_string(),
        source_url: String::new(),
        refreshed_at: None,
        models: catalog_models,
        refresh_supported: false,
    };

    let mut models = BTreeMap::new();
    let mut seen = HashSet::new();
    for (model_id, _) in &declared {
        if !seen.insert(model_id.to_ascii_lowercase()) {
            continue;
        }
        models.insert(
            model_id.clone(),
            merge_model_contract(
                ProviderAdapterKind::ConfigurableHttp,
                descriptor.protocol_probe,
                model_id,
                &declared,
                ContractEvidenceSource::Preset,
                evidence,
                overrides,
                descriptor.inference.catalog_routable && descriptor.inference.production_inference,
            ),
        );
    }
    overlay_probe_confirmed_models(
        &mut models,
        ProviderAdapterKind::ConfigurableHttp,
        descriptor.protocol_probe,
        &declared,
        evidence,
        overrides,
        descriptor.inference.catalog_routable && descriptor.inference.production_inference,
    );

    EffectiveScopeContract {
        scope: ContractScope::custom_endpoint(&runtime.account_id),
        provider_id: CUSTOM_PROVIDER_ID.to_string(),
        offering_id: CUSTOM_API_OFFERING_ID.to_string(),
        adapter_kind: ProviderAdapterKind::ConfigurableHttp,
        catalog_routable: descriptor.inference.catalog_routable,
        production_inference: descriptor.inference.production_inference,
        catalog,
        models,
        revision,
        fallback_priority: descriptor.protocol_probe.fallback_priority,
        disabled_reasons: Vec::new(),
    }
}

fn preferred_protocol(
    adapter: ProviderAdapterKind,
    model_id: &str,
    declared: &[(String, UpstreamProtocolKind)],
) -> UpstreamProtocolKind {
    match adapter {
        ProviderAdapterKind::OpenCodeGo | ProviderAdapterKind::ZenFree => {
            opencode_profile(model_id)
                .and_then(|(preferred, _)| protocol_from_api(preferred))
                .unwrap_or(UpstreamProtocolKind::ChatCompletions)
        }
        ProviderAdapterKind::CommandCodeGoat => {
            ocg_domain::protocol::command_code_preferred_format(model_id)
                .and_then(protocol_from_api)
                .unwrap_or(UpstreamProtocolKind::ChatCompletions)
        }
        ProviderAdapterKind::MiniMaxCn | ProviderAdapterKind::KimiCn => {
            UpstreamProtocolKind::ChatCompletions
        }
        ProviderAdapterKind::Cpa => UpstreamProtocolKind::ChatCompletions,
        ProviderAdapterKind::ConfigurableHttp => {
            // A Custom endpoint binds every declared model to exactly one
            // upstream protocol; that protocol is also the conversion target.
            declared
                .iter()
                .filter(|(id, _)| custom_model_id_matches(id, model_id))
                .map(|(_, protocol)| *protocol)
                .next()
                .unwrap_or(UpstreamProtocolKind::ChatCompletions)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn merge_model_contract(
    adapter: ProviderAdapterKind,
    probe: ProtocolProbeDescriptor,
    model_id: &str,
    declared: &[(String, UpstreamProtocolKind)],
    default_source: ContractEvidenceSource,
    evidence: &[PersistedModelProtocol],
    overrides: &[PersistedModelProtocolOverride],
    adapter_routable: bool,
) -> EffectiveModelContract {
    let default_enabled = adapter != ProviderAdapterKind::CommandCodeGoat
        || command_code_goat_includes_model(model_id);
    let preferred = preferred_protocol(adapter, model_id, declared);
    let ceiling = if adapter == ProviderAdapterKind::CommandCodeGoat
        && !model_id.eq_ignore_ascii_case("stealth/ox-alpha")
    {
        ocg_domain::protocol::command_code_supported_formats(model_id)
            .iter()
            .copied()
            .filter_map(protocol_from_api)
            .collect()
    } else {
        safety_ceiling_protocols(probe, model_id, declared)
    };
    let static_verified = static_verified_protocols(adapter, model_id, declared);
    let mut protocols = BTreeMap::new();
    for protocol in UpstreamProtocolKind::ALL {
        let persisted = evidence
            .iter()
            .find(|row| custom_or_case_match(&row.model_id, model_id) && row.protocol == protocol);
        let in_ceiling = ceiling.contains(&protocol);
        let statically_verified = static_verified.contains(&protocol);
        let override_state = overrides
            .iter()
            .find(|row| custom_or_case_match(&row.model_id, model_id) && row.protocol == protocol)
            .map(|row| row.state)
            .unwrap_or(ProtocolOverrideState::Auto);
        let has_override = override_state != ProtocolOverrideState::Auto;
        if persisted.is_none() && !in_ceiling && !statically_verified && !has_override {
            continue;
        }
        let source = persisted
            .map(|row| row.source)
            .unwrap_or(if statically_verified {
                default_source
            } else {
                ContractEvidenceSource::ProbeObserved
            });
        let supported = statically_verified || in_ceiling;
        // Static/preset support is declaration truth: a stale probe-failure
        // observation must not demote it. Probe outcomes move enablement only
        // through the explicit overrides the probe handler persists.
        let evidence_available = (statically_verified || source.confers_support()) && supported;
        let (available, enabled) = match override_state {
            // Sealed and configurable HTTP adapters may force on only a
            // protocol already admitted by their adapter safety ceiling.
            ProtocolOverrideState::ForceOn
                if matches!(
                    adapter,
                    ProviderAdapterKind::CommandCodeGoat | ProviderAdapterKind::ConfigurableHttp
                ) =>
            {
                (supported, supported)
            }
            ProtocolOverrideState::ForceOn => (true, true),
            ProtocolOverrideState::ForceOff => (evidence_available, false),
            ProtocolOverrideState::Auto => (
                evidence_available,
                evidence_available && (default_enabled || source == ContractEvidenceSource::Preset),
            ),
        };
        protocols.insert(
            protocol.as_str().to_string(),
            EffectiveProtocolEvidence {
                protocol,
                available,
                enabled,
                source: if statically_verified && persisted.is_none() {
                    default_source
                } else {
                    persisted.map(|row| row.source).unwrap_or(source)
                },
                verified_at: persisted.and_then(|row| row.verified_at),
                observed_at: persisted.and_then(|row| row.observed_at),
                last_probe_result: persisted.and_then(|row| row.last_probe_result),
                last_probe_at: persisted.and_then(|row| row.last_probe_at),
                last_probe_error: persisted.and_then(|row| row.last_probe_error.clone()),
                r#override: override_state,
            },
        );
    }
    if !protocols.contains_key(preferred.as_str()) && static_verified.contains(&preferred) {
        protocols.insert(
            preferred.as_str().to_string(),
            EffectiveProtocolEvidence {
                protocol: preferred,
                available: true,
                enabled: default_enabled,
                source: default_source,
                verified_at: None,
                observed_at: None,
                last_probe_result: None,
                last_probe_at: None,
                last_probe_error: None,
                r#override: ProtocolOverrideState::Auto,
            },
        );
    }
    let mut disabled_reasons = Vec::new();
    if !adapter_routable {
        disabled_reasons.push("adapter safety ceiling forbids production routing".to_string());
    }
    if !protocols.values().any(|row| row.enabled) {
        disabled_reasons.push(NO_ENABLED_UPSTREAM_PROTOCOL.to_string());
    }
    EffectiveModelContract {
        model_id: model_id.to_string(),
        preferred_protocol: preferred,
        routable: adapter_routable && protocols.values().any(|row| row.enabled),
        protocols,
        disabled_reasons,
    }
}

fn overlay_probe_confirmed_models(
    models: &mut BTreeMap<String, EffectiveModelContract>,
    adapter: ProviderAdapterKind,
    probe: ProtocolProbeDescriptor,
    declared: &[(String, UpstreamProtocolKind)],
    evidence: &[PersistedModelProtocol],
    overrides: &[PersistedModelProtocolOverride],
    adapter_routable: bool,
) {
    let mut extra: HashSet<String> = HashSet::new();
    for row in evidence {
        if !row.source.confers_support() {
            continue;
        }
        if models
            .keys()
            .any(|id| custom_or_case_match(id, &row.model_id))
        {
            continue;
        }
        if !probe_may_add(probe, &row.model_id, row.protocol, declared) {
            continue;
        }
        extra.insert(row.model_id.clone());
    }
    for model_id in extra {
        models.insert(
            model_id.clone(),
            merge_model_contract(
                adapter,
                probe,
                &model_id,
                declared,
                ContractEvidenceSource::ProbeConfirmed,
                evidence,
                overrides,
                adapter_routable,
            ),
        );
    }
}

fn custom_or_case_match(left: &str, right: &str) -> bool {
    custom_model_id_matches(left, right) || left.eq_ignore_ascii_case(right)
}

#[cfg(test)]
mod tests;
