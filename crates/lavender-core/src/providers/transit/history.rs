//! Bounded in-memory GTFS-RT vehicle position history and trail store.

use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;

use lavender_domain::{TransitEpoch, TransitFeed, TransitTrail, TransitVehicle};

/// Maximum fixes preserved per vehicle ring buffer.
pub const TRANSIT_HISTORY_MAX_FIXES: usize = 128;

/// Retention window for historical fixes (15 minutes in milliseconds).
pub const TRANSIT_HISTORY_RETENTION_MS: i64 = 900_000;

/// Default per-feed memory limit (16 MiB).
pub const DEFAULT_FEED_BYTES_LIMIT: usize = 16 * 1024 * 1024;

/// Default process-wide memory limit (64 MiB).
pub const DEFAULT_PROCESS_BYTES_LIMIT: usize = 64 * 1024 * 1024;

/// Default process-wide tracking key limit.
pub const DEFAULT_MAX_KEYS: usize = 15_000;

/// Maximum epoch contexts preserved per vehicle.
pub const TRANSIT_MAX_EPOCHS: usize = 8;

const FIX_BYTES: usize = 40; // 5 * f64
const EPOCH_BYTES: usize = 128;
const ENTRY_OVERHEAD: usize = 128;

/// Validate a transit vehicle identifier for path traversal and security bounds.
#[must_use]
pub fn valid_transit_identifier(value: &str) -> bool {
    if value.is_empty() || value.len() > 256 || value == "." || value == ".." {
        return false;
    }
    !value.chars().any(|c| c.is_control() || c == '/' || c == '\\')
}

#[derive(Debug, Clone)]
struct VehicleHistoryEntry {
    vehicle_id: String,
    fixes: VecDeque<[f64; 5]>,
    epochs: Vec<TransitEpoch>,
    next_epoch_id: u32,
    truncated: bool,
    last_observed_ms: i64,
}

impl VehicleHistoryEntry {
    fn new(vehicle_id: String) -> Self {
        Self {
            vehicle_id,
            fixes: VecDeque::with_capacity(32),
            epochs: Vec::with_capacity(4),
            next_epoch_id: 1,
            truncated: false,
            last_observed_ms: 0,
        }
    }

    fn estimated_bytes(&self) -> usize {
        ENTRY_OVERHEAD
            + self.vehicle_id.len()
            + (self.fixes.capacity() * FIX_BYTES)
            + (self.epochs.len() * EPOCH_BYTES)
    }

    fn prune_expired(&mut self, cutoff_ms: i64) {
        while let Some(front) = self.fixes.front() {
            #[allow(clippy::cast_possible_truncation)]
            let fix_time = front[0] as i64;
            if fix_time < cutoff_ms {
                self.fixes.pop_front();
                self.truncated = true;
            } else {
                break;
            }
        }
    }
}

/// Bounded in-memory store for GTFS-RT vehicle position trails.
#[derive(Debug)]
pub struct TransitHistoryStore {
    feed_bytes_limit: usize,
    process_bytes_limit: usize,
    max_keys: usize,
    inner: RwLock<StoreInner>,
}

#[derive(Debug)]
struct StoreInner {
    /// `feed_id` -> (`vehicle_id` -> entry)
    feeds: HashMap<String, HashMap<String, VehicleHistoryEntry>>,
    total_bytes: usize,
    total_keys: usize,
}

/// Diagnostic metrics for the transit history store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitHistoryDiagnostics {
    /// Total bytes allocated by history entries.
    pub allocated_bytes: usize,
    /// Total active vehicle keys.
    pub active_keys: usize,
    /// Number of registered feeds with active trails.
    pub feed_count: usize,
}

impl Default for TransitHistoryStore {
    fn default() -> Self {
        Self::new(DEFAULT_FEED_BYTES_LIMIT, DEFAULT_PROCESS_BYTES_LIMIT, DEFAULT_MAX_KEYS)
    }
}

impl TransitHistoryStore {
    /// Creates a new transit history store with explicit byte and key caps.
    #[must_use]
    pub fn new(feed_bytes_limit: usize, process_bytes_limit: usize, max_keys: usize) -> Self {
        Self {
            feed_bytes_limit,
            process_bytes_limit,
            max_keys,
            inner: RwLock::new(StoreInner { feeds: HashMap::new(), total_bytes: 0, total_keys: 0 }),
        }
    }

