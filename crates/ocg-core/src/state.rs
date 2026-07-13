use crate::crypto::KeyCipher;
use crate::db::Database;
use crate::models::AppConfig;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

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
        crate::auth::bootstrap_admin_from_env(&db)?;
        let (config, needs_persist) = load_config(&db)?;
        config.validate_timeouts().map_err(anyhow::Error::msg)?;
        if needs_persist {
            // Persist generated defaults and drop fields removed from AppConfig.
            save_config(&db, &config)?;
        }
        let http_client = build_http_client(&config)?;
        Ok(Self {
            db: Mutex::new(db),
            config: Mutex::new(config),
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

    pub fn set_config(&self, config: AppConfig) -> crate::Result<()> {
        config.validate_timeouts().map_err(anyhow::Error::msg)?;
        let http_client = build_http_client(&config)?;
        {
            let db = self.db.lock();
            save_config(&*db, &config)?;
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
        self.cipher.encrypt(plaintext).map_err(|e| e.into())
    }

    pub fn decrypt_key(&self, ciphertext: &str) -> crate::Result<String> {
        self.cipher.decrypt(ciphertext).map_err(|e| e.into())
    }
}

/// Loads persisted config. The `bool` marks config that needs canonical rewriting.
fn load_config(db: &Database) -> crate::Result<(AppConfig, bool)> {
    let mut config = AppConfig::default();
    let mut needs_persist = false;
    if let Some(value) = db.get_setting("config")? {
        config = serde_json::from_str(&value)?;
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
