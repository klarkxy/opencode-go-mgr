//! Compatibility facade for [`ocg_gateway::classify`].
//!
//! Crate-private items match the historical `ocg_core::gateway::classify`
//! surface. The public module path is unchanged; item visibility is not
//! widened. Do not glob-reexport or reexport the module itself.
//!
//! Pure classification policy lives in `ocg-gateway`. This module keeps the
//! host `classify_http` signature, 429 window/cooldown parsing, and fallback
//! derived from [`UsageWindowKind`].

use crate::gateway::limit::{parse_free_reset_or_default, parse_reset, parse_usage_limit_window};
use crate::models::{UpstreamChannel, UsageWindowKind};
use chrono::Duration;

pub(crate) use ocg_gateway::classify::{
    Auth401Policy, PreflightKind, ProviderErrorClass, ProviderErrorPolicy, RateLimit429Policy,
    RateLimitFallback, RateLimitPolicy, StreamClassifyInput, TransportClassifyInput,
    classify_preflight, classify_stream, classify_transport, provider_error_policy,
    schedule_go_usage_sync,
};

const _: fn(&str, &str) -> ProviderErrorPolicy = provider_error_policy;
const _: fn(ProviderErrorPolicy) -> (Auth401Policy, RateLimit429Policy) = split_provider_policy;

const fn split_provider_policy(policy: ProviderErrorPolicy) -> (Auth401Policy, RateLimit429Policy) {
    (policy.inference_401, policy.rate_limit_429)
}

/// Host compatibility wrapper: converts [`UpstreamChannel::Free`] to the
/// gateway classifier's `free_channel` flag.
pub(crate) fn classify_http(
    status: u16,
    provider_id: &str,
    offering_id: &str,
    channel: UpstreamChannel,
    anonymous: bool,
) -> ProviderErrorClass {
    ocg_gateway::classify::classify_http(
        status,
        provider_id,
        offering_id,
        channel == UpstreamChannel::Free,
        anonymous,
    )
}

/// Host compatibility wrapper for body-aware inference error classification.
pub(crate) fn classify_http_response(
    status: u16,
    provider_id: &str,
    offering_id: &str,
    channel: UpstreamChannel,
    anonymous: bool,
    response_body: &str,
) -> ProviderErrorClass {
    ocg_gateway::classify::classify_http_response(
        status,
        provider_id,
        offering_id,
        channel == UpstreamChannel::Free,
        anonymous,
        response_body,
    )
}

pub(crate) fn rate_limit_window_and_cooldown(
    policy: RateLimitPolicy,
    text: &str,
) -> (Option<UsageWindowKind>, Duration) {
    match policy {
        RateLimitPolicy::GenericFiveMinute => (None, Duration::minutes(5)),
        RateLimitPolicy::ZenFreeShared => (
            Some(UsageWindowKind::Free),
            parse_free_reset_or_default(text),
        ),
        RateLimitPolicy::GoWindow => {
            let window = parse_usage_limit_window(text);
            let cooldown = if window == Some(UsageWindowKind::Free) {
                parse_free_reset_or_default(text)
            } else {
                parse_reset(text).unwrap_or_else(|| Duration::minutes(5))
            };
            (window, cooldown)
        }
    }
}

