use crate::state::AppState;
use tauri::webview::WebviewWindowBuilder;
use tauri::{AppHandle, Manager, State};

const OCG_CONSOLE_URL: &str = "https://opencode.ai/zen/go";
const BROWSER_WINDOW_LABEL: &str = "ocg-browser";

#[tauri::command]
pub async fn open_browser(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<String, String> {
    if account_id.contains(['/', '\\']) || account_id.contains("..") {
        return Err("invalid account id".to_string());
    }
    {
        let db = state.core.db.lock();
        db.get_account(&account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "account not found".to_string())?;
    }
    let profile_dir = state.core.data_dir().join("profiles").join(&account_id);
    std::fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;

    // Close existing browser window if any
    {
        let mut current = state.current_browser_window.lock();
        if let Some(label) = current.take() {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.close();
            }
        }
    }

    // ponytail: take first 8 chars, or fewer if ID is shorter
    let short_id = account_id.chars().take(8).collect::<String>();
    let label = format!("{}-{}", BROWSER_WINDOW_LABEL, short_id);
    let url = tauri::Url::parse(OCG_CONSOLE_URL).map_err(|e| e.to_string())?;

    let window = WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(url))
        .title(format!("OCG Browser - {}", account_id))
        .inner_size(1200.0, 800.0)
        .data_directory(profile_dir)
        .build()
        .map_err(|e| e.to_string())?;

    {
        let mut current = state.current_browser_window.lock();
        *current = Some(window.label().to_string());
    }

    let _ = state.core.db.lock().log_gateway(
        "info",
        "browser",
        &format!("opened browser for account {}", account_id),
    );
    Ok(OCG_CONSOLE_URL.to_string())
}

#[tauri::command]
pub fn close_browser(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut current = state.current_browser_window.lock();
    if let Some(label) = current.take() {
        if let Some(window) = app.get_webview_window(&label) {
            window.close().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
