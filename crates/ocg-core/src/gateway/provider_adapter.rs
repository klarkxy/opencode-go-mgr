//! Provider offering adapters: endpoint, auth, and capability checks.
//!
//! Authentication belongs to the provider/offering, not the wire protocol.
//! [`resolve_route`] dispatches exhaustively on
//! [`crate::provider::ProviderAdapterKind`] onto sealed route helpers. Alias resolution
//! stays ahead of this seam: Alias and PinnedRaw candidates both materialize a
//! [`RequestPlan`] then call here. Adapters must not probe a billable inference
//! path to discover protocol support.
//!
//! Route resolution returns a data-only [`crate::gateway::attempt::AttemptSpec`]:
//! endpoint, path, upstream protocol, auth scheme, redirect policy, an opaque
//! credential handle, and the proxy-routing model. Adapters take an account,
//! config, and request plan. They do not decrypt keys, open databases, or
//! build HTTP clients; the Host resolver and single-attempt executor do that.
//!
//! Production Command Code GOAT uses the official Provider API origin after
//! explicit verification. [`command_code_goat_transport_spec`] proves
//! host/path/auth construction. The GOAT loopback helper substitutes a
//! loopback origin only and still uses `/provider/v1/...`.
//! Configurable HTTP is the Custom API identity, not a base class.

use crate::custom_http::{custom_auth_scheme, join_inference_endpoint, resolve_custom_endpoints};
use crate::gateway::attempt::{AttemptSpec, CredentialHandle, ProxyRoutingModel};
use crate::gateway::free_models::resolve_upstream_base;
use crate::gateway::protocol::{
    ApiFormat, RequestPlan, command_code_supports_upstream, command_code_upstream_path,
    opencode_supports_upstream,
};
use crate::models::{Account, AppConfig, UpstreamChannel};
use crate::provider::{
    COMMAND_CODE_GOAT_BASE_URL, COMMAND_CODE_GOAT_CHAT_COMPLETIONS_PATH, COMMAND_CODE_GOAT_HOST,
    COMMAND_CODE_GOAT_MESSAGES_PATH, COMMAND_CODE_GOAT_MODELS_PATH, CPA_ACCOUNT_ID, CredentialKind,
    InferenceAuthDescriptor, KIMI_CN_BASE_URL, KIMI_CN_CHAT_COMPLETIONS_PATH, MINIMAX_CN_BASE_URL,
    MINIMAX_CN_CHAT_COMPLETIONS_PATH, ProviderAdapterKind, ProviderRegistry, QuotaScope,
    UpstreamAuthScheme, ZEN_FREE_ACCOUNT_ID,
};
use crate::provider_contracts::EffectiveContractSet;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

pub(crate) use crate::gateway::attempt::UpstreamAuth;

/// Deterministic official Command Code GOAT transport. Production inference
/// uses this origin after an account is enabled, verified, and catalogued.
/// Loopback substitutes exist only as a test seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandCodeGoatTransportSpec {
    pub base_url: &'static str,
    pub host: &'static str,
    pub chat_completions_path: &'static str,
    pub messages_path: &'static str,
    pub models_path: &'static str,
    pub auth_scheme: UpstreamAuthScheme,
    pub follow_redirects: bool,
    pub zdr_header_name: Option<&'static str>,
    pub public_catalog_refresh: bool,
}

pub fn command_code_goat_transport_spec() -> CommandCodeGoatTransportSpec {
    CommandCodeGoatTransportSpec {
        base_url: COMMAND_CODE_GOAT_BASE_URL,
        host: COMMAND_CODE_GOAT_HOST,
        chat_completions_path: COMMAND_CODE_GOAT_CHAT_COMPLETIONS_PATH,
        messages_path: COMMAND_CODE_GOAT_MESSAGES_PATH,
        models_path: COMMAND_CODE_GOAT_MODELS_PATH,
        auth_scheme: UpstreamAuthScheme::Bearer,
        follow_redirects: false,
        zdr_header_name: None,
        public_catalog_refresh: true,
    }
}

pub fn command_code_goat_join_url(base: &str, upstream: ApiFormat) -> Result<String, String> {
    let path = command_code_upstream_path(upstream)
        .ok_or_else(|| format!("Command Code GOAT has no upstream path for {upstream:?}"))?;
    join_inference_endpoint(base, path)
        .map(|url| url.to_string())
        .map_err(|error| error.to_string())
}

pub fn command_code_goat_official_url(upstream: ApiFormat) -> Result<String, String> {
    command_code_goat_join_url(COMMAND_CODE_GOAT_BASE_URL, upstream)
}

pub fn command_code_goat_loopback_base(origin: &str) -> String {
    format!("{}/provider/v1", origin.trim_end_matches('/'))
}

#[derive(Debug, Clone)]
struct GoatLoopbackRoute {
    origin: String,
}

static GOAT_LOOPBACK_ROUTES: LazyLock<RwLock<HashMap<String, GoatLoopbackRoute>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[cfg(debug_assertions)]
#[doc(hidden)]
pub use crate::goat::{GoatVerifyOriginGuard, install_goat_verify_origin_for_test};

