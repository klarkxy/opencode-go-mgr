pub mod autostart;
pub mod commands;
pub mod state;
pub mod tray;

pub type Result<T> = anyhow::Result<T>;

use ocg_core::crypto::KeyCipher;
#[cfg(windows)]
use ocg_core::crypto::MachineBoundCipher;
#[cfg(not(windows))]
use ocg_core::crypto::load_or_create_static_cipher;
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::state::CoreStateInner;
use parking_lot::Mutex;
use state::GuiState;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    let data_dir = data_dir();
    let cipher = match load_cipher(&data_dir) {
        Ok(cipher) => cipher,
        Err(e) => {
            eprintln!("failed to initialize encryption: {}", e);
            std::process::exit(1);
        }
    };
    let db = match Database::open(data_dir.clone()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to open database: {}", e);
            std::process::exit(1);
        }
    };

    let core_state = match CoreStateInner::new(db, data_dir.clone(), cipher) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("failed to initialize state: {}", e);
            std::process::exit(1);
        }
    };

    // Start gateway on startup
    let config = core_state.config();
    let gateway_state = core_state.clone();
    let gateway_handle = match tauri::async_runtime::block_on(gateway::start_gateway(
        gateway_state,
        config.gateway_port,
    )) {
        Ok(handle) => {
            let _ = core_state.db.lock().log_gateway(
                "info",
                "gateway",
                &format!("gateway started on port {}", handle.port),
            );
            Some(handle)
        }
        Err(e) => {
            let _ = core_state.db.lock().log_gateway(
                "error",
                "gateway",
                &format!("failed to start gateway: {}", e),
            );
            None
        }
    };

    *core_state.gateway.lock() = gateway_handle;

    let gui_state = Arc::new(GuiState {
        core: core_state.clone(),
        current_browser_window: Mutex::new(None),
    });

    let app_state = gui_state.clone();
    let setup_core_state = core_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|arg| arg == "--startup") {
                tray::open_dashboard(app);
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .manage(app_state.clone())
        .setup(move |app| {
            if let Ok(resource_dir) = app.path().resource_dir() {
                setup_core_state.set_dashboard_dir(Some(resource_dir.join("dist")));
            }
            tray::setup_tray(app)?;
            if !autostart::is_startup_launch() {
                tray::open_dashboard(app.handle());
            }
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
            commands::account::get_account_usage,
            commands::account::reset_account_cooldown,
            commands::setting::get_settings,
            commands::setting::update_settings,
            commands::setting::regenerate_gateway_key,
            commands::gateway::get_gateway_status,
            commands::gateway::restart_gateway,
            commands::log::get_gateway_logs,
            commands::log::get_forward_logs,
            commands::dashboard::get_dashboard_summary,
            commands::dashboard::get_daily_cost_by_model,
            commands::browser::open_browser,
            commands::browser::close_browser,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |_app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(handle) = core_state.gateway.lock().take() {
                    gateway::stop_gateway(handle);
                }
                let _ = core_state
                    .db
                    .lock()
                    .log_gateway("info", "gateway", "application exiting");
            }
        });
}

fn load_cipher(data_dir: &std::path::Path) -> Result<Arc<dyn KeyCipher + Send + Sync>> {
    #[cfg(windows)]
    {
        let _ = data_dir;
        Ok(Arc::new(MachineBoundCipher::new()))
    }
    #[cfg(not(windows))]
    {
        Ok(Arc::new(load_or_create_static_cipher(data_dir)?))
    }
}

fn data_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    home.join(".ocg-mgr")
}
