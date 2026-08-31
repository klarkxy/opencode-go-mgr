//! Pure attempt-adjacent provider/transport error classification policy.
//!
//! [`ProviderErrorClass`] is data only: the host forwarder still owns logging,
//! CAS, cooldown writes, usage-sync scheduling, and wire envelopes. This table
//! exists so `forwarder` does not grow more `provider_id` policy branches.
//!
//! Runtime 401/429 rules here freeze Stage 0 `forwarder` behavior. OpenCode Go
//! 401 is passthrough at this layer; only exact structured CreditsError rotates
//! in the host. Only OpenCode responses use the provider-specific usage-window
//! parser.
//!
//! HTTP classification takes `free_channel: bool` rather than a host channel
//! enum. Window parsing, cooldown durations, and 429 body text stay in the host.
//!
//! Items are rust-public only as the cross-crate bridge; the host crate's
//! `gateway::classify` compatibility facade keeps them crate-private.

use crate::attempt::TransportFailureKind;
use ocg_domain::ids::{OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID};
use ocg_domain::provider::ProviderAdapterKind;
use serde_json::Value;

/// Semantic class of one attempt failure. Side effects stay in the forwarder.
///
/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this type crate-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum ProviderErrorClass {
    RouteUnavailable,
    DecryptFailed,
    Connect,
    OutcomeUnknown,
    RateLimited { policy: RateLimitPolicy },
    UnauthorizedPassthrough,
    UnauthorizedRotate,
    ForbiddenStop,
    ForbiddenRotate,
    HttpRequestTimeout,
    ClientError,
    ServerError,
    StreamRetryEligible,
    StreamNoReplay,
}

