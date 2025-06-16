use super::{
    CacheEntry, CacheKey, CacheSnapshot, CacheStats, SerializableCacheEntry,
    redis_backend::CacheBackend,
};
use crate::dns::{DNSPacket, enums::DNSResourceType};
use crate::pool::StringInterner;
use dashmap::DashMap;
use parking_lot::RwLock;
use rkyv;
use std::collections::{HashMap, VecDeque, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tracing::{debug, trace};

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

    fn insert(&mut self, domain: &str, cache_key: CacheKey) {
        let parts: Vec<&str> = domain.split('.').rev().collect();
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

    fn find_matching_keys(&self, domain: &str) -> Vec<&CacheKey> {
        let parts: Vec<&str> = domain.split('.').rev().collect();
        let mut current = self;
        let mut results = Vec::new();

        for part in parts {
            if let Some(child) = current.children.get(part) {
                current = child;
                results.extend(&current.cache_keys);
            } else {
                break;
            }
        }

        results
    }

    fn cleanup_expired(&mut self) {
        self.cache_keys.clear();
        for child in self.children.values_mut() {
            child.cleanup_expired();
        }
    }
}

/// Unified DNS cache combining the best features from all implementations
pub struct UnifiedDnsCache {
    /// Hot cache layer for frequently accessed entries
    hot_cache: DashMap<CacheKey, Arc<CacheEntry>>,
    hot_cache_size: usize,

    /// Main cache layer
    main_cache: DashMap<CacheKey, Arc<CacheEntry>>,
    max_size: usize,

    /// LRU tracking with sharding to reduce contention
    lru_shards: Vec<RwLock<VecDeque<CacheKey>>>,
    num_shards: usize,

    /// Domain trie for wildcard lookups
    domain_trie: RwLock<DomainTrie>,

    /// String interner for memory efficiency
    string_interner: StringInterner,

    /// Statistics
    stats: Arc<CacheStats>,

    /// Access tracking for hot cache promotion
    access_tracker: DashMap<CacheKey, AtomicU32>,
    promotion_threshold: u32,

    /// Global access counter for LRU ordering
    _access_counter: AtomicU64,

    /// Current size tracking
    current_size: AtomicUsize,

    /// Persistence
    cache_file_path: Option<String>,

    /// Optional L2 cache backend
    l2_cache: Option<Arc<dyn CacheBackend>>,

    /// Configuration
    negative_ttl: u32,
}

impl UnifiedDnsCache {
    /// Create a new unified DNS cache
    pub fn new(max_size: usize, negative_ttl: u32, cache_file_path: Option<String>) -> Self {
        let hot_cache_size = (max_size / 10).max(1).min(max_size / 2); // 10% for hot cache, but at least 1 and at most 50%
        let main_cache_size = max_size.saturating_sub(hot_cache_size);
        let num_shards = 16; // Could be configurable

        let mut lru_shards = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            lru_shards.push(RwLock::new(VecDeque::new()));
        }

        Self {
            hot_cache: DashMap::with_capacity(hot_cache_size),
            hot_cache_size,
            main_cache: DashMap::with_capacity(main_cache_size),
            max_size: main_cache_size,
            lru_shards,
            num_shards,
            domain_trie: RwLock::new(DomainTrie::new()),
            string_interner: StringInterner::new(10000),
            stats: Arc::new(CacheStats::new()),
            access_tracker: DashMap::new(),
            promotion_threshold: 3,
            _access_counter: AtomicU64::new(0),
            current_size: AtomicUsize::new(0),
            cache_file_path,
            l2_cache: None,
            negative_ttl,
        }
    }

    /// Create with an L2 cache backend
    pub fn with_l2_cache(
        max_size: usize,
        negative_ttl: u32,
        cache_file_path: Option<String>,
        l2_cache: Arc<dyn CacheBackend>,
    ) -> Self {
        let mut cache = Self::new(max_size, negative_ttl, cache_file_path);
        cache.l2_cache = Some(l2_cache);
        cache
    }

    /// Get a cached response
    pub fn get(&self, key: &CacheKey) -> Option<DNSPacket> {
        // Check hot cache first
        if let Some(entry) = self.hot_cache.get(key) {
            if let Some(response) = entry.get_response() {
                self.stats.record_hit();
                if entry.is_negative {
                    self.stats.record_negative_hit();
                }
                trace!("Hot cache hit for domain: {}", key.domain);
                return Some(response);
            } else {
                // Expired in hot cache
                drop(entry);
                self.hot_cache.remove(key);
            }
        }

        // Check main cache
        if let Some(entry) = self.main_cache.get(key) {
            if let Some(response) = entry.get_response() {
                self.stats.record_hit();
                if entry.is_negative {
                    self.stats.record_negative_hit();
                }

                // Track access for potential promotion
                let should_promote = self.track_access(key);
                if should_promote {
                    self.promote_to_hot_cache(key, entry.clone());
                }

                trace!("Main cache hit for domain: {}", key.domain);
                return Some(response);
            } else {
                // Expired in main cache
                drop(entry);
                self.remove_from_main_cache(key);
            }
        }

        // TODO: Check L2 cache if available
        // This would be async, so we'd need to make get() async or use a different pattern

        self.stats.record_miss();
        trace!("Cache miss for domain: {}", key.domain);
        None
    }

    /// Store a response in the cache
    pub fn put(&self, key: CacheKey, response: DNSPacket) {
        let ttl = self.calculate_ttl(&response);
        let is_negative = self.is_negative_response(&response);

        let final_ttl = if is_negative {
            ttl.min(self.negative_ttl)
        } else {
            ttl
        };

        if final_ttl == 0 {
            debug!("Not caching response with 0 TTL for domain: {}", key.domain);
            return;
        }

        // Record statistics
        if is_negative {
            match response.header.rcode {
                3 => self.stats.record_nxdomain_response(),
                0 => self.stats.record_nodata_response(),
                _ => {}
            }
        }

        let entry = Arc::new(CacheEntry::new(response, final_ttl, is_negative));

        // Check if we need to evict from main cache
        if self.main_cache.len() >= self.max_size {
            self.evict_lru();
        }

        // Insert into main cache
        let _was_new = self.main_cache.insert(key.clone(), entry.clone()).is_none();

        // Update size tracking
        self.current_size
            .store(self.main_cache.len(), Ordering::Relaxed);

        // Add to LRU tracking
        let shard_idx = self.hash_to_shard(&key);
        if let Some(mut shard) = self.lru_shards[shard_idx].try_write() {
            shard.push_back(key.clone());

            // Keep shard size reasonable
            if shard.len() > self.max_size / self.num_shards * 2 {
                let drain_count = shard.len() / 2;
                shard.drain(0..drain_count);
            }
        }

        // Update domain trie
        let mut trie = self.domain_trie.write();
        trie.insert(&key.domain, key.clone());

        // Reset access tracking
        self.access_tracker.remove(&key);

        debug!(
            "Cached {} response for domain: {} (TTL: {}s)",
            if is_negative { "negative" } else { "positive" },
            key.domain,
            final_ttl
        );

        // TODO: Write to L2 cache if available (async)
    }

    /// Track access and return true if should promote to hot cache
    fn track_access(&self, key: &CacheKey) -> bool {
        let count = self
            .access_tracker
            .entry(key.clone())
            .or_insert_with(|| AtomicU32::new(0));

        let new_count = count.fetch_add(1, Ordering::Relaxed) + 1;
        new_count >= self.promotion_threshold
    }

    /// Promote an entry to the hot cache
    fn promote_to_hot_cache(&self, key: &CacheKey, entry: Arc<CacheEntry>) {
        // Check if hot cache is full
        if self.hot_cache.len() >= self.hot_cache_size {
            // Evict least recently used from hot cache
            // Simple approach: remove first entry found
            if let Some(item) = self.hot_cache.iter().next() {
                let evict_key = item.key().clone();
                drop(item);
                self.hot_cache.remove(&evict_key);
            }
        }

        self.hot_cache.insert(key.clone(), entry);
        self.access_tracker.remove(key);
        debug!("Promoted {} to hot cache", key.domain);
    }

    /// Remove entry from main cache and update tracking
    fn remove_from_main_cache(&self, key: &CacheKey) {
        self.main_cache.remove(key);
        self.current_size.fetch_sub(1, Ordering::Relaxed);
        self.stats.record_expired_eviction();

        // Remove from LRU tracking
        let shard_idx = self.hash_to_shard(key);
        if let Some(mut shard) = self.lru_shards[shard_idx].try_write() {
            shard.retain(|k| k != key);
        }
    }

    /// Evict least recently used entry
    fn evict_lru(&self) {
        let mut oldest_key = None;

        // First try to find an entry from the LRU shards (these are truly old)
        for i in 0..4.min(self.num_shards) {
            if let Some(shard) = self.lru_shards[i].try_read() {
                if let Some(key) = shard.front() {
                    if self.main_cache.contains_key(key) {
                        oldest_key = Some(key.clone());
                        break;
                    }
                }
            }
        }

        // If no entry found in shards, just take the first entry we can find
        if oldest_key.is_none() {
            if let Some(item) = self.main_cache.iter().next() {
                oldest_key = Some(item.key().clone());
            }
        }

        if let Some(key) = oldest_key {
            self.main_cache.remove(&key);
            self.current_size
                .store(self.main_cache.len(), Ordering::Relaxed);
            self.stats.record_eviction();

            // Remove from LRU tracking
            let shard_idx = self.hash_to_shard(&key);
            if let Some(mut shard) = self.lru_shards[shard_idx].try_write() {
                shard.retain(|k| k != &key);
            }

            debug!("Evicted LRU cache entry for domain: {}", key.domain);
        }
    }

    /// Calculate TTL from response
    fn calculate_ttl(&self, response: &DNSPacket) -> u32 {
        let mut min_ttl = u32::MAX;
        let is_negative = self.is_negative_response(response);

        if is_negative {
            // Look for SOA record in authority section
            for authority in &response.authorities {
                if authority.rtype == DNSResourceType::SOA {
                    if let Some(soa_min_ttl) = authority.get_soa_minimum() {
                        min_ttl = min_ttl.min(authority.ttl.min(soa_min_ttl));
                    } else {
                        min_ttl = min_ttl.min(authority.ttl);
                    }
                    break;
                } else {
                    min_ttl = min_ttl.min(authority.ttl);
                }
            }
        } else {
            for answer in &response.answers {
                min_ttl = min_ttl.min(answer.ttl);
            }
            for authority in &response.authorities {
                min_ttl = min_ttl.min(authority.ttl);
            }
        }

        if min_ttl == u32::MAX {
            if is_negative {
                300 // 5 minutes for negative responses
            } else if response.header.ancount == 0 && response.header.nscount == 0 {
                60 // 1 minute for empty responses
            } else {
                300 // Standard default
            }
        } else {
            min_ttl
        }
    }

    /// Check if response is negative
    fn is_negative_response(&self, response: &DNSPacket) -> bool {
        response.header.rcode == 3
            || (response.header.rcode == 0
                && response.header.ancount == 0
                && response.header.qdcount > 0
                && response.header.qr
                && (response.header.aa || response.header.nscount > 0))
    }

    /// Hash key to shard index
    fn hash_to_shard(&self, key: &CacheKey) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.num_shards
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        self.hot_cache.clear();
        self.main_cache.clear();
        self.current_size.store(0, Ordering::Relaxed);
        self.access_tracker.clear();

        for shard in &self.lru_shards {
            if let Some(mut s) = shard.try_write() {
                s.clear();
            }
        }

        self.domain_trie.write().cleanup_expired();
        debug!("Cache cleared");
    }

    /// Cleanup expired entries
    pub fn cleanup_expired(&self) {
        let mut expired_count = 0;

        // Cleanup hot cache
        let mut expired_keys = Vec::new();
        for item in self.hot_cache.iter() {
            if item.value().is_expired() {
                expired_keys.push(item.key().clone());
            }
        }
        for key in expired_keys {
            self.hot_cache.remove(&key);
            expired_count += 1;
        }

        // Cleanup main cache
        let mut expired_keys = Vec::new();
        for item in self.main_cache.iter() {
            if item.value().is_expired() {
                expired_keys.push(item.key().clone());
            }
        }
        for key in &expired_keys {
            self.remove_from_main_cache(key);
            expired_count += 1;
        }

        if expired_count > 0 {
            debug!("Cleaned up {} expired cache entries", expired_count);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get current cache size
    pub fn size(&self) -> usize {
        self.hot_cache.len() + self.main_cache.len()
    }

    /// Get maximum cache capacity
    pub fn capacity(&self) -> usize {
        self.hot_cache_size + self.max_size
    }

    /// Get string interner
    pub fn string_interner(&self) -> &StringInterner {
        &self.string_interner
    }

    /// Get debug info
    pub fn debug_info(&self) -> String {
        let stats = &self.stats;
        format!(
            "UnifiedCache: hot={}/{}, main={}/{}, hits={}, misses={}, hit_rate={:.2}%, negative_hits={} ({:.1}%), evictions={}",
            self.hot_cache.len(),
            self.hot_cache_size,
            self.current_size.load(Ordering::Relaxed),
            self.max_size,
            stats.hits.load(Ordering::Relaxed),
            stats.misses.load(Ordering::Relaxed),
            stats.hit_rate() * 100.0,
            stats.negative_hits.load(Ordering::Relaxed),
            stats.negative_hit_rate() * 100.0,
            stats.evictions.load(Ordering::Relaxed)
        )
    }

    /// Save cache to disk using rkyv
    pub async fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cache_path = match &self.cache_file_path {
            Some(path) => path,
            None => return Ok(()),
        };

        let mut entries = Vec::new();

        // Save hot cache entries
        for item in self.hot_cache.iter() {
            let key = item.key().clone();
            let entry = item.value();
            if !entry.is_expired() {
                let serializable_entry = SerializableCacheEntry::from(entry.as_ref());
                entries.push((key, serializable_entry));
            }
        }

        // Save main cache entries
        for item in self.main_cache.iter() {
            let key = item.key().clone();
            let entry = item.value();
            if !entry.is_expired() {
                let serializable_entry = SerializableCacheEntry::from(entry.as_ref());
                entries.push((key, serializable_entry));
            }
        }

        let snapshot = CacheSnapshot {
            entries,
            snapshot_timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            version: 2,
        };

        let serialized_data = rkyv::to_bytes::<rkyv::rancor::Error>(&snapshot)
            .map_err(|e| format!("rkyv serialization failed: {}", e))?;

        if let Some(parent) = std::path::Path::new(cache_path).parent() {
            fs::create_dir_all(parent).await?;
        }

        let temp_path = format!("{}.tmp", cache_path);
        fs::write(&temp_path, &serialized_data).await?;
        fs::rename(&temp_path, cache_path).await?;

        debug!(
            "Saved {} cache entries to {} ({} bytes)",
            snapshot.entries.len(),
            cache_path,
            serialized_data.len()
        );

        Ok(())
    }

    /// Load cache from disk using rkyv
    pub async fn load_from_disk(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cache_path = match &self.cache_file_path {
            Some(path) => path,
            None => return Ok(()),
        };

        if !fs::try_exists(cache_path).await? {
            debug!(
                "Cache file {} does not exist, starting with empty cache",
                cache_path
            );
            return Ok(());
        }

        let serialized_data = fs::read(cache_path).await?;

        let snapshot = if serialized_data.starts_with(b"{") {
            // Legacy JSON format
            let json_str = std::str::from_utf8(&serialized_data)?;
            serde_json::from_str::<CacheSnapshot>(json_str)?
        } else {
            // rkyv format
            rkyv::from_bytes::<CacheSnapshot, rkyv::rancor::Error>(&serialized_data)
                .map_err(|e| format!("rkyv deserialization failed: {}", e))?
        };

        let now_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut loaded_count = 0;
        let mut expired_count = 0;

        for (key, serializable_entry) in snapshot.entries {
            if serializable_entry.expiry_timestamp > now_timestamp {
                let entry = Arc::new(CacheEntry::from(serializable_entry));

                // All loaded entries go to main cache initially
                self.main_cache.insert(key.clone(), entry);
                self.current_size.fetch_add(1, Ordering::Relaxed);

                // Update LRU tracking
                let shard_idx = self.hash_to_shard(&key);
                if let Some(mut shard) = self.lru_shards[shard_idx].try_write() {
                    shard.push_back(key.clone());
                }

                // Update domain trie
                self.domain_trie.write().insert(&key.domain, key.clone());

                loaded_count += 1;
            } else {
                expired_count += 1;
            }
        }

        debug!(
            "Loaded {} valid cache entries, skipped {} expired entries",
            loaded_count, expired_count
        );

        Ok(())
    }

    /// Check if persistence is enabled
    pub fn has_persistence(&self) -> bool {
        self.cache_file_path.is_some()
    }

    /// Get cache file path
    pub fn cache_file_path(&self) -> Option<&str> {
        self.cache_file_path.as_deref()
    }

    /// Find related entries by domain suffix
    pub fn find_related_entries(&self, domain: &str) -> Vec<CacheKey> {
        let trie = self.domain_trie.read();
        let matching_keys = trie.find_matching_keys(domain);

        matching_keys
            .into_iter()
            .filter(|key| self.hot_cache.contains_key(key) || self.main_cache.contains_key(key))
            .cloned()
            .collect()
    }
}

