//! The feed hub (ADR 0005).
//!
//! Each `(feed, region)` key that has at least one subscriber gets exactly one refresh
//! task, so upstream load scales with active regions, not with viewers. Results are
//! published through `watch` channels (latest value wins; slow consumers skip ahead).
//! Each subscription merges the slots it covers, filters to its own viewport and filters,
//! judges freshness (again on a timer, so a refresh that stalls turns `Stale`), and emits
//! a [`FeedSnapshot`].
//!
//! Capacity. At most `max_slots` keys exist at once. When the last subscriber of a key
//! leaves, its task lingers so repeated one-shot snapshots and quick pans do not refetch;
//! each slot has at most one linger timer, and a lingering slot is evicted at once when
//! the table is full, so idle slots never block active ones. A [`RegionBudget`] caps the
//! distinct keys one client holds. Keys a subscription cannot get are reported as partial
//! coverage with a notice and retried when capacity frees up.
//!
//! Refresh tasks are supervised: a provider panic is reported as a failure and the task
//! restarts with backoff, so subscribers never stay attached to a dead slot. Dropping the
//! hub without calling `shutdown` stops every task too.

mod refresh;
mod slots;
mod snapshot;
mod subscription;

use crate::aircraft_history::AircraftHistory;
use crate::config::HubConfig;
use crate::error::CoreError;
use crate::fetch::Fetcher;
use crate::health::HealthTracker;
use crate::providers::{FeedProvider, Region};
use crate::secrets::SecretStore;
use jiff::Timestamp;
use lavender_domain::{
    AircraftTrack, ApiError, Coverage, FeedId, FeedItems, FeedQuery, FeedSnapshot,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use subscription::{Subscriber, run_subscription};
use tokio::sync::{Notify, watch};
use tokio_util::sync::{CancellationToken, DropGuard};

/// Longest wait between failed attempts (unless a provider's retry floor is longer).
const MAX_BACKOFF: Duration = Duration::from_secs(300);
/// Longest interval at which a subscription re-judges freshness without new data.
const MAX_RECHECK: Duration = Duration::from_secs(60);
/// Longest wait before a subscription retries regions it could not get.
const MAX_RETRY: Duration = Duration::from_secs(30);
/// Notices per snapshot taken from providers (each is length-capped).
const MAX_PROVIDER_NOTICES: usize = 8;

const CAPACITY_NOTICE: &str = "Lavender is at capacity, so part of this view is not loaded \
     yet. It loads when capacity frees up.";
const BUDGET_NOTICE: &str = "Too many map areas are open from this client, so part of this \
     view is not loaded. Close a view or zoom in to load the rest.";

/// What a subscription delivers.
#[derive(Debug, Clone)]
pub enum SubscriptionEvent {
    Snapshot(Arc<FeedSnapshot>),
    Error(ApiError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SlotKey {
    feed: FeedId,
    region: Region,
}

/// The latest result of one refresh task.
#[derive(Debug, Default, Clone)]
struct SlotState {
    items: Option<Arc<FeedItems>>,
    /// When the data in `items` was obtained upstream. A provider serving a cache reports
    /// the cache's age, so this can be older than the last fetch.
    as_of: Option<Timestamp>,
    /// Why the latest attempt failed or answered only partly; cleared by a clean answer.
    last_error: Option<String>,
    /// Informational sentences from the latest answer (for example skipped records).
    notices: Vec<String>,
}

#[derive(Debug)]
struct Slot {
    /// Distinguishes this slot from earlier ones with the same key.
    id: u64,
    rx: watch::Receiver<Arc<SlotState>>,
    refs: usize,
    /// When `refs` last dropped to zero; `None` while in use.
    idle_since: Option<Instant>,
    /// Whether this slot's linger timer is running (at most one per slot).
    reaper_armed: bool,
    cancel: CancellationToken,
}

/// Dependencies the refresh tasks share.
#[derive(Debug, Clone)]
pub struct HubDeps {
    pub fetcher: Arc<Fetcher>,
    pub secrets: Arc<dyn SecretStore>,
    pub health: Arc<HealthTracker>,
    pub cache_dir: Option<PathBuf>,
    pub satellite_catalog: Arc<crate::satellite_catalog::SatelliteCatalog>,
}

#[derive(Debug)]
struct HubInner {
    providers: HashMap<FeedId, Arc<dyn FeedProvider>>,
    slots: Mutex<HashMap<SlotKey, Slot>>,
    subscribers: HashMap<FeedId, AtomicU32>,
    deps: HubDeps,
    aircraft_history: Arc<AircraftHistory>,
    config: HubConfig,
    shutdown: CancellationToken,
    next_slot_id: AtomicU64,
    /// Woken when capacity may have freed up: a slot went idle or was removed, or a
    /// budget released a region.
    capacity: Notify,
}

impl Drop for HubInner {
    /// A hub dropped without `shutdown` must not leave refresh tasks fetching forever.
    fn drop(&mut self) {
        self.shutdown.cancel();
        for provider in self.providers.values() {
            provider.shutdown();
        }
    }
}

/// Shared handle to the hub.
#[derive(Debug, Clone)]
pub struct FeedHub {
    inner: Arc<HubInner>,
}

/// Caps how many distinct regions one client (for example a network address) holds at
/// once, across all of its subscriptions, so one client cannot fill the hub's slot table
/// and starve everyone else. Regions several of the client's subscriptions share count
/// once.
#[derive(Debug)]
pub struct RegionBudget {
    limit: usize,
    held: Mutex<HashMap<SlotKey, usize>>,
}

/// Why a region could not be leased.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shortfall {
    /// The hub's slot table is full of slots in use.
    Capacity,
    /// The client's [`RegionBudget`] is used up.
    Budget,
}

/// Holds one reference to a slot (and to the client's budget); releasing happens on drop.
#[derive(Debug)]
struct Lease {
    key: SlotKey,
    slot_id: u64,
    rx: watch::Receiver<Arc<SlotState>>,
    hub: Weak<HubInner>,
    budget: Option<Arc<RegionBudget>>,
}

/// The keys a query needs and how much of its area they cover.
#[derive(Debug, Clone, PartialEq)]
struct Plan {
    keys: Vec<SlotKey>,
    coverage: Coverage,
}

/// A live subscription. Dropping it stops its task and releases its slots.
#[derive(Debug)]
pub struct FeedSubscription {
    events: watch::Receiver<Option<SubscriptionEvent>>,
    query: watch::Sender<FeedQuery>,
    feed: FeedId,
    _guard: DropGuard,
}

impl FeedHub {
    #[must_use]
    pub fn new(providers: Vec<Arc<dyn FeedProvider>>, deps: HubDeps, config: HubConfig) -> Self {
        let subscribers = providers.iter().map(|p| (p.feed(), AtomicU32::new(0))).collect();
        let providers = providers.into_iter().map(|p| (p.feed(), p)).collect();
        Self {
            inner: Arc::new(HubInner {
                providers,
                slots: Mutex::new(HashMap::new()),
                subscribers,
                deps,
                aircraft_history: Arc::new(AircraftHistory::default()),
                config,
                shutdown: CancellationToken::new(),
                next_slot_id: AtomicU64::new(0),
                capacity: Notify::new(),
            }),
        }
    }

    #[must_use]
    pub fn provider(&self, feed: FeedId) -> Option<&Arc<dyn FeedProvider>> {
        self.inner.providers.get(&feed)
    }

    /// Starts a subscription. Requires a Tokio runtime.
    ///
    /// # Errors
    /// Returns an error for unknown feeds or after shutdown.
    pub fn subscribe(&self, query: FeedQuery) -> Result<FeedSubscription, CoreError> {
        self.subscribe_with_budget(query, None)
    }

    /// Starts a subscription whose regions count against `budget`. Requires a Tokio
    /// runtime.
    ///
    /// # Errors
    /// Returns an error for unknown feeds or after shutdown.
    pub fn subscribe_with_budget(
        &self,
        query: FeedQuery,
        budget: Option<Arc<RegionBudget>>,
    ) -> Result<FeedSubscription, CoreError> {
        if self.inner.shutdown.is_cancelled() {
            return Err(CoreError::ShuttingDown);
        }
        let feed = query.feed;
        let Some(provider) = self.inner.providers.get(&feed).cloned() else {
            return Err(CoreError::NotFound(format!("no provider serves the {feed} feed")));
        };
        let (events_tx, events_rx) = watch::channel(None);
        let (query_tx, query_rx) = watch::channel(query);
        let cancel = self.inner.shutdown.child_token();
        tokio::spawn(run_subscription(
            Subscriber {
                hub: self.inner.clone(),
                provider,
                budget,
                events: events_tx,
                cancel: cancel.clone(),
            },
            query_rx,
        ));
        Ok(FeedSubscription {
            events: events_rx,
            query: query_tx,
            feed,
            _guard: cancel.drop_guard(),
        })
    }

    /// The items currently held for `feed` across all of its active regions.
    #[must_use]
    pub fn current_items(&self, feed: FeedId) -> Vec<Arc<FeedItems>> {
        let slots = self.inner.lock_slots();
        slots
            .iter()
            .filter(|(k, _)| k.feed == feed)
            .filter_map(|(_, s)| s.rx.borrow().items.clone())
            .collect()
    }

    /// Returns retained aircraft observations without creating a provider request.
    ///
    /// # Errors
    /// Rejects malformed addresses, aircraft with no retained points, and shutdown.
    pub fn aircraft_track(&self, hex: &str) -> Result<AircraftTrack, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        self.inner.aircraft_history.track(hex)
    }

    /// (active regions, subscribers) for `feed`.
    #[must_use]
    pub fn load(&self, feed: FeedId) -> (u32, u32) {
        let regions = self.inner.lock_slots().keys().filter(|k| k.feed == feed).count();
        let subs = self.inner.subscribers.get(&feed).map_or(0, |c| c.load(Ordering::Relaxed));
        (u32::try_from(regions).unwrap_or(u32::MAX), subs)
    }

    /// Cancels every refresh task and subscription, and stops providers' own background
    /// work (persistent upstream connections).
    pub fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.lock_slots().clear();
        self.inner.capacity.notify_waiters();
        for provider in self.inner.providers.values() {
            provider.shutdown();
        }
    }

    #[must_use]
    pub fn is_shut_down(&self) -> bool {
        self.inner.shutdown.is_cancelled()
    }
}

#[cfg(test)]
mod tests;
