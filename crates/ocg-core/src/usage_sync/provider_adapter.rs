//! Provider usage capability and experimental GOAT adapter boundary.
//!
//! OpenCode Go is the only verified authoritative contract today. Command
//! Code GOAT deliberately has no production endpoint constant or successful
//! parser: official-source research did not establish either. The local URL
//! seam below exists only to prove that unknown `/alpha/*` responses fail soft
//! without becoming account/inference eligibility state.

use crate::models::AppConfig;
use crate::provider::{ProviderRegistry, UsageContractKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[cfg(test)]
use futures_util::StreamExt;
#[cfg(test)]
use reqwest::StatusCode;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
const MAX_EXPERIMENTAL_BODY_BYTES: usize = 64 * 1024;
#[cfg(test)]
const EXPERIMENTAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderUsageEvidence {
    Authoritative,
    Experimental,
    Unavailable,
}

/// Stable API-facing capability description. An absent endpoint is meaningful:
/// no caller may infer or synthesize one from the provider base URL.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderUsageCapability {
    pub provider_id: &'static str,
    pub offering_id: &'static str,
    pub evidence: ProviderUsageEvidence,
    pub experimental: bool,
    pub endpoint: Option<&'static str>,
    pub automatic_sync: bool,
    pub authoritative_for_quota: bool,
    pub affects_inference_eligibility: bool,
}

pub fn provider_usage_capability(
    provider_id: &str,
    offering_id: &str,
) -> Option<ProviderUsageCapability> {
    let descriptor = ProviderRegistry::get(provider_id, offering_id)?;
    if !descriptor.usage.publishes_capability {
        return None;
    }
    Some(ProviderUsageCapability {
        provider_id: descriptor.provider_id,
        offering_id: descriptor.offering_id,
        evidence: match descriptor.usage.contract {
            UsageContractKind::Authoritative => ProviderUsageEvidence::Authoritative,
            UsageContractKind::LocalState
            | UsageContractKind::ExperimentalUnavailable
            | UsageContractKind::Unavailable => ProviderUsageEvidence::Unavailable,
        },
        experimental: descriptor.usage.experimental,
        endpoint: descriptor.usage.endpoint,
        automatic_sync: descriptor.usage.automatic_sync,
        authoritative_for_quota: descriptor.usage.authoritative_for_quota,
        affects_inference_eligibility: descriptor.usage.affects_inference_eligibility,
    })
}

pub fn supports_authoritative_auto_sync(provider_id: &str, offering_id: &str) -> bool {
    provider_usage_capability(provider_id, offering_id)
        .is_some_and(|capability| capability.automatic_sync && capability.authoritative_for_quota)
}

/// Normalized key-scoped window value reserved for a future verified GOAT
/// response contract. No model id is present: every window belongs to the
/// account/key quota pool, never to a model pool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderQuotaWindowValue {
    pub window_kind: String,
    pub used: Option<f64>,
    pub limit_value: Option<f64>,
    pub resets_at: Option<DateTime<Utc>>,
    pub unit: Option<String>,
}

/// Optional provider-reported balance. Purchased/free remain separate when an
/// official contract exposes them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCreditBalanceValue {
    pub balance_kind: String,
    pub amount: Option<f64>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoatUsageSnapshot {
    pub evidence: ProviderUsageEvidence,
    pub windows: Vec<ProviderQuotaWindowValue>,
    pub balances: Vec<ProviderCreditBalanceValue>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoatUsageError {
    ContractUnavailable,
    Unauthorized,
    Forbidden,
    RateLimited,
    Http(u16),
    Timeout,
    Network,
    Oversize,
    Schema,
}

impl fmt::Display for GoatUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContractUnavailable => f.write_str(
                "experimental GOAT usage is unavailable because no verified official contract is configured",
            ),
            Self::Unauthorized => f.write_str("experimental GOAT usage returned HTTP 401"),
            Self::Forbidden => f.write_str("experimental GOAT usage returned HTTP 403"),
            Self::RateLimited => f.write_str("experimental GOAT usage returned HTTP 429"),
            Self::Http(status) => write!(f, "experimental GOAT usage returned HTTP {status}"),
            Self::Timeout => f.write_str("experimental GOAT usage request timed out"),
            Self::Network => f.write_str("experimental GOAT usage request failed"),
            Self::Oversize => f.write_str("experimental GOAT usage response exceeds 64 KiB"),
            Self::Schema => f.write_str("experimental GOAT usage response is not valid JSON"),
        }
    }
}

impl std::error::Error for GoatUsageError {}

/// Production remains fail-closed for the missing endpoint and fail-soft for
/// the rest of the product: this function performs no request and mutates no
/// inference/account state.
pub async fn fetch_goat_usage(
    _config: &AppConfig,
    _api_key: &str,
) -> Result<GoatUsageSnapshot, GoatUsageError> {
    Err(GoatUsageError::ContractUnavailable)
}

