//! Host adapter around [`ocg_gateway::selector::SelectorState`].
//!
//! Owned outside `gateway` so `state` can hold the process slot without a
//! `state -> gateway` edge. Account eligibility, wall-clock cooling, Free dual
//! gates, and provider fail-closed stay in Core. Conversation-key parsing stays
//! in `gateway::routing` and is re-exported from there.

use crate::kernel::catalog::CredentialKind;
use crate::models::{Account, RoutingMode, UpstreamChannel};
use crate::provider::ProviderAdapterKind;
use chrono::{DateTime, Utc};
use ocg_gateway::selector::{BaseAvailability, Candidate as GatewayCandidate, SelectionPolicy};
use parking_lot::Mutex;
use std::time::{Duration, Instant};

#[cfg(test)]
use crate::kernel::ids::{ANONYMOUS_FREE_OFFERING_ID, OPENCODE_ZEN_FREE_PROVIDER_ID};

pub const CONVERSATION_TTL: Duration = ocg_gateway::selector::CONVERSATION_TTL;
pub const MAX_CONVERSATIONS: usize = ocg_gateway::selector::MAX_CONVERSATIONS;

#[derive(Debug, Default)]
pub struct RoutingRuntime {
    inner: Mutex<ocg_gateway::selector::SelectorState>,
}

#[derive(Debug, Clone)]
pub struct RoutingCandidate {
    pub account: Account,
    pub channel: UpstreamChannel,
    pub resolved_model: String,
}

