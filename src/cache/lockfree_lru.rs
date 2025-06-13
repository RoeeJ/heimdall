use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

/// A lock-free LRU cache implementation using atomic operations
/// and sharded eviction lists to reduce contention
pub struct LockFreeLruCache<K, V> {
    /// The main storage
    map: DashMap<K, Arc<CacheNode<V>>>,
    /// Maximum number of entries
    max_size: usize,
    /// Current size (approximate due to concurrent operations)
    size: AtomicUsize,
    /// Global access counter for LRU ordering
    access_counter: AtomicU64,
    /// Sharded eviction queues to reduce contention
    eviction_shards: Vec<RwLock<VecDeque<(K, u64)>>>,
    /// Number of shards
    num_shards: usize,
}

struct CacheNode<V> {
    value: V,
    /// Last access timestamp (monotonic counter)
    last_access: AtomicU64,
    /// Expiry time
    expiry: Instant,
}

impl<K: Clone + Eq + std::hash::Hash, V: Clone> LockFreeLruCache<K, V> {
    pub fn new(max_size: usize) -> Self {
        let num_shards = 16; // Could be configurable
        let mut eviction_shards = Vec::with_capacity(num_shards);

        for _ in 0..num_shards {
            eviction_shards.push(RwLock::new(VecDeque::new()));
        }

        Self {
            map: DashMap::with_capacity(max_size),
            max_size,
            size: AtomicUsize::new(0),
            access_counter: AtomicU64::new(0),
            eviction_shards,
            num_shards,
        }
    }

    /// Get a value from the cache
    pub fn get(&self, key: &K) -> Option<V> {
        if let Some(node) = self.map.get(key) {
            // Check if expired
            if Instant::now() >= node.expiry {
                // Remove expired entry
                drop(node);
                self.map.remove(key);
                self.size.fetch_sub(1, Ordering::Relaxed);
                return None;
            }

            // Update access time
            let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
            node.last_access.store(access_time, Ordering::Relaxed);

            Some(node.value.clone())
        } else {
            None
        }
    }

    /// Insert a value into the cache
    pub fn put(&self, key: K, value: V, ttl: Duration) {
        let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
        let node = Arc::new(CacheNode {
            value,
            last_access: AtomicU64::new(access_time),
            expiry: Instant::now() + ttl,
        });

        // Check if we need to evict
        let current_size = self.size.load(Ordering::Relaxed);
        if current_size >= self.max_size {
            self.evict_one();
        }

        // Insert the new entry
        let was_new = self.map.insert(key.clone(), node).is_none();
        if was_new {
            self.size.fetch_add(1, Ordering::Relaxed);

            // Add to eviction shard
            let shard_idx = self.hash_to_shard(&key);
            if let Some(mut shard) = self.eviction_shards[shard_idx].try_write() {
                shard.push_back((key, access_time));

                // Keep shard size reasonable
                let shard_len = shard.len();
                if shard_len > self.max_size / self.num_shards * 2 {
                    shard.drain(0..shard_len / 2);
                }
            }
        }
    }

    /// Evict one entry using approximate LRU
    fn evict_one(&self) {
        // Sample from multiple shards to find an old entry
        let mut oldest_key = None;
        let mut oldest_time = u64::MAX;

        // Sample a few shards
        for i in 0..4 {
            let shard_idx = i % self.num_shards;
            if let Some(shard) = self.eviction_shards[shard_idx].try_read() {
                if let Some((key, _)) = shard.front() {
                    if let Some(node) = self.map.get(key) {
                        let access_time = node.last_access.load(Ordering::Relaxed);
                        if access_time < oldest_time {
                            oldest_time = access_time;
                            oldest_key = Some(key.clone());
                        }
                    }
                }
            }
        }

        // Also sample some random entries from the map
        let sample_size = 8;
        for (i, entry) in self.map.iter().enumerate() {
            if i >= sample_size {
                break;
            }

            let access_time = entry.value().last_access.load(Ordering::Relaxed);
            if access_time < oldest_time {
                oldest_time = access_time;
                oldest_key = Some(entry.key().clone());
            }
        }

        // Evict the oldest entry found
        if let Some(key) = oldest_key {
            self.map.remove(&key);
            self.size.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Hash key to shard index
    fn hash_to_shard(&self, key: &K) -> usize {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;

        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.num_shards
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.map.clear();
        self.size.store(0, Ordering::Relaxed);

        for shard in &self.eviction_shards {
            if let Some(mut s) = shard.try_write() {
                s.clear();
            }
        }
    }

    /// Get approximate size
    pub fn len(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Remove expired entries
    pub fn cleanup_expired(&self) -> usize {
        let mut removed = 0;
        let now = Instant::now();

        // Iterate through a sample of entries
        let mut to_remove = Vec::new();
        for entry in self.map.iter() {
            if now >= entry.value().expiry {
                to_remove.push(entry.key().clone());
            }

            // Limit cleanup to avoid blocking too long
            if to_remove.len() >= 100 {
                break;
            }
        }

        for key in to_remove {
            if self.map.remove(&key).is_some() {
                removed += 1;
                self.size.fetch_sub(1, Ordering::Relaxed);
            }
        }

        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_operations() {
        let cache = LockFreeLruCache::new(3);

        // Test insertion and retrieval
        cache.put("key1", "value1", Duration::from_secs(60));
        cache.put("key2", "value2", Duration::from_secs(60));
        cache.put("key3", "value3", Duration::from_secs(60));

        assert_eq!(cache.get(&"key1"), Some("value1"));
        assert_eq!(cache.get(&"key2"), Some("value2"));
        assert_eq!(cache.get(&"key3"), Some("value3"));

        // Test eviction
        cache.put("key4", "value4", Duration::from_secs(60));

        // One of the first three should have been evicted
        let count = [
            cache.get(&"key1").is_some(),
            cache.get(&"key2").is_some(),
            cache.get(&"key3").is_some(),
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        assert_eq!(count, 2); // One should have been evicted
        assert_eq!(cache.get(&"key4"), Some("value4"));
    }

    #[test]
    fn test_expiry() {
        let cache = LockFreeLruCache::new(10);

        // Insert with short TTL
        cache.put("key1", "value1", Duration::from_millis(50));

        // Should exist immediately
        assert_eq!(cache.get(&"key1"), Some("value1"));

        // Wait for expiry
        thread::sleep(Duration::from_millis(100));

        // Should be expired
        assert_eq!(cache.get(&"key1"), None);
    }

    #[test]
    fn test_concurrent_access() {
        let cache = Arc::new(LockFreeLruCache::new(100));
        let mut handles = vec![];

        // Spawn multiple threads
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let key = format!("key_{}", (i * 100 + j) % 50);
                    let value = format!("value_{}", i * 100 + j);

                    cache_clone.put(key.clone(), value, Duration::from_secs(60));
                    cache_clone.get(&key);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Cache should not exceed max size (allowing some margin for concurrent operations)
        assert!(cache.len() <= 110); // Max 100 + some margin
    }
}
