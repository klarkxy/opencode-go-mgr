use super::*;
use crate::crypto::{KeyCipher, StaticKeyCipher};
use crate::gateway::protocol::CustomRouteSpec;
use crate::models::{Account, AccountSetupStep, AccountType, AppConfig};
use crate::provider::{
    ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_GOAT_BASE_URL,
    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, COMMAND_CODE_PROVIDER_ID, CUSTOM_API_OFFERING_ID,
    CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID, MINIMAX_CN_OFFERING_ID,
    MINIMAX_PROVIDER_ID, OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID,
    ZEN_FREE_ACCOUNT_NAME,
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
    let zen =
        ProviderRegistry::get(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID).unwrap();
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
