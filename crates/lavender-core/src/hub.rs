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

use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use futures::FutureExt;
use futures::future::select_all;
use jiff::{SignedDuration, Timestamp};
use lavender_domain::text::clean;
use lavender_domain::{
    ApiError, BBox, Coverage, ErrorCode, FeedId, FeedItems, FeedQuery, FeedSnapshot, Freshness,
    ProviderScope,
};
use lavender_geo::plan_cells;
use tokio::sync::{Notify, watch};
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::config::HubConfig;
use crate::error::CoreError;
use crate::fetch::Fetcher;
use crate::health::HealthTracker;
use crate::providers::{FeedProvider, FetchContext, Fetched, MAX_ERROR_CHARS, Region};
use crate::secrets::SecretStore;

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
}

#[derive(Debug)]
struct HubInner {
    providers: HashMap<FeedId, Arc<dyn FeedProvider>>,
    slots: Mutex<HashMap<SlotKey, Slot>>,
    subscribers: HashMap<FeedId, AtomicU32>,
    deps: HubDeps,
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

impl RegionBudget {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self { limit: limit.max(1), held: Mutex::new(HashMap::new()) }
    }

    #[must_use]
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Distinct regions currently held.
    #[must_use]
    pub fn in_use(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<SlotKey, usize>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Counts one more hold of `key`; `false` when that would exceed the limit.
    fn take(&self, key: SlotKey) -> bool {
        let mut held = self.lock();
        if let Some(n) = held.get_mut(&key) {
            *n += 1;
            return true;
        }
        if held.len() >= self.limit {
            return false;
        }
        held.insert(key, 1);
        true
    }

    /// Releases one hold of `key`; `true` when the key is no longer held at all.
    fn give_back(&self, key: SlotKey) -> bool {
        let mut held = self.lock();
        match held.get_mut(&key) {
            Some(n) if *n > 1 => {
                *n -= 1;
                false
            }
            Some(_) => {
                held.remove(&key);
                true
            }
            None => false,
        }
    }
}

/// Why a region could not be leased.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shortfall {
    /// The hub's slot table is full of slots in use.
    Capacity,
    /// The client's [`RegionBudget`] is used up.
    Budget,
}

impl Shortfall {
    fn notice(self) -> &'static str {
        match self {
            Self::Capacity => CAPACITY_NOTICE,
            Self::Budget => BUDGET_NOTICE,
        }
    }

    fn error(self) -> CoreError {
        CoreError::Limit(
            match self {
                Self::Capacity => "Lavender is at capacity; try again shortly.",
                Self::Budget => "Too many map areas are open from this client; close a view first.",
            }
            .to_owned(),
        )
    }
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

impl Drop for Lease {
    fn drop(&mut self) {
        let budget_freed = self.budget.as_ref().is_some_and(|b| b.give_back(self.key));
        if let Some(hub) = self.hub.upgrade() {
            HubInner::release(&hub, self.key, self.slot_id);
            if budget_freed {
                hub.capacity.notify_waiters();
            }
        }
    }
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

    /// (active regions, subscribers) for `feed`.
    #[must_use]
    pub fn load(&self, feed: FeedId) -> (u32, u32) {
        let regions = self.inner.lock_slots().keys().filter(|k| k.feed == feed).count();
        let subs = self.inner.subscribers.get(&feed).map_or(0, |c| c.load(Ordering::Relaxed));
        (u32::try_from(regions).unwrap_or(u32::MAX), subs)
    }

    /// Cancels every refresh task and subscription.
    pub fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.lock_slots().clear();
        self.inner.capacity.notify_waiters();
    }

    #[must_use]
    pub fn is_shut_down(&self) -> bool {
        self.inner.shutdown.is_cancelled()
    }
}

