//! Gateway Lifecycle host capability.
//!
//! Binds through `ocg_core::gateway::start_gateway` (listener + process-level
//! usage-sync workers) and stops with a signal-only `stop_gateway`. Settings
//! port changes rebind through CoreState / GatewayLifecycle, not this module.

use ocg_core::gateway;
use ocg_core::state::CoreState;

pub fn start_on_configured_port(core: &CoreState) -> anyhow::Result<()> {
    let port = core.settings_config().gateway_port;
    let gateway_state = core.clone();
    match tauri::async_runtime::block_on(gateway::start_gateway(gateway_state, port)) {
        Ok(handle) => {
            let _ = core.db.lock().log_gateway(
                "info",
                "gateway",
                &format!("gateway started on port {}", handle.port),
            );
            *core.gateway.lock() = Some(handle);
            Ok(())
        }
        Err(error) => {
            eprintln!("failed to start Gateway on 127.0.0.1:{port}: {error}");
            let _ = core.db.lock().log_gateway(
                "error",
                "gateway",
                &format!("failed to start gateway: {error}"),
            );
            Err(error)
        }
    }
}

pub fn stop_listener(core: &CoreState) {
    if let Some(handle) = core.gateway.lock().take() {
        gateway::stop_gateway(handle);
    }
}

#[cfg(test)]
mod tests;
