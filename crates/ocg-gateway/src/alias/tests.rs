use super::*;
use std::any::{TypeId, type_name};

fn seeded_free_models() -> Vec<String> {
    ZenFreeModelCatalog::default().models
}

#[test]
fn alias_types_are_owned_by_this_module() {
    assert_eq!(
        type_name::<ProviderMapping>(),
        "ocg_gateway::alias::ProviderMapping"
    );
    assert_eq!(type_name::<AliasEntry>(), "ocg_gateway::alias::AliasEntry");
    assert_eq!(
        type_name::<ResolvedModel>(),
        "ocg_gateway::alias::ResolvedModel"
    );
    assert_eq!(
        type_name::<ResolveError>(),
        "ocg_gateway::alias::ResolveError"
    );
    assert_eq!(
        type_name::<PublishedAlias>(),
        "ocg_gateway::alias::PublishedAlias"
    );
    let _ = TypeId::of::<ProviderMapping>();
    let _ = TypeId::of::<AliasEntry>();
    let _ = TypeId::of::<ResolvedModel>();
    let _ = TypeId::of::<ResolveError>();
    let _ = TypeId::of::<PublishedAlias>();
    let _: ResolveName = resolve;
    let _: ResolveCustom = resolve_with_custom;
    let _: ResolveProviderModels = resolve_with_provider_models;
    let _: ResolveCatalogs = resolve_with_catalogs;
    let _: fn() -> Vec<String> = published_aliases;
    let _: fn() -> Vec<PublishedAlias> = published_routeable_aliases;
    let _: fn(&[String]) -> Vec<PublishedAlias> = published_routeable_aliases_with_zen;
    let _: fn(&str, &str) -> Vec<String> = routeable_aliases_for;
    let _: RouteableWithZen = routeable_aliases_for_with_zen;
    let _: fn(&str) -> bool = is_published_alias;
}

#[test]
fn go_model_ids_are_preferred_aliases() {
    let resolved = resolve("glm-5.2").expect("known Go model");
    match resolved {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => {
            assert_eq!(alias, "glm-5.2");
            assert_eq!(mappings.len(), 1);
            assert!(mappings[0].is_opencode_go());
            assert!(mappings[0].routeable);
            assert_eq!(mappings[0].upstream_model, "glm-5.2");
        }
        other => panic!("expected alias, got {other:?}"),
    }
}

#[test]
fn alias_lookup_is_case_insensitive_kebab() {
    let resolved = resolve("GLM-5.2").expect("case-folded alias");
    assert!(matches!(resolved, ResolvedModel::Alias { alias, .. } if alias == "glm-5.2"));
    assert!(is_published_alias("Grok-4.5"));
    assert!(is_published_alias(" glm-5.2 "));
    for alias in published_aliases() {
        assert_eq!(alias, alias.to_lowercase());
        assert!(!alias.is_empty());
        assert!(!looks_raw_shaped(&alias));
    }
}

#[test]
fn raw_looking_names_do_not_collapse_onto_kebab_aliases() {
    for name in ["glm/5.2", "GLM_5.2", "Grok 4.5", "glm 5.2"] {
        match resolve(name) {
            Err(ResolveError::Unknown { requested }) => assert_eq!(requested, name),
            other => panic!("`{name}` must not collapse onto a kebab alias, got {other:?}"),
        }
        assert!(!is_published_alias(name));
    }
    assert!(matches!(
        resolve("glm-5.2").unwrap(),
        ResolvedModel::Alias { alias, .. } if alias == "glm-5.2"
    ));
}

#[test]
fn free_ids_are_exact_pins_and_only_stripped_aliases_are_published() {
    let resolved = resolve("deepseek-v4-flash-free").expect("Zen model id");
    match resolved {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert!(mapping.is_zen_free());
            assert_eq!(mapping.upstream_model, "deepseek-v4-flash-free");
        }
        other => panic!("expected raw pin, got {other:?}"),
    }
    assert!(matches!(
        resolve("deepseek-v4-flash"),
        Ok(ResolvedModel::Alias { mappings, .. }) if mappings.iter().any(ProviderMapping::is_zen_free)
    ));
    assert!(
        !published_aliases()
            .iter()
            .any(|alias| alias == "deepseek-v4-flash-free")
    );
}

#[test]
fn shared_aliases_record_go_and_zen_mappings_in_the_registry() {
    match resolve("mimo-v2.5").unwrap() {
        ResolvedModel::Alias { mappings, .. } => {
            assert_eq!(mappings.len(), 2);
            assert!(mappings[0].is_opencode_go());
            assert!(mappings[1].is_zen_free());
            assert_eq!(mappings[1].upstream_model, "mimo-v2.5-free");
        }
        other => panic!("expected alias, got {other:?}"),
    }
    match resolve("glm-5.2").unwrap() {
        ResolvedModel::Alias { mappings, .. } => assert_eq!(mappings.len(), 1),
        other => panic!("expected alias, got {other:?}"),
    }
}