pub(crate) fn rate_limit_fallback(window: Option<UsageWindowKind>) -> RateLimitFallback {
    if window == Some(UsageWindowKind::Free) {
        RateLimitFallback::ExhaustFreeChannel
    } else {
        RateLimitFallback::TryNextAccount
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    #[test]
    fn facade_reexports_gateway_classify_types_without_widening() {
        assert_eq!(
            TypeId::of::<Auth401Policy>(),
            TypeId::of::<ocg_gateway::classify::Auth401Policy>()
        );
        assert_eq!(
            TypeId::of::<PreflightKind>(),
            TypeId::of::<ocg_gateway::classify::PreflightKind>()
        );
        assert_eq!(
            TypeId::of::<ProviderErrorClass>(),
            TypeId::of::<ocg_gateway::classify::ProviderErrorClass>()
        );
        assert_eq!(
            TypeId::of::<ProviderErrorPolicy>(),
            TypeId::of::<ocg_gateway::classify::ProviderErrorPolicy>()
        );
        assert_eq!(
            TypeId::of::<RateLimit429Policy>(),
            TypeId::of::<ocg_gateway::classify::RateLimit429Policy>()
        );
        assert_eq!(
            TypeId::of::<RateLimitFallback>(),
            TypeId::of::<ocg_gateway::classify::RateLimitFallback>()
        );
        assert_eq!(
            TypeId::of::<RateLimitPolicy>(),
            TypeId::of::<ocg_gateway::classify::RateLimitPolicy>()
        );
        assert_eq!(
            TypeId::of::<StreamClassifyInput>(),
            TypeId::of::<ocg_gateway::classify::StreamClassifyInput>()
        );
        assert_eq!(
            TypeId::of::<TransportClassifyInput>(),
            TypeId::of::<ocg_gateway::classify::TransportClassifyInput>()
        );
        let _: fn(&str, &str) -> ProviderErrorPolicy = provider_error_policy;
        let _: fn(&str, &str) -> ProviderErrorPolicy = ocg_gateway::classify::provider_error_policy;
        let _: fn(PreflightKind) -> ProviderErrorClass = classify_preflight;
        let _: fn(ProviderErrorClass) -> bool = schedule_go_usage_sync;
    }

    #[test]
    fn free_429_does_not_rotate_keys() {
        for misleading_body in [
            "5-hour usage limit reached. Resets in 13min.",
            "Weekly usage limit reached. Resets in 4 days.",
            "Monthly usage limit reached. Resets in 13 days.",
        ] {
            let (window, _) =
                rate_limit_window_and_cooldown(RateLimitPolicy::ZenFreeShared, misleading_body);
            assert_eq!(window, Some(UsageWindowKind::Free), "{misleading_body}");
            assert_eq!(
                rate_limit_fallback(window),
                RateLimitFallback::ExhaustFreeChannel
            );
        }
        assert_eq!(
            rate_limit_fallback(Some(UsageWindowKind::FiveHours)),
            RateLimitFallback::TryNextAccount
        );
        assert_eq!(rate_limit_fallback(None), RateLimitFallback::TryNextAccount);
    }

    #[test]
    fn goat_429_is_generic_and_ignores_go_limit_windows() {
        for misleading_body in [
            "5-hour usage limit reached. Resets in 13min.",
            "Weekly usage limit reached. Resets in 4 days.",
            "Monthly usage limit reached. Resets in 13 days.",
            r#"{"type":"GoUsageLimitError","message":"Weekly usage limit reached. Resets in 3 days."}"#,
        ] {
            let (window, cooldown) =
                rate_limit_window_and_cooldown(RateLimitPolicy::GenericFiveMinute, misleading_body);
            assert_eq!(window, None, "{misleading_body}");
            assert_eq!(cooldown, Duration::minutes(5), "{misleading_body}");
        }
        let (go_window, go_cooldown) = rate_limit_window_and_cooldown(
            RateLimitPolicy::GoWindow,
            "Weekly usage limit reached. Resets in 4 days.",
        );
        assert_eq!(go_window, Some(UsageWindowKind::Week));
        assert_eq!(go_cooldown, Duration::days(4));
    }

    #[test]
    fn go_429_free_wording_still_exhausts_the_free_window() {
        let (window, _) = rate_limit_window_and_cooldown(
            RateLimitPolicy::GoWindow,
            "Free usage limit reached. Resets in 13min.",
        );
        assert_eq!(window, Some(UsageWindowKind::Free));
        assert_eq!(
            rate_limit_fallback(window),
            RateLimitFallback::ExhaustFreeChannel
        );
    }
}
