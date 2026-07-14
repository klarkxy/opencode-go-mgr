use crate::crypto::KeyCipher;
use crate::db::Database;
use crate::models::{AppConfig, normalize_client_root_url};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

const CLIENT_ROOT_URL_ENV: &str = "OCG_CLIENT_ROOT_URL";

pub struct GatewayHandle {
    pub port: u16,
    pub shutdown: tokio::sync::oneshot::Sender<()>,
    pub task: tokio::task::JoinHandle<()>,
}

pub type AutoStartSync = fn(bool) -> crate::Result<()>;

// Note: Mutex lock ordering is (1) settings_update, (2) db, (3) config,
// (4) http_client, (5) gateway.
// Never acquire in reverse order; always drop one before acquiring another where possible.
pub struct CoreStateInner {
    pub db: Mutex<Database>,
    pub config: Mutex<AppConfig>,
    client_root_url_override: Option<String>,
    pub settings_update: Mutex<()>,
    pub gateway: Mutex<Option<GatewayHandle>>,
    pub dashboard_session_token: String,
    dashboard_local_mode: AtomicBool,
    auto_start_sync: OnceLock<AutoStartSync>,
    pub dashboard_dir: Mutex<Option<PathBuf>>,
    http_client: Mutex<reqwest::Client>,
    pub data_dir: PathBuf,
    pub cipher: Arc<dyn KeyCipher + Send + Sync>,
}

pub type CoreState = Arc<CoreStateInner>;

impl CoreStateInner {
    pub fn new(
        db: Database,
        data_dir: PathBuf,
        cipher: Arc<dyn KeyCipher + Send + Sync>,
    ) -> crate::Result<Self> {
        let client_root_url_override = client_root_url_override_from_env()?;
        Self::new_with_client_root_url_override(db, data_dir, cipher, client_root_url_override)
    }

    fn new_with_client_root_url_override(
        db: Database,
        data_dir: PathBuf,
        cipher: Arc<dyn KeyCipher + Send + Sync>,
        client_root_url_override: Option<String>,
    ) -> crate::Result<Self> {
        crate::auth::bootstrap_admin_from_env(&db)?;
        let (config, needs_persist) = load_config(&db)?;
        config.validate().map_err(anyhow::Error::msg)?;
        if needs_persist {
            // Persist generated defaults and drop fields removed from AppConfig.
            save_config(&db, &config)?;
        }
        let http_client = build_http_client(&config)?;
        Ok(Self {
            db: Mutex::new(db),
            config: Mutex::new(config),
            client_root_url_override,
            settings_update: Mutex::new(()),
            gateway: Mutex::new(None),
            dashboard_session_token: uuid::Uuid::new_v4().simple().to_string(),
            dashboard_local_mode: AtomicBool::new(false),
            auto_start_sync: OnceLock::new(),
            dashboard_dir: Mutex::new(None),
            http_client: Mutex::new(http_client),
            data_dir,
            cipher,
        })
    }

    pub fn config(&self) -> AppConfig {
        self.config.lock().clone()
    }

    pub fn settings_config(&self) -> AppConfig {
        let mut config = self.config();
        if let Some(client_root_url) = &self.client_root_url_override {
            config.client_root_url.clone_from(client_root_url);
        }
        config
    }

    pub fn client_root_url_from_env(&self) -> bool {
        self.client_root_url_override.is_some()
    }

    pub fn upstream_context(&self) -> (AppConfig, reqwest::Client) {
        let config = self.config.lock();
        let client = self.http_client.lock();
        (config.clone(), client.clone())
    }

    pub fn active_gateway_port(&self) -> u16 {
        let configured = self.config().gateway_port;
        self.gateway
            .lock()
            .as_ref()
            .map(|handle| handle.port)
            .unwrap_or(configured)
    }

    pub fn set_dashboard_local_mode(&self, local: bool) {
        self.dashboard_local_mode.store(local, Ordering::Relaxed);
    }

    pub fn dashboard_local_mode(&self) -> bool {
        self.dashboard_local_mode.load(Ordering::Relaxed)
    }

    pub fn set_auto_start_sync(&self, sync: AutoStartSync) {
        assert!(
            self.auto_start_sync.set(sync).is_ok(),
            "auto-start sync is already configured"
        );
    }

    pub fn auto_start_supported(&self) -> bool {
        self.auto_start_sync.get().is_some()
    }

    pub fn sync_auto_start(&self, enabled: bool) -> crate::Result<()> {
        let sync = self
            .auto_start_sync
            .get()
            .ok_or_else(|| anyhow::anyhow!("auto-start is unavailable in this runtime"))?;
        sync(enabled)
    }

