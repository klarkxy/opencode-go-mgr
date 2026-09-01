//! Focused Command Code GOAT official-contract tests.
//!
//! These tests prove the fixed-origin transport/model seam without live
//! `api.commandcode.ai` calls.

use ocg_core::alias::{ResolvedModel, resolve};
use ocg_core::gateway::protocol::{
    ApiFormat, command_code_model_protocol, command_code_supports_upstream,
    command_code_upstream_path, opencode_supports_upstream,
};
use ocg_core::gateway::provider_adapter::{
    command_code_goat_official_url, command_code_goat_transport_spec,
};
use ocg_core::provider::{
    COMMAND_CODE_GOAT_BASE_URL, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, COMMAND_CODE_PROVIDER_ID,
};

#[test]
fn official_base_path_auth_and_public_catalog_contract_stay_fixed() {
    let spec = command_code_goat_transport_spec();
    assert_eq!(spec.base_url, COMMAND_CODE_GOAT_BASE_URL);
    assert_eq!(spec.host, "api.commandcode.ai");
    assert_eq!(spec.chat_completions_path, "/chat/completions");
    assert_eq!(spec.auth_scheme.as_str(), "bearer");
    assert!(!spec.follow_redirects);
    assert_eq!(spec.zdr_header_name, None);
    assert!(spec.public_catalog_refresh);
    assert_eq!(
        command_code_goat_official_url(ApiFormat::ChatCompletions).unwrap(),
        "https://api.commandcode.ai/provider/v1/chat/completions"
    );
    assert!(command_code_goat_official_url(ApiFormat::Responses).is_err());

    let plan = ocg_core::provider::builtin_provider(COMMAND_CODE_PROVIDER_ID).unwrap();
    assert!(plan.routable);
    assert_eq!(plan.verification_runtime_availability, "not_applicable");
}

#[test]
fn slash_raw_pin_is_chat_only_and_not_an_opencode_protocol_row() {
    assert!(command_code_model_protocol(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM).is_some());
    assert!(command_code_supports_upstream(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
        ApiFormat::ChatCompletions
    ));
    assert!(!command_code_supports_upstream(
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
        ApiFormat::Responses
    ));
    assert_eq!(
        command_code_upstream_path(ApiFormat::ChatCompletions),
        Some("/chat/completions")
    );
    assert_eq!(command_code_upstream_path(ApiFormat::Responses), None);
    assert!(
        !opencode_supports_upstream(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            ApiFormat::ChatCompletions
        ),
        "GOAT raw id must not resolve through OpenCode MODEL_PROTOCOLS"
    );
    match resolve(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM).unwrap() {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            assert!(mapping.is_command_code_goat());
            assert!(
                !mapping.routeable,
                "static registry pin stays closed until a verified catalog overlay"
            );
        }
        other => panic!("expected unique GOAT pin, got {other:?}"),
    }
    match resolve(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS).unwrap() {
        ResolvedModel::Alias { mappings, .. } => {
            assert!(
                mappings
                    .iter()
                    .filter(|mapping| mapping.routeable)
                    .all(|mapping| mapping.is_opencode_go() || mapping.is_zen_free())
            );
            assert!(mappings.iter().any(|mapping| mapping.is_opencode_go()));
            assert!(mappings.iter().any(|mapping| mapping.is_zen_free()));
            assert!(
                mappings
                    .iter()
                    .filter(|mapping| mapping.is_command_code_goat())
                    .all(|mapping| !mapping.routeable)
            );
        }
        other => panic!("expected shared Go/Zen alias, got {other:?}"),
    }
}