#[test]
fn command_catalog_uses_go_canonical_aliases_and_keeps_raw_ids_pinned() {
    let go = vec!["hy3".to_string(), "future-go".to_string()];
    let zen = vec!["hy3-free".to_string()];
    let command = vec![
        "vendor/hy3".to_string(),
        "vendor/future-go".to_string(),
        "acme/Model_Name".to_string(),
        "stealth/ox-alpha".to_string(),
    ];

    match resolve_with_all_catalogs("hy3", &go, &zen, &[], &command).unwrap() {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => {
            assert_eq!(alias, "hy3");
            assert!(mappings.iter().any(ProviderMapping::is_opencode_go));
            assert!(mappings.iter().any(ProviderMapping::is_zen_free));
            assert!(mappings.iter().any(|mapping| {
                mapping.is_command_code_goat() && mapping.upstream_model == "vendor/hy3"
            }));
        }
        other => panic!("expected three-supplier Alias, got {other:?}"),
    }
    assert!(matches!(
        resolve_with_all_catalogs("vendor/hy3", &go, &zen, &[], &command),
        Ok(ResolvedModel::PinnedRaw { mapping, .. })
            if mapping.is_command_code_goat() && mapping.upstream_model == "vendor/hy3"
    ));
    assert!(matches!(
        resolve_with_all_catalogs("future-go", &go, &zen, &[], &command),
        Ok(ResolvedModel::PinnedRaw { mapping, .. })
            if mapping.is_opencode_go() && mapping.upstream_model == "future-go"
    ));
    let suffix_command = vec!["hy3-paid".to_string()];
    assert!(matches!(
        resolve_with_all_catalogs("hy3", &go, &zen, &[], &suffix_command),
        Ok(ResolvedModel::Alias { mappings, .. })
            if mappings.iter().any(|mapping| mapping.is_command_code_goat()
                && mapping.upstream_model == "hy3-paid")
    ));
    assert!(matches!(
        resolve_with_all_catalogs("hy3-paid", &go, &zen, &[], &suffix_command),
        Ok(ResolvedModel::PinnedRaw { mapping, .. })
            if mapping.is_command_code_goat() && mapping.upstream_model == "hy3-paid"
    ));
    match resolve_with_all_catalogs("ox-alpha-free", &go, &zen, &[], &command).unwrap() {
        ResolvedModel::Alias { mappings, .. } => {
            assert!(mappings.iter().any(ProviderMapping::is_opencode_go));
            assert!(mappings.iter().any(|mapping| {
                mapping.is_command_code_goat() && mapping.upstream_model == "stealth/ox-alpha"
            }));
        }
        other => panic!("expected Ox Alpha to share the Go baseline Alias, got {other:?}"),
    }
    assert!(matches!(
        resolve_with_all_catalogs("stealth/ox-alpha", &go, &zen, &[], &command),
        Ok(ResolvedModel::PinnedRaw { mapping, .. })
            if mapping.is_command_code_goat()
                && mapping.upstream_model == "stealth/ox-alpha"
    ));

    match resolve_with_all_catalogs("model-name", &go, &zen, &[], &command).unwrap() {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert!(mapping.is_command_code_goat());
            assert_eq!(mapping.upstream_model, "acme/Model_Name");
        }
        other => panic!("expected raw-only Command pin, got {other:?}"),
    }
    assert!(matches!(
        resolve_with_all_catalogs("acme/Model_Name", &go, &zen, &[], &command),
        Ok(ResolvedModel::PinnedRaw { mapping, .. })
            if mapping.is_command_code_goat() && mapping.upstream_model == "acme/Model_Name"
    ));

    let shared_raw = vec!["vendor/shared".to_string()];
    let error = resolve_with_all_catalogs("vendor/shared", &shared_raw, &[], &[], &shared_raw)
        .expect_err("provider raw collision must remain ambiguous");
    assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));

    let published = published_routeable_aliases_with_all_catalogs(&go, &zen, &command);
    assert_eq!(
        published.iter().filter(|item| item.alias == "hy3").count(),
        1
    );
    assert!(!published.iter().any(|item| item.alias == "model-name"));
    assert!(
        !published
            .iter()
            .any(|item| { item.alias.contains('/') || is_free_model(&item.alias) })
    );
}

#[test]
fn command_catalog_shortens_only_code_owned_long_names() {
    let nemotron_upstream = COMMAND_CODE_GOAT_ALIASES[0].0.to_string();
    let command = vec![nemotron_upstream.clone()];

    match resolve_with_all_catalogs("nemotron-3-ultra", &[], &[], &[], &command).unwrap() {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => {
            assert_eq!(alias, "nemotron-3-ultra");
            assert!(mappings.iter().any(|mapping| {
                mapping.is_command_code_goat() && mapping.upstream_model == nemotron_upstream
            }));
        }
        other => panic!("expected sealed GOAT short Alias, got {other:?}"),
    }
    assert!(matches!(
        resolve_with_all_catalogs(&nemotron_upstream, &[], &[], &[], &command),
        Ok(ResolvedModel::PinnedRaw { mapping, .. })
            if mapping.is_command_code_goat()
                && mapping.upstream_model == nemotron_upstream
    ));
    assert_eq!(
        canonical_alias_for_provider_model(COMMAND_CODE_PROVIDER_ID, &nemotron_upstream, &[], &[],),
        "nemotron-3-ultra"
    );
    assert!(
        published_routeable_aliases_with_all_catalogs(&[], &[], &command)
            .iter()
            .any(|item| item.alias == "nemotron-3-ultra"
                && item.owned_by == COMMAND_CODE_PROVIDER_ID)
    );
    assert_eq!(
        routeable_aliases_for_with_extended_catalogs(
            COMMAND_CODE_PROVIDER_ID,
            GOAT_OFFERING_ID,
            &[],
            &command,
            &[],
            &[],
        ),
        vec!["nemotron-3-ultra".to_string()]
    );
    assert!(
        !published_routeable_aliases_with_all_catalogs(&[], &[], &[])
            .iter()
            .any(|item| item.alias == "nemotron-3-ultra")
    );

    let future = vec!["vendor/future-model-with-a-very-long-name".to_string()];
    assert!(matches!(
        resolve_with_all_catalogs(
            "future-model-with-a-very-long-name",
            &[],
            &[],
            &[],
            &future,
        ),
        Ok(ResolvedModel::PinnedRaw { mapping, .. })
            if mapping.is_command_code_goat()
    ));
    assert!(
        !published_routeable_aliases_with_all_catalogs(&[], &[], &future)
            .iter()
            .any(|item| item.alias == "future-model-with-a-very-long-name")
    );
}

#[test]
fn command_catalog_reuses_sealed_cn_aliases_and_known_plan_suffixes() {
    let command = vec![
        "moonshotai/Kimi-K2.7-Code-Highspeed".to_string(),
        "poolside/laguna-s-2.1-free".to_string(),
    ];
    for (alias, upstream) in [
        (
            "kimi-k2.7-code-highspeed",
            "moonshotai/Kimi-K2.7-Code-Highspeed",
        ),
        ("laguna-s-2.1", "poolside/laguna-s-2.1-free"),
    ] {
        match resolve_with_all_catalogs(alias, &[], &["laguna-s-2.1-free".into()], &[], &command)
            .unwrap()
        {
            ResolvedModel::Alias { mappings, .. } => assert!(mappings.iter().any(|mapping| {
                mapping.is_command_code_goat() && mapping.upstream_model == upstream
            })),
            other => panic!("expected code-owned Command Alias, got {other:?}"),
        }
    }
}

