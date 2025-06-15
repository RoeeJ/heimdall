use super::{CacheEntry, CacheKey, CacheStats, LockFreeDnsCache};
use crate::dns::DNSPacket;
use crate::pool::StringInterner;
use std::sync::Arc;
use tracing::{debug, trace};

/// Optimized DNS cache that uses lock-free data structures and cache-line optimization
pub struct OptimizedDnsCache {
    /// Primary storage using lock-free LRU
    primary: Arc<LockFreeDnsCache>,
    /// Hot entries cache (frequently accessed items)
    hot_cache: dashmap::DashMap<CacheKey, Arc<CacheEntry>>,
    /// Configuration
    max_size: usize,
    negative_ttl: u32,
    hot_cache_size: usize,
    /// Cache persistence path
    cache_file_path: Option<String>,
    /// Shared string interner
    string_interner: Arc<StringInterner>,
    /// Access tracking for hot cache promotion
    access_tracker: Arc<AccessTracker>,
}

/// Tracks access patterns for cache promotion
struct AccessTracker {
    /// Access counts for keys
    counts: dashmap::DashMap<CacheKey, u32>,
    /// Threshold for promotion to hot cache
    promotion_threshold: u32,
}

impl AccessTracker {
    fn new(promotion_threshold: u32) -> Self {
        Self {
            counts: dashmap::DashMap::new(),
            promotion_threshold,
        }
    }

    fn record_access(&self, key: &CacheKey) -> bool {
        let mut count = self.counts.entry(key.clone()).or_insert(0);
        *count += 1;
        *count >= self.promotion_threshold
    }

    fn reset(&self, key: &CacheKey) {
        self.counts.remove(key);
    }
}

impl OptimizedDnsCache {
    pub fn new(max_size: usize, negative_ttl: u32) -> Self {
        let hot_cache_size = (max_size / 10).max(100); // 10% for hot cache
        let primary_size = max_size - hot_cache_size;

        Self {
            primary: Arc::new(LockFreeDnsCache::new(primary_size, negative_ttl)),
            hot_cache: dashmap::DashMap::with_capacity(hot_cache_size),
            max_size,
            negative_ttl,
            hot_cache_size,
            cache_file_path: None,
            string_interner: Arc::new(StringInterner::new(10000)),
            access_tracker: Arc::new(AccessTracker::new(3)), // Promote after 3 accesses
        }
    }

    pub fn with_persistence(max_size: usize, negative_ttl: u32, cache_file_path: String) -> Self {
        let mut cache = Self::new(max_size, negative_ttl);
        cache.cache_file_path = Some(cache_file_path);
        cache
    }

    /// Get a cached response with hot cache optimization
    pub fn get(&self, key: &CacheKey) -> Option<DNSPacket> {
        // Check hot cache first (most frequently accessed)
        if let Some(entry) = self.hot_cache.get(key) {
            if let Some(response) = entry.get_response() {
                self.primary.stats().record_hit();
                trace!("Hot cache hit for domain: {}", key.domain);
                return Some(response);
            } else {
                // Expired in hot cache, remove it
                drop(entry);
                self.hot_cache.remove(key);
            }
        }

        // Check primary cache
        if let Some(response) = self.primary.get(key) {
            // Track access for potential promotion
            if self.access_tracker.record_access(key) {
                // Promote to hot cache
                self.promote_to_hot_cache(key, &response);
            }
            return Some(response);
        }

        None
    }

    /// Insert a response into the cache
    pub fn insert(&self, key: CacheKey, response: DNSPacket, ttl: u32, is_negative: bool) {
        let entry = CacheEntry::new(response, ttl, is_negative);

        // Always insert into primary cache
        self.primary.insert(key.clone(), entry.clone());

        // Reset access tracking for new entries
        self.access_tracker.reset(&key);
    }

    /// Put a response into the cache (wrapper for compatibility)
    pub fn put(&self, key: CacheKey, response: DNSPacket) {
        // Calculate TTL from response
        let ttl = self.calculate_ttl(&response);
        let is_negative = self.is_negative_response(&response);

        self.insert(key, response, ttl, is_negative);
    }