    pub fn set_config(&self, mut config: AppConfig) -> crate::Result<()> {
        if self.client_root_url_override.is_some() {
            config.client_root_url = self.config.lock().client_root_url.clone();
        }
        config.claude_desktop_models.normalize();
        config.validate().map_err(anyhow::Error::msg)?;
        let http_client = build_http_client(&config)?;
        {
            let db = self.db.lock();
            save_config(&db, &config)?;
        }
        let mut current_config = self.config.lock();
        let mut current_client = self.http_client.lock();
        *current_config = config;
        *current_client = http_client;
        Ok(())
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    pub fn set_dashboard_dir(&self, dir: Option<PathBuf>) {
        *self.dashboard_dir.lock() = dir;
    }

    pub fn dashboard_dir(&self) -> Option<PathBuf> {
        self.dashboard_dir.lock().clone()
    }

    pub fn encrypt_key(&self, plaintext: &str) -> crate::Result<String> {
        self.cipher.encrypt(plaintext)
    }

    pub fn decrypt_key(&self, ciphertext: &str) -> crate::Result<String> {
        self.cipher.decrypt(ciphertext)
    }
}

fn client_root_url_override_from_env() -> crate::Result<Option<String>> {
    match std::env::var(CLIENT_ROOT_URL_ENV) {
        Ok(value) => normalize_client_root_url_override(Some(&value))
            .map_err(|error| anyhow::anyhow!("{CLIENT_ROOT_URL_ENV}: {error}")),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(anyhow::anyhow!(
            "{CLIENT_ROOT_URL_ENV} must contain valid Unicode"
        )),
    }
}

fn normalize_client_root_url_override(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = normalize_client_root_url(value)?;
    Ok((!value.is_empty()).then_some(value))
}

/// Loads persisted config. The `bool` marks config that needs canonical rewriting.
fn load_config(db: &Database) -> crate::Result<(AppConfig, bool)> {
    let mut config = AppConfig::default();
    let mut needs_persist = false;
    if let Some(value) = db.get_setting("config")? {
        config = serde_json::from_str(&value)?;
        config.claude_desktop_models.normalize();
        needs_persist = serde_json::to_string(&config)? != value;
    }
    if config.gateway_key.is_empty() {
        config.gateway_key = generate_gateway_key();
        needs_persist = true;
    }
    Ok((config, needs_persist))
}

fn save_config(db: &Database, config: &AppConfig) -> crate::Result<()> {
    db.set_setting("config", &serde_json::to_string(config)?)?;
    Ok(())
}

fn build_http_client(config: &AppConfig) -> crate::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
        .read_timeout(Duration::from_secs(config.stream_idle_timeout_secs))
        .build()?)
}

fn generate_gateway_key() -> String {
    format!("ocg-{}-{}", random_word(), random_word())
}

pub fn random_word() -> String {
    // Use UUID v4 for proper randomness (122 bits entropy)
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::{CoreStateInner, normalize_client_root_url_override};
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::db::Database;
    use crate::models::AppConfig;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn temp_data_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        dir.push(format!("ocg-state-test-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("test data directory should be created");
        dir
    }

    #[test]
    fn client_root_url_override_normalizes_non_empty_values() {
        assert_eq!(normalize_client_root_url_override(None), Ok(None));
        assert_eq!(normalize_client_root_url_override(Some("   ")), Ok(None));
        assert_eq!(
            normalize_client_root_url_override(Some(" https://ocg.example.com/proxy/v1/ ")),
            Ok(Some("https://ocg.example.com/proxy".to_string()))
        );
        assert!(
            normalize_client_root_url_override(Some("https://ocg.example.com/v1/responses"))
                .is_err()
        );
    }

    #[test]
    fn client_root_url_override_never_replaces_persisted_setting() {
        let dir = temp_data_dir("client-root-override");
        let db = Database::open(dir.clone()).expect("test database should open");
        let persisted = AppConfig {
            gateway_key: "test-gateway-key".to_string(),
            client_root_url: "https://saved.example.com".to_string(),
            ..AppConfig::default()
        };
        db.set_setting(
            "config",
            &serde_json::to_string(&persisted).expect("test config should serialize"),
        )
        .expect("test config should persist");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("state-test"));
        let state = CoreStateInner::new_with_client_root_url_override(
            db,
            dir.clone(),
            cipher,
            Some("https://environment.example.com".to_string()),
        )
        .expect("state should initialize");

        assert_eq!(
            state.settings_config().client_root_url,
            "https://environment.example.com"
        );
        let mut submitted = state.settings_config();
        submitted.connect_timeout_secs = 45;
        state
            .set_config(submitted)
            .expect("other settings should save while the override is active");
        assert_eq!(state.config().client_root_url, "https://saved.example.com");
        let stored = state
            .db
            .lock()
            .get_setting("config")
            .expect("stored config should be readable")
            .expect("stored config should exist");
        let stored: AppConfig =
            serde_json::from_str(&stored).expect("stored config should deserialize");
        assert_eq!(stored.client_root_url, "https://saved.example.com");
        assert_eq!(stored.connect_timeout_secs, 45);

        drop(state);
        fs::remove_dir_all(dir).expect("test data directory should be removed");
    }

    #[test]
    fn legacy_config_gets_persisted_claude_desktop_defaults() {
        let dir = temp_data_dir("claude-desktop-migration");
        let db = Database::open(dir.clone()).expect("test database should open");
        let mut legacy = serde_json::to_value(AppConfig {
            gateway_key: "test-gateway-key".to_string(),
            ..AppConfig::default()
        })
        .expect("test config should serialize");
        legacy
            .as_object_mut()
            .expect("test config should be an object")
            .remove("claude_desktop_models");
        db.set_setting("config", &legacy.to_string())
            .expect("legacy config should persist");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("state-test"));
        let state = CoreStateInner::new(db, dir.clone(), cipher).expect("state should initialize");

        assert_eq!(
            state.config().claude_desktop_models.resolved(),
            AppConfig::default().claude_desktop_models.resolved()
        );
        let stored = state
            .db
            .lock()
            .get_setting("config")
            .expect("stored config should be readable")
            .expect("stored config should exist");
        assert!(stored.contains("claude_desktop_models"));

        drop(state);
        fs::remove_dir_all(dir).expect("test data directory should be removed");
    }
}
