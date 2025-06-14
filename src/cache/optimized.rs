use crate::cache::lockfree_lru::LockFreeLruCache;
use crate::cache::{CacheEntry, CacheKey, CacheStats};
use crate::dns::DNSPacket;
use crate::pool::StringInterner;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, trace};

/// Optimized DNS cache using lock-free LRU eviction
pub struct OptimizedDnsCache {
    /// Lock-free LRU cache
    cache: LockFreeLruCache<CacheKey, CacheEntry>,
    /// Cache configuration
    negative_ttl: u32,
    /// Statistics
    stats: Arc<CacheStats>,
    /// String interner for domain deduplication
    string_interner: Arc<StringInterner>,
}

impl OptimizedDnsCache {
    pub fn new(max_size: usize, negative_ttl: u32) -> Self {
        Self {
            cache: LockFreeLruCache::new(max_size),
            negative_ttl,
            stats: Arc::new(CacheStats::new()),
            string_interner: Arc::new(StringInterner::new(10000)),
        }
    }

    pub fn with_persistence(max_size: usize, negative_ttl: u32, _cache_file_path: String) -> Self {
        Self {
            cache: LockFreeLruCache::new(max_size),
            negative_ttl,
            stats: Arc::new(CacheStats::new()),
            string_interner: Arc::new(StringInterner::new(10000)),
        }
    }

    /// Get a cached response if it exists and hasn't expired
    pub fn get(&self, key: &CacheKey) -> Option<DNSPacket> {
        match self.cache.get(key) {
            Some(entry) => {
                if let Some(response) = entry.get_response() {
                    self.stats.record_hit();

                    // Track negative cache hits
                    if entry.is_negative {
                        self.stats.record_negative_hit();
                        trace!(
                            "Negative cache hit for domain: {} (RCODE={})",
                            key.domain, response.header.rcode
                        );
                    } else {
                        trace!("Positive cache hit for domain: {}", key.domain);
                    }

                    Some(response)
                } else {
                    // Entry expired
                    self.stats.record_expired_eviction();
                    debug!("Cache entry expired for domain: {}", key.domain);
                    None
                }
            }
            None => {
                self.stats.record_miss();
                trace!("Cache miss for domain: {}", key.domain);
                None
            }
        }
    }

    /// Store a response in the cache
    pub fn put(&self, key: CacheKey, response: DNSPacket) {
        let ttl = self.calculate_ttl(&response);
        let is_negative = self.is_negative_response(&response);

        // RFC 2308 compliant TTL handling
        let final_ttl = if is_negative {
            ttl.min(self.negative_ttl)
        } else if response.header.ancount == 0
            && response.header.nscount == 0
            && !response.header.qr
        {
            self.negative_ttl.min(ttl)
        } else {
            ttl
        };

        if final_ttl == 0 {
            debug!("Not caching response with 0 TTL for domain: {}", key.domain);
            return;
        }

        // Record statistics
        let cache_type = if is_negative {
            match response.header.rcode {
                3 => {
                    self.stats.record_nxdomain_response();
                    "NXDOMAIN"
                }
                0 => {
                    self.stats.record_nodata_response();
                    "NODATA"
                }
                _ => "NEGATIVE",
            }
        } else {
            "POSITIVE"
        };

        let entry = CacheEntry::new(response, final_ttl, is_negative);
        self.cache
            .put(key.clone(), entry, Duration::from_secs(final_ttl as u64));

        debug!(
            "Cached {} response for domain: {} (TTL: {}s)",
            cache_type, key.domain, final_ttl
        );
    }

    /// Calculate the minimum TTL from all records in the response
    fn calculate_ttl(&self, response: &DNSPacket) -> u32 {
        let mut min_ttl = u32::MAX;
        let is_negative = self.is_negative_response(response);

        if is_negative {
            // RFC 2308: For negative responses, use SOA minimum TTL
            for authority in &response.authorities {
                if authority.rtype == crate::dns::enums::DNSResourceType::SOA {
                    let soa_min_ttl = authority.get_soa_minimum().unwrap_or(authority.ttl);
                    min_ttl = min_ttl.min(authority.ttl.min(soa_min_ttl));
                }
            }
        } else {
            // For positive responses, use minimum TTL of all records
            for answer in &response.answers {
                min_ttl = min_ttl.min(answer.ttl);
            }

            if min_ttl == u32::MAX && !response.authorities.is_empty() {
                for authority in &response.authorities {
                    min_ttl = min_ttl.min(authority.ttl);
                }
            }
        }

        if min_ttl == u32::MAX {
            self.negative_ttl
        } else {
            min_ttl
        }
    }

    /// Check if this is a negative response
    fn is_negative_response(&self, response: &DNSPacket) -> bool {
        response.header.rcode == 3 || // NXDOMAIN
        (response.header.rcode == 0 && response.header.ancount == 0) // NODATA
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        self.cache.clear();
        debug!("Cleared all cache entries");
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get string interner
    pub fn string_interner(&self) -> &StringInterner {
        &self.string_interner
    }

    /// Clean up expired entries
    pub fn cleanup_expired(&self) -> usize {
        let count = self.cache.cleanup_expired();
        if count > 0 {
            self.stats.record_expired_evictions(count as u64);
            debug!("Cleaned up {} expired cache entries", count);
        }
        count
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}
