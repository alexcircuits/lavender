//! Per-client resources, keyed by [`crate::security::client_key`]: the region budget a
//! client's live streams and snapshots share (so one client cannot fill the feed hub and
//! starve the rest), and WebSocket connection admission (a global cap and a per-client
//! cap).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use lavender_core::RegionBudget;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Per-client limits.
#[derive(Debug, Clone, Copy)]
pub struct ClientLimits {
    /// Distinct feed regions one client may hold at once.
    pub regions: usize,
    /// Concurrent WebSocket connections in total.
    pub connections: usize,
    /// Concurrent WebSocket connections per client.
    pub connections_per_client: usize,
}

/// Why a WebSocket connection was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The server holds its maximum number of connections.
    ServerFull,
    /// This client holds its maximum number of connections.
    TooManyFromClient,
}

#[derive(Debug)]
struct Inner {
    limits: ClientLimits,
    budgets: Mutex<Budgets>,
    connections: Arc<Semaphore>,
    per_client: Mutex<HashMap<IpAddr, usize>>,
}

/// Budgets are kept only while something uses them; dead entries are swept when the map
/// has doubled since the last sweep, so the cost is amortised.
#[derive(Debug, Default)]
struct Budgets {
    map: HashMap<IpAddr, Weak<RegionBudget>>,
    sweep_at: usize,
}

/// Shared handle; cheap to clone.
#[derive(Debug, Clone)]
pub struct Clients {
    inner: Arc<Inner>,
}

/// One admitted connection; frees its places when dropped.
#[derive(Debug)]
pub struct ConnectionPermit {
    _global: OwnedSemaphorePermit,
    inner: Arc<Inner>,
    client: IpAddr,
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        let mut per_client = self.inner.per_client.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(n) = per_client.get_mut(&self.client) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                per_client.remove(&self.client);
            }
        }
    }
}

impl Clients {
    #[must_use]
    pub fn new(limits: ClientLimits) -> Self {
        Self {
            inner: Arc::new(Inner {
                limits,
                budgets: Mutex::new(Budgets { map: HashMap::new(), sweep_at: 64 }),
                connections: Arc::new(Semaphore::new(limits.connections)),
                per_client: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// The region budget of `client`, shared by all of its requests while any is active.
    #[must_use]
    pub fn budget(&self, client: IpAddr) -> Arc<RegionBudget> {
        let mut budgets = self.inner.budgets.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(budget) = budgets.map.get(&client).and_then(Weak::upgrade) {
            return budget;
        }
        if budgets.map.len() >= budgets.sweep_at {
            budgets.map.retain(|_, b| b.strong_count() > 0);
            budgets.sweep_at = (budgets.map.len() * 2).max(64);
        }
        let budget = Arc::new(RegionBudget::new(self.inner.limits.regions));
        budgets.map.insert(client, Arc::downgrade(&budget));
        budget
    }

    /// Admits a WebSocket connection from `client`, or says why not.
    ///
    /// # Errors
    /// Returns a [`Refusal`] when the server or this client is at its connection limit.
    pub fn admit(&self, client: IpAddr) -> Result<ConnectionPermit, Refusal> {
        let mut per_client = self.inner.per_client.lock().unwrap_or_else(PoisonError::into_inner);
        let held = per_client.get(&client).copied().unwrap_or(0);
        if held >= self.inner.limits.connections_per_client {
            return Err(Refusal::TooManyFromClient);
        }
        let global =
            self.inner.connections.clone().try_acquire_owned().map_err(|_| Refusal::ServerFull)?;
        per_client.insert(client, held + 1);
        Ok(ConnectionPermit { _global: global, inner: self.inner.clone(), client })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clients(connections: usize, per_client: usize) -> Clients {
        Clients::new(ClientLimits { regions: 4, connections, connections_per_client: per_client })
    }

    #[test]
    fn one_budget_per_client_while_in_use() {
        let c = clients(10, 10);
        let a = IpAddr::from([10, 0, 0, 1]);
        let first = c.budget(a);
        assert!(Arc::ptr_eq(&first, &c.budget(a)), "shared while alive");
        assert!(!Arc::ptr_eq(&first, &c.budget(IpAddr::from([10, 0, 0, 2]))));
        assert_eq!(first.limit(), 4);
        drop(first);
        for i in 0..1_000u32 {
            let _ = c.budget(IpAddr::from(i.to_be_bytes()));
        }
        let size = c.inner.budgets.lock().unwrap().map.len();
        assert!(size <= 128, "dead budgets are swept: {size}");
    }

    #[test]
    fn connections_are_capped_per_client_and_in_total() {
        let c = clients(3, 2);
        let (a, b) = (IpAddr::from([10, 0, 0, 1]), IpAddr::from([10, 0, 0, 2]));
        let a1 = c.admit(a).unwrap();
        let _a2 = c.admit(a).unwrap();
        assert_eq!(c.admit(a).unwrap_err(), Refusal::TooManyFromClient);
        let _b1 = c.admit(b).unwrap();
        assert_eq!(c.admit(b).unwrap_err(), Refusal::ServerFull);
        drop(a1);
        let _b2 = c.admit(b).unwrap();
        assert_eq!(c.admit(a).unwrap_err(), Refusal::ServerFull);
    }
}
