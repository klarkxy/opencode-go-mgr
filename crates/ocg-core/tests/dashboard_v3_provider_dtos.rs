//! Dashboard V3 provider/Zen/contract DTO catalog: schema prefix, nullability,
//! request omission, protocol tokens, and secrecy.

use ocg_core::dashboard_v3::{CATALOG_TYPE_NAMES, contract_schema};
use serde_json::{Map, Value, json};

const ACCOUNTS_CATALOG_PREFIX: &[&str] = &[
    "ControlRevision",
    "MutationAck",
    "MutationExpectation",
    "PricingRevision",
    "V3Error",
    "ConnectionInfo",
    "ConnectionSubKey",
    "Settings",
    "SettingsUpdate",
    "ProxySupportedModel",
    "KeyCreate",
    "KeyUpdate",
    "Account",
    "AccountList",
    "AccountMutation",
    "AccountCustomConfig",
    "AccountModelCapability",
    "AccountCreate",
    "AccountManagedCreate",
    "AccountModelTestRequest",
    "AccountModelTestResponse",
    "AccountUpdate",
    "AccountOrder",
    "AccountSetupUpdate",
    "AccountCustomConfigUpdate",
    "AccountCustomConfigWrite",
    "AccountModelCapabilitiesUpdate",
    "AccountModelCapabilityWrite",
];

const PROVIDER_CATALOG_TYPES: &[&str] = &[
    "ProviderCatalog",
    "ProviderCatalogEntry",
    "ProviderCatalogFormField",
    "ProviderModelCapability",
    "ZenFreeSettings",
    "ZenFreeSettingsUpdate",
    "ZenFreeModels",
    "ZenFreeModel",
    "ProviderContracts",
    "ProviderContractGroup",
    "CustomEndpointContract",
    "ProviderAccountChoice",
    "EffectiveCatalog",
    "EffectiveModelContract",
    "EffectiveModelProtocols",
    "EffectiveProtocolEvidence",
    "CapabilitySummary",
    "CardCapabilitySummary",
    "ModelProtocolOverridesUpdate",
    "ModelProtocolOverride",
    "ProtocolOverrideState",
    "ProtocolProbeRequest",
    "ProtocolProbeResult",
    "ProtocolProbeResponse",
];

const SECRET_FIELD_NAMES: &[&str] = &[
    "key",
    "password",
    "passwordCipher",
    "keyCipher",
    "gatewayKey",
    "gateway_key",
    "primaryKey",
    "primary_key",
    "referralCode",
    "referral_code",
    "cipher",
    "apiKey",
    "api_key",
    "token",
    "secret",
];

fn defs(schema: &Value) -> &Map<String, Value> {
    schema["$defs"].as_object().expect("catalog $defs")
}

fn properties<'a>(defs: &'a Map<String, Value>, name: &str) -> &'a Map<String, Value> {
    defs[name]["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{name}.properties"))
}

fn required_fields<'a>(defs: &'a Map<String, Value>, name: &str) -> Vec<&'a str> {
    defs[name]["required"]
        .as_array()
        .unwrap_or_else(|| panic!("{name}.required"))
        .iter()
        .filter_map(Value::as_str)
        .collect()
}

fn schema_field_names<'a>(value: &'a Value, acc: &mut Vec<&'a str>) {
    match value {
        Value::Object(map) => {
            if let Some(properties) = map.get("properties").and_then(Value::as_object) {
                acc.extend(properties.keys().map(String::as_str));
                for nested in properties.values() {
                    schema_field_names(nested, acc);
                }
            }
            for (key, nested) in map {
                if key != "properties" {
                    schema_field_names(nested, acc);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                schema_field_names(item, acc);
            }
        }
        _ => {}
    }
}

#[test]
fn catalog_type_names_keep_the_accounts_prefix() {
    assert_eq!(
        &CATALOG_TYPE_NAMES[..ACCOUNTS_CATALOG_PREFIX.len()],
        ACCOUNTS_CATALOG_PREFIX
    );
    for name in PROVIDER_CATALOG_TYPES {
        assert!(
            CATALOG_TYPE_NAMES.contains(name),
            "CATALOG_TYPE_NAMES missing {name}"
        );
    }
    let provider_end = ACCOUNTS_CATALOG_PREFIX.len() + PROVIDER_CATALOG_TYPES.len();
    assert_eq!(
        &CATALOG_TYPE_NAMES[ACCOUNTS_CATALOG_PREFIX.len()..provider_end],
        PROVIDER_CATALOG_TYPES
    );
}

