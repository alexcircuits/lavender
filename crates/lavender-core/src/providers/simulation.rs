//! The synthetic aircraft feed source (LB-151): when configuration enables
//! simulation, this provider replaces the live adsb.lol source for the
//! aircraft feed, ticking `AircraftSimulator` on the hub's cadence. Never the
//! default (an owner decision, like every optional source); no network is
//! touched, and every record is stamped with the `simulation` source id so
//! provenance stays honest in snapshots and the inspector.

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use lavender_domain::{FeedId, FeedItems, ProviderScope};

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use crate::config::SimulationConfig;
use crate::simulation::AircraftSimulator;

/// The notice every simulated snapshot carries, mirroring the fixture
/// provider's honesty about non-live data.
pub const SIMULATION_NOTICE: &str = "Synthetic simulated aircraft; no external source.";

#[derive(Debug)]
pub struct SimulationProvider {
    config: SimulationConfig,
    sim: Mutex<AircraftSimulator>,
}

impl SimulationProvider {
    #[must_use]
    pub fn new(config: SimulationConfig) -> Self {
        let sim = AircraftSimulator::new(config.seed, config.fleet_size);
        Self { config, sim: Mutex::new(sim) }
    }
}

impl SimulationProvider {
    /// One hub cadence step of the fleet, as a snapshot.
    fn tick_snapshot(&self) -> Fetched {
        let step_s = self.config.step_s.max(1) as f64;
        let aircraft = self.sim.lock().map(|mut sim| sim.tick(step_s)).unwrap_or_default();
        Fetched {
            items: FeedItems::Aircraft(aircraft),
            as_of: None,
            degraded: None,
            notices: vec![SIMULATION_NOTICE.to_string()],
        }
    }
}

#[async_trait]
impl FeedProvider for SimulationProvider {
    fn feed(&self) -> FeedId {
        FeedId::Aircraft
    }

    fn source_id(&self) -> &'static str {
        crate::simulation::SIMULATION_SOURCE
    }

    fn cadence(&self) -> Duration {
        Duration::from_secs(self.config.step_s.max(1))
    }

    fn scope(&self) -> ProviderScope {
        // The simulator is worldwide by construction, like the fixture source.
        ProviderScope::Global
    }

    fn enabled(&self) -> bool {
        true // The configuration gate happened at registration.
    }

    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        Ok(self.tick_snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SimulationConfig {
        SimulationConfig { enabled: true, seed: 7, fleet_size: 6, step_s: 2 }
    }

    #[test]
    fn fetches_deterministic_fleets_that_advance() {
        let provider = SimulationProvider::new(config());
        let first = provider.tick_snapshot();
        let second = provider.tick_snapshot();
        let FeedItems::Aircraft(a) = &first.items else {
            panic!("not an aircraft snapshot");
        };
        let FeedItems::Aircraft(b) = &second.items else {
            panic!("not an aircraft snapshot");
        };
        assert_eq!(a.len(), 6);
        assert_eq!(b.len(), 6);
        // Same fleet, moved: ids identical, at least one position changed.
        assert_eq!(a[0].hex, b[0].hex);
        assert!(a.iter().zip(b.iter()).any(|(x, y)| x.position != y.position));
        assert!(a.iter().all(|r| r.source == crate::simulation::SIMULATION_SOURCE));
        // Honesty: the snapshot says it is synthetic.
        assert!(first.notices.iter().any(|n| n.contains(SIMULATION_NOTICE)));
    }

    #[test]
    fn same_seed_same_sequence() {
        let a = SimulationProvider::new(config());
        let b = SimulationProvider::new(config());
        let fa = a.tick_snapshot();
        let fb = b.tick_snapshot();
        assert_eq!(fa.items, fb.items);
    }
}
