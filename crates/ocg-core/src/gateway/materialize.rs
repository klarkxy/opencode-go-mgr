//! Candidate request materialization for the alias runtime.
//!
//! # Adapter interface
//!
//! Later provider adapters should treat this module as the boundary between
//! a parsed client request and an upstream call:
//!
//! 1. Parse the client protocol **once** with
//!    [`parse_client_request`] / [`parse_gemini_request`].
//! 2. Resolve the requested name through [`crate::alias::resolve`]. Alias
//!    results may follow account order, sticky, and fallback. A unique raw
//!    upstream ID is pinned to its single mapping; overlapping raw IDs return
//!    [`crate::alias::AMBIGUOUS_MODEL_ID`].
//! 3. Build candidate plans from [`ResolvedModel`] mappings. Match accounts in saved
//!    account order through [`super::provider_adapter::supports_production_plan`], using
//!    mapping order only as the per-account tie-break. Protocol selection
//!    uses the OpenCode `MODEL_PROTOCOLS` table for Go/Zen upstream models
//!    and Command Code family rules (Chat vs Messages) for GOAT IDs.
//!    **Never** trial a billable inference path.
//!    Adapter identity is [`crate::provider::ProviderAdapterKind`]; Custom is
//!    Configurable HTTP, not a base class.
//! 4. Ask [`super::provider_adapter::resolve_route`] for endpoint + auth.
//!    Production GOAT uses the official Provider API after a saved verified
//!    catalog snapshot. The official slash raw ID pins to command-code/goat
//!    without stealing Go kebab aliases.
//!
//! OpenCode Go and Zen Free are implemented here. Claude Desktop
//! `sonnet` / `opus` / `haiku` aliases are rewritten to a configured Go
//! model before resolution; the original Claude name is kept as
//! `RequestPlan.client_model`.

use crate::alias::{ProviderMapping, ResolveError, ResolvedModel};
use crate::custom::CustomAccountRuntime;
use crate::gateway::free_models::resolve_upstream_base;
use crate::gateway::protocol::{
    CustomRouteSpec, MaterializeSpec, ParsedClientRequest, ProtocolError, RequestPlan,
    materialize_parsed_request,
};
use crate::gateway::provider_adapter;
use crate::gateway::routing::RoutingCandidate;
use crate::goat::GoatAccountRuntime;
use crate::kernel::ids::normalize_model_name;
use crate::kernel::protocol::ApiFormat;
use crate::models::{Account, AppConfig, UpstreamChannel};
use crate::provider::ProviderAdapterKind;
use crate::provider_contracts::{ContractScope, EffectiveContractSet};
use axum::http::StatusCode;
use bytes::Bytes;

pub use crate::gateway::protocol::{
    parse_client_request as parse_client, parse_gemini_request as parse_gemini,
};

#[derive(Debug, Clone)]
pub(crate) struct MaterializedCandidate {
    pub routing: RoutingCandidate,
    pub plan: RequestPlan,
}

#[derive(Debug, Clone)]
pub(crate) struct MaterializedRouteSet {
    pub routes: Vec<MaterializedCandidate>,
    pub free_only: bool,
    pub incompatibility: Option<String>,
}

/// Diagnostics are not a candidate protocol decision. If a resolution can use
/// Custom or Command Code GOAT, preserve the client wire format until each
/// actual mapping/account is materialized. Unique GOAT catalog IDs are not in
/// OpenCode `MODEL_PROTOCOLS`. Pure builtin resolutions keep their normal
/// early validation.
pub(crate) fn diagnostic_forced_upstream(
    resolved: &ResolvedModel,
    client: ApiFormat,
) -> Option<ApiFormat> {
    if let ResolvedModel::PinnedRaw { mapping, .. } = resolved
        && mapping_adapter_kind(mapping) == Some(ProviderAdapterKind::Cpa)
    {
        return Some(
            crate::kernel::protocol::model_protocol(&mapping.upstream_model)
                .map(|profile| profile.preferred)
                .unwrap_or(ApiFormat::ChatCompletions),
        );
    }
    let preserve_client = match resolved {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            mapping_is_configurable_http(mapping) || mapping_is_command_code_goat(mapping)
        }
        ResolvedModel::Alias { mappings, .. } => mappings.iter().any(|mapping| {
            mapping.routeable
                && (mapping_is_configurable_http(mapping) || mapping_is_command_code_goat(mapping))
        }),
    };
    preserve_client.then_some(client)
}

fn mapping_adapter_kind(mapping: &ProviderMapping) -> Option<ProviderAdapterKind> {
    ProviderAdapterKind::from_offering(mapping.provider_id, mapping.offering_id)
}

fn mapping_is_configurable_http(mapping: &ProviderMapping) -> bool {
    mapping_adapter_kind(mapping) == Some(ProviderAdapterKind::ConfigurableHttp)
}

fn mapping_is_zen_free(mapping: &ProviderMapping) -> bool {
    mapping_adapter_kind(mapping) == Some(ProviderAdapterKind::ZenFree)
}

pub(crate) fn protocol_error_from_resolve(error: ResolveError) -> ProtocolError {
    match error.code() {
        Some(code) => ProtocolError::with_code(StatusCode::BAD_REQUEST, code, error.message()),
        None => ProtocolError::new(error.message()),
    }
}

/// Canonical registry alias persisted on forward logs for this resolution.
pub(crate) fn resolved_alias_from_model(resolved: &ResolvedModel) -> Option<String> {
    match resolved {
        ResolvedModel::Alias { alias, .. } => Some((*alias).to_string()),
        ResolvedModel::PinnedRaw { mapping, .. } => registry_alias_for_mapping(mapping),
    }
}

/// Registry alias for a unique raw mapping, when one is published.
pub(crate) fn registry_alias_for_mapping(mapping: &ProviderMapping) -> Option<String> {
    for published in crate::alias::published_aliases() {
        match crate::alias::resolve(&published) {
            Ok(ResolvedModel::Alias {
                alias, mappings, ..
            }) => {
                if mappings.iter().any(|candidate| {
                    candidate.provider_id == mapping.provider_id
                        && candidate.offering_id == mapping.offering_id
                        && candidate.upstream_model == mapping.upstream_model
                }) {
                    return Some(alias.to_string());
                }
            }
            Ok(ResolvedModel::PinnedRaw { .. }) | Err(_) => {}
        }
    }
    None
}

