use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, menu::Menu, menu::MenuItem};
use tauri_plugin_shell::ShellExt;

pub fn open_dashboard(app: &AppHandle) {
    let state = app.state::<crate::state::AppState>();
    let url = format!(
        "http://127.0.0.1:{}/dashboard/",
        state.core.active_gateway_port()
    );
    #[allow(deprecated)]
    let opened = app.shell().open(url, None);
    if let Err(e) = opened {
        let _ = state.core.db.lock().log_gateway(
            "error",
            "dashboard",
            &format!("failed to open dashboard: {}", e),
        );
    }
}

pub fn setup_tray(app: &tauri::App) -> crate::Result<()> {
    let open_i = MenuItem::with_id(app, "open", "打开管理界面", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_i, &quit_i])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                open_dashboard(app);
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                open_dashboard(app);
            }
        })
        .build(app)?;

    Ok(())
}
