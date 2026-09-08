//! Dynamic Provider persistence, V3 control plane, and routing snapshot tests.

use chrono::Utc;
use ocg_core::dashboard_v3::ERROR_INVALID_REQUEST;
use ocg_core::models::ProxyMode;
use ocg_core::provider::{CUSTOM_PROVIDER_ID, OPENCODE_PROVIDER_ID};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

#[allow(dead_code)]
#[path = "fixtures/fake_upstream.rs"]
mod fake_upstream;
#[path = "fixtures/dashboard_v3/harness.rs"]
mod harness;

use fake_upstream::{FakeReply, start_fake_upstream, start_fake_upstream_with_delay};
use harness::{V3Harness, start_loopback};

const CHAT_OK: &str = r#"{"id":"ok","object":"chat.completion","model":"vendor/opus","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#;

fn cas(harness: &V3Harness, patch: Value) -> Value {
    let mut body = patch.as_object().cloned().unwrap_or_default();
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
) -> (StatusCode, Value) {
    let response = harness
        .client
        .request(method, format!("{}{path}", harness.v3_base))
        .json(body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let parsed = response.json().await.unwrap_or(Value::Null);
    (status, parsed)
}

fn create_body(name: &str, endpoint: &str, protocol: &str, auth: &str, key: Option<&str>) -> Value {
    let mut body = json!({
        "name": name,
        "endpointUrl": endpoint,
        "upstreamProtocol": protocol,
        "authKind": auth,
        "models": [{
            "publicModel": "lab-opus",
            "upstreamModel": "vendor/opus"
        }]
    });
    if let Some(key) = key {
        body["key"] = json!(key);
    }
    body
}

async fn chat_completion(harness: &V3Harness, model: &str) -> (StatusCode, String) {
    let response = harness
        .client
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            harness.handle.port
        ))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", harness.state.config().gateway_key),
        )
        .json(&json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1
        }))
        .send()
        .await
        .unwrap();
    (response.status(), response.text().await.unwrap())
}

