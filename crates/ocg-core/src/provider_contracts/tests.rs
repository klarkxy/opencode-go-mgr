use super::*;
use crate::custom::CustomAccountRuntime;
use crate::kernel::ids::{
    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, KIMI_PROVIDER_ID, MINIMAX_PROVIDER_ID,
};
use crate::models::{AccountCustomConfig, AccountModelCapability};
use crate::provider::ConnectionVerificationStatus;

fn empty_persisted() -> PersistedContracts {
    PersistedContracts::default()
}

fn zen_seed() -> ZenFreeModelCatalog {
    ZenFreeModelCatalog::default()
}

fn go_contract() -> EffectiveScopeContract {
    build_effective_contracts(&zen_seed(), &[], empty_persisted())
        .providers
        .remove(OPENCODE_PROVIDER_ID)
        .unwrap()
}

fn probe_for(provider_id: &str) -> ProtocolProbeDescriptor {
    ProviderRegistry::get(provider_id)
        .expect("test provider scope")
        .protocol_probe
}

#[test]
fn custom_endpoints_are_isolated_by_account() {
    let left = ContractScope::from_provider_id(CUSTOM_PROVIDER_ID, Some("one"));
    let right = ContractScope::from_provider_id(CUSTOM_PROVIDER_ID, Some("two"));
    assert_ne!(left, right);
    assert!(matches!(left, Some(ContractScope::CustomEndpoint(id)) if id == "one"));
}

#[test]
fn provider_scopes_identify_one_exact_registered_offering() {
    let set = build_effective_contracts(&zen_seed(), &[], empty_persisted());
    assert_eq!(set.providers.len(), builtin_provider_scope_ids().len());
    for (scope_id, contract) in &set.providers {
        let descriptor = provider_scope_descriptor(scope_id)
            .expect("effective provider scope must identify a registered offering");
        assert_eq!(descriptor.kind, contract.adapter_kind);
        assert_eq!(descriptor.provider_id, contract.provider_id);
        assert_eq!(descriptor.provider_id, contract.provider_id);
    }
    assert!(ContractScope::parse("provider", "unknown-scope").is_err());
}

#[test]
fn probe_success_adds_inside_ceiling_and_failure_does_not_remove_static() {
    let now = Utc::now();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    let static_row = PersistedModelProtocol {
        scope: scope.clone(),
        model_id: "glm-5.2".into(),
        protocol: UpstreamProtocolKind::ChatCompletions,
        source: ContractEvidenceSource::Static,
        verified_at: None,
        observed_at: None,
        last_probe_result: None,
        last_probe_at: None,
        last_probe_error: None,
    };
    let failed = apply_probe_observation(
        Some(&static_row),
        scope.clone(),
        "glm-5.2",
        UpstreamProtocolKind::ChatCompletions,
        false,
        Some("upstream 500".into()),
        now,
        true,
    )
    .unwrap();
    assert_eq!(failed.source, ContractEvidenceSource::Static);
    assert!(failed.source.confers_support());
    assert_eq!(failed.last_probe_result, Some(ProbeResultKind::Failure));

    let added = apply_probe_observation(
        None,
        scope,
        "glm-5.2",
        UpstreamProtocolKind::Messages,
        true,
        None,
        now,
        true,
    )
    .unwrap();
    assert_eq!(added.source, ContractEvidenceSource::ProbeConfirmed);

    let rejected = apply_probe_observation(
        None,
        ContractScope::provider(OPENCODE_PROVIDER_ID),
        "not-a-catalog-model",
        UpstreamProtocolKind::ChatCompletions,
        true,
        None,
        now,
        false,
    );
    assert!(rejected.is_err());
}