/// RAII guard for the integration-only GOAT seam. The production adapter has
/// no endpoint or protocol guesses: without a live guard, GOAT is unsupported.
#[doc(hidden)]
pub struct GoatLoopbackRouteGuard {
    account_id: String,
    base_url: String,
}

impl Drop for GoatLoopbackRouteGuard {
    fn drop(&mut self) {
        if let Ok(mut routes) = GOAT_LOOPBACK_ROUTES.write()
            && routes
                .get(&self.account_id)
                .is_some_and(|route| route.origin == self.base_url)
        {
            routes.remove(&self.account_id);
        }
    }
}

/// Installs a loopback-only origin substitute used by gateway integration tests.
/// Models, protocol, path, and Bearer auth come from the official Command Code
/// contract; this cannot configure a remote production endpoint.
#[doc(hidden)]
pub fn install_goat_loopback_route_for_test(
    account_id: impl Into<String>,
    origin: impl Into<String>,
) -> Result<GoatLoopbackRouteGuard, String> {
    let account_id = account_id.into();
    let origin = origin.into();
    ensure_loopback_base(&origin)?;
    let route = GoatLoopbackRoute {
        origin: origin.trim_end_matches('/').to_string(),
    };
    let guard = GoatLoopbackRouteGuard {
        account_id: account_id.clone(),
        base_url: route.origin.clone(),
    };
    GOAT_LOOPBACK_ROUTES
        .write()
        .map_err(|_| "GOAT loopback route lock is poisoned".to_string())?
        .insert(account_id, route);
    Ok(guard)
}

#[derive(Clone, Copy)]
enum RoutePolicy<'a> {
    /// Production inference. When `contracts` is present, the effective
    /// contract (static/preset/probe-confirmed + switches) is required.
    /// The forwarder keeps the historical three-argument signature and
    /// still refuses protocols outside the adapter safety ceiling.
    Production {
        contracts: Option<&'a EffectiveContractSet>,
    },
    /// Explicit admin probe: validate the structural ceiling and construct
    /// the endpoint/auth path without requiring prior verified support.
    Probe,
    /// Account-scoped operational test. This keeps the production route shape
    /// (including GOAT and CN routes) while deliberately bypassing normal
    /// availability selection: the dashboard has already locked one account.
    AccountTest,
}

pub(crate) fn supports_production_plan(
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
    contracts: &EffectiveContractSet,
) -> Result<(), String> {
    resolve_route_with_policy(
        account,
        config,
        plan,
        RoutePolicy::Production {
            contracts: Some(contracts),
        },
    )
    .map(|_| ())
}

pub(crate) fn resolve_route(
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
) -> Result<AttemptSpec, String> {
    resolve_route_with_policy(
        account,
        config,
        plan,
        RoutePolicy::Production { contracts: None },
    )
}

pub(crate) fn resolve_probe_route(
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
) -> Result<AttemptSpec, String> {
    resolve_route_with_policy(account, config, plan, RoutePolicy::Probe)
}

pub(crate) fn resolve_account_test_route(
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
) -> Result<AttemptSpec, String> {
    resolve_route_with_policy(account, config, plan, RoutePolicy::AccountTest)
}

fn resolve_route_with_policy(
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
) -> Result<AttemptSpec, String> {
    match ProviderAdapterKind::from_offering(&account.provider_id, &account.offering_id) {
        Some(ProviderAdapterKind::OpenCodeGo) => {
            resolve_open_code_go(account, config, plan, policy)
        }
        Some(ProviderAdapterKind::ZenFree) => resolve_zen_free(account, config, plan, policy),
        Some(ProviderAdapterKind::CommandCodeGoat) => {
            resolve_command_code_goat(account, config, plan, policy)
        }
        Some(ProviderAdapterKind::MiniMaxCn) => resolve_minimax_cn(account, config, plan, policy),
        Some(ProviderAdapterKind::KimiCn) => resolve_kimi_cn(account, config, plan, policy),
        Some(ProviderAdapterKind::ConfigurableHttp) => {
            resolve_configurable_http(account, config, plan, policy)
        }
        Some(ProviderAdapterKind::Cpa) => resolve_cpa(account, config, plan, policy),
        None => Err(format!(
            "unsupported provider offering `{}/{}`",
            account.provider_id, account.offering_id
        )),
    }
}

fn resolve_open_code_go(
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
) -> Result<AttemptSpec, String> {
    let descriptor = registered_descriptor(ProviderAdapterKind::OpenCodeGo, account)?;
    require_binding(
        account,
        descriptor.inference.credential_kind,
        descriptor.inference.quota_scope,
    )?;
    if plan.channel != UpstreamChannel::Go {
        return Err("OpenCode Go does not serve the Zen free channel".to_string());
    }
    require_opencode_protocol_policy(descriptor, account, plan, policy, "OpenCode Go")?;
    Ok(AttemptSpec {
        base_url: config.upstream_base_url.trim_end_matches('/').to_string(),
        path: opencode_upstream_path(plan.upstream)?,
        upstream: plan.upstream,
        auth: descriptor_auth(descriptor.inference.auth)?,
        follow_redirects: descriptor.inference.follow_redirects,
        credential: credential_handle(account, descriptor),
        proxy_routing: ProxyRoutingModel::RequestEntrySnapshot,
    })
}

