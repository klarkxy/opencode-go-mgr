//! HTTP-only helpers for the v2 alias / multi-Plan black-box suite.
//!
//! Tests talk to Gateway and dashboard JSON. They do not construct private
//! gateway types. `CoreStateInner` is used only to boot an isolated data dir.

#![allow(dead_code)]

use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::models::ProxyMode;
use ocg_core::state::{CoreStateInner, GatewayHandle};
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[path = "../fake_upstream.rs"]
mod fake_upstream;

pub(crate) use fake_upstream::FakeReply;
use fake_upstream::{FakeCall, FakeCalls, start_fake_upstream, start_raw_disconnect_upstream};

pub(crate) const GATEWAY_KEY: &str = "gw-v2-contract";
pub(crate) const GO_ACCOUNT_KEY: &str = "v2-secret-KEY-9f3a2c1b-go";
pub(crate) const GO_ACCOUNT_KEY_2: &str = "v2-secret-KEY-9f3a2c1b-go-2";
pub(crate) const GOAT_ACCOUNT_KEY: &str = "v2-secret-KEY-9f3a2c1b-goat";
pub(crate) const CUSTOM_ACCOUNT_KEY: &str = "v2-secret-KEY-9f3a2c1b-custom";

pub(crate) const OPENCODE_PROVIDER_ID: &str = "opencode";
pub(crate) const GO_OFFERING_ID: &str = "go";
pub(crate) const COMMAND_CODE_PROVIDER_ID: &str = "command-code";
pub(crate) const GOAT_OFFERING_ID: &str = "goat";
pub(crate) const CUSTOM_PROVIDER_ID: &str = "custom";
pub(crate) const CUSTOM_OFFERING_ID: &str = "api";
pub(crate) const CUSTOM_UNROUTABLE_MODEL_ID: &str = "custom-unroutable-model";

pub(crate) const GO_ALIAS: &str = "deepseek-v4-flash";
pub(crate) const GOAT_UNIQUE_RAW_ID: &str = "deepseek/deepseek-v4-flash";
pub(crate) const FREE_MODEL: &str = "hy3-free";
pub(crate) const AMBIGUOUS_ERROR_TYPE: &str = "ambiguous_model_id";
pub(crate) const CUSTOM_OVERLAP_RAW_ID: &str = "shared-raw-model";

pub(crate) const SUCCESS_CHAT_BODY: &str = r#"{"id":"ok","object":"chat.completion","model":"upstream-should-not-leak","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}"#;

pub(crate) const MIXED_UPSTREAM_MODELS_BODY: &str = r#"{"object":"list","data":[{"id":"deepseek-v4-flash"},{"id":"deepseek/deepseek-v4-flash"},{"id":"vendor-raw-not-an-alias"},{"id":"minimax-m2.7"},{"id":"grok-4.5"}]}"#;

pub(crate) const CATALOG_CONTRACT: &str = include_str!("catalog_contract.json");

const CHAT_STREAM_HEAD: &str = "data: {\"id\":\"chat-stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n";

pub(crate) fn loopback_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("v2 test client should build")
}

pub(crate) fn catalog_contract() -> Value {
    serde_json::from_str(CATALOG_CONTRACT).expect("catalog contract fixture")
}

pub(crate) struct V2Harness {
    pub state: Arc<CoreStateInner>,
    pub dir: PathBuf,
    pub handle: GatewayHandle,
    pub client: reqwest::Client,
    pub port: u16,
    pub upstream_base_url: String,
    fake_calls: Option<FakeCalls>,
    disconnect_calls: Option<Arc<std::sync::atomic::AtomicUsize>>,
    stop_fake: Option<tokio::sync::oneshot::Sender<()>>,
}

impl V2Harness {
    pub(crate) async fn start() -> Self {
        Self::start_with_upstream(None).await
    }

    pub(crate) async fn start_with_chat_success(account_keys: &[&str]) -> Self {
        let mut replies = HashMap::new();
        for key in account_keys {
            replies.insert(
                (*key).to_string(),
                VecDeque::from([FakeReply {
                    status: 200,
                    body: SUCCESS_CHAT_BODY,
                }]),
            );
        }
        replies.insert(
            String::new(),
            VecDeque::from([FakeReply {
                status: 200,
                body: SUCCESS_CHAT_BODY,
            }]),
        );
        Self::start_with_upstream(Some(replies)).await
    }

    pub(crate) async fn start_with_upstream(
        replies: Option<HashMap<String, VecDeque<FakeReply>>>,
    ) -> Self {
        let dir = temp_data_dir();
        let db = Database::open(dir.clone()).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("v2-tests"));
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

