use crate::models::DashboardSummary;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn get_dashboard_summary(state: State<'_, AppState>) -> Result<DashboardSummary, String> {
    let db = state.db.lock();
    let accounts = db.list_accounts().map_err(|e| e.to_string())?;
    let total_accounts = accounts.len();
    let available_accounts = accounts.iter().filter(|a| a.enabled).count();

    let gateway_running = state.gateway.lock().is_some();

    let (today_cost, week_cost, month_cost) = db.total_usage().map_err(|e| e.to_string())?;

    Ok(DashboardSummary {
        total_accounts,
        available_accounts,
        gateway_running,
        today_cost,
        week_cost,
        month_cost,
    })
}