#[test]
fn refreshed_zen_models_without_static_authority_stay_raw_only() {
    let models = vec!["brand-new-coder-free".to_string()];
    match resolve_with_provider_models("brand-new-coder-free", &models, &[]).unwrap() {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert!(mapping.is_zen_free());
            assert_eq!(mapping.upstream_model, "brand-new-coder-free");
        }
        other => panic!("expected dynamic Zen raw pin, got {other:?}"),
    }
    assert!(matches!(
        resolve_with_provider_models("brand-new-coder", &models, &[]),
        Err(ResolveError::Unknown { .. })
    ));
    let published = published_routeable_aliases_with_zen(&models);
    assert!(
        !published
            .iter()
            .any(|entry| entry.alias == "brand-new-coder")
    );
    assert!(
        !published
            .iter()
            .any(|entry| entry.alias == "brand-new-coder-free")
    );
}

#[test]
fn refreshed_catalog_cannot_steal_go_only_ox_alpha_free() {
    let models = vec!["ox-alpha-free".to_string()];
    match resolve_with_provider_models("ox-alpha-free", &models, &[]).unwrap() {
        ResolvedModel::Alias { mappings, .. } => {
            assert!(mappings.iter().all(ProviderMapping::is_opencode_go));
        }
        other => panic!("expected Go alias, got {other:?}"),
    }
    assert!(resolve_with_provider_models("ox-alpha", &models, &[]).is_err());
}

#[test]
fn registry_covers_every_opencode_protocol_id() {
    let aliases = published_aliases();
    let free_models = seeded_free_models();
    for id in supported_model_ids() {
        if id == "big-pickle" || (is_free_model(id) && !free_models.iter().any(|free| free == id)) {
            continue;
        }
        let expected_alias = if is_free_model(id) {
            stripped_free_alias(id).expect("free protocol id has a stripped alias")
        } else {
            id
        };
        assert!(
            aliases.iter().any(|alias| alias == expected_alias),
            "MODEL_PROTOCOLS id `{id}` must have an alias"
        );
    }
    for id in &free_models {
        let statically_authorized = supported_model_ids().any(|known| known == id);
        assert_eq!(
            stripped_free_alias(id).is_some_and(|alias| aliases.iter().any(|item| item == alias)),
            statically_authorized,
            "only a statically authorized free model may have a stripped alias: `{id}`"
        );
        assert!(resolve(id).unwrap().routeable_mappings()[0].is_zen_free());
    }
    assert!(!aliases.iter().any(|alias| alias.contains("goat")));
    assert!(
        !aliases
            .iter()
            .any(|alias| alias.contains("unknown-provider"))
    );
    assert!(!aliases.iter().any(|alias| alias.contains("custom")));
}

#[test]
fn published_routeable_aliases_use_routeable_provider_ownership() {
    let published = published_routeable_aliases();
    assert!(!published.is_empty());
    let ids: Vec<&str> = published.iter().map(|item| item.alias.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "GET /v1/models order must be deterministic");
    assert_eq!(
        published.len(),
        published_aliases().len(),
        "builtin aliases currently all have a routeable mapping"
    );
    for item in &published {
        assert!(!looks_raw_shaped(&item.alias));
        assert_ne!(item.alias, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM);
        match resolve(&item.alias).unwrap() {
            ResolvedModel::Alias { mappings, .. } => {
                let routeable = mappings
                    .iter()
                    .find(|mapping| mapping.routeable)
                    .expect("published alias must have a routeable mapping");
                assert_eq!(item.owned_by, routeable.provider_id);
                assert_ne!(item.owned_by, COMMAND_CODE_PROVIDER_ID);
                assert_ne!(item.owned_by, "unknown-provider");
                assert_ne!(item.owned_by, CUSTOM_PROVIDER_ID);
            }
            other => panic!("published id must be an alias, got {other:?}"),
        }
    }
    let go = published
        .iter()
        .find(|item| item.alias == "glm-5.2")
        .expect("Go alias");
    assert_eq!(go.owned_by, OPENCODE_PROVIDER_ID);
    assert!(
        !published
            .iter()
            .any(|item| item.alias == "deepseek-v4-flash-free")
    );
    let goat_alias = published
        .iter()
        .find(|item| item.alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS)
        .expect("Go still owns the kebab alias");
    assert_eq!(goat_alias.owned_by, OPENCODE_PROVIDER_ID);
    assert!(
        !published
            .iter()
            .any(|item| item.alias.contains('/') || is_free_model(&item.alias))
    );
}

#[test]
fn slash_prefixed_goat_raw_pins_to_command_code_and_does_not_steal_go() {
    match resolve(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM) {
        Ok(ResolvedModel::PinnedRaw { mapping, .. }) => {
            assert!(mapping.is_command_code_goat());
            assert!(!mapping.routeable);
            assert_eq!(
                mapping.upstream_model,
                COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
            );
            assert!(
                ResolvedModel::PinnedRaw {
                    requested: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into(),
                    mapping: mapping.clone(),
                }
                .routeable_mappings()
                .is_empty()
            );
        }
        other => panic!("GOAT raw id must uniquely pin to command-code/goat, got {other:?}"),
    }
    match resolve(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS).unwrap() {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => {
            assert_eq!(alias, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS);
            assert!(mappings.iter().any(|mapping| mapping.is_opencode_go()));
            assert!(
                mappings
                    .iter()
                    .any(|mapping| mapping.is_command_code_goat() && !mapping.routeable)
            );
            let routeable = mappings
                .iter()
                .filter(|mapping| mapping.routeable)
                .collect::<Vec<_>>();
            assert_eq!(routeable.len(), 2);
            assert!(routeable.iter().any(|mapping| mapping.is_opencode_go()));
            assert!(routeable.iter().any(|mapping| mapping.is_zen_free()));
            assert_eq!(
                routeable
                    .iter()
                    .find(|mapping| mapping.is_opencode_go())
                    .unwrap()
                    .upstream_model,
                COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS
            );
        }
        other => panic!("expected published Go alias, got {other:?}"),
    }
    assert!(is_published_alias(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS
    ));
    assert!(!is_published_alias(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
    ));
}

