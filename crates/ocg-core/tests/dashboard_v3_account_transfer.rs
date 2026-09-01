//! Dashboard V3 encrypted node migration: secrecy, preview, stable-ID merge,
//! atomic import, and destination-first account ordering.

use chrono::Utc;
use ocg_core::provider::{
    COMMAND_CODE_PROVIDER_ID, CUSTOM_PROVIDER_ID, ConnectionVerificationStatus,
    OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID, ZEN_FREE_ACCOUNT_ID,
};
use reqwest::header::CACHE_CONTROL;
use reqwest::{Method, StatusCode};
use serde_json::{Map, Value, json};
use std::time::Duration;

#[path = "fixtures/dashboard_v3/harness.rs"]
mod harness;

use harness::{V3Harness, start_loopback, start_public};

const BUNDLE_PASSWORD: &str = "migration-password-123";
const PUBLIC_ADMIN_PASSWORD: &str = "public-admin-password-123";
const GO_KEY: &str = "sk-transfer-go";
const CUSTOM_KEY: &str = "custom-transfer-key";
const GOAT_KEY: &str = "goat-transfer-key";

fn cas(harness: &V3Harness, patch: Value) -> Value {
    let mut body = match patch {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    body.insert(
        "expectedRevision".into(),
        json!(harness.state.settings_revision()),
    );
    body.insert(
        "processGeneration".into(),
        json!(harness.state.process_generation()),
    );
    Value::Object(body)
}

async fn send_json(
    harness: &V3Harness,
    method: Method,
    path: &str,
    body: &Value,
) -> (StatusCode, reqwest::header::HeaderMap, Value) {
    let response = harness
        .client
        .request(method, format!("{}{path}", harness.v3_base))
        .json(body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.json().await.unwrap_or(Value::Null);
    (status, headers, body)
}

fn assert_no_store(headers: &reqwest::header::HeaderMap) {
    assert_eq!(
        headers
            .get(CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
}

async fn create_source_accounts(harness: &V3Harness) {
    let (status, _, body) = send_json(
        harness,
        Method::POST,
        "/accounts",
        &cas(harness, json!({ "name": "Migrated Go", "key": GO_KEY })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, _, body) = send_json(
        harness,
        Method::POST,
        "/accounts",
        &cas(
            harness,
            json!({
                "name": "Migrated Custom",
                "key": CUSTOM_KEY,
                "providerId": CUSTOM_PROVIDER_ID,
                "customConfig": {
                    "endpointUrl": "https://api.example.com/v1/messages",
                    "upstreamProtocol": "messages"
                },
                "modelCapabilities": [{
                    "modelId": "org/migrated-model",
                    "protocol": "messages"
                }]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, _, body) = send_json(
        harness,
        Method::POST,
        "/accounts",
        &cas(
            harness,
            json!({
                "name": "Migrated GOAT",
                "key": GOAT_KEY,
                "providerId": COMMAND_CODE_PROVIDER_ID,
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

fn custom_pending_but_enabled(harness: &V3Harness) -> String {
    let (id, enabled) = harness
        .state
        .db
        .lock()
        .list_accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.provider_id == CUSTOM_PROVIDER_ID)
        .map(|account| (account.id, account.enabled))
        .unwrap();
    assert!(enabled, "Custom creation should default to enabled");
    assert_eq!(
        harness
            .state
            .db
            .lock()
            .account_verification_state(&id)
            .unwrap()
            .unwrap()
            .status,
        ConnectionVerificationStatus::Pending
    );
    id
}

#[tokio::test]
async fn encrypted_account_migration_moves_keys_without_exposing_them() {
    let source = start_loopback("account-transfer-source").await;

    let oversized = source
        .client
        .post(format!("{}/accounts/transfer/preview", source.v3_base))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body("x".repeat(4 * 1024 * 1024 + 1))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_no_store(oversized.headers());

    create_source_accounts(&source).await;
    let custom_id = custom_pending_but_enabled(&source);
    let source_go_id = source
        .state
        .db
        .lock()
        .list_accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.provider_id == OPENCODE_PROVIDER_ID)
        .unwrap()
        .id;
    let source_goat_id = source
        .state
        .db
        .lock()
        .list_accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.provider_id == COMMAND_CODE_PROVIDER_ID)
        .unwrap()
        .id;
    let source_primary = source.state.config().gateway_key;
    let source_sub_key = {
        let _settings = source.state.settings_update.lock();
        ocg_core::gateway_keys::create_sub_key(&source.state, "Migrated client").unwrap()
    };

    let (status, headers, body) = send_json(
        &source,
        Method::POST,
        "/accounts/transfer/export",
        &json!({
            "bundlePassword": BUNDLE_PASSWORD
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_no_store(&headers);
    assert_eq!(body["exportedAccounts"], 3);
    assert_eq!(body["skippedAccounts"], 0);
    let encoded = body.to_string();
    assert!(!encoded.contains(GO_KEY));
    assert!(!encoded.contains(CUSTOM_KEY));
    assert!(!encoded.contains(GOAT_KEY));
    let bundle = body["bundle"].as_str().unwrap().to_string();

    let (status, headers, body) = send_json(
        &source,
        Method::POST,
        "/accounts/transfer/export",
        &json!({
            "bundlePassword": "too-short"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_no_store(&headers);

    let target = start_loopback("account-transfer-target").await;
    let (status, _, body) = send_json(
        &target,
        Method::POST,
        "/accounts",
        &cas(
            &target,
            json!({ "name": "Migrated Go", "key": "sk-target-extra" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let target_extra_id = body["account"]["id"].as_str().unwrap().to_string();
    let (status, headers, preview) = send_json(
        &target,
        Method::POST,
        "/accounts/transfer/preview",
        &json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_no_store(&headers);
    assert_eq!(preview["importableAccounts"], 3);
    assert_eq!(preview["duplicateAccounts"], 0);
    assert_eq!(preview["items"].as_array().unwrap().len(), 3);
    assert!(!preview.to_string().contains(GO_KEY));
    assert!(!preview.to_string().contains(CUSTOM_KEY));

    let preview_url = format!("{}/accounts/transfer/preview", target.v3_base);
    let first = target
        .client
        .post(&preview_url)
        .json(&json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }));
    let second = target
        .client
        .post(&preview_url)
        .json(&json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }));
    let (first, second) = tokio::join!(first.send(), second.send());
    let first = first.unwrap();
    let second = second.unwrap();
    assert_no_store(first.headers());
    assert_no_store(second.headers());
    let mut statuses = [first.status(), second.status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::SERVICE_UNAVAILABLE]);

    let stale_request = cas(
        &target,
        json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
    );
    let client = target.client.clone();
    let import_url = format!("{}/accounts/transfer/import", target.v3_base);
    let stale_import = tokio::spawn(async move {
        client
            .post(import_url)
            .json(&stale_request)
            .send()
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    target.state.bump_settings_revision();
    let stale_import = stale_import.await.unwrap();
    assert_eq!(stale_import.status(), StatusCode::CONFLICT);
    assert_no_store(stale_import.headers());
    assert_eq!(target.state.db.lock().list_accounts().unwrap().len(), 2);

    let before = target.state.settings_revision();
    let (status, headers, imported) = send_json(
        &target,
        Method::POST,
        "/accounts/transfer/import",
        &cas(
            &target,
            json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{imported}");
    assert_no_store(&headers);
    assert_eq!(imported["importedAccounts"], 3);
    assert_eq!(imported["duplicateAccounts"], 0);
    assert_eq!(target.state.settings_revision(), before + 1);
    assert!(!imported.to_string().contains(GO_KEY));
    assert!(!imported.to_string().contains(CUSTOM_KEY));

    let accounts = target.state.db.lock().list_accounts().unwrap();
    let ordinary: Vec<_> = accounts
        .iter()
        .filter(|account| account.provider_id != OPENCODE_ZEN_FREE_PROVIDER_ID)
        .collect();
    assert_eq!(ordinary.len(), 4);
    assert_eq!(
        accounts
            .iter()
            .map(|account| account.id.as_str())
            .collect::<Vec<_>>(),
        [
            ZEN_FREE_ACCOUNT_ID,
            target_extra_id.as_str(),
            source_go_id.as_str(),
            custom_id.as_str(),
            source_goat_id.as_str(),
        ]
    );
    let go = ordinary
        .iter()
        .find(|account| account.id == source_go_id)
        .unwrap();
    assert_eq!(target.state.decrypt_key(&go.key_cipher).unwrap(), GO_KEY);
    assert!(go.enabled);
    let target_extra = ordinary
        .iter()
        .find(|account| account.id == target_extra_id)
        .unwrap();
    assert_eq!(
        target.state.decrypt_key(&target_extra.key_cipher).unwrap(),
        "sk-target-extra"
    );
    let custom = ordinary
        .iter()
        .find(|account| account.id == custom_id)
        .unwrap();
    assert_eq!(
        target.state.decrypt_key(&custom.key_cipher).unwrap(),
        CUSTOM_KEY
    );
    assert!(
        custom.enabled,
        "pending Custom accounts should remain usable"
    );
    let goat = ordinary
        .iter()
        .find(|account| account.id == source_goat_id)
        .unwrap();
    assert_eq!(
        target.state.decrypt_key(&goat.key_cipher).unwrap(),
        GOAT_KEY
    );
    let custom_contract = target
        .state
        .db
        .lock()
        .load_account_contract(&custom.id)
        .unwrap();
    assert_eq!(custom_contract.model_capabilities[0].source, "import");
    let (_, listed) = target
        .get_json(&format!("{}/accounts", target.v3_base))
        .await;
    let custom_view = listed["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["providerId"] == CUSTOM_PROVIDER_ID)
        .unwrap();
    assert_eq!(custom_view["verificationStatus"], "pending");

    assert_eq!(target.state.config().gateway_key, source_primary);
    let target_sub_keys = target
        .state
        .db
        .lock()
        .list_active_sub_gateway_keys()
        .unwrap();
    let migrated_sub_key = target_sub_keys
        .iter()
        .find(|key| key.id == source_sub_key.id)
        .unwrap();
    assert_eq!(migrated_sub_key.name, source_sub_key.name);
    assert_eq!(migrated_sub_key.key, source_sub_key.key);
    assert_eq!(migrated_sub_key.enabled, source_sub_key.enabled);

    let revision = target.state.settings_revision();
    let (status, _, duplicate) = send_json(
        &target,
        Method::POST,
        "/accounts/transfer/import",
        &cas(
            &target,
            json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{duplicate}");
    assert_eq!(duplicate["importedAccounts"], 3);
    assert_eq!(duplicate["duplicateAccounts"], 0);
    assert!(
        duplicate["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["disposition"] == "merged")
    );
    assert_eq!(target.state.settings_revision(), revision + 1);
    assert_eq!(
        target
            .state
            .db
            .lock()
            .list_accounts()
            .unwrap()
            .iter()
            .map(|account| account.id.as_str())
            .collect::<Vec<_>>(),
        [
            ZEN_FREE_ACCOUNT_ID,
            target_extra_id.as_str(),
            source_go_id.as_str(),
            custom_id.as_str(),
            source_goat_id.as_str(),
        ]
    );

    let (status, headers, body) = send_json(
        &target,
        Method::POST,
        "/accounts/transfer/preview",
        &json!({ "password": "wrong-bundle-password", "bundle": bundle }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_no_store(&headers);
    assert!(!body.to_string().contains(GO_KEY));
    assert!(!body.to_string().contains(CUSTOM_KEY));
    assert!(!body.to_string().contains(GOAT_KEY));

    let public = start_public("account-transfer-public").await;
    let unauthorized = public
        .client
        .post(format!("{}/accounts/transfer/preview", public.v3_base))
        .json(&json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert_no_store(unauthorized.headers());
    let registered = public
        .client
        .post(format!("{}/auth/register", public.v2_base))
        .json(&json!({
            "username": "admin",
            "password": PUBLIC_ADMIN_PASSWORD
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(registered.status(), StatusCode::CREATED);
    let cookie = registered
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let export_body = json!({
        "bundlePassword": BUNDLE_PASSWORD
    });
    let insecure = public
        .client
        .post(format!("{}/accounts/transfer/export", public.v3_base))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&export_body)
        .send()
        .await
        .unwrap();
    assert_eq!(insecure.status(), StatusCode::FORBIDDEN);
    assert_no_store(insecure.headers());
    let spoofed_https = public
        .client
        .post(format!("{}/accounts/transfer/export", public.v3_base))
        .header(reqwest::header::COOKIE, cookie)
        .header("x-forwarded-proto", "https")
        .json(&export_body)
        .send()
        .await
        .unwrap();
    assert_eq!(spoofed_https.status(), StatusCode::FORBIDDEN);
    assert_no_store(spoofed_https.headers());

    let collision_target = start_loopback("account-transfer-key-collision-target").await;
    let collision_primary = collision_target.state.config().gateway_key;
    let target_conflict = ocg_core::models::SubGatewayKey {
        id: "destination-only-key".into(),
        name: "Destination client".into(),
        key: source_sub_key.key.clone(),
        enabled: true,
        deleted_at: None,
        created_at: Utc::now(),
    };
    collision_target
        .state
        .db
        .lock()
        .insert_sub_gateway_key(&target_conflict)
        .unwrap();
    let before_order = collision_target
        .state
        .db
        .lock()
        .list_accounts()
        .unwrap()
        .into_iter()
        .map(|account| account.id)
        .collect::<Vec<_>>();
    let (status, headers, body) = send_json(
        &collision_target,
        Method::POST,
        "/accounts/transfer/preview",
        &json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_no_store(&headers);
    assert!(!body.to_string().contains(&source_sub_key.key));
    let (status, headers, body) = send_json(
        &collision_target,
        Method::POST,
        "/accounts/transfer/import",
        &cas(
            &collision_target,
            json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_no_store(&headers);
    assert_eq!(
        collision_target.state.config().gateway_key,
        collision_primary
    );
    assert_eq!(
        collision_target
            .state
            .db
            .lock()
            .primary_access_key_value()
            .unwrap()
            .as_deref(),
        Some(collision_primary.as_str())
    );
    let target_keys = collision_target
        .state
        .db
        .lock()
        .list_active_sub_gateway_keys()
        .unwrap();
    assert_eq!(target_keys.len(), 1);
    assert_eq!(target_keys[0].id, target_conflict.id);
    assert!(target_keys.iter().all(|key| key.id != source_sub_key.id));
    assert_eq!(
        collision_target
            .state
            .db
            .lock()
            .list_accounts()
            .unwrap()
            .into_iter()
            .map(|account| account.id)
            .collect::<Vec<_>>(),
        before_order
    );

    source.stop();
    target.stop();
    public.stop();
    collision_target.stop();
}