        let (upstream, fake_calls, stop_fake) = if let Some(replies) = replies {
            let (base, calls, stop) = start_fake_upstream(replies).await;
            (Some(base), Some(calls), Some(stop))
        } else {
            (None, None, None)
        };

        let mut config = state.config();
        config.gateway_key = GATEWAY_KEY.into();
        config.proxy_mode = ProxyMode::Direct;
        let upstream_base_url = if let Some(base) = upstream {
            // Go and Zen share this suffix; the fake server is path-agnostic.
            format!("{}/zen/go", base.trim_end_matches('/'))
        } else {
            // Isolated tests must never touch a real provider. A closed
            // loopback port fails closed without leaving the machine.
            "http://127.0.0.1:1".into()
        };
        config.upstream_base_url = upstream_base_url.clone();
        state.set_config(config).unwrap();

        let handle =
            gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
                .await
                .unwrap();
        let client = loopback_client();
        wait_ready(&client, handle.port).await;
        Self {
            state,
            dir,
            port: handle.port,
            handle,
            client,
            upstream_base_url,
            fake_calls,
            disconnect_calls: None,
            stop_fake,
        }
    }

    pub(crate) fn dashboard(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}/dashboard/api/v3{path}", self.port)
    }

    pub(crate) fn mutation_body(&self, body: Value) -> Value {
        keys_to_camel(with_cas_tokens(
            body,
            self.state.settings_revision(),
            self.state.process_generation(),
        ))
    }

    pub(crate) fn gateway(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    pub(crate) async fn get_json(&self, path: &str) -> (StatusCode, Value) {
        let response = self.client.get(self.dashboard(path)).send().await.unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, adapt_v3_response(path, status, body))
    }

    pub(crate) async fn post_json(&self, path: &str, body: &Value) -> (StatusCode, Value) {
        let response = self
            .client
            .post(self.dashboard(path))
            .json(&self.mutation_body(body.clone()))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, adapt_v3_response(path, status, body))
    }

    pub(crate) async fn patch_json(&self, path: &str, body: &Value) -> (StatusCode, Value) {
        let response = self
            .client
            .patch(self.dashboard(path))
            .json(&self.mutation_body(body.clone()))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, adapt_v3_response(path, status, body))
    }

    pub(crate) async fn put_json(&self, path: &str, body: &Value) -> (StatusCode, Value) {
        let response = self
            .client
            .put(self.dashboard(path))
            .json(&self.mutation_body(body.clone()))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, adapt_v3_response(path, status, body))
    }

    pub(crate) async fn delete_json(&self, path: &str, body: &Value) -> (StatusCode, Value) {
        let response = self
            .client
            .delete(self.dashboard(path))
            .json(&self.mutation_body(body.clone()))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, adapt_v3_response(path, status, body))
    }

    pub(crate) async fn catalog(&self) -> Value {
        let (status, body) = self.get_json("/providers").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "catalog must be readable on loopback: {body}"
        );
        body
    }

    pub(crate) async fn accounts(&self) -> Value {
        let (status, body) = self.get_json("/accounts").await;
        assert_eq!(status, StatusCode::OK, "account list: {body}");
        body
    }

    pub(crate) async fn create_account(&self, payload: Value) -> (StatusCode, Value) {
        self.post_json("/accounts", &payload).await
    }

    pub(crate) async fn create_go_account(&self, name: &str, key: &str) -> Value {
        let revision = self.settings_revision().await;
        let (status, body) = self
            .create_account(json!({
                "provider_id": OPENCODE_PROVIDER_ID,
                "offering_id": GO_OFFERING_ID,
                "name": name,
                "key": key,
                "expected_revision": revision
            }))
            .await;
        assert_eq!(status, StatusCode::OK, "create Go account: {body}");
        body
    }

    pub(crate) async fn account_by_id(&self, id: &str) -> Value {
        self.accounts()
            .await
            .as_array()
            .into_iter()
            .flatten()
            .find(|account| account["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("account {id} missing from dashboard list"))
    }

    pub(crate) async fn settings_revision(&self) -> u64 {
        let (_, settings) = self.get_json("/settings").await;
        settings["revision"].as_u64().unwrap_or(0)
    }

    pub(crate) async fn chat(&self, model: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .post(self.gateway("/v1/chat/completions"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {GATEWAY_KEY}"),
            )
            .json(&json!({
                "model": model,
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 3,
                "stream": false
            }))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, body)
    }

    pub(crate) async fn list_client_models(&self) -> (StatusCode, Value) {
        let response = self
            .client
            .get(self.gateway("/v1/models"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {GATEWAY_KEY}"),
            )
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, body)
    }

    pub(crate) async fn claude_desktop_models(&self) -> (StatusCode, Value) {
        let response = self
            .client
            .get(self.gateway("/claude-desktop/v1/models"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {GATEWAY_KEY}"),
            )
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = decode_json(response).await;
        (status, body)
    }

    pub(crate) async fn forward_logs(&self) -> Value {
        let (status, body) = self.get_json("/logs/forward?limit=50").await;
        assert_eq!(status, StatusCode::OK, "forward logs: {body}");
        body
    }

    pub(crate) async fn gateway_logs(&self) -> Value {
        let (status, body) = self.get_json("/logs/gateway?limit=100").await;
        assert_eq!(status, StatusCode::OK, "gateway logs: {body}");
        body
    }

    pub(crate) fn fake_calls(&self) -> Vec<FakeCall> {
        self.fake_calls
            .as_ref()
            .map(|calls| calls.lock().expect("fake call log").clone())
            .unwrap_or_default()
    }

    pub(crate) fn fake_call_keys(&self) -> Vec<String> {
        self.fake_calls().into_iter().map(|call| call.key).collect()
    }

    pub(crate) fn disconnect_call_count(&self) -> usize {
        self.disconnect_calls
            .as_ref()
            .map(|calls| calls.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub(crate) fn shutdown(mut self) {
        gateway::stop_gateway(self.handle);
        if let Some(stop) = self.stop_fake.take() {
            let _ = stop.send(());
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub(crate) async fn start_output_then_disconnect_upstream() -> (
    String,
    Arc<std::sync::atomic::AtomicUsize>,
    tokio::sync::oneshot::Sender<()>,
) {
    let payload = CHAT_STREAM_HEAD;
    let raw = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n{:X}\r\n{}\r\n",
        payload.len(),
        payload
    )
    .into_bytes();
    start_raw_disconnect_upstream(raw).await
}

pub(crate) async fn start_v2_with_disconnect_upstream() -> V2Harness {
    let dir = temp_data_dir();
    let db = Database::open(dir.clone()).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("v2-tests"));
    let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
    let (base, calls, stop) = start_output_then_disconnect_upstream().await;
    let mut config = state.config();
    config.gateway_key = GATEWAY_KEY.into();
    config.proxy_mode = ProxyMode::Direct;
    config.upstream_base_url = format!("{}/zen/go", base.trim_end_matches('/'));
    state.set_config(config).unwrap();
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let client = loopback_client();
    wait_ready(&client, handle.port).await;
    V2Harness {
        state,
        dir,
        port: handle.port,
        handle,
        client,
        upstream_base_url: format!("{}/zen/go", base.trim_end_matches('/')),
        fake_calls: None,
        disconnect_calls: Some(calls),
        stop_fake: Some(stop),
    }
}

pub(crate) fn catalog_entry<'a>(
    catalog: &'a Value,
    provider_id: &str,
    offering_id: &str,
) -> Option<&'a Value> {
    catalog
        .as_array()?
        .iter()
        .find(|entry| entry["provider_id"] == provider_id && entry["offering_id"] == offering_id)
}

pub(crate) fn catalog_aliases(entry: &Value) -> Vec<Value> {
    match &entry["model_aliases"] {
        Value::Array(items) => items.clone(),
        _ => Vec::new(),
    }
}

pub(crate) fn alias_name_list(entry: &Value) -> Vec<String> {
    catalog_aliases(entry)
        .into_iter()
        .filter_map(|item| {
            item.as_str()
                .or_else(|| item["alias"].as_str())
                .map(str::to_string)
        })
        .collect()
}

pub(crate) fn alias_names(entry: &Value) -> HashSet<String> {
    alias_name_list(entry).into_iter().collect()
}

pub(crate) fn form_field_ids(entry: &Value) -> HashSet<String> {
    entry["form_fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| field["id"].as_str().map(str::to_string))
        .collect()
}

pub(crate) fn custom_create_payload(
    name: &str,
    key: &str,
    revision: u64,
    base_url: &str,
    model_id: &str,
) -> Value {
    let endpoint_url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    json!({
        "provider_id": CUSTOM_PROVIDER_ID,
        "offering_id": CUSTOM_OFFERING_ID,
        "name": name,
        "key": key,
        "expected_revision": revision,
        "custom_config": {
            "endpoint_url": endpoint_url,
            "upstream_protocol": "chat_completions"
        },
        "model_capabilities": [{
            "model_id": model_id,
            "protocol": "chat_completions"
        }]
    })
}

pub(crate) fn overlapping_raw_ids(catalog: &Value) -> Vec<(String, Vec<(String, String)>)> {
    let mut by_raw: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let Some(entries) = catalog.as_array() else {
        return Vec::new();
    };
    for entry in entries {
        let provider = entry["provider_id"].as_str().unwrap_or_default();
        let offering = entry["offering_id"].as_str().unwrap_or_default();
        for alias in catalog_aliases(entry) {
            let raw = alias["upstream_model"]
                .as_str()
                .or_else(|| alias["upstream_model_id"].as_str());
            if let Some(raw) = raw {
                by_raw
                    .entry(raw.to_string())
                    .or_default()
                    .push((provider.to_string(), offering.to_string()));
            }
        }
    }
    by_raw
        .into_iter()
        .filter_map(|(raw, plans)| {
            let mut unique = plans.clone();
            unique.sort();
            unique.dedup();
            if unique.len() > 1 {
                Some((raw, unique))
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn error_type(body: &Value) -> Option<&str> {
    body.pointer("/error/type")
        .and_then(Value::as_str)
        .or_else(|| body.get("type").and_then(Value::as_str))
}

pub(crate) fn error_message(body: &Value) -> String {
    if let Some(message) = body.pointer("/error/message").and_then(Value::as_str) {
        return message.to_string();
    }
    match &body["error"] {
        Value::String(message) => message.clone(),
        other => other.to_string(),
    }
}

pub(crate) fn json_contains_secret(value: &Value, secret: &str) -> bool {
    if secret.is_empty() {
        return false;
    }
    match value {
        Value::String(text) => text.contains(secret),
        Value::Array(items) => items.iter().any(|item| json_contains_secret(item, secret)),
        Value::Object(map) => map.values().any(|item| json_contains_secret(item, secret)),
        _ => false,
    }
}

pub(crate) fn client_model_ids(body: &Value) -> Vec<String> {
    body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str().map(str::to_string))
        .collect()
}

pub(crate) fn required_catalog_fields() -> Vec<String> {
    catalog_contract()["required_entry_fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

pub(crate) fn risk_notice_fields() -> Vec<String> {
    catalog_contract()["risk_notice_fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

pub(crate) fn missing_fields(entry: &Value, fields: &[String]) -> Vec<String> {
    fields
        .iter()
        .filter(|field| entry.get(field.as_str()).is_none() || entry[field.as_str()].is_null())
        .cloned()
        .collect()
}

fn temp_data_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ocg-v2-contract-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

async fn wait_ready(client: &reqwest::Client, port: u16) {
    let url = format!("http://127.0.0.1:{port}/dashboard/api/auth/status");
    for _ in 0..50 {
        if client.get(&url).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("gateway on port {port} did not become ready");
}

async fn decode_json(response: reqwest::Response) -> Value {
    let text = response.text().await.unwrap_or_default();
    serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }))
}

fn with_cas_tokens(mut body: Value, revision: u64, process_generation: u64) -> Value {
    let Some(object) = body.as_object_mut() else {
        return body;
    };
    if !object.contains_key("expectedRevision") && !object.contains_key("expected_revision") {
        object.insert("expected_revision".into(), json!(revision));
    }
    if !object.contains_key("processGeneration") && !object.contains_key("process_generation") {
        object.insert("process_generation".into(), json!(process_generation));
    }
    body
}

fn keys_to_camel(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, child)| (snake_to_camel(&key), keys_to_camel(child)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(keys_to_camel).collect()),
        other => other,
    }
}

fn keys_to_snake(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, child)| (camel_to_snake(&key), keys_to_snake(child)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(keys_to_snake).collect()),
        other => other,
    }
}

fn snake_to_camel(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut upper = false;
    for ch in key.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn camel_to_snake(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    for (index, ch) in key.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn adapt_v3_response(path: &str, status: StatusCode, body: Value) -> Value {
    let path = path.split('?').next().unwrap_or(path);
    let body = keys_to_snake(body);
    if !status.is_success() {
        return body;
    }
    if path == "/providers" || path == "/providers/catalog" {
        if let Some(entries) = body.get("entries") {
            return entries.clone();
        }
    }
    if path == "/accounts" {
        if let Some(accounts) = body.get("accounts").and_then(Value::as_array) {
            return Value::Array(
                accounts
                    .iter()
                    .cloned()
                    .map(|account| normalize_account(account, None))
                    .collect(),
            );
        }
    }
    if path == "/application-models" {
        if let Some(models) = body.get("models") {
            return models.clone();
        }
    }
    if let Some(account) = body.get("account") {
        if !account.is_null() {
            return normalize_account(account.clone(), body.get("revision").cloned());
        }
    }
    if body.get("id").is_some() && body.get("provider_id").is_some() {
        return normalize_account(body, None);
    }
    body
}

fn normalize_account(mut account: Value, revision: Option<Value>) -> Value {
    if let Some(object) = account.as_object_mut() {
        object.entry("key").or_insert_with(|| json!(""));
        object.entry("password").or_insert_with(|| json!(""));
        if let Some(revision) = revision {
            object.entry("revision").or_insert(revision);
        }
    }
    account
}
