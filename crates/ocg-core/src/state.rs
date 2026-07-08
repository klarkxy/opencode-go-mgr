use crate::crypto::KeyCipher;
use crate::db::Database;
use crate::models::AppConfig;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use parking_lot::Mutex;

pub struct GatewayHandle {
    pub port: u16,
    pub shutdown: tokio::sync::oneshot::Sender<()>,
    pub task: tokio::task::JoinHandle<()>,
}

// Note: Mutex lock ordering is (1) db, (2) config, (3) gateway.
// Never acquire in reverse order; always drop one before acquiring another where possible.
pub struct CoreStateInner {
    pub db: Mutex<Database>,
    pub config: Mutex<AppConfig>,
    pub gateway: Mutex<Option<GatewayHandle>>,
    pub round_robin_counter: Arc<AtomicUsize>,
    pub http_client: reqwest::Client,
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
        let (config, needs_persist) = load_config(&db)?;
        if needs_persist {
            // ponytail: persist the auto-generated gateway_key once so it survives restarts;
            // load_config returns needs_persist=true when it had to invent the key.
            save_config(&db, &config)?;
        }
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;
        Ok(Self {
            db: Mutex::new(db),
            config: Mutex::new(config),
            gateway: Mutex::new(None),
            round_robin_counter: Arc::new(AtomicUsize::new(0)),
            http_client,
            data_dir,
            cipher,
        })
    }

    pub fn config(&self) -> AppConfig {
        self.config.lock().clone()
    }

    pub fn set_config(&self, config: AppConfig) -> crate::Result<()> {
        {
            let db = self.db.lock();
            save_config(&*db, &config)?;
        }
        *self.config.lock() = config;
        Ok(())
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    pub fn encrypt_key(&self, plaintext: &str) -> crate::Result<String> {
        self.cipher.encrypt(plaintext).map_err(|e| e.into())
    }

    pub fn decrypt_key(&self, ciphertext: &str) -> crate::Result<String> {
        self.cipher.decrypt(ciphertext).map_err(|e| e.into())
    }
}

/// Loads the persisted config. The `bool` is `true` when the config was
/// freshly created with an auto-generated gateway_key and must be persisted.
fn load_config(db: &Database) -> crate::Result<(AppConfig, bool)> {
    let mut config = AppConfig::default();
    let mut needs_persist = false;
    if let Some(value) = db.get_setting("config")? {
        config = serde_json::from_str(&value)?;
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

fn generate_gateway_key() -> String {
    format!("ocg-{}-{}", random_word(), random_word())
}

pub fn random_word() -> String {
    // Use UUID v4 for proper randomness (122 bits entropy)
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}
