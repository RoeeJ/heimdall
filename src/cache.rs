use crate::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
};
use dashmap::DashMap;
use parking_lot::Mutex;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, trace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    pub domain: String,
    pub record_type: DNSResourceType,
    pub record_class: DNSResourceClass,
    /// Pre-computed hash for faster lookups
    hash: u64,
}

impl Hash for CacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl CacheKey {
    pub fn new(
        domain: String,
        record_type: DNSResourceType,
        record_class: DNSResourceClass,
    ) -> Self {
        let normalized_domain = domain.to_lowercase(); // DNS is case-insensitive

        // Pre-compute hash for faster lookups
        let mut hasher = DefaultHasher::new();
        normalized_domain.hash(&mut hasher);
        record_type.hash(&mut hasher);
        record_class.hash(&mut hasher);
        let hash = hasher.finish();

        Self {
            domain: normalized_domain,
            record_type,
            record_class,
            hash,
        }
    }

    pub fn from_question(question: &crate::dns::question::DNSQuestion) -> Self {
        // Optimized domain construction - avoid intermediate vector allocation
        let mut domain = String::with_capacity(256); // Pre-allocate reasonable capacity
        let mut first = true;

        for label in question.labels.iter() {
            if !label.is_empty() {
                if !first {
                    domain.push('.');
                }
                domain.push_str(label);
                first = false;
            }
        }

        Self::new(domain, question.qtype, question.qclass)
    }

    /// Fast domain comparison for prefix matching
    pub fn domain_matches_suffix(&self, suffix: &str) -> bool {
        if self.domain.len() < suffix.len() {
            return false;
        }

        let suffix_lower = suffix.to_lowercase();

        // Check if domain ends with suffix
        if self.domain.ends_with(&suffix_lower) {
            // Ensure it's a proper domain boundary (not partial match)
            let prefix_len = self.domain.len() - suffix_lower.len();
            prefix_len == 0 || self.domain.chars().nth(prefix_len - 1) == Some('.')
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub response: DNSPacket,
    pub expiry: Instant,
    pub original_ttl: u32,
    pub is_negative: bool, // For NXDOMAIN/NODATA responses
}

impl CacheEntry {
    pub fn new(response: DNSPacket, ttl: u32, is_negative: bool) -> Self {
        Self {
            response,
            expiry: Instant::now() + Duration::from_secs(ttl as u64),
            original_ttl: ttl,
            is_negative,
        }
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expiry
    }

    pub fn remaining_ttl(&self) -> u32 {
        if self.is_expired() {
            0
        } else {
            self.expiry.duration_since(Instant::now()).as_secs() as u32
        }
    }

    /// Get a copy of the response with adjusted TTLs
    pub fn get_response(&self) -> Option<DNSPacket> {
        if self.is_expired() {
            return None;
        }

        let remaining_ttl = self.remaining_ttl();
        let mut response = self.response.clone();

        // Adjust TTLs in all answer records
        for answer in &mut response.answers {
            answer.ttl = remaining_ttl;
        }

        // Adjust TTLs in authority records
        for authority in &mut response.authorities {
            authority.ttl = remaining_ttl;
        }

        // Adjust TTLs in additional records
        for additional in &mut response.resources {
            additional.ttl = remaining_ttl;
        }

        Some(response)
    }
}

/// Simple domain trie for efficient domain prefix matching
#[derive(Debug)]
struct DomainTrie {
    children: HashMap<String, DomainTrie>,
    is_terminal: bool,
    cache_keys: Vec<CacheKey>,
}

impl DomainTrie {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            is_terminal: false,
            cache_keys: Vec::new(),
        }
    }

    /// Insert a domain into the trie (in reverse order for efficient suffix matching)
    fn insert(&mut self, domain: &str, cache_key: CacheKey) {
        let parts: Vec<&str> = domain.split('.').rev().collect(); // Reverse for suffix matching
        let mut current = self;

        for part in parts {
            current = current
                .children
                .entry(part.to_string())
                .or_insert_with(DomainTrie::new);
        }

        current.is_terminal = true;
        current.cache_keys.push(cache_key);
    }

    /// Find all cache keys that match a domain suffix
    fn find_matching_keys(&self, domain: &str) -> Vec<&CacheKey> {
        let parts: Vec<&str> = domain.split('.').rev().collect();
        let mut current = self;
        let mut results = Vec::new();

        // Traverse the trie following the domain path
        for part in parts {
            if let Some(child) = current.children.get(part) {
                current = child;
                // Collect all cache keys at this level
                results.extend(&current.cache_keys);
            } else {
                break;
            }
        }

        results
    }

    /// Remove expired cache keys from the trie
    fn cleanup_expired(&mut self) {
        self.cache_keys.clear(); // Will be repopulated as cache is rebuilt
        for child in self.children.values_mut() {
            child.cleanup_expired();
        }
    }
}

