use crate::gateway;
use crate::models::GatewayStatus;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn get_gateway_status(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    let config = state.config();
    let running = state.gateway.lock().is_some();
    Ok(GatewayStatus {
        running,
        port: config.gateway_port,
        key: config.gateway_key,
        upstream_base_url: config.upstream_base_url,
    })
}

#[tauri::command]
pub fn restart_gateway(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    let config = state.config();
    let mut gw_lock = state.gateway.lock();

    // Stop existing
    if let Some(handle) = gw_lock.take() {
        gateway::stop_gateway(handle);
    }

    // Start new (hold the lock across start to prevent get_gateway_status from seeing None)
    let new_state = std::sync::Arc::clone(&*state);
    let handle = tauri::async_runtime::block_on(gateway::start_gateway(new_state, config.gateway_port))
        .map_err(|e| e.to_string())?;
    let status = GatewayStatus {
        running: true,
        port: handle.port,
        key: config.gateway_key.clone(),
        upstream_base_url: config.upstream_base_url.clone(),
    };
    *gw_lock = Some(handle);
    drop(gw_lock);

    let _ = state
        .db
        .lock()
        .log_gateway("info", "gateway", &format!("gateway restarted on port {}", config.gateway_port));
    Ok(status)
}
