use anyhow::Result;
use chrono::Utc;
use clap::{Parser, Subcommand};
use ocg_core::crypto::{KeyCipher, StaticKeyCipher, load_or_create_static_cipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::models::{Account, AppConfig};
use ocg_core::state::CoreStateInner;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "ocg-manager-cli")]
#[command(about = "Headless CLI for OCG Manager gateway")]
#[command(version)]
struct Cli {
    /// Data directory for the CLI (default: ~/.ocg-mgr-cli)
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    /// Encryption key for API key storage.
    /// If omitted, uses OCG_MANAGER_ENCRYPTION_KEY env var or generates one in <data-dir>/.encryption-key.
    #[arg(long, global = true)]
    encryption_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server
    Serve {
        /// Address to listen on
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        /// Gateway port (overrides config)
        #[arg(short, long)]
        port: Option<u16>,
        /// Directory containing the built web dashboard (dist)
        #[arg(long)]
        dashboard_dir: Option<PathBuf>,
    },
    /// Manage API keys
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Show gateway status
    Status,
}

#[derive(Subcommand)]
enum KeyAction {
    /// List all keys and their status
    List,
    /// Add a new key
    Add {
        /// Display name for the key
        name: String,
        /// The OpenCode-Go API key
        key: String,
        /// OpenCode-Go login account
        #[arg(long)]
        username: Option<String>,
        /// OpenCode-Go login password
        #[arg(long)]
        password: Option<String>,
    },
    /// Remove a key
    Remove {
        /// Account ID
        id: String,
    },
    /// Enable a key
    Enable {
        /// Account ID
        id: String,
    },
    /// Disable a key
    Disable {
        /// Account ID
        id: String,
    },
    /// Ping upstream with one or all enabled keys — shows real status code / body
    Ping {
        /// Account ID; omit to ping every enabled key
        id: Option<String>,
        /// Model to send (default: deepseek-v4-flash)
        #[arg(long, default_value = "deepseek-v4-flash")]
        model: String,
        /// User message (default: "ping")
        #[arg(long, default_value = "ping")]
        message: String,
        /// max_tokens for the ping (default: 3)
        #[arg(long, default_value_t = 3)]
        max_tokens: u32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let data_dir = resolve_data_dir(cli.data_dir);
    let cipher = resolve_cipher(&data_dir, cli.encryption_key)?;

    match cli.command {
        Commands::Serve {
            host,
            port,
            dashboard_dir,
        } => serve(data_dir, cipher, host, port, dashboard_dir).await,
        Commands::Key { action } => key_command(data_dir, cipher, action).await,
        Commands::Status => status_command(data_dir, cipher).await,
    }
}

fn resolve_data_dir(data_dir: Option<PathBuf>) -> PathBuf {
    data_dir.unwrap_or_else(|| {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        home.join(".ocg-mgr-cli")
    })
}

fn resolve_cipher(
    data_dir: &Path,
    encryption_key: Option<String>,
) -> Result<Arc<dyn KeyCipher + Send + Sync>> {
    let cipher = match encryption_key {
        Some(secret) => StaticKeyCipher::new(&secret),
        None => match std::env::var("OCG_MANAGER_ENCRYPTION_KEY") {
            Ok(secret) => StaticKeyCipher::new(&secret),
            Err(_) => load_or_create_static_cipher(data_dir)?,
        },
    };
    Ok(Arc::new(cipher))
}

fn build_state(
    data_dir: PathBuf,
    cipher: Arc<dyn KeyCipher + Send + Sync>,
) -> Result<Arc<CoreStateInner>> {
    let db = Database::open(data_dir.clone())?;
    Ok(Arc::new(CoreStateInner::new(db, data_dir, cipher)?))
}

async fn serve(
    data_dir: PathBuf,
    cipher: Arc<dyn KeyCipher + Send + Sync>,
    host: IpAddr,
    port: Option<u16>,
    dashboard_dir: Option<PathBuf>,
) -> Result<()> {
    let state = build_state(data_dir, cipher)?;
    let executable = if dashboard_dir.is_none() {
        std::env::current_exe().ok()
    } else {
        None
    };
    state.set_dashboard_dir(resolve_dashboard_dir(dashboard_dir, executable.as_deref()));

    let mut config = state.config();
    if let Some(port) = port {
        config.gateway_port = port;
        state.set_config(config.clone())?;
    }

    let handle =
        gateway::start_gateway_on(state.clone(), SocketAddr::new(host, config.gateway_port))
            .await?;
    println!("gateway started on http://{}:{}", host, handle.port);
    println!("gateway key: {}", config.gateway_key);
    println!("dashboard: http://{}:{}/dashboard/", host, handle.port);
    println!("upstream: {}", config.upstream_base_url);

    println!("press Ctrl+C to stop");

    // Hold the gateway handle so it stays alive
    {
        let mut gateway_lock = state.gateway.lock();
        *gateway_lock = Some(handle);
    }

    let _ = state.db.lock().log_gateway(
        "info",
        "gateway",
        &format!("cli gateway started on port {}", config.gateway_port),
    );

    tokio::signal::ctrl_c().await?;
    println!("shutting down...");

    if let Some(handle) = state.gateway.lock().take() {
        gateway::stop_gateway(handle);
    }
    let _ = state
        .db
        .lock()
        .log_gateway("info", "gateway", "cli gateway stopped");
    Ok(())
}

fn resolve_dashboard_dir(explicit: Option<PathBuf>, executable: Option<&Path>) -> Option<PathBuf> {
    explicit.or_else(|| {
        let dist = executable?.parent()?.join("dist");
        dist.is_dir().then_some(dist)
    })
}

async fn key_command(
    data_dir: PathBuf,
    cipher: Arc<dyn KeyCipher + Send + Sync>,
    action: KeyAction,
) -> Result<()> {
    let state = build_state(data_dir, cipher)?;
    let db = state.db.lock();

    match action {
        KeyAction::List => {
            let accounts = db.list_accounts()?;
            if accounts.is_empty() {
                println!("no keys configured");
                return Ok(());
            }
            println!("{:<36} {:<20} {:<8}", "id", "name", "enabled");
            for account in accounts {
                println!(
                    "{:<36} {:<20} {:<8}",
                    account.id,
                    account.name,
                    if account.enabled { "yes" } else { "no" },
                );
            }
        }
        KeyAction::Add {
            name,
            key,
            username,
            password,
        } => {
            let id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now();
            let key_cipher = state.encrypt_key(&key)?;
            let password_cipher = match password {
                Some(p) if !p.trim().is_empty() => Some(state.encrypt_key(p.trim())?),
                _ => None,
            };
            let account = Account {
                id: id.clone(),
                name,
                username: username.and_then(|s| {
                    let trimmed = s.trim().to_string();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                }),
                password_cipher,
                key_cipher,
                enabled: true,
                referral_code: None,
                purchase_date: String::new(),
                expires_on: String::new(),
                cooldown_until: None,
                cooldown_generic_until: None,
                cooldown_5h_until: None,
                cooldown_week_until: None,
                cooldown_month_until: None,
                last_error: None,
                created_at: now,
                updated_at: now,
            };
            db.create_account(&account)?;
            let account = db
                .get_account(&id)?
                .ok_or_else(|| anyhow::anyhow!("created key not found: {}", id))?;
            db.log_gateway(
                "info",
                "account",
                &format!("cli added account {}", account.name),
            )?;
            println!("added key {} ({})", id, account.name);
        }
        KeyAction::Remove { id } => {
            // ponytail: drop the outer guard from line 197 before re-locking —
            // parking_lot::Mutex is not re-entrant, so the second lock() would deadlock.
            drop(db);
            let mut db = state.db.lock();
            if let Some(account) = db.get_account(&id)? {
                db.delete_account(&id)?;
                db.log_gateway(
                    "info",
                    "account",
                    &format!("cli removed account {}", account.name),
                )?;
                println!("removed key {} ({})", id, account.name);
            } else {
                anyhow::bail!("key not found: {}", id);
            }
        }
        KeyAction::Enable { id } => {
            drop(db);
            toggle_account(&state, &id, true)?;
        }
        KeyAction::Disable { id } => {
            drop(db);
            toggle_account(&state, &id, false)?;
        }
        KeyAction::Ping {
            id,
            model,
            message,
            max_tokens,
        } => {
            drop(db);
            ping_keys(&state, id.as_deref(), &model, &message, max_tokens).await?;
        }
    }
    Ok(())
}

fn toggle_account(state: &Arc<CoreStateInner>, id: &str, enabled: bool) -> Result<()> {
    let db = state.db.lock();
    let account = db
        .get_account(id)?
        .ok_or_else(|| anyhow::anyhow!("key not found: {}", id))?;
    let update = ocg_core::models::AccountUpdate {
        name: None,
        username: None,
        password: None,
        key: None,
        enabled: Some(enabled),
        referral_code: None,
        purchase_date: None,
    };
    db.update_account(id, &update, None, None)?;
    db.log_gateway(
        "info",
        "account",
        &format!(
            "cli {} account {}",
            if enabled { "enabled" } else { "disabled" },
            account.name
        ),
    )?;
    println!(
        "{} key {} ({})",
        if enabled { "enabled" } else { "disabled" },
        id,
        account.name
    );
    Ok(())
}

async fn status_command(data_dir: PathBuf, cipher: Arc<dyn KeyCipher + Send + Sync>) -> Result<()> {
    let state = build_state(data_dir, cipher)?;
    let config: AppConfig = state.config();
    let db = state.db.lock();
    let accounts = db.list_accounts()?;
    let enabled = accounts.iter().filter(|a| a.enabled).count();

    println!("data dir: {:?}", state.data_dir());
    println!("gateway port: {}", config.gateway_port);
    println!("gateway key: {}", config.gateway_key);
    println!("upstream: {}", config.upstream_base_url);
    println!("accounts: {} total, {} enabled", accounts.len(), enabled);
    Ok(())
}

/// One-shot ping: decrypts the key, sends a tiny chat completion, prints real upstream status.
/// Used to surface real 401/403/429/200 — what each key actually does upstream, no inference.
async fn ping_one(
    state: &Arc<CoreStateInner>,
    account: &Account,
    model: &str,
    message: &str,
    max_tokens: u32,
) -> (u16, String) {
    let key = match state.decrypt_key(&account.key_cipher) {
        Ok(k) => k,
        Err(e) => return (0, format!("decrypt failed: {}", e)),
    };
    let (config, client) = state.upstream_context();
    let url = format!(
        "{}/v1/chat/completions",
        config.upstream_base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": message}],
        "max_tokens": max_tokens,
        "stream": false,
    });
    let started = std::time::Instant::now();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(
            config.non_stream_timeout_secs,
        ))
        .send()
        .await;
    let elapsed = started.elapsed();
    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            match r.text().await {
                Ok(text) => {
                    let trimmed = text.chars().take(200).collect::<String>();
                    (status, format!("{}ms {}", elapsed.as_millis(), trimmed))
                }
                Err(error) => {
                    let error = if error.is_timeout() {
                        "response body timed out".to_string()
                    } else {
                        format!("response body failed: {error}")
                    };
                    (
                        0,
                        format!("{}ms {} after HTTP {}", elapsed.as_millis(), error, status),
                    )
                }
            }
        }
        Err(e) => (
            0,
            format!("{}ms request failed: {}", elapsed.as_millis(), e),
        ),
    }
}

