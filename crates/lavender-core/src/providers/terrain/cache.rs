//! Bounded in-memory LRU cache for terrain point height observations.

use std::collections::HashMap;
use std::sync::RwLock;

use lavender_domain::TerrainPointResult;

/// A cached terrain observation with ingestion timestamp.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainCacheEntry {
    /// Ingestion timestamp in epoch milliseconds.
    pub at_ms: i64,
    /// Resolved terrain point result.
    pub result: TerrainPointResult,
}

#[derive(Debug)]
struct LruNode {
    key: String,
    entry: TerrainCacheEntry,
    prev: Option<usize>,
    next: Option<usize>,
}

#[derive(Debug)]
struct CacheInner {
    nodes: Vec<Option<LruNode>>,
    map: HashMap<String, usize>,
    free_indices: Vec<usize>,
    head: Option<usize>, // Most recently used
    tail: Option<usize>, // Least recently used
}

impl CacheInner {
    fn new(capacity: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(capacity),
            map: HashMap::with_capacity(capacity),
            free_indices: Vec::new(),
            head: None,
            tail: None,
        }
    }

    fn detach(&mut self, idx: usize) {
        let (prev, next) = if let Some(node) = self.nodes.get_mut(idx).and_then(Option::as_mut) {
            (node.prev.take(), node.next.take())
        } else {
            return;
        };

        if let Some(p) = prev {
            if let Some(prev_node) = self.nodes.get_mut(p).and_then(Option::as_mut) {
                prev_node.next = next;
            }
        } else {
            self.head = next;
        }

        if let Some(n) = next {
            if let Some(next_node) = self.nodes.get_mut(n).and_then(Option::as_mut) {
                next_node.prev = prev;
            }
        } else {
            self.tail = prev;
        }
    }

    fn attach_head(&mut self, idx: usize) {
        let old_head = self.head;
        if let Some(node) = self.nodes.get_mut(idx).and_then(Option::as_mut) {
            node.prev = None;
            node.next = old_head;
        } else {
            return;
        }

        if let Some(h) = old_head {
            if let Some(head_node) = self.nodes.get_mut(h).and_then(Option::as_mut) {
                head_node.prev = Some(idx);
            }
        } else {
            self.tail = Some(idx);
        }

        self.head = Some(idx);
    }

    fn move_to_head(&mut self, idx: usize) {
        if self.head == Some(idx) {
            return;
        }
        self.detach(idx);
        self.attach_head(idx);
    }

    fn evict_tail(&mut self) -> Option<String> {
        let tail_idx = self.tail?;
        let evicted_key = self.nodes[tail_idx].as_ref()?.key.clone();
        self.detach(tail_idx);
        self.map.remove(&evicted_key);
        self.nodes[tail_idx] = None;
        self.free_indices.push(tail_idx);
        Some(evicted_key)
    }

    fn insert(&mut self, key: String, result: TerrainPointResult, at_ms: i64, max_capacity: usize) {
        if let Some(&existing_idx) = self.map.get(&key) {
            if let Some(node) = self.nodes[existing_idx].as_mut() {
                node.entry.result = result;
                node.entry.at_ms = at_ms;
            }
            self.move_to_head(existing_idx);
            return;
        }

        if self.map.len() >= max_capacity {
            self.evict_tail();
        }

        let idx = if let Some(free_idx) = self.free_indices.pop() {
            free_idx
        } else {
            let new_idx = self.nodes.len();
            self.nodes.push(None);
            new_idx
        };

        self.nodes[idx] = Some(LruNode {
            key: key.clone(),
            entry: TerrainCacheEntry { at_ms, result },
            prev: None,
            next: None,
        });

        self.map.insert(key, idx);
        self.attach_head(idx);
    }

    fn get(&mut self, key: &str, now_ms: i64, ttl_ms: i64) -> Option<(TerrainPointResult, bool)> {
        let &idx = self.map.get(key)?;
        self.move_to_head(idx);

        let node = self.nodes[idx].as_ref()?;
        let is_stale = (now_ms - node.entry.at_ms) >= ttl_ms;
        Some((node.entry.result.clone(), is_stale))
    }
}