fn resolve_zen_free(
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
) -> Result<AttemptSpec, String> {
    let descriptor = registered_descriptor(ProviderAdapterKind::ZenFree, account)?;
    require_binding(
        account,
        descriptor.inference.credential_kind,
        descriptor.inference.quota_scope,
    )?;
    if account.id != ZEN_FREE_ACCOUNT_ID {
        return Err("Zen Free route must use the reserved singleton account".to_string());
    }
    if plan.channel != UpstreamChannel::Free {
        return Err(format!(
            "Zen Free does not support routed model `{}` on this channel",
            plan.model
        ));
    }
    require_opencode_protocol_policy(descriptor, account, plan, policy, "Zen Free")?;
    let base_url = plan.upstream_base_override.clone().map_or_else(
        || resolve_upstream_base(UpstreamChannel::Free, &config.upstream_base_url),
        Ok,
    )?;
    Ok(AttemptSpec {
        base_url,
        path: opencode_upstream_path(plan.upstream)?,
        upstream: plan.upstream,
        auth: descriptor_auth(descriptor.inference.auth)?,
        follow_redirects: descriptor.inference.follow_redirects,
        credential: credential_handle(account, descriptor),
        proxy_routing: ProxyRoutingModel::RequestEntrySnapshot,
    })
}

fn resolve_command_code_goat(
    account: &Account,
    _config: &AppConfig,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
) -> Result<AttemptSpec, String> {
    if matches!(policy, RoutePolicy::Probe) {
        return Err(
            "protocol probes are not available for Command Code GOAT in this slice".to_string(),
        );
    }
    let descriptor = registered_descriptor(ProviderAdapterKind::CommandCodeGoat, account)?;
    require_binding(
        account,
        descriptor.inference.credential_kind,
        descriptor.inference.quota_scope,
    )?;
    if plan.channel != UpstreamChannel::Go {
        return Err("Command Code GOAT does not serve the Zen free channel".to_string());
    }
    if !command_code_supports_upstream(&plan.model, plan.upstream) {
        return Err(format!(
            "Command Code GOAT has no verified support for model `{}` over {:?}",
            plan.model, plan.upstream
        ));
    }
    require_opencode_protocol_policy(descriptor, account, plan, policy, "Command Code GOAT")?;
    let path = command_code_upstream_path(plan.upstream).ok_or_else(|| {
        format!(
            "Command Code GOAT has no upstream path for {:?}",
            plan.upstream
        )
    })?;
    let routes = GOAT_LOOPBACK_ROUTES
        .read()
        .map_err(|_| "GOAT loopback route lock is poisoned".to_string())?;
    let base_url = routes.get(&account.id).map_or_else(
        || COMMAND_CODE_GOAT_BASE_URL.to_string(),
        |route| command_code_goat_loopback_base(&route.origin),
    );
    Ok(AttemptSpec {
        base_url,
        path: path.to_string(),
        upstream: plan.upstream,
        auth: descriptor_auth(descriptor.inference.auth)?,
        follow_redirects: descriptor.inference.follow_redirects,
        credential: credential_handle(account, descriptor),
        proxy_routing: ProxyRoutingModel::ProcessWideNoRedirect,
    })
}

fn resolve_fixed_chat_plan(
    account: &Account,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
    adapter: ProviderAdapterKind,
    label: &str,
    base_url: &str,
    path: &str,
) -> Result<AttemptSpec, String> {
    if matches!(policy, RoutePolicy::Probe) {
        return Err(format!("protocol probes are not available for {label}"));
    }
    let descriptor = registered_descriptor(adapter, account)?;
    require_binding(
        account,
        descriptor.inference.credential_kind,
        descriptor.inference.quota_scope,
    )?;
    if plan.channel != UpstreamChannel::Go {
        return Err(format!("{label} does not serve the Zen free channel"));
    }
    if plan.upstream != ApiFormat::ChatCompletions {
        return Err(format!("{label} only accepts Chat Completions upstream"));
    }
    require_opencode_protocol_policy(descriptor, account, plan, policy, label)?;
    Ok(AttemptSpec {
        base_url: base_url.to_string(),
        path: path.to_string(),
        upstream: ApiFormat::ChatCompletions,
        auth: descriptor_auth(descriptor.inference.auth)?,
        follow_redirects: descriptor.inference.follow_redirects,
        credential: credential_handle(account, descriptor),
        proxy_routing: ProxyRoutingModel::ProcessWideNoRedirect,
    })
}

fn resolve_minimax_cn(
    account: &Account,
    _config: &AppConfig,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
) -> Result<AttemptSpec, String> {
    resolve_fixed_chat_plan(
        account,
        plan,
        policy,
        ProviderAdapterKind::MiniMaxCn,
        "MiniMax CN Token Plan",
        MINIMAX_CN_BASE_URL,
        MINIMAX_CN_CHAT_COMPLETIONS_PATH,
    )
}

fn resolve_kimi_cn(
    account: &Account,
    _config: &AppConfig,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
) -> Result<AttemptSpec, String> {
    resolve_fixed_chat_plan(
        account,
        plan,
        policy,
        ProviderAdapterKind::KimiCn,
        "Kimi Code CN",
        KIMI_CN_BASE_URL,
        KIMI_CN_CHAT_COMPLETIONS_PATH,
    )
}

