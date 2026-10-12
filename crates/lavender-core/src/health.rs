//! Per-source health from recent fetch outcomes; feeds `SystemStatus` and the Sources panel.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use jiff::Timestamp;
use lavender_domain::{HealthState, ProviderHealth};

const LATENCY_WINDOW: usize = 20;

#[derive(Debug, Default, Clone)]
struct Entry {
    last_success: Option<Timestamp>,
    last_attempt: Option<Timestamp>,
    last_error: Option<String>,
    consecutive_failures: u32,
    /// The latest attempt answered, but only partly or from an old cache.
    degraded: bool,
    latencies_ms: VecDeque<u32>,
}

impl Entry {
    fn push_latency(&mut self, latency: Duration) {
        if self.latencies_ms.len() == LATENCY_WINDOW {
            self.latencies_ms.pop_front();
        }
        self.latencies_ms.push_back(u32::try_from(latency.as_millis()).unwrap_or(u32::MAX));
    }
}

#[derive(Debug, Default)]
pub struct HealthTracker {
    entries: Mutex<HashMap<String, Entry>>,
}

impl HealthTracker {
    fn with_entry(&self, source: &str, f: impl FnOnce(&mut Entry)) {
        let mut map = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        f(map.entry(source.to_owned()).or_default());
    }

    pub fn record_success(&self, source: &str, latency: Duration) {
        let now = Timestamp::now();
        self.with_entry(source, |e| {
            e.last_success = Some(now);
            e.last_attempt = Some(now);
            e.consecutive_failures = 0;
            e.degraded = false;
            e.push_latency(latency);
        });
    }

    /// The provider answered, but part of the upstream failed or old cached data was
    /// served (`reason` says which). Reported as `Degraded`, never as a success.
    pub fn record_degraded(&self, source: &str, latency: Duration, reason: &str) {
        let now = Timestamp::now();
        self.with_entry(source, |e| {
            e.last_attempt = Some(now);
            e.last_error = Some(reason.to_owned());
            e.consecutive_failures = 0;
            e.degraded = true;
            e.push_latency(latency);
        });
    }

    pub fn record_failure(&self, source: &str, error: &str) {
        let now = Timestamp::now();
        self.with_entry(source, |e| {
            e.last_attempt = Some(now);
            e.last_error = Some(error.to_owned());
            e.consecutive_failures = e.consecutive_failures.saturating_add(1);
            e.degraded = false;
        });
    }

    /// Health for `source`. `enabled = false` reports `Disabled` regardless of history.
    #[must_use]
    pub fn health(&self, source: &str, enabled: bool) -> ProviderHealth {
        let map = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let e = map.get(source).cloned().unwrap_or_default();
        let state = if !enabled {
            HealthState::Disabled
        } else if e.last_attempt.is_none() {
            HealthState::Idle
        } else if e.consecutive_failures == 0 && e.degraded {
            HealthState::Degraded
        } else if e.consecutive_failures == 0 {
            HealthState::Healthy
        } else if e.consecutive_failures < 3 && e.last_success.is_some() {
            HealthState::Degraded
        } else {
            HealthState::Failing
        };
        let latency_ms_p50 = if e.latencies_ms.is_empty() {
            None
        } else {
            let mut v: Vec<u32> = e.latencies_ms.iter().copied().collect();
            v.sort_unstable();
            v.get(v.len() / 2).copied()
        };
        ProviderHealth {
            source_id: source.to_owned(),
            state,
            last_success: e.last_success,
            last_attempt: e.last_attempt,
            last_error: e.last_error,
            consecutive_failures: e.consecutive_failures,
            latency_ms_p50,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_follow_outcomes() {
        let h = HealthTracker::default();
        assert_eq!(h.health("s", true).state, HealthState::Idle);
        h.record_success("s", Duration::from_millis(120));
        assert_eq!(h.health("s", true).state, HealthState::Healthy);
        assert_eq!(h.health("s", true).latency_ms_p50, Some(120));
        h.record_failure("s", "boom");
        assert_eq!(h.health("s", true).state, HealthState::Degraded);
        h.record_failure("s", "boom");
        h.record_failure("s", "boom");
        assert_eq!(h.health("s", true).state, HealthState::Failing);
        assert_eq!(h.health("s", false).state, HealthState::Disabled);
        h.record_success("s", Duration::from_millis(80));
        assert_eq!(h.health("s", true).consecutive_failures, 0);
    }

    #[test]
    fn partial_answers_are_degraded_not_healthy() {
        let h = HealthTracker::default();
        h.record_degraded("s", Duration::from_millis(50), "group weather failed");
        let health = h.health("s", true);
        assert_eq!(health.state, HealthState::Degraded);
        assert_eq!(health.last_success, None, "a partial answer is not a success");
        assert_eq!(health.last_error.as_deref(), Some("group weather failed"));
        h.record_success("s", Duration::from_millis(50));
        assert_eq!(h.health("s", true).state, HealthState::Healthy);
    }

    #[test]
    fn never_succeeded_is_failing() {
        let h = HealthTracker::default();
        h.record_failure("s", "unreachable");
        assert_eq!(h.health("s", true).state, HealthState::Failing);
    }
}
