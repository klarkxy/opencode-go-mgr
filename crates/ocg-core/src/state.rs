use crate::crypto::KeyCipher;
use crate::db::Database;
use crate::models::{AppConfig, normalize_client_root_url};
use crate::pricing::{
    PricingEstimate, PricingSnapshot, embedded_seed, ensure_current_adjustment_policy,
};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::fmt;
use std::path::PathBuf;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

const CLIENT_ROOT_URL_ENV: &str = "OCG_CLIENT_ROOT_URL";

pub struct GatewayHandle {
    pub port: u16,
    pub shutdown: tokio::sync::oneshot::Sender<()>,
    pub task: tokio::task::JoinHandle<()>,
}

pub type AutoStartSync = fn(bool) -> crate::Result<()>;

pub type DockVisibilitySync = Arc<dyn Fn(bool) -> crate::Result<()> + Send + Sync + 'static>;

pub type DesktopUpdateStarter = Arc<dyn Fn(String) -> crate::Result<()> + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DesktopUpdatePhase {
    Idle,
    Checking,
    Downloading,
    Installing,
    Failed,
}

impl DesktopUpdatePhase {
    fn is_busy(self) -> bool {
        matches!(self, Self::Checking | Self::Downloading | Self::Installing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DesktopUpdateStatus {
    pub phase: DesktopUpdatePhase,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
    pub current_version: String,
    pub install_supported: bool,
}

impl DesktopUpdateStatus {
    fn new() -> Self {
        Self {
            phase: DesktopUpdatePhase::Idle,
            downloaded: 0,
            total: None,
            error: None,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            install_supported: false,
        }
    }
}

#[derive(Debug)]
pub enum DesktopUpdateStartError {
    Unsupported,
    Busy,
    Starter(anyhow::Error),
}

impl fmt::Display for DesktopUpdateStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("desktop update installation is unavailable"),
            Self::Busy => f.write_str("a desktop update is already in progress"),
            Self::Starter(error) => write!(f, "failed to start desktop update: {error}"),
        }
    }
}

impl std::error::Error for DesktopUpdateStartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Starter(error) => Some(error.as_ref()),
            Self::Unsupported | Self::Busy => None,
        }
    }
}