    /// Ingest a batch of vehicle positions for a feed at the specified timestamp.
    pub fn ingest(&self, feed: &TransitFeed, vehicles: &[TransitVehicle], now_ms: i64) {
        let Ok(mut inner) = self.inner.write() else {
            return;
        };

        let cutoff_ms = now_ms - TRANSIT_HISTORY_RETENTION_MS;
        let feed_map = inner.feeds.entry(feed.id.clone()).or_default();

        for vehicle in vehicles {
            if !valid_transit_identifier(&vehicle.vehicle_id) {
                continue;
            }

            let Some(ts_s) = vehicle.timestamp else {
                continue;
            };
            let time_ms = ts_s * 1000;
            if time_ms < cutoff_ms {
                continue;
            }

            let entry = feed_map
                .entry(vehicle.vehicle_id.clone())
                .or_insert_with(|| VehicleHistoryEntry::new(vehicle.vehicle_id.clone()));

            // Skip duplicate or older observation timestamps
            if let Some(back) = entry.fixes.back() {
                #[allow(clippy::cast_possible_truncation)]
                let last_time = back[0] as i64;
                if time_ms <= last_time {
                    continue;
                }
            }

            let trip = vehicle.trip_id.as_deref().unwrap_or("");
            let route = vehicle.route_id.as_deref().unwrap_or("");
            let mode = &vehicle.mode;

            // Check if context changed from current active epoch
            let mut break_flag = 0.0;
            let active_epoch_id = match entry.epochs.last() {
                Some(active)
                    if active.trip == trip && active.route == route && active.mode == *mode =>
                {
                    active.id
                }
                _ => {
                    let epoch_id = entry.next_epoch_id;
                    entry.next_epoch_id = entry.next_epoch_id.wrapping_add(1).max(1);
                    if !entry.epochs.is_empty() {
                        break_flag = 16.0; // Bit 4 set on trip/route break
                    }
                    if entry.epochs.len() >= TRANSIT_MAX_EPOCHS {
                        entry.epochs.remove(0);
                    }
                    entry.epochs.push(TransitEpoch {
                        id: epoch_id,
                        trip: trip.to_string(),
                        route: route.to_string(),
                        mode: mode.clone(),
                    });
                    epoch_id
                }
            };

            #[allow(clippy::cast_precision_loss)]
            let fix = [
                time_ms as f64,
                vehicle.position.lat,
                vehicle.position.lon,
                break_flag,
                f64::from(active_epoch_id),
            ];

            if entry.fixes.len() >= TRANSIT_HISTORY_MAX_FIXES {
                entry.fixes.pop_front();
                entry.truncated = true;
            }
            entry.fixes.push_back(fix);
            entry.last_observed_ms = now_ms;
            entry.prune_expired(cutoff_ms);
        }

        // Clean up empty/expired entries and enforce byte caps
        Self::enforce_limits(
            &mut inner,
            cutoff_ms,
            self.feed_bytes_limit,
            self.process_bytes_limit,
            self.max_keys,
        );
    }

    /// Retrieve the historical position trail for a vehicle.
    #[must_use]
    pub fn get(&self, feed_id: &str, vehicle_id: &str, now_ms: i64) -> Option<TransitTrail> {
        let inner = self.inner.read().ok()?;
        let feed_map = inner.feeds.get(feed_id)?;
        let entry = feed_map.get(vehicle_id)?;

        let cutoff_ms = now_ms - TRANSIT_HISTORY_RETENTION_MS;
        let valid_fixes: Vec<[f64; 5]> = entry
            .fixes
            .iter()
            .copied()
            .filter(|f| {
                #[allow(clippy::cast_possible_truncation)]
                let fix_time = f[0] as i64;
                fix_time >= cutoff_ms
            })
            .collect();

        if valid_fixes.is_empty() {
            return None;
        }

        Some(TransitTrail {
            version: 1,
            feed_id: feed_id.to_string(),
            vehicle_id: vehicle_id.to_string(),
            fixes: valid_fixes,
            epochs: entry.epochs.clone(),
            truncated: entry.truncated,
        })
    }

