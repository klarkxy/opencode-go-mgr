//! HTTP helpers for Dashboard V3 contract kernel tests.

#![allow(dead_code)]

use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::host_router::{DASHBOARD_V2_REMOVED_CODE, DASHBOARD_V2_REMOVED_MESSAGE};
use ocg_core::state::{CoreStateInner, GatewayHandle};
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) struct V3Harness {
    pub state: Arc<CoreStateInner>,
    pub dir: PathBuf,
    pub handle: GatewayHandle,
    pub client: reqwest::Client,
    pub v2_base: String,
    pub v3_base: String,
}

pub(crate) fn temp_data_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "ocg-dashboard-v3-{}-{}",
        label,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn loopback_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("dashboard v3 test client should build")
}

pub(crate) fn state(label: &str) -> Arc<CoreStateInner> {
    let dir = temp_data_dir(label);
    let db = Database::open(dir.clone()).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("v3-contract"));
    Arc::new(CoreStateInner::new(db, dir, cipher).unwrap())
}

pub(crate) async fn start_loopback(label: &str) -> V3Harness {
    start_on(label, SocketAddr::from(([127, 0, 0, 1], 0))).await
}

pub(crate) async fn start_public(label: &str) -> V3Harness {
    #[cfg(windows)]
    {
        // Windows Firewall prompts for every hashed integration-test binary
        // that binds a wildcard socket. Keep the transport loopback-only and
        // explicitly exercise the same session-protected dashboard mode.
        let harness = start_on(label, SocketAddr::from(([127, 0, 0, 1], 0))).await;
        harness.state.set_dashboard_local_mode(false);
        harness
    }
    #[cfg(not(windows))]
    {
        start_on(label, SocketAddr::from(([0, 0, 0, 0], 0))).await
    }
}

async fn start_on(label: &str, addr: SocketAddr) -> V3Harness {
    let state = state(label);
    let dir = state.data_dir();
    let handle = gateway::start_gateway_on(state.clone(), addr)
        .await
        .unwrap();
    let host = format!("http://127.0.0.1:{}", handle.port);
    V3Harness {
        state,
        dir,
        handle,
        client: loopback_client(),
        v2_base: format!("{host}/dashboard/api"),
        v3_base: format!("{host}/dashboard/api/v3"),
    }
}

impl V3Harness {
    pub(crate) async fn get_json(&self, url: &str) -> (StatusCode, Value) {
        let response = self.client.get(url).send().await.unwrap();
        let status = response.status();
        let body = response.json().await.unwrap_or(Value::Null);
        (status, body)
    }

    pub(crate) fn assert_v2_removed(status: StatusCode, body: &Value) {
        assert_eq!(status, StatusCode::GONE, "{body}");
        assert_eq!(
            body,
            &json!({
                "code": DASHBOARD_V2_REMOVED_CODE,
                "message": DASHBOARD_V2_REMOVED_MESSAGE })
        );
    }

    pub(crate) async fn assert_v2_path_removed(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.v2_base));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let body = response.json().await.unwrap_or(Value::Null);
        Self::assert_v2_removed(status, &body);
    }

    pub(crate) fn stop(self) {
        gateway::stop_gateway(self.handle);
        let _ = fs::remove_dir_all(self.dir);
    }
}
