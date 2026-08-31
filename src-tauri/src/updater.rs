use anyhow::{Context, anyhow};
use ocg_core::models::{AppConfig, ProxyListDirection, ProxyMode};
use ocg_core::state::CoreState;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::{UpdaterBuilder, UpdaterExt};

const UPDATE_ENDPOINT: &str =
    "https://github.com/klarkxy/opencode-go-mgr/releases/latest/download/latest.json";
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub fn configure(app: &AppHandle, state: CoreState) -> crate::Result<()> {
    if cfg!(debug_assertions) {
        return Ok(());
    }

    let Some(public_key) = embedded_public_key() else {
        let _ = state.db.lock().log_gateway(
            "warn",
            "update",
            "signed desktop updates are disabled because this build has no updater public key",
        );
        return Ok(());
    };

    #[cfg(target_os = "macos")]
    if std::env::current_exe()
        .ok()
        .is_some_and(|path| path.starts_with("/Volumes"))
    {
        let _ = state.db.lock().log_gateway(
            "warn",
            "update",
            "signed desktop updates are disabled while the app is running from a mounted DMG",
        );
        return Ok(());
    }

    app.plugin(
        tauri_plugin_updater::Builder::new()
            .pubkey(public_key)
            .build(),
    )?;

    let app = app.clone();
    let task_state = state.clone();
    state.set_desktop_update_starter(Arc::new(move |expected_version| {
        let app = app.clone();
        let state = task_state.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = install_update(app, state.clone(), expected_version).await {
                let message = format!("signed desktop update failed: {error:#}");
                state.set_desktop_update_failed(message.clone());
                let _ = state.db.lock().log_gateway("error", "update", &message);
            }
        });
        Ok(())
    }));

    Ok(())
}

async fn install_update(
    app: AppHandle,
    state: CoreState,
    expected_version: String,
) -> crate::Result<()> {
    let endpoint = UPDATE_ENDPOINT
        .parse()
        .context("invalid built-in updater endpoint")?;
    let updater = apply_updater_proxy(app.updater_builder(), &state.config())?
        .timeout(UPDATE_CHECK_TIMEOUT)
        .endpoints(vec![endpoint])?
        .build()?;
    let mut update = updater
        .check()
        .await?
        .ok_or_else(|| anyhow!("the signed update feed has no newer version"))?;

    if update.version != expected_version {
        return Err(anyhow!(
            "the signed update feed changed from expected version {expected_version} to {}",
            update.version
        ));
    }
    update.timeout = Some(UPDATE_DOWNLOAD_TIMEOUT);

    let progress_state = state.clone();
    let mut downloaded = 0_u64;
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                progress_state.set_desktop_update_progress(downloaded, total);
            },
            || {},
        )
        .await?;

    state.set_desktop_update_installing();
    let _ = state.db.lock().log_gateway(
        "info",
        "update",
        &format!("installing signed desktop update {expected_version}"),
    );
    update.install(&bytes)?;

    #[cfg(not(windows))]
    app.request_restart();

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpdaterProxySetting {
    FollowSystem,
    Manual(String),
    Disabled,
}

fn updater_proxy_setting(config: &AppConfig) -> UpdaterProxySetting {
    match config.proxy_mode {
        ProxyMode::Auto => UpdaterProxySetting::FollowSystem,
        ProxyMode::Manual => UpdaterProxySetting::Manual(config.proxy_url.clone()),
        ProxyMode::Direct => UpdaterProxySetting::Disabled,
        // Signed downloads are non-model-scoped traffic: they follow the
        // direction's default leg, matching `configured_builder`.
        ProxyMode::List => match config.proxy_list_direction {
            ProxyListDirection::Whitelist => UpdaterProxySetting::Disabled,
            ProxyListDirection::Blacklist => UpdaterProxySetting::Manual(config.proxy_url.clone()),
        },
    }
}

fn apply_updater_proxy(
    builder: UpdaterBuilder,
    config: &AppConfig,
) -> crate::Result<UpdaterBuilder> {
    match updater_proxy_setting(config) {
        UpdaterProxySetting::FollowSystem => Ok(builder),
        UpdaterProxySetting::Manual(url) => Ok(builder.proxy(
            url.parse()
                .context("invalid outbound proxy URL for signed desktop updates")?,
        )),
        UpdaterProxySetting::Disabled => Ok(builder.no_proxy()),
    }
}

fn embedded_public_key() -> Option<&'static str> {
    normalize_public_key(option_env!("TAURI_UPDATER_PUBLIC_KEY"))
}

fn normalize_public_key(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{UpdaterProxySetting, normalize_public_key, updater_proxy_setting};
    use ocg_core::models::{AppConfig, ProxyListDirection, ProxyMode};

    #[test]
    fn updater_public_key_must_be_non_empty() {
        assert_eq!(normalize_public_key(None), None);
        assert_eq!(normalize_public_key(Some("  \r\n")), None);
        assert_eq!(
            normalize_public_key(Some("  public-key  ")),
            Some("public-key")
        );
    }

    #[test]
    fn updater_follows_the_process_wide_proxy_policy() {
        let mut config = AppConfig::default();
        assert_eq!(
            updater_proxy_setting(&config),
            UpdaterProxySetting::FollowSystem
        );

        config.proxy_mode = ProxyMode::Direct;
        assert_eq!(
            updater_proxy_setting(&config),
            UpdaterProxySetting::Disabled
        );

        config.proxy_mode = ProxyMode::Manual;
        config.proxy_url = "http://127.0.0.1:7890".to_string();
        assert_eq!(
            updater_proxy_setting(&config),
            UpdaterProxySetting::Manual("http://127.0.0.1:7890".to_string())
        );
    }

    #[test]
    fn updater_follows_the_list_mode_default_leg_per_direction() {
        let mut config = AppConfig {
            gateway_key: "k".to_string(),
            proxy_mode: ProxyMode::List,
            proxy_url: "http://127.0.0.1:7890".to_string(),
            ..AppConfig::default()
        };

        config.proxy_list_direction = ProxyListDirection::Whitelist;
        assert_eq!(
            updater_proxy_setting(&config),
            UpdaterProxySetting::Disabled,
            "whitelist default leg is direct"
        );

        config.proxy_list_direction = ProxyListDirection::Blacklist;
        assert_eq!(
            updater_proxy_setting(&config),
            UpdaterProxySetting::Manual("http://127.0.0.1:7890".to_string()),
            "blacklist default leg is the manual proxy"
        );
    }
}