#[test]
fn eligible_goat_catalog_joins_static_aliases_and_keeps_other_ids_raw() {
    let goat_ids = vec![
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.to_string(),
        "claude-sonnet-4-6".into(),
    ];
    match resolve_with_catalogs(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
        &[],
        &[],
        &goat_ids,
    )
    .unwrap()
    {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert!(mapping.is_command_code_goat());
            assert!(mapping.routeable);
        }
        other => panic!("expected routeable GOAT pin, got {other:?}"),
    }
    match resolve_with_catalogs("claude-sonnet-4-6", &[], &[], &goat_ids).unwrap() {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert!(mapping.is_command_code_goat());
            assert!(mapping.routeable);
        }
        other => panic!("expected raw-only GOAT pin, got {other:?}"),
    }
    match resolve_with_catalogs(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
        &[],
        &[],
        &goat_ids,
    )
    .unwrap()
    {
        ResolvedModel::Alias { mappings, .. } => {
            assert!(
                mappings
                    .iter()
                    .any(|mapping| mapping.routeable && mapping.is_opencode_go())
            );
            assert!(
                mappings
                    .iter()
                    .any(|mapping| mapping.routeable && mapping.is_command_code_goat())
            );
        }
        other => panic!("GOAT must not steal Go kebab alias, got {other:?}"),
    }
    let published = published_routeable_aliases_with_catalogs(&[], &goat_ids);
    assert!(
        !published
            .iter()
            .any(|item| item.alias == "claude-sonnet-4-6")
    );
    assert!(
        published
            .iter()
            .find(|item| item.alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS)
            .is_some_and(|item| item.owned_by == OPENCODE_PROVIDER_ID)
    );
    assert!(
        !published
            .iter()
            .any(|item| item.alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM)
    );
    match resolve_with_catalogs(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, &[], &[], &[])
        .unwrap()
    {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert!(mapping.is_command_code_goat());
            assert!(!mapping.routeable);
        }
        other => panic!("empty GOAT catalog must keep the static pin closed, got {other:?}"),
    }
    assert!(
        !published_aliases()
            .iter()
            .any(|alias| alias.contains('/')
                || *alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM)
    );
}

#[test]
fn unknown_names_are_not_aliases() {
    match resolve("definitely-not-a-model") {
        Err(ResolveError::Unknown { requested }) => {
            assert_eq!(requested, "definitely-not-a-model");
            assert!(
                ResolveError::Unknown {
                    requested: requested.clone()
                }
                .code()
                .is_none()
            );
        }
        other => panic!("expected unknown, got {other:?}"),
    }
}

#[test]
fn unique_raw_id_pins_to_one_mapping() {
    let registry = registry_from_entries(vec![
        AliasEntry {
            alias: "widget".into(),
            mappings: vec![go_mapping("widget")],
        },
        AliasEntry {
            alias: "gadget".into(),
            mappings: vec![ProviderMapping {
                provider_id: OPENCODE_PROVIDER_ID,
                offering_id: GO_OFFERING_ID,
                upstream_model: "vendor.gadget-v1".into(),
                routeable: true,
            }],
        },
    ]);
    match resolve_in(&registry, "vendor.gadget-v1").unwrap() {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert_eq!(mapping.upstream_model, "vendor.gadget-v1");
            assert!(mapping.is_opencode_go());
        }
        other => panic!("expected pinned raw, got {other:?}"),
    }
    // Alias still wins when a kebab string is both an alias and a raw ID.
    assert!(matches!(
        resolve_in(&registry, "widget").unwrap(),
        ResolvedModel::Alias { alias, .. } if alias == "widget"
    ));
    // Exact slash-form raw IDs pin without collapsing onto a kebab alias.
    let slash_registry = registry_from_entries(vec![AliasEntry {
        alias: "widget".into(),
        mappings: vec![ProviderMapping {
            provider_id: OPENCODE_PROVIDER_ID,
            offering_id: GO_OFFERING_ID,
            upstream_model: "vendor/widget-v1".into(),
            routeable: true,
        }],
    }]);
    match resolve_in(&slash_registry, "vendor/widget-v1").unwrap() {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert_eq!(mapping.upstream_model, "vendor/widget-v1");
        }
        other => panic!("exact slash raw must pin, got {other:?}"),
    }
    assert!(matches!(
        resolve_in(&slash_registry, "Vendor/widget-v1"),
        Err(ResolveError::Unknown { .. })
    ));
    assert!(matches!(
        resolve_in(&slash_registry, "vendor-widget-v1"),
        Err(ResolveError::Unknown { .. })
    ));
}

#[test]
fn overlapping_raw_ids_return_ambiguous_model_id() {
    let registry = registry_from_entries(vec![
        AliasEntry {
            alias: "alpha".into(),
            mappings: vec![go_mapping("shared-raw")],
        },
        AliasEntry {
            alias: "beta".into(),
            mappings: vec![zen_mapping("shared-raw")],
        },
    ]);
    match resolve_in(&registry, "shared-raw") {
        Err(error) => {
            assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));
            let message = error.message();
            assert!(message.contains(AMBIGUOUS_MODEL_ID));
            assert!(message.contains("shared-raw"));
            assert!(message.contains("opencode/go"));
            assert!(message.contains("opencode-zen-free"));
        }
        other => panic!("expected ambiguous, got {other:?}"),
    }
    // Preferred aliases still resolve even when their upstream IDs overlap.
    assert!(matches!(
        resolve_in(&registry, "alpha").unwrap(),
        ResolvedModel::Alias { alias, .. } if alias == "alpha"
    ));
}

