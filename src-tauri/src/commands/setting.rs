use crate::models::AppConfig;
use crate::state::{random_word, AppState};
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config())
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, config: AppConfig) -> Result<AppConfig, String> {
    state.set_config(config.clone()).map_err(|e| e.to_string())?;
    let _ = state.db.lock().log_gateway("info", "settings", "settings updated");
    Ok(config)
}

#[tauri::command]
pub fn regenerate_gateway_key(state: State<'_, AppState>) -> Result<String, String> {
    let mut config = state.config();
    config.gateway_key = format!("ocg-{}-{}", random_word(), random_word());
    state.set_config(config.clone()).map_err(|e| e.to_string())?;
    let _ = state.db.lock().log_gateway("info", "settings", "gateway key regenerated");
    Ok(config.gateway_key)
}