    /// Clear all stored vehicle trails.
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.feeds.clear();
            inner.total_bytes = 0;
            inner.total_keys = 0;
        }
    }

    /// Returns diagnostic memory and key usage metrics.
    #[must_use]
    pub fn diagnostics(&self) -> TransitHistoryDiagnostics {
        let Ok(inner) = self.inner.read() else {
            return TransitHistoryDiagnostics { allocated_bytes: 0, active_keys: 0, feed_count: 0 };
        };

        TransitHistoryDiagnostics {
            allocated_bytes: inner.total_bytes,
            active_keys: inner.total_keys,
            feed_count: inner.feeds.len(),
        }
    }

    fn enforce_limits(
        inner: &mut StoreInner,
        cutoff_ms: i64,
        feed_bytes_limit: usize,
        process_bytes_limit: usize,
        max_keys: usize,
    ) {
        // 1. Evict expired entries where last observation is older than cutoff
        for feed_map in inner.feeds.values_mut() {
            feed_map.retain(|_, entry| {
                entry.prune_expired(cutoff_ms);
                !entry.fixes.is_empty()
            });
        }
        inner.feeds.retain(|_, feed_map| !feed_map.is_empty());

        // 2. Recompute total keys and bytes
        let mut total_keys = 0;
        let mut total_bytes = 0;
        for feed_map in inner.feeds.values() {
            total_keys += feed_map.len();
            for entry in feed_map.values() {
                total_bytes += entry.estimated_bytes();
            }
        }
        inner.total_keys = total_keys;
        inner.total_bytes = total_bytes;

        // 3. Enforce per-feed byte limits
        for feed_map in inner.feeds.values_mut() {
            let mut feed_bytes: usize =
                feed_map.values().map(VehicleHistoryEntry::estimated_bytes).sum();
            if feed_bytes > feed_bytes_limit {
                // Collect keys sorted by oldest last_observed_ms
                let mut entries: Vec<(String, i64)> =
                    feed_map.iter().map(|(k, v)| (k.clone(), v.last_observed_ms)).collect();
                entries.sort_by_key(|&(_, t)| t);

                for (key, _) in entries {
                    if feed_bytes <= feed_bytes_limit {
                        break;
                    }
                    if let Some(removed) = feed_map.remove(&key) {
                        let bytes = removed.estimated_bytes();
                        feed_bytes = feed_bytes.saturating_sub(bytes);
                        inner.total_bytes = inner.total_bytes.saturating_sub(bytes);
                        inner.total_keys = inner.total_keys.saturating_sub(1);
                    }
                }
            }
        }

        // 4. Enforce process-wide byte and key limits
        while (inner.total_bytes > process_bytes_limit || inner.total_keys > max_keys)
            && inner.total_keys > 0
        {
            // Find global oldest entry across all feeds
            let mut oldest_feed: Option<String> = None;
            let mut oldest_key: Option<String> = None;
            let mut oldest_time = i64::MAX;

            for (feed_id, feed_map) in &inner.feeds {
                for (v_id, entry) in feed_map {
                    if entry.last_observed_ms < oldest_time {
                        oldest_time = entry.last_observed_ms;
                        oldest_feed = Some(feed_id.clone());
                        oldest_key = Some(v_id.clone());
                    }
                }
            }

            if let (Some(f_id), Some(v_id)) = (oldest_feed, oldest_key) {
                if let Some(feed_map) = inner.feeds.get_mut(&f_id)
                    && let Some(removed) = feed_map.remove(&v_id)
                {
                    let bytes = removed.estimated_bytes();
                    inner.total_bytes = inner.total_bytes.saturating_sub(bytes);
                    inner.total_keys = inner.total_keys.saturating_sub(1);
                }
            } else {
                break;
            }
        }

        inner.feeds.retain(|_, feed_map| !feed_map.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lavender_domain::GeoPoint;

    fn test_feed() -> TransitFeed {
        TransitFeed {
            id: "mbta".to_string(),
            name: "MBTA".to_string(),
            operator: "Massachusetts Bay Transportation Authority".to_string(),
            region: "Boston, MA".to_string(),
            center: GeoPoint::new(42.3601, -71.0589).unwrap(),
            load_radius_km: 70.0,
            url: "https://cdn.mbta.com/realtime/VehiclePositions.pb".to_string(),
            license: "MassDOT Developers License Agreement".to_string(),
            attribution: "MBTA / MassDOT".to_string(),
            default_enabled: true,
            default_mode: "bus".to_string(),
        }
    }

    fn make_vehicle(
        id: &str,
        time_s: i64,
        lat: f64,
        lon: f64,
        trip: &str,
        route: &str,
    ) -> TransitVehicle {
        TransitVehicle {
            id: format!("mbta:{id}"),
            vehicle_id: id.to_string(),
            feed_id: "mbta".to_string(),
            position: GeoPoint::new(lat, lon).unwrap(),
            bearing: None,
            speed_mps: None,
            timestamp: Some(time_s),
            timestamp_source: "vehicle".to_string(),
            route_id: Some(route.to_string()),
            trip_id: Some(trip.to_string()),
            direction_id: None,
            label: None,
            stop_id: None,
            status: None,
            occupancy: None,
            mode: "bus".to_string(),
        }
    }

    #[test]
    fn valid_transit_identifier_rules() {
        assert!(valid_transit_identifier("bus-123"));
        assert!(valid_transit_identifier("12345"));
        assert!(!valid_transit_identifier(""));
        assert!(!valid_transit_identifier("."));
        assert!(!valid_transit_identifier(".."));
        assert!(!valid_transit_identifier("path/traversal"));
        assert!(!valid_transit_identifier("win\\traversal"));
        assert!(!valid_transit_identifier("null\0byte"));
        assert!(!valid_transit_identifier(&"a".repeat(257)));
    }

    #[test]
    fn ring_buffer_caps_at_128_fixes_and_marks_truncated() {
        let store = TransitHistoryStore::default();
        let feed = test_feed();

        for i in 0..200_i32 {
            let v = make_vehicle(
                "bus-1",
                1_000 + i64::from(i),
                42.0 + (f64::from(i) * 0.001),
                -71.0,
                "trip-1",
                "1",
            );
            store.ingest(&feed, &[v], (1_000 + i64::from(i)) * 1000);
        }

        let trail = store.get("mbta", "bus-1", 200_000).unwrap();
        assert_eq!(trail.fixes.len(), 128);
        assert!(trail.truncated);
        assert_eq!(trail.epochs.len(), 1);
    }

    #[test]
    fn trip_break_records_epochs_and_break_flags() {
        let store = TransitHistoryStore::default();
        let feed = test_feed();

        // Ingest under trip 1
        for i in 0..10 {
            let v = make_vehicle("bus-1", 1_000 + i, 42.0, -71.0, "trip-1", "1");
            store.ingest(&feed, &[v], (1_000 + i) * 1000);
        }

        // Ingest under trip 2
        for i in 10..20 {
            let v = make_vehicle("bus-1", 1_000 + i, 42.1, -71.1, "trip-2", "2");
            store.ingest(&feed, &[v], (1_000 + i) * 1000);
        }

        let trail = store.get("mbta", "bus-1", 1_025_000).unwrap();
        assert_eq!(trail.epochs.len(), 2);
        assert_eq!(trail.epochs[0].trip, "trip-1");
        assert_eq!(trail.epochs[1].trip, "trip-2");

        // The first fix of trip 2 should have break flag bit 16 set
        let trip_2_fix = &trail.fixes[10];
        assert_eq!(trip_2_fix[3], 16.0);
    }

    #[test]
    fn retention_expires_old_fixes() {
        let store = TransitHistoryStore::default();
        let feed = test_feed();

        let v1 = make_vehicle("bus-1", 100, 42.0, -71.0, "trip-1", "1");
        store.ingest(&feed, &[v1], 100_000);

        // Fetch within 15 minutes (900s) -> present
        let trail1 = store.get("mbta", "bus-1", 500_000);
        assert!(trail1.is_some());

        // Fetch after 15 minutes (900s) -> expired
        let trail2 = store.get("mbta", "bus-1", 1_100_000);
        assert!(trail2.is_none());
    }

    #[test]
    fn byte_and_key_limits_are_enforced() {
        // Strict store with max 3 keys and small byte cap
        let store = TransitHistoryStore::new(4000, 4000, 3);
        let feed = test_feed();

        for i in 0..10 {
            let v = make_vehicle(&format!("bus-{i}"), 100 + i, 42.0, -71.0, "trip", "1");
            store.ingest(&feed, &[v], (100 + i) * 1000);
        }

        let diag = store.diagnostics();
        assert!(diag.active_keys <= 3);
        assert!(diag.allocated_bytes <= 4000);
    }
}
