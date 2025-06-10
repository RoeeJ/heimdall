use super::redis_backend::{CacheBackend, CachedEntry};
use super::{CacheKey, CacheStats};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, trace};

/// Local in-memory cache backend
pub struct LocalCache {
    entries: Arc<DashMap<CacheKey, CachedEntry>>,
    max_size: usize,
    stats: Arc<CacheStats>,
}

impl LocalCache {
    /// Create a new local cache
    pub fn new(max_size: usize, stats: Arc<CacheStats>) -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            max_size,
            stats,
        }
    }

    /// Evict expired entries
    pub fn evict_expired(&self) {
        let now = SystemTime::now();
        let mut expired_count = 0;

        self.entries.retain(|_, entry| {
            if now > entry.expires_at {
                expired_count += 1;
                false
            } else {
                true
            }
        });

        if expired_count > 0 {
            self.stats.record_expired_evictions(expired_count);
            debug!("Evicted {} expired entries from local cache", expired_count);
        }
    }

    /// Evict entries if cache is over capacity (LRU-style)
    fn evict_if_needed(&self) {
        if self.entries.len() > self.max_size {
            // Simple eviction: remove oldest entries
            // In production, we'd want proper LRU tracking
            let to_evict = self.entries.len() - self.max_size;
            let mut evicted = 0;

            // Collect keys to evict
            let keys_to_evict: Vec<CacheKey> = self
                .entries
                .iter()
                .take(to_evict)
                .map(|entry| entry.key().clone())
                .collect();

            // Evict them
            for key in keys_to_evict {
                self.entries.remove(&key);
                evicted += 1;
            }

            if evicted > 0 {
                self.stats.record_evictions(evicted);
                debug!("Evicted {} entries due to cache size limit", evicted);
            }
        }
    }
}

#[async_trait]
impl CacheBackend for LocalCache {
    async fn get(&self, key: &CacheKey) -> Option<CachedEntry> {
        // First check if entry exists and is not expired
        let entry = self.entries.get(key)?;

        if entry.is_expired() {
            // Remove expired entry
            drop(entry); // Release the lock
            self.entries.remove(key);
            self.stats.record_expired_evictions(1);
            None
        } else {
            trace!("Local cache hit for key: {}", key);
            Some(entry.clone())
        }
    }

    async fn set(&self, key: &CacheKey, entry: CachedEntry) {
        // Evict if needed before inserting
        self.evict_if_needed();

        self.entries.insert(key.clone(), entry);
        trace!("Cached entry locally: {}", key);
    }

    async fn remove(&self, key: &CacheKey) {
        self.entries.remove(key);
    }

    async fn clear(&self) {
        let size = self.entries.len();
        self.entries.clear();
        debug!("Cleared {} entries from local cache", size);
    }

    async fn len(&self) -> usize {
        self.entries.len()
    }
}
