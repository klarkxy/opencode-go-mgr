//! Command Code account runtime and public Provider catalog refresh.
//!
//! Model supply is governed by the Provider model/protocol contract. Accounts
//! contribute credentials and ordering only; the public `/models` response is
//! not treated as proof that a stored Key is valid.

use crate::http_client;
use crate::models::AppConfig;
use crate::provider::{
    COMMAND_CODE_GOAT_BASE_URL, COMMAND_CODE_GOAT_MODELS_PATH, COMMAND_CODE_PROVIDER_ID,
    ConnectionVerificationStatus, GOAT_OFFERING_ID, is_command_code_goat,
    parse_provider_models_catalog,
};
use std::collections::HashMap;
use std::fmt;
#[cfg(debug_assertions)]
use std::sync::{LazyLock, RwLock};
use std::time::Duration;

/// Snapshot used to reject stale GOAT verification commits after network I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoatVerificationContract {
    pub account_id: String,
    pub account_updated_at: String,
    pub key_cipher: String,
}

/// Data-only GOAT routing state loaded from persistence for one account.
#[derive(Debug, Clone)]
pub struct GoatAccountRuntime {
    pub account_id: String,
    pub enabled: bool,
    pub verification_status: ConnectionVerificationStatus,
    pub setup_ready: bool,
    pub has_key: bool,
}

pub const MAX_GOAT_VERIFICATION_BODY_BYTES: usize = 256 * 1024;
pub const GOAT_VERIFICATION_CONFLICT_MESSAGE: &str =
    "the Command Code GOAT account changed while it was being verified; retry verification";

#[cfg(debug_assertions)]
static GOAT_VERIFY_ORIGINS: LazyLock<RwLock<HashMap<u64, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// RAII guard for the debug-only GOAT verification origin substitute.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub struct GoatVerifyOriginGuard {
    process_generation: u64,
    origin: String,
}

#[cfg(debug_assertions)]
impl Drop for GoatVerifyOriginGuard {
    fn drop(&mut self) {
        if let Ok(mut origins) = GOAT_VERIFY_ORIGINS.write()
            && origins
                .get(&self.process_generation)
                .is_some_and(|origin| origin == &self.origin)
        {
            origins.remove(&self.process_generation);
        }
    }
}

/// Installs a loopback-only origin used by GOAT GET `/models` tests.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn install_goat_verify_origin_for_test(
    process_generation: u64,
    origin: impl Into<String>,
) -> Result<GoatVerifyOriginGuard, String> {
    let origin = origin.into();
    ensure_loopback_origin(&origin)?;
    let origin = origin.trim_end_matches('/').to_string();
    let guard = GoatVerifyOriginGuard {
        process_generation,
        origin: origin.clone(),
    };
    GOAT_VERIFY_ORIGINS
        .write()
        .map_err(|_| "GOAT verify origin lock is poisoned".to_string())?
        .insert(process_generation, origin);
    Ok(guard)
}

#[cfg(debug_assertions)]
pub fn goat_verify_base_url(process_generation: Option<u64>) -> String {
    if let Some(generation) = process_generation
        && let Ok(origins) = GOAT_VERIFY_ORIGINS.read()
        && let Some(origin) = origins.get(&generation)
    {
        return format!("{}/provider/v1", origin.trim_end_matches('/'));
    }
    COMMAND_CODE_GOAT_BASE_URL.to_string()
}

#[cfg(debug_assertions)]
fn ensure_loopback_origin(origin: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(origin).map_err(|error| error.to_string())?;
    if url.scheme() != "http"
        || !matches!(
            url.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
        )
    {
        return Err("GOAT verify test origin must be an HTTP loopback URL".to_string());
    }
    Ok(())
}

impl GoatAccountRuntime {
    pub fn eligible(&self) -> bool {
        self.enabled
            && self.setup_ready
            && self.has_key
            && is_command_code_goat(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID)
    }

    pub fn serves(&self, _requested: &str) -> bool {
        self.eligible()
    }
}