/// How a classified 429 cools down and whether it may fall through.
///
/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this type crate-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum RateLimitPolicy {
    /// Parse OpenCode Go window text, rotate, key-match CAS, deferred usage sync.
    GoWindow,
    /// Shared egress-IP Free channel; exhaust it, no key rotate, no usage sync.
    ZenFreeShared,
    /// Custom/GOAT: five-minute generic cooldown, no Go window parse, no usage sync.
    GenericFiveMinute,
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this type crate-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum RateLimitFallback {
    ExhaustFreeChannel,
    TryNextAccount,
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this type crate-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum Auth401Policy {
    Passthrough,
    RotatePersistAuthError,
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this type crate-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum RateLimit429Policy {
    GoWindow,
    GenericFiveMinute,
}

/// Static per-adapter 401/429 policy. Free-channel 429 overlay is applied by
/// [`classify_http`] from the `free_channel` flag, not from adapter identity.
///
/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this type crate-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub struct ProviderErrorPolicy {
    #[doc(hidden)]
    pub inference_401: Auth401Policy,
    #[doc(hidden)]
    pub rate_limit_429: RateLimit429Policy,
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this type crate-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum PreflightKind {
    Route,
    Decrypt,
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this type crate-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum TransportClassifyInput {
    Connect,
    SendTimeout,
    HeaderTimeout,
    BodyTimeout,
    OtherSendFailure,
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this type crate-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum StreamClassifyInput {
    InterruptedBeforeOutput,
    EndedIncompleteBeforeOutput,
    ConversionFailedBeforeOutput,
    IdleTimeoutBeforeOutput,
    AfterDownstreamBytes,
}

impl From<TransportFailureKind> for TransportClassifyInput {
    fn from(kind: TransportFailureKind) -> Self {
        match kind {
            TransportFailureKind::Connect => Self::Connect,
            TransportFailureKind::Timeout => Self::SendTimeout,
            TransportFailureKind::Other => Self::OtherSendFailure,
        }
    }
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this function crate-private.
#[doc(hidden)]
pub fn provider_error_policy(provider_id: &str, offering_id: &str) -> ProviderErrorPolicy {
    match ProviderAdapterKind::from_offering(provider_id, offering_id) {
        Some(kind) => policy_for_kind(kind),
        None => ProviderErrorPolicy {
            // Stage 0 matched OpenCode/Zen 401 on provider_id only.
            inference_401: if is_opencode_family_provider(provider_id) {
                Auth401Policy::Passthrough
            } else {
                Auth401Policy::RotatePersistAuthError
            },
            rate_limit_429: RateLimit429Policy::GoWindow,
        },
    }
}

fn policy_for_kind(kind: ProviderAdapterKind) -> ProviderErrorPolicy {
    match kind {
        ProviderAdapterKind::OpenCodeGo => ProviderErrorPolicy {
            // Status-only default: Go uses 401 for ModelError as well as
            // account failures. classify_http_response refines only the exact
            // structured CreditsError case.
            inference_401: Auth401Policy::Passthrough,
            rate_limit_429: RateLimit429Policy::GoWindow,
        },
        ProviderAdapterKind::ZenFree => ProviderErrorPolicy {
            inference_401: Auth401Policy::Passthrough,
            rate_limit_429: RateLimit429Policy::GoWindow,
        },
        ProviderAdapterKind::CommandCodeGoat
        | ProviderAdapterKind::MiniMaxCn
        | ProviderAdapterKind::KimiCn
        | ProviderAdapterKind::ConfigurableHttp
        | ProviderAdapterKind::Cpa => ProviderErrorPolicy {
            inference_401: Auth401Policy::RotatePersistAuthError,
            rate_limit_429: RateLimit429Policy::GenericFiveMinute,
        },
    }
}

fn is_opencode_family_provider(provider_id: &str) -> bool {
    matches!(
        provider_id,
        OPENCODE_PROVIDER_ID | OPENCODE_ZEN_FREE_PROVIDER_ID
    )
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this function crate-private.
#[doc(hidden)]
pub fn classify_preflight(kind: PreflightKind) -> ProviderErrorClass {
    match kind {
        PreflightKind::Route => ProviderErrorClass::RouteUnavailable,
        PreflightKind::Decrypt => ProviderErrorClass::DecryptFailed,
    }
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this function crate-private.
#[doc(hidden)]
pub fn classify_transport(input: TransportClassifyInput) -> ProviderErrorClass {
    match input {
        TransportClassifyInput::Connect => ProviderErrorClass::Connect,
        TransportClassifyInput::SendTimeout
        | TransportClassifyInput::HeaderTimeout
        | TransportClassifyInput::BodyTimeout
        | TransportClassifyInput::OtherSendFailure => ProviderErrorClass::OutcomeUnknown,
    }
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this function crate-private.
#[doc(hidden)]
pub fn classify_stream(input: StreamClassifyInput) -> ProviderErrorClass {
    match input {
        StreamClassifyInput::InterruptedBeforeOutput
        | StreamClassifyInput::EndedIncompleteBeforeOutput => {
            ProviderErrorClass::StreamRetryEligible
        }
        StreamClassifyInput::ConversionFailedBeforeOutput
        | StreamClassifyInput::IdleTimeoutBeforeOutput
        | StreamClassifyInput::AfterDownstreamBytes => ProviderErrorClass::StreamNoReplay,
    }
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this function crate-private.
#[doc(hidden)]
pub fn classify_http(
    status: u16,
    provider_id: &str,
    offering_id: &str,
    free_channel: bool,
    anonymous: bool,
) -> ProviderErrorClass {
    if (500..600).contains(&status) {
        return ProviderErrorClass::ServerError;
    }
    if status == 429 {
        let policy = provider_error_policy(provider_id, offering_id);
        let rate = match policy.rate_limit_429 {
            RateLimit429Policy::GenericFiveMinute => RateLimitPolicy::GenericFiveMinute,
            RateLimit429Policy::GoWindow if free_channel => RateLimitPolicy::ZenFreeShared,
            RateLimit429Policy::GoWindow => RateLimitPolicy::GoWindow,
        };
        return ProviderErrorClass::RateLimited { policy: rate };
    }
    if status == 408 {
        return ProviderErrorClass::HttpRequestTimeout;
    }
    if status == 401 {
        return match provider_error_policy(provider_id, offering_id).inference_401 {
            Auth401Policy::Passthrough => ProviderErrorClass::UnauthorizedPassthrough,
            Auth401Policy::RotatePersistAuthError => ProviderErrorClass::UnauthorizedRotate,
        };
    }
    if status == 403 {
        return if anonymous {
            ProviderErrorClass::ForbiddenStop
        } else {
            ProviderErrorClass::ForbiddenRotate
        };
    }
    if (400..500).contains(&status) {
        return ProviderErrorClass::ClientError;
    }
    ProviderErrorClass::ClientError
}

/// Refines an HTTP classification with a bounded upstream response body.
///
/// OpenCode Go uses 401 for both unsupported models and account-level credit
/// failures. Only its structured `CreditsError` is strong enough evidence to
/// skip and break the current account; malformed, unknown, and `ModelError`
/// responses retain the conservative 401 passthrough behavior.
#[doc(hidden)]
pub fn classify_http_response(
    status: u16,
    provider_id: &str,
    offering_id: &str,
    free_channel: bool,
    anonymous: bool,
    response_body: &str,
) -> ProviderErrorClass {
    let base = classify_http(status, provider_id, offering_id, free_channel, anonymous);
    if status == 401
        && ProviderAdapterKind::from_offering(provider_id, offering_id)
            == Some(ProviderAdapterKind::OpenCodeGo)
        && response_has_error_type(response_body, "CreditsError")
    {
        ProviderErrorClass::UnauthorizedRotate
    } else {
        base
    }
}

fn response_has_error_type(response_body: &str, expected: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(response_body) else {
        return false;
    };
    value
        .pointer("/error/type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind == expected)
}

/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps this function crate-private.
#[doc(hidden)]
pub fn schedule_go_usage_sync(class: ProviderErrorClass) -> bool {
    matches!(
        class,
        ProviderErrorClass::RateLimited {
            policy: RateLimitPolicy::GoWindow
        }
    )
}

impl ProviderErrorClass {
    pub fn same_account_retry_eligible(self) -> bool {
        matches!(self, Self::Connect | Self::StreamRetryEligible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocg_domain::ids::{
        ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_PROVIDER_ID, CUSTOM_API_OFFERING_ID,
        CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID,
    };

    fn classify(
        status: u16,
        provider_id: &str,
        offering_id: &str,
        free_channel: bool,
        anonymous: bool,
    ) -> ProviderErrorClass {
        classify_http(status, provider_id, offering_id, free_channel, anonymous)
    }

    fn production_source(source: &str) -> &str {
        source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests")
    }

    #[test]
    fn classify_production_source_stays_policy_only() {
        let production = production_source(include_str!("classify.rs"));
        assert!(
            production.contains("use ocg_domain::provider::ProviderAdapterKind"),
            "classify.rs must import ProviderAdapterKind from ocg_domain::provider"
        );
        assert!(
            production.contains("free_channel: bool"),
            "classify_http must take free_channel: bool rather than a host channel type"
        );
        assert!(
            production.contains("impl From<TransportFailureKind> for TransportClassifyInput"),
            "TransportFailureKind must map locally into TransportClassifyInput"
        );
        for needle in [
            "CoreState",
            "Database",
            "reqwest",
            "rusqlite",
            "tokio",
            "axum",
            "chrono",
            "UsageWindowKind",
            "UpstreamChannel",
            "ocg_core",
            "rate_limit_window_and_cooldown",
            "parse_reset",
            "parse_usage_limit_window",
            "parse_free_reset",
            "std::fs",
            "std::process",
            "KeyCipher",
            "decrypt_key",
            "key_cipher",
        ] {
            assert!(
                !production.contains(needle),
                "production ocg-gateway classify source must not name `{needle}`"
            );
        }
    }

    #[test]
    fn provider_error_policy_covers_every_adapter_kind() {
        for kind in ProviderAdapterKind::ALL {
            let policy = policy_for_kind(kind);
            match kind {
                ProviderAdapterKind::OpenCodeGo => {
                    assert_eq!(policy.inference_401, Auth401Policy::Passthrough);
                    assert_eq!(policy.rate_limit_429, RateLimit429Policy::GoWindow);
                }
                ProviderAdapterKind::ZenFree => {
                    assert_eq!(policy.inference_401, Auth401Policy::Passthrough);
                    assert_eq!(policy.rate_limit_429, RateLimit429Policy::GoWindow);
                }
                ProviderAdapterKind::CommandCodeGoat
                | ProviderAdapterKind::MiniMaxCn
                | ProviderAdapterKind::KimiCn
                | ProviderAdapterKind::ConfigurableHttp
                | ProviderAdapterKind::Cpa => {
                    assert_eq!(policy.inference_401, Auth401Policy::RotatePersistAuthError);
                    assert_eq!(policy.rate_limit_429, RateLimit429Policy::GenericFiveMinute);
                }
            }
        }
    }

    #[test]
    fn opencode_and_zen_401_passthrough_without_rotation() {
        assert_eq!(
            classify(401, OPENCODE_PROVIDER_ID, GO_OFFERING_ID, false, false),
            ProviderErrorClass::UnauthorizedPassthrough
        );
        assert_eq!(
            classify(
                401,
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID,
                true,
                true
            ),
            ProviderErrorClass::UnauthorizedPassthrough
        );
        assert_eq!(
            classify(
                401,
                OPENCODE_PROVIDER_ID,
                "not-a-catalog-offering",
                false,
                false
            ),
            ProviderErrorClass::UnauthorizedPassthrough
        );
    }

    #[test]
    fn opencode_go_credits_401_rotates_but_model_and_unknown_401_passthrough() {
        let classify_body = |body| {
            classify_http_response(
                401,
                OPENCODE_PROVIDER_ID,
                GO_OFFERING_ID,
                false,
                false,
                body,
            )
        };
        assert_eq!(
            classify_body(
                r#"{"type":"error","error":{"type":"CreditsError","message":"No active subscription"}}"#
            ),
            ProviderErrorClass::UnauthorizedRotate
        );
        assert_eq!(
            classify_body(
                r#"{"type":"error","error":{"type":"ModelError","message":"not supported"}}"#
            ),
            ProviderErrorClass::UnauthorizedPassthrough
        );
        assert_eq!(
            classify_body(r#"{"error":{"message":"expired key"}}"#),
            ProviderErrorClass::UnauthorizedPassthrough
        );
        assert_eq!(
            classify_body(r#"{"error":{"type":"OtherError"}}"#),
            ProviderErrorClass::UnauthorizedPassthrough
        );
        assert_eq!(
            classify_body(r#"{"error":{"type":"creditserror"}}"#),
            ProviderErrorClass::UnauthorizedPassthrough
        );
        assert_eq!(
            classify_body("not json"),
            ProviderErrorClass::UnauthorizedPassthrough
        );
    }

    #[test]
    fn credits_error_refinement_is_go_only() {
        let body = r#"{"error":{"type":"CreditsError"}}"#;
        assert_eq!(
            classify_http_response(
                401,
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID,
                true,
                true,
                body,
            ),
            ProviderErrorClass::UnauthorizedPassthrough
        );
        assert_eq!(
            classify_http_response(
                401,
                CUSTOM_PROVIDER_ID,
                CUSTOM_API_OFFERING_ID,
                false,
                false,
                body,
            ),
            ProviderErrorClass::UnauthorizedRotate
        );
    }

    #[test]
    fn ordinary_401_rotates_and_persists_auth_error() {
        for (provider_id, offering_id) in [
            (CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID),
            (COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID),
            ("unknown-provider", "unknown-offering"),
        ] {
            assert_eq!(
                classify(401, provider_id, offering_id, false, false),
                ProviderErrorClass::UnauthorizedRotate,
                "{provider_id}/{offering_id}"
            );
        }
    }

    #[test]
    fn go_zen_free_and_generic_429_policies() {
        assert_eq!(
            classify(429, OPENCODE_PROVIDER_ID, GO_OFFERING_ID, false, false),
            ProviderErrorClass::RateLimited {
                policy: RateLimitPolicy::GoWindow
            }
        );
        assert_eq!(
            classify(
                429,
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID,
                true,
                true
            ),
            ProviderErrorClass::RateLimited {
                policy: RateLimitPolicy::ZenFreeShared
            }
        );
        assert_eq!(
            classify(
                429,
                CUSTOM_PROVIDER_ID,
                CUSTOM_API_OFFERING_ID,
                false,
                false
            ),
            ProviderErrorClass::RateLimited {
                policy: RateLimitPolicy::GenericFiveMinute
            }
        );
        assert_eq!(
            classify(
                429,
                COMMAND_CODE_PROVIDER_ID,
                GOAT_OFFERING_ID,
                false,
                false
            ),
            ProviderErrorClass::RateLimited {
                policy: RateLimitPolicy::GenericFiveMinute
            }
        );
        assert!(schedule_go_usage_sync(ProviderErrorClass::RateLimited {
            policy: RateLimitPolicy::GoWindow
        }));
        assert!(!schedule_go_usage_sync(ProviderErrorClass::RateLimited {
            policy: RateLimitPolicy::ZenFreeShared
        }));
        assert!(!schedule_go_usage_sync(ProviderErrorClass::RateLimited {
            policy: RateLimitPolicy::GenericFiveMinute
        }));
    }

    #[test]
    fn generic_429_wins_over_free_channel_and_zen_go_channel_parses_windows() {
        assert_eq!(
            classify(429, CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID, true, false),
            ProviderErrorClass::RateLimited {
                policy: RateLimitPolicy::GenericFiveMinute
            }
        );
        assert_eq!(
            classify(
                429,
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID,
                false,
                true
            ),
            ProviderErrorClass::RateLimited {
                policy: RateLimitPolicy::GoWindow
            }
        );
    }

    #[test]
    fn credentialed_403_rotates_anonymous_403_stops() {
        assert_eq!(
            classify(403, OPENCODE_PROVIDER_ID, GO_OFFERING_ID, false, false),
            ProviderErrorClass::ForbiddenRotate
        );
        assert_eq!(
            classify(
                403,
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID,
                true,
                true
            ),
            ProviderErrorClass::ForbiddenStop
        );
        assert_eq!(
            classify(
                403,
                CUSTOM_PROVIDER_ID,
                CUSTOM_API_OFFERING_ID,
                false,
                false
            ),
            ProviderErrorClass::ForbiddenRotate
        );
    }

    #[test]
    fn http_408_is_outcome_unknown_and_5xx_passthrough() {
        assert_eq!(
            classify(408, OPENCODE_PROVIDER_ID, GO_OFFERING_ID, false, false),
            ProviderErrorClass::HttpRequestTimeout
        );
        for status in [500, 502, 503, 599] {
            assert_eq!(
                classify(status, OPENCODE_PROVIDER_ID, GO_OFFERING_ID, false, false),
                ProviderErrorClass::ServerError,
                "{status}"
            );
        }
        for status in [400, 404, 413] {
            assert_eq!(
                classify(
                    status,
                    CUSTOM_PROVIDER_ID,
                    CUSTOM_API_OFFERING_ID,
                    false,
                    false
                ),
                ProviderErrorClass::ClientError,
                "{status}"
            );
        }
    }

    #[test]
    fn connect_is_retry_eligible_non_connect_transport_is_outcome_unknown() {
        assert_eq!(
            classify_transport(TransportClassifyInput::Connect),
            ProviderErrorClass::Connect
        );
        assert!(ProviderErrorClass::Connect.same_account_retry_eligible());
        for input in [
            TransportClassifyInput::SendTimeout,
            TransportClassifyInput::HeaderTimeout,
            TransportClassifyInput::BodyTimeout,
            TransportClassifyInput::OtherSendFailure,
        ] {
            let class = classify_transport(input);
            assert_eq!(class, ProviderErrorClass::OutcomeUnknown, "{input:?}");
            assert!(!class.same_account_retry_eligible());
        }
    }

    #[test]
    fn transport_failure_kind_from_impl_matches_classify_input() {
        assert_eq!(
            TransportClassifyInput::from(TransportFailureKind::Connect),
            TransportClassifyInput::Connect
        );
        assert_eq!(
            TransportClassifyInput::from(TransportFailureKind::Timeout),
            TransportClassifyInput::SendTimeout
        );
        assert_eq!(
            TransportClassifyInput::from(TransportFailureKind::Other),
            TransportClassifyInput::OtherSendFailure
        );
        assert_eq!(
            classify_transport(TransportFailureKind::Connect.into()),
            ProviderErrorClass::Connect
        );
        assert_eq!(
            classify_transport(TransportFailureKind::Timeout.into()),
            ProviderErrorClass::OutcomeUnknown
        );
        assert_eq!(
            classify_transport(TransportFailureKind::Other.into()),
            ProviderErrorClass::OutcomeUnknown
        );
    }

    #[test]
    fn stream_retry_only_before_downstream_bytes_for_interrupt_or_incomplete() {
        assert_eq!(
            classify_stream(StreamClassifyInput::InterruptedBeforeOutput),
            ProviderErrorClass::StreamRetryEligible
        );
        assert_eq!(
            classify_stream(StreamClassifyInput::EndedIncompleteBeforeOutput),
            ProviderErrorClass::StreamRetryEligible
        );
        assert!(ProviderErrorClass::StreamRetryEligible.same_account_retry_eligible());
        for input in [
            StreamClassifyInput::ConversionFailedBeforeOutput,
            StreamClassifyInput::IdleTimeoutBeforeOutput,
            StreamClassifyInput::AfterDownstreamBytes,
        ] {
            let class = classify_stream(input);
            assert_eq!(class, ProviderErrorClass::StreamNoReplay, "{input:?}");
            assert!(!class.same_account_retry_eligible());
        }
    }

    #[test]
    fn route_and_decrypt_preflight_are_explicit_classes() {
        assert_eq!(
            classify_preflight(PreflightKind::Route),
            ProviderErrorClass::RouteUnavailable
        );
        assert_eq!(
            classify_preflight(PreflightKind::Decrypt),
            ProviderErrorClass::DecryptFailed
        );
        assert!(!ProviderErrorClass::RouteUnavailable.same_account_retry_eligible());
        assert!(!ProviderErrorClass::DecryptFailed.same_account_retry_eligible());
    }
}
