//! The hourly Launch Library 2 request budget.
//!
//! The free tier allows 15 requests per hour per IP address. Every attempt is recorded
//! before its request is sent, whatever its outcome, and the provider stores the record on
//! disk (see `cache.rs`), so a restarted process still counts the last hour's attempts. An
//! HTTP 429 answer starts a cooldown of 60 s that doubles with each further 429, up to an
//! hour.

use std::collections::VecDeque;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

/// The budget window.
const WINDOW: SignedDuration = SignedDuration::from_hours(1);
const FIRST_COOLDOWN_S: i64 = 60;
const MAX_COOLDOWN: SignedDuration = SignedDuration::from_hours(1);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Budget {
    /// When each request of the last hour was sent, oldest first.
    attempts: VecDeque<Timestamp>,
    /// No request is sent before this time after an HTTP 429.
    cooldown_until: Option<Timestamp>,
    consecutive_429s: u32,
}

impl Budget {
    /// Why no request may be sent at `now`, or `None` when one may.
    pub(super) fn refusal(&mut self, now: Timestamp, max_per_hour: u32) -> Option<String> {
        self.forget_expired(now);
        if let Some(until) = self.cooldown_until.filter(|until| *until > now) {
            let remaining = until.duration_since(now).as_secs();
            return Some(format!("Launch Library 2 429 cooldown active ({remaining} s remaining)"));
        }
        let limit = usize::try_from(max_per_hour).unwrap_or(usize::MAX);
        if self.attempts.len() < limit {
            return None;
        }
        // A slot frees when enough of the oldest attempts leave the window.
        let freeing = self.attempts.get(self.attempts.len() - limit).copied().unwrap_or(now);
        let wait = freeing.checked_add(WINDOW).map_or(WINDOW, |t| t.duration_since(now));
        Some(format!(
            "Launch Library 2 hourly budget reached (limit {max_per_hour} per hour); next request allowed in {} s",
            wait.as_secs().max(1)
        ))
    }

    /// Counts a request that is about to be sent.
    pub(super) fn record_attempt(&mut self, now: Timestamp) {
        self.attempts.push_back(now);
    }

    /// Starts or lengthens the cooldown after an HTTP 429 and returns its length.
    pub(super) fn record_throttled(&mut self, now: Timestamp) -> SignedDuration {
        self.consecutive_429s = self.consecutive_429s.saturating_add(1);
        let doublings = self.consecutive_429s.saturating_sub(1).min(6);
        let cooldown = SignedDuration::from_secs(FIRST_COOLDOWN_S << doublings).min(MAX_COOLDOWN);
        self.cooldown_until = now.checked_add(cooldown).ok();
        cooldown
    }

    /// Ends any cooldown after a successful answer.
    pub(super) fn record_success(&mut self) {
        self.consecutive_429s = 0;
        self.cooldown_until = None;
    }

    /// Adds the attempts and cooldown recorded by an earlier process.
    pub(super) fn absorb(&mut self, earlier: Budget) {
        self.attempts.extend(earlier.attempts);
        self.attempts.make_contiguous().sort_unstable();
        self.cooldown_until = self.cooldown_until.max(earlier.cooldown_until);
        self.consecutive_429s = self.consecutive_429s.max(earlier.consecutive_429s);
    }

    /// Drops attempts that left the window. Records more than a window in the future
    /// (the clock was set back) are dropped too, as is a cooldown longer than the
    /// longest one this budget sets, so a clock change cannot block requests indefinitely.
    fn forget_expired(&mut self, now: Timestamp) {
        self.attempts.retain(|sent| now.duration_since(*sent).abs() < WINDOW);
        if self.cooldown_until.is_some_and(|until| until.duration_since(now) > MAX_COOLDOWN) {
            self.cooldown_until = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> Timestamp {
        let start: Timestamp = "2026-10-15T12:00:00Z".parse().unwrap();
        start.checked_add(SignedDuration::from_mins(minutes)).unwrap()
    }

    #[test]
    fn allows_the_configured_number_per_sliding_hour() {
        let mut budget = Budget::default();
        for minute in [0, 10, 20] {
            assert_eq!(budget.refusal(at(minute), 3), None);
            budget.record_attempt(at(minute));
        }
        let refused = budget.refusal(at(30), 3).unwrap();
        assert!(refused.contains("limit 3 per hour"), "{refused}");
        assert!(refused.contains("in 1800 s"), "{refused}");
        assert_eq!(budget.refusal(at(60), 3), None, "the first attempt left the window");
    }

    #[test]
    fn a_lowered_limit_waits_for_enough_attempts_to_expire() {
        let mut budget = Budget::default();
        for minute in [0, 10, 20] {
            budget.record_attempt(at(minute));
        }
        let refused = budget.refusal(at(30), 1).unwrap();
        assert!(refused.contains("in 3000 s"), "{refused}");
    }

    #[test]
    fn cooldown_doubles_per_429_and_ends_on_success() {
        let mut budget = Budget::default();
        assert_eq!(budget.record_throttled(at(0)), SignedDuration::from_secs(60));
        assert_eq!(budget.record_throttled(at(0)), SignedDuration::from_secs(120));
        assert!(budget.refusal(at(1), 15).unwrap().contains("cooldown"));
        assert_eq!(budget.refusal(at(3), 15), None);
        for _ in 0..10 {
            budget.record_throttled(at(0));
        }
        assert_eq!(budget.record_throttled(at(0)), MAX_COOLDOWN);
        budget.record_success();
        assert_eq!(budget.refusal(at(1), 15), None);
    }

    #[test]
    fn records_from_a_clock_set_far_back_do_not_block_forever() {
        let mut budget = Budget::default();
        budget.record_attempt(at(600));
        budget.cooldown_until = Some(at(600));
        assert_eq!(budget.refusal(at(0), 1), None);
    }

    #[test]
    fn absorbing_an_earlier_record_keeps_every_attempt_and_the_longer_cooldown() {
        let mut earlier = Budget::default();
        earlier.record_attempt(at(0));
        earlier.record_throttled(at(0));
        let mut current = Budget::default();
        current.record_attempt(at(5));
        current.absorb(earlier);
        assert_eq!(current.attempts, [at(0), at(5)]);
        assert_eq!(current.cooldown_until, Some(at(1)));
        assert_eq!(current.consecutive_429s, 1);
    }
}