pub fn goat_runtimes_by_account(
    runtimes: &[GoatAccountRuntime],
) -> HashMap<String, GoatAccountRuntime> {
    runtimes
        .iter()
        .cloned()
        .map(|runtime| (runtime.account_id.clone(), runtime))
        .collect()
}

#[derive(Debug, Clone)]
pub struct GoatVerifyFailure {
    pub message: String,
}

impl fmt::Display for GoatVerifyFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GoatVerifyFailure {}

pub fn official_goat_models_url() -> String {
    format!(
        "{}{}",
        COMMAND_CODE_GOAT_BASE_URL.trim_end_matches('/'),
        COMMAND_CODE_GOAT_MODELS_PATH
    )
}

pub fn goat_models_url_for_base(base: &str) -> String {
    format!(
        "{}{}",
        base.trim_end_matches('/'),
        COMMAND_CODE_GOAT_MODELS_PATH
    )
}

pub fn opencode_go_models_url_for_base(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    }
}

pub fn ollama_cloud_models_url_for_base(base: &str) -> String {
    format!(
        "{}{}",
        base.trim_end_matches('/'),
        crate::kernel::ids::OLLAMA_CLOUD_MODELS_PATH
    )
}

/// Public, keyless Ollama Cloud GET `/models` refresh. Auth-free by design:
/// the endpoint is the catalog discovery surface, never a Key check.
pub async fn refresh_ollama_cloud_models(
    config: &AppConfig,
    base_url: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    let url = ollama_cloud_models_url_for_base(base_url);
    probe_public_provider_models_at_url(config, &url, "Ollama Cloud").await
}

/// Debug-only loopback origin substitute for Ollama Cloud GET `/models`
/// tests. Mirrors the GOAT verify seam but never appends a provider path.
#[cfg(debug_assertions)]
static OLLAMA_MODELS_ORIGINS: LazyLock<RwLock<HashMap<u64, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[cfg(debug_assertions)]
#[doc(hidden)]
pub struct OllamaModelsOriginGuard {
    process_generation: u64,
    origin: String,
}

#[cfg(debug_assertions)]
impl Drop for OllamaModelsOriginGuard {
    fn drop(&mut self) {
        if let Ok(mut origins) = OLLAMA_MODELS_ORIGINS.write()
            && origins
                .get(&self.process_generation)
                .is_some_and(|origin| origin == &self.origin)
        {
            origins.remove(&self.process_generation);
        }
    }
}

#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn install_ollama_models_origin_for_test(
    process_generation: u64,
    origin: impl Into<String>,
) -> Result<OllamaModelsOriginGuard, String> {
    let origin = origin.into();
    ensure_loopback_origin(&origin)?;
    let origin = origin.trim_end_matches('/').to_string();
    let guard = OllamaModelsOriginGuard {
        process_generation,
        origin: origin.clone(),
    };
    OLLAMA_MODELS_ORIGINS
        .write()
        .map_err(|_| "Ollama models origin lock is poisoned".to_string())?
        .insert(process_generation, origin);
    Ok(guard)
}

#[cfg(debug_assertions)]
pub fn ollama_cloud_models_base_url(process_generation: Option<u64>) -> String {
    if let Some(generation) = process_generation
        && let Ok(origins) = OLLAMA_MODELS_ORIGINS.read()
        && let Some(origin) = origins.get(&generation)
    {
        return origin.trim_end_matches('/').to_string();
    }
    crate::kernel::ids::OLLAMA_CLOUD_BASE_URL.to_string()
}

pub async fn probe_goat_models(
    config: &AppConfig,
    _api_key: &str,
    base_url: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    let url = goat_models_url_for_base(base_url);
    probe_public_provider_models_at_url(config, &url, "Command Code").await
}

pub async fn refresh_command_code_models(
    config: &AppConfig,
    base_url: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    let url = goat_models_url_for_base(base_url);
    probe_public_provider_models_at_url(config, &url, "Command Code").await
}