#[derive(Debug)]
pub struct CacheStats {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub evictions: AtomicU64,
    pub expired_evictions: AtomicU64,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            expired_evictions: AtomicU64::new(0),
        }
    }

    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_expired_eviction(&self) {
        self.expired_evictions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

#[derive(Debug)]
pub struct DnsCache {
    cache: DashMap<CacheKey, CacheEntry>,
    max_size: usize,
    negative_ttl: u32,
    stats: CacheStats,
    insertion_order: Mutex<Vec<CacheKey>>, // For LRU eviction
    domain_trie: Mutex<DomainTrie>,        // For efficient domain lookups
}

impl DnsCache {
    pub fn new(max_size: usize, negative_ttl: u32) -> Self {
        Self {
            cache: DashMap::new(),
            max_size,
            negative_ttl,
            stats: CacheStats::new(),
            insertion_order: Mutex::new(Vec::new()),
            domain_trie: Mutex::new(DomainTrie::new()),
        }
    }

    /// Get a cached response if it exists and hasn't expired
    pub fn get(&self, key: &CacheKey) -> Option<DNSPacket> {
        if let Some(entry) = self.cache.get(key) {
            if let Some(response) = entry.get_response() {
                self.stats.record_hit();
                trace!("Cache hit for domain: {}", key.domain);
                return Some(response);
            } else {
                // Entry expired, remove it
                drop(entry); // Release the read lock
                self.cache.remove(key);
                self.stats.record_expired_eviction();
                debug!("Removed expired cache entry for domain: {}", key.domain);
            }
        }

        self.stats.record_miss();
        trace!("Cache miss for domain: {}", key.domain);
        None
    }

    /// Store a response in the cache
    pub fn put(&self, key: CacheKey, response: DNSPacket) {
        let ttl = self.calculate_ttl(&response);
        let is_negative = self.is_negative_response(&response);

        // Use negative TTL for error responses
        let final_ttl = if is_negative {
            self.negative_ttl.min(ttl)
        } else {
            ttl
        };

        if final_ttl == 0 {
            debug!("Not caching response with 0 TTL for domain: {}", key.domain);
            return;
        }

        let entry = CacheEntry::new(response, final_ttl, is_negative);

        // Check if we need to evict entries
        if self.cache.len() >= self.max_size {
            self.evict_lru();
        }

        // Insert the new entry
        self.cache.insert(key.clone(), entry);

        // Update insertion order for LRU
        {
            let mut order = self.insertion_order.lock();
            order.retain(|k| k != &key); // Remove if already present
            order.push(key.clone());
        }

        // Update domain trie for efficient lookups
        {
            let mut trie = self.domain_trie.lock();
            trie.insert(&key.domain, key.clone());
        }

        debug!(
            "Cached response for domain: {} (TTL: {}s, negative: {})",
            key.domain, final_ttl, is_negative
        );
    }

    /// Calculate the minimum TTL from all records in the response
    fn calculate_ttl(&self, response: &DNSPacket) -> u32 {
        let mut min_ttl = u32::MAX;

        // Check TTLs in answers - these are the primary records
        for answer in &response.answers {
            min_ttl = min_ttl.min(answer.ttl);
        }

        // Check TTLs in authorities (for negative responses)
        for authority in &response.authorities {
            min_ttl = min_ttl.min(authority.ttl);
        }

        // Skip additional records for TTL calculation as they often contain
        // EDNS OPT records with TTL=0 which shouldn't affect caching
        // If we have answers or authorities, use their TTL

        // If no relevant records found, use a default TTL
        if min_ttl == u32::MAX {
            300 // 5 minutes default
        } else {
            min_ttl
        }
    }

    /// Check if this is a negative response (NXDOMAIN, NODATA)
    fn is_negative_response(&self, response: &DNSPacket) -> bool {
        // NXDOMAIN
        if response.header.rcode == 3 {
            return true;
        }

        // NODATA (NOERROR with no answers)
        if response.header.rcode == 0 && response.header.ancount == 0 {
            return true;
        }

        false
    }

    /// Evict the least recently used entry
    fn evict_lru(&self) {
        let key_to_evict = {
            let mut order = self.insertion_order.lock();
            if let Some(key) = order.first().cloned() {
                order.retain(|k| k != &key);
                Some(key)
            } else {
                None
            }
        };

        if let Some(key) = key_to_evict {
            self.cache.remove(&key);
            self.stats.record_eviction();
            debug!("Evicted LRU cache entry for domain: {}", key.domain);
        }
    }

    /// Remove all expired entries
    pub fn cleanup_expired(&self) {
        let mut expired_keys = Vec::new();

        // Find expired entries
        for item in self.cache.iter() {
            if item.value().is_expired() {
                expired_keys.push(item.key().clone());
            }
        }

        let expired_count = expired_keys.len();

        // Remove expired entries
        for key in &expired_keys {
            self.cache.remove(key);
            self.stats.record_expired_eviction();

            // Remove from insertion order
            let mut order = self.insertion_order.lock();
            order.retain(|k| k != key);
        }

        if expired_count > 0 {
            debug!("Cleaned up {} expired cache entries", expired_count);
        }
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        let count = self.cache.len();
        self.cache.clear();
        self.insertion_order.lock().clear();
        self.domain_trie.lock().cleanup_expired();
        debug!("Cleared {} cache entries", count);
    }

    /// Find related cache entries by domain suffix (for wildcard matching)
    pub fn find_related_entries(&self, domain: &str) -> Vec<CacheKey> {
        let trie = self.domain_trie.lock();
        let matching_keys = trie.find_matching_keys(domain);

        // Filter out expired entries
        matching_keys
            .into_iter()
            .filter(|key| {
                if let Some(entry) = self.cache.get(key) {
                    !entry.is_expired()
                } else {
                    false
                }
            })
            .cloned()
            .collect()
    }

    /// Get current cache size
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    /// Get maximum cache size
    pub fn capacity(&self) -> usize {
        self.max_size
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get detailed cache information for debugging
    pub fn debug_info(&self) -> String {
        let stats = &self.stats;
        format!(
            "Cache: size={}/{}, hits={}, misses={}, hit_rate={:.2}%, evictions={}, expired={}",
            self.size(),
            self.capacity(),
            stats.hits.load(Ordering::Relaxed),
            stats.misses.load(Ordering::Relaxed),
            stats.hit_rate() * 100.0,
            stats.evictions.load(Ordering::Relaxed),
            stats.expired_evictions.load(Ordering::Relaxed)
        )
    }
}

impl Default for DnsCache {
    fn default() -> Self {
        Self::new(10000, 300) // 10k entries, 5 min negative TTL
    }
}
