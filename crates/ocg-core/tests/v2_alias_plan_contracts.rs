//! Black-box v2.0 Alias and multi-Plan contract tests.
//!
//! These tests drive public Gateway and dashboard HTTP/JSON. They are the
//! independent acceptance slice for the accepted unified-alias / multi-Plan
//! contracts. Command Code refreshes its public catalog with GET `/models`. Custom is catalog-routable with
//! an available verification runtime; live Custom network coverage lives in
//! `custom_trusted_admin.rs`.
//!
//! Out of scope: live GOAT network calls.

use reqwest::StatusCode;
use serde_json::json;

#[path = "fixtures/v2/harness.rs"]
mod harness;

use harness::*;

async fn reorder_account_first(harness: &V2Harness, account_id: &str) {
    let mut account_ids = harness
        .accounts()
        .await
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|account| account["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    account_ids.sort_by_key(|id| if id == account_id { 0 } else { 1 });
    let (status, body) = harness
        .put_json("/accounts/order", &json!({ "account_ids": account_ids }))
        .await;
    assert_eq!(status, StatusCode::OK, "account reorder failed: {body}");
}

/// Unknown offerings fail closed at the dashboard create gate.
#[tokio::test]
async fn unknown_offering_create_fails_closed() {
    let harness = V2Harness::start().await;
    let before = harness.accounts().await;
    let (status, body) = harness
        .create_account(json!({
            "provider_id": "not-a-provider",
            "name": "should-not-exist",
            "key": GO_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await
        }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        !json_contains_secret(&body, GO_ACCOUNT_KEY),
        "unknown-offering error leaked the Key: {body}"
    );
    let after = harness.accounts().await;
    assert_eq!(
        after.as_array().map(Vec::len),
        before.as_array().map(Vec::len),
        "unknown offering must not persist an account: {after}"
    );
    harness.shutdown();
}

/// A unique raw upstream ID is pinned to one provider. With only Go
/// routeable, the GOAT-shaped raw id must not fall through to OpenCode Go.
#[tokio::test]
async fn unique_raw_upstream_id_pins_to_one_provider_and_skips_go() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let (status, body) = harness.chat(GOAT_UNIQUE_RAW_ID).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "raw id {GOAT_UNIQUE_RAW_ID} is uniquely GOAT and must not succeed on Go: {body}"
    );
    assert_eq!(
        harness.fake_call_keys(),
        Vec::<String>::new(),
        "unique raw id must pin to command-code/goat and must not call the Go upstream"
    );
    let logs = harness.forward_logs().await;
    for item in logs["items"].as_array().unwrap_or(&Vec::new()) {
        assert_ne!(
            item["provider_id"].as_str(),
            Some(OPENCODE_PROVIDER_ID),
            "raw GOAT id was attributed to Go: {item}"
        );
        assert_ne!(
            item["account_id"], go["id"],
            "raw GOAT id was routed to the Go account: {item}"
        );
    }
    harness.shutdown();
}