#[test]
fn provider_schema_registers_nullable_responses_and_omittable_requests() {
    let schema = contract_schema();
    let defs = defs(&schema);
    for name in CATALOG_TYPE_NAMES {
        assert!(defs.contains_key(*name), "schema missing {name}");
    }

    let any_of = schema["anyOf"].as_array().expect("catalog anyOf");
    for (index, name) in ACCOUNTS_CATALOG_PREFIX.iter().enumerate() {
        assert_eq!(
            any_of[index]["$ref"],
            format!("#/$defs/{name}"),
            "anyOf prefix drifted at {index}"
        );
    }

    let entry_required = required_fields(defs, "ProviderCatalogEntry");
    for field in ["creationUnavailableReason", "keyPrefix"] {
        assert!(
            entry_required.contains(&field),
            "ProviderCatalogEntry.{field} must stay required T|null"
        );
    }
    let zen_required = required_fields(defs, "ZenFreeModels");
    assert!(zen_required.contains(&"refreshedAt"));
    let evidence_required = required_fields(defs, "EffectiveProtocolEvidence");
    for field in [
        "verifiedAt",
        "observedAt",
        "lastProbeResult",
        "lastProbeAt",
        "lastProbeError",
    ] {
        assert!(
            evidence_required.contains(&field),
            "EffectiveProtocolEvidence.{field} must stay required T|null"
        );
    }
    let probe_response_required = required_fields(defs, "ProtocolProbeResponse");
    assert!(probe_response_required.contains(&"accountId"));
    assert!(probe_response_required.contains(&"providerId"));
    assert!(probe_response_required.contains(&"contract"));
    assert!(probe_response_required.contains(&"pricingRevision"));

    assert_eq!(
        required_fields(defs, "ZenFreeSettings"),
        vec![
            "accountId",
            "enabled",
            "revision",
            "processGeneration",
            "pricingRevision",
        ]
    );

    let probe_request_required = required_fields(defs, "ProtocolProbeRequest");
    assert!(probe_request_required.contains(&"expectedRevision"));
    assert!(probe_request_required.contains(&"processGeneration"));
    assert!(probe_request_required.contains(&"modelId"));
    assert!(probe_request_required.contains(&"protocols"));
    assert!(!probe_request_required.contains(&"accountId"));
    assert_eq!(defs["ProtocolProbeRequest"]["additionalProperties"], false);
    assert_eq!(defs["ZenFreeSettingsUpdate"]["additionalProperties"], false);
    assert_eq!(
        defs["ModelProtocolOverridesUpdate"]["additionalProperties"],
        false
    );

    let catalog_required = required_fields(defs, "ProviderCatalog");
    for field in [
        "revision",
        "processGeneration",
        "pricingRevision",
        "entries",
    ] {
        assert!(
            catalog_required.contains(&field),
            "ProviderCatalog missing {field}"
        );
    }
    assert!(!catalog_required.contains(&"modelCapabilities"));
    let contracts_required = required_fields(defs, "ProviderContracts");
    for field in [
        "revision",
        "processGeneration",
        "pricingRevision",
        "providers",
        "customEndpoints",
    ] {
        assert!(
            contracts_required.contains(&field),
            "ProviderContracts missing {field}"
        );
    }
}

#[test]
fn protocol_tokens_stay_snake_case_and_nested_revision_is_not_cas() {
    let schema = contract_schema();
    let defs = defs(&schema);
    let override_state = schema["$defs"]["ProtocolOverrideState"]
        .as_object()
        .unwrap();
    assert_eq!(
        override_state["enum"],
        json!(["auto", "force_on", "force_off"])
    );

    let model_protocols = properties(defs, "EffectiveModelProtocols");
    assert!(model_protocols.contains_key("chat_completions"));
    assert!(model_protocols.contains_key("responses"));
    assert!(model_protocols.contains_key("messages"));
    let required = required_fields(defs, "EffectiveModelProtocols");
    assert!(required.contains(&"chat_completions"));
    assert!(required.contains(&"responses"));
    assert!(required.contains(&"messages"));

    let group = properties(defs, "ProviderContractGroup");
    assert!(group.contains_key("revision"));
    assert!(!group.contains_key("scopeRevision"));
    assert!(group.contains_key("scopeKind"));
    assert!(!group.contains_key("customEndpoints"));

    let custom = properties(defs, "CustomEndpointContract");
    assert!(custom.contains_key("revision"));
    assert!(!custom.contains_key("scopeRevision"));
    assert!(custom.contains_key("account"));

    let catalog_entry = properties(defs, "ProviderCatalogEntry");
    assert!(catalog_entry.contains_key("providerId"));
    assert!(!catalog_entry.contains_key("offeringId"));
    assert!(!catalog_entry.contains_key("provider_id"));

    let override_update = properties(defs, "ModelProtocolOverridesUpdate");
    assert!(override_update.contains_key("expectedRevision"));
    assert!(override_update.contains_key("processGeneration"));
    assert!(override_update.contains_key("overrides"));
    assert!(!override_update.contains_key("expected_revision"));

    let override_entry = properties(defs, "ModelProtocolOverride");
    assert!(override_entry.contains_key("modelId"));
    assert!(override_entry.contains_key("protocol"));
    assert!(override_entry.contains_key("state"));

    for field in [
        "creationAvailability",
        "verificationPolicy",
        "verificationRuntimeAvailability",
        "pricingAvailability",
        "usageAvailability",
    ] {
        assert!(
            properties(defs, "ProviderCatalogEntry")[field]
                .get("enum")
                .is_none(),
            "{field} must remain registry-extensible"
        );
    }
    assert!(
        properties(defs, "ProviderCatalogFormField")["kind"]
            .get("enum")
            .is_none(),
        "form-field kind must remain registry-extensible"
    );
}

#[test]
fn provider_dto_schema_has_no_secret_bearing_fields() {
    let schema = contract_schema();
    let defs = defs(&schema);
    for name in PROVIDER_CATALOG_TYPES {
        let mut fields = Vec::new();
        schema_field_names(&defs[*name], &mut fields);
        for field in fields {
            assert!(
                !SECRET_FIELD_NAMES.contains(&field),
                "{name} schema leaked secret-bearing field {field}"
            );
        }
        let encoded = defs[*name].to_string();
        for secret in ["sk-secret", "ocg-secret", "pw-secret", "user:pass@"] {
            assert!(
                !encoded.contains(secret),
                "{name} schema leaked secret sample {secret}"
            );
        }
    }
    assert!(
        properties(defs, "ConnectionInfo").contains_key("primaryKey"),
        "ConnectionInfo remains the only secret-bearing V3 DTO"
    );
}
