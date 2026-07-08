use crate::state::AppState;
use ocg_core::gateway;
use ocg_core::models::GatewayStatus;
use tauri::State;

#[tauri::command]
pub fn get_gateway_status(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    let config = state.core.config();
    let running = state.core.gateway.lock().is_some();
    Ok(GatewayStatus {
        running,
        port: config.gateway_port,
        key: config.gateway_key,
        upstream_base_url: config.upstream_base_url,
    })
}

#[tauri::command]
pub fn restart_gateway(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    let config = state.core.config();
    let mut gw_lock = state.core.gateway.lock();

    // Stop existing and wait for the old listener to actually release the port.
    if let Some(handle) = gw_lock.take() {
        let _ = handle.shutdown.send(());
        let _ = tauri::async_runtime::block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), handle.task)
                .await
                .ok()
        });
    }

    // Start new (hold the lock across start to prevent get_gateway_status from seeing None)
    let new_state = std::sync::Arc::clone(&state.core);
    let handle = tauri::async_runtime::block_on(gateway::start_gateway(new_state, config.gateway_port))
        .map_err(|e| e.to_string())?;

    // If start fails here, gw_lock still holds None from take() above.
    *gw_lock = Some(handle);
    let status = GatewayStatus {
        running: true,
        port: gw_lock.as_ref().unwrap().port,
        key: config.gateway_key.clone(),
        upstream_base_url: config.upstream_base_url.clone(),
    };
    drop(gw_lock);

    let _ = state
        .core
        .db
        .lock()
        .log_gateway("info", "gateway", &format!("gateway restarted on port {}", config.gateway_port));
    Ok(status)
}