// Note: Mutex lock ordering is (1) settings_update, (2) db, (3) config,
// (4) http_client, (5) gateway, (6) pricing. desktop_update_status and the async
// pricing_refresh guard are never held while acquiring another sync lock.
// Never acquire in reverse order; always drop one before acquiring another where possible.
pub struct CoreStateInner {
    pub db: Mutex<Database>,
    pub config: Mutex<AppConfig>,
    client_root_url_override: Option<String>,
    pub settings_update: Mutex<()>,
    settings_revision: AtomicU64,
    pub gateway: Mutex<Option<GatewayHandle>>,
    pub dashboard_session_token: String,
    dashboard_local_mode: AtomicBool,
    auto_start_sync: OnceLock<AutoStartSync>,
    dock_visibility_sync: OnceLock<DockVisibilitySync>,
    desktop_update_starter: OnceLock<DesktopUpdateStarter>,
    desktop_update_status: Mutex<DesktopUpdateStatus>,
    pub dashboard_dir: Mutex<Option<PathBuf>>,
    http_client: Mutex<reqwest::Client>,
    pricing: RwLock<Arc<PricingSnapshot>>,
    pub pricing_refresh: tokio::sync::Mutex<()>,
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
        let pricing = match db.latest_pricing_snapshot()? {
            Some(snapshot) => {
                let previous_revision = snapshot.revision.clone();
                let snapshot = ensure_current_adjustment_policy(snapshot);
                if snapshot.revision != previous_revision {
                    db.insert_pricing_snapshot(&snapshot)?;
                }
                snapshot
            }
            None => {
                let snapshot = embedded_seed();
                db.insert_pricing_snapshot(&snapshot)?;
                snapshot
            }
        };
        let http_client = build_http_client(&config)?;
        Ok(Self {
            db: Mutex::new(db),
            config: Mutex::new(config),
            client_root_url_override,
            settings_update: Mutex::new(()),
            // Use a per-runtime random epoch so a browser tab left open across a
            // process restart cannot accidentally match the new runtime's first
            // revision. The low 48 bits leave ample room for monotonic increments.
            settings_revision: AtomicU64::new(
                (uuid::Uuid::new_v4().as_u128() as u64) & 0x0000_FFFF_FFFF_FFFF,
            ),
            gateway: Mutex::new(None),
            dashboard_session_token: uuid::Uuid::new_v4().simple().to_string(),
            dashboard_local_mode: AtomicBool::new(false),
            auto_start_sync: OnceLock::new(),
            dock_visibility_sync: OnceLock::new(),
            desktop_update_starter: OnceLock::new(),
            desktop_update_status: Mutex::new(DesktopUpdateStatus::new()),
            dashboard_dir: Mutex::new(None),
            http_client: Mutex::new(http_client),
            pricing: RwLock::new(Arc::new(pricing)),
            pricing_refresh: tokio::sync::Mutex::new(()),
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

    pub fn settings_revision(&self) -> u64 {
        self.settings_revision.load(Ordering::Acquire)
    }

    pub fn client_root_url_from_env(&self) -> bool {
        self.client_root_url_override.is_some()
    }

    pub fn upstream_context(&self) -> (AppConfig, reqwest::Client) {
        let config = self.config.lock();
        let client = self.http_client.lock();
        (config.clone(), client.clone())
    }

    pub fn pricing_snapshot(&self) -> Arc<PricingSnapshot> {
        self.pricing.read().clone()
    }

    pub fn activate_pricing_snapshot(&self, snapshot: PricingSnapshot) -> crate::Result<()> {
        // Keep database persistence and the in-memory active pointer behind the
        // documented db -> pricing lock order, so readers never observe a
        // partially activated revision.
        let db = self.db.lock();
        let mut active = self.pricing.write();
        db.insert_pricing_snapshot(&snapshot)?;
        *active = Arc::new(snapshot);
        Ok(())
    }

    pub fn estimate_cost(
        &self,
        model: &str,
        prompt: i64,
        completion: i64,
        cached: i64,
        cache_creation: i64,
        service_tier: Option<&str>,
    ) -> PricingEstimate {
        self.pricing_snapshot().estimate(
            model,
            prompt,
            completion,
            cached,
            cache_creation,
            service_tier,
        )
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

    pub fn set_dock_visibility_sync(&self, sync: DockVisibilitySync) {
        assert!(
            self.dock_visibility_sync.set(sync).is_ok(),
            "dock visibility sync is already configured"
        );
    }

    pub fn dock_visibility_supported(&self) -> bool {
        self.dock_visibility_sync.get().is_some()
    }

    pub fn sync_dock_visibility(&self, visible: bool) -> crate::Result<()> {
        let sync = self
            .dock_visibility_sync
            .get()
            .ok_or_else(|| anyhow::anyhow!("dock visibility is unavailable in this runtime"))?;
        sync(visible)
    }

    pub fn set_desktop_update_starter(&self, starter: DesktopUpdateStarter) {
        assert!(
            self.desktop_update_starter.set(starter).is_ok(),
            "desktop update starter is already configured"
        );
        self.desktop_update_status.lock().install_supported = true;
    }

    pub fn desktop_update_supported(&self) -> bool {
        self.desktop_update_starter.get().is_some()
    }

    pub fn desktop_update_status(&self) -> DesktopUpdateStatus {
        self.desktop_update_status.lock().clone()
    }

    pub fn start_desktop_update(
        &self,
        expected_version: String,
    ) -> Result<(), DesktopUpdateStartError> {
        let starter = self
            .desktop_update_starter
            .get()
            .cloned()
            .ok_or(DesktopUpdateStartError::Unsupported)?;
        {
            let mut status = self.desktop_update_status.lock();
            if status.phase.is_busy() {
                return Err(DesktopUpdateStartError::Busy);
            }
            status.phase = DesktopUpdatePhase::Checking;
            status.downloaded = 0;
            status.total = None;
            status.error = None;
            status.install_supported = true;
        }

        if let Err(error) = starter(expected_version) {
            self.set_desktop_update_failed(error.to_string());
            return Err(DesktopUpdateStartError::Starter(error));
        }
        Ok(())
    }

    pub fn set_desktop_update_progress(&self, downloaded: u64, total: Option<u64>) -> bool {
        let mut status = self.desktop_update_status.lock();
        if !matches!(
            status.phase,
            DesktopUpdatePhase::Checking | DesktopUpdatePhase::Downloading
        ) {
            return false;
        }
        status.phase = DesktopUpdatePhase::Downloading;
        status.downloaded = downloaded;
        status.total = total;
        status.error = None;
        true
    }

    pub fn set_desktop_update_installing(&self) -> bool {
        let mut status = self.desktop_update_status.lock();
        if !matches!(
            status.phase,
            DesktopUpdatePhase::Checking | DesktopUpdatePhase::Downloading
        ) {
            return false;
        }
        status.phase = DesktopUpdatePhase::Installing;
        status.error = None;
        true
    }

    pub fn set_desktop_update_failed(&self, error: impl Into<String>) {
        let mut status = self.desktop_update_status.lock();
        status.phase = DesktopUpdatePhase::Failed;
        status.error = Some(error.into());
    }

    pub fn set_desktop_update_idle(&self) {
        let mut status = self.desktop_update_status.lock();
        status.phase = DesktopUpdatePhase::Idle;
        status.downloaded = 0;
        status.total = None;
        status.error = None;
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
        self.settings_revision.fetch_add(1, Ordering::AcqRel);
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
    // v1.4.2 shipped 30/120/300 as one default tuple. Migrate that exact,
    // untouched tuple once while preserving every user-customized combination.
    if (
        config.connect_timeout_secs,
        config.non_stream_timeout_secs,
        config.stream_idle_timeout_secs,
    ) == (30, 120, 300)
    {
        config.non_stream_timeout_secs = 900;
        needs_persist = true;
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
        // Drop idle pooled connections earlier than the default so a stale connection
        // closed by the upstream/CDN isn't reused. Keep-alive probes further reduce
        // silent drops for long-lived gateways.
        .pool_idle_timeout(Duration::from_secs(30))
        .tcp_keepalive(Duration::from_secs(30))
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
    use super::{
        CoreStateInner, DesktopUpdatePhase, DesktopUpdateStartError,
        normalize_client_root_url_override,
    };
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::db::Database;
    use crate::models::AppConfig;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Barrier, Mutex as StdMutex};

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
    fn settings_revision_advances_only_after_successful_commit() {
        let dir = temp_data_dir("settings-revision");
        let db = Database::open(dir.clone()).expect("test database should open");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("state-test"));
        let state = CoreStateInner::new(db, dir.clone(), cipher).expect("state should initialize");
        let initial_revision = state.settings_revision();

        let mut valid = state.config();
        valid.connect_timeout_secs += 1;
        state.set_config(valid).expect("valid settings should save");
        assert_eq!(state.settings_revision(), initial_revision + 1);

        let committed_revision = state.settings_revision();
        let mut invalid = state.config();
        invalid.connect_timeout_secs = 0;
        assert!(state.set_config(invalid).is_err());
        assert_eq!(state.settings_revision(), committed_revision);

        drop(state);
        fs::remove_dir_all(dir).expect("test data directory should be removed");
    }

    #[test]
    fn desktop_update_state_machine_is_serializable_atomic_and_retriable() {
        let dir = temp_data_dir("desktop-update-state");
        let db = Database::open(dir.clone()).expect("test database should open");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("state-test"));
        let state = Arc::new(
            CoreStateInner::new(db, dir.clone(), cipher).expect("state should initialize"),
        );

        assert_eq!(
            serde_json::to_value(state.desktop_update_status()).expect("status should serialize"),
            serde_json::json!({
                "phase": "idle",
                "downloaded": 0,
                "total": null,
                "error": null,
                "current_version": env!("CARGO_PKG_VERSION"),
                "install_supported": false,
            })
        );
        assert!(!state.set_desktop_update_progress(1, Some(2)));
        assert!(!state.set_desktop_update_installing());

        let started_versions = Arc::new(StdMutex::new(Vec::new()));
        let captured_versions = started_versions.clone();
        state.set_desktop_update_starter(Arc::new(move |expected_version| {
            captured_versions
                .lock()
                .expect("captured versions lock should work")
                .push(expected_version);
            Ok(())
        }));
        assert!(state.desktop_update_supported());
        assert!(state.desktop_update_status().install_supported);

        let barrier = Arc::new(Barrier::new(3));
        let threads = [state.clone(), state.clone()].map(|state| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                state.start_desktop_update("9.9.9".to_string())
            })
        });
        barrier.wait();
        let results = threads.map(|thread| thread.join().expect("start thread should not panic"));
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(DesktopUpdateStartError::Busy)))
                .count(),
            1
        );
        assert_eq!(
            started_versions
                .lock()
                .expect("started versions lock should work")
                .as_slice(),
            ["9.9.9"]
        );
        assert_eq!(
            state.desktop_update_status().phase,
            DesktopUpdatePhase::Checking
        );

        assert!(state.set_desktop_update_progress(25, Some(100)));
        let downloading = state.desktop_update_status();
        assert_eq!(downloading.phase, DesktopUpdatePhase::Downloading);
        assert_eq!(downloading.downloaded, 25);
        assert_eq!(downloading.total, Some(100));
        assert!(state.set_desktop_update_installing());
        assert!(!state.set_desktop_update_progress(50, Some(100)));
        state.set_desktop_update_failed("install failed");
        let failed = state.desktop_update_status();
        assert_eq!(failed.phase, DesktopUpdatePhase::Failed);
        assert_eq!(failed.error.as_deref(), Some("install failed"));

        state
            .start_desktop_update("10.0.0".to_string())
            .expect("a failed update should be retriable");
        let retrying = state.desktop_update_status();
        assert_eq!(retrying.phase, DesktopUpdatePhase::Checking);
        assert_eq!(retrying.downloaded, 0);
        assert_eq!(retrying.total, None);
        assert_eq!(retrying.error, None);
        assert_eq!(
            started_versions
                .lock()
                .expect("started versions lock should work")
                .as_slice(),
            ["9.9.9", "10.0.0"]
        );

        state.set_desktop_update_idle();
        assert_eq!(
            state.desktop_update_status().phase,
            DesktopUpdatePhase::Idle
        );
        drop(state);
        fs::remove_dir_all(dir).expect("test data directory should be removed");
    }

    #[test]
    fn desktop_update_starter_failure_is_reported_in_status() {
        let dir = temp_data_dir("desktop-update-start-failure");
        let db = Database::open(dir.clone()).expect("test database should open");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("state-test"));
        let state = CoreStateInner::new(db, dir.clone(), cipher).expect("state should initialize");
        state.set_desktop_update_starter(Arc::new(|_| anyhow::bail!("starter failed")));

        assert!(matches!(
            state.start_desktop_update("9.9.9".to_string()),
            Err(DesktopUpdateStartError::Starter(_))
        ));
        let status = state.desktop_update_status();
        assert_eq!(status.phase, DesktopUpdatePhase::Failed);
        assert_eq!(status.error.as_deref(), Some("starter failed"));

        drop(state);
        fs::remove_dir_all(dir).expect("test data directory should be removed");
    }

    #[test]
    fn legacy_config_gets_persisted_desktop_defaults() {
        let dir = temp_data_dir("desktop-config-migration");
        let db = Database::open(dir.clone()).expect("test database should open");
        let mut legacy = serde_json::to_value(AppConfig {
            gateway_key: "test-gateway-key".to_string(),
            ..AppConfig::default()
        })
        .expect("test config should serialize");
        {
            let legacy_object = legacy
                .as_object_mut()
                .expect("test config should be an object");
            legacy_object.remove("claude_desktop_models");
            legacy_object.remove("show_dock_icon");
        }
        db.set_setting("config", &legacy.to_string())
            .expect("legacy config should persist");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("state-test"));
        let state = CoreStateInner::new(db, dir.clone(), cipher).expect("state should initialize");

        assert_eq!(
            state.config().claude_desktop_models.resolved(),
            AppConfig::default().claude_desktop_models.resolved()
        );
        assert!(state.config().show_dock_icon);
        let stored = state
            .db
            .lock()
            .get_setting("config")
            .expect("stored config should be readable")
            .expect("stored config should exist");
        assert!(stored.contains("claude_desktop_models"));
        assert!(stored.contains("show_dock_icon"));

        drop(state);
        fs::remove_dir_all(dir).expect("test data directory should be removed");
    }
}
