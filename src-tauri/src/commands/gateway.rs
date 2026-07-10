use crate::state::AppState;
use ocg_core::gateway;
use ocg_core::models::{AppConfig, GatewayStatus};
use ocg_core::state::{CoreState, GatewayHandle};
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
        port: core.active_gateway_port(),
        key: config.gateway_key.clone(),
        upstream_base_url: config.upstream_base_url.clone(),
        last_error,
    }
}

fn stop_and_wait(handle: GatewayHandle) {
    let _ = handle.shutdown.send(());
    let _ = tauri::async_runtime::block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(5), handle.task)
            .await
            .ok()
    });
}

pub(super) fn restart_inner(core: &CoreState, config: &AppConfig) -> Result<GatewayStatus, String> {
    let current_port = core.gateway.lock().as_ref().map(|handle| handle.port);
    if current_port == Some(config.gateway_port) {
        if let Some(handle) = core.gateway.lock().take() {
            stop_and_wait(handle);
        }
    }

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
            let _ = core.db.lock().log_gateway("error", "gateway", &message);
            return Err(message);
        }
    };

    let old_handle = core.gateway.lock().replace(handle);
    if current_port != Some(config.gateway_port) {
        if let Some(handle) = old_handle {
            stop_and_wait(handle);
        }
    }
    let status = status_from_config(core, true, config);

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

#[cfg(test)]
mod tests {
    use super::{restart_inner, stop_and_wait};
    use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
    use ocg_core::db::Database;
    use ocg_core::gateway;
    use ocg_core::state::CoreStateInner;
    use std::fs;
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn temp_data_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ocg-restart-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn free_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[test]
    fn failed_port_change_keeps_old_gateway_running() {
        let dir = temp_data_dir();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        let old_port = free_port();
        let old_handle =
            tauri::async_runtime::block_on(gateway::start_gateway(state.clone(), old_port))
                .unwrap();
        *state.gateway.lock() = Some(old_handle);

        let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut config = state.config();
        config.gateway_port = occupied.local_addr().unwrap().port();

        assert!(restart_inner(&state, &config).is_err());
        assert_eq!(state.active_gateway_port(), old_port);
        assert!(TcpStream::connect(("127.0.0.1", old_port)).is_ok());

        stop_and_wait(state.gateway.lock().take().unwrap());
        drop(occupied);
        let _ = fs::remove_dir_all(dir);
    }
}
