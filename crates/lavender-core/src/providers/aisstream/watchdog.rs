//! The connection watchdog's policy, free of I/O: how failures are classified and how
//! long to wait before the next attempt.
//!
//! Only transport failures walk the fast exponential ladder. A rejected key cannot be
//! fixed by retrying, and a rate limit must be honoured, so both wait long; this is
//! what keeps the watchdog from hammering AISStream's per-key connection limit.

use std::time::Duration;

/// Wait after a rejected key or a refused endpoint (probe hourly, like the legacy app).
pub const REJECTED_RETRY: Duration = Duration::from_hours(1);
/// Shortest wait after a rate limit, whatever `Retry-After` says.
pub const RATE_LIMIT_FLOOR: Duration = Duration::from_mins(1);
/// Longest wait after repeated rate limits.
pub const RATE_LIMIT_MAX: Duration = Duration::from_mins(30);
/// A session that streamed this long proves the link works: the ladder starts over.
pub const STABLE_AFTER: Duration = Duration::from_mins(1);

/// Why a session ended or could not start. Messages are Lavender's own wording; upstream
/// text is never forwarded (it may echo subscription content).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Network, TLS, WebSocket or upstream end of the session; retrying may help.
    Transport(String),
    /// HTTP 429 on the upgrade, or an upstream error about connection or rate limits.
    RateLimited { retry_after: Option<Duration> },
    /// HTTP 401/403 on the upgrade, or an upstream error about the API key.
    Rejected { status: Option<u16> },
    /// Lavender's egress policy refused the endpoint (a configuration problem).
    Refused(String),
}

impl Failure {
    /// The sentence shown in health and snapshot freshness.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Transport(reason) => format!("AISStream connection failed: {reason}"),
            Self::RateLimited { .. } => {
                "AISStream is limiting connections for this key (too many requests or connections)"
                    .to_owned()
            }
            Self::Rejected { status: Some(status) } => {
                format!("AISStream rejected the API key (HTTP {status})")
            }
            Self::Rejected { status: None } => "AISStream rejected the API key".to_owned(),
            Self::Refused(reason) => format!("The AISStream endpoint is refused: {reason}"),
        }
    }
}

/// Classifies an HTTP status from the WebSocket upgrade.
#[must_use]
pub fn classify_status(status: u16, retry_after: Option<Duration>) -> Failure {
    match status {
        401 | 403 => Failure::Rejected { status: Some(status) },
        420 | 429 => Failure::RateLimited { retry_after },
        _ => Failure::Transport(format!("the WebSocket upgrade was answered with HTTP {status}")),
    }
}

/// Classifies an AISStream error frame or close reason. Limits are checked first:
/// "too many connections for this API key" is a limit, not a bad key.
#[must_use]
pub fn classify_upstream_text(text: &str) -> Failure {
    let text = text.to_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|n| text.contains(n));
    if any(&["too many", "rate limit", "ratelimit", "quota", "concurrent", "limit exceeded", "429"])
    {
        Failure::RateLimited { retry_after: None }
    } else if any(&[
        "api key",
        "apikey",
        "api_key",
        "unauthori",
        "forbidden",
        "invalid key",
        "authenticat",
    ]) {
        Failure::Rejected { status: None }
    } else {
        Failure::Transport("AISStream reported an error".to_owned())
    }
}

/// Exponential backoff with jitter, per failure class.
#[derive(Debug, Clone)]
pub struct Backoff {
    min: Duration,
    max: Duration,
    transport_failures: u32,
    rate_limits: u32,
}

impl Backoff {
    #[must_use]
    pub fn new(min: Duration, max: Duration) -> Self {
        Self { min, max: max.max(min), transport_failures: 0, rate_limits: 0 }
    }

    /// Forgets earlier failures (after a stable session).
    pub fn reset(&mut self) {
        self.transport_failures = 0;
        self.rate_limits = 0;
    }