fn resolve_configurable_http(
    account: &Account,
    _config: &AppConfig,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
) -> Result<AttemptSpec, String> {
    let descriptor = registered_descriptor(ProviderAdapterKind::ConfigurableHttp, account)?;
    require_binding(
        account,
        descriptor.inference.credential_kind,
        descriptor.inference.quota_scope,
    )?;
    if plan.channel != UpstreamChannel::Go {
        return Err("Custom API does not serve the Zen free channel".to_string());
    }
    let custom = plan.custom_route.as_ref().ok_or_else(|| {
        "Custom API account is missing a persisted endpoint URL and upstream protocol".to_string()
    })?;
    let protocol = protocol_kind_for(plan.upstream)?;
    if let RoutePolicy::Production {
        contracts: Some(contracts),
    } = policy
        && !contracts.production_protocol_allowed(
            account,
            plan.resolved_alias.as_deref().unwrap_or(&plan.model),
            protocol,
        )
    {
        return Err(format!(
            "Custom API has no verified support for public model `{}` over {:?}",
            plan.resolved_alias.as_deref().unwrap_or(&plan.model),
            plan.upstream
        ));
    }
    let endpoint = resolve_custom_endpoints(&custom.endpoint_url, protocol)
        .map_err(|error| error.to_string())?
        .inference;
    let endpoint_path = endpoint.path().to_string();
    let mut base = endpoint;
    base.set_path("");
    base.set_query(None);
    base.set_fragment(None);
    Ok(AttemptSpec {
        base_url: base.as_str().trim_end_matches('/').to_string(),
        path: endpoint_path,
        upstream: plan.upstream,
        auth: match custom_auth_scheme(protocol) {
            UpstreamAuthScheme::Bearer => UpstreamAuth::Bearer,
            UpstreamAuthScheme::XApiKey => UpstreamAuth::XApiKey,
        },
        follow_redirects: descriptor.inference.follow_redirects,
        credential: credential_handle(account, descriptor),
        proxy_routing: ProxyRoutingModel::IsolatedTrustedAdmin,
    })
}

fn resolve_cpa(
    account: &Account,
    _config: &AppConfig,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
) -> Result<AttemptSpec, String> {
    if matches!(policy, RoutePolicy::Probe | RoutePolicy::AccountTest) {
        return Err("CPA protocol probes and account tests are not available".to_string());
    }
    let descriptor = registered_descriptor(ProviderAdapterKind::Cpa, account)?;
    require_binding(
        account,
        descriptor.inference.credential_kind,
        descriptor.inference.quota_scope,
    )?;
    if account.id != CPA_ACCOUNT_ID {
        return Err("CPA route must use the reserved singleton account".to_string());
    }
    if plan.channel != UpstreamChannel::Go {
        return Err("CPA does not serve the Zen free channel".to_string());
    }
    let path = plan
        .upstream
        .upstream_path()
        .ok_or_else(|| "CPA has no native Gemini inference path".to_string())?;
    let base_url = plan
        .upstream_base_override
        .clone()
        .ok_or_else(|| "CPA is not configured".to_string())?;
    crate::cpa::normalize_base_url(&base_url, true).map_err(|error| error.to_string())?;
    Ok(AttemptSpec {
        base_url,
        path: path.to_string(),
        upstream: plan.upstream,
        auth: UpstreamAuth::Bearer,
        follow_redirects: false,
        credential: credential_handle(account, descriptor),
        proxy_routing: ProxyRoutingModel::LocalExternalIntegration,
    })
}

fn registered_descriptor(
    expected: ProviderAdapterKind,
    account: &Account,
) -> Result<crate::provider::ProviderDescriptor, String> {
    let descriptor =
        ProviderRegistry::get(&account.provider_id, &account.offering_id).ok_or_else(|| {
            format!(
                "unsupported provider offering `{}/{}`",
                account.provider_id, account.offering_id
            )
        })?;
    if descriptor.kind != expected {
        return Err(format!(
            "unsupported provider offering `{}/{}`",
            account.provider_id, account.offering_id
        ));
    }
    Ok(descriptor)
}

fn descriptor_auth(auth: InferenceAuthDescriptor) -> Result<UpstreamAuth, String> {
    match auth {
        InferenceAuthDescriptor::OpenCodeProtocolDefault => {
            Ok(UpstreamAuth::OpenCodeProtocolDefault)
        }
        InferenceAuthDescriptor::Bearer => Ok(UpstreamAuth::Bearer),
        InferenceAuthDescriptor::None => Ok(UpstreamAuth::None),
        InferenceAuthDescriptor::ProtocolDerivedBearerOrXApiKey => {
            Err("Configurable HTTP auth is derived from the account protocol".to_string())
        }
    }
}

fn credential_handle(
    account: &Account,
    descriptor: crate::provider::ProviderDescriptor,
) -> CredentialHandle {
    match descriptor.inference.credential_kind {
        CredentialKind::ApiKey => CredentialHandle::Account {
            id: account.id.clone(),
        },
        CredentialKind::None => CredentialHandle::None,
    }
}