impl HubInner {
    fn lock_slots(&self) -> std::sync::MutexGuard<'_, HashMap<SlotKey, Slot>> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn plan(&self, query: &FeedQuery) -> Result<Plan, CoreError> {
        let provider = self.providers.get(&query.feed).ok_or_else(|| {
            CoreError::NotFound(format!("no provider serves the {} feed", query.feed))
        })?;
        let feed = query.feed;
        match provider.scope() {
            ProviderScope::Global => Ok(Plan {
                keys: vec![SlotKey { feed, region: Region::Global }],
                coverage: Coverage::Complete,
            }),
            ProviderScope::Regional { cell_deg, max_cells } => {
                let bbox = query.bbox.unwrap_or(BBox::WORLD);
                let plan =
                    plan_cells(&bbox, cell_deg, usize::try_from(max_cells).unwrap_or(usize::MAX));
                if !plan.truncated {
                    let keys = plan
                        .cells
                        .into_iter()
                        .map(|c| SlotKey { feed, region: Region::Cell(c) })
                        .collect();
                    return Ok(Plan { keys, coverage: Coverage::Complete });
                }
                match provider.global_fallback_notice() {
                    Some(notice) => Ok(Plan {
                        keys: vec![SlotKey { feed, region: Region::Global }],
                        coverage: Coverage::Partial { reason: notice.to_owned() },
                    }),
                    None => Ok(Plan {
                        keys: plan
                            .cells
                            .into_iter()
                            .map(|c| SlotKey { feed, region: Region::Cell(c) })
                            .collect(),
                        coverage: Coverage::Partial {
                            reason: "Showing the centre of the view. Zoom in to see everything."
                                .to_owned(),
                        },
                    }),
                }
            }
        }
    }

    /// Leases `key`, counting it against `budget` first.
    fn acquire(
        self: &Arc<Self>,
        key: SlotKey,
        provider: &Arc<dyn FeedProvider>,
        budget: Option<&Arc<RegionBudget>>,
    ) -> Result<Lease, Shortfall> {
        if let Some(budget) = budget
            && !budget.take(key)
        {
            return Err(Shortfall::Budget);
        }
        if let Some((slot_id, rx)) = self.lease_slot(key, provider) {
            return Ok(Lease {
                key,
                slot_id,
                rx,
                hub: Arc::downgrade(self),
                budget: budget.cloned(),
            });
        }
        if let Some(budget) = budget {
            budget.give_back(key);
        }
        Err(Shortfall::Capacity)
    }

    /// Joins the slot for `key`, creating it (and evicting an idle slot when the table is
    /// full) if needed. `None` when every slot is in use.
    fn lease_slot(
        self: &Arc<Self>,
        key: SlotKey,
        provider: &Arc<dyn FeedProvider>,
    ) -> Option<(u64, watch::Receiver<Arc<SlotState>>)> {
        let mut slots = self.lock_slots();
        if let Some(slot) = slots.get_mut(&key) {
            slot.refs += 1;
            slot.idle_since = None;
            return Some((slot.id, slot.rx.clone()));
        }
        if slots.len() >= self.config.max_slots && !evict_idle(&mut slots) {
            return None;
        }
        let (tx, rx) = watch::channel(Arc::new(SlotState::default()));
        let cancel = self.shutdown.child_token();
        let id = self.next_slot_id.fetch_add(1, Ordering::Relaxed);
        tokio::spawn(supervise(
            provider.clone(),
            key.region,
            tx,
            cancel.clone(),
            self.deps.clone(),
        ));
        slots.insert(
            key,
            Slot { id, rx: rx.clone(), refs: 1, idle_since: None, reaper_armed: false, cancel },
        );
        Some((id, rx))
    }

    fn release(self: &Arc<Self>, key: SlotKey, slot_id: u64) {
        let mut slots = self.lock_slots();
        let Some(slot) = slots.get_mut(&key).filter(|s| s.id == slot_id) else { return };
        slot.refs = slot.refs.saturating_sub(1);
        if slot.refs > 0 {
            return;
        }
        let linger = Duration::from_secs(self.config.linger_s);
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) if !linger.is_zero() => {
                slot.idle_since = Some(Instant::now());
                // One timer per slot: a slot that goes idle again while its timer runs
                // only moves the deadline.
                if !slot.reaper_armed {
                    slot.reaper_armed = true;
                    runtime.spawn(reap_when_idle(Arc::downgrade(self), key, slot_id, linger));
                }
            }
            _ => {
                if let Some(slot) = slots.remove(&key) {
                    slot.cancel.cancel();
                }
            }
        }
        drop(slots);
        self.capacity.notify_waiters();
    }
}

