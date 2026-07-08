use crate::state::AppState;
use ocg_core::models::AppConfig;
use ocg_core::state::random_word;
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.core.config())
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, config: AppConfig) -> Result<AppConfig, String> {
    state.core.set_config(config.clone()).map_err(|e| e.to_string())?;
    let _ = state.core.db.lock().log_gateway("info", "settings", "settings updated");
    Ok(config)
}

#[tauri::command]
pub fn regenerate_gateway_key(state: State<'_, AppState>) -> Result<String, String> {
    let mut config = state.core.config();
    config.gateway_key = format!("ocg-{}-{}", random_word(), random_word());
    state.core.set_config(config.clone()).map_err(|e| e.to_string())?;
    let _ = state.core.db.lock().log_gateway("info", "settings", "gateway key regenerated");
    Ok(config.gateway_key)
}
