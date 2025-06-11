# ADR-003: Redis L2 Cache and Distributed Caching Strategy

## Status
Accepted

## Context
DNS caching in distributed/clustered environments faces several challenges:
- Cache warming time for new instances
- Duplicate upstream queries across replicas
- Memory efficiency across cluster
- Service discovery and coordination
- High availability requirements

In single-node deployments, local caching is sufficient. However, in clustered Kubernetes deployments with multiple replicas, each instance maintains its own cache, leading to:
- Redundant upstream queries for the same domains
- Slower cache warm-up for new pods
- Higher overall upstream load
- Inefficient memory utilization

## Decision
We implemented a **Redis-based L2 distributed cache** with the following architecture:

### 1. Two-Tier Cache Architecture
```rust
pub struct CacheManager {
    local: Arc<LocalCacheBackend>,      // L1: DashMap-based
    redis: Option<Arc<RedisCacheBackend>>, // L2: Redis-based
    cluster_registry: Arc<ClusterRegistry>,
}
```

**L1 Cache (Local)**: DashMap for sub-millisecond access
**L2 Cache (Redis)**: Distributed sharing across cluster members

### 2. Redis vs Alternatives

#### Redis Selected Over:
- **Memcached**: Less rich TTL handling, no persistence
- **Apache Ignite**: Overly complex for DNS caching use case
- **Hazelcast**: Commercial licensing, heavyweight
- **Etcd**: Designed for configuration, not high-throughput caching

#### Key Redis Advantages:
- Native TTL support with automatic expiration
- Optional persistence (AOF/RDB) for cache survival
- Kubernetes operator ecosystem
- Simple deployment and management
- Strong Rust client library (`redis-rs`)

### 3. Cluster Discovery: Redis-based vs DNS-based

#### Redis-based Discovery (Implemented)
```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct ClusterMember {
    pub id: String,
    pub hostname: String,
    pub pod_ip: String,
    pub last_heartbeat: u64,
    pub status: MemberStatus,
    pub stats: MemberStats,
}
```

**Advantages:**
- Rich metadata storage (health, stats, performance)
- TTL-based automatic cleanup (10s TTL, 5s heartbeat)
- No circular dependency (DNS server doing DNS lookups)
- Real-time status updates

#### DNS-based Discovery (Rejected)
**Problems:**
- Circular dependency: DNS server performing DNS queries
- Limited to IP addresses only
- DNS caching conflicts with real-time updates
- Requires separate health checking mechanism

### 4. Auto-Detection and Configuration

#### Environment-based Auto-Configuration
```rust
impl RedisCacheBackend {
    pub async fn auto_detect() -> Option<Self> {
        // 1. Explicit configuration
        if let Ok(url) = env::var("HEIMDALL_REDIS_URL") {
            return Self::new(&url).await.ok();
        }
        
        // 2. Standard Redis URL
        if let Ok(url) = env::var("REDIS_URL") {
            return Self::new(&url).await.ok();
        }
        
        // 3. Kubernetes service discovery
        if let Ok(host) = env::var("HEIMDALL_REDIS_SERVICE_HOST") {
            let port = env::var("HEIMDALL_REDIS_SERVICE_PORT").unwrap_or_else(|_| "6379".to_string());
            let url = format!("redis://{}:{}", host, port);
            return Self::new(&url).await.ok();
        }
        
        // 4. Default Kubernetes service name
        Self::new("redis://heimdall-redis:6379").await.ok()
    }
}
```

## Consequences

### Positive
- **Improved Hit Rate**: 85% vs 60% with L2 sharing
- **Faster Warm-up**: New pods immediately benefit from shared cache
- **Reduced Upstream Load**: 62% reduction in duplicate queries
- **Better Resource Utilization**: Shared memory across cluster
- **Zero Configuration**: Auto-detection in Kubernetes environments

### Negative
- **Additional Dependency**: Redis required for distributed benefits
- **Increased Complexity**: Two-tier cache coordination
- **Network Latency**: 1-5ms for L2 hits vs <1μs for L1 hits
- **Memory Overhead**: ~30% increase for distributed setup