/// Removes the slot that has been idle longest; `false` when no slot is idle.
fn evict_idle(slots: &mut HashMap<SlotKey, Slot>) -> bool {
    let oldest = slots
        .iter()
        .filter(|(_, s)| s.refs == 0)
        .min_by_key(|(_, s)| s.idle_since)
        .map(|(k, _)| *k);
    match oldest.and_then(|k| slots.remove(&k)) {
        Some(slot) => {
            slot.cancel.cancel();
            true
        }
        None => false,
    }
}

/// The linger timer of one slot: removes it once it has been idle for `linger`.
async fn reap_when_idle(hub: Weak<HubInner>, key: SlotKey, slot_id: u64, linger: Duration) {
    let mut wait = linger;
    loop {
        tokio::time::sleep(wait).await;
        let Some(inner) = hub.upgrade() else { return };
        let removed = {
            let mut slots = inner.lock_slots();
            let Some(slot) = slots.get_mut(&key).filter(|s| s.id == slot_id) else { return };
            match slot.idle_since.map(|since| since.elapsed()) {
                None => {
                    // In use again; the next release arms a new timer.
                    slot.reaper_armed = false;
                    return;
                }
                Some(idle) if idle < linger => {
                    wait = linger.saturating_sub(idle);
                    continue;
                }
                Some(_) => slots.remove(&key),
            }
        };
        if let Some(slot) = removed {
            slot.cancel.cancel();
        }
        return;
    }
}

/// Runs a slot's refresh loop and restarts it, with backoff, if the provider panics. The
/// panic is reported as a failure of the slot, so subscribers see an error state instead
/// of a slot that silently stopped.
async fn supervise(
    provider: Arc<dyn FeedProvider>,
    region: Region,
    tx: watch::Sender<Arc<SlotState>>,
    cancel: CancellationToken,
    deps: HubDeps,
) {
    let source = provider.source_id();
    let mut crashes: u32 = 0;
    loop {
        let run = AssertUnwindSafe(refresh(provider.as_ref(), region, &tx, &cancel, &deps))
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
        let delay = backoff(crashes, provider.retry_floor(), provider.cadence());
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = cancel.cancelled() => return,
        }
    }
}

/// One slot's refresh loop: fetch, publish, wait for the cadence (or back off), repeat.
/// Returns only when cancelled.
async fn refresh(
    provider: &dyn FeedProvider,
    region: Region,
    tx: &watch::Sender<Arc<SlotState>>,
    cancel: &CancellationToken,
    deps: &HubDeps,
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
                let state = answered(fetched);
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
        let delay =
            if failures == 0 { jittered(cadence) } else { backoff(failures, floor, cadence) };
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = cancel.cancelled() => return,
        }
    }
}

/// Slot state for a provider's answer: its real age (never in the future), and its
/// degradation reason and notices cleaned and length-capped.
fn answered(fetched: Fetched) -> SlotState {
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
fn backoff(failures: u32, floor: Duration, cadence: Duration) -> Duration {
    let exponent = failures.clamp(1, 7) - 1;
    Duration::from_secs(5u64 << exponent).min(cadence).min(MAX_BACKOFF).max(floor)
}

/// ±10 % jitter so refresh tasks for many regions do not align.
fn jittered(base: Duration) -> Duration {
    let ms = u64::try_from(base.as_millis()).unwrap_or(u64::MAX);
    let spread = ms / 10;
    Duration::from_millis(ms.saturating_sub(spread) + fastrand::u64(0..=spread.saturating_mul(2)))
}

struct SubscriberCount<'a>(&'a AtomicU32);

impl Drop for SubscriberCount<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// What a subscription task works with.
struct Subscriber {
    hub: Arc<HubInner>,
    provider: Arc<dyn FeedProvider>,
    budget: Option<Arc<RegionBudget>>,
    events: watch::Sender<Option<SubscriptionEvent>>,
    cancel: CancellationToken,
}

