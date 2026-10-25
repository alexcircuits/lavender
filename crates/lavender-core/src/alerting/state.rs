//! Bounded LRU state storage for entity geofence containment history.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// A memory-bounded LRU cache backed by generational sequence numbers and a deque.
#[derive(Debug)]
pub struct BoundedLru<K, V> {
    entries: HashMap<K, (V, u64)>,
    order: VecDeque<(K, u64)>,
    capacity: usize,
    seq: u64,
}

impl<K: Clone + Eq + Hash, V> BoundedLru<K, V> {
    /// Creates a new bounded LRU with the specified maximum capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity.min(1024)),
            order: VecDeque::with_capacity(capacity.min(1024)),
            capacity: capacity.max(1),
            seq: 0,
        }
    }

    /// Looks up a reference to the value for `key` without altering LRU order.
    #[must_use]
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|(v, _)| v)
    }

    /// Retrieves a reference to the value for `key` and marks it as recently used.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;
        if let Some((val, entry_seq)) = self.entries.get_mut(key) {
            *entry_seq = seq;
            self.order.push_back((key.clone(), seq));
            Some(val)
        } else {
            None
        }
    }

    /// Inserts or replaces a key-value pair, evicting the least recently used entry
    /// if the capacity is exceeded.
    pub fn insert(&mut self, key: K, val: V) {
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;
        self.entries.insert(key.clone(), (val, seq));
        self.order.push_back((key, seq));
        self.evict_if_needed();
    }

    /// Removes an entry by key if present.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.entries.remove(key).map(|(v, _)| v)
    }

    /// Current number of retained entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache contains zero entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clears all entries and the eviction queue.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn evict_if_needed(&mut self) {
        while self.entries.len() > self.capacity {
            if let Some((candidate_key, candidate_seq)) = self.order.pop_front() {
                if let Some((_, actual_seq)) = self.entries.get(&candidate_key)
                    && *actual_seq == candidate_seq
                {
                    self.entries.remove(&candidate_key);
                }
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_peek() {
        let mut lru = BoundedLru::new(3);
        lru.insert("a", 1);
        lru.insert("b", 2);

        assert_eq!(lru.peek(&"a"), Some(&1));
        assert_eq!(lru.peek(&"b"), Some(&2));
        assert_eq!(lru.peek(&"c"), None);
        assert_eq!(lru.len(), 2);
    }

    #[test]
    fn eviction_preserves_capacity_and_drops_lru() {
        let mut lru = BoundedLru::new(2);
        lru.insert("k1", 10);
        lru.insert("k2", 20);
        assert_eq!(lru.len(), 2);

        // Access k1 so k2 becomes least recently used
        let _ = lru.get_mut(&"k1");

        // Insert k3 -> should evict k2
        lru.insert("k3", 30);
        assert_eq!(lru.len(), 2);
        assert_eq!(lru.peek(&"k1"), Some(&10));
        assert_eq!(lru.peek(&"k3"), Some(&30));
        assert_eq!(lru.peek(&"k2"), None);
    }
}