/// Local-only HTTP seam for captured-response tests. A syntactically valid
/// body is still rejected as `ContractUnavailable` until its official schema
/// is verified; it is never persisted as quota or balance data.
#[cfg(test)]
async fn fetch_goat_usage_from(
    config: &AppConfig,
    api_key: &str,
    endpoint: &str,
) -> Result<GoatUsageSnapshot, GoatUsageError> {
    let client = crate::http_client::configured_builder(config)
        .map_err(|_| GoatUsageError::Network)?
        .redirect(reqwest::redirect::Policy::none())
        .timeout(EXPERIMENTAL_REQUEST_TIMEOUT)
        .build()
        .map_err(|_| GoatUsageError::Network)?;
    let response = client
        .get(endpoint)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(map_reqwest_error)?;
    match response.status() {
        StatusCode::OK => {
            let body = read_body_limited(response).await?;
            parse_unverified_goat_body(&body)
        }
        StatusCode::UNAUTHORIZED => Err(GoatUsageError::Unauthorized),
        StatusCode::FORBIDDEN => Err(GoatUsageError::Forbidden),
        StatusCode::TOO_MANY_REQUESTS => Err(GoatUsageError::RateLimited),
        status => Err(GoatUsageError::Http(status.as_u16())),
    }
}

#[cfg(test)]
async fn read_body_limited(response: reqwest::Response) -> Result<Vec<u8>, GoatUsageError> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_reqwest_error)?;
        if body.len().saturating_add(chunk.len()) > MAX_EXPERIMENTAL_BODY_BYTES {
            return Err(GoatUsageError::Oversize);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
fn map_reqwest_error(error: reqwest::Error) -> GoatUsageError {
    if error.is_timeout() {
        GoatUsageError::Timeout
    } else {
        GoatUsageError::Network
    }
}

#[cfg(test)]
fn parse_unverified_goat_body(body: &[u8]) -> Result<GoatUsageSnapshot, GoatUsageError> {
    let _: Value = serde_json::from_slice(body).map_err(|_| GoatUsageError::Schema)?;
    Err(GoatUsageError::ContractUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const TEST_BEARER: &str = "goat-test-key-must-not-leak";

    #[test]
    fn capabilities_keep_goat_and_zen_out_of_authoritative_go_sync() {
        use crate::provider::{
            ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_PROVIDER_ID, CUSTOM_API_OFFERING_ID,
            CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID, OPENCODE_PROVIDER_ID,
            OPENCODE_ZEN_FREE_PROVIDER_ID, ProviderAdapterKind,
        };

        let go = provider_usage_capability(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert_eq!(go.evidence, ProviderUsageEvidence::Authoritative);
        assert!(go.automatic_sync);
        assert!(go.authoritative_for_quota);

        let goat = provider_usage_capability(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert_eq!(goat.evidence, ProviderUsageEvidence::Unavailable);
        assert!(!goat.automatic_sync);
        assert!(!goat.authoritative_for_quota);

        assert!(
            provider_usage_capability(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID)
                .is_none()
        );

        assert!(provider_usage_capability(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).is_none());
        for descriptor in crate::provider::ProviderRegistry::iter() {
            let capability =
                provider_usage_capability(descriptor.provider_id, descriptor.offering_id);
            match descriptor.kind {
                ProviderAdapterKind::OpenCodeGo => {
                    let capability = capability.expect("Go publishes authoritative usage");
                    assert_eq!(capability.evidence, ProviderUsageEvidence::Authoritative);
                    assert!(capability.automatic_sync);
                    assert!(capability.authoritative_for_quota);
                }
                ProviderAdapterKind::CommandCodeGoat => {
                    let capability = capability.expect("GOAT publishes local-state usage");
                    assert_eq!(capability.evidence, ProviderUsageEvidence::Unavailable);
                    assert!(!capability.automatic_sync);
                    assert!(!capability.authoritative_for_quota);
                }
                ProviderAdapterKind::MiniMaxCn | ProviderAdapterKind::KimiCn => {
                    let capability = capability.expect("sealed CN Plan publishes usage");
                    assert_eq!(capability.evidence, ProviderUsageEvidence::Authoritative);
                    assert!(!capability.automatic_sync);
                }
                ProviderAdapterKind::ZenFree => {
                    assert!(capability.is_none());
                }
                ProviderAdapterKind::Cpa => {
                    assert!(capability.is_none());
                }
                ProviderAdapterKind::ConfigurableHttp => {
                    assert!(capability.is_none());
                }
            }
        }
    }

    #[test]
    fn usage_capability_delegates_through_provider_descriptor() {
        use crate::provider::{
            ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_PROVIDER_ID, CUSTOM_API_OFFERING_ID,
            CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID, OPENCODE_PROVIDER_ID,
            OPENCODE_ZEN_FREE_PROVIDER_ID, ProviderRegistry,
        };

        let go_usage = ProviderRegistry::get(OPENCODE_PROVIDER_ID, GO_OFFERING_ID)
            .unwrap()
            .usage;
        let go = provider_usage_capability(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert_eq!(go.endpoint, go_usage.endpoint);
        assert_eq!(go.automatic_sync, go_usage.automatic_sync);
        assert_eq!(go.authoritative_for_quota, go_usage.authoritative_for_quota);
        assert!(go_usage.publishes_capability);

        let goat_usage = ProviderRegistry::get(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID)
            .unwrap()
            .usage;
        assert!(!goat_usage.experimental);
        assert!(goat_usage.publishes_capability);
        assert!(!goat_usage.automatic_sync);
        let goat = provider_usage_capability(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert_eq!(goat.evidence, ProviderUsageEvidence::Unavailable);
        assert!(!goat.automatic_sync);

        let zen_usage =
            ProviderRegistry::get(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID)
                .unwrap()
                .usage;
        assert!(!zen_usage.publishes_capability);
        assert!(!zen_usage.authoritative_for_quota);
        assert!(
            provider_usage_capability(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID)
                .is_none()
        );

        assert!(
            !ProviderRegistry::get(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID)
                .unwrap()
                .usage
                .publishes_capability
        );
    }

    #[tokio::test]
    async fn production_goat_fetch_has_no_guessed_endpoint() {
        assert_eq!(
            fetch_goat_usage(&AppConfig::default(), TEST_BEARER).await,
            Err(GoatUsageError::ContractUnavailable)
        );
    }

    #[tokio::test]
    async fn unknown_success_response_fails_soft_without_guessing_schema() {
        let (url, server) = serve_once(200, "OK", br#"{"fiveHour":{"used":3}}"#).await;
        let error = fetch_goat_usage_from(&AppConfig::default(), TEST_BEARER, &url)
            .await
            .unwrap_err();
        let request = server.await.unwrap();
        assert_eq!(
            request.authorization.as_deref(),
            Some(format!("Bearer {TEST_BEARER}").as_str())
        );
        assert_eq!(error, GoatUsageError::ContractUnavailable);
        assert!(!error.to_string().contains(TEST_BEARER));
    }

    #[tokio::test]
    async fn malformed_and_http_errors_are_sanitized_and_fail_soft() {
        let (url, server) = serve_once(200, "OK", b"not-json").await;
        let error = fetch_goat_usage_from(&AppConfig::default(), TEST_BEARER, &url)
            .await
            .unwrap_err();
        let _ = server.await;
        assert_eq!(error, GoatUsageError::Schema);
        assert!(!format!("{error:?}").contains(TEST_BEARER));

        for (status, reason, expected) in [
            (401, "Unauthorized", GoatUsageError::Unauthorized),
            (403, "Forbidden", GoatUsageError::Forbidden),
            (429, "Too Many Requests", GoatUsageError::RateLimited),
            (503, "Unavailable", GoatUsageError::Http(503)),
        ] {
            let (url, server) = serve_once(status, reason, br#"{"error":"redacted"}"#).await;
            let error = fetch_goat_usage_from(&AppConfig::default(), TEST_BEARER, &url)
                .await
                .unwrap_err();
            let _ = server.await;
            assert_eq!(error, expected);
            assert!(!error.to_string().contains(TEST_BEARER));
        }
    }

    struct ServedRequest {
        authorization: Option<String>,
    }

    async fn serve_once(
        status: u16,
        reason: &str,
        body: &[u8],
    ) -> (String, tokio::task::JoinHandle<ServedRequest>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let reason = reason.to_string();
        let body = body.to_vec();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let head = read_http_head(&mut stream).await;
            let authorization = authorization_header(&head);
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
            ServedRequest { authorization }
        });
        (endpoint_url(addr), task)
    }

    fn endpoint_url(addr: SocketAddr) -> String {
        format!("http://{addr}/alpha/usage")
    }

    async fn read_http_head(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut tmp = [0_u8; 1024];
        loop {
            let n = stream.read(&mut tmp).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if buf.windows(4).any(|window| window == b"\r\n\r\n") || buf.len() > 16 * 1024 {
                break;
            }
        }
        buf
    }

    fn authorization_header(head: &[u8]) -> Option<String> {
        String::from_utf8_lossy(head).lines().find_map(|line| {
            let line = line.trim_end_matches('\r');
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("authorization")
                    .then(|| value.trim().to_string())
            })
        })
    }
}
