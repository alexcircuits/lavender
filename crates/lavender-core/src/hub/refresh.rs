//! Fetches one region on its provider cadence and retries failures without losing cached data.

use super::{HubDeps, MAX_BACKOFF, MAX_PROVIDER_NOTICES, SlotState};
use crate::aircraft_history::AircraftHistory;
use crate::providers::{FeedProvider, FetchContext, Fetched, MAX_ERROR_CHARS, Region};
use futures::FutureExt;
use jiff::Timestamp;
use lavender_domain::FeedItems;
use lavender_domain::text::clean;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Runs a slot's refresh loop and restarts it, with backoff, if the provider panics. The
/// panic is reported as a failure of the slot, so subscribers see an error state instead
/// of a slot that silently stopped.
pub(super) async fn supervise_refresh(
    provider: Arc<dyn FeedProvider>,
    region: Region,
    tx: watch::Sender<Arc<SlotState>>,
    cancel: CancellationToken,
    deps: HubDeps,
    aircraft_history: Arc<AircraftHistory>,
) {
    let source = provider.source_id();
    let mut crashes: u32 = 0;
    loop {
        let run = AssertUnwindSafe(refresh_region(
            provider.as_ref(),
            region,
            &tx,
            &cancel,
            &deps,
            &aircraft_history,
        ))
        .catch_unwind()
        .await;
        if run.is_ok() || cancel.is_cancelled() {
            return;
        }
        crashes = crashes.saturating_add(1);
        let message = format!("The {source} adapter failed unexpectedly; retrying.");
        tracing::error!(source, ?region, crashes, "provider refresh panicked; restarting it");
        deps.health.record_failure(source, &message);
        let next = SlotState { last_error: Some(message), ..(**tx.borrow()).clone() };
        tx.send_replace(Arc::new(next));
        let delay = retry_delay(crashes, provider.retry_floor(), provider.cadence());
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = cancel.cancelled() => return,
        }
    }
}

/// One slot's refresh loop: fetch, publish, wait for the cadence (or back off), repeat.
/// Returns only when cancelled.
async fn refresh_region(
    provider: &dyn FeedProvider,
    region: Region,
    tx: &watch::Sender<Arc<SlotState>>,
    cancel: &CancellationToken,
    deps: &HubDeps,
    aircraft_history: &AircraftHistory,
) {
    let source = provider.source_id();
    let cadence = provider.cadence();
    let floor = provider.retry_floor();
    let mut failures: u32 = 0;
    loop {
        let ctx = FetchContext {
            fetcher: &deps.fetcher,
            secrets: deps.secrets.as_ref(),
            cancel: cancel.child_token(),
            cache_dir: deps.cache_dir.as_deref(),
        };
        let started = Instant::now();
        let result = tokio::select! {
            r = provider.fetch(&ctx, region) => r,
            () = cancel.cancelled() => return,
        };
        let next = match result {
            Ok(fetched) => {
                // Receipt time may describe transport freshness, but it cannot establish
                // when an aircraft position was observed upstream.
                let observation_base = fetched.as_of;
                let state = provider_answer_to_state(fetched);
                if let (Some(items), Some(as_of)) = (&state.items, observation_base)
                    && let FeedItems::Aircraft(aircraft) = items.as_ref()
                {
                    aircraft_history.capture(aircraft, as_of);
                }
                if let Some(reason) = &state.last_error {
                    failures = failures.saturating_add(1);
                    tracing::warn!(source, ?region, error = %reason, "provider answered only partly");
                    deps.health.record_degraded(source, started.elapsed(), reason);
                } else {
                    failures = 0;
                    deps.health.record_success(source, started.elapsed());
                }
                state
            }
            Err(err) => {
                failures = failures.saturating_add(1);
                let message = err.public_message();
                tracing::warn!(source, ?region, failures, error = %message, "provider fetch failed");
                deps.health.record_failure(source, &message);
                SlotState { last_error: Some(message), ..(**tx.borrow()).clone() }
            }
        };
        tx.send_replace(Arc::new(next));
        let delay = if failures == 0 {
            healthy_refresh_delay(cadence, floor)
        } else {
            retry_delay(failures, floor, cadence)
        };
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = cancel.cancelled() => return,
        }
    }
}

/// Slot state for a provider's answer: its real age (never in the future), and its
/// degradation reason and notices cleaned and length-capped.
fn provider_answer_to_state(fetched: Fetched) -> SlotState {
    let now = Timestamp::now();
    SlotState {
        items: Some(Arc::new(fetched.items)),
        as_of: Some(fetched.as_of.map_or(now, |t| t.min(now))),
        last_error: fetched.degraded.map(|r| clean(&r, MAX_ERROR_CHARS)),
        notices: fetched
            .notices
            .iter()
            .map(|n| clean(n, MAX_ERROR_CHARS))
            .filter(|n| !n.is_empty())
            .take(MAX_PROVIDER_NOTICES)
            .collect(),
    }
}

/// The wait after `failures` consecutive failures: 5 s, 10 s, 20 s … capped at the cadence
/// and five minutes, and never below the provider's retry floor.
pub(super) fn retry_delay(failures: u32, floor: Duration, cadence: Duration) -> Duration {
    let exponent = failures.clamp(1, 7) - 1;
    Duration::from_secs(5u64 << exponent).min(cadence).min(MAX_BACKOFF).max(floor)
}

/// ±10 % jitter so refresh tasks for many regions do not align.
fn jittered(base: Duration) -> Duration {
    let ms = u64::try_from(base.as_millis()).unwrap_or(u64::MAX);
    let spread = ms / 10;
    Duration::from_millis(ms.saturating_sub(spread) + fastrand::u64(0..=spread.saturating_mul(2)))
}

fn healthy_refresh_delay(cadence: Duration, floor: Duration) -> Duration {
    // A source's minimum spacing also applies after success: an early jitter tick
    // must not consume a whole cycle merely by returning a still-cooling cache.
    jittered(cadence).max(floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_refresh_respects_source_floor_and_preserves_zero_floor_jitter() {
        let cadence = Duration::from_secs(600);
        for _ in 0..100 {
            let cyclone = healthy_refresh_delay(cadence, cadence);
            assert!((Duration::from_secs(600)..=Duration::from_mins(11)).contains(&cyclone));
            let ordinary = healthy_refresh_delay(Duration::from_secs(500), Duration::ZERO);
            assert!((Duration::from_secs(450)..=Duration::from_secs(550)).contains(&ordinary));
        }
    }
}
