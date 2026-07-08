pub mod commands;
pub mod crypto;
pub mod db;
pub mod gateway;
pub mod models;
pub mod state;
pub mod tray;

pub type Result<T> = anyhow::Result<T>;

use state::AppStateInner;
use std::path::PathBuf;
use tauri::Manager;

pub fn run() {
    let data_dir = data_dir();
    let db = match db::Database::open(data_dir.clone()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to open database: {}", e);
            std::process::exit(1);
        }
    };

    let state = match AppStateInner::new(db, data_dir.clone()) {
        Ok(s) => std::sync::Arc::new(s),
        Err(e) => {
            eprintln!("failed to initialize state: {}", e);
            std::process::exit(1);
        }
    };

    // Start gateway on startup
    let config = state.config();
    let gateway_state = state.clone();
    let gateway_handle = match tauri::async_runtime::block_on(gateway::start_gateway(
        gateway_state,
        config.gateway_port,
    )) {
        Ok(handle) => {
            let _ = state.db.lock().log_gateway("info", "gateway", &format!("gateway started on port {}", handle.port));
            Some(handle)
        }
        Err(e) => {
            let _ = state.db.lock().log_gateway("error", "gateway", &format!("failed to start gateway: {}", e));
            None
        }
    };

    *state.gateway.lock() = gateway_handle;

    let app_state = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_http::init())
        .manage(app_state.clone())
        .setup(move |app| {
            tray::setup_tray(app)?;
            let window = app.get_webview_window("main").expect("main window not found");
            window.show()?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().ok();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::account::get_accounts,
            commands::account::create_account,
            commands::account::update_account,
            commands::account::delete_account,
            commands::account::toggle_account,
            commands::account::test_account,
            commands::account::reset_circuit,
            commands::account::get_account_usage,
            commands::setting::get_settings,
            commands::setting::update_settings,
            commands::setting::regenerate_gateway_key,
            commands::gateway::get_gateway_status,
            commands::gateway::restart_gateway,
            commands::log::get_gateway_logs,
            commands::log::get_forward_logs,
            commands::dashboard::get_dashboard_summary,
            commands::browser::open_browser,
            commands::browser::close_browser,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |_app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(handle) = state.gateway.lock().take() {
                    gateway::stop_gateway(handle);
                }
                let _ = state.db.lock().log_gateway("info", "gateway", "application exiting");
            }
        });
}

fn data_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    home.join(".ocg-mgr")
}
