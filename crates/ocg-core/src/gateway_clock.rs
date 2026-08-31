//! Host-owned injectable dual clock for Gateway selection.
//!
//! Wall time (`DateTime<Utc>`) drives durable cooldown expiry, Free-channel
//! exhaustion, candidate availability, and soonest-reset responses.
//! Monotonic time (`Instant`) drives conversation/sticky TTL only.
//!
//! This runtime is distinct from `UsageSyncRuntime`'s calendar clock. It must
//! not enter request snapshots, protocol wire timestamps, pricing estimates,
//! logs, Tokio timeout enforcement, or listener/browser lifecycles.
//! Production uses `Utc::now` / `Instant::now`. Sources are immutable after
//! construction; tests inject closures at `CoreState` construction rather
//! than mutating a live clock.

use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Instant;

type WallFn = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;
type MonoFn = Arc<dyn Fn() -> Instant + Send + Sync>;

enum WallSource {
    System,
    Injected(WallFn),
}

enum MonoSource {
    System,
    Injected(MonoFn),
}

/// Process-wide Gateway decision clock. Production construction always uses
/// system clocks. Test sources, when present, are fixed at construction.
pub(crate) struct GatewayClock {
    wall: WallSource,
    mono: MonoSource,
}

impl Default for GatewayClock {
    fn default() -> Self {
        Self::system()
    }
}

impl GatewayClock {
    pub(crate) fn system() -> Self {
        Self {
            wall: WallSource::System,
            mono: MonoSource::System,
        }
    }

    pub(crate) fn from_sources(
        wall: impl Fn() -> DateTime<Utc> + Send + Sync + 'static,
        mono: impl Fn() -> Instant + Send + Sync + 'static,
    ) -> Self {
        Self {
            wall: WallSource::Injected(Arc::new(wall)),
            mono: MonoSource::Injected(Arc::new(mono)),
        }
    }

    pub(crate) fn now_wall(&self) -> DateTime<Utc> {
        match &self.wall {
            WallSource::System => Utc::now(),
            WallSource::Injected(clock) => clock(),
        }
    }

    pub(crate) fn now_mono(&self) -> Instant {
        match &self.mono {
            MonoSource::System => Instant::now(),
            MonoSource::Injected(clock) => clock(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn frozen_wall() -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(
            chrono::NaiveDate::from_ymd_opt(2024, 1, 2)
                .unwrap()
                .and_hms_opt(3, 4, 5)
                .unwrap(),
            Utc,
        )
    }

    #[test]
    fn production_default_uses_system_wall_and_mono() {
        let before_wall = Utc::now();
        let before_mono = Instant::now();
        let clock = GatewayClock::system();
        let wall = clock.now_wall();
        let mono = clock.now_mono();
        let after_wall = Utc::now();
        let after_mono = Instant::now();
        assert!(wall >= before_wall - ChronoDuration::seconds(1));
        assert!(wall <= after_wall + ChronoDuration::seconds(1));
        assert!(mono >= before_mono);
        assert!(mono <= after_mono);
    }

    #[test]
    fn wall_and_mono_sources_are_independent_and_immutable() {
        let wall = frozen_wall();
        let mono = Instant::now() - Duration::from_secs(3_600);
        let clock = GatewayClock::from_sources(move || wall, move || mono);
        assert_eq!(clock.now_wall(), wall);
        assert_eq!(clock.now_mono(), mono);

        let wall2 = wall + ChronoDuration::days(1);
        let clock2 = GatewayClock::from_sources(move || wall2, move || mono);
        assert_eq!(clock2.now_wall(), wall2);
        assert_eq!(clock2.now_mono(), mono);
        assert_eq!(clock.now_wall(), wall);
        assert_eq!(clock.now_mono(), mono);
    }

    #[test]
    fn injected_callbacks_are_not_invoked_under_a_clock_lock() {
        let slot: Arc<OnceLock<GatewayClock>> = Arc::new(OnceLock::new());
        let wall = frozen_wall();
        let mono = Instant::now();
        let reentered = Arc::new(AtomicUsize::new(0));
        let clock = GatewayClock::from_sources(
            {
                let slot = slot.clone();
                let reentered = reentered.clone();
                move || {
                    if reentered.fetch_add(1, Ordering::SeqCst) == 0
                        && let Some(clock) = slot.get()
                    {
                        assert_eq!(clock.now_wall(), wall);
                        assert_eq!(clock.now_mono(), mono);
                    }
                    wall
                }
            },
            move || mono,
        );
        assert!(slot.set(clock).is_ok(), "clock slot is set once");
        let clock = slot.get().expect("clock slot is populated");
        assert_eq!(clock.now_wall(), wall);
        assert_eq!(clock.now_mono(), mono);
        assert!(
            reentered.load(Ordering::SeqCst) >= 2,
            "wall source must re-enter now_wall without deadlocking"
        );
    }
}
