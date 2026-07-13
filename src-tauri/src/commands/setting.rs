use crate::state::AppState;
use ocg_core::models::{AppConfig, GatewayStatus, normalize_client_root_url};
use ocg_core::state::random_word;
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.core.config())
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    mut config: AppConfig,
) -> Result<GatewayStatus, String> {
    let _settings_update = state.core.settings_update.lock();
    config.validate_timeouts()?;
    validate_upstream_url(&config.upstream_base_url)?;
    config.client_root_url = normalize_client_root_url(&config.client_root_url)?;
    // ponytail: only restart if the port actually changed. Gateway key and
    // upstream URL are already live
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
                    crate::autostart::sync(config.auto_start).map_err(|e| e.to_string())?;
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

    crate::autostart::sync(config.auto_start).map_err(|e| e.to_string())?;
    state
        .core
        .set_config(config.clone())
        .map_err(|e| e.to_string())?;
    let _ = state
        .core
        .db
        .lock()
        .log_gateway("info", "settings", "settings updated");

    let snapshot = state.core.config();
    Ok(crate::commands::gateway::status_from_config(
        &state.core,
        was_running,
        &snapshot,
    ))
}

#[tauri::command]
pub fn regenerate_gateway_key(state: State<'_, AppState>) -> Result<String, String> {
    let _settings_update = state.core.settings_update.lock();
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

fn validate_upstream_url(url: &str) -> Result<(), String> {
    let parsed = tauri::Url::parse(url).map_err(|e| format!("invalid upstream URL: {}", e))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(&parsed) => Ok(()),
        _ => Err("upstream must use https, except loopback http for local development".to_string()),
    }
}

fn is_loopback(url: &tauri::Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
    )
}
