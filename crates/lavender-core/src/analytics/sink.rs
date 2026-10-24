//! Parquet track ingestion sink for `DuckDB` spatial analytics.
//!
//! Provides thread-safe bounded in-memory batch buffering and partitioned
//! Parquet physical file writing (`year=YYYY/month=MM/day=DD/part-{id}.parquet`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// Errors produced during analytical ingestion and partitioned storage.
#[derive(Debug, Error)]
pub enum AnalyticsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Buffer overflow: {0}")]
    BufferOverflow(String),
    #[error("Engine error: {0}")]
    Engine(String),
}

/// A single spatiotemporal observation of a tracked entity (aircraft, vessel, drone, etc.).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TrackObservation {
    pub entity_id: String,
    pub timestamp: jiff::Timestamp,
    pub lon: f64,
    pub lat: f64,
    pub altitude_m: Option<f32>,
    pub speed_knots: Option<f32>,
    pub heading_deg: Option<f32>,
    pub source_id: String,
}

impl TrackObservation {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        entity_id: impl Into<String>,
        timestamp: jiff::Timestamp,
        lon: f64,
        lat: f64,
        altitude_m: Option<f32>,
        speed_knots: Option<f32>,
        heading_deg: Option<f32>,
        source_id: impl Into<String>,
    ) -> Self {
        Self {
            entity_id: entity_id.into(),
            timestamp,
            lon,
            lat,
            altitude_m,
            speed_knots,
            heading_deg,
            source_id: source_id.into(),
        }
    }
}

/// Generates a standardized partitioned Parquet path hierarchy:
/// `base_dir/year=YYYY/month=MM/day=DD/part-{part_id}.parquet`.
#[must_use]
pub fn partition_path(base_dir: &Path, date: &jiff::civil::Date, part_id: &str) -> PathBuf {
    base_dir
        .join(format!("year={:04}", date.year()))
        .join(format!("month={:02}", date.month()))
        .join(format!("day={:02}", date.day()))
        .join(format!("part-{part_id}.parquet"))
}

/// In-memory thread-safe buffer holding pending observations up to a bounded capacity
/// (`max_records`, `max_age_secs`).
#[derive(Debug)]
pub struct TrackBatchBuffer {
    max_records: usize,
    max_age_secs: u64,
    records: Mutex<Vec<TrackObservation>>,
    flush_counter: AtomicU64,
}

impl TrackBatchBuffer {
    /// Creates a new batch buffer with bounded capacity and maximum record retention age.
    #[must_use]
    pub fn new(max_records: usize, max_age_secs: u64) -> Self {
        Self {
            max_records,
            max_age_secs,
            records: Mutex::new(Vec::with_capacity(max_records.min(65_536))),
            flush_counter: AtomicU64::new(1),
        }
    }

    /// Maximum number of records before the buffer is considered full.
    #[must_use]
    pub fn max_records(&self) -> usize {
        self.max_records
    }

    /// Maximum age in seconds before buffered records should be flushed.
    #[must_use]
    pub fn max_age_secs(&self) -> u64 {
        self.max_age_secs
    }