#[test]
fn opencode_ceiling_is_constructable_paths_not_static_model_protocols() {
    let grok_ceiling = safety_ceiling_protocols(probe_for(OPENCODE_PROVIDER_ID), "grok-4.5");
    let grok_static = static_verified_protocols(ProviderAdapterKind::OpenCodeGo, "grok-4.5", &[]);
    assert!(grok_ceiling.contains(&UpstreamProtocolKind::ChatCompletions));
    assert!(grok_ceiling.contains(&UpstreamProtocolKind::Responses));
    assert!(grok_ceiling.contains(&UpstreamProtocolKind::Messages));
    assert_eq!(grok_static, vec![UpstreamProtocolKind::Responses]);
    assert!(probe_may_add(
        probe_for(OPENCODE_PROVIDER_ID),
        "grok-4.5",
        UpstreamProtocolKind::ChatCompletions,
    ));

    let unknown_zen = safety_ceiling_protocols(
        probe_for(OPENCODE_ZEN_FREE_PROVIDER_ID),
        "brand-new-promo-free",
    );
    assert_eq!(unknown_zen, vec![UpstreamProtocolKind::ChatCompletions]);
    assert!(probe_may_add(
        probe_for(COMMAND_CODE_PROVIDER_ID),
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
        UpstreamProtocolKind::ChatCompletions,
    ));
    assert!(!probe_may_add(
        probe_for(COMMAND_CODE_PROVIDER_ID),
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
        UpstreamProtocolKind::Messages,
    ));
    assert!(probe_may_add(
        probe_for(COMMAND_CODE_PROVIDER_ID),
        "claude-sonnet-5",
        UpstreamProtocolKind::Messages,
    ));
    for provider_id in [MINIMAX_PROVIDER_ID, KIMI_PROVIDER_ID] {
        assert!(probe_may_add(
            probe_for(provider_id),
            "new-catalog-model",
            UpstreamProtocolKind::ChatCompletions,
        ));
        assert!(probe_may_add(
            probe_for(provider_id),
            "new-catalog-model",
            UpstreamProtocolKind::Messages,
        ));
        assert!(!probe_may_add(
            probe_for(provider_id),
            "new-catalog-model",
            UpstreamProtocolKind::Responses,
        ));
    }
}

#[test]
fn probe_confirmed_opencode_extra_protocol_becomes_effective() {
    let mut persisted = empty_persisted();
    let now = Utc::now();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    persisted.evidence.insert(
        scope.clone(),
        vec![PersistedModelProtocol {
            scope,
            model_id: "grok-4.5".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: ContractEvidenceSource::ProbeConfirmed,
            verified_at: Some(now),
            observed_at: Some(now),
            last_probe_result: Some(ProbeResultKind::Success),
            last_probe_at: Some(now),
            last_probe_error: None,
        }],
    );
    let go = build_effective_contracts(&zen_seed(), &[], persisted)
        .providers
        .remove(OPENCODE_PROVIDER_ID)
        .unwrap();
    let grok = go.model("grok-4.5").unwrap();
    assert!(grok.protocols.get("chat_completions").unwrap().available);
    assert!(grok.protocols.get("chat_completions").unwrap().enabled);
    assert!(grok.protocols.get("responses").unwrap().available);
    assert_eq!(
        grok.protocols.get("chat_completions").unwrap().source,
        ContractEvidenceSource::ProbeConfirmed
    );
}

#[test]
fn probe_failure_does_not_add_or_remove_static_support() {
    let mut persisted = empty_persisted();
    let now = Utc::now();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    persisted.evidence.insert(
        scope.clone(),
        vec![PersistedModelProtocol {
            scope,
            model_id: "grok-4.5".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: ContractEvidenceSource::ProbeObserved,
            verified_at: None,
            observed_at: Some(now),
            last_probe_result: Some(ProbeResultKind::Failure),
            last_probe_at: Some(now),
            last_probe_error: Some("upstream 500".into()),
        }],
    );
    let go = build_effective_contracts(&zen_seed(), &[], persisted)
        .providers
        .remove(OPENCODE_PROVIDER_ID)
        .unwrap();
    let grok = go.model("grok-4.5").unwrap();
    assert!(!grok.protocols.get("chat_completions").unwrap().available);
    assert!(grok.protocols.get("responses").unwrap().available);
    assert!(grok.routable);
}

