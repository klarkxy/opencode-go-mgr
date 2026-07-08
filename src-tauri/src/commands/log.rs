use crate::state::AppState;
use ocg_core::models::{ForwardLog, GatewayLog};
use tauri::State;

#[tauri::command]
pub fn get_gateway_logs(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<GatewayLog>, String> {
    state
        .core
        .db
        .lock()
        .list_gateway_logs(limit.unwrap_or(200))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_forward_logs(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<ForwardLog>, String> {
    state
        .core
        .db
        .lock()
        .list_forward_logs(limit.unwrap_or(200))
        .map_err(|e| e.to_string())
}
