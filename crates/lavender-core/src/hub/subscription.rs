//! Follows one viewer's changing query and publishes the latest matching snapshot.

use super::snapshot::{compose_snapshot, freshness_changed};
use super::{
    FeedSubscription, HubInner, Lease, MAX_RECHECK, MAX_RETRY, Plan, RegionBudget, Shortfall,
    SlotKey, SubscriptionEvent,
};
use crate::error::CoreError;
use crate::providers::FeedProvider;
use futures::future::select_all;
use lavender_domain::{ApiError, ErrorCode, FeedId, FeedQuery, Freshness};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

impl FeedSubscription {
    /// Waits for the next event. Returns `None` when the subscription has ended (hub
    /// shutdown).
    pub async fn next(&mut self) -> Option<SubscriptionEvent> {
        self.events.changed().await.ok()?;
        self.events.borrow_and_update().clone()
    }

    /// The most recent event, without waiting.
    #[must_use]
    pub fn latest(&self) -> Option<SubscriptionEvent> {
        self.events.borrow().clone()
    }

    /// Replaces the query, for example with a new viewport.
    ///
    /// # Errors
    /// Returns an error when the query names a different feed.
    pub fn update(&self, query: FeedQuery) -> Result<(), CoreError> {
        if query.feed != self.feed {
            return Err(CoreError::BadRequest("a subscription cannot change its feed".to_owned()));
        }
        self.query.send_replace(query);
        Ok(())
    }

    #[must_use]
    pub fn feed(&self) -> FeedId {
        self.feed
    }
}

struct SubscriberCount<'a>(&'a AtomicU32);

impl Drop for SubscriberCount<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// What a subscription task works with.
pub(super) struct Subscriber {
    pub(super) hub: Arc<HubInner>,
    pub(super) provider: Arc<dyn FeedProvider>,
    pub(super) budget: Option<Arc<RegionBudget>>,
    pub(super) events: watch::Sender<Option<SubscriptionEvent>>,
    pub(super) cancel: CancellationToken,
}

/// Static facts about the subscribed feed.
#[derive(Debug, Clone, Copy)]
pub(super) struct FeedMeta {
    pub(super) cadence: Duration,
    pub(super) source_id: &'static str,
    pub(super) max_items: usize,
}

/// Why a subscription woke up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wake {
    /// Unsubscribed or shut down.
    Stop,
    /// A slot's refresh task ended (only on shutdown).
    SlotClosed,
    Query,
    Slot,
    /// Capacity may have freed up, or the retry timer fired.
    Retry,
    /// Time to re-judge freshness.
    Recheck,
}

impl Subscriber {
    fn send(&self, event: SubscriptionEvent) {
        self.events.send_replace(Some(event));
    }

    /// Leases every key of `plan` not held yet; returns the first shortfall, if any.
    fn acquire_missing(
        &self,
        plan: &Plan,
        leases: &mut HashMap<SlotKey, Lease>,
    ) -> Option<Shortfall> {
        let mut shortfall = None;
        for key in &plan.keys {
            if leases.contains_key(key) {
                continue;
            }
            match self.hub.acquire(*key, &self.provider, self.budget.as_ref()) {
                Ok(lease) => {
                    leases.insert(*key, lease);
                }
                Err(short) => {
                    shortfall.get_or_insert(short);
                }
            }
        }
        shortfall
    }
}