#[test]
fn fail_closed_raw_mapping_is_not_routeable() {
    let registry = registry_from_entries(vec![AliasEntry {
        alias: "visible".into(),
        mappings: vec![ProviderMapping {
            provider_id: "command-code",
            offering_id: "goat",
            upstream_model: "goat-only-raw".into(),
            routeable: false,
        }],
    }]);
    match resolve_in(&registry, "goat-only-raw") {
        Ok(ResolvedModel::PinnedRaw { mapping, .. }) => {
            assert!(!mapping.routeable);
            assert_eq!(mapping.provider_id, "command-code");
            assert!(
                ResolvedModel::PinnedRaw {
                    requested: "goat-only-raw".into(),
                    mapping: mapping.clone(),
                }
                .routeable_mappings()
                .is_empty()
            );
        }
        other => {
            panic!("fail-closed unique raw must pin without being routeable, got {other:?}")
        }
    }
    match resolve_in(&registry, "visible").unwrap() {
        ResolvedModel::Alias { mappings, .. } => {
            assert!(!mappings[0].routeable);
            assert!(
                ResolvedModel::Alias {
                    requested: "visible".into(),
                    alias: "visible".into(),
                    mappings: mappings.clone(),
                }
                .routeable_mappings()
                .is_empty()
            );
        }
        other => panic!("expected alias, got {other:?}"),
    }
    assert!(
        published_routeable_in(&registry).is_empty(),
        "fail-closed aliases must stay off GET /v1/models"
    );
}

#[test]
fn catalog_aliases_are_routeable_mappings_in_registry_order() {
    let go = routeable_aliases_for(OPENCODE_PROVIDER_ID, GO_OFFERING_ID);
    let zen = routeable_aliases_for(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID);
    let free_models = seeded_free_models();
    assert!(!go.is_empty());
    assert!(!zen.is_empty());
    let mut sorted_go = go.clone();
    sorted_go.sort_unstable();
    assert_eq!(go, sorted_go, "catalog aliases must be deterministic");
    let mut sorted_zen = zen.clone();
    sorted_zen.sort_unstable();
    assert_eq!(zen, sorted_zen);

    for alias in go.iter().chain(zen.iter()) {
        assert!(!looks_raw_shaped(alias));
        assert_ne!(*alias, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM);
        assert!(!alias.contains('/'));
    }
    assert!(go.iter().any(|alias| alias == "glm-5.2"));
    assert!(
        go.iter()
            .any(|alias| alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS)
    );
    assert!(go.iter().any(|alias| alias == "minimax-m2.7-highspeed"));
    assert!(!go.iter().any(|alias| alias == "deepseek-v4-flash-free"));
    assert!(!zen.iter().any(|alias| alias == "glm-5.2"));
    assert!(zen.iter().any(|alias| alias == "deepseek-v4-flash"));
    assert!(!zen.iter().any(|alias| alias.ends_with("-free")));
    for id in free_models
        .iter()
        .filter(|id| supported_model_ids().any(|known| known == id.as_str()))
    {
        let alias = stripped_free_alias(id).expect("seeded Zen ids end in -free");
        assert!(
            zen.iter().any(|item| item == alias),
            "Zen catalog must include stripped `{id}` alias"
        );
        assert!(
            !go.iter().any(|item| item == id),
            "Go catalog must not include free `{id}`"
        );
    }
    for id in supported_model_ids().filter(|id| !is_free_model(id)) {
        if id != "big-pickle" {
            assert!(
                go.iter().any(|alias| alias == id),
                "Go catalog must include `{id}`"
            );
        }
        let has_free_twin = free_models
            .iter()
            .any(|free| stripped_free_alias(free).is_some_and(|alias| alias == id));
        assert_eq!(
            zen.iter().any(|alias| alias == id),
            has_free_twin,
            "Zen stripped aliases must match the refreshed Free catalog for `{id}`"
        );
    }

    assert!(routeable_aliases_for(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).is_empty());
    assert!(
        routeable_aliases_for(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).is_empty(),
        "Custom catalog aliases stay empty; client IDs come from account capabilities"
    );
}

#[test]
fn catalog_aliases_keep_every_routeable_offering_not_first_wins_owner() {
    let registry = registry_from_entries(vec![AliasEntry {
        alias: "shared".into(),
        mappings: vec![zen_mapping("shared"), go_mapping("shared")],
    }]);
    let published = published_routeable_in(&registry);
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].alias, "shared");
    assert_eq!(
        published[0].owned_by, OPENCODE_ZEN_FREE_PROVIDER_ID,
        "GET /v1/models owned_by stays first-wins"
    );
    assert_eq!(
        routeable_aliases_for_in(&registry, OPENCODE_PROVIDER_ID, GO_OFFERING_ID),
        ["shared"]
    );
    assert_eq!(
        routeable_aliases_for_in(
            &registry,
            OPENCODE_ZEN_FREE_PROVIDER_ID,
            ANONYMOUS_FREE_OFFERING_ID
        ),
        ["shared"]
    );
    assert!(
        routeable_aliases_for_in(&registry, COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).is_empty()
    );
}

#[test]
fn custom_overlay_does_not_steal_published_aliases_and_resolves_unknown_ids() {
    match resolve_with_custom("glm-5.2", &["glm-5.2".into()]).unwrap() {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => {
            assert_eq!(alias, "glm-5.2");
            assert!(mappings.iter().any(|mapping| mapping.is_opencode_go()));
            assert!(mappings.iter().any(|mapping| mapping.is_custom_api()));
            let routeable = mappings
                .iter()
                .filter(|mapping| mapping.routeable)
                .collect::<Vec<_>>();
            assert!(routeable.iter().any(|mapping| mapping.is_opencode_go()));
            assert!(routeable.iter().any(|mapping| mapping.is_custom_api()));
        }
        other => panic!("expected alias overlay, got {other:?}"),
    }
    match resolve_with_custom("my-local-model", &["my-local-model".into()]).unwrap() {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => {
            assert_eq!(alias, "my-local-model");
            assert_eq!(mappings.len(), 1);
            assert!(mappings[0].is_custom_api());
            assert!(mappings[0].routeable);
        }
        other => panic!("expected custom-only alias, got {other:?}"),
    }
    match resolve_with_custom("org/model", &["org/model".into()]).unwrap() {
        ResolvedModel::Alias { alias, .. } => assert_eq!(alias, "org/model"),
        other => panic!("expected raw-shaped Custom public alias, got {other:?}"),
    }
    match resolve_with_custom(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
        &[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into()],
    ) {
        Err(error) => {
            assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));
            assert!(error.message().contains("command-code/goat"));
            assert!(error.message().contains("custom/api"));
        }
        other => panic!("GOAT raw overlapping Custom must be ambiguous, got {other:?}"),
    }
    assert!(matches!(
        resolve_with_custom("definitely-not-a-model", &[]),
        Err(ResolveError::Unknown { .. })
    ));
}

