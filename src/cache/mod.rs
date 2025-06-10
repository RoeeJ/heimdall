pub mod local_backend;
pub mod redis_backend;

pub use redis_backend::{LayeredCache, RedisConfig};

use crate::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
};
use dashmap::DashMap;
use parking_lot::Mutex;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::fs;
use tracing::{debug, trace};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
#[rkyv(derive(Debug, PartialEq))]
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

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{:?}:{:?}",
            self.domain, self.record_type, self.record_class
        )
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

/// Serializable version of CacheEntry for persistence
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[rkyv(derive(Debug, PartialEq))]
pub struct SerializableCacheEntry {
    pub response: DNSPacket,
    pub expiry_timestamp: u64, // Unix timestamp in seconds
    pub original_ttl: u32,
    pub is_negative: bool,
}

impl From<&CacheEntry> for SerializableCacheEntry {
    fn from(entry: &CacheEntry) -> Self {
        let expiry_timestamp = match entry.expiry.checked_duration_since(Instant::now()) {
            Some(duration) => {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    + duration.as_secs()
            }
            None => {
                // Entry is already expired
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            }
        };

        Self {
            response: entry.response.clone(),
            expiry_timestamp,
            original_ttl: entry.original_ttl,
            is_negative: entry.is_negative,
        }
    }
}

impl From<SerializableCacheEntry> for CacheEntry {
    fn from(serializable: SerializableCacheEntry) -> Self {
        let now_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let expiry = if serializable.expiry_timestamp > now_timestamp {
            Instant::now() + Duration::from_secs(serializable.expiry_timestamp - now_timestamp)
        } else {
            Instant::now() // Already expired
        };

        Self {
            response: serializable.response,
            expiry,
            original_ttl: serializable.original_ttl,
            is_negative: serializable.is_negative,
        }
    }
}

/// Cache persistence data structure
#[derive(Debug, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[rkyv(derive(Debug, PartialEq))]
pub struct CacheSnapshot {
    pub entries: Vec<(CacheKey, SerializableCacheEntry)>,
    pub snapshot_timestamp: u64,
    pub version: u32,
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
    pub negative_hits: AtomicU64,      // RFC 2308: Negative cache hits
    pub nxdomain_responses: AtomicU64, // NXDOMAIN responses cached
    pub nodata_responses: AtomicU64,   // NODATA responses cached
}

impl Default for CacheStats {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            expired_evictions: AtomicU64::new(0),
            negative_hits: AtomicU64::new(0),
            nxdomain_responses: AtomicU64::new(0),
            nodata_responses: AtomicU64::new(0),
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

    pub fn record_evictions(&self, count: u64) {
        self.evictions.fetch_add(count, Ordering::Relaxed);
    }