pub(super) async fn run_subscription(sub: Subscriber, mut query_rx: watch::Receiver<FeedQuery>) {
    let feed = sub.provider.feed();
    let counter = sub.hub.subscribers.get(&feed);
    if let Some(c) = counter {
        c.fetch_add(1, Ordering::Relaxed);
    }
    let _count = counter.map(SubscriberCount);
    let meta = FeedMeta {
        cadence: sub.provider.cadence(),
        source_id: sub.provider.source_id(),
        max_items: sub.hub.config.max_items,
    };
    let recheck = (meta.cadence / 2).clamp(Duration::from_secs(1), MAX_RECHECK);
    let retry = meta.cadence.clamp(Duration::from_secs(1), MAX_RETRY);
    let mut leases: HashMap<SlotKey, Lease> = HashMap::new();
    let mut query = query_rx.borrow_and_update().clone();
    let mut plan = sub.hub.plan(&query);
    // The freshness of the last snapshot sent, for the recheck timer.
    let mut sent: Option<Freshness> = None;
    let mut wake = Wake::Query;

    loop {
        match &plan {
            Ok(p) => {
                let held = leases.len();
                let shortfall = sub.acquire_missing(p, &mut leases);
                // A retry that leased nothing new changes nothing worth sending.
                let emit = wake != Wake::Retry || leases.len() != held;
                if emit {
                    if let Some(short) = shortfall.filter(|_| leases.is_empty()) {
                        sent = None;
                        sub.send(SubscriptionEvent::Error(ApiError::from(&short.error())));
                    } else if let Some(snapshot) =
                        compose_snapshot(&query, p, &mut leases, &meta, shortfall)
                    {
                        sent = Some(snapshot.freshness.clone());
                        sub.send(SubscriptionEvent::Snapshot(Arc::new(snapshot)));
                    }
                }
                // Absolute deadline, so rechecks that change nothing do not postpone it.
                let retry_at = shortfall.map(|_| tokio::time::Instant::now() + retry);
                wake = loop {
                    let recheck_at = sent.as_ref().map(|_| tokio::time::Instant::now() + recheck);
                    let next =
                        wait_for_change(&mut leases, &mut query_rx, &sub, retry_at, recheck_at)
                            .await;
                    if next == Wake::Recheck && !freshness_changed(&leases, &meta, sent.as_ref()) {
                        continue;
                    }
                    break next;
                };
            }
            Err(err) => {
                sent = None;
                sub.send(SubscriptionEvent::Error(ApiError::from(err)));
                wake = wait_for_change(&mut leases, &mut query_rx, &sub, None, None).await;
            }
        }
        match wake {
            Wake::Stop => break,
            Wake::SlotClosed => {
                if !sub.cancel.is_cancelled() {
                    sub.send(SubscriptionEvent::Error(ApiError::new(
                        ErrorCode::Internal,
                        "the feed stopped",
                    )));
                }
                break;
            }
            Wake::Query => {
                query = query_rx.borrow_and_update().clone();
                plan = sub.hub.plan(&query);
                match &plan {
                    Ok(p) => {
                        let wanted: HashSet<SlotKey> = p.keys.iter().copied().collect();
                        leases.retain(|k, _| wanted.contains(k));
                    }
                    Err(_) => leases.clear(),
                }
            }
            Wake::Slot | Wake::Retry | Wake::Recheck => {}
        }
    }
}

/// Waits for whatever changes this subscription's answer. `retry_at` is set while some
/// regions are missing (capacity freeing up also wakes it); `recheck_at` while a snapshot
/// has been sent.
async fn wait_for_change(
    leases: &mut HashMap<SlotKey, Lease>,
    query_rx: &mut watch::Receiver<FeedQuery>,
    sub: &Subscriber,
    retry_at: Option<tokio::time::Instant>,
    recheck_at: Option<tokio::time::Instant>,
) -> Wake {
    let slot_changed = async {
        if leases.is_empty() {
            std::future::pending::<bool>().await
        } else {
            let (result, _, _) =
                select_all(leases.values_mut().map(|l| Box::pin(l.rx.changed()))).await;
            result.is_ok()
        }
    };
    let retry_due = async {
        match retry_at {
            Some(at) => tokio::select! {
                () = sub.hub.capacity.notified() => {}
                () = tokio::time::sleep_until(at) => {}
            },
            None => std::future::pending::<()>().await,
        }
    };
    let recheck_due = async {
        match recheck_at {
            Some(at) => tokio::time::sleep_until(at).await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        // Cancellation (unsubscribe or shutdown) wins over everything else.
        biased;
        () = sub.cancel.cancelled() => Wake::Stop,
        r = query_rx.changed() => if r.is_ok() { Wake::Query } else { Wake::Stop },
        alive = slot_changed => if alive { Wake::Slot } else { Wake::SlotClosed },
        () = retry_due => Wake::Retry,
        () = recheck_due => Wake::Recheck,
    }
}