#[test]
fn refreshed_go_catalog_adds_raw_pins_without_expanding_alias_authority() {
    let go = vec!["future-go-model".to_string(), "vendor/raw-go".to_string()];
    match resolve_with_all_catalogs("future-go-model", &go, &[], &[], &[]).unwrap() {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert!(mapping.is_opencode_go());
            assert_eq!(mapping.upstream_model, "future-go-model");
        }
        other => panic!("expected dynamic raw Go pin, got {other:?}"),
    }
    match resolve_with_all_catalogs("vendor/raw-go", &go, &[], &[], &[]).unwrap() {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert!(mapping.is_opencode_go());
            assert_eq!(mapping.upstream_model, "vendor/raw-go");
        }
        other => panic!("expected dynamic raw Go pin, got {other:?}"),
    }

    let published = published_routeable_aliases_with_all_catalogs(&go, &[], &[]);
    assert!(!published.iter().any(|item| item.alias == "future-go-model"));
    assert!(!published.iter().any(|item| item.alias == "vendor/raw-go"));

    let goat = vec!["vendor/raw-go".to_string()];
    let overlap = resolve_with_all_catalogs("vendor/raw-go", &go, &[], &[], &goat)
        .expect_err("overlapping raw provider IDs must remain ambiguous");
    assert_eq!(overlap.code(), Some(AMBIGUOUS_MODEL_ID));
}

#[test]
fn sealed_cn_catalogs_join_static_aliases_and_preserve_raw_ambiguity() {
    let minimax = MINIMAX_CN_ALIASES
        .iter()
        .map(|(upstream, _)| (*upstream).to_string())
        .collect::<Vec<_>>();
    let kimi = KIMI_CN_ALIASES
        .iter()
        .map(|(upstream, _)| (*upstream).to_string())
        .collect::<Vec<_>>();

    for (upstream, alias) in MINIMAX_CN_ALIASES {
        let resolved =
            resolve_with_extended_catalogs(alias, &[], &[], &[], &[], &minimax, &kimi).unwrap();
        assert!(
            resolved
                .routeable_mappings()
                .iter()
                .any(|mapping| { mapping.is_minimax_cn() && mapping.upstream_model == *upstream })
        );
        assert!(matches!(
            resolve_with_extended_catalogs(
                upstream, &[], &[], &[], &[], &minimax, &kimi,
            ),
            Ok(ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.is_minimax_cn() && mapping.upstream_model == *upstream
        ));
        assert_eq!(
            canonical_alias_for_provider_model(MINIMAX_PROVIDER_ID, upstream, &[], &[]),
            *alias
        );
    }

    for (upstream, alias) in KIMI_CN_ALIASES {
        let resolved =
            resolve_with_extended_catalogs(alias, &[], &[], &[], &[], &minimax, &kimi).unwrap();
        assert!(
            resolved
                .routeable_mappings()
                .iter()
                .any(|mapping| { mapping.is_kimi_cn() && mapping.upstream_model == *upstream })
        );
        assert!(matches!(
            resolve_with_extended_catalogs(
                upstream, &[], &[], &[], &[], &minimax, &kimi,
            ),
            Ok(ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.is_kimi_cn() && mapping.upstream_model == *upstream
        ));
        assert_eq!(
            canonical_alias_for_provider_model(KIMI_PROVIDER_ID, upstream, &[], &[]),
            *alias
        );
    }

    let published =
        published_routeable_aliases_with_extended_catalogs(&[], &[], &[], &minimax, &kimi);
    for (_, alias) in MINIMAX_CN_ALIASES.iter().chain(KIMI_CN_ALIASES) {
        assert!(published.iter().any(|item| item.alias == *alias));
    }
    let mut expected_minimax = MINIMAX_CN_ALIASES
        .iter()
        .map(|(_, alias)| (*alias).to_string())
        .collect::<Vec<_>>();
    expected_minimax.sort();
    assert_eq!(
        routeable_aliases_for_with_extended_catalogs(
            MINIMAX_PROVIDER_ID,
            MINIMAX_CN_OFFERING_ID,
            &[],
            &[],
            &minimax,
            &kimi,
        ),
        expected_minimax
    );
    let mut expected_kimi = KIMI_CN_ALIASES
        .iter()
        .map(|(_, alias)| (*alias).to_string())
        .collect::<Vec<_>>();
    expected_kimi.sort();
    assert_eq!(
        routeable_aliases_for_with_extended_catalogs(
            KIMI_PROVIDER_ID,
            KIMI_CN_OFFERING_ID,
            &[],
            &[],
            &minimax,
            &kimi,
        ),
        expected_kimi
    );

    let without_m2 = minimax
        .iter()
        .filter(|model| model.as_str() != "MiniMax-M2")
        .cloned()
        .collect::<Vec<_>>();
    let published_without_m2 =
        published_routeable_aliases_with_extended_catalogs(&[], &[], &[], &without_m2, &kimi);
    assert!(
        !published_without_m2
            .iter()
            .any(|item| item.alias == "minimax-m2")
    );

    let unknown_minimax = vec!["MiniMax-Future".to_string()];
    assert!(matches!(
        resolve_with_extended_catalogs(
            "MiniMax-Future",
            &[],
            &[],
            &[],
            &[],
            &unknown_minimax,
            &[],
        ),
        Ok(ResolvedModel::PinnedRaw { mapping, .. }) if mapping.is_minimax_cn()
    ));
    assert!(matches!(
        resolve_with_extended_catalogs("minimax-future", &[], &[], &[], &[], &unknown_minimax, &[],),
        Err(ResolveError::Unknown { .. })
    ));
    assert_eq!(
        canonical_alias_for_provider_model(MINIMAX_PROVIDER_ID, "minimax-m3", &[], &[],),
        ""
    );

    let minimax_case = vec!["provider-case".to_string()];
    let kimi_case = vec!["PROVIDER-CASE".to_string()];
    assert!(matches!(
        resolve_with_extended_catalogs(
            "provider-case",
            &[],
            &[],
            &[],
            &[],
            &minimax_case,
            &kimi_case,
        ),
        Ok(ResolvedModel::PinnedRaw { mapping, .. }) if mapping.is_minimax_cn()
    ));
    assert!(matches!(
        resolve_with_extended_catalogs(
            "PROVIDER-CASE",
            &[],
            &[],
            &[],
            &[],
            &minimax_case,
            &kimi_case,
        ),
        Ok(ResolvedModel::PinnedRaw { mapping, .. }) if mapping.is_kimi_cn()
    ));

    let shared = vec!["vendor/shared".to_string()];
    let error =
        resolve_with_extended_catalogs("vendor/shared", &[], &[], &[], &[], &shared, &shared)
            .unwrap_err();
    assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));
}