/// Request / alias / upstream identity persisted on every forward log row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeLogIdentity {
    pub requested_model: String,
    pub resolved_alias: Option<String>,
    pub upstream_model: String,
}

/// Carry materialization identity into logs without inferring at the DB layer.
pub(crate) fn native_log_identity(plan: &RequestPlan) -> NativeLogIdentity {
    let requested_model = plan.log_requested_model().to_string();
    let upstream_model = plan.log_upstream_model().to_string();
    let resolved_alias = plan
        .resolved_alias
        .clone()
        .filter(|alias| !alias.is_empty())
        .or_else(|| resolved_alias_for_name(&requested_model))
        .or_else(|| {
            plan.original_model
                .as_deref()
                .and_then(resolved_alias_for_name)
        })
        .or_else(|| resolved_alias_for_name(&upstream_model));
    NativeLogIdentity {
        requested_model,
        resolved_alias,
        upstream_model,
    }
}

pub(crate) fn resolved_alias_for_name(name: &str) -> Option<String> {
    match crate::alias::resolve(name) {
        Ok(resolved) => resolved_alias_from_model(&resolved),
        Err(_) => None,
    }
}

/// Preserve original casing when the client name already identifies this mapping.
pub(crate) fn upstream_model_for(requested: &str, canonical: &str) -> String {
    if normalize_model_name(requested) == normalize_model_name(canonical) {
        requested.to_string()
    } else {
        canonical.to_string()
    }
}

