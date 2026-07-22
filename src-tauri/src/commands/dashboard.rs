use crate::state::AppState;
use chrono::Utc;
use ocg_core::models::{DailyModelCost, DashboardSummary};
use tauri::State;

#[tauri::command]
pub fn get_dashboard_summary(state: State<'_, AppState>) -> Result<DashboardSummary, String> {
    let db = state.core.db.lock();
    let accounts = db.list_accounts().map_err(|e| e.to_string())?;
    let total_accounts = accounts.len();
    let now = Utc::now();
    let available_accounts = accounts
        .iter()
        .filter(|a| a.enabled && a.auth_error.is_none() && !a.is_cooling_at(now))
        .count();

    let gateway_running = state.core.gateway.lock().is_some();

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

/// Return per-day, per-model cost buckets for the last `days` days, for the
/// dashboard stacked-bar chart. Defaults to 30 days.
#[tauri::command]
pub fn get_daily_cost_by_model(
    state: State<'_, AppState>,
    days: Option<i64>,
) -> Result<Vec<DailyModelCost>, String> {
    state
        .core
        .db
        .lock()
        .daily_cost_by_model(days.unwrap_or(30))
        .map_err(|e| e.to_string())
}