    /// Calculate TTL from response
    fn calculate_ttl(&self, response: &DNSPacket) -> u32 {
        let mut min_ttl = u32::MAX;

        // Check answer records
        for answer in &response.answers {
            min_ttl = min_ttl.min(answer.ttl);
        }

        // Check authority records
        for authority in &response.authorities {
            min_ttl = min_ttl.min(authority.ttl);
        }

        // Use default if no records found
        if min_ttl == u32::MAX {
            self.negative_ttl
        } else {
            min_ttl
        }
    }

    /// Check if response is negative (NXDOMAIN or NODATA)
    fn is_negative_response(&self, response: &DNSPacket) -> bool {
        response.header.rcode == 3 || (response.header.rcode == 0 && response.header.ancount == 0)
    }

    /// Promote an entry to the hot cache
    fn promote_to_hot_cache(&self, key: &CacheKey, response: &DNSPacket) {
        // Check if hot cache is full
        if self.hot_cache.len() >= self.hot_cache_size {
            // Simple eviction: remove a random entry
            // In production, we'd want LRU or LFU here too
            if let Some(evict_key) = self.hot_cache.iter().next().map(|e| e.key().clone()) {
                self.hot_cache.remove(&evict_key);
            }
        }

        // Create entry for hot cache
        let remaining_ttl = response
            .answers
            .first()
            .map(|a| a.ttl)
            .unwrap_or(self.negative_ttl);

        let entry = CacheEntry::new(response.clone(), remaining_ttl, false);
        self.hot_cache.insert(key.clone(), Arc::new(entry));

        debug!("Promoted {} to hot cache", key.domain);
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        self.primary.stats()
    }

    /// Get current cache size
    pub fn size(&self) -> usize {
        self.primary.size() + self.hot_cache.len()
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.primary.clear();
        self.hot_cache.clear();
        self.access_tracker.counts.clear();
        debug!("Cache cleared (including hot cache)");
    }

    /// Get the string interner
    pub fn string_interner(&self) -> &StringInterner {
        &self.string_interner
    }

    /// Get debug info for the cache
    pub fn debug_info(&self) -> String {
        let stats = self.stats();
        format!(
            "OptimizedCache: primary_size={}/{}, hot_cache={}/{}, hits={}, misses={}, hit_rate={:.2}%",
            self.primary.size(),
            self.max_size - self.hot_cache_size,
            self.hot_cache.len(),
            self.hot_cache_size,
            stats.hits.load(std::sync::atomic::Ordering::Relaxed),
            stats.misses.load(std::sync::atomic::Ordering::Relaxed),
            stats.hit_rate() * 100.0
        )
    }

    /// Cleanup expired entries
    pub fn cleanup_expired(&self) {
        // The primary cache (LockFreeDnsCache) handles expiry during get operations
        // so we only need to cleanup the hot cache

        // Cleanup hot cache
        let mut expired_keys = Vec::new();
        for entry_ref in self.hot_cache.iter() {
            if entry_ref.value().is_expired() {
                expired_keys.push(entry_ref.key().clone());
            }
        }

        for key in expired_keys {
            self.hot_cache.remove(&key);
        }
    }

    /// Save cache to disk if persistence is enabled
    pub async fn save_to_disk(&self) -> Result<(), std::io::Error> {
        if let Some(_path) = &self.cache_file_path {
            // TODO: Implement cache persistence
            debug!("Cache persistence not yet implemented for optimized cache");
            Ok(())
        } else {
            Ok(())
        }
    }

    /// Load cache from disk if persistence is enabled
    pub async fn load_from_disk(&self) -> Result<(), std::io::Error> {
        if let Some(_path) = &self.cache_file_path {
            // TODO: Implement cache loading
            debug!("Cache loading not yet implemented for optimized cache");
            Ok(())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::enums::{DNSResourceClass, DNSResourceType};

    #[test]
    fn test_hot_cache_promotion() {
        let cache = OptimizedDnsCache::new(1000, 300);

        // Create test key
        let key = CacheKey::new(
            "hot.example.com".to_string(),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );

        // Create test packet
        let mut packet = DNSPacket::default();
        packet.header.id = 12345;

        // Insert into cache
        cache.insert(key.clone(), packet.clone(), 300, false);

        // Access multiple times to trigger promotion
        for _ in 0..3 {
            assert!(cache.get(&key).is_some());
        }

        // Verify it's in hot cache
        assert!(cache.hot_cache.contains_key(&key));
    }
}
