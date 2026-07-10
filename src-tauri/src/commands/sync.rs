//! Manual remote-node actions. The GUI is local-first: no startup pull, no
//! account-CRUD auto push. These commands only run when the user clicks the
//! remote buttons in Settings.

use crate::state::AppState;
use ocg_core::models::Account;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Serialize)]
struct PushPayload {
    id: String,
    name: String,
    username: Option<String>,
    password: Option<String>,
    password_cipher: Option<String>,
    key: String,
    key_cipher: String,
    enabled: bool,
    referral_code: Option<String>,
    recharge_date: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteNodeStatus {
    #[serde(default)]
    pub url: String,
    pub version: String,
    pub gateway: RemoteGatewayStatus,
    pub accounts: RemoteAccountStatus,
    pub usage: RemoteUsageStatus,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteGatewayStatus {
    pub running: bool,
    pub port: u16,
    pub upstream_base_url: String,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteAccountStatus {
    pub total: usize,
    pub enabled: usize,
    pub disabled: usize,
    pub cooldown: usize,
    pub available: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteUsageStatus {
    pub today_cost: f64,
    pub week_cost: f64,
    pub month_cost: f64,
}

#[derive(Debug, Serialize)]
pub struct RemoteSyncResult {
    pub pushed: usize,
    pub message: String,
}

#[tauri::command]
pub async fn get_remote_node_status(
    state: State<'_, AppState>,
) -> Result<RemoteNodeStatus, String> {
    fetch_remote_node_status(state.inner().clone()).await
}

#[tauri::command]
pub async fn push_local_to_remote(state: State<'_, AppState>) -> Result<RemoteSyncResult, String> {
    sync_local_to_remote(state.inner().clone()).await
}

async fn fetch_remote_node_status(state: AppState) -> Result<RemoteNodeStatus, String> {
    let (base, token) = configured_remote(&state)?;
    let url = format!("{}/admin/status", base);
    let response = state
        .core
        .http_client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("刷新远端状态失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("远端状态返回 {}", response.status()));
    }
    let mut status = response
        .json::<RemoteNodeStatus>()
        .await
        .map_err(|e| format!("解析远端状态失败: {}", e))?;
    status.url = base;
    Ok(status)
}

async fn sync_local_to_remote(state: AppState) -> Result<RemoteSyncResult, String> {
    let (base, token) = configured_remote(&state)?;
    let payloads = local_payloads(&state)?;

    let mut pushed = 0usize;
    for payload in &payloads {
        upsert_remote_key(&state, &base, &token, payload).await?;
        pushed += 1;
    }

    let message = format!("推送完成：推送 {} 个账号，未删除远端账号", pushed);
    let _ = state
        .core
        .db
        .lock()
        .log_gateway("info", "remote_sync", &message);

    Ok(RemoteSyncResult { pushed, message })
}

fn configured_remote(state: &AppState) -> Result<(String, String), String> {
    let cfg = state.core.config();
    let base = cfg.remote.url.trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Err("请先填写并保存远端 URL".to_string());
    }
    validate_remote_base(&base)?;
    let token = cfg.remote.token.trim().to_string();
    if token.is_empty() {
        return Err("请先填写并保存远端 token".to_string());
    }
    Ok((base, token))
}

fn local_payloads(state: &AppState) -> Result<Vec<PushPayload>, String> {
    let accounts = state
        .core
        .db
        .lock()
        .list_accounts()
        .map_err(|e| e.to_string())?;
    accounts
        .iter()
        .map(|account| payload_from_account(state, account))
        .collect()
}

fn payload_from_account(state: &AppState, account: &Account) -> Result<PushPayload, String> {
    let key = state
        .core
        .decrypt_key(&account.key_cipher)
        .map_err(|e| format!("解密账号 {} 失败: {}", account.name, e))?;
    Ok(PushPayload {
        id: account.id.clone(),
        name: account.name.clone(),
        username: account.username.clone(),
        password: match account.password_cipher.as_deref() {
            Some(cipher) => Some(
                state
                    .core
                    .decrypt_key(cipher)
                    .map_err(|e| format!("解密账号 {} 密码失败: {}", account.name, e))?,
            ),
            None => None,
        },
        password_cipher: account.password_cipher.clone(),
        key,
        key_cipher: account.key_cipher.clone(),
        enabled: account.enabled,
        referral_code: account.referral_code.clone(),
        recharge_date: account.recharge_date.clone(),
        created_at: account.created_at.to_rfc3339(),
        updated_at: account.updated_at.to_rfc3339(),
    })
}

async fn upsert_remote_key(
    state: &AppState,
    base: &str,
    token: &str,
    payload: &PushPayload,
) -> Result<(), String> {
    let response = state
        .core
        .http_client
        .post(format!("{}/admin/keys", base))
        .bearer_auth(token)
        .json(payload)
        .send()
        .await
        .map_err(|e| format!("推送账号 {} 失败: {}", payload.name, e))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "推送账号 {} 返回 {}",
            payload.name,
            response.status()
        ))
    }
}