    /// The wait before the next attempt after `failure`.
    pub fn delay(&mut self, failure: &Failure) -> Duration {
        match failure {
            Failure::Transport(_) => {
                self.transport_failures = self.transport_failures.saturating_add(1);
                // Equal jitter: somewhere in [d/2, d], so reconnecting clients spread out.
                let d = doubled(self.min, self.transport_failures).min(self.max);
                d / 2 + d.mul_f64(fastrand::f64() / 2.0)
            }
            Failure::RateLimited { retry_after } => {
                self.rate_limits = self.rate_limits.saturating_add(1);
                let d = doubled(RATE_LIMIT_FLOOR, self.rate_limits).min(RATE_LIMIT_MAX);
                // Never earlier than the server asked; jitter only adds.
                d.max(retry_after.unwrap_or_default()) + d.mul_f64(fastrand::f64() / 4.0)
            }
            Failure::Rejected { .. } | Failure::Refused(_) => {
                REJECTED_RETRY + REJECTED_RETRY.mul_f64(fastrand::f64() / 10.0)
            }
        }
    }
}

/// `base × 2^(n-1)`, saturating.
fn doubled(base: Duration, n: u32) -> Duration {
    base.saturating_mul(1_u32 << n.saturating_sub(1).min(20))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_statuses_and_upstream_errors_are_classified() {
        assert_eq!(classify_status(401, None), Failure::Rejected { status: Some(401) });
        assert_eq!(classify_status(403, None), Failure::Rejected { status: Some(403) });
        let wait = Some(Duration::from_secs(120));
        assert_eq!(classify_status(429, wait), Failure::RateLimited { retry_after: wait });
        assert!(matches!(classify_status(502, None), Failure::Transport(_)));
        for text in ["Api Key Is Not Valid", "unauthorized", "Forbidden", "authentication failed"] {
            assert_eq!(classify_upstream_text(text), Failure::Rejected { status: None }, "{text}");
        }
        for text in [
            "Too many connections for this API key",
            "rate limit exceeded",
            "concurrent connections per user exceeded",
        ] {
            assert!(matches!(classify_upstream_text(text), Failure::RateLimited { .. }), "{text}");
        }
        let other = classify_upstream_text("secret-key-123 is broken somehow");
        assert_eq!(other, Failure::Transport("AISStream reported an error".to_owned()));
        assert!(!other.describe().contains("secret-key-123"), "upstream text is never echoed");
    }

    #[test]
    fn transport_backoff_grows_with_jitter_and_resets() {
        let mut backoff = Backoff::new(Duration::from_secs(5), Duration::from_secs(300));
        let failure = Failure::Transport("closed".to_owned());
        for expected in [5, 10, 20, 40, 80, 160, 300, 300] {
            let d = backoff.delay(&failure);
            let full = Duration::from_secs(expected);
            assert!(d >= full / 2 && d <= full, "{d:?} for {expected}");
        }
        backoff.reset();
        assert!(backoff.delay(&failure) <= Duration::from_secs(5));
    }

    #[test]
    fn rejected_keys_and_rate_limits_wait_long_and_honour_retry_after() {
        let mut backoff = Backoff::new(Duration::from_secs(5), Duration::from_secs(300));
        let rejected = backoff.delay(&Failure::Rejected { status: Some(401) });
        assert!(rejected >= REJECTED_RETRY && rejected <= REJECTED_RETRY.mul_f64(1.1));
        let limited = backoff.delay(&Failure::RateLimited { retry_after: None });
        assert!(limited >= RATE_LIMIT_FLOOR && limited <= RATE_LIMIT_FLOOR.mul_f64(1.25));
        let asked = Duration::from_secs(600);
        let honoured = backoff.delay(&Failure::RateLimited { retry_after: Some(asked) });
        assert!(honoured >= asked, "{honoured:?}");
        for _ in 0..10 {
            backoff.delay(&Failure::RateLimited { retry_after: None });
        }
        assert!(
            backoff.delay(&Failure::RateLimited { retry_after: None })
                <= RATE_LIMIT_MAX.mul_f64(1.25)
        );
    }
}
