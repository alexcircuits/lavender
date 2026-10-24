//! Outbound rate pacing for routing requests.
//!
//! Community OSRM servers publish a strict rate limit of at most 1 request per
//! second across all clients. The [`RoutePacer`] gates outbound upstream requests
//! so that calls leave at least 1,000 ms apart, while queuing at most 8 pending
//! requests. Any request that arrives when the queue is saturated is rejected
//! with an honest 429 (`RouteError::Busy`).

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Minimum gap between two outbound requests.
pub const ROUTE_UPSTREAM_MIN_INTERVAL: Duration = Duration::from_millis(1000);

/// Maximum number of requests allowed to wait in the queue before returning 429.
pub const ROUTE_UPSTREAM_QUEUE_MAX: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("invalid profile")]
    InvalidProfile,
    #[error("need 2-12 coordinates")]
    InvalidCoordinateCount,
    #[error("invalid coordinate")]
    InvalidCoordinate,
    #[error("route leg too long")]
    LegTooLong,
    #[error("route too long")]
    RouteTooLong,
    #[error("no route found")]
    NotFound,
    #[error("routing busy — too many routes at once")]
    Busy { retry_after_s: u64 },
    #[error("routing service is rate limited")]
    UpstreamRateLimited { retry_after_s: u64 },
    #[error("route proxy error")]
    ProxyError(String),
}

#[derive(Debug)]
struct PacerState {
    next_allowed_at: Instant,
    queue_depth: usize,
    interval: Duration,
}

/// Outbound request gate ensuring spacing between requests and a bounded queue.
#[derive(Debug)]
pub struct RoutePacer {
    max_queue: usize,
    state: Mutex<PacerState>,
}

impl Default for RoutePacer {
    fn default() -> Self {
        Self::new(ROUTE_UPSTREAM_MIN_INTERVAL, ROUTE_UPSTREAM_QUEUE_MAX)
    }
}

impl RoutePacer {
    #[must_use]
    pub fn new(interval: Duration, max_queue: usize) -> Self {
        Self {
            max_queue,
            state: Mutex::new(PacerState {
                next_allowed_at: Instant::now(),
                queue_depth: 0,
                interval,
            }),
        }
    }

    /// Enter the gate. Returns a [`QueueGuard`] once the rate-limit slot arrives.
    ///
    /// # Errors
    /// Returns [`RouteError::Busy`] if the queue is at capacity.
    pub async fn acquire(&self) -> Result<QueueGuard<'_>, RouteError> {
        let wait = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| RouteError::ProxyError("pacer mutex poisoned".to_owned()))?;

            if state.queue_depth >= self.max_queue {
                let now = Instant::now();
                let wait = state
                    .next_allowed_at
                    .saturating_duration_since(now)
                    .saturating_add(state.interval);
                return Err(RouteError::Busy { retry_after_s: wait.as_secs().max(1) });
            }

            state.queue_depth += 1;
            let now = Instant::now();
            let slot = state.next_allowed_at.max(now);
            state.next_allowed_at = slot + state.interval;
            slot.saturating_duration_since(now)
        };

        if wait > Duration::ZERO {
            tokio::time::sleep(wait).await;
        }

        Ok(QueueGuard(self))
    }

    /// Current queue depth (waiting + currently executing).
    #[must_use]
    pub fn queue_depth(&self) -> usize {
        self.state.lock().map(|s| s.queue_depth).unwrap_or_default()
    }

    /// Test seam: reset the pacer state.
    pub fn reset_for_test(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.next_allowed_at = Instant::now();
            state.queue_depth = 0;
        }
    }

    /// Test seam: configure a custom interval.
    pub fn set_interval_for_test(&self, interval: Duration) {
        if let Ok(mut state) = self.state.lock() {
            state.interval = interval;
        }
    }
}

/// RAII guard ensuring the waiting/active queue slot is released on completion or cancellation.
#[derive(Debug)]
pub struct QueueGuard<'a>(&'a RoutePacer);

impl Drop for QueueGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.queue_depth = state.queue_depth.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pacer_admits_up_to_max_queue_then_rejects() {
        let pacer = RoutePacer::new(Duration::from_millis(50), 3);
        assert_eq!(pacer.queue_depth(), 0);

        let g1 = pacer.acquire().await.unwrap();
        assert_eq!(pacer.queue_depth(), 1);

        let g2 = pacer.acquire().await.unwrap();
        assert_eq!(pacer.queue_depth(), 2);

        let g3 = pacer.acquire().await.unwrap();
        assert_eq!(pacer.queue_depth(), 3);

        // Fourth request exceeds queue capacity 3
        let err = pacer.acquire().await.unwrap_err();
        match err {
            RouteError::Busy { retry_after_s } => {
                assert!(retry_after_s >= 1);
            }
            other => panic!("expected Busy, got {other:?}"),
        }

        // Dropping guard g1 frees up capacity
        drop(g1);
        assert_eq!(pacer.queue_depth(), 2);

        let g4 = pacer.acquire().await.unwrap();
        assert_eq!(pacer.queue_depth(), 3);
        drop(g2);
        drop(g3);
        drop(g4);
        assert_eq!(pacer.queue_depth(), 0);
    }
}
