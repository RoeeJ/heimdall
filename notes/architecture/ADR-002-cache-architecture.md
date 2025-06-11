# ADR-002: Cache Architecture and Implementation

## Status
Accepted

## Context
DNS caching is critical for DNS server performance and efficiency. Requirements include:
- Sub-millisecond response times for cached queries
- Concurrent access from multiple async tasks
- TTL-aware expiration per RFC standards
- Persistence across server restarts
- Distributed caching for clustered deployments
- RFC 2308 compliant negative caching

Traditional approaches include:
1. **HashMap + Mutex**: Simple but poor concurrent performance
2. **RwLock-based caching**: Better read performance but writer starvation
3. **External cache only**: Higher latency, network dependency
4. **Single-tier in-memory**: No persistence or distribution

## Decision
We implemented a **two-tier cache architecture** with the following design:

### 1. Technology Choices

#### DashMap for Local Cache
```rust
use dashmap::DashMap;

pub struct LocalCacheBackend {
    cache: DashMap<String, CacheEntry>,
    stats: Arc<CacheStatistics>,
    // ...
}
```
- Lock-free concurrent reads for optimal performance
- Fine-grained locking via sharded buckets
- Built-in LRU eviction support

#### rkyv for Serialization
```rust
#[derive(Archive, Serialize, Deserialize)]
pub struct CacheEntry {
    response: Vec<u8>,
    expiry: u64,
    record_type: RecordType,
    negative: bool,
}
```
- Zero-copy deserialization for persistence
- 83% smaller files vs JSON
- Version-aware for future compatibility

### 2. Two-Tier Architecture

#### L1 Cache (Local)
- **Storage**: In-memory DashMap
- **Access Time**: ~250ns per lookup
- **Capacity**: 10,000 entries default
- **Eviction**: LRU with TTL expiration

#### L2 Cache (Redis)
- **Storage**: Distributed Redis instance
- **Access Time**: 1-5ms (network latency)
- **Capacity**: Memory-limited, configurable
- **Promotion**: L2 hits promoted to L1

### 3. TTL Management (RFC 2308 Compliant)
```rust
impl CacheEntry {
    pub fn is_expired(&self) -> bool {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() > self.expiry
    }
    
    pub fn remaining_ttl(&self) -> u32 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        if now >= self.expiry {
            0
        } else {
            (self.expiry - now) as u32
        }
    }
}
```

### 4. Negative Caching Implementation
- SOA-based TTL extraction for NXDOMAIN/NODATA responses
- Separate tracking for different negative response types
- Minimum TTL enforcement per SOA records

## Consequences

### Positive
- **Performance**: Sub-millisecond L1 hits, ~85% hit rate with L2
- **Scalability**: Lock-free concurrent access scales with CPU cores
- **Reliability**: Persistent cache survives restarts, Redis failover
- **RFC Compliance**: Proper negative caching per RFC 2308
- **Distribution**: Shared cache improves cluster-wide hit rates

### Negative
- **Memory Usage**: Dual caching increases memory footprint
- **Complexity**: Two-tier coordination adds implementation complexity
- **Dependencies**: Redis dependency for distributed functionality

### Trade-offs Accepted
1. **Memory vs Performance**: Higher memory usage for better hit rates
2. **Complexity vs Features**: More complex code for distributed caching
3. **Dependency vs Capability**: Redis dependency for clustering benefits

## Benchmarks
```
Cache Performance:
- L1 hit: 257 ns/lookup
- L2 hit: 1-5 ms/lookup  
- Cache miss: 50-100 ms (upstream query)

Storage Efficiency:
- rkyv format: 83% smaller than JSON
- Zero-copy deserialize: 10-100x faster than JSON parsing
- Persistent cache: 95% hit rate preservation across restarts
```

## Implementation Details

### Cache Entry Structure
```rust
#[derive(Archive, Serialize, Deserialize, Clone)]
pub struct CacheEntry {
    /// The DNS response packet
    response: Vec<u8>,
    
    /// Unix timestamp when entry expires
    expiry: u64,
    
    /// DNS record type for statistics
    record_type: RecordType,
    
    /// Whether this is a negative cache entry
    negative: bool,
    
    /// SOA minimum TTL for negative responses
    soa_minimum: Option<u32>,
}
```

### Two-Tier Coordination
```rust
impl CacheManager {
    pub async fn get(&self, key: &str) -> Option<CacheEntry> {
        // 1. Try L1 cache first
        if let Some(entry) = self.local.get(key).await {
            return Some(entry);
        }
        
        // 2. Try L2 cache (Redis)
        if let Some(entry) = self.redis.get(key).await {
            // Promote to L1
            self.local.set(key.to_string(), entry.clone()).await;
            return Some(entry);
        }
        
        None
    }
}
```

### Persistence Implementation
```rust
impl LocalCacheBackend {
    pub async fn save_to_disk(&self) -> Result<(), Box<dyn Error>> {
        let temp_file = format!("{}.tmp", self.file_path);
        
        // Serialize to temporary file
        let data = rkyv::to_bytes::<_, 256>(&cache_snapshot)?;
        fs::write(&temp_file, &data).await?;
        
        // Atomic rename
        fs::rename(&temp_file, &self.file_path).await?;
        
        Ok(())
    }
}
```

## Alternatives Considered

### 1. HashMap + Mutex (Rejected)
- **Pros**: Simple implementation, low memory overhead
- **Cons**: Poor concurrent performance, mutex contention
- **Reason**: Performance requirements incompatible with locking overhead

### 2. Single Redis Cache (Rejected)
- **Pros**: Simple architecture, natural distribution
- **Cons**: Network latency for all requests, single point of failure
- **Reason**: Sub-millisecond response requirements need local cache

### 3. evmap/left-right (Rejected)
- **Pros**: Very fast concurrent reads
- **Cons**: Complex writer coordination, memory overhead
- **Reason**: DashMap provides similar performance with simpler API

### 4. JSON Serialization (Rejected)
- **Pros**: Human-readable, widely supported
- **Cons**: Large file sizes, slow parsing
- **Reason**: Performance and storage efficiency requirements

## Performance Analysis

### Cache Hit Distribution
```
Scenario: 10,000 queries over 5 minutes
- L1 hits: 7,500 (75%) - avg 257ns
- L2 hits: 1,500 (15%) - avg 2.1ms  
- Cache miss: 1,000 (10%) - avg 75ms
- Overall avg: 8.2ms response time
```

### Memory Efficiency
- **L1 Cache**: ~50MB for 10,000 entries
- **Persistent File**: ~12MB (rkyv) vs ~70MB (JSON)
- **Redis Memory**: Shared across cluster members

## Future Considerations
- **Bloom filters**: Reduce L2 cache misses
- **Compression**: LZ4 compression for L2 storage
- **Sharding**: Horizontal partitioning for very large caches
- **Analytics**: Query pattern analysis for cache optimization

## References
- RFC 1035: Domain Names - Implementation and Specification
- RFC 2308: Negative Caching of DNS Queries
- DashMap documentation: https://docs.rs/dashmap
- rkyv documentation: https://docs.rs/rkyv
- Internal benchmarks: `/benches/dns_performance.rs`