### Trade-offs Accepted
1. **Performance vs Distribution**: Slightly higher latency for much better hit rates
2. **Simplicity vs Capability**: Added complexity for distributed caching benefits
3. **Independence vs Efficiency**: Redis dependency for shared cache benefits

## Performance Analysis

### Cache Hit Distribution (3 replicas, 10,000 queries)

**Without Redis L2:**
- L1 hits: 60% (250ns avg)
- Cache miss: 40% (75ms avg)
- **Overall: 30.1ms avg response time**

**With Redis L2:**
- L1 hits: 60% (250ns avg)
- L2 hits: 25% (2.1ms avg)
- Cache miss: 15% (75ms avg)
- **Overall: 11.8ms avg response time (61% improvement)**

### Resource Impact
```
Memory Usage (per replica):
- L1 Cache: 50MB (unchanged)
- Redis Connection: ~5MB
- Total increase: ~10%

Network Bandwidth:
- Redis traffic: ~20KB/minute per replica
- Heartbeat traffic: ~1KB/minute per replica
```

## Implementation Details

### Connection Management
```rust
impl RedisCacheBackend {
    pub async fn new(redis_url: &str) -> Result<Self, Box<dyn Error>> {
        let client = redis::Client::open(redis_url)?;
        let connection_manager = ConnectionManager::new(client).await?;
        
        Ok(Self {
            client: connection_manager,
            stats: Arc::new(CacheStatistics::new()),
        })
    }
}
```

### Failover Strategy
```rust
impl CacheManager {
    pub async fn get(&self, key: &str) -> Option<CacheEntry> {
        // L1 cache first
        if let Some(entry) = self.local.get(key).await {
            return Some(entry);
        }
        
        // L2 cache with graceful degradation
        if let Some(redis) = &self.redis {
            if let Ok(Some(entry)) = redis.get(key).await {
                // Promote to L1
                self.local.set(key.to_string(), entry.clone()).await;
                return Some(entry);
            }
        }
        
        None
    }
}
```

### Kubernetes Integration
```yaml
# Helm chart values
redis:
  enabled: true
  persistence:
    enabled: true
    size: 10Gi
  auth:
    enabled: false
  resources:
    requests:
      memory: 256Mi
      cpu: 100m
```

## Alternatives Considered

### 1. Pure L1 Caching (Rejected)
- **Pros**: Simple, no dependencies, lowest latency
- **Cons**: Poor cache utilization across replicas, slow warm-up
- **Reason**: Inefficient in multi-replica deployments

### 2. External Memcached (Rejected)
- **Pros**: Simple key-value semantics
- **Cons**: No native TTL, no persistence, limited metadata
- **Reason**: Redis provides better feature set for similar complexity

### 3. Database-backed Cache (Rejected)
- **Pros**: Strong consistency, complex queries
- **Cons**: High latency, complex schema, overkill for caching
- **Reason**: Performance requirements incompatible with database latency

### 4. DNS-based Discovery (Rejected)
- **Pros**: Native DNS integration
- **Cons**: Circular dependency, limited metadata, caching conflicts
- **Reason**: Architectural complexity and reliability concerns

## Security Considerations
- **Network Isolation**: Redis accessible only within Kubernetes cluster
- **Authentication**: Optional but disabled by default (DNS cache contains public data)
- **Encryption**: Can be enabled via Redis TLS configuration
- **Network Policies**: Restrict Redis access to Heimdall pods only

## Future Enhancements
- **Redis Cluster**: Horizontal scaling for very large deployments
- **Compression**: LZ4 compression for reduced network usage
- **Analytics**: Cache pattern analysis across cluster
- **Geo-distribution**: Regional cache distribution for global deployments

## References
- Redis documentation: https://redis.io/documentation
- Kubernetes service discovery: https://kubernetes.io/docs/concepts/services-networking/
- redis-rs client: https://docs.rs/redis
- Internal benchmarks: `/benches/dns_performance.rs`
- Cluster registry implementation: `/src/cluster_registry.rs`