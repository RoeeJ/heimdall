use super::{CacheKey, DnsCache, OptimizedDnsCache};
use crate::dns::DNSPacket;
use crate::pool::StringInterner;

/// Wrapper enum to support both standard and optimized cache implementations
pub enum CacheWrapper {
    Standard(DnsCache),
    Optimized(OptimizedDnsCache),
}

impl CacheWrapper {
    /// Create a new cache wrapper based on configuration
    pub fn new(
        use_optimized: bool,
        max_size: usize,
        negative_ttl: u32,
        cache_file_path: Option<String>,
    ) -> Self {
        if use_optimized {
            CacheWrapper::Optimized(OptimizedDnsCache::new(max_size, negative_ttl))
        } else if let Some(path) = cache_file_path {
            CacheWrapper::Standard(DnsCache::with_persistence(max_size, negative_ttl, path))
        } else {
            CacheWrapper::Standard(DnsCache::new(max_size, negative_ttl))
        }
    }

    /// Get a cached response
    pub fn get(&self, key: &CacheKey) -> Option<DNSPacket> {
        match self {
            CacheWrapper::Standard(cache) => cache.get(key),
            CacheWrapper::Optimized(cache) => cache.get(key),
        }
    }

    /// Store a response in cache
    pub fn put(&self, key: CacheKey, response: DNSPacket) {
        match self {
            CacheWrapper::Standard(cache) => cache.put(key, response),
            CacheWrapper::Optimized(cache) => cache.put(key, response),
        }
    }

    /// Get string interner
    pub fn string_interner(&self) -> &StringInterner {
        match self {
            CacheWrapper::Standard(cache) => cache.string_interner(),
            CacheWrapper::Optimized(cache) => cache.string_interner(),
        }
    }

    /// Get cache debug info
    pub fn debug_info(&self) -> String {
        match self {
            CacheWrapper::Standard(cache) => cache.debug_info(),
            CacheWrapper::Optimized(cache) => cache.debug_info(),
        }
    }

    /// Get cache stats
    pub fn stats(&self) -> &crate::cache::CacheStats {
        match self {
            CacheWrapper::Standard(cache) => cache.stats(),
            CacheWrapper::Optimized(cache) => cache.stats(),
        }
    }

    /// Get cache size
    pub fn size(&self) -> usize {
        match self {
            CacheWrapper::Standard(cache) => cache.size(),
            CacheWrapper::Optimized(cache) => cache.size(),
        }
    }

    /// Clear the cache
    pub fn clear(&self) {
        match self {
            CacheWrapper::Standard(cache) => cache.clear(),
            CacheWrapper::Optimized(cache) => cache.clear(),
        }
    }

    /// Cleanup expired entries
    pub fn cleanup_expired(&self) {
        match self {
            CacheWrapper::Standard(cache) => cache.cleanup_expired(),
            CacheWrapper::Optimized(cache) => cache.cleanup_expired(),
        }
    }

    /// Save to disk (only for standard cache)
    pub async fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            CacheWrapper::Standard(cache) => cache.save_to_disk().await,
            CacheWrapper::Optimized(_) => Ok(()), // TODO: Implement persistence for optimized cache
        }
    }

    /// Load from disk (only for standard cache)
    pub async fn load_from_disk(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            CacheWrapper::Standard(cache) => cache.load_from_disk().await,
            CacheWrapper::Optimized(_) => Ok(()), // TODO: Implement persistence for optimized cache
        }
    }

    /// Check if cache has persistence enabled
    pub fn has_persistence(&self) -> bool {
        match self {
            CacheWrapper::Standard(cache) => cache.has_persistence(),
            CacheWrapper::Optimized(_) => false, // TODO: Implement persistence for optimized cache
        }
    }

    /// Get cache file path
    pub fn cache_file_path(&self) -> Option<&str> {
        match self {
            CacheWrapper::Standard(cache) => cache.cache_file_path(),
            CacheWrapper::Optimized(_) => None, // TODO: Implement persistence for optimized cache
        }
    }
}
