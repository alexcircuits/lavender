//! Shares refresh regions between viewers, counts client budgets, and releases idle regions.

use super::refresh::supervise_refresh;
use super::{
    BUDGET_NOTICE, CAPACITY_NOTICE, HubInner, Lease, Plan, RegionBudget, Shortfall, Slot, SlotKey,
    SlotState,
};
use crate::error::CoreError;
use crate::providers::{FeedProvider, Region};
use lavender_domain::{BBox, Coverage, FeedQuery, ProviderScope};
use lavender_geo::plan_cells;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};
use tokio::sync::watch;

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
    pub(super) fn take(&self, key: SlotKey) -> bool {
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
    pub(super) fn give_back(&self, key: SlotKey) -> bool {
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

impl Shortfall {
    pub(super) fn notice(self) -> &'static str {
        match self {
            Self::Capacity => CAPACITY_NOTICE,
            Self::Budget => BUDGET_NOTICE,
        }
    }

    pub(super) fn error(self) -> CoreError {
        CoreError::Limit(
            match self {
                Self::Capacity => "Lavender is at capacity; try again shortly.",
                Self::Budget => "Too many map areas are open from this client; close a view first.",
            }
            .to_owned(),
        )
    }
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

impl HubInner {
    pub(super) fn lock_slots(&self) -> std::sync::MutexGuard<'_, HashMap<SlotKey, Slot>> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn plan(&self, query: &FeedQuery) -> Result<Plan, CoreError> {
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
    pub(super) fn acquire(
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
        tokio::spawn(supervise_refresh(
            provider.clone(),
            key.region,
            tx,
            cancel.clone(),
            self.deps.clone(),
            self.aircraft_history.clone(),
        ));
        slots.insert(
            key,
            Slot { id, rx: rx.clone(), refs: 1, idle_since: None, reaper_armed: false, cancel },
        );
        Some((id, rx))
    }

    pub(super) fn release(self: &Arc<Self>, key: SlotKey, slot_id: u64) {
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
