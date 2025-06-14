use crate::cache::{CacheEntry, CacheKey, CacheStats};
use crate::cache::lockfree_lru::LockFreeLruCache;
use crate::dns::DNSPacket;
use crate::pool::StringInterner;
use std::sync::Arc;
use tracing::{debug, trace};

/// Lock-free DNS cache implementation for maximum performance
pub struct LockFreeDnsCache {
    cache: LockFreeLruCache<CacheKey, CacheEntry>,
    negative_ttl: u32,
    stats: Arc<CacheStats>,
    string_interner: Arc<StringInterner>,
}

impl LockFreeDnsCache {
    pub fn new(capacity: usize, negative_ttl: u32) -> Self {
        Self {
            cache: LockFreeLruCache::new(capacity),
            negative_ttl,
            stats: Arc::new(CacheStats::new()),
            string_interner: Arc::new(StringInterner::new(10000)),
        }
    }

    /// Get a cached response if it exists and hasn't expired
    pub fn get(&self, key: &CacheKey) -> Option<DNSPacket> {
        if let Some(entry) = self.cache.get(key) {
            if let Some(response) = entry.get_response() {
                self.stats.record_hit();
                
                if entry.is_negative {
                    self.stats.record_negative_hit();
                    trace!(
                        "Negative cache hit for domain: {} (RCODE={})",
                        key.domain, response.header.rcode
                    );
                } else {
                    trace!("Positive cache hit for domain: {}", key.domain);
                }
                
                return Some(response);
            } else {
                // Entry expired, stats are updated but we don't actively remove
                // The LRU will handle eviction when capacity is reached
                self.stats.record_expired_eviction();
                self.stats.record_miss(); // Also count as miss since we can't use it
                debug!("Expired cache entry for domain: {}", key.domain);
                return None;
            }
        }
        
        self.stats.record_miss();
        trace!("Cache miss for domain: {}", key.domain);
        None
    }

    /// Insert a response into the cache
    pub fn insert(&self, key: CacheKey, entry: CacheEntry) {
        trace!(
            "Inserting {} cache entry for domain: {}, TTL: {}",
            if entry.is_negative { "negative" } else { "positive" },
            key.domain,
            entry.original_ttl
        );
        
        // Track NXDOMAIN and NODATA responses for statistics
        if entry.is_negative {
            match entry.response.header.rcode {
                3 => self.stats.record_nxdomain_response(),
                _ => self.stats.record_nodata_response(),
            }
        }
        
        // Use the entry's TTL for the cache duration
        let ttl = std::time::Duration::from_secs(entry.original_ttl as u64);
        self.cache.put(key, entry, ttl);
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get current cache size
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.cache.clear();
        debug!("Cache cleared");
    }

    /// Get the string interner for domain normalization
    pub fn interner(&self) -> &StringInterner {
        &self.string_interner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::enums::{DNSResourceClass, DNSResourceType};

    #[test]
    fn test_lockfree_cache_basic() {
        let cache = LockFreeDnsCache::new(100, 300);
        
        // Create a test key
        let key = CacheKey::new(
            "example.com".to_string(),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        
        // Create a test packet
        let mut packet = DNSPacket::default();
        packet.header.id = 12345;
        
        // Create and insert entry
        let entry = CacheEntry::new(packet.clone(), 300, false);
        cache.insert(key.clone(), entry);
        
        // Verify we can retrieve it
        let retrieved = cache.get(&key).unwrap();
        assert_eq!(retrieved.header.id, 12345);
        
        // Check stats
        assert_eq!(cache.stats().hits.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(cache.stats().misses.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn test_lockfree_cache_expiry() {
        let cache = LockFreeDnsCache::new(100, 300);
        
        let key = CacheKey::new(
            "expired.com".to_string(),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        
        let packet = DNSPacket::default();
        
        // Create an entry with very short TTL
        let entry = CacheEntry::new(packet, 1, false);
        cache.insert(key.clone(), entry);
        
        // Wait for it to expire
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        // Should get None for expired entry
        let result = cache.get(&key);
        assert!(result.is_none());
        
        // Check that it was recorded as a miss
        // Note: The LRU cache handles expiry internally, so we only see a miss
        assert_eq!(cache.stats().misses.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
}