async fn listed_gateway_model_ids(harness: &V3Harness) -> Vec<String> {
    let models = harness
        .client
        .get(format!(
            "http://127.0.0.1:{}/v1/models",
            harness.handle.port
        ))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", harness.state.config().gateway_key),
        )
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    models["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn create_patch_delete_and_cas_conflict() {
    let harness = start_loopback("dyn-cas").await;
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            create_body(
                "Lab",
                "http://127.0.0.1:9",
                "chat_completions",
                "bearer",
                Some("sk-lab"),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let provider_id = created["provider"]["id"].as_str().unwrap().to_string();
    assert!(created["provider"].get("key").is_none());
    let revision = created["revision"].as_u64().unwrap();

    let (status, conflict) = send_json(
        &harness,
        Method::PATCH,
        &format!("/providers/{provider_id}"),
        &json!({
            "expectedRevision": revision.saturating_sub(1),
            "processGeneration": harness.state.process_generation(),
            "name": "Hacked",
            "endpointUrl": "http://127.0.0.1:9",
            "upstreamProtocol": "chat_completions",
            "authKind": "bearer",
            "models": [{"publicModel": "lab-opus", "upstreamModel": "vendor/opus"}]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    let stored = harness
        .state
        .db
        .lock()
        .get_dynamic_provider(&provider_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.name, "Lab");

    let (status, deleted) = send_json(
        &harness,
        Method::DELETE,
        &format!("/providers/{provider_id}"),
        &cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{deleted}");

    let accounts = harness.state.db.lock().list_accounts().unwrap();
    let account_id = accounts
        .iter()
        .find(|account| account.provider_id == provider_id)
        .unwrap()
        .id
        .clone();
    harness.state.db.lock().delete_account(&account_id).unwrap();
    let (status, ack) = send_json(
        &harness,
        Method::DELETE,
        &format!("/providers/{provider_id}"),
        &cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ack}");
    assert!(
        harness
            .state
            .db
            .lock()
            .get_dynamic_provider(&provider_id)
            .unwrap()
            .is_none()
    );

    let (status, builtin) = send_json(
        &harness,
        Method::DELETE,
        &format!("/providers/{OPENCODE_PROVIDER_ID}"),
        &cas(&harness, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{builtin}");
    harness.stop();
}

#[tokio::test]
async fn protocols_auth_kinds_and_no_auth_singleton() {
    let harness = start_loopback("dyn-kinds").await;
    for (protocol, auth, key) in [
        ("chat_completions", "bearer", Some("sk-a")),
        ("responses", "x-api-key", Some("sk-b")),
        ("messages", "none", None),
    ] {
        let (status, body) = send_json(
            &harness,
            Method::POST,
            "/providers",
            &cas(
                &harness,
                create_body(
                    &format!("{protocol}-{auth}"),
                    "http://127.0.0.1:9",
                    protocol,
                    auth,
                    key,
                ),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        if auth == "none" {
            let provider_id = body["provider"]["id"].as_str().unwrap().to_string();
            let (status, second) = send_json(
                &harness,
                Method::POST,
                "/accounts",
                &cas(
                    &harness,
                    json!({
                        "name": "second",
                        "providerId": provider_id,
                        "key": "sk-should-fail"
                    }),
                ),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{second}");
        }
    }
    harness.stop();
}

#[tokio::test]
async fn two_keyed_accounts_select_and_fallback() {
    let mut replies = HashMap::new();
    replies.insert(
        "sk-first".to_string(),
        VecDeque::from([FakeReply {
            status: 401,
            body: r#"{"error":{"message":"bad"}}"#,
        }]),
    );
    replies.insert(
        "sk-second".to_string(),
        VecDeque::from([FakeReply {
            status: 200,
            body: CHAT_OK,
        }]),
    );
    let (upstream, _calls, _stop) = start_fake_upstream(replies).await;
    let harness = start_loopback("dyn-fallback").await;
    let mut config = harness.state.config();
    config.proxy_mode = ProxyMode::Direct;
    harness.state.set_config(config).unwrap();
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            create_body(
                "Fallback",
                &format!("{upstream}/v1"),
                "chat_completions",
                "bearer",
                Some("sk-first"),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let provider_id = created["provider"]["id"].as_str().unwrap().to_string();
    let (status, second) = send_json(
        &harness,
        Method::POST,
        "/accounts",
        &cas(
            &harness,
            json!({
                "name": "second",
                "providerId": provider_id,
                "key": "sk-second"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    let second_account_id = second["account"]["id"].as_str().unwrap().to_string();
    let (gw_status, body) = chat_completion(&harness, "lab-opus").await;
    assert_eq!(gw_status, StatusCode::OK, "{body}");
    let logs = harness.state.db.lock().list_forward_logs(8).unwrap();
    let attributed: Vec<_> = logs
        .iter()
        .filter(|row| row.provider_id.as_deref() == Some(provider_id.as_str()))
        .collect();
    assert!(
        attributed.iter().any(|row| {
            !row.account_id.is_empty()
                && (row.http_status == Some(401) || row.status.contains("error"))
        }),
        "expected first-account failure log: {logs:?}"
    );
    assert!(
        attributed.iter().any(|row| {
            !row.account_id.is_empty()
                && (row.http_status == Some(200) || row.status.contains("success"))
        }),
        "expected fallback success log: {logs:?}"
    );
    let account_ids: HashSet<_> = attributed
        .iter()
        .map(|row| row.account_id.as_str())
        .filter(|id| !id.is_empty())
        .collect();
    assert!(
        account_ids.len() >= 2 && account_ids.contains(second_account_id.as_str()),
        "expected log attribution across accounts, second={second_account_id}, logs={logs:?}"
    );
    harness.stop();
}

#[tokio::test]
async fn none_auth_dynamic_provider_forwards_without_upstream_auth() {
    let mut replies = HashMap::new();
    replies.insert(
        String::new(),
        VecDeque::from([FakeReply {
            status: 200,
            body: CHAT_OK,
        }]),
    );
    let (upstream, calls, _stop) = start_fake_upstream(replies).await;
    let harness = start_loopback("dyn-none-auth-forward").await;
    let mut config = harness.state.config();
    config.proxy_mode = ProxyMode::Direct;
    harness.state.set_config(config).unwrap();
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            create_body(
                "OpenLab",
                &format!("{upstream}/v1"),
                "chat_completions",
                "none",
                None,
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");

    let response = harness
        .client
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            harness.handle.port
        ))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", harness.state.config().gateway_key),
        )
        .header("x-opencode-session", "ses_client")
        .header("x-opencode-client", "cli")
        .header("x-opencode-request", "req_client")
        .header("x-opencode-project", "proj_client")
        .header("x-session-id", "ses_id")
        .header("x-session-affinity", "ses_aff")
        .json(&json!({
            "model": "lab-opus",
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1
        }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");

    let captured = calls.lock().expect("fake call log").clone();
    assert_eq!(captured.len(), 1, "{captured:?}");
    let call = &captured[0];
    assert!(call.authorization.is_none(), "{call:?}");
    assert!(call.x_api_key.is_none(), "{call:?}");
    assert!(call.x_goog_api_key.is_none(), "{call:?}");
    assert!(call.key.is_empty(), "{call:?}");
    assert!(call.opencode_session.is_none(), "{call:?}");
    assert!(call.opencode_client.is_none(), "{call:?}");
    assert!(call.opencode_request.is_none(), "{call:?}");
    assert!(call.opencode_project.is_none(), "{call:?}");
    assert!(call.session_id.is_none(), "{call:?}");
    assert!(call.session_affinity.is_none(), "{call:?}");
    harness.stop();
}

#[tokio::test]
async fn raw_ambiguity_makes_zero_outbound_requests() {
    let (upstream, calls, _stop) = start_fake_upstream(HashMap::new()).await;
    let harness = start_loopback("dyn-ambiguous").await;
    let mut config = harness.state.config();
    config.proxy_mode = ProxyMode::Direct;
    harness.state.set_config(config).unwrap();
    for name in ["A", "B"] {
        let (status, created) = send_json(
            &harness,
            Method::POST,
            "/providers",
            &cas(
                &harness,
                json!({
                    "name": name,
                    "endpointUrl": format!("{upstream}/v1"),
                    "upstreamProtocol": "chat_completions",
                    "authKind": "bearer",
                    "key": format!("sk-{name}"),
                    "models": [{"publicModel": format!("{name}-pub"), "upstreamModel": "shared/raw"}]
                }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
    }
    let (status, body) = chat_completion(&harness, "shared/raw").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    assert_eq!(parsed["error"]["type"], "ambiguous_model_id", "{body}");
    assert!(calls.lock().expect("fake call log").is_empty());
    let ids = listed_gateway_model_ids(&harness).await;
    assert!(
        !ids.iter().any(|id| id == "shared/raw"),
        "ambiguous raw id must stay unpublished: {ids:?}"
    );
    harness.stop();
}

#[tokio::test]
async fn raw_shaped_public_models_are_listed_under_public_name_only() {
    let mut replies = HashMap::new();
    replies.insert(
        "sk-lab".to_string(),
        VecDeque::from([FakeReply {
            status: 200,
            body: CHAT_OK,
        }]),
    );
    let (upstream, calls, _stop) = start_fake_upstream(replies).await;
    let harness = start_loopback("dyn-raw-public").await;
    let mut config = harness.state.config();
    config.proxy_mode = ProxyMode::Direct;
    harness.state.set_config(config).unwrap();
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            json!({
                "name": "RawLab",
                "endpointUrl": format!("{upstream}/v1"),
                "upstreamProtocol": "chat_completions",
                "authKind": "bearer",
                "key": "sk-lab",
                "models": [
                    {"publicModel": "org/same", "upstreamModel": "org/same"},
                    {"publicModel": "org/public", "upstreamModel": "vendor/real"},
                    {"publicModel": "lab_model", "upstreamModel": "vendor/lab"},
                    {"publicModel": "lab model", "upstreamModel": "vendor/space"}
                ]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");

    let ids = listed_gateway_model_ids(&harness).await;
    for public in ["org/same", "org/public", "lab_model", "lab model"] {
        assert_eq!(
            ids.iter().filter(|id| **id == public).count(),
            1,
            "{public} missing or duplicated in {ids:?}"
        );
    }
    for leaked in ["vendor/real", "vendor/lab", "vendor/space"] {
        assert!(
            !ids.iter().any(|id| id == leaked),
            "differing upstream leaked into /v1/models: {leaked} in {ids:?}"
        );
    }

    let (status, body) = chat_completion(&harness, "org/public").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    {
        let outbound = calls.lock().expect("fake call log");
        assert_eq!(outbound.len(), 1, "{outbound:?}");
        assert!(
            outbound[0].body.contains("\"model\":\"vendor/real\""),
            "public!=upstream must forward the exact upstream id: {}",
            outbound[0].body
        );
        assert!(
            !outbound[0].body.contains("\"model\":\"org/public\""),
            "public name must not replace the upstream id: {}",
            outbound[0].body
        );
    }

    let (status, same_body) = chat_completion(&harness, "org/same").await;
    assert_eq!(status, StatusCode::OK, "{same_body}");
    harness.stop();
}

#[tokio::test]
async fn public_alias_aggregates_across_dynamic_providers() {
    let mut replies = HashMap::new();
    replies.insert(
        "sk-one".to_string(),
        VecDeque::from([FakeReply {
            status: 401,
            body: r#"{"error":{"message":"bad"}}"#,
        }]),
    );
    replies.insert(
        "sk-two".to_string(),
        VecDeque::from([FakeReply {
            status: 200,
            body: CHAT_OK,
        }]),
    );
    let (upstream, _calls, _stop) = start_fake_upstream(replies).await;
    let harness = start_loopback("dyn-aggregate").await;
    let mut config = harness.state.config();
    config.proxy_mode = ProxyMode::Direct;
    harness.state.set_config(config).unwrap();
    for (name, key) in [("One", "sk-one"), ("Two", "sk-two")] {
        let (status, created) = send_json(
            &harness,
            Method::POST,
            "/providers",
            &cas(
                &harness,
                json!({
                    "name": name,
                    "endpointUrl": format!("{upstream}/v1"),
                    "upstreamProtocol": "chat_completions",
                    "authKind": "bearer",
                    "key": key,
                    "models": [{"publicModel": "shared-opus", "upstreamModel": format!("vendor/{name}")}]
                }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
    }
    let models = harness
        .client
        .get(format!(
            "http://127.0.0.1:{}/v1/models",
            harness.handle.port
        ))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", harness.state.config().gateway_key),
        )
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let ids = models["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids.iter().filter(|id| **id == "shared-opus").count(),
        1,
        "{models}"
    );
    let (status, body) = chat_completion(&harness, "shared-opus").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    harness.stop();
}

#[tokio::test]
async fn discover_and_test_do_not_persist_keys_or_providers() {
    let mut replies = HashMap::new();
    replies.insert(
        "sk-probe".to_string(),
        VecDeque::from([
            FakeReply {
                status: 200,
                body: r#"{"data":[{"id":"discovered-1"}]}"#,
            },
            FakeReply {
                status: 200,
                body: CHAT_OK,
            },
        ]),
    );
    let (upstream, _calls, _stop) = start_fake_upstream(replies).await;
    let harness = start_loopback("dyn-probe").await;
    let mut config = harness.state.config();
    config.proxy_mode = ProxyMode::Direct;
    harness.state.set_config(config).unwrap();
    let before = harness
        .state
        .db
        .lock()
        .list_dynamic_providers()
        .unwrap()
        .len();
    let (status, discovered) = send_json(
        &harness,
        Method::POST,
        "/providers/models/discover",
        &json!({
            "endpointUrl": format!("{upstream}/v1"),
            "upstreamProtocol": "chat_completions",
            "authKind": "bearer",
            "key": "sk-probe"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{discovered}");
    assert!(discovered.get("key").is_none());
    assert!(!discovered.to_string().contains("sk-probe"));
    let (status, tested) = send_json(
        &harness,
        Method::POST,
        "/providers/test",
        &json!({
            "endpointUrl": format!("{upstream}/v1"),
            "upstreamProtocol": "chat_completions",
            "authKind": "bearer",
            "publicModel": "lab-opus",
            "upstreamModel": "vendor/opus",
            "key": "sk-probe"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{tested}");
    assert!(tested.get("key").is_none());
    assert!(!tested.to_string().contains("sk-probe"));
    assert_eq!(
        harness
            .state
            .db
            .lock()
            .list_dynamic_providers()
            .unwrap()
            .len(),
        before
    );
    harness.stop();
}

#[tokio::test]
async fn patch_clears_runtime_state_and_usage_is_unpriced() {
    let harness = start_loopback("dyn-patch").await;
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            create_body(
                "PatchMe",
                "http://127.0.0.1:9",
                "chat_completions",
                "bearer",
                Some("sk-keep"),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let provider_id = created["provider"]["id"].as_str().unwrap().to_string();
    let account_id = harness
        .state
        .db
        .lock()
        .list_accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.provider_id == provider_id)
        .unwrap()
        .id;
    {
        let db = harness.state.db.lock();
        db.set_account_auth_error(&account_id, Some("stale"))
            .unwrap();
        db.set_account_cooldown(
            &account_id,
            Some(Utc::now() + chrono::Duration::hours(1)),
            Some("boom"),
        )
        .unwrap();
    }
    let (status, renamed) = send_json(
        &harness,
        Method::PATCH,
        &format!("/providers/{provider_id}"),
        &cas(
            &harness,
            json!({
                "name": "Renamed",
                "endpointUrl": "http://127.0.0.1:9",
                "upstreamProtocol": "chat_completions",
                "authKind": "bearer",
                "models": [{"publicModel": "lab-opus", "upstreamModel": "vendor/opus"}]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    let account = harness
        .state
        .db
        .lock()
        .get_account(&account_id)
        .unwrap()
        .unwrap();
    assert_eq!(account.auth_error.as_deref(), Some("stale"));
    assert_eq!(account.last_error.as_deref(), Some("boom"));
    assert!(account.cooldown_until.is_some());

    let (status, patched) = send_json(
        &harness,
        Method::PATCH,
        &format!("/providers/{provider_id}"),
        &cas(
            &harness,
            json!({
                "name": "Renamed",
                "endpointUrl": "http://127.0.0.1:10",
                "upstreamProtocol": "responses",
                "authKind": "bearer",
                "models": [{"publicModel": "lab-opus", "upstreamModel": "vendor/opus-2"}]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    let account = harness
        .state
        .db
        .lock()
        .get_account(&account_id)
        .unwrap()
        .unwrap();
    assert!(account.auth_error.is_none());
    assert!(account.last_error.is_none());
    assert!(account.cooldown_until.is_none());
    assert!(!account.key_cipher.is_empty());

    let (status, usage) = harness
        .get_json(&format!(
            "{}/accounts/{account_id}/provider-usage",
            harness.v3_base
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{usage}");
    assert_eq!(usage["availability"], "unavailable");
    let (status, pricing) = harness
        .get_json(&format!(
            "{}/providers/{provider_id}/pricing",
            harness.v3_base
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{pricing}");
    assert_eq!(pricing["availability"], "unpriced");
    harness.stop();
}

#[tokio::test]
async fn none_to_keyed_requires_replacement_key() {
    let harness = start_loopback("dyn-auth-swap").await;
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            create_body(
                "OpenLab",
                "http://127.0.0.1:9",
                "chat_completions",
                "none",
                None,
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let provider_id = created["provider"]["id"].as_str().unwrap().to_string();
    let (status, missing) = send_json(
        &harness,
        Method::PATCH,
        &format!("/providers/{provider_id}"),
        &cas(
            &harness,
            json!({
                "name": "OpenLab",
                "endpointUrl": "http://127.0.0.1:9",
                "upstreamProtocol": "chat_completions",
                "authKind": "bearer",
                "models": [{"publicModel": "lab-opus", "upstreamModel": "vendor/opus"}]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{missing}");
    let (status, patched) = send_json(
        &harness,
        Method::PATCH,
        &format!("/providers/{provider_id}"),
        &cas(
            &harness,
            json!({
                "name": "OpenLab",
                "endpointUrl": "http://127.0.0.1:9",
                "upstreamProtocol": "chat_completions",
                "authKind": "bearer",
                "key": "sk-now",
                "models": [{"publicModel": "lab-opus", "upstreamModel": "vendor/opus"}]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    let account = harness
        .state
        .db
        .lock()
        .list_accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.provider_id == provider_id)
        .unwrap();
    assert!(!account.key_cipher.is_empty());
    harness.stop();
}

#[tokio::test]
async fn keyed_provider_update_rejects_key_and_does_not_fan_out() {
    let harness = start_loopback("dyn-no-key-fanout").await;
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            create_body(
                "Keys",
                "http://127.0.0.1:9",
                "chat_completions",
                "bearer",
                Some("sk-first"),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let provider_id = created["provider"]["id"].as_str().unwrap().to_string();
    let (status, second) = send_json(
        &harness,
        Method::POST,
        "/accounts",
        &cas(
            &harness,
            json!({
                "name": "second",
                "providerId": provider_id,
                "key": "sk-second"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    let before = harness
        .state
        .db
        .lock()
        .list_accounts()
        .unwrap()
        .into_iter()
        .filter(|account| account.provider_id == provider_id)
        .map(|account| (account.id, account.key_cipher))
        .collect::<Vec<_>>();
    assert_eq!(before.len(), 2);
    let revision = harness.state.settings_revision();
    let (status, rejected) = send_json(
        &harness,
        Method::PATCH,
        &format!("/providers/{provider_id}"),
        &cas(
            &harness,
            json!({
                "name": "Keys",
                "endpointUrl": "http://127.0.0.1:9",
                "upstreamProtocol": "chat_completions",
                "authKind": "bearer",
                "key": "sk-fanout",
                "models": [{"publicModel": "lab-opus", "upstreamModel": "vendor/opus"}]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert_eq!(rejected["code"], ERROR_INVALID_REQUEST, "{rejected}");
    assert_eq!(harness.state.settings_revision(), revision);
    let after = harness
        .state
        .db
        .lock()
        .list_accounts()
        .unwrap()
        .into_iter()
        .filter(|account| account.provider_id == provider_id)
        .map(|account| (account.id, account.key_cipher))
        .collect::<Vec<_>>();
    assert_eq!(after, before);
    let first = harness.state.decrypt_key(&before[0].1).unwrap();
    let second_key = harness.state.decrypt_key(&before[1].1).unwrap();
    assert_ne!(first, second_key);
    assert!(first == "sk-first" || second_key == "sk-first");
    assert!(first == "sk-second" || second_key == "sk-second");
    harness.stop();
}

#[tokio::test]
async fn in_flight_request_keeps_frozen_snapshot_across_provider_patch() {
    let mut replies = HashMap::new();
    replies.insert(
        "sk-first".to_string(),
        VecDeque::from([FakeReply {
            status: 401,
            body: r#"{"error":{"message":"bad"}}"#,
        }]),
    );
    replies.insert(
        "sk-second".to_string(),
        VecDeque::from([FakeReply {
            status: 200,
            body: CHAT_OK,
        }]),
    );
    let (upstream, calls, _stop) =
        start_fake_upstream_with_delay(replies, Duration::from_millis(400)).await;
    let harness = start_loopback("dyn-snapshot").await;
    let mut config = harness.state.config();
    config.proxy_mode = ProxyMode::Direct;
    harness.state.set_config(config).unwrap();
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            create_body(
                "Snap",
                &format!("{upstream}/v1"),
                "chat_completions",
                "bearer",
                Some("sk-first"),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let provider_id = created["provider"]["id"].as_str().unwrap().to_string();
    let (status, second) = send_json(
        &harness,
        Method::POST,
        "/accounts",
        &cas(
            &harness,
            json!({
                "name": "second",
                "providerId": provider_id,
                "key": "sk-second"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");

    let client = harness.client.clone();
    let port = harness.handle.port;
    let gateway_key = harness.state.config().gateway_key.clone();
    let pending = tokio::spawn(async move {
        client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {gateway_key}"),
            )
            .json(&json!({
                "model": "lab-opus",
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 1
            }))
            .send()
            .await
            .unwrap()
    });
    let started = tokio::time::Instant::now();
    loop {
        if !calls.lock().expect("fake call log").is_empty() {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "in-flight request never reached the original upstream"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, patched) = send_json(
        &harness,
        Method::PATCH,
        &format!("/providers/{provider_id}"),
        &cas(
            &harness,
            json!({
                "name": "Snap",
                "endpointUrl": "http://127.0.0.1:1",
                "upstreamProtocol": "chat_completions",
                "authKind": "bearer",
                "models": [{"publicModel": "lab-opus", "upstreamModel": "vendor/opus"}]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    let response = pending.await.unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    harness.stop();
}

#[tokio::test]
async fn custom_api_still_creates_account_owned_endpoint() {
    let harness = start_loopback("dyn-custom-regression").await;
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/accounts",
        &cas(
            &harness,
            json!({
                "name": "Custom",
                "providerId": CUSTOM_PROVIDER_ID,
                "key": "sk-custom",
                "customConfig": {
                    "endpointUrl": "http://127.0.0.1:9",
                    "upstreamProtocol": "chat_completions"
                },
                "modelCapabilities": [{
                    "publicModel": "home-model",
                    "upstreamModel": "home-model",
                    "protocol": "chat_completions"
                }]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["account"]["providerId"], CUSTOM_PROVIDER_ID);
    let account_id = created["account"]["id"].as_str().unwrap();
    let custom = harness
        .state
        .db
        .lock()
        .account_custom_config(account_id)
        .unwrap()
        .unwrap();
    assert_eq!(custom.endpoint_url, "http://127.0.0.1:9");
    harness.stop();
}

async fn account_id_for_provider(harness: &V3Harness, provider_id: &str) -> String {
    harness
        .state
        .db
        .lock()
        .list_accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.provider_id == provider_id)
        .map(|account| account.id)
        .unwrap_or_else(|| panic!("missing account for {provider_id}"))
}

async fn toggle_account(harness: &V3Harness, account_id: &str) -> (StatusCode, Value) {
    send_json(
        harness,
        Method::POST,
        &format!("/accounts/{account_id}/toggle"),
        &cas(harness, json!({})),
    )
    .await
}

#[tokio::test]
async fn keyed_dynamic_account_can_disable_and_re_enable() {
    let harness = start_loopback("dyn-enable-keyed").await;
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            create_body(
                "Lab",
                "http://127.0.0.1:9",
                "chat_completions",
                "bearer",
                Some("sk-lab"),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let provider_id = created["provider"]["id"].as_str().unwrap().to_string();
    let account_id = account_id_for_provider(&harness, &provider_id).await;
    let before = harness
        .state
        .db
        .lock()
        .get_account(&account_id)
        .unwrap()
        .unwrap();
    assert!(before.enabled);
    let mut revision = harness.state.settings_revision();

    let (status, disabled) = toggle_account(&harness, &account_id).await;
    assert_eq!(status, StatusCode::OK, "{disabled}");
    assert_eq!(disabled["account"]["enabled"], false);
    assert_eq!(harness.state.settings_revision(), revision + 1);
    revision = harness.state.settings_revision();
    let after_disable = harness
        .state
        .db
        .lock()
        .get_account(&account_id)
        .unwrap()
        .unwrap();
    assert_eq!(after_disable.provider_id, before.provider_id);
    assert_eq!(after_disable.credential_kind, before.credential_kind);
    assert_eq!(after_disable.key_cipher, before.key_cipher);

    let (status, enabled) = toggle_account(&harness, &account_id).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    assert_eq!(enabled["account"]["enabled"], true);
    assert_eq!(harness.state.settings_revision(), revision + 1);
    let after_enable = harness
        .state
        .db
        .lock()
        .get_account(&account_id)
        .unwrap()
        .unwrap();
    assert_eq!(after_enable.provider_id, before.provider_id);
    assert_eq!(after_enable.credential_kind, before.credential_kind);
    assert_eq!(after_enable.key_cipher, before.key_cipher);
    harness.stop();
}

#[tokio::test]
async fn dynamic_none_auth_singleton_can_disable_and_re_enable_without_a_key() {
    let harness = start_loopback("dyn-enable-none").await;
    let (status, created) = send_json(
        &harness,
        Method::POST,
        "/providers",
        &cas(
            &harness,
            create_body(
                "OpenLab",
                "http://127.0.0.1:9",
                "chat_completions",
                "none",
                None,
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let provider_id = created["provider"]["id"].as_str().unwrap().to_string();
    let account_id = account_id_for_provider(&harness, &provider_id).await;
    let stored = harness
        .state
        .db
        .lock()
        .get_account(&account_id)
        .unwrap()
        .unwrap();
    assert!(stored.enabled);
    assert!(stored.key_cipher.is_empty());
    assert_eq!(
        stored.credential_kind,
        ocg_core::provider::CredentialKind::None
    );
    let mut revision = harness.state.settings_revision();

    let (status, disabled) = toggle_account(&harness, &account_id).await;
    assert_eq!(status, StatusCode::OK, "{disabled}");
    assert_eq!(disabled["account"]["enabled"], false);
    assert_eq!(harness.state.settings_revision(), revision + 1);
    revision = harness.state.settings_revision();
    let after_disable = harness
        .state
        .db
        .lock()
        .get_account(&account_id)
        .unwrap()
        .unwrap();
    assert_eq!(after_disable.provider_id, stored.provider_id);
    assert_eq!(after_disable.credential_kind, stored.credential_kind);
    assert_eq!(after_disable.key_cipher, stored.key_cipher);
    assert!(after_disable.key_cipher.is_empty());

    let (status, enabled) = toggle_account(&harness, &account_id).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    assert_eq!(enabled["account"]["enabled"], true);
    assert_eq!(harness.state.settings_revision(), revision + 1);
    let after_enable = harness
        .state
        .db
        .lock()
        .get_account(&account_id)
        .unwrap()
        .unwrap();
    assert_eq!(after_enable.provider_id, stored.provider_id);
    assert_eq!(after_enable.credential_kind, stored.credential_kind);
    assert_eq!(after_enable.key_cipher, stored.key_cipher);
    assert!(after_enable.key_cipher.is_empty());
    harness.stop();
}
