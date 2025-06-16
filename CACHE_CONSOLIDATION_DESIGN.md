# Cache Consolidation Design

## Current State Analysis

We have identified 4 separate cache implementations:

1. **Standard DnsCache** (`src/cache/mod.rs`)
   - Features: DashMap-based, LRU eviction, persistence (rkyv), domain trie, negative caching
   - Lines: 585
   - Full-featured, production-ready

2. **OptimizedDnsCache** (`src/cache/optimized_cache.rs`)
   - Features: Wraps LockFreeDnsCache, adds hot cache layer, access tracking
   - Lines: 285
   - Missing: persistence, domain trie, full negative caching

3. **LockFreeDnsCache** (`src/cache/lockfree_cache.rs`)
   - Features: Lock-free LRU, basic operations
   - Lines: 185
   - Missing: persistence, domain trie, many features

4. **CacheWrapper** (`src/cache/cache_wrapper.rs`)
   - Purpose: Enum wrapper to switch between implementations
   - Lines: 124
   - Adds complexity without value

Additionally:
- **LayeredCache** with Redis backend for L2 caching
- Duplicate configuration in `CacheConfig` and `DnsConfig`

## Unified Cache Design

### Architecture

```
┌─────────────────────────────────────────────┐
│           Unified DNS Cache                 │
├─────────────────────────────────────────────┤
│  Hot Cache Layer (10% capacity)             │
│  - Lock-free DashMap                        │
│  - Most frequently accessed entries         │
├─────────────────────────────────────────────┤
│  Main Cache Layer (90% capacity)            │
│  - Lock-free DashMap with sharded LRU       │
│  - Domain trie for wildcard lookups         │
│  - String interning for memory efficiency   │
├─────────────────────────────────────────────┤
│  Optional L2 Cache (Redis)                  │
│  - For distributed deployments              │
│  - Async interface                          │
├─────────────────────────────────────────────┤
│  Persistence Layer                          │
│  - Zero-copy rkyv serialization             │
│  - Atomic file operations                   │
└─────────────────────────────────────────────┘
```

### Key Features to Preserve

1. **From Standard Cache:**
   - Complete negative caching (RFC 2308)
   - Domain trie for efficient lookups
   - rkyv persistence
   - Comprehensive statistics
   - String interning

2. **From Optimized Cache:**
   - Hot cache layer for frequently accessed entries
   - Access tracking for promotion
   - Lock-free operations

3. **From LockFree Cache:**
   - Sharded eviction lists to reduce contention
   - Approximate LRU with sampling

4. **From LayeredCache:**
   - Optional Redis L2 backend
   - Distributed caching support

### Implementation Plan

1. Create new `UnifiedDnsCache` struct combining all features
2. Use best practices from each implementation:
   - Lock-free DashMap for both hot and main cache
   - Sharded LRU from lockfree implementation
   - Domain trie and persistence from standard cache
   - Hot cache promotion from optimized cache
3. Single configuration in `DnsConfig` (remove `CacheConfig`)
4. Async trait for optional L2 backends

### API Design

```rust
pub struct UnifiedDnsCache {
    // Hot cache layer
    hot_cache: DashMap<CacheKey, Arc<CacheEntry>>,
    hot_cache_size: usize,
    
    // Main cache layer
    main_cache: DashMap<CacheKey, Arc<CacheEntry>>,
    max_size: usize,
    
    // LRU tracking with sharding
    lru_shards: Vec<RwLock<VecDeque<CacheKey>>>,
    
    // Domain trie for wildcard lookups
    domain_trie: RwLock<DomainTrie>,
    
    // String interner
    string_interner: StringInterner,
    
    // Statistics
    stats: CacheStats,
    
    // Access tracking for hot cache promotion
    access_tracker: DashMap<CacheKey, AtomicU32>,
    promotion_threshold: u32,
    
    // Persistence
    cache_file_path: Option<String>,
    
    // Optional L2 cache
    l2_cache: Option<Arc<dyn CacheBackend>>,
    
    // Configuration
    negative_ttl: u32,
}

impl UnifiedDnsCache {
    pub fn new(config: &DnsConfig) -> Self;
    pub fn with_l2_cache(config: &DnsConfig, l2: Arc<dyn CacheBackend>) -> Self;
    
    // Core operations
    pub fn get(&self, key: &CacheKey) -> Option<DNSPacket>;
    pub fn put(&self, key: CacheKey, response: DNSPacket);
    
    // Management
    pub fn cleanup_expired(&self);
    pub fn clear(&self);
    
    // Persistence
    pub async fn save_to_disk(&self) -> Result<()>;
    pub async fn load_from_disk(&self) -> Result<()>;
    
    // Stats and debugging
    pub fn stats(&self) -> &CacheStats;
    pub fn debug_info(&self) -> String;
}
```

### Migration Strategy

1. Implement `UnifiedDnsCache` with all features
2. Update `DnsResolver` to use `UnifiedDnsCache` directly (no wrapper)
3. Remove `CacheWrapper`, `OptimizedDnsCache`, `LockFreeDnsCache`
4. Consolidate configuration
5. Update tests

### Expected Benefits

- **Code reduction**: ~2,000 lines (from 4 implementations to 1)
- **Performance**: Best of all implementations
- **Maintainability**: Single implementation to optimize and debug
- **Features**: All features available everywhere
- **Simplicity**: No wrapper enum, no configuration confusion