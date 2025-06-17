# Replica Coordination and Shared State Design

## Current Issues

1. **Metrics Issue**: ~~The `heimdall_upstream_response_time_seconds_bucket` histogram is not recording individual response times correctly. Currently, it only records the average response time during periodic metric updates, causing all buckets to increment at the same rate.~~ **FIXED** - Now recording individual response times as they occur.

2. **Replica Isolation**: Each Heimdall replica operates independently with its own:
   - Cache (with persistence to disk)
   - Health statistics for upstream servers
   - Query deduplication state
   - Connection pools

## Proposed Solutions

### 1. Fix Metrics Recording (Immediate)

The metrics should record individual response times as they occur, not just averages during periodic updates.

**Implementation**:
- Add a metrics reference to the resolver
- Call `metrics.upstream_response_time.with_label_values(&[server]).observe(elapsed.as_secs_f64())` immediately after each query completes
- Remove the average response time recording from `update_from_resolver`

### 2. Inter-Replica Communication Options

#### Option A: Shared Redis Backend
Use Redis for shared state across replicas:
- **Shared Cache**: Move DNS cache to Redis (with TTL support)
- **Health Statistics**: Store upstream server health in Redis
- **Query Deduplication**: Use Redis for cluster-wide deduplication
- **Pros**: Simple, battle-tested, good Rust support
- **Cons**: Additional infrastructure dependency, potential latency

#### Option B: Kubernetes StatefulSet with Gossip Protocol
Convert to StatefulSet and implement peer discovery:
- Use headless service for peer discovery
- Implement gossip protocol (e.g., using `async-gossip` crate)
- Share health statistics and cache entries via gossip
- **Pros**: No external dependencies, resilient
- **Cons**: More complex implementation, eventual consistency

#### Option C: Hazelcast In-Memory Data Grid
Use Hazelcast for distributed data structures:
- Distributed cache with automatic replication
- Distributed metrics aggregation
- **Pros**: Rich features, automatic discovery in K8s
- **Cons**: JVM dependency, resource overhead

#### Option D: gRPC Mesh Communication
Implement direct replica-to-replica communication:
- Each replica exposes gRPC service
- Use K8s endpoints API for peer discovery
- Exchange health stats and cache entries directly
- **Pros**: Low latency, flexible protocol
- **Cons**: Complex mesh management

## Recommended Approach

### Phase 1: Fix Metrics (Immediate) ✅ COMPLETED
1. ✅ Pass metrics reference to resolver
2. ✅ Record individual response times as they occur
3. ✅ Keep aggregated stats for health monitoring

**Implementation Details**:
- Modified `DnsResolver::new()` to accept optional metrics reference
- Added metrics recording in both parallel and sequential query paths
- Removed duplicate average recording from periodic updates
- Metrics now accurately reflect individual query response times

### Phase 2: Redis Shared Cache (Short-term)
1. Add optional Redis backend for cache
2. Keep local cache as L1, Redis as L2
3. Share only frequently accessed entries

### Phase 3: Gossip Protocol for Health Stats (Medium-term)
1. Implement lightweight gossip for health statistics
2. Each replica maintains full view of cluster health
3. Use consensus for upstream server selection

## Implementation Details

### Metrics Fix Example
```rust
// In resolver.rs
pub struct DnsResolver {
    // ... existing fields ...
    metrics: Option<Arc<DnsMetrics>>,
}

// After query completes
if let Some(metrics) = &self.metrics {
    metrics.upstream_response_time
        .with_label_values(&[&server.to_string()])
        .observe(elapsed.as_secs_f64());
}
```

### Redis Integration Example
```rust
// New cache backend trait
trait CacheBackend: Send + Sync {
    async fn get(&self, key: &str) -> Option<CachedEntry>;
    async fn set(&self, key: &str, entry: CachedEntry, ttl: Duration);
    async fn remove(&self, key: &str);
}

// Redis implementation
struct RedisCache {
    client: redis::aio::ConnectionManager,
}

// Layered cache
struct LayeredCache {
    l1: LocalCache,
    l2: Option<Box<dyn CacheBackend>>,
}
```

### Kubernetes Service Discovery
```yaml
# Headless service for peer discovery
apiVersion: v1
kind: Service
metadata:
  name: heimdall-peers
spec:
  clusterIP: None
  selector:
    app: heimdall
  ports:
  - name: gossip
    port: 7946
```

## Benefits of Coordination

1. **Unified Health View**: All replicas share the same view of upstream server health
2. **Better Cache Hit Rates**: Shared cache increases overall hit rate
3. **Reduced Upstream Load**: Cluster-wide query deduplication
4. **Consistent Metrics**: Aggregated metrics across all replicas
5. **Improved Failover**: Faster detection of unhealthy upstreams

## Considerations

1. **Complexity**: Each coordination method adds operational complexity
2. **Latency**: Shared state access may add latency to queries
3. **Partition Tolerance**: Must handle network splits gracefully
4. **Resource Usage**: Additional memory/CPU for coordination
5. **Backward Compatibility**: Should work with standalone deployments

## Next Steps

1. **Immediate**: Fix metrics recording issue
2. **Evaluate**: Test Redis integration for shared cache
3. **Prototype**: Build gossip protocol for health sharing
4. **Benchmark**: Compare performance of different approaches
5. **Gradual Rollout**: Implement features behind feature flags