impl RoutingRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&self) {
        self.inner.lock().reset();
    }

    /// Select an account for Go channel requests (test and legacy callers).
    pub fn select_account(
        &self,
        accounts: &[Account],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
    ) -> Option<Account> {
        self.select_account_at(
            accounts,
            mode,
            conversation_sticky,
            conversation_key,
            exclude_ids,
            Utc::now(),
            Instant::now(),
        )
    }

    /// Select an account for Go channel requests against an explicit wall/mono pair.
    #[allow(clippy::too_many_arguments)]
    pub fn select_account_at(
        &self,
        accounts: &[Account],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
        wall: DateTime<Utc>,
        mono: Instant,
    ) -> Option<Account> {
        self.select_account_for_at(
            accounts,
            mode,
            conversation_sticky,
            conversation_key,
            UpstreamChannel::Go,
            "",
            exclude_ids,
            wall,
            mono,
        )
    }

    /// Select an account for a generation request and update sticky/round-robin state.
    #[allow(clippy::too_many_arguments)]
    pub fn select_account_for(
        &self,
        accounts: &[Account],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        channel: UpstreamChannel,
        resolved_model: &str,
        exclude_ids: &[&str],
    ) -> Option<Account> {
        self.select_account_for_at(
            accounts,
            mode,
            conversation_sticky,
            conversation_key,
            channel,
            resolved_model,
            exclude_ids,
            Utc::now(),
            Instant::now(),
        )
    }

    /// Select an account against an explicit wall/mono pair.
    #[allow(clippy::too_many_arguments)]
    pub fn select_account_for_at(
        &self,
        accounts: &[Account],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        channel: UpstreamChannel,
        resolved_model: &str,
        exclude_ids: &[&str],
        wall: DateTime<Utc>,
        mono: Instant,
    ) -> Option<Account> {
        let candidates = accounts
            .iter()
            .cloned()
            .map(|account| RoutingCandidate {
                account,
                channel,
                resolved_model: resolved_model.to_string(),
            })
            .collect::<Vec<_>>();
        self.select_candidate_at(
            &candidates,
            mode,
            conversation_sticky,
            conversation_key,
            exclude_ids,
            wall,
            mono,
        )
        .map(|candidate| candidate.account)
    }

    /// Select one already capability-filtered route target. Candidates retain
    /// database order, while each carries its own provider channel and resolved
    /// model (for example, a Zen mapped model beside later paid accounts).
    pub fn select_candidate(
        &self,
        candidates: &[RoutingCandidate],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
    ) -> Option<RoutingCandidate> {
        self.select_candidate_at(
            candidates,
            mode,
            conversation_sticky,
            conversation_key,
            exclude_ids,
            Utc::now(),
            Instant::now(),
        )
    }

    /// Select one capability-filtered route target against an explicit wall/mono pair.
    /// Wall drives cooldown/availability; mono drives conversation TTL.
    ///
    /// Duplicate account ids fail closed to `None` and leave sticky / round-robin
    /// / conversation state unchanged.
    #[allow(clippy::too_many_arguments)]
    pub fn select_candidate_at(
        &self,
        candidates: &[RoutingCandidate],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
        wall: DateTime<Utc>,
        mono: Instant,
    ) -> Option<RoutingCandidate> {
        match self.try_select_candidate_index_at(
            candidates,
            mode,
            conversation_sticky,
            conversation_key,
            exclude_ids,
            true,
            wall,
            mono,
        ) {
            Ok(Some(index)) => candidates.get(index).cloned(),
            Ok(None) | Err(_) => None,
        }
    }

    /// Typed production selection. Returns a slice index into `candidates`.
    ///
    /// Base availability is computed with no transient excludes. Free candidates
    /// are closed when `free_channel_available` is false (durable SQLite gate and
    /// disabled-Zen-row exhaustion combined by the caller). Duplicate account
    /// ids error before any state mutation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_select_candidate_index_at(
        &self,
        candidates: &[RoutingCandidate],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
        free_channel_available: bool,
        wall: DateTime<Utc>,
        mono: Instant,
    ) -> Result<Option<usize>, ocg_gateway::selector::SelectionError> {
        let gateway_candidates = candidates
            .iter()
            .map(|candidate| gateway_candidate(candidate, free_channel_available, wall))
            .collect::<Vec<_>>();
        let mut state = self.inner.lock();
        Ok(state
            .select_at(
                &gateway_candidates,
                selection_policy(mode),
                conversation_sticky,
                conversation_key,
                exclude_ids,
                mono,
            )?
            .map(|selection| selection.candidate_index()))
    }

    /// Read sticky binding for a conversation if still fresh.
    pub fn sticky_binding(
        &self,
        conversation_key: &str,
    ) -> Option<(String, UpstreamChannel, String)> {
        self.sticky_binding_at(conversation_key, Instant::now())
    }

    /// Read sticky binding against an explicit monotonic instant.
    pub fn sticky_binding_at(
        &self,
        conversation_key: &str,
        now: Instant,
    ) -> Option<(String, UpstreamChannel, String)> {
        let mut state = self.inner.lock();
        state.binding_at(conversation_key, now).map(|binding| {
            (
                binding.account_id().to_string(),
                binding.channel(),
                binding.resolved_model().to_string(),
            )
        })
    }
}

pub(crate) fn account_is_available_for(
    account: &Account,
    channel: UpstreamChannel,
    exclude_ids: &[&str],
) -> bool {
    account_is_available_for_at(account, channel, exclude_ids, Utc::now())
}

pub(crate) fn account_is_available_for_at(
    account: &Account,
    channel: UpstreamChannel,
    exclude_ids: &[&str],
    now: DateTime<Utc>,
) -> bool {
    account.enabled
        && account.setup_step.is_ready()
        && account_matches_channel(account, channel)
        && match account.credential_kind {
            CredentialKind::ApiKey => !account.key_cipher.is_empty(),
            CredentialKind::None => true,
        }
        && account.auth_error.is_none()
        && !exclude_ids.iter().any(|excluded| account.id == *excluded)
        && !account.is_cooling_for(channel, now)
}

/// Runtime channel owned by one valid sealed provider/offering binding.
///
/// Keeping this mapping beside selector eligibility prevents observability and
/// other read paths from growing their own, incomplete provider lists.
pub(crate) fn account_channel(account: &Account) -> Option<UpstreamChannel> {
    if account.validate_provider_binding().is_err() {
        return None;
    }
    match ProviderAdapterKind::from_offering(&account.provider_id, &account.offering_id)? {
        ProviderAdapterKind::OpenCodeGo
        | ProviderAdapterKind::CommandCodeGoat
        | ProviderAdapterKind::MiniMaxCn
        | ProviderAdapterKind::KimiCn
        | ProviderAdapterKind::Cpa
        | ProviderAdapterKind::ConfigurableHttp => Some(UpstreamChannel::Go),
        ProviderAdapterKind::ZenFree => Some(UpstreamChannel::Free),
    }
}