impl Default for UnifiedDnsCache {
    fn default() -> Self {
        Self::new(10000, 300, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::enums::{DNSResourceClass, DNSResourceType};

    #[test]
    fn test_unified_cache_basic() {
        let cache = UnifiedDnsCache::new(1000, 300, None);

        let key = CacheKey::new(
            "example.com".to_string(),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );

        let mut packet = DNSPacket::default();
        packet.header.id = 12345;

        cache.put(key.clone(), packet.clone());

        let retrieved = cache.get(&key).unwrap();
        assert_eq!(retrieved.header.id, 12345);
    }

    #[test]
    fn test_hot_cache_promotion() {
        let mut cache = UnifiedDnsCache::new(1000, 300, None);
        cache.promotion_threshold = 3; // Promote after 3 accesses

        let key = CacheKey::new(
            "hot.example.com".to_string(),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );

        let packet = DNSPacket::default();
        cache.put(key.clone(), packet);

        // Access multiple times to trigger promotion
        for _ in 0..3 {
            assert!(cache.get(&key).is_some());
        }

        // Should now be in hot cache
        assert!(cache.hot_cache.contains_key(&key));
    }

    #[test]
    fn test_lru_eviction() {
        let cache = UnifiedDnsCache::new(10, 300, None); // Reasonable size for testing

        // Fill main cache beyond capacity
        for i in 0..12 {
            let key = CacheKey::new(
                format!("example{}.com", i),
                DNSResourceType::A,
                DNSResourceClass::IN,
            );
            let packet = DNSPacket::default();
            cache.put(key, packet);
        }

        // Cache should have evicted entries to stay within capacity
        // With max_size=10: hot_cache_size=1, main_cache_size=9, total capacity=10
        assert!(cache.size() <= 10);

        // Specifically, main cache should not exceed its capacity
        assert!(cache.main_cache.len() <= cache.max_size);
    }
}