async fn ping_keys(
    state: &Arc<CoreStateInner>,
    id: Option<&str>,
    model: &str,
    message: &str,
    max_tokens: u32,
) -> Result<()> {
    let targets: Vec<Account> = {
        let db = state.db.lock();
        match id {
            Some(i) => match db.get_account(i)? {
                Some(a) => vec![a],
                None => anyhow::bail!("key not found: {}", i),
            },
            None => db
                .list_accounts()?
                .into_iter()
                .filter(|a| a.enabled)
                .collect(),
        }
    };
    if targets.is_empty() {
        println!("no enabled keys to ping");
        return Ok(());
    }
    println!(
        "pinging {} key(s) with model={} message={:?}",
        targets.len(),
        model,
        message
    );
    for account in targets {
        let (status, body) = ping_one(state, &account, model, message, max_tokens).await;
        let verdict = if status == 200 { "OK" } else { "FAIL" };
        println!(
            "[{}] {} ({}) status={} {}",
            verdict, account.id, account.name, status, body
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands, resolve_dashboard_dir};
    use clap::{CommandFactory, Parser};

    #[test]
    fn exposes_package_version() {
        assert_eq!(
            Cli::command().get_version(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn serve_accepts_container_bind_address() {
        let cli = Cli::try_parse_from(["ocg-manager-cli", "serve", "--host", "0.0.0.0"]).unwrap();
        let Commands::Serve { host, .. } = cli.command else {
            panic!("expected serve command");
        };
        assert!(host.is_unspecified());
    }

    #[test]
    fn dashboard_dir_prefers_explicit_then_existing_packaged_dist() {
        let root = std::env::temp_dir().join(format!("ocg-cli-dashboard-{}", uuid::Uuid::new_v4()));
        let dist = root.join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        let executable = root.join("ocg-manager-cli");
        let explicit = root.join("custom");

        assert_eq!(
            resolve_dashboard_dir(Some(explicit.clone()), Some(&executable)),
            Some(explicit)
        );
        assert_eq!(
            resolve_dashboard_dir(None, Some(&executable)),
            Some(dist.clone())
        );
        std::fs::remove_dir_all(&dist).unwrap();
        assert_eq!(resolve_dashboard_dir(None, Some(&executable)), None);

        std::fs::remove_dir_all(root).unwrap();
    }
}