/// Thread-safe bounded LRU cache for terrain point heights.
#[derive(Debug)]
pub struct TerrainLruCache {
    max_capacity: usize,
    inner: RwLock<CacheInner>,
}

impl TerrainLruCache {
    /// Create a new bounded LRU cache with fixed maximum item capacity.
    #[must_use]
    pub fn new(max_capacity: usize) -> Self {
        let cap = max_capacity.max(1);
        Self { max_capacity: cap, inner: RwLock::new(CacheInner::new(cap)) }
    }

    /// Insert or update an entry in the cache. Evicts the least-recently used entry if at capacity.
    pub fn insert(&self, key: String, result: TerrainPointResult, at_ms: i64) {
        if let Ok(mut inner) = self.inner.write() {
            inner.insert(key, result, at_ms, self.max_capacity);
        }
    }

    /// Retrieve an entry by canonical key. Returns the result and whether it is stale per `ttl_ms`.
    #[must_use]
    pub fn get(&self, key: &str, now_ms: i64, ttl_ms: i64) -> Option<(TerrainPointResult, bool)> {
        let mut inner = self.inner.write().ok()?;
        inner.get(key, now_ms, ttl_ms)
    }

    /// Get current number of stored entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.read().map_or(0, |i| i.map.len())
    }

    /// Check if the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Maximum capacity of this cache.
    #[must_use]
    pub fn max_capacity(&self) -> usize {
        self.max_capacity
    }

    /// Clear all entries from the cache.
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.nodes.clear();
            inner.map.clear();
            inner.free_indices.clear();
            inner.head = None;
            inner.tail = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result(id: &str, ellipsoid: f64) -> TerrainPointResult {
        TerrainPointResult {
            id: Some(id.to_string()),
            ellipsoid,
            elevation: Some(ellipsoid - 10.0),
            geoid: Some(10.0),
        }
    }

    #[test]
    fn cache_capacity_strictly_bounded_and_evicts_lru() {
        let cache = TerrainLruCache::new(3);

        cache.insert("p1".into(), sample_result("1", 100.0), 1000);
        cache.insert("p2".into(), sample_result("2", 200.0), 1000);
        cache.insert("p3".into(), sample_result("3", 300.0), 1000);

        assert_eq!(cache.len(), 3);

        // Access p1 to make it most recently used (order: p1, p3, p2)
        let (r1, _) = cache.get("p1", 1000, 10_000).unwrap();
        assert_eq!(r1.ellipsoid, 100.0);

        // Insert p4 -> causes eviction of p2 (least recently used)
        cache.insert("p4".into(), sample_result("4", 400.0), 1000);
        assert_eq!(cache.len(), 3);
        assert!(cache.get("p2", 1000, 10_000).is_none());
        assert!(cache.get("p1", 1000, 10_000).is_some());
        assert!(cache.get("p3", 1000, 10_000).is_some());
        assert!(cache.get("p4", 1000, 10_000).is_some());
    }

    #[test]
    fn staleness_reported_correctly() {
        let cache = TerrainLruCache::new(10);
        cache.insert("p1".into(), sample_result("1", 50.0), 1_000);

        // Fresh within 10s TTL
        let (_, is_stale) = cache.get("p1", 5_000, 10_000).unwrap();
        assert!(!is_stale);

        // Stale beyond 10s TTL
        let (_, is_stale) = cache.get("p1", 15_000, 10_000).unwrap();
        assert!(is_stale);
    }

    #[test]
    fn duplicate_insert_updates_value_and_promotes_mru() {
        let cache = TerrainLruCache::new(2);
        cache.insert("p1".into(), sample_result("1", 50.0), 1_000);
        cache.insert("p2".into(), sample_result("2", 60.0), 1_000);

        // Update p1 with new elevation
        cache.insert("p1".into(), sample_result("1", 99.0), 2_000);

        // Insert p3 -> should evict p2, keeping p1
        cache.insert("p3".into(), sample_result("3", 70.0), 3_000);
        assert_eq!(cache.len(), 2);
        assert!(cache.get("p2", 3_000, 10_000).is_none());
        let (r1, _) = cache.get("p1", 3_000, 10_000).unwrap();
        assert_eq!(r1.ellipsoid, 99.0);
    }
}
