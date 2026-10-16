//! Time buckets: vessels grouped by when they were last heard, so expiry and
//! least-recently-heard eviction touch only the vessels that leave, never every vessel.

use std::collections::{BTreeMap, HashSet};

use jiff::Timestamp;

use super::Mmsi;

/// Groups vessels by the bucket (a fixed span of seconds) of a timestamp. Each vessel is
/// in at most one bucket; moving it is O(1), and dropping everything older than a
/// cutoff visits only the buckets that expire.
#[derive(Debug)]
pub(super) struct TimeBuckets {
    width_s: i64,
    buckets: BTreeMap<i64, HashSet<Mmsi>>,
}

impl TimeBuckets {
    pub(super) fn new(width_s: i64) -> Self {
        Self { width_s: width_s.max(1), buckets: BTreeMap::new() }
    }

    pub(super) fn key(&self, time: Timestamp) -> i64 {
        time.as_second().div_euclid(self.width_s)
    }

    pub(super) fn insert(&mut self, mmsi: Mmsi, key: i64) {
        self.buckets.entry(key).or_default().insert(mmsi);
    }

    pub(super) fn remove(&mut self, mmsi: Mmsi, key: i64) {
        if let Some(bucket) = self.buckets.get_mut(&key) {
            bucket.remove(&mmsi);
            if bucket.is_empty() {
                self.buckets.remove(&key);
            }
        }
    }

    /// Moves `mmsi` from bucket `from` (if any) to `to`.
    pub(super) fn relocate(&mut self, mmsi: Mmsi, from: Option<i64>, to: i64) {
        if from == Some(to) {
            return;
        }
        if let Some(from) = from {
            self.remove(mmsi, from);
        }
        self.insert(mmsi, to);
    }

    /// Removes and returns one vessel from the oldest bucket.
    pub(super) fn pop_oldest(&mut self) -> Option<Mmsi> {
        let mut oldest = self.buckets.first_entry()?;
        let mmsi = oldest.get().iter().next().copied();
        if let Some(mmsi) = mmsi {
            oldest.get_mut().remove(&mmsi);
        }
        if oldest.get().is_empty() {
            oldest.remove();
        }
        mmsi
    }

    /// Removes every bucket whose whole span is older than `cutoff` and returns the
    /// vessels it held. Vessels in the bucket that contains `cutoff` stay until that
    /// bucket expires, so expiry is late by at most one bucket width.
    pub(super) fn take_older_than(&mut self, cutoff: Timestamp) -> Vec<Mmsi> {
        let keep = self.buckets.split_off(&self.key(cutoff));
        let expired = std::mem::replace(&mut self.buckets, keep);
        expired.into_values().flatten().collect()
    }

    /// Vessels from the newest bucket to the oldest; ordered by MMSI inside a bucket so
    /// the result does not depend on hashing.
    pub(super) fn newest_first(&self) -> impl Iterator<Item = Mmsi> + '_ {
        self.buckets.values().rev().flat_map(|bucket| {
            let mut members: Vec<Mmsi> = bucket.iter().copied().collect();
            members.sort_unstable();
            members
        })
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.buckets.values().map(HashSet::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).unwrap()
    }

    fn mmsi(n: u32) -> Mmsi {
        Mmsi::from_number(u64::from(n)).unwrap()
    }

    #[test]
    fn expiry_drops_only_whole_buckets_older_than_the_cutoff() {
        let mut buckets = TimeBuckets::new(60);
        for (n, second) in [(1, 0), (2, 59), (3, 60), (4, 150)] {
            buckets.insert(mmsi(n), buckets.key(at(second)));
        }
        let mut expired = buckets.take_older_than(at(90));
        expired.sort_unstable();
        assert_eq!(expired, vec![mmsi(1), mmsi(2)]);
        assert_eq!(buckets.len(), 2, "the bucket holding the cutoff is kept");
        assert_eq!(buckets.take_older_than(at(119)), Vec::<Mmsi>::new());
        assert_eq!(buckets.take_older_than(at(120)), vec![mmsi(3)]);
    }

    #[test]
    fn relocation_keeps_each_vessel_in_one_bucket_and_pop_takes_the_oldest() {
        let mut buckets = TimeBuckets::new(10);
        buckets.insert(mmsi(1), 0);
        buckets.insert(mmsi(2), 1);
        buckets.relocate(mmsi(1), Some(0), 5);
        buckets.relocate(mmsi(1), Some(5), 5);
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets.newest_first().collect::<Vec<_>>(), vec![mmsi(1), mmsi(2)]);
        assert_eq!(buckets.pop_oldest(), Some(mmsi(2)));
        assert_eq!(buckets.pop_oldest(), Some(mmsi(1)));
        assert_eq!(buckets.pop_oldest(), None);
    }
}