#[test]
fn override_force_off_disables_without_destroying_evidence() {
    let mut persisted = empty_persisted();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    persisted.overrides.insert(
        scope.clone(),
        vec![PersistedModelProtocolOverride {
            scope: scope.clone(),
            model_id: "glm-5.3".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            state: ProtocolOverrideState::ForceOff,
            updated_at: Utc::now(),
        }],
    );
    let set = build_effective_contracts(&zen_seed(), &[], persisted);
    let go = set.providers.get(OPENCODE_PROVIDER_ID).unwrap();
    let glm = go.model("glm-5.3").unwrap();
    let chat = glm.protocols.get("chat_completions").unwrap();
    assert!(chat.available);
    assert!(!chat.enabled);
    assert_eq!(chat.r#override, ProtocolOverrideState::ForceOff);
    assert!(!glm.routable);

    let grok = go.model("grok-4.5").unwrap();
    assert!(grok.routable);
    assert!(grok.protocols.get("responses").unwrap().enabled);
}

#[test]
fn override_force_on_enables_supported_protocol_without_evidence() {
    let mut persisted = empty_persisted();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    persisted.overrides.insert(
        scope.clone(),
        vec![PersistedModelProtocolOverride {
            scope: scope.clone(),
            model_id: "grok-4.5".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            state: ProtocolOverrideState::ForceOn,
            updated_at: Utc::now(),
        }],
    );
    let set = build_effective_contracts(&zen_seed(), &[], persisted);
    let go = set.providers.get(OPENCODE_PROVIDER_ID).unwrap();
    let grok = go.model("grok-4.5").unwrap();
    let chat = grok.protocols.get("chat_completions").unwrap();
    assert!(chat.available);
    assert!(chat.enabled);
    assert_eq!(chat.r#override, ProtocolOverrideState::ForceOn);
    assert!(grok.routable);
}

#[test]
fn override_force_on_enables_protocol_beyond_static_and_ceiling() {
    let mut persisted = empty_persisted();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    let now = Utc::now();
    // A refreshed Go catalog can carry models the static table does not
    // know; those sit outside the safety ceiling for every protocol.
    persisted.scopes.insert(
        scope.clone(),
        PersistedScopeRow {
            scope: scope.clone(),
            catalog_models: vec!["future-go-model".into()],
            catalog_refreshed_at: Some(now),
            catalog_source: CATALOG_SOURCE_OPENCODE_MODELS.into(),
            catalog_source_url: "https://opencode.ai/zen/go/v1/models".into(),
            revision: 1,
            updated_at: now,
        },
    );
    persisted.overrides.insert(
        scope.clone(),
        vec![PersistedModelProtocolOverride {
            scope,
            model_id: "future-go-model".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            state: ProtocolOverrideState::ForceOn,
            updated_at: now,
        }],
    );
    let set = build_effective_contracts(&zen_seed(), &[], persisted);
    let go = set.providers.get(OPENCODE_PROVIDER_ID).unwrap();
    let model = go
        .model("future-go-model")
        .expect("catalog model is present");
    let chat = model.protocols.get("chat_completions").unwrap();
    assert!(chat.available, "force_on wins beyond static/ceiling");
    assert!(chat.enabled);
    assert_eq!(chat.r#override, ProtocolOverrideState::ForceOn);
    assert!(model.routable);
}

#[test]
fn refreshed_catalog_is_authoritative_and_new_models_can_start_fully_off() {
    let mut persisted = empty_persisted();
    let now = Utc::now();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    persisted.scopes.insert(
        scope.clone(),
        PersistedScopeRow {
            scope: scope.clone(),
            catalog_models: vec!["future-go-model".into()],
            catalog_refreshed_at: Some(now),
            catalog_source: CATALOG_SOURCE_OPENCODE_MODELS.into(),
            catalog_source_url: "https://opencode.ai/zen/go/v1/models".into(),
            revision: 2,
            updated_at: now,
        },
    );
    persisted.evidence.insert(
        scope.clone(),
        vec![PersistedModelProtocol {
            scope: scope.clone(),
            model_id: "grok-4.5".into(),
            protocol: UpstreamProtocolKind::Responses,
            source: ContractEvidenceSource::ProbeConfirmed,
            verified_at: Some(now),
            observed_at: Some(now),
            last_probe_result: Some(ProbeResultKind::Success),
            last_probe_at: Some(now),
            last_probe_error: None,
        }],
    );
    persisted.overrides.insert(
        scope.clone(),
        [
            UpstreamProtocolKind::ChatCompletions,
            UpstreamProtocolKind::Responses,
            UpstreamProtocolKind::Messages,
        ]
        .into_iter()
        .map(|protocol| PersistedModelProtocolOverride {
            scope: scope.clone(),
            model_id: "future-go-model".into(),
            protocol,
            state: ProtocolOverrideState::ForceOff,
            updated_at: now,
        })
        .collect(),
    );

    let set = build_effective_contracts(&zen_seed(), &[], persisted);
    let go = set.providers.get(OPENCODE_PROVIDER_ID).unwrap();
    assert_eq!(go.catalog.source, CATALOG_SOURCE_OPENCODE_MODELS);
    assert_eq!(go.catalog.models, vec!["future-go-model"]);
    assert!(
        !go.models.contains_key("grok-4.5"),
        "models removed by the official catalog must not be restored by stale probe evidence"
    );
    let future = go.model("future-go-model").unwrap();
    assert!(!future.routable);
    assert!(future.protocols.values().all(|protocol| {
        !protocol.enabled && protocol.r#override == ProtocolOverrideState::ForceOff
    }));
}

#[test]
fn stale_probe_failure_does_not_demote_static_support() {
    let mut persisted = empty_persisted();
    let now = Utc::now();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    persisted.evidence.insert(
        scope.clone(),
        vec![PersistedModelProtocol {
            scope,
            model_id: "glm-5.3".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: ContractEvidenceSource::ProbeObserved,
            verified_at: None,
            observed_at: Some(now),
            last_probe_result: Some(ProbeResultKind::Failure),
            last_probe_at: Some(now),
            last_probe_error: Some("upstream 500".into()),
        }],
    );
    let go = build_effective_contracts(&zen_seed(), &[], persisted)
        .providers
        .remove(OPENCODE_PROVIDER_ID)
        .unwrap();
    let glm = go.model("glm-5.3").unwrap();
    let chat = glm.protocols.get("chat_completions").unwrap();
    assert!(
        chat.available,
        "static support survives a stale probe-failure observation"
    );
    assert!(chat.enabled);
    assert_eq!(chat.r#override, ProtocolOverrideState::Auto);
    assert_eq!(chat.last_probe_result, Some(ProbeResultKind::Failure));
    assert_eq!(
        chat.last_probe_error.as_deref(),
        Some("upstream 500"),
        "failure detail stays visible as evidence"
    );
}

#[test]
fn protocol_fallback_prefers_client_then_adapter_priority() {
    let mut go = go_contract();
    let glm = go.models.get_mut("glm-5.2").unwrap();
    glm.protocols.get_mut("chat_completions").unwrap().enabled = false;
    glm.protocols.get_mut("responses").unwrap().enabled = true;
    glm.protocols.get_mut("messages").unwrap().enabled = true;
    glm.routable = true;

    let selected = select_upstream_protocol(&go, ApiFormat::Messages, "glm-5.2").unwrap();
    assert_eq!(selected, ApiFormat::Messages);

    let selected = select_upstream_protocol(&go, ApiFormat::Gemini, "glm-5.2").unwrap();
    assert_eq!(selected, ApiFormat::Responses);
}

#[test]
fn no_valid_protocol_fails_locally() {
    let mut go = go_contract();
    for model in go.models.values_mut() {
        for evidence in model.protocols.values_mut() {
            evidence.enabled = false;
        }
        model.routable = false;
    }
    let error = select_upstream_protocol(&go, ApiFormat::ChatCompletions, "glm-5.3").unwrap_err();
    assert_eq!(error.message, NO_ENABLED_UPSTREAM_PROTOCOL);
}

#[test]
fn goat_is_production_routable_after_probe_success() {
    let now = Utc::now();
    let mut persisted = empty_persisted();
    let goat_scope = ContractScope::provider(COMMAND_CODE_PROVIDER_ID);
    persisted.scopes.insert(
        goat_scope.clone(),
        PersistedScopeRow {
            scope: goat_scope.clone(),
            catalog_models: vec![COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into()],
            catalog_refreshed_at: Some(now),
            catalog_source: CATALOG_SOURCE_COMMAND_CODE_MODELS.into(),
            catalog_source_url: COMMAND_CODE_GOAT_BASE_URL.into(),
            revision: 1,
            updated_at: now,
        },
    );
    persisted.evidence.insert(
        goat_scope,
        vec![PersistedModelProtocol {
            scope: ContractScope::provider(COMMAND_CODE_PROVIDER_ID),
            model_id: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: ContractEvidenceSource::ProbeConfirmed,
            verified_at: Some(now),
            observed_at: Some(now),
            last_probe_result: Some(ProbeResultKind::Success),
            last_probe_at: Some(now),
            last_probe_error: None,
        }],
    );
    let set = build_effective_contracts(&zen_seed(), &[], persisted);
    let goat = set.providers.get(COMMAND_CODE_PROVIDER_ID).unwrap();
    assert!(goat.catalog_routable);
    assert!(goat.production_inference);
    assert!(
        goat.model(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM)
            .unwrap()
            .routable
    );
    assert!(
        ProviderRegistry::get(COMMAND_CODE_PROVIDER_ID)
            .unwrap()
            .card_actions
            .protocol_probe
    );
}

#[test]
fn custom_discovery_does_not_become_routable_without_declaration() {
    let runtime = CustomAccountRuntime {
        account_id: "custom-1".into(),
        enabled: true,
        verification_status: ConnectionVerificationStatus::Verified,
        setup_ready: true,
        has_key: true,
        config: AccountCustomConfig {
            account_id: "custom-1".into(),
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        capabilities: vec![AccountModelCapability {
            account_id: "custom-1".into(),
            public_model: "declared-model".into(),
            upstream_model: "declared-upstream".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            verified_at: None,
            source: "manual".into(),
        }],
    };
    let mut persisted = empty_persisted();
    let scope = ContractScope::custom_endpoint("custom-1");
    persisted.scopes.insert(
        scope.clone(),
        PersistedScopeRow {
            scope: scope.clone(),
            catalog_models: vec!["discovered-only".into()],
            catalog_refreshed_at: Some(Utc::now()),
            catalog_source: CATALOG_SOURCE_CUSTOM_DISCOVERY.into(),
            catalog_source_url: String::new(),
            revision: 1,
            updated_at: Utc::now(),
        },
    );
    let set = build_effective_contracts(&zen_seed(), &[runtime], persisted);
    let custom = set.custom_endpoints.get("custom-1").unwrap();
    assert_eq!(custom.catalog.source, CATALOG_SOURCE_DECLARED);
    assert_eq!(custom.catalog.models, vec!["declared-model"]);
    assert!(custom.model("declared-model").unwrap().routable);
    assert!(custom.model("discovered-only").is_none());
}

#[test]
fn custom_declared_protocol_is_preferred_and_other_clients_fall_back_to_it() {
    let declared = vec![("declared-model".to_string(), UpstreamProtocolKind::Messages)];
    let runtime = CustomAccountRuntime {
        account_id: "custom-single".into(),
        enabled: true,
        verification_status: ConnectionVerificationStatus::Verified,
        setup_ready: true,
        has_key: true,
        config: AccountCustomConfig {
            account_id: "custom-single".into(),
            endpoint_url: "https://api.example.com/v1/messages".into(),
            upstream_protocol: UpstreamProtocolKind::Messages,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        capabilities: declared
            .iter()
            .map(|(model_id, protocol)| AccountModelCapability {
                account_id: "custom-single".into(),
                public_model: model_id.clone(),
                upstream_model: model_id.clone(),
                protocol: *protocol,
                verified_at: None,
                source: "manual".into(),
            })
            .collect(),
    };
    let ceiling = safety_ceiling_protocols(probe_for(CUSTOM_PROVIDER_ID), "declared-model");
    assert!(
        ceiling.is_empty(),
        "Custom uses exact-account model tests, not Provider probes"
    );

    let set = build_effective_contracts(
        &zen_seed(),
        std::slice::from_ref(&runtime),
        empty_persisted(),
    );
    let custom = set.custom_endpoints.get("custom-single").unwrap();
    let model = custom.model("declared-model").unwrap();
    assert!(model.routable);
    assert_eq!(
        model.preferred_protocol,
        UpstreamProtocolKind::Messages,
        "the account's only declared protocol is always preferred"
    );
    let scope = ContractScope::custom_endpoint("custom-single");
    let selected = set
        .select_upstream(&scope, ApiFormat::Messages, "declared-model")
        .unwrap();
    assert_eq!(selected, ApiFormat::Messages);
    let selected = set
        .select_upstream(&scope, ApiFormat::ChatCompletions, "declared-model")
        .unwrap();
    assert_eq!(selected, ApiFormat::Messages);
    let selected = set
        .select_upstream(&scope, ApiFormat::Responses, "declared-model")
        .unwrap();
    assert_eq!(
        selected,
        ApiFormat::Messages,
        "an undeclared client protocol falls back to the preferred protocol"
    );

    let mut persisted = empty_persisted();
    persisted.overrides.insert(
        scope,
        vec![PersistedModelProtocolOverride {
            scope: ContractScope::custom_endpoint("custom-single"),
            model_id: "declared-model".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            state: ProtocolOverrideState::ForceOn,
            updated_at: Utc::now(),
        }],
    );
    let set = build_effective_contracts(&zen_seed(), &[runtime], persisted);
    let chat = &set.custom_endpoints["custom-single"]
        .model("declared-model")
        .unwrap()
        .protocols["chat_completions"];
    assert!(!chat.available);
    assert!(
        !chat.enabled,
        "force_on cannot enable an undeclared Custom protocol"
    );
}

#[test]
fn sanitize_probe_error_strips_userinfo_and_truncates() {
    let raw = format!(
        "failed https://user:secret@api.example.com/v1 {}",
        "x".repeat(600)
    );
    let sanitized = sanitize_probe_error(&raw, Some("secret"));
    assert!(!sanitized.contains("user:secret"));
    assert!(!sanitized.contains("secret"));
    assert!(sanitized.chars().count() <= MAX_PROBE_ERROR_CHARS + 1);
}

#[test]
fn official_protocol_baselines_cover_every_builtin_provider_shape() {
    assert_eq!(
        static_protocol_snapshot_date(OPENCODE_PROVIDER_ID),
        Some("2026-09-01")
    );
    assert_eq!(
        static_protocol_snapshot_date(MINIMAX_PROVIDER_ID),
        Some("2026-09-01")
    );
    assert_eq!(
        static_verified_protocols(ProviderAdapterKind::OpenCodeGo, "deepseek-v4-flash", &[],),
        vec![UpstreamProtocolKind::ChatCompletions]
    );
    assert_eq!(
        static_verified_protocols(ProviderAdapterKind::OpenCodeGo, "grok-4.6", &[]),
        vec![UpstreamProtocolKind::Responses]
    );
    assert_eq!(
        static_verified_protocols(ProviderAdapterKind::CommandCodeGoat, "claude-fable-5", &[],),
        vec![UpstreamProtocolKind::Messages]
    );
    for adapter in [ProviderAdapterKind::MiniMaxCn, ProviderAdapterKind::KimiCn] {
        assert_eq!(
            static_verified_protocols(adapter, "catalog-model", &[]),
            vec![
                UpstreamProtocolKind::ChatCompletions,
                UpstreamProtocolKind::Messages,
            ]
        );
        let probe = ProviderRegistry::iter()
            .find(|descriptor| descriptor.kind == adapter)
            .unwrap()
            .protocol_probe;
        assert_eq!(
            safety_ceiling_protocols(probe, "catalog-model"),
            vec![
                UpstreamProtocolKind::ChatCompletions,
                UpstreamProtocolKind::Messages,
            ]
        );
    }
    assert_eq!(
        static_verified_protocols(ProviderAdapterKind::Cpa, "catalog-model", &[]),
        vec![
            UpstreamProtocolKind::ChatCompletions,
            UpstreamProtocolKind::Responses,
            UpstreamProtocolKind::Messages,
        ]
    );
}

#[test]
fn stale_override_outside_fixed_provider_ceiling_is_not_materialized() {
    let mut persisted = empty_persisted();
    let scope = ContractScope::provider(MINIMAX_PROVIDER_ID);
    persisted.overrides.insert(
        scope.clone(),
        vec![PersistedModelProtocolOverride {
            scope,
            model_id: "MiniMax-M3".into(),
            protocol: UpstreamProtocolKind::Responses,
            state: ProtocolOverrideState::ForceOff,
            updated_at: Utc::now(),
        }],
    );

    let set = build_effective_contracts(&zen_seed(), &[], persisted);
    let minimax = set.providers.get(MINIMAX_PROVIDER_ID).unwrap();
    let model = minimax.model("MiniMax-M3").unwrap();
    assert!(model.protocols.contains_key("chat_completions"));
    assert!(model.protocols.contains_key("messages"));
    assert!(!model.protocols.contains_key("responses"));
}