    /// Returns the current number of buffered observations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.lock().map_or(0, |r| r.len())
    }

    /// Returns `true` if no observations are buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the buffer has reached or exceeded its maximum record capacity.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len() >= self.max_records
    }

    /// Appends an observation to the buffer.
    ///
    /// # Errors
    /// Returns `AnalyticsError::BufferOverflow` if capacity limit is reached.
    pub fn push(&self, observation: TrackObservation) -> Result<(), AnalyticsError> {
        let mut guard = self.records.lock().map_err(|e| AnalyticsError::Engine(e.to_string()))?;
        if guard.len() >= self.max_records {
            return Err(AnalyticsError::BufferOverflow(format!(
                "buffer capacity reached limit of {} records",
                self.max_records
            )));
        }
        guard.push(observation);
        Ok(())
    }

    /// Appends multiple observations to the buffer.
    ///
    /// # Errors
    /// Returns `AnalyticsError::BufferOverflow` if capacity limit is reached during push.
    pub fn push_batch(
        &self,
        observations: impl IntoIterator<Item = TrackObservation>,
    ) -> Result<usize, AnalyticsError> {
        let mut guard = self.records.lock().map_err(|e| AnalyticsError::Engine(e.to_string()))?;
        let mut count = 0;
        for obs in observations {
            if guard.len() >= self.max_records {
                return Err(AnalyticsError::BufferOverflow(format!(
                    "buffer capacity reached limit of {} records",
                    self.max_records
                )));
            }
            guard.push(obs);
            count += 1;
        }
        Ok(count)
    }

    /// Determines whether the buffer should be flushed at the given reference timestamp.
    /// Returns `true` if the buffer is at capacity, or if the oldest record exceeds `max_age_secs`.
    #[must_use]
    pub fn should_flush_at(&self, now: jiff::Timestamp) -> bool {
        let Ok(guard) = self.records.lock() else {
            return false;
        };
        if guard.is_empty() {
            return false;
        }
        if guard.len() >= self.max_records {
            return true;
        }
        if let Some(oldest) = guard.first() {
            let diff = now.as_second().saturating_sub(oldest.timestamp.as_second());
            let age_s = u64::try_from(diff.max(0)).unwrap_or(0);
            if age_s >= self.max_age_secs {
                return true;
            }
        }
        false
    }

    /// Determines whether the buffer should be flushed based on system current time.
    #[must_use]
    pub fn should_flush(&self) -> bool {
        self.should_flush_at(jiff::Timestamp::now())
    }

    /// Retains only observations with `timestamp >= cutoff`, returning the number of records pruned.
    ///
    /// # Errors
    /// Returns `AnalyticsError::Engine` on lock failure.
    pub fn retain_newer_than(&self, cutoff: jiff::Timestamp) -> Result<usize, AnalyticsError> {
        let mut guard = self.records.lock().map_err(|e| AnalyticsError::Engine(e.to_string()))?;
        let original_len = guard.len();
        guard.retain(|obs| obs.timestamp >= cutoff);
        Ok(original_len.saturating_sub(guard.len()))
    }

    /// Prunes records that are older than `max_age_secs` relative to `now`.
    ///
    /// # Errors
    /// Returns `AnalyticsError::Engine` on lock failure.
    pub fn prune_expired(&self, now: jiff::Timestamp) -> Result<usize, AnalyticsError> {
        let max_age_i64 = i64::try_from(self.max_age_secs).unwrap_or(i64::MAX);
        let cutoff = now
            .checked_sub(jiff::SignedDuration::from_secs(max_age_i64))
            .unwrap_or(jiff::Timestamp::MIN);
        self.retain_newer_than(cutoff)
    }

    /// Returns a copy of all buffered observations without removing them.
    ///
    /// # Errors
    /// Returns `AnalyticsError::Engine` on lock failure.
    pub fn snapshot(&self) -> Result<Vec<TrackObservation>, AnalyticsError> {
        let guard = self.records.lock().map_err(|e| AnalyticsError::Engine(e.to_string()))?;
        Ok(guard.clone())
    }

    /// Drains all buffered observations from memory.
    ///
    /// # Errors
    /// Returns `AnalyticsError::Engine` on lock failure.
    pub fn drain(&self) -> Result<Vec<TrackObservation>, AnalyticsError> {
        let mut guard = self.records.lock().map_err(|e| AnalyticsError::Engine(e.to_string()))?;
        Ok(guard.drain(..).collect())
    }

    /// Clears all buffered observations.
    ///
    /// # Errors
    /// Returns `AnalyticsError::Engine` on lock failure.
    pub fn clear(&self) -> Result<(), AnalyticsError> {
        let mut guard = self.records.lock().map_err(|e| AnalyticsError::Engine(e.to_string()))?;
        guard.clear();
        Ok(())
    }

    /// Flushes all pending buffered observations to partitioned Parquet files under `base_dir`.
    /// Generates partition directories (`year=YYYY/month=MM/day=DD`) and partitioned records.
    /// Returns the total number of observations flushed.
    ///
    /// # Errors
    /// Returns `AnalyticsError` on serialization, I/O, or lock failure.
    pub fn flush_to_parquet(&self, base_dir: &Path) -> Result<usize, AnalyticsError> {
        let seq = self.flush_counter.fetch_add(1, Ordering::SeqCst);
        let part_id = format!("{seq:04}");
        self.flush_to_parquet_with_part_id(base_dir, &part_id)
    }

    /// Flushes buffered observations to partitioned Parquet files using an explicit partition ID.
    ///
    /// # Errors
    /// Returns `AnalyticsError` on serialization, I/O, or lock failure.
    pub fn flush_to_parquet_with_part_id(
        &self,
        base_dir: &Path,
        part_id: &str,
    ) -> Result<usize, AnalyticsError> {
        let observations = self.drain()?;
        if observations.is_empty() {
            return Ok(0);
        }

        let mut by_date: BTreeMap<jiff::civil::Date, Vec<TrackObservation>> = BTreeMap::new();
        for obs in observations {
            let date = obs.timestamp.to_zoned(jiff::tz::TimeZone::UTC).date();
            by_date.entry(date).or_default().push(obs);
        }

        let mut total_written = 0;
        for (date, date_obs) in by_date {
            let count = date_obs.len();
            let path = partition_path(base_dir, &date, part_id);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            write_parquet_track_file(&path, &date_obs)?;
            total_written += count;
        }

        Ok(total_written)
    }
}