async fn probe_public_provider_models_at_url(
    config: &AppConfig,
    url: &str,
    provider_label: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    let client = http_client::configured_builder(config)
        .and_then(|builder| {
            builder
                .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
                .redirect(http_client::no_redirect_policy())
                .build()
                .map_err(Into::into)
        })
        .map_err(|error| GoatVerifyFailure {
            message: format!("failed to build {provider_label} model refresh client: {error}"),
        })?;
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(Duration::from_secs(config.non_stream_timeout_secs))
        .send()
        .await
        .map_err(|error| GoatVerifyFailure {
            message: format!("{provider_label} GET /models failed: {error}"),
        })?;
    parse_provider_models_response(response, provider_label).await
}

async fn parse_provider_models_response(
    response: reqwest::Response,
    provider_label: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    let status = response.status();
    let bytes = read_limited_body(response, provider_label).await?;
    if !status.is_success() {
        return Err(GoatVerifyFailure {
            message: format!("{provider_label} GET /models returned {}", status.as_u16()),
        });
    }
    parse_provider_models_catalog(&bytes, provider_label)
        .map_err(|message| GoatVerifyFailure { message })
}

pub async fn probe_opencode_go_models(
    config: &AppConfig,
    api_key: &str,
    base_url: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    let url = opencode_go_models_url_for_base(base_url);
    probe_provider_models_at_url(config, api_key, &url, "OpenCode Go").await
}

pub async fn probe_provider_models(
    config: &AppConfig,
    api_key: &str,
    base_url: &str,
    provider_label: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    let url = goat_models_url_for_base(base_url);
    probe_provider_models_at_url(config, api_key, &url, provider_label).await
}

async fn probe_provider_models_at_url(
    config: &AppConfig,
    api_key: &str,
    url: &str,
    provider_label: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(GoatVerifyFailure {
            message: format!("{provider_label} model refresh requires a stored Key"),
        });
    }
    let client = http_client::configured_builder(config)
        .and_then(|builder| {
            builder
                .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
                .redirect(http_client::no_redirect_policy())
                .build()
                .map_err(Into::into)
        })
        .map_err(|error| GoatVerifyFailure {
            message: format!("failed to build {provider_label} model client: {error}"),
        })?;
    let response = client
        .get(url)
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(Duration::from_secs(config.non_stream_timeout_secs))
        .send()
        .await
        .map_err(|error| GoatVerifyFailure {
            message: format!("{provider_label} GET /models failed: {error}"),
        })?;
    parse_provider_models_response(response, provider_label).await
}

async fn read_limited_body(
    response: reqwest::Response,
    provider_label: &str,
) -> Result<Vec<u8>, GoatVerifyFailure> {
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| GoatVerifyFailure {
            message: format!("{provider_label} GET /models body failed: {error}"),
        })?;
        if bytes.len() + chunk.len() > MAX_GOAT_VERIFICATION_BODY_BYTES {
            return Err(GoatVerifyFailure {
                message: format!(
                    "{provider_label} GET /models exceeded the {MAX_GOAT_VERIFICATION_BODY_BYTES}-byte limit"
                ),
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(
        enabled: bool,
        verification_status: ConnectionVerificationStatus,
    ) -> GoatAccountRuntime {
        GoatAccountRuntime {
            account_id: "goat-1".into(),
            enabled,
            verification_status,
            setup_ready: true,
            has_key: true,
        }
    }

    #[test]
    fn account_eligibility_does_not_reinterpret_the_provider_model_preset() {
        let pending = runtime(true, ConnectionVerificationStatus::Pending);
        assert!(pending.serves("any-model-in-the-provider-contract"));
        assert!(!runtime(false, ConnectionVerificationStatus::Verified).eligible());
    }

    #[test]
    fn opencode_go_models_url_keeps_the_official_v1_segment() {
        assert_eq!(
            opencode_go_models_url_for_base("https://opencode.ai/zen/go"),
            "https://opencode.ai/zen/go/v1/models"
        );
        assert_eq!(
            opencode_go_models_url_for_base("http://127.0.0.1:9/provider/v1/"),
            "http://127.0.0.1:9/provider/v1/models"
        );
    }
}