fn require_opencode_protocol_policy(
    descriptor: crate::provider::ProviderDescriptor,
    account: &Account,
    plan: &RequestPlan,
    policy: RoutePolicy<'_>,
    label: &str,
) -> Result<(), String> {
    let protocol = protocol_kind_for(plan.upstream)?;
    let ceiling = crate::provider_contracts::safety_ceiling_protocols(
        descriptor.protocol_probe,
        &plan.model,
        &[],
    );
    let static_verified =
        crate::provider_contracts::static_verified_protocols(descriptor.kind, &plan.model, &[]);
    match policy {
        RoutePolicy::Probe => {
            // Dashboard V3 admits probe requests only for models present in
            // the selected provider's effective catalog. Once admitted, an
            // explicit admin probe must reach every constructible protocol
            // endpoint; the static model table is evidence, not an admission
            // gate for freshly fetched catalog models.
            let _ = (protocol, ceiling, label);
            Ok(())
        }
        RoutePolicy::Production {
            contracts: Some(contracts),
        } => {
            if !contracts.production_protocol_allowed(account, &plan.model, protocol) {
                return Err(format!(
                    "{label} has no verified support for model `{}` over {:?}",
                    plan.model, plan.upstream
                ));
            }
            Ok(())
        }
        RoutePolicy::AccountTest | RoutePolicy::Production { contracts: None } => {
            let statically_ok = static_verified.contains(&protocol)
                || opencode_supports_upstream(&plan.model, plan.upstream)
                || (descriptor.kind == ProviderAdapterKind::CommandCodeGoat
                    && command_code_supports_upstream(&plan.model, plan.upstream))
                || (descriptor.kind == ProviderAdapterKind::ZenFree
                    && !crate::gateway::protocol::is_known_model(&plan.model)
                    && plan.model.ends_with("-free")
                    && plan.upstream == ApiFormat::ChatCompletions);
            // Forwarder has no request-scoped contract. Static MODEL_PROTOCOLS
            // remains the default policy; ceiling-only extras are still
            // constructable so a probe-confirmed plan selected by materialize
            // can be forwarded. Anything outside the ceiling is rejected.
            if statically_ok || ceiling.contains(&protocol) {
                Ok(())
            } else {
                Err(format!(
                    "{label} has no verified support for model `{}` over {:?}",
                    plan.model, plan.upstream
                ))
            }
        }
    }
}

fn protocol_kind_for(upstream: ApiFormat) -> Result<crate::provider::UpstreamProtocolKind, String> {
    match upstream {
        ApiFormat::ChatCompletions => Ok(crate::provider::UpstreamProtocolKind::ChatCompletions),
        ApiFormat::Responses => Ok(crate::provider::UpstreamProtocolKind::Responses),
        ApiFormat::Messages => Ok(crate::provider::UpstreamProtocolKind::Messages),
        ApiFormat::Gemini => Err("Gemini is a client-only protocol".to_string()),
    }
}

fn opencode_upstream_path(upstream: ApiFormat) -> Result<String, String> {
    upstream
        .upstream_path()
        .map(str::to_string)
        .ok_or_else(|| "Gemini is a client-only protocol".to_string())
}

fn require_binding(
    account: &Account,
    credential_kind: CredentialKind,
    quota_scope: QuotaScope,
) -> Result<(), String> {
    if account.credential_kind != credential_kind || account.quota_scope != quota_scope {
        return Err(format!(
            "provider binding mismatch for account `{}`",
            account.id
        ));
    }
    Ok(())
}

