//! In-memory rolling observation store for GNSS navigation integrity analysis.

use std::collections::HashMap;

use jiff::{SignedDuration, Timestamp};
use lavender_domain::{BBox, GnssCell, GnssSnapshot};

use super::records::{
    GNSS_CELL_DEG, GNSS_MIN_AIRCRAFT, GNSS_WINDOW_MINUTES, GnssAircraftObservation,
    default_gnss_provenance, evaluate_gnss_level, gnss_cell_bbox, gnss_cell_center,
    gnss_cell_coords, gnss_cell_id,
};

/// Maximum observations stored concurrently (bounded memory).
pub const MAX_GNSS_OBSERVATIONS: usize = 100_000;

#[derive(Debug, Clone, Copy)]
struct ObservationEntry {
    row: u16,
    col: u16,
    seen_at: Timestamp,
    degraded_at: Option<Timestamp>,
}

/// Rolling window observation store for aircraft navigation integrity reports.
#[derive(Debug, Clone)]
pub struct GnssObservationStore {
    /// Maps `(hex, (row, col))` to observation timestamps.
    observations: HashMap<(String, (u16, u16)), ObservationEntry>,
    window_duration: SignedDuration,
    cell_deg: f64,
    min_aircraft: usize,
    capacity: usize,
}

impl Default for GnssObservationStore {
    fn default() -> Self {
        Self::new(
            SignedDuration::from_mins(i64::from(GNSS_WINDOW_MINUTES)),
            GNSS_CELL_DEG,
            GNSS_MIN_AIRCRAFT,
            MAX_GNSS_OBSERVATIONS,
        )
    }
}

impl GnssObservationStore {
    /// Creates a new observation store with the given window, cell resolution, and capacity.
    #[must_use]
    pub fn new(
        window_duration: SignedDuration,
        cell_deg: f64,
        min_aircraft: usize,
        capacity: usize,
    ) -> Self {
        Self { observations: HashMap::new(), window_duration, cell_deg, min_aircraft, capacity }
    }

    /// Number of active observation entries in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.observations.len()
    }

    /// Whether the store contains zero active observations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }

    /// Ingests a batch of aircraft observations into the store, updating seen and degraded timestamps.
    pub fn accumulate(&mut self, records: &[GnssAircraftObservation], now: Timestamp) {
        let window_cutoff = now.checked_sub(self.window_duration).unwrap_or(Timestamp::UNIX_EPOCH);

        for obs in records {
            // Clamped observation timestamp: future timestamps are clamped to now,
            // while observations older than the window are dropped.
            let at = if obs.observed_at > now { now } else { obs.observed_at };
            if at < window_cutoff {
                continue;
            }

            let coords = gnss_cell_coords(obs.position.lat, obs.position.lon, self.cell_deg);
            let key = (obs.hex.clone(), coords);

            if let Some(entry) = self.observations.get_mut(&key) {
                entry.seen_at = entry.seen_at.max(at);
                if obs.degraded {
                    entry.degraded_at = Some(entry.degraded_at.map_or(at, |prev| prev.max(at)));
                }
            } else {
                if self.observations.len() >= self.capacity {
                    self.prune(now);
                    // If still at capacity, drop arbitrary oldest
                    if self.observations.len() >= self.capacity
                        && let Some(oldest_key) = self
                            .observations
                            .iter()
                            .min_by_key(|(_, v)| v.seen_at)
                            .map(|(k, _)| k.clone())
                    {
                        self.observations.remove(&oldest_key);
                    }
                }

                self.observations.insert(
                    key,
                    ObservationEntry {
                        row: coords.0,
                        col: coords.1,
                        seen_at: at,
                        degraded_at: if obs.degraded { Some(at) } else { None },
                    },
                );
            }
        }

        self.prune(now);
    }

    /// Prunes entries and degraded timestamps that have aged out of the rolling window.
    pub fn prune(&mut self, now: Timestamp) {
        let window_cutoff = now.checked_sub(self.window_duration).unwrap_or(Timestamp::UNIX_EPOCH);

        self.observations.retain(|_, entry| {
            if entry.seen_at < window_cutoff {
                return false;
            }
            if let Some(deg_at) = entry.degraded_at
                && deg_at < window_cutoff
            {
                entry.degraded_at = None;
            }
            true
        });
    }

    /// Aggregates all valid observations into displayable geographic cells.
    #[must_use]
    pub fn bin_cells(&self, now: Timestamp, bbox_filter: Option<&BBox>) -> GnssSnapshot {
        let mut tallies: HashMap<(u16, u16), (usize, usize)> = HashMap::new();

        for entry in self.observations.values() {
            let tally = tallies.entry((entry.row, entry.col)).or_insert((0, 0));
            tally.0 += 1;
            if entry.degraded_at.is_some() {
                tally.1 += 1;
            }
        }

        let mut cells = Vec::new();
        let mut anomaly_count = 0;

        for ((row, col), (total, degraded)) in tallies {
            if total < self.min_aircraft {
                continue;
            }

            let bbox = gnss_cell_bbox(row, col, self.cell_deg);
            if let Some(filter) = bbox_filter
                && !filter.intersects(&bbox)
            {
                continue;
            }

            let center = gnss_cell_center(row, col, self.cell_deg);
            let id = gnss_cell_id(row, col);
            let (percent_degraded, level, state, interpretation) =
                evaluate_gnss_level(total, degraded);

            if interpretation.is_some() {
                anomaly_count += 1;
            }

            cells.push(GnssCell {
                id,
                bbox,
                center,
                aircraft: total,
                degraded,
                percent_degraded,
                level,
                state,
                interpretation,
            });
        }

        cells.sort_by(|a, b| a.id.cmp(&b.id));

        GnssSnapshot {
            as_of: now,
            window_minutes: default_gnss_provenance().window_minutes,
            cell_deg: self.cell_deg,
            min_aircraft: self.min_aircraft,
            total_cells: cells.len(),
            anomaly_cells: anomaly_count,
            cells,
        }
    }
}