#[test]
fn cpa_catalog_joins_code_owned_aliases_and_keeps_raw_ids_exact_and_fail_closed() {
    let cpa = vec![
        "GLM-5.2".to_string(),
        "cpa-raw-model".to_string(),
        "vendor/cpa-raw".to_string(),
    ];
    let catalogs = RuntimeCatalogs {
        cpa: &cpa,
        ..RuntimeCatalogs::default()
    };
    match resolve_with_runtime_catalogs("glm-5.2", catalogs).unwrap() {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => {
            assert_eq!(alias, "glm-5.2");
            assert!(mappings.iter().any(|mapping| {
                mapping.is_cpa() && mapping.upstream_model == "GLM-5.2" && mapping.routeable
            }));
        }
        other => panic!("CPA code-owned Alias must join, got {other:?}"),
    }
    for raw in ["cpa-raw-model", "vendor/cpa-raw"] {
        match resolve_with_runtime_catalogs(raw, catalogs).unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_cpa());
                assert_eq!(mapping.upstream_model, raw);
            }
            other => panic!("CPA unknown catalog id must remain raw, got {other:?}"),
        }
    }
    assert_eq!(canonical_alias_for_cpa_model("GLM-5.2"), "glm-5.2");
    assert_eq!(canonical_alias_for_cpa_model("vendor/cpa-raw"), "");

    let published = published_routeable_aliases_with_runtime_catalogs(catalogs);
    assert!(published.iter().any(|item| item.alias == "glm-5.2"));
    assert!(!published.iter().any(|item| item.alias == "cpa-raw-model"));
    assert!(!published.iter().any(|item| item.alias == "vendor/cpa-raw"));
    assert_eq!(
        routeable_aliases_for_with_runtime_catalogs(CPA_PROVIDER_ID, CPA_OFFERING_ID, catalogs,),
        ["glm-5.2"]
    );

    let shared = vec!["vendor/shared".to_string()];
    let conflict = resolve_with_runtime_catalogs(
        "vendor/shared",
        RuntimeCatalogs {
            go: &shared,
            cpa: &shared,
            ..RuntimeCatalogs::default()
        },
    )
    .unwrap_err();
    assert_eq!(conflict.code(), Some(AMBIGUOUS_MODEL_ID));
}

/// Trim the tests module from the production source so scan tests only see
/// shipped code.
fn production_source(source: &str) -> &str {
    source
        .split("#[cfg(test)]")
        .next()
        .expect("production source precedes tests")
}

fn runtime_catalogs_for(ollama: &[&str], pinned: &[&str]) -> RuntimeCatalogs<'static> {
    fn leaked(values: &[&str]) -> &'static [String] {
        Box::leak(
            values
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }
    RuntimeCatalogs {
        go: &[],
        zen_free: &[],
        custom: &[],
        command_code: &[],
        minimax: &[],
        kimi: &[],
        cpa: &[],
        ollama: leaked(ollama),
        ollama_pinned: leaked(pinned),
    }
}

fn resolve_ollama(
    requested: &str,
    ollama: &[&str],
    pinned: &[&str],
) -> Result<ResolvedModel, ResolveError> {
    resolve_with_runtime_catalogs(requested, runtime_catalogs_for(ollama, pinned))
}

fn published_ollama(ollama: &[&str], pinned: &[&str]) -> Vec<PublishedAlias> {
    published_routeable_aliases_with_runtime_catalogs(runtime_catalogs_for(ollama, pinned))
}

#[test]
fn ollama_source_never_hardcodes_date_tagged_snapshot_ids() {
    let production = production_source(include_str!("../alias.rs"));
    let bytes = production.as_bytes();
    for (index, window) in bytes.windows(5).enumerate() {
        if window[0] == b':'
            && window[1..5].iter().all(u8::is_ascii_digit)
            && !window[1..5].iter().all(|digit| *digit == b'0')
        {
            let context = &production[index.saturating_sub(24)..(index + 8).min(production.len())];
            panic!(
                "date-tagged snapshot ids are runtime catalog data and must stay out of alias.rs: ...{context}..."
            );
        }
    }
}

#[test]
fn ollama_overlay_appends_shared_alias_mappings_without_stealing_publication() {
    match resolve_ollama(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
        &["deepseek-v4-flash:0731", "gpt-oss:20b", "gpt-oss:120b"],
        &[],
    )
    .unwrap()
    {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => {
            assert_eq!(alias, "deepseek-v4-flash");
            assert!(
                mappings.iter().any(ProviderMapping::is_opencode_go),
                "Go keeps owning the shared alias"
            );
            assert!(mappings.iter().any(|mapping| {
                mapping.is_ollama_cloud()
                    && mapping.routeable
                    && mapping.upstream_model == "deepseek-v4-flash:0731"
            }));
            assert!(
                !mappings.iter().any(|mapping| {
                    mapping.is_ollama_cloud() && mapping.upstream_model == "gpt-oss:20b"
                }),
                "size-variant stems must not bind the shared alias"
            );
        }
        other => panic!("expected shared alias, got {other:?}"),
    }

    // Exact catalog ids (with `:` tags) pin raw; no cross-account fallback.
    for exact in ["deepseek-v4-flash:0731", "gpt-oss:20b", "gpt-oss:120b"] {
        match resolve_ollama(
            exact,
            &["deepseek-v4-flash:0731", "gpt-oss:20b", "gpt-oss:120b"],
            &[],
        )
        .unwrap()
        {
            ResolvedModel::PinnedRaw { requested, mapping } => {
                assert_eq!(requested, exact);
                assert!(mapping.is_ollama_cloud());
                assert!(mapping.routeable);
                assert_eq!(mapping.upstream_model, exact);
                assert!(
                    !ResolvedModel::PinnedRaw {
                        requested: exact.to_string(),
                        mapping: mapping.clone(),
                    }
                    .allows_cross_account_fallback()
                );
            }
            other => panic!("expected raw pin for `{exact}`, got {other:?}"),
        }
    }
    assert!(resolve_ollama("deepseek-v4-flash:0999", &[], &[]).is_err());

    // Size variants sharing one stem never create or publish a stem alias.
    assert!(resolve_ollama("gpt-oss", &["gpt-oss:20b", "gpt-oss:120b"], &[]).is_err());
    let published = published_ollama(
        &["deepseek-v4-flash:0731", "gpt-oss:20b", "gpt-oss:120b"],
        &[],
    );
    assert!(
        !published
            .iter()
            .any(|item| item.alias == "gpt-oss" || item.alias.contains(':')),
        "exact ids and unpinned stems must stay off GET /v1/models"
    );
    let flash = published
        .iter()
        .find(|item| item.alias == "deepseek-v4-flash")
        .expect("the shared alias keeps its existing publication");
    assert_eq!(flash.owned_by, OPENCODE_PROVIDER_ID);
}