fn ensure_loopback_base(base_url: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(base_url).map_err(|error| error.to_string())?;
    if url.scheme() != "http"
        || !matches!(
            url.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
        )
    {
        return Err("GOAT test route must be an HTTP loopback URL".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::gateway::protocol::CustomRouteSpec;
    use crate::models::{Account, AccountSetupStep, AccountType, AppConfig};
    use crate::provider::{
        ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_GOAT_BASE_URL,
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, COMMAND_CODE_PROVIDER_ID,
        CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID,
        MINIMAX_CN_OFFERING_ID, MINIMAX_PROVIDER_ID, OPENCODE_PROVIDER_ID,
        OPENCODE_ZEN_FREE_PROVIDER_ID, ZEN_FREE_ACCOUNT_NAME,
    };
    use bytes::Bytes;
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Arc;

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

    fn chat_plan(
        model: &str,
        channel: UpstreamChannel,
        upstream: ApiFormat,
        custom_route: Option<CustomRouteSpec>,
    ) -> RequestPlan {
        RequestPlan {
            client: ApiFormat::ChatCompletions,
            upstream,
            model: model.into(),
            client_model: model.into(),
            stream: false,
            body: Bytes::from(
                serde_json::to_vec(&json!({
                    "model": model,
                    "messages": [{"role": "user", "content": "hi"}]
                }))
                .unwrap(),
            ),
            channel,
            upstream_base_override: None,
            original_model: None,
            allow_go_fallback: false,
            resolved_alias: Some(model.into()),
            custom_route,
            service_tier: None,
            custom_tools: Vec::new(),
            namespace_tools: Vec::new(),
            response_parallel_tool_calls: true,
            response_tool_choice: json!("auto"),
            response_tools: Vec::new(),
        }
    }

    #[test]
    fn official_transport_is_fixed_bearer_chat_without_redirects_or_zdr() {
        let spec = command_code_goat_transport_spec();
        assert_eq!(spec.base_url, "https://api.commandcode.ai/provider/v1");
        assert_eq!(spec.host, "api.commandcode.ai");
        assert_eq!(spec.chat_completions_path, "/chat/completions");
        assert_eq!(spec.messages_path, "/messages");
        assert_eq!(spec.models_path, "/models");
        assert_eq!(spec.auth_scheme, UpstreamAuthScheme::Bearer);
        assert!(!spec.follow_redirects);
        assert_eq!(spec.zdr_header_name, None);
        assert!(spec.public_catalog_refresh);
        assert_eq!(
            command_code_goat_official_url(ApiFormat::ChatCompletions).unwrap(),
            "https://api.commandcode.ai/provider/v1/chat/completions"
        );
        assert_eq!(
            command_code_goat_official_url(ApiFormat::Messages).unwrap(),
            "https://api.commandcode.ai/provider/v1/messages"
        );
        assert!(command_code_goat_official_url(ApiFormat::Responses).is_err());
        let loopback = command_code_goat_loopback_base("http://127.0.0.1:9");
        assert_eq!(loopback, "http://127.0.0.1:9/provider/v1");
        assert_eq!(
            command_code_goat_join_url(&loopback, ApiFormat::ChatCompletions).unwrap(),
            "http://127.0.0.1:9/provider/v1/chat/completions"
        );
        assert!(crate::provider::is_command_code_goat(
            COMMAND_CODE_PROVIDER_ID,
            GOAT_OFFERING_ID
        ));
        assert_eq!(
            crate::provider::COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
            "deepseek-v4-flash"
        );
        assert_eq!(
            crate::provider::COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            "deepseek/deepseek-v4-flash"
        );
    }

    #[test]
    fn adapter_kind_dispatch_preserves_route_auth_and_model_decisions() {
        let config = AppConfig::default();
        let go = account(
            "go-1",
            OPENCODE_PROVIDER_ID,
            GO_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        );
        let go_route = resolve_route(
            &go,
            &config,
            &chat_plan(
                "glm-5.2",
                UpstreamChannel::Go,
                ApiFormat::ChatCompletions,
                None,
            ),
        )
        .unwrap();
        assert_eq!(go_route.auth, UpstreamAuth::OpenCodeProtocolDefault);
        assert!(go_route.follow_redirects);
        assert_eq!(go_route.path, "/v1/chat/completions");
        assert_eq!(
            go_route.credential,
            CredentialHandle::Account { id: "go-1".into() }
        );
        assert_eq!(
            go_route.proxy_routing,
            ProxyRoutingModel::RequestEntrySnapshot
        );
        assert!(go_route.restricted_upstream_url());
        assert!(!go_route.isolates_client_headers());
        assert_eq!(go_route.wire_auth(), UpstreamAuth::Bearer);
        assert!(
            resolve_route(
                &go,
                &config,
                &chat_plan(
                    "glm-5.2",
                    UpstreamChannel::Free,
                    ApiFormat::ChatCompletions,
                    None
                ),
            )
            .unwrap_err()
            .contains("does not serve the Zen free channel")
        );

        let mut zen = account(
            ZEN_FREE_ACCOUNT_ID,
            OPENCODE_ZEN_FREE_PROVIDER_ID,
            ANONYMOUS_FREE_OFFERING_ID,
            CredentialKind::None,
            QuotaScope::EgressIp,
        );
        zen.name = ZEN_FREE_ACCOUNT_NAME.into();
        let zen_route = resolve_route(
            &zen,
            &config,
            &chat_plan(
                "mimo-v2.5-free",
                UpstreamChannel::Free,
                ApiFormat::ChatCompletions,
                None,
            ),
        )
        .unwrap();
        assert_eq!(zen_route.auth, UpstreamAuth::None);
        assert!(zen_route.follow_redirects);
        assert_eq!(zen_route.credential, CredentialHandle::None);
        assert_eq!(
            zen_route.proxy_routing,
            ProxyRoutingModel::RequestEntrySnapshot
        );
        assert!(zen_route.credential_account_id().is_none());

        let goat = account(
            "goat-1",
            COMMAND_CODE_PROVIDER_ID,
            GOAT_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        );
        let official = resolve_route(
            &goat,
            &config,
            &chat_plan(
                COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
                UpstreamChannel::Go,
                ApiFormat::ChatCompletions,
                None,
            ),
        )
        .unwrap();
        assert_eq!(official.base_url, COMMAND_CODE_GOAT_BASE_URL);
        assert_eq!(official.path, "/chat/completions");
        assert_eq!(official.auth, UpstreamAuth::Bearer);
        assert!(!official.follow_redirects);
        assert_eq!(
            official.proxy_routing,
            ProxyRoutingModel::ProcessWideNoRedirect
        );
        let claude = resolve_route(
            &goat,
            &config,
            &chat_plan(
                "claude-sonnet-4-6",
                UpstreamChannel::Go,
                ApiFormat::Messages,
                None,
            ),
        )
        .unwrap();
        assert_eq!(claude.path, "/messages");
        let _guard =
            install_goat_loopback_route_for_test(goat.id.clone(), "http://127.0.0.1:9").unwrap();
        let goat_route = resolve_route(
            &goat,
            &config,
            &chat_plan(
                COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
                UpstreamChannel::Go,
                ApiFormat::ChatCompletions,
                None,
            ),
        )
        .unwrap();
        assert_eq!(goat_route.auth, UpstreamAuth::Bearer);
        assert!(!goat_route.follow_redirects);
        assert_eq!(goat_route.base_url, "http://127.0.0.1:9/provider/v1");
        assert_eq!(goat_route.path, "/chat/completions");
        assert_eq!(
            goat_route.proxy_routing,
            ProxyRoutingModel::ProcessWideNoRedirect
        );
        assert!(goat_route.restricted_upstream_url());

        let custom = account(
            "custom-1",
            CUSTOM_PROVIDER_ID,
            CUSTOM_API_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        );
        let custom_missing = resolve_route(
            &custom,
            &config,
            &chat_plan(
                "local-model",
                UpstreamChannel::Go,
                ApiFormat::ChatCompletions,
                None,
            ),
        )
        .unwrap_err();
        assert!(custom_missing.contains("missing a persisted endpoint URL"));
        let custom_route = resolve_route(
            &custom,
            &config,
            &chat_plan(
                "local-model",
                UpstreamChannel::Go,
                ApiFormat::Messages,
                Some(CustomRouteSpec {
                    endpoint_url: "http://127.0.0.1:9/v1/messages".into(),
                }),
            ),
        )
        .unwrap();
        assert_eq!(custom_route.auth, UpstreamAuth::XApiKey);
        assert!(!custom_route.follow_redirects);
        assert_eq!(custom_route.base_url, "http://127.0.0.1:9");
        assert_eq!(custom_route.path, "/v1/messages");
        assert_eq!(
            custom_route.proxy_routing,
            ProxyRoutingModel::IsolatedTrustedAdmin
        );
        assert!(custom_route.isolates_client_headers());
        assert!(!custom_route.restricted_upstream_url());
        assert_eq!(
            custom_route.credential,
            CredentialHandle::Account {
                id: "custom-1".into()
            }
        );

        for endpoint_url in ["http://127.0.0.1:9", "http://127.0.0.1:9/v1"] {
            let resolved = resolve_route(
                &custom,
                &config,
                &chat_plan(
                    "local-model",
                    UpstreamChannel::Go,
                    ApiFormat::ChatCompletions,
                    Some(CustomRouteSpec {
                        endpoint_url: endpoint_url.into(),
                    }),
                ),
            )
            .unwrap();
            assert_eq!(resolved.base_url, "http://127.0.0.1:9");
            assert_eq!(resolved.path, "/v1/chat/completions");
        }

        let unknown = account(
            "unknown-1",
            "unknown",
            "unknown",
            CredentialKind::ApiKey,
            QuotaScope::Key,
        );
        assert_eq!(
            resolve_route(
                &unknown,
                &config,
                &chat_plan(
                    "glm-5.2",
                    UpstreamChannel::Go,
                    ApiFormat::ChatCompletions,
                    None
                ),
            )
            .unwrap_err(),
            "unsupported provider offering `unknown/unknown`"
        );
    }

    #[test]
    fn resolve_uses_the_same_sealed_descriptors() {
        let go = ProviderRegistry::get(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        let zen = ProviderRegistry::get(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID)
            .unwrap();
        let goat = ProviderRegistry::get(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        let custom = ProviderRegistry::get(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).unwrap();
        assert!(go.inference.production_inference);
        assert_eq!(
            zen.inference.channel,
            Some(crate::provider::InferenceChannelKind::Free)
        );
        assert!(goat.inference.production_inference);
        assert!(!goat.inference.loopback_test_seam_only);
        assert_eq!(
            custom.inference.auth,
            InferenceAuthDescriptor::ProtocolDerivedBearerOrXApiKey
        );
    }

    #[test]
    fn adapter_kind_match_is_exhaustive_and_consistent_with_descriptors() {
        for kind in ProviderAdapterKind::ALL {
            match kind {
                ProviderAdapterKind::OpenCodeGo
                | ProviderAdapterKind::ZenFree
                | ProviderAdapterKind::CommandCodeGoat
                | ProviderAdapterKind::MiniMaxCn
                | ProviderAdapterKind::KimiCn
                | ProviderAdapterKind::Cpa
                | ProviderAdapterKind::ConfigurableHttp => {}
            }
            let descriptor = ProviderRegistry::iter()
                .find(|entry| entry.kind == kind)
                .expect("each adapter kind has a registry descriptor");
            match kind {
                ProviderAdapterKind::OpenCodeGo => {
                    assert_eq!(
                        descriptor.inference.auth,
                        InferenceAuthDescriptor::OpenCodeProtocolDefault
                    );
                    assert!(descriptor.inference.follow_redirects);
                }
                ProviderAdapterKind::ZenFree => {
                    assert_eq!(descriptor.inference.auth, InferenceAuthDescriptor::None);
                    assert!(descriptor.inference.follow_redirects);
                }
                ProviderAdapterKind::CommandCodeGoat => {
                    assert_eq!(descriptor.inference.auth, InferenceAuthDescriptor::Bearer);
                    assert!(!descriptor.inference.follow_redirects);
                    assert!(descriptor.inference.production_inference);
                    assert!(!descriptor.inference.loopback_test_seam_only);
                }
                ProviderAdapterKind::MiniMaxCn | ProviderAdapterKind::KimiCn => {
                    assert_eq!(descriptor.inference.auth, InferenceAuthDescriptor::Bearer);
                    assert!(!descriptor.inference.follow_redirects);
                }
                ProviderAdapterKind::Cpa => {
                    assert_eq!(descriptor.inference.auth, InferenceAuthDescriptor::Bearer);
                    assert!(!descriptor.inference.follow_redirects);
                }
                ProviderAdapterKind::ConfigurableHttp => {
                    assert_eq!(
                        descriptor.inference.auth,
                        InferenceAuthDescriptor::ProtocolDerivedBearerOrXApiKey
                    );
                    assert!(!descriptor.inference.follow_redirects);
                }
            }
        }
    }

    #[test]
    fn probe_route_allows_ceiling_without_static_support_production_requires_contract() {
        let config = AppConfig::default();
        let go = account(
            "go-1",
            OPENCODE_PROVIDER_ID,
            GO_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        );
        let chat_grok = chat_plan(
            "grok-4.5",
            UpstreamChannel::Go,
            ApiFormat::ChatCompletions,
            None,
        );
        let probe = resolve_probe_route(&go, &config, &chat_grok).unwrap();
        assert_eq!(probe.path, "/v1/chat/completions");
        assert_eq!(probe.upstream, ApiFormat::ChatCompletions);
        let fetched_model_probe = resolve_probe_route(
            &go,
            &config,
            &chat_plan(
                "future-go-model",
                UpstreamChannel::Go,
                ApiFormat::Messages,
                None,
            ),
        )
        .expect("the Dashboard catalog gate admits fetched models before route construction");
        assert_eq!(fetched_model_probe.path, "/v1/messages");

        let static_contracts = crate::provider_contracts::build_effective_contracts(
            &crate::zen_models::ZenFreeModelCatalog::default(),
            &[],
            crate::provider_contracts::PersistedContracts::default(),
        );
        assert!(
            supports_production_plan(&go, &config, &chat_grok, &static_contracts).is_err(),
            "static grok-4.5 Chat must stay unverified until a probe succeeds"
        );
        assert!(
            supports_production_plan(
                &go,
                &config,
                &chat_plan("grok-4.5", UpstreamChannel::Go, ApiFormat::Responses, None),
                &static_contracts
            )
            .is_ok()
        );

        let now = Utc::now();
        let mut persisted = crate::provider_contracts::PersistedContracts::default();
        let scope = crate::provider_contracts::ContractScope::provider(OPENCODE_PROVIDER_ID);
        persisted.evidence.insert(
            scope.clone(),
            vec![crate::provider_contracts::PersistedModelProtocol {
                scope,
                model_id: "grok-4.5".into(),
                protocol: crate::provider::UpstreamProtocolKind::ChatCompletions,
                source: crate::provider_contracts::ContractEvidenceSource::ProbeConfirmed,
                verified_at: Some(now),
                observed_at: Some(now),
                last_probe_result: Some(crate::provider_contracts::ProbeResultKind::Success),
                last_probe_at: Some(now),
                last_probe_error: None,
            }],
        );
        let probed = crate::provider_contracts::build_effective_contracts(
            &crate::zen_models::ZenFreeModelCatalog::default(),
            &[],
            persisted,
        );
        assert!(supports_production_plan(&go, &config, &chat_grok, &probed).is_ok());

        let goat = account(
            "goat-1",
            COMMAND_CODE_PROVIDER_ID,
            GOAT_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        );
        assert!(
            resolve_probe_route(
                &goat,
                &config,
                &chat_plan(
                    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
                    UpstreamChannel::Go,
                    ApiFormat::ChatCompletions,
                    None,
                ),
            )
            .unwrap_err()
            .contains("not available")
        );
    }

    #[test]
    fn account_test_route_keeps_fixed_chat_plan_available_without_enabling_provider_probes() {
        let config = AppConfig::default();
        let minimax = account(
            "minimax-test",
            MINIMAX_PROVIDER_ID,
            MINIMAX_CN_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        );
        let route = resolve_account_test_route(
            &minimax,
            &config,
            &chat_plan(
                "MiniMax-M3",
                UpstreamChannel::Go,
                ApiFormat::ChatCompletions,
                None,
            ),
        )
        .expect("account-level tests use the fixed production Chat route");
        assert_eq!(route.base_url, MINIMAX_CN_BASE_URL);
        assert_eq!(route.path, MINIMAX_CN_CHAT_COMPLETIONS_PATH);
    }

    #[test]
    fn adapter_production_source_has_no_host_transport_or_plaintext_clients() {
        let source = include_str!("provider_adapter.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(
            !production.contains("CoreState"),
            "adapters must not name CoreState"
        );
        assert!(
            !production.contains("Database"),
            "adapters must not name Database"
        );
        assert!(
            !production.contains("reqwest::Client"),
            "adapters must not name reqwest::Client"
        );
    }
}