/// A raw upstream ID mapped to more than one Plan is rejected as
/// `ambiguous_model_id` and never reaches an upstream.
///
/// Live catalog/registry currently has no overlapping raw IDs unless an
/// eligible Custom capability collides with a distinct provider mapping.
/// Structured `ambiguous_model_id` coverage lives in
/// `v2_alias_runtime::ambiguous_model_id_is_structured_across_client_formats`.
#[tokio::test]
async fn ambiguous_raw_upstream_id_is_rejected() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY, CUSTOM_ACCOUNT_KEY]).await;
    let _go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let catalog = harness.catalog().await;
    let overlaps = overlapping_raw_ids(&catalog);
    let custom =
        catalog_entry(&catalog, CUSTOM_PROVIDER_ID).expect("catalog must include custom/api");
    assert_eq!(
        custom["routable"], true,
        "v2-contract: custom/api is catalog-routable: {custom}"
    );
    assert_eq!(
        custom["verification_runtime_availability"].as_str(),
        Some("available"),
        "v2-contract: custom/api verification runtime is available: {custom}"
    );

    if overlaps.is_empty() {
        // Disabled pending Custom drafts do not publish overlapping raw IDs.
        let (status, body) = harness.chat(CUSTOM_OVERLAP_RAW_ID).await;
        assert_ne!(
            status,
            StatusCode::OK,
            "unmapped raw id {CUSTOM_OVERLAP_RAW_ID} must fail closed rather than fall through to Go: {body}"
        );
        assert_ne!(
            error_type(&body),
            Some(AMBIGUOUS_ERROR_TYPE),
            "live registry has no overlapping raw ids; {CUSTOM_OVERLAP_RAW_ID} is not an invented Custom ambiguous route: {body}"
        );
    } else {
        for (raw, _) in overlaps {
            let (status, body) = harness.chat(&raw).await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "overlapping raw id {raw} must fail closed: {body}"
            );
            assert_eq!(
                error_type(&body),
                Some(AMBIGUOUS_ERROR_TYPE),
                "overlapping raw id {raw} must return {AMBIGUOUS_ERROR_TYPE}: {body}"
            );
            assert!(
                error_message(&body).to_ascii_lowercase().contains("alias"),
                "ambiguous error should point the client at an alias: {body}"
            );
        }
    }
    assert!(
        harness
            .fake_call_keys()
            .into_iter()
            .all(|key| key != GO_ACCOUNT_KEY && key != CUSTOM_ACCOUNT_KEY),
        "ambiguous or unmapped raw ids must not call any upstream: {:?}",
        harness.fake_calls()
    );
    harness.shutdown();
}