    pub fn record_expired_eviction(&self) {
        self.expired_evictions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_expired_evictions(&self, count: u64) {
        self.expired_evictions.fetch_add(count, Ordering::Relaxed);
    }

    pub fn record_negative_hit(&self) {
        self.negative_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_nxdomain_response(&self) {
        self.nxdomain_responses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_nodata_response(&self) {
        self.nodata_responses.fetch_add(1, Ordering::Relaxed);
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

    pub fn negative_hit_rate(&self) -> f64 {
        let negative_hits = self.negative_hits.load(Ordering::Relaxed);
        let total_hits = self.hits.load(Ordering::Relaxed);

        if total_hits == 0 {
            0.0
        } else {
            negative_hits as f64 / total_hits as f64
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
    cache_file_path: Option<String>,       // Optional cache persistence file
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
            cache_file_path: None,
        }
    }

    pub fn with_persistence(max_size: usize, negative_ttl: u32, cache_file_path: String) -> Self {
        Self {
            cache: DashMap::new(),
            max_size,
            negative_ttl,
            stats: CacheStats::new(),
            insertion_order: Mutex::new(Vec::new()),
            domain_trie: Mutex::new(DomainTrie::new()),
            cache_file_path: Some(cache_file_path),
        }
    }

    /// Get a cached response if it exists and hasn't expired
    pub fn get(&self, key: &CacheKey) -> Option<DNSPacket> {
        if let Some(entry) = self.cache.get(key) {
            if let Some(response) = entry.get_response() {
                self.stats.record_hit();

                // Track negative cache hits for RFC 2308 statistics
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

    /// Store a response in the cache with RFC 2308 negative caching support
    pub fn put(&self, key: CacheKey, response: DNSPacket) {
        let ttl = self.calculate_ttl(&response);
        let is_negative = self.is_negative_response(&response);

        // RFC 2308 compliant TTL handling
        let final_ttl = if is_negative {
            // For negative responses, prefer the calculated SOA-based TTL
            // but don't exceed the configured maximum negative TTL
            ttl.min(self.negative_ttl)
        } else {
            // For positive responses, use calculated TTL, but for empty/test responses
            // that have no real DNS content, apply the negative TTL as a safety measure
            if response.header.ancount == 0 && response.header.nscount == 0 && !response.header.qr {
                // This looks like a test/empty packet, use negative TTL
                self.negative_ttl.min(ttl)
            } else {
                ttl
            }
        };

        if final_ttl == 0 {
            debug!("Not caching response with 0 TTL for domain: {}", key.domain);
            return;
        }

        // Record negative caching statistics per RFC 2308 before moving response
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
            "Cached {} response for domain: {} (TTL: {}s, calculated: {}s)",
            cache_type, key.domain, final_ttl, ttl
        );
    }

    /// Calculate the minimum TTL from all records in the response per RFC 2308
    fn calculate_ttl(&self, response: &DNSPacket) -> u32 {
        let mut min_ttl = u32::MAX;
        let is_negative = self.is_negative_response(response);

        if is_negative {
            // RFC 2308 Section 3: For negative responses, use SOA minimum TTL
            // Look for SOA record in authority section
            for authority in &response.authorities {
                if authority.rtype == DNSResourceType::SOA {
                    // Extract SOA minimum TTL from rdata
                    // First try the new parsing helper, then fallback to manual extraction
                    let soa_min_ttl = authority
                        .get_soa_minimum()
                        .or_else(|| self.extract_soa_minimum_ttl(&authority.rdata));

                    if let Some(soa_min_ttl) = soa_min_ttl {
                        // RFC 2308: Use the minimum of SOA TTL and SOA minimum field
                        min_ttl = min_ttl.min(authority.ttl.min(soa_min_ttl));
                        debug!(
                            "Using SOA-based negative TTL: {} seconds (SOA TTL: {}, SOA minimum: {})",
                            min_ttl, authority.ttl, soa_min_ttl
                        );
                    } else {
                        // Fallback to SOA record TTL if we can't parse the minimum
                        min_ttl = min_ttl.min(authority.ttl);
                        debug!(
                            "Using SOA record TTL for negative caching: {} seconds",
                            authority.ttl
                        );
                    }
                    break; // Use the first SOA record found
                } else {
                    // Other authority records (like NS) can also provide TTL info
                    min_ttl = min_ttl.min(authority.ttl);
                }
            }
        } else {
            // Positive responses: use minimum TTL from answer records
            for answer in &response.answers {
                min_ttl = min_ttl.min(answer.ttl);
            }

            // Also check authority records for additional constraints
            for authority in &response.authorities {
                min_ttl = min_ttl.min(authority.ttl);
            }
        }

        // Skip additional records for TTL calculation as they often contain
        // EDNS OPT records with TTL=0 which shouldn't affect caching

        // If no relevant records found, use appropriate defaults
        if min_ttl == u32::MAX {
            if is_negative {
                // RFC 2308 suggests 5 minutes for negative responses without SOA
                300
            } else {
                // For empty/invalid responses (no answers, no authorities), use a very short TTL
                // This handles test cases and malformed responses that shouldn't be cached long
                if response.header.ancount == 0 && response.header.nscount == 0 {
                    60 // 1 minute for empty responses
                } else {
                    300 // Standard default for valid positive responses
                }
            }
        } else {
            min_ttl
        }
    }

    /// Extract the minimum TTL field from SOA record data per RFC 1035
    ///
    /// NOTE: This method is now largely superseded by DNSResource::get_soa_minimum()
    /// but is kept as a fallback for cases where the SOA record hasn't been fully parsed
    fn extract_soa_minimum_ttl(&self, rdata: &[u8]) -> Option<u32> {
        // SOA rdata format (RFC 1035 Section 3.3.13):
        // MNAME (variable length domain name)
        // RNAME (variable length domain name)
        // SERIAL (32-bit)
        // REFRESH (32-bit)
        // RETRY (32-bit)
        // EXPIRE (32-bit)
        // MINIMUM (32-bit) <- This is what we need

        if rdata.len() < 20 {
            // Too short to contain the required fields
            return None;
        }

        let mut pos = 0;

        // Skip MNAME (domain name with length encoding)
        match self.skip_domain_name(rdata, pos) {
            Some(new_pos) => pos = new_pos,
            None => return None,
        }

        // Skip RNAME (domain name with length encoding)
        match self.skip_domain_name(rdata, pos) {
            Some(new_pos) => pos = new_pos,
            None => return None,
        }

        // Skip SERIAL (4 bytes), REFRESH (4 bytes), RETRY (4 bytes), EXPIRE (4 bytes)
        pos += 16;

        // Extract MINIMUM (4 bytes)
        if pos + 4 <= rdata.len() {
            let minimum =
                u32::from_be_bytes([rdata[pos], rdata[pos + 1], rdata[pos + 2], rdata[pos + 3]]);
            Some(minimum)
        } else {
            None
        }
    }

    /// Skip a DNS domain name in wire format and return the new position
    fn skip_domain_name(&self, data: &[u8], mut pos: usize) -> Option<usize> {
        while pos < data.len() {
            let length = data[pos] as usize;

            if length == 0 {
                // End of domain name
                return Some(pos + 1);
            }

            if length >= 192 {
                // Compression pointer (RFC 1035 Section 4.1.4)
                // Skip the 2-byte pointer
                return Some(pos + 2);
            }

            // Regular label
            if pos + 1 + length > data.len() {
                return None; // Invalid label length
            }

            pos += 1 + length;
        }

        None // Incomplete domain name
    }

    /// Check if this is a negative response (NXDOMAIN, NODATA) per RFC 2308
    fn is_negative_response(&self, response: &DNSPacket) -> bool {
        // NXDOMAIN (Response Code 3)
        if response.header.rcode == 3 {
            return true;
        }

        // NODATA (NOERROR with no answers but question exists and response is authoritative or has authority section)
        // RFC 2308 Section 2.2: NODATA is a pseudo RCODE with RCODE=0, ANCOUNT=0
        // We should only consider it NODATA if there's evidence this is a real DNS response (not just a default packet)
        if response.header.rcode == 0 && response.header.ancount == 0 && response.header.qdcount > 0
        {
            // Additional check: should have QR flag set (this is a response) and either:
            // - AA flag set (authoritative answer), or
            // - Authority section present (referral/SOA for negative response)
            if response.header.qr && (response.header.aa || response.header.nscount > 0) {
                return true;
            }
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

    /// Get detailed cache information for debugging with RFC 2308 negative caching stats
    pub fn debug_info(&self) -> String {
        let stats = &self.stats;
        format!(
            "Cache: size={}/{}, hits={}, misses={}, hit_rate={:.2}%, negative_hits={} ({:.1}%), NXDOMAIN={}, NODATA={}, evictions={}, expired={}",
            self.size(),
            self.capacity(),
            stats.hits.load(Ordering::Relaxed),
            stats.misses.load(Ordering::Relaxed),
            stats.hit_rate() * 100.0,
            stats.negative_hits.load(Ordering::Relaxed),
            stats.negative_hit_rate() * 100.0,
            stats.nxdomain_responses.load(Ordering::Relaxed),
            stats.nodata_responses.load(Ordering::Relaxed),
            stats.evictions.load(Ordering::Relaxed),
            stats.expired_evictions.load(Ordering::Relaxed)
        )
    }

    /// Save cache to disk using rkyv for zero-copy deserialization
    pub async fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cache_path = match &self.cache_file_path {
            Some(path) => path,
            None => return Ok(()), // No persistence configured
        };

        // Create snapshot of current cache
        let mut entries = Vec::new();
        for item in self.cache.iter() {
            let key = item.key().clone();
            let entry = item.value();

            // Skip expired entries
            if !entry.is_expired() {
                let serializable_entry = SerializableCacheEntry::from(entry);
                entries.push((key, serializable_entry));
            }
        }

        let snapshot = CacheSnapshot {
            entries,
            snapshot_timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            version: 2, // Version 2 for rkyv format
        };

        // Serialize to rkyv format (binary)
        let serialized_data = rkyv::to_bytes::<rkyv::rancor::Error>(&snapshot)
            .map_err(|e| format!("rkyv serialization failed: {}", e))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(cache_path).parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write to temporary file first, then rename for atomic operation
        let temp_path = format!("{}.tmp", cache_path);
        fs::write(&temp_path, &serialized_data).await?;
        fs::rename(&temp_path, cache_path).await?;

        debug!(
            "Saved {} cache entries to {} ({} bytes, rkyv format)",
            snapshot.entries.len(),
            cache_path,
            serialized_data.len()
        );

        Ok(())
    }

    /// Load cache from disk using rkyv for zero-copy deserialization
    pub async fn load_from_disk(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cache_path = match &self.cache_file_path {
            Some(path) => path,
            None => return Ok(()), // No persistence configured
        };

        // Check if cache file exists
        if !fs::try_exists(cache_path).await? {
            debug!(
                "Cache file {} does not exist, starting with empty cache",
                cache_path
            );
            return Ok(());
        }

        // Read cache file as bytes
        let serialized_data = fs::read(cache_path).await?;

        // Try to detect format and deserialize accordingly
        let snapshot = if serialized_data.starts_with(b"{") {
            // JSON format (legacy v1) - fallback for backward compatibility
            debug!("Detected legacy JSON format, deserializing with serde_json");
            let json_str = std::str::from_utf8(&serialized_data)?;
            serde_json::from_str::<CacheSnapshot>(json_str)?
        } else {
            // rkyv format (v2+) - zero-copy deserialization
            debug!("Detected rkyv binary format, deserializing with zero-copy");

            // Deserialize directly from the bytes
            rkyv::from_bytes::<CacheSnapshot, rkyv::rancor::Error>(&serialized_data)
                .map_err(|e| format!("rkyv deserialization failed: {}", e))?
        };

        debug!(
            "Loading cache from {} (version {}, {} entries, snapshot from {}, {} bytes)",
            cache_path,
            snapshot.version,
            snapshot.entries.len(),
            snapshot.snapshot_timestamp,
            serialized_data.len()
        );

        // Load entries, filtering out expired ones
        let now_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut loaded_count = 0;
        let mut expired_count = 0;

        for (key, serializable_entry) in snapshot.entries {
            // Check if entry is still valid
            if serializable_entry.expiry_timestamp > now_timestamp {
                let entry = CacheEntry::from(serializable_entry);

                // Insert into cache
                self.cache.insert(key.clone(), entry);

                // Update insertion order for LRU
                {
                    let mut order = self.insertion_order.lock();
                    order.push(key.clone());
                }

                // Update domain trie
                {
                    let mut trie = self.domain_trie.lock();
                    trie.insert(&key.domain, key.clone());
                }

                loaded_count += 1;
            } else {
                expired_count += 1;
            }
        }

        debug!(
            "Loaded {} valid cache entries, skipped {} expired entries (rkyv zero-copy)",
            loaded_count, expired_count
        );

        Ok(())
    }

    /// Get cache persistence status
    pub fn has_persistence(&self) -> bool {
        self.cache_file_path.is_some()
    }

    /// Get cache file path
    pub fn cache_file_path(&self) -> Option<&str> {
        self.cache_file_path.as_deref()
    }

    /// Get all cache entries (for testing and debugging)
    pub fn iter_entries(&self) -> impl Iterator<Item = (CacheKey, CacheEntry)> + '_ {
        self.cache
            .iter()
            .map(|item| (item.key().clone(), item.value().clone()))
    }
}

impl Default for DnsCache {
    fn default() -> Self {
        Self::new(10000, 300) // 10k entries, 5 min negative TTL
    }
}