struct MappingPlan {
    mapping: ProviderMapping,
    plan: RequestPlan,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn materialize_account_routes(
    accounts: &[Account],
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    resolved: &ResolvedModel,
    client_model: &str,
    routing_model: &str,
    _client_body: &Bytes,
    free_available: bool,
    custom_runtimes: &std::collections::HashMap<String, CustomAccountRuntime>,
    goat_runtimes: &std::collections::HashMap<String, GoatAccountRuntime>,
    cpa_base_url: Option<&str>,
    contracts: &EffectiveContractSet,
) -> Result<MaterializedRouteSet, ProtocolError> {
    match resolved {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            let zen_only = mapping_is_zen_free(mapping);
            let plan = materialize_mapping_plan(
                config,
                parsed,
                client_model,
                routing_model,
                mapping,
                resolved_alias_from_model(resolved),
                None,
                false,
                cpa_base_url,
                contracts,
            )?;
            collect_mapping_plans(
                accounts,
                config,
                parsed,
                client_model,
                routing_model,
                resolved_alias_from_model(resolved),
                vec![MappingPlan {
                    mapping: mapping.clone(),
                    plan,
                }],
                zen_only,
                Vec::new(),
                custom_runtimes,
                goat_runtimes,
                contracts,
            )
        }
        ResolvedModel::Alias {
            mappings, alias, ..
        } => {
            let routeable: Vec<ProviderMapping> = mappings
                .iter()
                .filter(|mapping| mapping.routeable)
                .cloned()
                .collect();
            let zen_only = !routeable.is_empty() && routeable.iter().all(mapping_is_zen_free);
            let mut plans = Vec::new();
            let mut rejected = Vec::new();
            let mut first_materialization_error = None;
            let resolved_alias = Some(alias.to_string());
            for mapping in &routeable {
                if mapping_is_zen_free(mapping) && !free_available && !zen_only {
                    continue;
                }
                match materialize_mapping_plan(
                    config,
                    parsed,
                    client_model,
                    routing_model,
                    mapping,
                    resolved_alias.clone(),
                    None,
                    false,
                    cpa_base_url,
                    contracts,
                ) {
                    Ok(plan) => plans.push(MappingPlan {
                        mapping: mapping.clone(),
                        plan,
                    }),
                    Err(error) => {
                        rejected.push(format!(
                            "{}/{} mapping `{}`: {error}",
                            mapping.provider_id, mapping.offering_id, mapping.upstream_model
                        ));
                        first_materialization_error.get_or_insert(error);
                    }
                }
            }

            // Preserve the existing pure-builtin 400 when every actual
            // mapping rejects the request. Mixed resolutions continue so a
            // compatible Custom account can still be materialized below.
            if plans.is_empty()
                && let Some(error) = first_materialization_error
            {
                return Err(error);
            }

            collect_mapping_plans(
                accounts,
                config,
                parsed,
                client_model,
                routing_model,
                Some(alias.to_string()),
                plans,
                zen_only,
                rejected,
                custom_runtimes,
                goat_runtimes,
                contracts,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn materialize_mapping_plan(
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    client_model: &str,
    routing_model: &str,
    mapping: &ProviderMapping,
    resolved_alias: Option<String>,
    original_model: Option<String>,
    allow_go_fallback: bool,
    cpa_base_url: Option<&str>,
    contracts: &EffectiveContractSet,
) -> Result<RequestPlan, ProtocolError> {
    let adapter_kind = mapping_adapter_kind(mapping);
    let channel = if adapter_kind == Some(ProviderAdapterKind::ZenFree) {
        UpstreamChannel::Free
    } else {
        // GOAT / Configurable HTTP share the Go channel discriminator.
        // Custom is rematerialized per account with that account's configured
        // protocol. Configurable HTTP is not a base class.
        UpstreamChannel::Go
    };
    let model = if adapter_kind == Some(ProviderAdapterKind::ConfigurableHttp) {
        routing_model.to_string()
    } else if original_model.is_some() {
        mapping.upstream_model.to_string()
    } else {
        upstream_model_for(routing_model, &mapping.upstream_model)
    };
    let forced_upstream = if adapter_kind == Some(ProviderAdapterKind::ConfigurableHttp) {
        Some(parsed.client)
    } else if adapter_kind == Some(ProviderAdapterKind::Cpa) {
        Some(
            crate::kernel::protocol::model_protocol(&model)
                .map(|profile| profile.preferred)
                .unwrap_or(ApiFormat::ChatCompletions),
        )
    } else if adapter_kind == Some(ProviderAdapterKind::CommandCodeGoat) {
        Some(
            contracts
                .select_for_mapping(mapping, parsed.client, &model)
                .map_err(|error| ProtocolError::new(error.message))?,
        )
    } else {
        Some(
            contracts
                .select_for_mapping(mapping, parsed.client, &model)
                .map_err(|error| ProtocolError::new(error.message))?,
        )
    };
    let mut plan = materialize_channel_plan(
        config,
        parsed,
        client_model,
        &model,
        resolved_alias,
        channel,
        original_model,
        allow_go_fallback,
        forced_upstream,
        None,
    )?;
    if adapter_kind == Some(ProviderAdapterKind::Cpa) {
        plan.upstream_base_override = Some(
            cpa_base_url
                .ok_or_else(|| ProtocolError::new("CPA is not configured"))?
                .to_string(),
        );
    }
    Ok(plan)
}

#[allow(clippy::too_many_arguments)]
fn materialize_channel_plan(
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    client_model: &str,
    model: &str,
    resolved_alias: Option<String>,
    channel: UpstreamChannel,
    original_model: Option<String>,
    allow_go_fallback: bool,
    forced_upstream: Option<ApiFormat>,
    custom_route: Option<CustomRouteSpec>,
) -> Result<RequestPlan, ProtocolError> {
    let base =
        resolve_upstream_base(channel, &config.upstream_base_url).map_err(ProtocolError::new)?;
    materialize_parsed_request(
        parsed,
        &MaterializeSpec {
            client_model: client_model.to_string(),
            upstream_model: model.to_string(),
            resolved_alias,
            channel,
            upstream_base_override: match channel {
                UpstreamChannel::Free => Some(base),
                UpstreamChannel::Go => None,
            },
            original_model,
            allow_go_fallback,
            forced_upstream,
            custom_route,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn materialize_custom_account_plan(
    account: &Account,
    runtime: Option<&CustomAccountRuntime>,
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    client_model: &str,
    routing_model: &str,
    resolved_alias: Option<String>,
    contracts: &EffectiveContractSet,
) -> Result<RequestPlan, ProtocolError> {
    let runtime = runtime.ok_or_else(|| {
        ProtocolError::new(format!(
            "Custom account `{}` is missing a persisted API URL and upstream protocol",
            account.name
        ))
    })?;
    if !runtime.eligible() {
        return Err(ProtocolError::new(format!(
            "Custom account `{}` is not enabled, ready, and configured with a non-empty Key",
            account.name
        )));
    }
    let capability = runtime
        .capability_matching_public(routing_model)
        .ok_or_else(|| {
            ProtocolError::new(format!(
                "Custom account `{}` did not declare model `{routing_model}`",
                account.name
            ))
        })?;
    let resolved_alias = resolved_alias
        .filter(|alias| !alias.is_empty())
        .or_else(|| Some(capability.public_model.clone()));
    let contract = contracts
        .scope(&ContractScope::custom_endpoint(&account.id))
        .ok_or_else(|| {
            ProtocolError::new(format!(
                "no effective contract for custom_endpoint `{}`",
                account.id
            ))
        })?;
    // Every Custom account has one declared upstream protocol. The contract
    // passes the same client format through or converts every other format to it.
    let upstream = crate::provider_contracts::select_upstream_protocol(
        contract,
        parsed.client,
        &capability.public_model,
    )
    .map_err(|error| ProtocolError::new(error.message))?;
    materialize_channel_plan(
        config,
        parsed,
        client_model,
        &capability.upstream_model,
        resolved_alias,
        UpstreamChannel::Go,
        None,
        false,
        Some(upstream),
        Some(CustomRouteSpec {
            endpoint_url: runtime.config.endpoint_url.clone(),
        }),
    )
}

fn mapping_is_command_code_goat(mapping: &ProviderMapping) -> bool {
    mapping_adapter_kind(mapping) == Some(ProviderAdapterKind::CommandCodeGoat)
}

#[allow(clippy::too_many_arguments)]
fn collect_mapping_plans(
    accounts: &[Account],
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    client_model: &str,
    routing_model: &str,
    resolved_alias: Option<String>,
    plans: Vec<MappingPlan>,
    free_only: bool,
    mut rejected: Vec<String>,
    custom_runtimes: &std::collections::HashMap<String, CustomAccountRuntime>,
    goat_runtimes: &std::collections::HashMap<String, GoatAccountRuntime>,
    contracts: &EffectiveContractSet,
) -> Result<MaterializedRouteSet, ProtocolError> {
    let mut routes = Vec::new();
    for account in accounts {
        for candidate in &plans {
            if account.provider_id != candidate.mapping.provider_id
                || account.offering_id != candidate.mapping.offering_id
            {
                continue;
            }
            if routes.iter().any(|route: &MaterializedCandidate| {
                route.routing.account.id == account.id
                    && route.routing.channel == candidate.plan.channel
            }) {
                continue;
            }
            if mapping_is_command_code_goat(&candidate.mapping) {
                match goat_runtimes.get(&account.id) {
                    Some(runtime) if runtime.serves(&candidate.plan.model) => {}
                    Some(_) => {
                        rejected.push(format!(
                            "{}/{} account `{}`: Command Code GOAT catalog does not include model `{}`",
                            account.provider_id,
                            account.offering_id,
                            account.name,
                            candidate.plan.model
                        ));
                        continue;
                    }
                    None => {
                        rejected.push(format!(
                            "{}/{} account `{}`: Command Code GOAT production inference endpoint, auth, protocol, and model catalog are not verified; route is disabled",
                            account.provider_id, account.offering_id, account.name
                        ));
                        continue;
                    }
                }
            }
            let plan = if mapping_is_configurable_http(&candidate.mapping) {
                match materialize_custom_account_plan(
                    account,
                    custom_runtimes.get(&account.id),
                    config,
                    parsed,
                    client_model,
                    routing_model,
                    resolved_alias.clone(),
                    contracts,
                ) {
                    Ok(plan) => plan,
                    Err(error) => {
                        rejected.push(format!(
                            "{}/{} account `{}`: {error}",
                            account.provider_id, account.offering_id, account.name
                        ));
                        continue;
                    }
                }
            } else {
                candidate.plan.clone()
            };
            match provider_adapter::supports_production_plan(account, config, &plan, contracts) {
                Ok(()) => {
                    routes.push(MaterializedCandidate {
                        routing: RoutingCandidate {
                            account: account.clone(),
                            channel: plan.channel,
                            resolved_model: plan.model.clone(),
                        },
                        plan,
                    });
                    break;
                }
                Err(error) => rejected.push(format!(
                    "{}/{} account `{}`: {error}",
                    account.provider_id, account.offering_id, account.name
                )),
            }
        }
    }
    let incompatibility = (routes.is_empty() && !rejected.is_empty()).then(|| {
        format!(
            "no compatible provider account for model `{client_model}` and {:?}: {}",
            parsed.client,
            rejected.join("; ")
        )
    });
    Ok(MaterializedRouteSet {
        routes,
        free_only,
        incompatibility,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::{self, ResolvedModel};
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::custom::CustomAccountRuntime;
    use crate::gateway::protocol::{ApiFormat, parse_client_request};
    use crate::gateway::provider_adapter::install_goat_loopback_route_for_test;
    use crate::goat::GoatAccountRuntime;
    use crate::models::{
        Account, AccountCustomConfig, AccountModelCapability, AccountSetupStep, AccountType,
        AppConfig,
    };
    use crate::provider::{
        ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, COMMAND_CODE_PROVIDER_ID, CPA_ACCOUNT_ID,
        CPA_OFFERING_ID, CPA_PROVIDER_ID, CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID,
        ConnectionVerificationStatus, CredentialKind, GO_OFFERING_ID, GOAT_OFFERING_ID,
        OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID, ProviderAdapterKind, QuotaScope,
        UpstreamProtocolKind, ZEN_FREE_ACCOUNT_ID, ZEN_FREE_ACCOUNT_NAME,
    };
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Arc;

    fn chat_body(model: &str) -> Bytes {
        Bytes::from(
            serde_json::to_vec(&json!({
                "model": model,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .unwrap(),
        )
    }

    fn account(
        id: &str,
        provider_id: &str,
        offering_id: &str,
        credential_kind: CredentialKind,
        quota_scope: QuotaScope,
    ) -> Account {
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        Account {
            id: id.into(),
            provider_id: provider_id.into(),
            offering_id: offering_id.into(),
            credential_kind,
            quota_scope,
            name: id.into(),
            username: None,
            password_cipher: None,
            key_cipher: cipher.encrypt("key").unwrap(),
            enabled: true,
            account_type: AccountType::Key,
            setup_step: AccountSetupStep::Ready,
            referral_code: None,
            purchase_date: String::new(),
            expires_on: String::new(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            cooldown_free_until: None,
            last_error: None,
            auth_error: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn go_account(id: &str) -> Account {
        account(
            id,
            OPENCODE_PROVIDER_ID,
            GO_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        )
    }

    fn zen_account() -> Account {
        let mut item = account(
            ZEN_FREE_ACCOUNT_ID,
            OPENCODE_ZEN_FREE_PROVIDER_ID,
            ANONYMOUS_FREE_OFFERING_ID,
            CredentialKind::None,
            QuotaScope::EgressIp,
        );
        item.name = ZEN_FREE_ACCOUNT_NAME.into();
        item
    }

    fn cpa_account() -> Account {
        account(
            CPA_ACCOUNT_ID,
            CPA_PROVIDER_ID,
            CPA_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        )
    }

    fn goat_runtime(id: &str, _models: &[&str]) -> GoatAccountRuntime {
        GoatAccountRuntime {
            account_id: id.into(),
            enabled: true,
            verification_status: ConnectionVerificationStatus::Verified,
            setup_ready: true,
            has_key: true,
        }
    }

    fn goat_runtimes(
        id: &str,
        models: &[&str],
    ) -> std::collections::HashMap<String, GoatAccountRuntime> {
        let mut runtimes = std::collections::HashMap::new();
        runtimes.insert(id.to_string(), goat_runtime(id, models));
        runtimes
    }

    fn goat_account(id: &str) -> Account {
        account(
            id,
            COMMAND_CODE_PROVIDER_ID,
            GOAT_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        )
    }

    fn static_contracts() -> crate::provider_contracts::EffectiveContractSet {
        contracts_for(&[])
    }

    fn goat_contracts(models: &[&str]) -> crate::provider_contracts::EffectiveContractSet {
        let now = Utc::now();
        let scope = crate::provider_contracts::ContractScope::provider(COMMAND_CODE_PROVIDER_ID);
        let mut persisted = crate::provider_contracts::PersistedContracts::default();
        persisted.scopes.insert(
            scope.clone(),
            crate::provider_contracts::PersistedScopeRow {
                scope,
                catalog_models: models.iter().map(|model| (*model).to_string()).collect(),
                catalog_refreshed_at: Some(now),
                catalog_source: crate::provider_contracts::CATALOG_SOURCE_COMMAND_CODE_MODELS
                    .into(),
                catalog_source_url: crate::provider::COMMAND_CODE_GOAT_BASE_URL.into(),
                revision: 1,
                updated_at: now,
            },
        );
        persisted.overrides.insert(
            crate::provider_contracts::ContractScope::provider(COMMAND_CODE_PROVIDER_ID),
            models
                .iter()
                .map(
                    |model| crate::provider_contracts::PersistedModelProtocolOverride {
                        scope: crate::provider_contracts::ContractScope::provider(
                            COMMAND_CODE_PROVIDER_ID,
                        ),
                        model_id: (*model).to_string(),
                        protocol: if ocg_domain::protocol::command_code_is_anthropic_model(model) {
                            crate::provider::UpstreamProtocolKind::Messages
                        } else {
                            crate::provider::UpstreamProtocolKind::ChatCompletions
                        },
                        state: crate::provider_contracts::ProtocolOverrideState::ForceOn,
                        updated_at: now,
                    },
                )
                .collect(),
        );
        crate::provider_contracts::build_effective_contracts(
            &crate::zen_models::ZenFreeModelCatalog::default(),
            &[],
            persisted,
        )
    }

    fn contracts_for(
        runtimes: &[CustomAccountRuntime],
    ) -> crate::provider_contracts::EffectiveContractSet {
        crate::provider_contracts::build_effective_contracts(
            &crate::zen_models::ZenFreeModelCatalog::default(),
            runtimes,
            crate::provider_contracts::PersistedContracts::default(),
        )
    }

    fn routes_for(
        model: &str,
        accounts: &[Account],
        config: &AppConfig,
        free_available: bool,
    ) -> MaterializedRouteSet {
        routes_for_with_contracts(model, accounts, config, free_available, &static_contracts())
    }

    fn routes_for_with_contracts(
        model: &str,
        accounts: &[Account],
        config: &AppConfig,
        free_available: bool,
        contracts: &crate::provider_contracts::EffectiveContractSet,
    ) -> MaterializedRouteSet {
        let body = chat_body(model);
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = alias::resolve(model).unwrap();
        materialize_account_routes(
            accounts,
            config,
            &parsed,
            &resolved,
            &parsed.requested_model,
            model,
            &body,
            free_available,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            None,
            contracts,
        )
        .unwrap()
    }

    #[test]
    fn go_alias_materializes_opencode_go_candidates() {
        let config = AppConfig::default();
        let set = routes_for(
            "glm-5.2",
            &[go_account("go-1"), zen_account()],
            &config,
            true,
        );
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(set.routes[0].plan.model, "glm-5.2");
        assert_eq!(set.routes[0].plan.client_model, "glm-5.2");
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Go);
        assert!(!set.free_only);
        let identity = native_log_identity(&set.routes[0].plan);
        assert_eq!(identity.requested_model, "glm-5.2");
        assert_eq!(identity.resolved_alias.as_deref(), Some("glm-5.2"));
        assert_eq!(identity.upstream_model, "glm-5.2");
    }

    #[test]
    fn unknown_cpa_raw_model_defaults_to_chat_and_uses_local_base() {
        let model = "vendor/cpa-new-model";
        let cpa_models = vec![model.to_string()];
        let resolved = alias::resolve_with_runtime_catalogs(
            model,
            alias::RuntimeCatalogs {
                cpa: &cpa_models,
                ..alias::RuntimeCatalogs::default()
            },
        )
        .unwrap();
        let body = chat_body(model);
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let set = materialize_account_routes(
            &[cpa_account()],
            &AppConfig::default(),
            &parsed,
            &resolved,
            model,
            model,
            &body,
            true,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            Some(crate::cpa::DEFAULT_CPA_BASE_URL),
            &static_contracts(),
        )
        .unwrap();
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::ChatCompletions);
        assert_eq!(
            set.routes[0].plan.upstream_base_override.as_deref(),
            Some(crate::cpa::DEFAULT_CPA_BASE_URL)
        );
    }

    #[test]
    fn mixed_case_go_alias_preserves_requested_casing() {
        let config = AppConfig::default();
        let set = routes_for("MiniMax-M3", &[go_account("go-1")], &config, true);
        assert_eq!(set.routes[0].plan.model, "MiniMax-M3");
        assert_eq!(set.routes[0].plan.client_model, "MiniMax-M3");
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::ChatCompletions);
        let identity = native_log_identity(&set.routes[0].plan);
        assert_eq!(identity.requested_model, "MiniMax-M3");
        assert_eq!(identity.resolved_alias.as_deref(), Some("minimax-m3"));
        assert_eq!(identity.upstream_model, "MiniMax-M3");
    }

    #[test]
    fn zen_free_alias_materializes_anonymous_channel() {
        let config = AppConfig::default();
        let set = routes_for(
            "hy3-free",
            &[go_account("go-1"), zen_account()],
            &config,
            true,
        );
        assert!(set.free_only);
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, ZEN_FREE_ACCOUNT_ID);
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Free);
        assert_eq!(set.routes[0].plan.model, "hy3-free");
        assert!(set.routes[0].plan.upstream_base_override.is_some());
    }

    #[test]
    fn shared_alias_builds_go_and_free_candidates_in_account_order() {
        let config = AppConfig::default();
        let set = routes_for("hy3", &[go_account("go-1"), zen_account()], &config, true);
        assert_eq!(set.routes.len(), 2);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Go);
        assert_eq!(set.routes[1].routing.account.id, ZEN_FREE_ACCOUNT_ID);
        assert_eq!(set.routes[1].plan.channel, UpstreamChannel::Free);
        assert_eq!(set.routes[1].plan.model, "hy3-free");
        assert_eq!(set.routes[1].plan.client_model, "hy3");
        assert!(set.routes[1].plan.original_model.is_none());
        assert!(!set.routes[1].plan.allow_go_fallback);
        let free_identity = native_log_identity(&set.routes[1].plan);
        assert_eq!(free_identity.requested_model, "hy3");
        assert_eq!(free_identity.resolved_alias.as_deref(), Some("hy3"));
        assert_eq!(free_identity.upstream_model, "hy3-free");
    }

    #[test]
    fn pinned_raw_stays_pinned_to_its_provider() {
        let config = AppConfig::default();
        let body = chat_body("vendor.gadget-v1");
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = ResolvedModel::PinnedRaw {
            requested: "vendor.gadget-v1".into(),
            mapping: crate::alias::ProviderMapping {
                provider_id: OPENCODE_PROVIDER_ID,
                offering_id: GO_OFFERING_ID,
                upstream_model: "deepseek-v4-flash".into(),
                routeable: true,
            },
        };
        let set = materialize_account_routes(
            &[go_account("go-1"), zen_account()],
            &config,
            &parsed,
            &resolved,
            "vendor.gadget-v1",
            "vendor.gadget-v1",
            &body,
            true,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            None,
            &static_contracts(),
        )
        .unwrap();
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Go);
        assert_eq!(set.routes[0].plan.model, "deepseek-v4-flash");
        assert_eq!(set.routes[0].plan.client_model, "vendor.gadget-v1");
        assert!(!set.routes[0].plan.allow_go_fallback);
        assert!(set.routes[0].plan.original_model.is_none());
        let identity = native_log_identity(&set.routes[0].plan);
        assert_eq!(identity.requested_model, "vendor.gadget-v1");
        assert_eq!(
            identity.resolved_alias.as_deref(),
            Some("deepseek-v4-flash")
        );
        assert_eq!(identity.upstream_model, "deepseek-v4-flash");
    }

    #[test]
    fn mapping_plans_follow_registry_order_while_candidates_keep_account_order() {
        let config = AppConfig::default();
        let body = chat_body("widget");
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = ResolvedModel::Alias {
            requested: "widget".into(),
            alias: "widget".into(),
            mappings: vec![
                crate::alias::ProviderMapping {
                    provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
                    offering_id: ANONYMOUS_FREE_OFFERING_ID,
                    upstream_model: "hy3-free".into(),
                    routeable: true,
                },
                crate::alias::ProviderMapping {
                    provider_id: OPENCODE_PROVIDER_ID,
                    offering_id: GO_OFFERING_ID,
                    upstream_model: "glm-5.2".into(),
                    routeable: true,
                },
            ],
        };
        let set = materialize_account_routes(
            &[go_account("go-1"), zen_account()],
            &config,
            &parsed,
            &resolved,
            "widget",
            "widget",
            &body,
            true,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            None,
            &static_contracts(),
        )
        .unwrap();
        assert_eq!(set.routes.len(), 2);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Go);
        assert_eq!(set.routes[0].plan.model, "glm-5.2");
        assert_eq!(set.routes[1].routing.account.id, ZEN_FREE_ACCOUNT_ID);
        assert_eq!(set.routes[1].plan.channel, UpstreamChannel::Free);
        assert_eq!(set.routes[1].plan.model, "hy3-free");
    }

    #[test]
    fn pinned_raw_unverified_goat_is_fail_closed_through_adapter() {
        let config = AppConfig::default();
        let body = chat_body(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM);
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = ResolvedModel::PinnedRaw {
            requested: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into(),
            mapping: crate::alias::ProviderMapping {
                provider_id: COMMAND_CODE_PROVIDER_ID,
                offering_id: GOAT_OFFERING_ID,
                upstream_model: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into(),
                routeable: true,
            },
        };
        let set = materialize_account_routes(
            &[goat_account("goat-1"), go_account("go-1")],
            &config,
            &parsed,
            &resolved,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &body,
            true,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            None,
            &goat_contracts(&[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM]),
        )
        .unwrap();
        assert!(set.routes.is_empty());
        assert!(set.incompatibility.as_deref().is_some_and(|message| {
            message.contains("not verified")
                || message.contains("disabled")
                || message.contains("unsupported")
        }));
    }

    #[test]
    fn goat_without_loopback_is_fail_closed() {
        let config = AppConfig::default();
        let set = routes_for(
            "glm-5.2",
            &[goat_account("goat-1"), go_account("go-1")],
            &config,
            true,
        );
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
    }

    #[test]
    fn goat_alias_does_not_steal_go_requests_even_with_loopback() {
        let config = AppConfig::default();
        let goat = goat_account("goat-loop-alias");
        let _guard =
            install_goat_loopback_route_for_test(goat.id.clone(), "http://127.0.0.1:9").unwrap();
        let set = routes_for(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
            &[goat, go_account("go-1")],
            &config,
            true,
        );
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(
            set.routes[0].plan.model,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS
        );
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::ChatCompletions);
    }

    #[test]
    fn goat_slash_raw_pins_through_loopback_as_chat() {
        let config = AppConfig::default();
        let goat = goat_account("goat-loop-raw");
        let _guard =
            install_goat_loopback_route_for_test(goat.id.clone(), "http://127.0.0.1:9").unwrap();
        let body = chat_body(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM);
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = crate::alias::resolve_with_catalogs(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &[],
            &[],
            &[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into()],
        )
        .unwrap();
        let set = materialize_account_routes(
            &[goat, go_account("go-1")],
            &config,
            &parsed,
            &resolved,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &body,
            true,
            &std::collections::HashMap::new(),
            &goat_runtimes(
                "goat-loop-raw",
                &[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM],
            ),
            None,
            &goat_contracts(&[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM]),
        )
        .unwrap();
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "goat-loop-raw");
        assert_eq!(
            set.routes[0].plan.model,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
        );
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::ChatCompletions);
        assert_eq!(
            set.routes[0].plan.client_model,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
        );
    }

    #[test]
    fn goat_anthropic_alias_uses_messages_and_converts_client_responses() {
        let config = AppConfig::default();
        let goat = goat_account("goat-claude");
        let runtimes = goat_runtimes("goat-claude", &["claude-sonnet-4-6"]);
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": "claude-sonnet-4-6",
                "input": [{"role": "user", "content": "hi"}],
                "store": false
            }))
            .unwrap(),
        );
        let parsed = parse_client_request(ApiFormat::Responses, body.clone()).unwrap();
        let resolved = crate::alias::resolve_with_catalogs(
            "claude-sonnet-4-6",
            &[],
            &[],
            &["claude-sonnet-4-6".into()],
        )
        .unwrap();
        let set = materialize_account_routes(
            &[goat],
            &config,
            &parsed,
            &resolved,
            "claude-sonnet-4-6",
            "claude-sonnet-4-6",
            &body,
            true,
            &std::collections::HashMap::new(),
            &runtimes,
            None,
            &goat_contracts(&["claude-sonnet-4-6"]),
        )
        .unwrap();
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].plan.client, ApiFormat::Responses);
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::Messages);
        assert_eq!(set.routes[0].plan.model, "claude-sonnet-4-6");
    }

    #[test]
    fn goat_slash_raw_without_loopback_is_fail_closed() {
        let config = AppConfig::default();
        let set = routes_for_with_contracts(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &[goat_account("goat-1"), go_account("go-1")],
            &config,
            true,
            &goat_contracts(&[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM]),
        );
        assert!(set.routes.is_empty());
        assert!(set.incompatibility.as_deref().is_some_and(|message| {
            message.contains("not verified")
                || message.contains("disabled")
                || message.contains("unsupported")
        }));
    }

    #[test]
    fn resolve_error_exposes_ambiguous_code() {
        let error = protocol_error_from_resolve(crate::alias::ResolveError::Ambiguous {
            requested: "shared-raw".into(),
            mappings: vec![
                crate::alias::ProviderMapping {
                    provider_id: OPENCODE_PROVIDER_ID,
                    offering_id: GO_OFFERING_ID,
                    upstream_model: "shared-raw".into(),
                    routeable: true,
                },
                crate::alias::ProviderMapping {
                    provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
                    offering_id: ANONYMOUS_FREE_OFFERING_ID,
                    upstream_model: "shared-raw".into(),
                    routeable: true,
                },
            ],
        });
        assert_eq!(error.code, Some(crate::alias::AMBIGUOUS_MODEL_ID));
        assert!(error.message.contains("alias"));
    }

    #[test]
    fn parse_helpers_are_reexported_for_adapters() {
        let parsed = parse_client(ApiFormat::ChatCompletions, chat_body("glm-5.2")).unwrap();
        assert_eq!(parsed.requested_model, "glm-5.2");
        let gemini = parse_gemini(
            "glm-5.2".into(),
            false,
            Bytes::from(
                serde_json::to_vec(&json!({"contents":[{"role":"user","parts":[{"text":"hi"}]}]}))
                    .unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(gemini.client, ApiFormat::Gemini);
    }

    #[test]
    fn claude_desktop_identity_keeps_client_name_and_mapped_alias() {
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": crate::models::CLAUDE_DESKTOP_OPUS_ALIAS,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .unwrap(),
        );
        let parsed = parse_client_request(ApiFormat::Messages, body).unwrap();
        let plan = materialize_parsed_request(
            &parsed,
            &MaterializeSpec {
                client_model: parsed.requested_model.clone(),
                upstream_model: "glm-5.2".into(),
                resolved_alias: Some("glm-5.2".into()),
                channel: UpstreamChannel::Go,
                upstream_base_override: None,
                original_model: None,
                allow_go_fallback: false,
                forced_upstream: None,
                custom_route: None,
            },
        )
        .unwrap();
        let identity = native_log_identity(&plan);
        assert_eq!(
            identity.requested_model,
            crate::models::CLAUDE_DESKTOP_OPUS_ALIAS
        );
        assert_eq!(identity.resolved_alias.as_deref(), Some("glm-5.2"));
        assert_eq!(identity.upstream_model, "glm-5.2");
    }

    fn custom_account(id: &str) -> Account {
        account(
            id,
            CUSTOM_PROVIDER_ID,
            CUSTOM_API_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        )
    }

    fn custom_runtime(
        account_id: &str,
        model_id: &str,
        protocol: UpstreamProtocolKind,
    ) -> CustomAccountRuntime {
        CustomAccountRuntime {
            account_id: account_id.into(),
            enabled: true,
            verification_status: ConnectionVerificationStatus::Verified,
            setup_ready: true,
            has_key: true,
            config: AccountCustomConfig {
                account_id: account_id.into(),
                endpoint_url: match protocol {
                    UpstreamProtocolKind::ChatCompletions => {
                        "http://127.0.0.1:9/v1/chat/completions"
                    }
                    UpstreamProtocolKind::Responses => "http://127.0.0.1:9/v1/responses",
                    UpstreamProtocolKind::Messages => "http://127.0.0.1:9/v1/messages",
                }
                .into(),
                upstream_protocol: protocol,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            capabilities: vec![AccountModelCapability {
                account_id: account_id.into(),
                public_model: model_id.into(),
                upstream_model: model_id.into(),
                protocol,
                verified_at: None,
                source: "manual".into(),
            }],
        }
    }

    #[test]
    fn materialize_dispatches_builtin_and_custom_through_adapter_kinds() {
        assert_eq!(
            mapping_adapter_kind(&crate::alias::ProviderMapping {
                provider_id: OPENCODE_PROVIDER_ID,
                offering_id: GO_OFFERING_ID,
                upstream_model: "glm-5.2".into(),
                routeable: true,
            }),
            Some(ProviderAdapterKind::OpenCodeGo)
        );
        assert_eq!(
            mapping_adapter_kind(&crate::alias::ProviderMapping {
                provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
                offering_id: ANONYMOUS_FREE_OFFERING_ID,
                upstream_model: "mimo-v2.5-free".into(),
                routeable: true,
            }),
            Some(ProviderAdapterKind::ZenFree)
        );
        assert_eq!(
            mapping_adapter_kind(&crate::alias::ProviderMapping {
                provider_id: CUSTOM_PROVIDER_ID,
                offering_id: CUSTOM_API_OFFERING_ID,
                upstream_model: "local".into(),
                routeable: true,
            }),
            Some(ProviderAdapterKind::ConfigurableHttp)
        );
        assert!(!mapping_is_configurable_http(
            &crate::alias::ProviderMapping {
                provider_id: COMMAND_CODE_PROVIDER_ID,
                offering_id: GOAT_OFFERING_ID,
                upstream_model: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into(),
                routeable: false,
            }
        ));
    }

    #[test]
    fn custom_candidate_diagnostic_passthrough_keeps_client_protocol() {
        let resolved =
            alias::resolve_with_custom("local-custom", &["local-custom".into()]).unwrap();
        assert_eq!(
            diagnostic_forced_upstream(&resolved, ApiFormat::Responses),
            Some(ApiFormat::Responses)
        );
        assert_eq!(
            diagnostic_forced_upstream(&resolved, ApiFormat::Messages),
            Some(ApiFormat::Messages)
        );
        let mixed = alias::resolve_with_custom("hy3", &["hy3".into()]).unwrap();
        assert_eq!(
            diagnostic_forced_upstream(&mixed, ApiFormat::Responses),
            Some(ApiFormat::Responses)
        );
        let builtin = alias::resolve("hy3").unwrap();
        assert_eq!(
            diagnostic_forced_upstream(&builtin, ApiFormat::Responses),
            None
        );
        let goat = crate::alias::resolve_with_catalogs(
            "claude-sonnet-4-6",
            &[],
            &[],
            &["claude-sonnet-4-6".into()],
        )
        .unwrap();
        assert_eq!(
            diagnostic_forced_upstream(&goat, ApiFormat::Responses),
            Some(ApiFormat::Responses)
        );
        assert_eq!(
            diagnostic_forced_upstream(&goat, ApiFormat::Messages),
            Some(ApiFormat::Messages)
        );
    }

    #[test]
    fn custom_native_responses_structured_format_does_not_guess_chat() {
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": "local-custom",
                "input": "hi",
                "store": false,
                "text": {
                    "format": {
                        "type": "json_schema",
                        "name": "answer",
                        "schema": {"type": "object"}
                    }
                }
            }))
            .unwrap(),
        );
        let parsed = parse_client_request(ApiFormat::Responses, body.clone()).unwrap();
        let resolved =
            alias::resolve_with_custom("local-custom", &["local-custom".into()]).unwrap();
        let account = custom_account("custom-1");
        let runtime = custom_runtime("custom-1", "local-custom", UpstreamProtocolKind::Responses);
        let mut runtimes = std::collections::HashMap::new();
        let contracts = contracts_for(std::slice::from_ref(&runtime));
        runtimes.insert(account.id.clone(), runtime);
        let set = materialize_account_routes(
            &[account],
            &AppConfig::default(),
            &parsed,
            &resolved,
            &parsed.requested_model,
            "local-custom",
            &body,
            false,
            &runtimes,
            &std::collections::HashMap::new(),
            None,
            &contracts,
        )
        .expect("native Responses structured output must not be rejected via Chat conversion");
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::Responses);
        assert_eq!(set.routes[0].plan.client, ApiFormat::Responses);
        let upstream: serde_json::Value = serde_json::from_slice(&set.routes[0].plan.body).unwrap();
        assert_eq!(upstream["text"]["format"]["type"], "json_schema");
    }

    #[test]
    fn custom_native_messages_structured_format_does_not_guess_chat() {
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": "local-custom",
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}],
                "output_config": {
                    "format": {"type": "json_schema", "schema": {"type": "object"}}
                }
            }))
            .unwrap(),
        );
        let parsed = parse_client_request(ApiFormat::Messages, body.clone()).unwrap();
        let resolved =
            alias::resolve_with_custom("local-custom", &["local-custom".into()]).unwrap();
        let account = custom_account("custom-1");
        let runtime = custom_runtime("custom-1", "local-custom", UpstreamProtocolKind::Messages);
        let mut runtimes = std::collections::HashMap::new();
        let contracts = contracts_for(std::slice::from_ref(&runtime));
        runtimes.insert(account.id.clone(), runtime);
        let set = materialize_account_routes(
            &[account],
            &AppConfig::default(),
            &parsed,
            &resolved,
            &parsed.requested_model,
            "local-custom",
            &body,
            false,
            &runtimes,
            &std::collections::HashMap::new(),
            None,
            &contracts,
        )
        .expect("native Messages structured output must not be rejected via Chat conversion");
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::Messages);
        let upstream: serde_json::Value = serde_json::from_slice(&set.routes[0].plan.body).unwrap();
        assert_eq!(upstream["output_config"]["format"]["type"], "json_schema");
    }

    #[test]
    fn custom_single_protocol_converts_other_client_wire_formats() {
        fn route_upstream(client: ApiFormat, body: Bytes) -> ApiFormat {
            let parsed = if client == ApiFormat::Gemini {
                parse_gemini("local-custom".into(), false, body.clone()).unwrap()
            } else {
                parse_client_request(client, body.clone()).unwrap()
            };
            let resolved =
                alias::resolve_with_custom("local-custom", &["local-custom".into()]).unwrap();
            let account = custom_account("custom-single");
            let runtime = custom_runtime(
                "custom-single",
                "local-custom",
                UpstreamProtocolKind::Messages,
            );
            let contracts = contracts_for(std::slice::from_ref(&runtime));
            let mut runtimes = std::collections::HashMap::new();
            runtimes.insert(account.id.clone(), runtime);
            let set = materialize_account_routes(
                &[account],
                &AppConfig::default(),
                &parsed,
                &resolved,
                &parsed.requested_model,
                "local-custom",
                &body,
                false,
                &runtimes,
                &std::collections::HashMap::new(),
                None,
                &contracts,
            )
            .expect("single-protocol account must convert supported client formats");
            assert_eq!(set.routes.len(), 1);
            set.routes[0].plan.upstream
        }

        let chat = route_upstream(ApiFormat::ChatCompletions, chat_body("local-custom"));
        assert_eq!(chat, ApiFormat::Messages);
        let responses = route_upstream(
            ApiFormat::Responses,
            Bytes::from(
                serde_json::to_vec(&json!({
                    "model": "local-custom",
                    "input": "hi",
                    "store": false,
                    "max_output_tokens": 4
                }))
                .unwrap(),
            ),
        );
        assert_eq!(responses, ApiFormat::Messages);
        let messages = route_upstream(
            ApiFormat::Messages,
            Bytes::from(
                serde_json::to_vec(&json!({
                    "model": "local-custom",
                    "max_tokens": 4,
                    "messages": [{"role": "user", "content": "hi"}]
                }))
                .unwrap(),
            ),
        );
        assert_eq!(messages, ApiFormat::Messages);
        let gemini = route_upstream(
            ApiFormat::Gemini,
            Bytes::from(
                serde_json::to_vec(&json!({
                    "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
                    "generationConfig": {"maxOutputTokens": 4}
                }))
                .unwrap(),
            ),
        );
        assert_eq!(gemini, ApiFormat::Messages);
    }

    #[test]
    fn custom_without_scope_contract_does_not_produce_a_candidate() {
        let body = chat_body("local-custom");
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved =
            alias::resolve_with_custom("local-custom", &["local-custom".into()]).unwrap();
        let account = custom_account("custom-missing-scope");
        let runtime = custom_runtime(
            "custom-missing-scope",
            "local-custom",
            UpstreamProtocolKind::ChatCompletions,
        );
        let mut runtimes = std::collections::HashMap::new();
        runtimes.insert(account.id.clone(), runtime);
        let set = materialize_account_routes(
            &[account],
            &AppConfig::default(),
            &parsed,
            &resolved,
            &parsed.requested_model,
            "local-custom",
            &body,
            false,
            &runtimes,
            &std::collections::HashMap::new(),
            None,
            &static_contracts(),
        )
        .expect("missing custom contract must fail closed without a protocol error for mixed resolution");
        assert!(
            set.routes.is_empty(),
            "no production Custom candidate without ContractScope::CustomEndpoint"
        );
        assert!(set.incompatibility.as_deref().is_some_and(|message| {
            message.contains("no effective contract") || message.contains("custom_endpoint")
        }));
    }

    #[test]
    fn probed_opencode_protocol_is_selected_after_contract_evidence() {
        let body = chat_body("grok-4.5");
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = alias::resolve("grok-4.5").unwrap();
        let account = go_account("go-probe");
        let before = materialize_account_routes(
            std::slice::from_ref(&account),
            &AppConfig::default(),
            &parsed,
            &resolved,
            &parsed.requested_model,
            "grok-4.5",
            &body,
            false,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            None,
            &static_contracts(),
        )
        .unwrap();
        assert_eq!(before.routes.len(), 1);
        assert_eq!(before.routes[0].plan.upstream, ApiFormat::Responses);

        let now = Utc::now();
        let mut persisted = crate::provider_contracts::PersistedContracts::default();
        let scope = crate::provider_contracts::ContractScope::provider(OPENCODE_PROVIDER_ID);
        persisted.evidence.insert(
            scope.clone(),
            vec![crate::provider_contracts::PersistedModelProtocol {
                scope,
                model_id: "grok-4.5".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: crate::provider_contracts::ContractEvidenceSource::ProbeConfirmed,
                verified_at: Some(now),
                observed_at: Some(now),
                last_probe_result: Some(crate::provider_contracts::ProbeResultKind::Success),
                last_probe_at: Some(now),
                last_probe_error: None,
            }],
        );
        let contracts = crate::provider_contracts::build_effective_contracts(
            &crate::zen_models::ZenFreeModelCatalog::default(),
            &[],
            persisted,
        );
        let after = materialize_account_routes(
            &[account],
            &AppConfig::default(),
            &parsed,
            &resolved,
            &parsed.requested_model,
            "grok-4.5",
            &body,
            false,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            None,
            &contracts,
        )
        .unwrap();
        assert_eq!(after.routes.len(), 1);
        assert_eq!(after.routes[0].plan.upstream, ApiFormat::ChatCompletions);
        assert_eq!(after.routes[0].routing.account.id, "go-probe");
    }
}