fn validate_remote_base(base: &str) -> Result<(), String> {
    let parsed = tauri::Url::parse(base).map_err(|e| format!("invalid remote URL: {}", e))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(&parsed) => Ok(()),
        _ => Err("remote node must use https, except loopback http".to_string()),
    }
}

fn is_loopback(url: &tauri::Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::GuiState;
    use chrono::Utc;
    use ocg_core::admin;
    use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
    use ocg_core::db::Database;
    use ocg_core::models::Account;
    use ocg_core::state::CoreStateInner;
    use parking_lot::Mutex as ParkingMutex;
    use std::fs;
    use std::net::TcpListener as StdTcpListener;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn temp_data_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "ocg-gui-sync-test-{}-{}",
            label,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn free_port() -> u16 {
        let listener = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    }

    fn build_core(label: &str) -> (Arc<CoreStateInner>, PathBuf) {
        let dir = temp_data_dir(label);
        let db = Database::open(dir.clone()).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new(label));
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        (state, dir)
    }

    fn app_state(core: Arc<CoreStateInner>) -> AppState {
        Arc::new(GuiState {
            core,
            current_browser_window: ParkingMutex::new(None),
        })
    }

    fn insert_account(state: &Arc<CoreStateInner>, id: &str, name: &str, key: &str) {
        let now = Utc::now();
        let account = Account {
            id: id.to_string(),
            name: name.to_string(),
            username: None,
            password_cipher: None,
            key_cipher: state.encrypt_key(key).unwrap(),
            enabled: true,
            referral_code: None,
            recharge_date: None,
            cooldown_until: None,
            last_error: None,
            created_at: now,
            updated_at: now,
        };
        state.db.lock().create_account(&account).unwrap();
    }

    #[tokio::test]
    async fn push_preserves_remote_extras() {
        let (remote_core, remote_dir) = build_core("remote");
        insert_account(&remote_core, "remote-extra", "remote extra", "remote-key");
        let admin_port = free_port();
        let admin_handle =
            admin::start_admin(remote_core.clone(), admin_port, "admin-token".into())
                .await
                .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (local_core, local_dir) = build_core("local");
        let mut config = local_core.config();
        config.remote.url = format!("http://127.0.0.1:{}", admin_port);
        config.remote.token = "admin-token".into();
        local_core.set_config(config).unwrap();
        insert_account(&local_core, "local-1", "local one", "local-key");
        let local_app = app_state(local_core);

        let pushed = sync_local_to_remote(local_app).await.unwrap();
        assert_eq!(pushed.pushed, 1);
        let remote_accounts = remote_core.db.lock().list_accounts().unwrap();
        assert!(remote_accounts.iter().any(|a| a.id == "remote-extra"));
        let imported = remote_accounts
            .iter()
            .find(|a| a.id == "local-1")
            .expect("local account should be imported");
        assert_eq!(
            remote_core.decrypt_key(&imported.key_cipher).unwrap(),
            "local-key"
        );

        admin::stop_admin(admin_handle);
        let _ = fs::remove_dir_all(remote_dir);
        let _ = fs::remove_dir_all(local_dir);
    }
}