/// Zen Free stays anonymous and does not send an account Key.
#[tokio::test]
async fn zen_free_explicit_free_model_stays_anonymous() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let _go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let revision = harness.settings_revision().await;
    let (status, body) = harness
        .patch_json(
            "/providers/zen-free",
            &json!({
                "enabled": true,
                "expected_revision": revision
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = harness.chat(FREE_MODEL).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let keys = harness.fake_call_keys();
    assert_eq!(
        keys,
        vec![String::new()],
        "Zen Free must remain anonymous and must not rotate a Go Key: {keys:?}"
    );
    let calls = harness.fake_calls();
    assert!(
        calls.iter().all(|call| {
            call.authorization.is_none()
                && call.x_api_key.is_none()
                && call.x_goog_api_key.is_none()
        }),
        "Zen Free leaked an auth header: {calls:?}"
    );
    harness.shutdown();
}

/// Go import stays immediately routable; verification is not required.
#[tokio::test]
async fn go_import_remains_immediately_routable_without_verification() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let account = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    assert_eq!(account["enabled"], true, "{account}");
    assert_eq!(account["setup_step"], "ready", "{account}");
    let status = account["verification_status"]
        .as_str()
        .unwrap_or("not_required");
    assert_eq!(
        status, "not_required",
        "Go import must not require connection verification: {account}"
    );
    let (chat_status, body) = harness.chat(GO_ALIAS).await;
    assert_eq!(chat_status, StatusCode::OK, "{body}");
    harness.shutdown();
}

/// GOAT is live without directory verification; Custom remains an optional-verification draft.
#[tokio::test]
async fn goat_creates_live_while_custom_creates_a_pending_draft() {
    let harness = V2Harness::start().await;
    let catalog = harness.catalog().await;

    let goat = catalog_entry(&catalog, COMMAND_CODE_PROVIDER_ID)
        .expect("catalog must include command-code/goat");
    assert_eq!(
        goat["verification_policy"].as_str(),
        Some("not_required"),
        "Command Code's public catalog must not be presented as Key verification: {goat}"
    );
    assert_eq!(
        goat["creation_availability"].as_str(),
        Some("available"),
        "GOAT accounts must be creatable: {goat}"
    );
    let (status, body) = harness
        .create_account(json!({
            "provider_id": COMMAND_CODE_PROVIDER_ID,
            "name": "goat-live",
            "key": GOAT_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["enabled"], true,
        "GOAT must be immediately eligible when ready and keyed: {body}"
    );
    assert_eq!(
        body["verification_status"].as_str(),
        Some("not_required"),
        "GOAT directory refresh is not account verification: {body}"
    );
    assert_eq!(
        body["key"], "",
        "account JSON must not return the Key: {body}"
    );

    let (status, body) = harness
        .create_account(custom_create_payload(
            "custom-draft",
            CUSTOM_ACCOUNT_KEY,
            harness.settings_revision().await,
            &harness.upstream_base_url,
            CUSTOM_UNROUTABLE_MODEL_ID,
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["enabled"], true,
        "Custom creates enabled while verification stays pending: {body}"
    );
    assert_eq!(
        body["verification_status"].as_str(),
        Some("pending"),
        "Custom draft verification_status: {body}"
    );
    assert_eq!(
        body["plan_routable"], true,
        "Custom is catalog-routable: {body}"
    );
    assert_eq!(
        body["custom_config"]["endpoint_url"]
            .as_str()
            .map(|value| value.trim_end_matches("/chat/completions")),
        Some(harness.upstream_base_url.trim_end_matches('/')),
        "Custom create must persist the complete custom_config.endpoint_url: {body}"
    );
    let capabilities = body["model_capabilities"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        capabilities.iter().any(|item| {
            item["public_model"] == CUSTOM_UNROUTABLE_MODEL_ID
                && item["upstream_model"] == CUSTOM_UNROUTABLE_MODEL_ID
        }),
        "Custom create must persist model_capabilities: {body}"
    );
    harness.shutdown();
}

/// Explicitly disabled GOAT accounts must not be selected when a shared alias is requested.
#[tokio::test]
async fn disabled_goat_is_not_selected_for_alias_routing() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY, GOAT_ACCOUNT_KEY]).await;
    let go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let (status, goat) = harness
        .create_account(json!({
            "provider_id": COMMAND_CODE_PROVIDER_ID,
            "name": "goat-disabled",
            "key": GOAT_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{goat}");
    assert_eq!(goat["enabled"], true, "{goat}");
    let (status, goat) = harness
        .patch_json(
            &format!("/accounts/{}", goat["id"].as_str().unwrap()),
            &json!({ "enabled": false }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{goat}");
    assert_eq!(goat["enabled"], false, "{goat}");
    reorder_account_first(&harness, go["id"].as_str().unwrap()).await;

    let (status, body) = harness.chat(GO_ALIAS).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(harness.fake_call_keys(), vec![GO_ACCOUNT_KEY.to_string()]);
    let logs = harness.forward_logs().await;
    let item = &logs["items"]
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or_else(|| panic!("expected a forward log: {logs}"));
    assert_eq!(item["account_id"], go["id"]);
    assert_ne!(item["account_id"], goat["id"]);
    harness.shutdown();
}

/// GOAT verification is not applicable because its public catalog is not a Key check.
#[tokio::test]
async fn goat_account_reports_verification_not_applicable() {
    let harness = V2Harness::start().await;
    let (status, account) = harness
        .create_account(json!({
            "provider_id": COMMAND_CODE_PROVIDER_ID,
            "name": "goat-verify",
            "key": GOAT_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{account}");
    assert_eq!(account["enabled"], true, "{account}");
    assert_eq!(
        account["verification_status"].as_str(),
        Some("not_required"),
        "{account}"
    );
    let goat_id = account["id"].as_str().expect("account id").to_string();
    let catalog = harness.catalog().await;
    let goat = catalog_entry(&catalog, COMMAND_CODE_PROVIDER_ID).unwrap();
    assert_eq!(
        goat["verification_runtime_availability"].as_str(),
        Some("not_applicable")
    );

    let stored = harness.account_by_id(&goat_id).await;
    assert_eq!(
        stored["enabled"], true,
        "GOAT account enabled state must not be gated by directory refresh: {stored}"
    );
    assert_eq!(
        stored["verification_status"].as_str(),
        Some("not_required"),
        "GOAT account must not expose a pending Key-verification state: {stored}"
    );
    assert!(
        stored["connection_verified_at"].is_null()
            || stored
                .get("connection_verified_at")
                .is_none_or(|value| value.as_str().is_none_or(|stamp| stamp.is_empty())),
        "connection_verified_at must remain unset: {stored}"
    );
    assert_eq!(stored["key"], "", "{stored}");
    harness.shutdown();
}

/// Account Keys stay out of dashboard JSON, errors, and logs.
#[tokio::test]
async fn account_secrets_absent_from_json_errors_and_logs() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let account = harness.create_go_account("go-secret", GO_ACCOUNT_KEY).await;
    assert_eq!(account["key"], "");
    assert_eq!(account["password"], "");
    assert!(!json_contains_secret(&account, GO_ACCOUNT_KEY));

    let listed = harness.accounts().await;
    assert!(!json_contains_secret(&listed, GO_ACCOUNT_KEY));

    let (status, unknown) = harness.chat("definitely-not-a-model-or-alias").await;
    assert_ne!(status, StatusCode::UNAUTHORIZED, "{unknown}");
    assert!(!json_contains_secret(&unknown, GO_ACCOUNT_KEY));

    let _ = harness.chat(GO_ALIAS).await;
    let logs = harness.forward_logs().await;
    let gateway_logs = harness.gateway_logs().await;
    assert!(!json_contains_secret(&logs, GO_ACCOUNT_KEY), "{logs}");
    assert!(
        !json_contains_secret(&gateway_logs, GO_ACCOUNT_KEY),
        "{gateway_logs}"
    );
    assert!(!json_contains_secret(&logs, GATEWAY_KEY), "{logs}");

    let (conn_status, connection) = harness.get_json("/connection").await;
    assert_eq!(conn_status, StatusCode::OK, "{connection}");
    assert!(
        !json_contains_secret(&connection, GO_ACCOUNT_KEY),
        "connection info must not include the account Key: {connection}"
    );
    harness.shutdown();
}

/// After the client has seen output, alias routing must not hop accounts.
#[tokio::test]
async fn alias_stream_does_not_cross_account_retry_after_output() {
    let harness = start_v2_with_disconnect_upstream().await;
    let first = harness.create_go_account("go-one", GO_ACCOUNT_KEY).await;
    let _second = harness.create_go_account("go-two", GO_ACCOUNT_KEY_2).await;
    reorder_account_first(&harness, first["id"].as_str().unwrap()).await;

    let response = harness
        .client
        .post(harness.gateway("/v1/chat/completions"))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {GATEWAY_KEY}"),
        )
        .json(&json!({
            "model": GO_ALIAS,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(
        body.contains("ok") || body.contains("delta"),
        "client must have seen output before the disconnect: {body}"
    );
    assert_eq!(
        harness.disconnect_call_count(),
        1,
        "output already started; upstream must not be retried on another account"
    );

    let logs = harness.forward_logs().await;
    let items = logs["items"].as_array().cloned().unwrap_or_default();
    let account_ids: Vec<String> = items
        .iter()
        .filter_map(|item| item["account_id"].as_str().map(str::to_string))
        .collect();
    let unique: std::collections::HashSet<_> = account_ids.iter().cloned().collect();
    assert_eq!(
        unique.len(),
        1,
        "output already started; must not retry on another account: {items:?}"
    );
    assert_eq!(
        account_ids.first().map(String::as_str),
        first["id"].as_str()
    );
    harness.shutdown();
}