pub(crate) fn free_channel_is_exhausted_at(accounts: &[Account], now: DateTime<Utc>) -> bool {
    accounts
        .iter()
        .filter(|account| {
            account.id == crate::kernel::ids::ZEN_FREE_ACCOUNT_ID
                && account.provider_id == crate::kernel::ids::OPENCODE_ZEN_FREE_PROVIDER_ID
                && account.offering_id == crate::kernel::ids::ANONYMOUS_FREE_OFFERING_ID
        })
        .any(|account| account.cooldown_free_until.is_some_and(|until| until > now))
}

fn account_matches_channel(account: &Account, channel: UpstreamChannel) -> bool {
    account_channel(account) == Some(channel)
}

fn selection_policy(mode: RoutingMode) -> SelectionPolicy {
    match mode {
        RoutingMode::StrictPriority => SelectionPolicy::StrictPriority,
        RoutingMode::StickyGlobal => SelectionPolicy::StickyGlobal,
        RoutingMode::RoundRobin => SelectionPolicy::RoundRobin,
    }
}

fn gateway_candidate<'a>(
    candidate: &'a RoutingCandidate,
    free_channel_available: bool,
    wall: DateTime<Utc>,
) -> GatewayCandidate<'a> {
    let available = account_is_available_for_at(&candidate.account, candidate.channel, &[], wall)
        && (candidate.channel != UpstreamChannel::Free || free_channel_available);
    GatewayCandidate::new(
        candidate.account.id.as_str(),
        candidate.channel,
        candidate.resolved_model.as_str(),
        if available {
            BaseAvailability::Available
        } else {
            BaseAvailability::Unavailable
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::kernel::catalog::QuotaScope;
    use crate::kernel::ids::ZEN_FREE_ACCOUNT_ID;
    use ocg_gateway::selector::SelectionError;
    use std::sync::Arc;

    fn account(id: &str, enabled: bool) -> Account {
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        Account {
            id: id.into(),
            provider_id: crate::provider::default_provider_id(),
            offering_id: crate::provider::default_offering_id(),
            credential_kind: crate::provider::default_credential_kind(),
            quota_scope: crate::provider::default_quota_scope(),
            name: id.into(),
            username: None,
            password_cipher: None,
            key_cipher: cipher.encrypt(id).unwrap(),
            enabled,
            account_type: crate::models::AccountType::Key,
            setup_step: crate::models::AccountSetupStep::Ready,
            referral_code: None,
            purchase_date: String::new(),
            expires_on: String::new(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            cooldown_free_until: None,
            last_error: None,
            auth_error: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn cooling(id: &str) -> Account {
        let mut item = account(id, true);
        item.cooldown_generic_until = Some(Utc::now() + chrono::Duration::hours(1));
        item.cooldown_until = item.cooldown_generic_until;
        item
    }

    fn frozen_wall() -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2024, 1, 2)
                .unwrap()
                .and_hms_opt(3, 4, 5)
                .unwrap(),
            Utc,
        )
    }

    fn cooling_at(id: &str, until: DateTime<Utc>) -> Account {
        let mut item = account(id, true);
        item.cooldown_generic_until = Some(until);
        item.cooldown_until = Some(until);
        item
    }

    fn zen_account(enabled: bool) -> Account {
        let mut item = account(ZEN_FREE_ACCOUNT_ID, enabled);
        item.provider_id = OPENCODE_ZEN_FREE_PROVIDER_ID.into();
        item.offering_id = ANONYMOUS_FREE_OFFERING_ID.into();
        item.credential_kind = CredentialKind::None;
        item.quota_scope = QuotaScope::EgressIp;
        item.key_cipher.clear();
        item
    }

    fn routing_candidate(
        account: Account,
        channel: UpstreamChannel,
        resolved_model: &str,
    ) -> RoutingCandidate {
        RoutingCandidate {
            account,
            channel,
            resolved_model: resolved_model.to_string(),
        }
    }

    fn go_candidate(item: Account) -> RoutingCandidate {
        routing_candidate(item, UpstreamChannel::Go, "test-model")
    }

    #[allow(clippy::too_many_arguments)]
    fn pick_index(
        runtime: &RoutingRuntime,
        candidates: &[RoutingCandidate],
        mode: RoutingMode,
        conversation_sticky: bool,
        conversation_key: Option<&str>,
        exclude_ids: &[&str],
        free_channel_available: bool,
        wall: DateTime<Utc>,
        mono: Instant,
    ) -> Option<usize> {
        runtime
            .try_select_candidate_index_at(
                candidates,
                mode,
                conversation_sticky,
                conversation_key,
                exclude_ids,
                free_channel_available,
                wall,
                mono,
            )
            .expect("candidates must not contain duplicate account ids")
    }

    #[test]
    fn strict_priority_picks_first_available() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", false), account("b", true), account("c", true)];
        let selected = runtime
            .select_account(&accounts, RoutingMode::StrictPriority, false, None, &[])
            .unwrap();
        assert_eq!(selected.id, "b");
    }

    #[test]
    fn sticky_global_keeps_current_when_higher_priority_recovers() {
        let runtime = RoutingRuntime::new();
        let first = vec![cooling("a"), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&first, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );
        let recovered = vec![account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&recovered, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );
    }

    #[test]
    fn sticky_global_transient_exclude_does_not_rewrite_global() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
        // Request-local exclude (e.g. 403/preflight failover): use next account now,
        // but keep the persistent global sticky on a.
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &["a"])
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn sticky_global_switches_when_current_persistently_unavailable() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
        let disabled = vec![account("a", false), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&disabled, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );

        let runtime = RoutingRuntime::new();
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
        let cooled = vec![cooling("a"), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&cooled, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StickyGlobal, false, None, &[])
                .unwrap()
                .id,
            "b"
        );
    }

    #[test]
    fn round_robin_cycles_and_skips_unavailable() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), cooling("b"), account("c", true)];
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "c"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn round_robin_cursor_survives_reordering_and_missing_accounts_by_id() {
        let runtime = RoutingRuntime::new();
        let original = vec![account("a", true), account("b", true), account("c", true)];
        assert_eq!(
            runtime
                .select_account(&original, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "a"
        );

        let reordered = vec![account("c", true), account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&reordered, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "b"
        );

        let missing_cursor = vec![account("a", true), account("c", true)];
        assert_eq!(
            runtime
                .select_account(&missing_cursor, RoutingMode::RoundRobin, false, None, &[],)
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn concurrent_round_robin_selection_updates_one_shared_cursor() {
        let runtime = Arc::new(RoutingRuntime::new());
        let accounts = Arc::new(vec![account("a", true), account("b", true)]);
        let workers = (0..100)
            .map(|_| {
                let runtime = runtime.clone();
                let accounts = accounts.clone();
                std::thread::spawn(move || {
                    runtime
                        .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                        .unwrap()
                        .id
                })
            })
            .collect::<Vec<_>>();
        let selected = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(selected.iter().filter(|id| id.as_str() == "a").count(), 50);
        assert_eq!(selected.iter().filter(|id| id.as_str() == "b").count(), 50);
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn conversation_sticky_prefers_binding_without_advancing_round_robin() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        let key = "conv-1";
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, true, Some(key), &[],)
                .unwrap()
                .id,
            "a"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, true, Some(key), &[],)
                .unwrap()
                .id,
            "a"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "b",
            "conversation hits must not advance the round-robin cursor"
        );
    }

    #[test]
    fn conversation_sticky_rebinds_when_bound_account_excluded() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        let key = "conv-2";
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StrictPriority, true, Some(key), &[],)
                .unwrap()
                .id,
            "a"
        );
        assert_eq!(
            runtime
                .select_account(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some(key),
                    &["a"],
                )
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::StrictPriority, true, Some(key), &[],)
                .unwrap()
                .id,
            "b"
        );
    }

    #[test]
    fn conversation_ttl_expires_bindings() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        let wall = frozen_wall();
        let t0 = Instant::now();
        assert_eq!(
            runtime
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some("old"),
                    &["a"],
                    wall,
                    t0,
                )
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            runtime
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some("old"),
                    &[],
                    wall,
                    t0 + CONVERSATION_TTL + Duration::from_secs(1),
                )
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn conversation_capacity_evicts_least_recently_used() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true)];
        for index in 0..=MAX_CONVERSATIONS {
            let key = format!("k{index}");
            runtime
                .select_account(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some(&key),
                    &[],
                )
                .unwrap();
        }
        let now = Instant::now();
        assert!(runtime.sticky_binding_at("k0", now).is_none());
        assert!(
            runtime
                .sticky_binding_at(&format!("k{MAX_CONVERSATIONS}"), now)
                .is_some()
        );
    }

    #[test]
    fn conversation_hit_refreshes_lru_order_before_capacity_eviction() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true)];
        for index in 0..MAX_CONVERSATIONS {
            let key = format!("k{index}");
            runtime
                .select_account(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some(&key),
                    &[],
                )
                .unwrap();
        }
        runtime
            .select_account(
                &accounts,
                RoutingMode::StrictPriority,
                true,
                Some("k0"),
                &[],
            )
            .unwrap();
        runtime
            .select_account(
                &accounts,
                RoutingMode::StrictPriority,
                true,
                Some("new"),
                &[],
            )
            .unwrap();

        let now = Instant::now();
        assert!(runtime.sticky_binding_at("k0", now).is_some());
        assert!(runtime.sticky_binding_at("k1", now).is_none());
        assert!(runtime.sticky_binding_at("new", now).is_some());
    }

    #[test]
    fn reset_clears_runtime_state() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        runtime
            .select_account(&accounts, RoutingMode::RoundRobin, true, Some("c1"), &[])
            .unwrap();
        runtime.reset();
        assert!(runtime.sticky_binding("c1").is_none());
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, false, None, &[])
                .unwrap()
                .id,
            "a"
        );
    }

    #[test]
    fn disabling_conversation_sticky_ignores_existing_bindings() {
        let runtime = RoutingRuntime::new();
        let accounts = vec![account("a", true), account("b", true)];
        assert_eq!(
            runtime
                .select_account(&accounts, RoutingMode::RoundRobin, true, Some("bound"), &[],)
                .unwrap()
                .id,
            "a"
        );
        assert_eq!(
            runtime
                .select_account(
                    &accounts,
                    RoutingMode::RoundRobin,
                    false,
                    Some("bound"),
                    &[],
                )
                .unwrap()
                .id,
            "b"
        );
    }

    #[test]
    fn compatibility_select_helpers_still_sample_system_time() {
        let production = include_str!("routing_runtime.rs")
            .split("mod tests {")
            .next()
            .expect("production source precedes tests");
        assert!(production.contains("Utc::now()"));
        assert!(production.contains("Instant::now()"));
        assert!(production.contains("fn select_candidate("));
        assert!(production.contains("fn select_candidate_at("));
        assert!(production.contains("fn try_select_candidate_index_at("));
        assert!(production.contains("fn sticky_binding("));
        assert!(production.contains("fn sticky_binding_at("));
        assert!(production.contains("ocg_gateway::selector::SelectorState"));
        assert!(production.contains("fn selection_policy("));
        assert!(!production.contains("struct ConversationBinding"));
        assert!(!production.contains("struct ConversationMap"));
        assert!(!production.contains("struct RoutingRuntimeState"));
        assert!(
            !production.contains("crate::gateway_clock"),
            "routing_runtime must take explicit time arguments instead of owning GatewayClock"
        );
    }

    #[test]
    fn selection_cooldown_uses_injected_wall() {
        let runtime = RoutingRuntime::new();
        let wall = frozen_wall();
        let until = wall + chrono::Duration::hours(1);
        let accounts = vec![cooling_at("a", until), account("b", true)];
        let mono = Instant::now();
        assert_eq!(
            runtime
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    false,
                    None,
                    &[],
                    wall,
                    mono,
                )
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            runtime
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    false,
                    None,
                    &[],
                    until + chrono::Duration::seconds(1),
                    mono,
                )
                .unwrap()
                .id,
            "a"
        );
        assert_eq!(
            runtime
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    false,
                    None,
                    &[],
                    until,
                    mono,
                )
                .unwrap()
                .id,
            "a",
            "until == now must treat the cooled candidate as available"
        );
        let still_cooling = vec![cooling_at("a", until), account("b", true)];
        assert_eq!(
            runtime
                .select_account_at(
                    &still_cooling,
                    RoutingMode::StrictPriority,
                    false,
                    None,
                    &[],
                    until - chrono::Duration::seconds(1),
                    mono,
                )
                .unwrap()
                .id,
            "b",
            "until > now must keep the candidate cooling"
        );
    }

    #[test]
    fn conversation_ttl_uses_injected_mono_and_not_wall() {
        let runtime = RoutingRuntime::new();
        let wall = frozen_wall();
        let far_wall = wall + chrono::Duration::hours(24);
        let t0 = Instant::now();
        let accounts = vec![account("a", true), account("b", true)];
        let key = "ttl-mono";
        assert_eq!(
            runtime
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some(key),
                    &["a"],
                    wall,
                    t0,
                )
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            runtime
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some(key),
                    &[],
                    far_wall,
                    t0 + Duration::from_secs(60),
                )
                .unwrap()
                .id,
            "b",
            "a large wall jump must not expire conversation TTL"
        );
        assert_eq!(
            runtime
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some(key),
                    &[],
                    far_wall,
                    t0 + Duration::from_secs(60) + CONVERSATION_TTL + Duration::from_secs(1),
                )
                .unwrap()
                .id,
            "a",
            "conversation TTL must expire from injected mono"
        );
    }

    #[test]
    fn selection_policy_maps_every_routing_mode() {
        assert_eq!(
            selection_policy(RoutingMode::StrictPriority),
            SelectionPolicy::StrictPriority
        );
        assert_eq!(
            selection_policy(RoutingMode::StickyGlobal),
            SelectionPolicy::StickyGlobal
        );
        assert_eq!(
            selection_policy(RoutingMode::RoundRobin),
            SelectionPolicy::RoundRobin
        );
    }

    #[test]
    fn typed_index_follows_card_order_for_all_three_modes() {
        let runtime = RoutingRuntime::new();
        let wall = frozen_wall();
        let mono = Instant::now();
        let candidates = vec![
            go_candidate(account("a", false)),
            go_candidate(account("b", true)),
            go_candidate(account("c", true)),
        ];

        assert_eq!(
            pick_index(
                &runtime,
                &candidates,
                RoutingMode::StrictPriority,
                false,
                None,
                &[],
                true,
                wall,
                mono,
            ),
            Some(1)
        );

        let runtime = RoutingRuntime::new();
        assert_eq!(
            pick_index(
                &runtime,
                &candidates,
                RoutingMode::StickyGlobal,
                false,
                None,
                &[],
                true,
                wall,
                mono,
            ),
            Some(1)
        );
        assert_eq!(
            pick_index(
                &runtime,
                &candidates,
                RoutingMode::StickyGlobal,
                false,
                None,
                &[],
                true,
                wall,
                mono,
            ),
            Some(1)
        );

        let runtime = RoutingRuntime::new();
        assert_eq!(
            pick_index(
                &runtime,
                &candidates,
                RoutingMode::RoundRobin,
                false,
                None,
                &[],
                true,
                wall,
                mono,
            ),
            Some(1)
        );
        assert_eq!(
            pick_index(
                &runtime,
                &candidates,
                RoutingMode::RoundRobin,
                false,
                None,
                &[],
                true,
                wall,
                mono,
            ),
            Some(2)
        );
        assert_eq!(
            pick_index(
                &runtime,
                &candidates,
                RoutingMode::RoundRobin,
                false,
                None,
                &[],
                true,
                wall,
                mono,
            ),
            Some(1)
        );
    }

    #[test]
    fn typed_transient_excludes_do_not_rewrite_sticky_global() {
        let runtime = RoutingRuntime::new();
        let wall = frozen_wall();
        let mono = Instant::now();
        let candidates = vec![
            go_candidate(account("a", true)),
            go_candidate(account("b", true)),
        ];
        assert_eq!(
            pick_index(
                &runtime,
                &candidates,
                RoutingMode::StickyGlobal,
                false,
                None,
                &[],
                true,
                wall,
                mono,
            ),
            Some(0)
        );
        assert_eq!(
            pick_index(
                &runtime,
                &candidates,
                RoutingMode::StickyGlobal,
                false,
                None,
                &["a"],
                true,
                wall,
                mono,
            ),
            Some(1)
        );
        assert_eq!(
            pick_index(
                &runtime,
                &candidates,
                RoutingMode::StickyGlobal,
                false,
                None,
                &[],
                true,
                wall,
                mono,
            ),
            Some(0)
        );
    }

    #[test]
    fn typed_duplicate_ids_error_before_state_mutation() {
        let runtime = RoutingRuntime::new();
        let wall = frozen_wall();
        let mono = Instant::now();
        let unique = vec![
            go_candidate(account("a", true)),
            go_candidate(account("b", true)),
        ];
        assert_eq!(
            pick_index(
                &runtime,
                &unique,
                RoutingMode::StickyGlobal,
                true,
                Some("dup-conv"),
                &[],
                true,
                wall,
                mono,
            ),
            Some(0)
        );
        let duplicates = vec![
            go_candidate(account("a", true)),
            go_candidate(account("a", true)),
        ];
        let error = runtime
            .try_select_candidate_index_at(
                &duplicates,
                RoutingMode::StickyGlobal,
                true,
                Some("dup-conv"),
                &[],
                true,
                wall,
                mono,
            )
            .expect_err("duplicate account ids must be a typed error");
        assert_eq!(
            error,
            SelectionError::DuplicateAccountId {
                first: 0,
                duplicate: 1
            }
        );
        assert_eq!(
            pick_index(
                &runtime,
                &unique,
                RoutingMode::StickyGlobal,
                true,
                Some("dup-conv"),
                &[],
                true,
                wall,
                mono,
            ),
            Some(0),
            "duplicate rejection must not rewrite sticky or conversation state"
        );
        assert_eq!(
            runtime
                .sticky_binding_at("dup-conv", mono)
                .map(|(id, _, _)| id)
                .as_deref(),
            Some("a")
        );
    }

    #[test]
    fn legacy_option_wrappers_fail_closed_on_duplicates_and_preserve_state() {
        let runtime = RoutingRuntime::new();
        let wall = frozen_wall();
        let mono = Instant::now();
        let unique = vec![
            go_candidate(account("a", true)),
            go_candidate(account("b", true)),
        ];
        assert_eq!(
            runtime
                .select_candidate_at(
                    &unique,
                    RoutingMode::RoundRobin,
                    false,
                    None,
                    &[],
                    wall,
                    mono,
                )
                .unwrap()
                .account
                .id,
            "a"
        );
        let duplicates = vec![
            go_candidate(account("b", true)),
            go_candidate(account("b", true)),
        ];
        assert!(
            runtime
                .select_candidate_at(
                    &duplicates,
                    RoutingMode::RoundRobin,
                    false,
                    None,
                    &[],
                    wall,
                    mono,
                )
                .is_none()
        );
        assert_eq!(
            runtime
                .select_candidate_at(
                    &unique,
                    RoutingMode::RoundRobin,
                    false,
                    None,
                    &[],
                    wall,
                    mono,
                )
                .unwrap()
                .account
                .id,
            "b",
            "legacy fail-closed must leave the round-robin cursor on the last successful pick"
        );
    }

    #[test]
    fn free_channel_gate_closes_only_free_candidates() {
        let runtime = RoutingRuntime::new();
        let wall = frozen_wall();
        let mono = Instant::now();
        let mixed = vec![
            routing_candidate(zen_account(true), UpstreamChannel::Free, "m-free"),
            go_candidate(account("go", true)),
        ];
        assert_eq!(
            pick_index(
                &runtime,
                &mixed,
                RoutingMode::StrictPriority,
                false,
                None,
                &[],
                false,
                wall,
                mono,
            ),
            Some(1)
        );
        assert_eq!(
            pick_index(
                &runtime,
                &mixed,
                RoutingMode::StrictPriority,
                false,
                None,
                &[],
                true,
                wall,
                mono,
            ),
            Some(0)
        );

        let only_free = vec![routing_candidate(
            zen_account(true),
            UpstreamChannel::Free,
            "m-free",
        )];
        assert!(
            pick_index(
                &runtime,
                &only_free,
                RoutingMode::StrictPriority,
                false,
                None,
                &[],
                false,
                wall,
                mono,
            )
            .is_none()
        );
    }

    #[test]
    fn conversation_ttl_expires_at_inclusive_boundary() {
        let wall = frozen_wall();
        let t0 = Instant::now();
        let accounts = vec![account("a", true), account("b", true)];

        let still = RoutingRuntime::new();
        assert_eq!(
            still
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some("old"),
                    &["a"],
                    wall,
                    t0,
                )
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            still
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some("old"),
                    &[],
                    wall,
                    t0 + CONVERSATION_TTL - Duration::from_secs(1),
                )
                .unwrap()
                .id,
            "b"
        );

        let expired = RoutingRuntime::new();
        assert_eq!(
            expired
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some("old"),
                    &["a"],
                    wall,
                    t0,
                )
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            expired
                .select_account_at(
                    &accounts,
                    RoutingMode::StrictPriority,
                    true,
                    Some("old"),
                    &[],
                    wall,
                    t0 + CONVERSATION_TTL,
                )
                .unwrap()
                .id,
            "a",
            "duration_since == CONVERSATION_TTL must expire the binding"
        );
    }

    #[test]
    fn conversation_sticky_requires_account_channel_and_resolved_model() {
        let runtime = RoutingRuntime::new();
        let wall = frozen_wall();
        let t0 = Instant::now();
        let first = vec![
            routing_candidate(zen_account(true), UpstreamChannel::Free, "m1"),
            go_candidate(account("b", true)),
        ];
        assert_eq!(
            pick_index(
                &runtime,
                &first,
                RoutingMode::StrictPriority,
                true,
                Some("conv"),
                &[],
                true,
                wall,
                t0,
            ),
            Some(0)
        );

        let wrong_model = vec![
            routing_candidate(zen_account(true), UpstreamChannel::Free, "m2"),
            go_candidate(account("b", true)),
        ];
        assert_eq!(
            pick_index(
                &runtime,
                &wrong_model,
                RoutingMode::StrictPriority,
                true,
                Some("conv"),
                &[],
                true,
                wall,
                t0,
            ),
            Some(0)
        );
        assert_eq!(
            runtime
                .sticky_binding_at("conv", t0)
                .map(|(_, _, model)| model)
                .as_deref(),
            Some("m2")
        );

        let missing_triple = vec![go_candidate(account("b", true))];
        assert_eq!(
            pick_index(
                &runtime,
                &missing_triple,
                RoutingMode::StrictPriority,
                true,
                Some("conv"),
                &[],
                true,
                wall,
                t0,
            ),
            Some(0),
            "a conversation hit requires the bound account, channel, and resolved model"
        );
        assert_eq!(
            runtime.sticky_binding_at("conv", t0),
            Some((
                "b".to_string(),
                UpstreamChannel::Go,
                "test-model".to_string()
            ))
        );
    }
}
