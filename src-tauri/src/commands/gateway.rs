use crate::state::AppState;
use ocg_core::gateway;
use ocg_core::models::{AppConfig, GatewayStatus};
use ocg_core::state::CoreState;
use tauri::State;

#[tauri::command]
pub fn get_gateway_status(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    let config = state.core.config();
    let running = state.core.gateway.lock().is_some();
    Ok(status_from_config(&state.core, running, &config))
}

pub(super) fn status_from_config(
    core: &CoreState,
    running: bool,
    config: &AppConfig,
) -> GatewayStatus {
    let last_error = if running {
        None
    } else {
        core.db.lock().latest_gateway_error().ok().flatten()
    };
    GatewayStatus {
        running,
        port: config.gateway_port,
        key: config.gateway_key.clone(),
        upstream_base_url: config.upstream_base_url.clone(),
        last_error,
    }
}

pub(super) fn restart_inner(core: &CoreState, config: &AppConfig) -> Result<GatewayStatus, String> {
    let mut gw_lock = core.gateway.lock();

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
    let new_state = std::sync::Arc::clone(core);
    let handle = match tauri::async_runtime::block_on(gateway::start_gateway(
        new_state,
        config.gateway_port,
    )) {
        Ok(handle) => handle,
        Err(e) => {
            let message = format!(
                "failed to restart gateway on port {}: {}",
                config.gateway_port, e
            );
            drop(gw_lock);
            let _ = core.db.lock().log_gateway("error", "gateway", &message);
            return Err(message);
        }
    };

    // If start fails here, gw_lock still holds None from take() above.
    *gw_lock = Some(handle);
    let status = status_from_config(core, true, config);
    drop(gw_lock);

    let _ = core.db.lock().log_gateway(
        "info",
        "gateway",
        &format!("gateway restarted on port {}", config.gateway_port),
    );
    Ok(status)
}

#[tauri::command]
pub fn restart_gateway(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    let config = state.core.config();
    restart_inner(&state.core, &config)
}