/// Static facts about the subscribed feed.
#[derive(Debug, Clone, Copy)]
struct FeedMeta {
    cadence: Duration,
    source_id: &'static str,
    max_items: usize,
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

async fn run_subscription(sub: Subscriber, mut query_rx: watch::Receiver<FeedQuery>) {
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
                    } else if let Some(snapshot) = compose(&query, p, &mut leases, &meta, shortfall)
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

fn whole_seconds(d: SignedDuration) -> u32 {
    u32::try_from(d.as_secs().max(0)).unwrap_or(u32::MAX)
}

/// Freshness of the slots that hold items, and the time of their oldest data; `None`
/// while no slot holds items.
fn judge(
    states: &[Arc<SlotState>],
    now: Timestamp,
    cadence: Duration,
) -> Option<(Freshness, Option<Timestamp>)> {
    if states.iter().all(|s| s.items.is_none()) {
        return None;
    }
    let fetched_at = states.iter().filter(|s| s.items.is_some()).filter_map(|s| s.as_of).min();
    let age_s = fetched_at.map_or(0, |t| whole_seconds(now.duration_since(t)));
    let cadence_s = u32::try_from(cadence.as_secs()).unwrap_or(u32::MAX);
    let freshness = if let Some(reason) = states.iter().find_map(|s| s.last_error.as_deref()) {
        Freshness::Degraded { reason: reason.to_owned(), age_s }
    } else if age_s > cadence_s.saturating_mul(2) {
        Freshness::Stale { age_s }
    } else {
        Freshness::Live
    };
    Some((freshness, fetched_at))
}

/// Whether two freshness values are the same state (ages may differ).
fn same_state(a: &Freshness, b: &Freshness) -> bool {
    match (a, b) {
        (Freshness::Live, Freshness::Live) | (Freshness::Stale { .. }, Freshness::Stale { .. }) => {
            true
        }
        (Freshness::Degraded { reason: x, .. }, Freshness::Degraded { reason: y, .. })
        | (Freshness::Unavailable { reason: x }, Freshness::Unavailable { reason: y }) => x == y,
        _ => false,
    }
}

/// Whether the freshness judged now differs from what was last sent (without consuming
/// slot change notifications).
fn freshness_changed(
    leases: &HashMap<SlotKey, Lease>,
    meta: &FeedMeta,
    sent: Option<&Freshness>,
) -> bool {
    let states: Vec<Arc<SlotState>> = leases.values().map(|l| l.rx.borrow().clone()).collect();
    match (judge(&states, Timestamp::now(), meta.cadence), sent) {
        (Some((now, _)), Some(before)) => !same_state(&now, before),
        _ => false,
    }
}

/// Merges the subscription's slots, filters them to its query and judges freshness.
/// Returns `None` until at least one slot has produced a result or an error.
fn compose(
    query: &FeedQuery,
    plan: &Plan,
    leases: &mut HashMap<SlotKey, Lease>,
    meta: &FeedMeta,
    shortfall: Option<Shortfall>,
) -> Option<FeedSnapshot> {
    let states: Vec<Arc<SlotState>> =
        leases.values_mut().map(|l| l.rx.borrow_and_update().clone()).collect();
    let now = Timestamp::now();
    let sources = vec![meta.source_id.to_owned()];
    // Regions that have neither data nor an error have not reported yet.
    let pending = states.iter().filter(|s| s.items.is_none() && s.last_error.is_none()).count();
    let pending_regions = u32::try_from(pending).unwrap_or(u32::MAX);

    let mut notices: Vec<String> = Vec::new();
    for notice in states.iter().flat_map(|s| s.notices.iter()) {
        if notices.len() < MAX_PROVIDER_NOTICES && !notices.contains(notice) {
            notices.push(notice.clone());
        }
    }
    let mut coverage = plan.coverage.clone();
    if let Some(short) = shortfall {
        notices.push(short.notice().to_owned());
        if coverage == Coverage::Complete {
            coverage = Coverage::Partial { reason: short.notice().to_owned() };
        }
    }

    let Some((freshness, fetched_at)) = judge(&states, now, meta.cadence) else {
        let reason = states.iter().find_map(|s| s.last_error.as_deref())?;
        return Some(FeedSnapshot {
            feed: query.feed,
            sources,
            fetched_at: None,
            generated_at: now,
            freshness: Freshness::Unavailable { reason: reason.to_owned() },
            coverage,
            source_count: 0,
            pending_regions,
            items: FeedItems::empty(query.feed),
            notices,
        });
    };

    let lists: Vec<&FeedItems> = states.iter().filter_map(|s| s.items.as_deref()).collect();
    let (items, source_count, truncated) = filter_items(query, &lists, meta.max_items);
    if truncated {
        notices.push(format!(
            "Showing the first {} items. Zoom in or add filters to see the rest.",
            meta.max_items
        ));
    }
    Some(FeedSnapshot {
        feed: query.feed,
        sources,
        fetched_at,
        generated_at: now,
        freshness,
        coverage,
        source_count,
        pending_regions,
        items,
        notices,
    })
}

/// Concatenates lists, drops duplicate ids (overlapping regional queries), applies the
/// query's spatial and attribute filters and the item cap. Returns (items, count before
/// filtering, whether the cap truncated).
fn filter_items(
    query: &FeedQuery,
    lists: &[&FeedItems],
    max_items: usize,
) -> (FeedItems, u32, bool) {
    fn dedup_by<T: Clone>(lists: &[&Vec<T>], key: impl Fn(&T) -> String) -> Vec<T> {
        let mut seen = HashSet::new();
        lists.iter().flat_map(|l| l.iter()).filter(|x| seen.insert(key(x))).cloned().collect()
    }
    fn cap<T>(mut v: Vec<T>, max: usize) -> (Vec<T>, bool) {
        let truncated = v.len() > max;
        v.truncate(max);
        (v, truncated)
    }
    let in_view = |p: &lavender_domain::GeoPoint| query.bbox.is_none_or(|b| b.contains(p));
    let f = &query.filters;
    match query.feed {
        FeedId::Aircraft => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Aircraft(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |a| a.hex.clone());
            let count = merged.len();
            let filtered = merged
                .into_iter()
                .filter(|a| {
                    in_view(&a.position) && (f.military_only != Some(true) || a.is_military)
                })
                .collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Aircraft(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
        FeedId::Earthquakes => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Earthquakes(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |q| q.id.clone());
            let count = merged.len();
            let min_mag = f.min_magnitude.unwrap_or(f64::NEG_INFINITY);
            let filtered = merged
                .into_iter()
                .filter(|q| in_view(&q.position) && q.magnitude >= min_mag)
                .collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Earthquakes(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
        FeedId::Satellites => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Satellites(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |s| s.norad_id.to_string());
            let count = merged.len();
            let filtered = merged
                .into_iter()
                .filter(|s| f.groups.as_ref().is_none_or(|g| g.contains(&s.group)))
                .collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Satellites(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_is_capped_and_respects_the_floor() {
        let hour = Duration::from_secs(3_600);
        assert_eq!(backoff(1, Duration::ZERO, hour), Duration::from_secs(5));
        assert_eq!(backoff(3, Duration::ZERO, hour), Duration::from_secs(20));
        assert_eq!(backoff(50, Duration::ZERO, hour), MAX_BACKOFF);
        assert_eq!(backoff(2, Duration::ZERO, Duration::from_secs(7)), Duration::from_secs(7));
        let floor = Duration::from_secs(600);
        assert_eq!(backoff(1, floor, Duration::from_secs(7_200)), floor, "floor beats backoff");
    }

    #[test]
    fn budgets_count_distinct_regions() {
        let budget = RegionBudget::new(2);
        let key = |lon_idx| SlotKey {
            feed: FeedId::Aircraft,
            region: Region::Cell(lavender_geo::Cell { size_mdeg: 5000, lat_idx: 1, lon_idx }),
        };
        assert!(budget.take(key(1)));
        assert!(budget.take(key(1)), "a region already held counts once");
        assert!(budget.take(key(2)));
        assert!(!budget.take(key(3)));
        assert_eq!(budget.in_use(), 2);
        assert!(!budget.give_back(key(1)), "still held once");
        assert!(budget.give_back(key(1)));
        assert!(budget.take(key(3)));
    }

    #[test]
    fn freshness_states_compare_without_ages() {
        assert!(same_state(&Freshness::Stale { age_s: 1 }, &Freshness::Stale { age_s: 9 }));
        assert!(!same_state(&Freshness::Live, &Freshness::Stale { age_s: 9 }));
        let d = |r: &str| Freshness::Degraded { reason: r.to_owned(), age_s: 0 };
        assert!(same_state(&d("a"), &d("a")) && !same_state(&d("a"), &d("b")));
    }
}