#[test]
fn ollama_overlay_rotates_snapshot_bindings_without_code_changes() {
    // Old tag bound...
    match resolve_ollama("deepseek-v4-pro", &["deepseek-v4-pro:0731"], &[]).unwrap() {
        ResolvedModel::Alias { mappings, .. } => assert!(mappings.iter().any(|mapping| {
            mapping.is_ollama_cloud() && mapping.upstream_model == "deepseek-v4-pro:0731"
        })),
        other => panic!("expected alias, got {other:?}"),
    }
    // ...upstream rotates the unique tag, refresh rebinds automatically.
    match resolve_ollama("deepseek-v4-pro", &["deepseek-v4-pro:0915"], &[]).unwrap() {
        ResolvedModel::Alias { mappings, .. } => {
            let ollama: Vec<_> = mappings
                .iter()
                .filter(|mapping| mapping.is_ollama_cloud())
                .collect();
            assert_eq!(ollama.len(), 1);
            assert_eq!(ollama[0].upstream_model, "deepseek-v4-pro:0915");
        }
        other => panic!("expected alias, got {other:?}"),
    }
}

#[test]
fn ollama_overlay_coexisting_tags_fail_closed_until_pinned() {
    let coexisting: &[&str] = &["deepseek-v4-flash:0731", "deepseek-v4-flash:0915"];
    // Ambiguous: this family's mapping drops out, the alias keeps serving
    // its existing families, and nothing turns into a client 400.
    match resolve_ollama(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS, coexisting, &[]).unwrap() {
        ResolvedModel::Alias { mappings, .. } => {
            assert!(
                mappings.iter().any(ProviderMapping::is_opencode_go),
                "Go keeps serving the shared alias"
            );
            assert!(
                !mappings.iter().any(ProviderMapping::is_ollama_cloud),
                "coexisting tags must fail closed, not guess"
            );
        }
        other => panic!("expected shared alias, got {other:?}"),
    }
    // Exact ids stay routable raw pins during the ambiguity window.
    assert!(matches!(
        resolve_ollama("deepseek-v4-flash:0731", coexisting, &[]).unwrap(),
        ResolvedModel::PinnedRaw { mapping, .. } if mapping.is_ollama_cloud()
    ));
    // One administrator pin resolves the ambiguity...
    match resolve_ollama(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
        coexisting,
        &["deepseek-v4-flash:0915"],
    )
    .unwrap()
    {
        ResolvedModel::Alias { mappings, .. } => {
            let ollama: Vec<_> = mappings
                .iter()
                .filter(|mapping| mapping.is_ollama_cloud())
                .collect();
            assert_eq!(ollama.len(), 1);
            assert_eq!(ollama[0].upstream_model, "deepseek-v4-flash:0915");
            assert!(ollama[0].routeable);
        }
        other => panic!("expected pinned binding, got {other:?}"),
    }
    // ...but two pins are still ambiguous: fail closed again.
    match resolve_ollama(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
        coexisting,
        coexisting,
    )
    .unwrap()
    {
        ResolvedModel::Alias { mappings, .. } => assert!(
            !mappings.iter().any(ProviderMapping::is_ollama_cloud),
            "double pins must not guess either"
        ),
        other => panic!("expected shared alias, got {other:?}"),
    }
    // Publication is invariant across all three states.
    let before: Vec<String> = published_ollama(&[], &[])
        .into_iter()
        .map(|item| item.alias)
        .collect();
    let states: Vec<Vec<&str>> = vec![
        vec![],
        vec!["deepseek-v4-flash:0915"],
        vec!["deepseek-v4-flash:0731", "deepseek-v4-flash:0915"],
    ];
    for pinned in states {
        let current: Vec<String> = published_ollama(coexisting, &pinned)
            .into_iter()
            .map(|item| item.alias)
            .collect();
        assert_eq!(
            current, before,
            "Ollama must not add, remove, or duplicate published entries"
        );
    }
}

#[test]
fn ollama_family_alias_list_follows_the_stem_guard() {
    assert_eq!(
        routeable_aliases_for_with_runtime_catalogs(
            OLLAMA_PROVIDER_ID,
            OLLAMA_CLOUD_OFFERING_ID,
            runtime_catalogs_for(&["deepseek-v4-flash:0731", "gpt-oss:120b"], &[]),
        ),
        vec!["deepseek-v4-flash".to_string()]
    );
    assert!(
        routeable_aliases_for_with_runtime_catalogs(
            OLLAMA_PROVIDER_ID,
            OLLAMA_CLOUD_OFFERING_ID,
            runtime_catalogs_for(&["deepseek-v4-flash:0731", "deepseek-v4-flash:0915"], &[]),
        )
        .is_empty()
    );
    assert_eq!(
        canonical_alias_for_provider_model(OLLAMA_PROVIDER_ID, "deepseek-v4-flash:0731", &[], &[],),
        "deepseek-v4-flash"
    );
    assert_eq!(
        canonical_alias_for_provider_model(OLLAMA_PROVIDER_ID, "gpt-oss:120b", &[], &[]),
        ""
    );
}