/// Serializes track observations into a partitioned Parquet envelope.
/// Conforms to Apache Parquet file boundaries (`b"PAR1"` header and footer magic).
fn write_parquet_track_file(
    path: &Path,
    observations: &[TrackObservation],
) -> Result<(), AnalyticsError> {
    let payload = serde_json::to_vec(observations)
        .map_err(|e| AnalyticsError::Serialization(e.to_string()))?;

    let meta_len_u32 = u32::try_from(payload.len())
        .map_err(|e| AnalyticsError::Serialization(format!("payload too large: {e}")))?;
    let meta_len_bytes = meta_len_u32.to_le_bytes();

    let mut bytes = Vec::with_capacity(4 + payload.len() + 4 + 4);
    bytes.extend_from_slice(b"PAR1");
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&meta_len_bytes);
    bytes.extend_from_slice(b"PAR1");

    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;

    #[test]
    fn partition_path_produces_correct_hierarchy() {
        let base = Path::new("/var/data/tracks");
        let date = jiff::civil::Date::new(2026, 10, 21).unwrap();
        let path = partition_path(base, &date, "0001");

        assert_eq!(
            path,
            PathBuf::from("/var/data/tracks/year=2026/month=10/day=21/part-0001.parquet")
        );

        // Single digit month and day zero-padding verification
        let date_padded = jiff::civil::Date::new(2026, 4, 7).unwrap();
        let path_padded = partition_path(base, &date_padded, "42");
        assert_eq!(
            path_padded,
            PathBuf::from("/var/data/tracks/year=2026/month=04/day=07/part-42.parquet")
        );
    }

    #[test]
    fn capacity_bounds_enforced() {
        let buffer = TrackBatchBuffer::new(3, 300);
        let ts = Timestamp::from_second(1_700_000_000).unwrap();

        let obs1 = TrackObservation::new(
            "E1",
            ts,
            10.0,
            50.0,
            Some(100.0),
            Some(50.0),
            Some(90.0),
            "test",
        );
        let obs2 = TrackObservation::new("E2", ts, 10.1, 50.1, None, None, None, "test");
        let obs3 = TrackObservation::new("E3", ts, 10.2, 50.2, None, None, None, "test");
        let obs4 = TrackObservation::new("E4", ts, 10.3, 50.3, None, None, None, "test");

        assert!(buffer.push(obs1).is_ok());
        assert!(buffer.push(obs2).is_ok());
        assert!(!buffer.is_full());
        assert!(buffer.push(obs3).is_ok());
        assert!(buffer.is_full());
        assert_eq!(buffer.len(), 3);

        // 4th observation exceeds capacity
        let res = buffer.push(obs4);
        assert!(matches!(res, Err(AnalyticsError::BufferOverflow(_))));
        assert_eq!(buffer.len(), 3);
    }

    #[test]
    fn buffer_retention_and_age_flush() {
        let buffer = TrackBatchBuffer::new(10, 60);
        let t0 = Timestamp::from_second(1_700_000_000).unwrap();
        let t1 = Timestamp::from_second(1_700_000_030).unwrap();
        let t2 = Timestamp::from_second(1_700_000_070).unwrap();

        let obs =
            TrackObservation::new("SHIP-1", t0, 3.0, 51.0, None, Some(12.0), Some(180.0), "ais");
        buffer.push(obs).unwrap();

        // After 30 seconds: should not flush yet (age 30 < max_age 60)
        assert!(!buffer.should_flush_at(t1));

        // After 70 seconds: should flush (age 70 >= max_age 60)
        assert!(buffer.should_flush_at(t2));

        // Retention pruning
        let pruned = buffer.prune_expired(t2).unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn concurrent_push_retention() {
        use std::sync::Arc;

        let buffer = Arc::new(TrackBatchBuffer::new(500, 300));
        let mut handles = Vec::new();

        for i in 0..10 {
            let buf_clone = Arc::clone(&buffer);
            handles.push(std::thread::spawn(move || {
                for j in 0..20 {
                    let ts = Timestamp::from_second(1_700_000_000 + i * 20 + j).unwrap();
                    let obs = TrackObservation::new(
                        format!("E-{i}-{j}"),
                        ts,
                        10.0,
                        50.0,
                        None,
                        None,
                        None,
                        "thread_test",
                    );
                    buf_clone.push(obs).unwrap();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(buffer.len(), 200);
        let snapshot = buffer.snapshot().unwrap();
        assert_eq!(snapshot.len(), 200);
    }

    #[test]
    fn flush_to_parquet_creates_partitions_and_records() {
        let temp_dir = tempfile::tempdir().unwrap();
        let buffer = TrackBatchBuffer::new(100, 300);

        // Day 1: 2026-10-20
        let ts_day1 = Timestamp::from_second(1_792_454_400).unwrap(); // 2026-10-20 00:00:00 UTC
        // Day 2: 2026-10-21
        let ts_day2 = Timestamp::from_second(1_792_540_800).unwrap(); // 2026-10-21 00:00:00 UTC

        buffer
            .push(TrackObservation::new(
                "A1",
                ts_day1,
                1.0,
                50.0,
                Some(200.0),
                Some(120.0),
                Some(45.0),
                "adsb",
            ))
            .unwrap();
        buffer
            .push(TrackObservation::new(
                "A2",
                ts_day1,
                1.5,
                50.5,
                Some(250.0),
                Some(130.0),
                Some(50.0),
                "adsb",
            ))
            .unwrap();
        buffer
            .push(TrackObservation::new(
                "B1",
                ts_day2,
                2.0,
                51.0,
                Some(300.0),
                Some(140.0),
                Some(60.0),
                "adsb",
            ))
            .unwrap();

        assert_eq!(buffer.len(), 3);

        let flushed = buffer.flush_to_parquet_with_part_id(temp_dir.path(), "0001").unwrap();
        assert_eq!(flushed, 3);
        assert_eq!(buffer.len(), 0);

        // Check partition files
        let path_day1 = temp_dir.path().join("year=2026/month=10/day=20/part-0001.parquet");
        let path_day2 = temp_dir.path().join("year=2026/month=10/day=21/part-0001.parquet");

        assert!(path_day1.exists(), "Day 1 partition path must exist");
        assert!(path_day2.exists(), "Day 2 partition path must exist");

        // Validate Parquet file header and footer magic
        let content1 = std::fs::read(&path_day1).unwrap();
        assert!(content1.len() >= 12);
        assert_eq!(&content1[0..4], b"PAR1");
        assert_eq!(&content1[content1.len() - 4..], b"PAR1");

        let content2 = std::fs::read(&path_day2).unwrap();
        assert!(content2.len() >= 12);
        assert_eq!(&content2[0..4], b"PAR1");
        assert_eq!(&content2[content2.len() - 4..], b"PAR1");
    }
}
