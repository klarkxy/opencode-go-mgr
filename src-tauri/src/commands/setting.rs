use crate::state::AppState;
use ocg_core::models::{AppConfig, GatewayStatus};
use ocg_core::state::random_word;
use serde::Serialize;
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.core.config())
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<GatewayStatus, String> {
    validate_upstream_url(&config.upstream_base_url)?;
    validate_remote_url(&config.remote.url)?;
    // ponytail: only restart if the port actually changed. The other three fields
    // (gateway_key / upstream_base_url / selection_strategy) are already live
    // — handler.rs reads state.config() per request. Restarting on every save
    // would drop in-flight requests for no reason.
    // ponytail: if the gateway is not running, do not start it here; the next
    // manual start will pick up the new port from config.
    // ponytail: probe-bind the new port BEFORE we touch the DB or in-memory
    // config. If the bind fails, the old config stays put and the gateway keeps
    // serving on the old port — no "save failed with gateway down" regression.
    let old_port = state.core.config().gateway_port;
    let port_changed = old_port != config.gateway_port;
    let was_running = {
        let gw = state.core.gateway.lock();
        gw.is_some()
    };

    if port_changed {
        // ponytail: skip the TOCTOU pre-bind. The previous code opened a probe
        // TcpListener, dropped it, then called start_gateway — a classic
        // race-stop window where another process could grab the port between
        // probe and bind. Instead, write the new config only AFTER a
        // successful restart, so a failed bind leaves the in-memory and
        // on-disk configs on the old port.
        if was_running {
            match crate::commands::gateway::restart_inner(&state.core, &config) {
                Ok(status) => {
                    state.core.set_config(config).map_err(|e| e.to_string())?;
                    return Ok(status);
                }
                Err(e) => {
                    let _ = state.core.db.lock().log_gateway(
                        "warn",
                        "settings",
                        &format!("port change to {} failed: {}", config.gateway_port, e),
                    );
                    return Err(e);
                }
            }
        }
    }

    state
        .core
        .set_config(config.clone())
        .map_err(|e| e.to_string())?;
    let _ = state
        .core
        .db
        .lock()
        .log_gateway("info", "settings", "settings updated");
    // ponytail: latch the wizard-once flag here, not in a separate command.
    // The wizard's only job is to populate AppConfig.remote; once any
    // update_settings runs the user has made a deliberate choice.
    let _ = state
        .core
        .db
        .lock()
        .set_setting("settings.bootstrapped", "1");

    let snapshot = state.core.config();
    Ok(GatewayStatus {
        running: was_running,
        port: snapshot.gateway_port,
        key: snapshot.gateway_key,
        upstream_base_url: snapshot.upstream_base_url,
    })
}

#[tauri::command]
pub fn regenerate_gateway_key(state: State<'_, AppState>) -> Result<String, String> {
    let mut config = state.core.config();
    config.gateway_key = format!("ocg-{}-{}", random_word(), random_word());
    state
        .core
        .set_config(config.clone())
        .map_err(|e| e.to_string())?;
    let _ = state
        .core
        .db
        .lock()
        .log_gateway("info", "settings", "gateway key regenerated");
    Ok(config.gateway_key)
}

#[derive(Serialize)]
pub struct RemoteStatus {
    pub url: String,
    pub bootstrapped: bool,
}

#[tauri::command]
pub fn get_remote_status(state: State<'_, AppState>) -> RemoteStatus {
    let cfg = state.core.config();
    let bootstrapped = state
        .core
        .db
        .lock()
        .get_setting("settings.bootstrapped")
        .ok()
        .flatten()
        .as_deref()
        == Some("1");
    RemoteStatus {
        url: cfg.remote.url,
        bootstrapped,
    }
}

#[derive(Serialize)]
pub struct RemoteTestResult {
    pub ok: bool,
    pub message: String,
}

#[tauri::command]
pub async fn test_remote(
    state: State<'_, AppState>,
    url: String,
    token: String,
) -> Result<RemoteTestResult, String> {
    // ponytail: probe the same /admin/health the GUI will use. We do NOT
    // mutate settings here — that is the wizard's job after a successful probe.
    // ponytail: reject schemes other than http(s) so a free-text URL can't be
    // used to exfiltrate the bearer token to an arbitrary scheme handler
    // (file://, javascript:, data:, etc.).
    let base = url.trim_end_matches('/');
    if let Err(message) = validate_remote_url(base) {
        return Ok(RemoteTestResult { ok: false, message });
    }
    let health_url = format!("{}/admin/health", base);
    let result = state
        .core
        .http_client
        .get(&health_url)
        .bearer_auth(&token)
        .send()
        .await;
    match result {
        Ok(r) if r.status().is_success() => Ok(RemoteTestResult {
            ok: true,
            message: format!("{} OK", r.status()),
        }),
        Ok(r) => Ok(RemoteTestResult {
            ok: false,
            message: format!("server replied {}", r.status()),
        }),
        Err(e) => Ok(RemoteTestResult {
            ok: false,
            message: format!("connection failed: {}", e),
        }),
    }
}

fn validate_upstream_url(url: &str) -> Result<(), String> {
    let parsed = tauri::Url::parse(url).map_err(|e| format!("invalid upstream URL: {}", e))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(&parsed) => Ok(()),
        _ => Err("upstream must use https, except loopback http for local development".to_string()),
    }
}

fn validate_remote_url(url: &str) -> Result<(), String> {
    if url.trim().is_empty() {
        return Ok(());
    }
    let parsed = tauri::Url::parse(url).map_err(|e| format!("invalid remote URL: {}", e))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(&parsed) => Ok(()),
        _ => Err("remote sync must use https, except loopback http".to_string()),
    }
}

fn is_loopback(url: &tauri::Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
    )
}
