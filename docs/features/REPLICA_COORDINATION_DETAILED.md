# Detailed Replica Coordination Approaches for Heimdall DNS

## Overview

Currently, each Heimdall replica operates independently, leading to:
- Duplicate upstream queries across replicas
- Inconsistent cache states
- No shared view of upstream server health
- Redundant connection pools
- Separate metrics that need external aggregation

This document expands on four approaches to enable inter-replica coordination.

## Option A: Redis Backend for Shared State

### Architecture
```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Heimdall   │     │  Heimdall   │     │  Heimdall   │
│  Replica 1  │     │  Replica 2  │     │  Replica 3  │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │
       └───────────────────┴───────────────────┘
                           │
                    ┌──────▼──────┐
                    │    Redis    │
                    │   Cluster   │
                    └─────────────┘
```

### Implementation Details

#### 1. Shared Cache Layer
```rust
// Redis cache backend implementation
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, RedisResult};

pub struct RedisCache {
    client: ConnectionManager,
    local_cache: DashMap<String, CachedEntry>, // L1 cache
    ttl_offset: i64, // Account for clock drift
}

impl RedisCache {
    pub async fn get(&self, key: &str) -> Option<DNSPacket> {
        // Try L1 cache first
        if let Some(entry) = self.local_cache.get(key) {
            if !entry.is_expired() {
                return Some(entry.packet.clone());
            }
        }
        
        // Try Redis (L2 cache)
        let redis_key = format!("dns:cache:{}", key);
        let result: RedisResult<Vec<u8>> = self.client.get(&redis_key).await;
        
        if let Ok(data) = result {
            if let Ok(packet) = bincode::deserialize::<DNSPacket>(&data) {
                // Store in L1 for faster subsequent access
                self.local_cache.insert(key.to_string(), CachedEntry {
                    packet: packet.clone(),
                    expires_at: Instant::now() + Duration::from_secs(300),
                });
                return Some(packet);
            }
        }
        
        None
    }
    
    pub async fn set(&self, key: &str, packet: DNSPacket, ttl: Duration) {
        // Store in both L1 and L2
        self.local_cache.insert(key.to_string(), CachedEntry {
            packet: packet.clone(),
            expires_at: Instant::now() + ttl,
        });
        
        let redis_key = format!("dns:cache:{}", key);
        let data = bincode::serialize(&packet).unwrap();
        let _: RedisResult<()> = self.client
            .set_ex(&redis_key, data, ttl.as_secs())
            .await;
    }
}
```

#### 2. Distributed Health Tracking
```rust
// Store health metrics in Redis with atomic operations
pub struct RedisHealthTracker {
    client: ConnectionManager,
}

impl RedisHealthTracker {
    pub async fn record_success(&self, server: &SocketAddr, response_time: Duration) {
        let key = format!("dns:health:{}:success", server);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        
        // Use Redis sorted sets for time-series data
        let _: RedisResult<()> = self.client
            .zadd(&key, timestamp, response_time.as_millis())
            .await;
        
        // Trim old entries (keep last hour)
        let cutoff = timestamp - 3600;
        let _: RedisResult<()> = self.client
            .zremrangebyscore(&key, 0, cutoff)
            .await;
        
        // Update success counter
        let counter_key = format!("dns:health:{}:total", server);
        let _: RedisResult<()> = self.client.incr(&counter_key, 1).await;
    }
    
    pub async fn get_server_stats(&self, server: &SocketAddr) -> ServerStats {
        let key = format!("dns:health:{}:success", server);
        let recent: Vec<(String, f64)> = self.client
            .zrevrange_withscores(&key, 0, 100)
            .await
            .unwrap_or_default();
        
        // Calculate average response time from recent queries
        let avg_response_time = if !recent.is_empty() {
            let sum: f64 = recent.iter().map(|(_, score)| score).sum();
            Duration::from_millis((sum / recent.len() as f64) as u64)
        } else {
            Duration::from_millis(0)
        };
        
        ServerStats {
            avg_response_time,
            success_count: recent.len(),
            // ... other stats
        }
    }
}
```

#### 3. Query Deduplication Across Cluster
```rust
// Use Redis for cluster-wide query deduplication
pub struct RedisQueryDedup {
    client: ConnectionManager,
}

impl RedisQueryDedup {
    pub async fn acquire_query_lock(&self, query_key: &str) -> Option<QueryLock> {
        let lock_key = format!("dns:query:lock:{}", query_key);
        let lock_id = Uuid::new_v4().to_string();
        
        // Try to acquire lock with 5-second expiry
        let result: RedisResult<bool> = self.client
            .set_nx_ex(&lock_key, &lock_id, 5)
            .await;
        
        if result.unwrap_or(false) {
            Some(QueryLock {
                key: lock_key,
                id: lock_id,
                client: self.client.clone(),
            })
        } else {
            None
        }
    }
    
    pub async fn wait_for_result(&self, query_key: &str, timeout: Duration) -> Option<DNSPacket> {
        let result_key = format!("dns:query:result:{}", query_key);
        let pubsub_key = format!("dns:query:notify:{}", query_key);
        
        // Subscribe to completion notification
        let mut pubsub = self.client.get_async_connection().await.ok()?;
        pubsub.subscribe(&pubsub_key).await.ok()?;
        
        // Wait for notification or timeout
        let result = timeout_at(Instant::now() + timeout, async {
            let mut stream = pubsub.on_message();
            stream.next().await
        }).await;
        
        if result.is_ok() {
            // Fetch the result
            let data: RedisResult<Vec<u8>> = self.client.get(&result_key).await;
            if let Ok(data) = data {
                return bincode::deserialize(&data).ok();
            }
        }
        
        None
    }
}
```

### Pros
- **Mature Technology**: Redis is battle-tested with excellent Rust support
- **Flexible Data Structures**: Supports various data types (strings, sets, sorted sets, streams)
- **Pub/Sub Support**: Built-in messaging for real-time coordination
- **Persistence Options**: Can persist to disk for disaster recovery
- **Monitoring**: Rich ecosystem of monitoring tools

### Cons
- **Additional Infrastructure**: Requires Redis cluster deployment and maintenance
- **Network Latency**: Adds ~1-5ms per Redis operation
- **Consistency**: Eventual consistency model may lead to stale reads
- **Cost**: Additional memory and compute resources for Redis
- **Single Point of Failure**: Without Redis Cluster/Sentinel

### Deployment Example
```yaml
apiVersion: v1
kind: Service
metadata:
  name: heimdall-redis
spec:
  ports:
  - port: 6379
  selector:
    app: redis
---
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: redis
spec:
  serviceName: heimdall-redis
  replicas: 3
  selector:
    matchLabels:
      app: redis
  template:
    metadata:
      labels:
        app: redis
    spec:
      containers:
      - name: redis
        image: redis:7-alpine
        ports:
        - containerPort: 6379
        command: ["redis-server"]
        args: ["--appendonly", "yes", "--cluster-enabled", "yes"]
        volumeMounts:
        - name: data
          mountPath: /data
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 10Gi
```

## Option B: Kubernetes StatefulSet with Gossip Protocol

### Architecture
```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Heimdall-0    │◄────┤   Heimdall-1    ├────►│   Heimdall-2    │
│  (StatefulSet)  │     │  (StatefulSet)  │     │  (StatefulSet)  │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │                       │                       │
         └───────────────────────┴───────────────────────┘
                        Gossip Protocol (P2P)
                         UDP Port 7946
```

### Implementation Details

#### 1. Peer Discovery Using Kubernetes API
```rust
use k8s_openapi::api::core::v1::Endpoints;
use kube::{Api, Client};

pub struct K8sPeerDiscovery {
    client: Client,
    namespace: String,
    service_name: String,
}

impl K8sPeerDiscovery {
    pub async fn discover_peers(&self) -> Result<Vec<SocketAddr>> {
        let api: Api<Endpoints> = Api::namespaced(self.client.clone(), &self.namespace);
        let endpoints = api.get(&self.service_name).await?;
        
        let mut peers = Vec::new();
        if let Some(subsets) = endpoints.subsets {
            for subset in subsets {
                if let Some(addresses) = subset.addresses {
                    for addr in addresses {
                        if let Some(ip) = addr.ip {
                            // Gossip port is different from DNS port
                            let peer_addr = format!("{}:7946", ip).parse()?;
                            peers.push(peer_addr);
                        }
                    }
                }
            }
        }
        
        Ok(peers)
    }
    
    pub async fn watch_peers(&self) -> impl Stream<Item = Vec<SocketAddr>> {
        let api: Api<Endpoints> = Api::namespaced(self.client.clone(), &self.namespace);
        
        // Watch for endpoint changes
        let stream = watcher(api, Default::default())
            .try_filter_map(|event| async move {
                match event {
                    Event::Applied(endpoints) => {
                        let peers = Self::extract_peers(&endpoints);
                        Ok(Some(peers))
                    }
                    Event::Deleted(_) => Ok(Some(Vec::new())),
                    _ => Ok(None),
                }
            });
        
        stream
    }
}
```

#### 2. Gossip Protocol Implementation
```rust
use memberlist::{Config, Memberlist, Node};

pub struct GossipCluster {
    memberlist: Arc<Memberlist>,
    state: Arc<RwLock<ClusterState>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ClusterState {
    cache_entries: HashMap<String, CacheMetadata>,
    health_stats: HashMap<SocketAddr, ServerHealth>,
    query_locks: HashMap<String, QueryLock>,
}

impl GossipCluster {
    pub async fn new(pod_name: String, gossip_port: u16) -> Result<Self> {
        let config = Config {
            name: pod_name,
            bind_addr: "0.0.0.0".to_string(),
            bind_port: gossip_port,
            // Use SWIM protocol for failure detection
            protocol_version: 4,
            gossip_interval: Duration::from_millis(200),
            probe_timeout: Duration::from_secs(1),
            suspicion_multiplier: 4,
            ..Default::default()
        };
        
        let state = Arc::new(RwLock::new(ClusterState::default()));
        let delegate = GossipDelegate { state: state.clone() };
        
        let memberlist = Memberlist::create(config, delegate).await?;
        
        Ok(Self { memberlist, state })
    }
    
    pub async fn share_cache_entry(&self, key: String, metadata: CacheMetadata) {
        // Update local state
        self.state.write().await.cache_entries.insert(key.clone(), metadata.clone());
        
        // Broadcast to cluster
        let update = StateUpdate::CacheEntry { key, metadata };
        let data = bincode::serialize(&update).unwrap();
        self.memberlist.broadcast(&data).await;
    }
    
    pub async fn get_cache_metadata(&self, key: &str) -> Option<CacheMetadata> {
        self.state.read().await.cache_entries.get(key).cloned()
    }
}

// Gossip delegate handles incoming updates
struct GossipDelegate {
    state: Arc<RwLock<ClusterState>>,
}

impl memberlist::Delegate for GossipDelegate {
    fn notify_msg(&self, msg: &[u8]) {
        if let Ok(update) = bincode::deserialize::<StateUpdate>(msg) {
            match update {
                StateUpdate::CacheEntry { key, metadata } => {
                    let state = self.state.clone();
                    tokio::spawn(async move {
                        state.write().await.cache_entries.insert(key, metadata);
                    });
                }
                StateUpdate::HealthStats { server, stats } => {
                    let state = self.state.clone();
                    tokio::spawn(async move {
                        state.write().await.health_stats.insert(server, stats);
                    });
                }
            }
        }
    }
    
    fn get_broadcast(&self, overhead: usize, limit: usize) -> Vec<Vec<u8>> {
        // Return pending updates to broadcast
        vec![]
    }
}
```

#### 3. Consistent Hashing for Cache Distribution
```rust
use consistent_hash::{ConsistentHash, Node};

pub struct DistributedCache {
    local_cache: DashMap<String, DNSPacket>,
    consistent_hash: RwLock<ConsistentHash<String>>,
    gossip: Arc<GossipCluster>,
    peers: Arc<RwLock<HashMap<String, PeerConnection>>>,
}

impl DistributedCache {
    pub async fn get(&self, key: &str) -> Option<DNSPacket> {
        let owner = self.get_key_owner(key).await;
        
        if owner == self.local_node_id() {
            // We own this key
            self.local_cache.get(key).map(|e| e.clone())
        } else {
            // Remote fetch
            if let Some(peer) = self.peers.read().await.get(&owner) {
                peer.fetch_cache_entry(key).await
            } else {
                None
            }
        }
    }
    
    pub async fn set(&self, key: &str, packet: DNSPacket, ttl: Duration) {
        let owner = self.get_key_owner(key).await;
        
        if owner == self.local_node_id() {
            // Store locally
            self.local_cache.insert(key.to_string(), packet);
            
            // Broadcast metadata
            self.gossip.share_cache_entry(key.to_string(), CacheMetadata {
                owner: owner.clone(),
                expires_at: SystemTime::now() + ttl,
                size: std::mem::size_of_val(&packet),
            }).await;
        } else {
            // Forward to owner
            if let Some(peer) = self.peers.read().await.get(&owner) {
                peer.store_cache_entry(key, packet, ttl).await;
            }
        }
    }
    
    async fn get_key_owner(&self, key: &str) -> String {
        let hash = self.consistent_hash.read().await;
        hash.get_node(key).unwrap_or_else(|| self.local_node_id())
    }
}
```

### Pros
- **No External Dependencies**: Uses only Kubernetes native features
- **Automatic Peer Discovery**: Leverages K8s service discovery
- **Fault Tolerant**: SWIM protocol handles node failures gracefully
- **Low Latency**: Direct peer-to-peer communication
- **Scalable**: Gossip protocol scales logarithmically

### Cons
- **Complex Implementation**: Requires implementing distributed systems primitives
- **Eventual Consistency**: Gossip propagation takes time
- **Network Overhead**: Continuous gossip traffic between nodes
- **Debugging Difficulty**: P2P systems are harder to debug
- **Split Brain Risk**: Network partitions can cause inconsistencies

### Deployment Configuration
```yaml
apiVersion: v1
kind: Service
metadata:
  name: heimdall-peers
  labels:
    app: heimdall
spec:
  ports:
  - port: 7946
    name: gossip
  clusterIP: None  # Headless service
  selector:
    app: heimdall
---
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: heimdall
spec:
  serviceName: heimdall-peers
  replicas: 3
  selector:
    matchLabels:
      app: heimdall
  template:
    metadata:
      labels:
        app: heimdall
    spec:
      containers:
      - name: heimdall
        image: heimdall:latest
        env:
        - name: POD_NAME
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: POD_NAMESPACE
          valueFrom:
            fieldRef:
              fieldPath: metadata.namespace
        - name: GOSSIP_PEERS
          value: "heimdall-peers.$(POD_NAMESPACE).svc.cluster.local"
        ports:
        - containerPort: 1053
          name: dns
          protocol: UDP
        - containerPort: 7946
          name: gossip
          protocol: UDP
```

## Option C: Hazelcast In-Memory Data Grid

### Architecture
```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Heimdall   │     │  Heimdall   │     │  Heimdall   │
│     +       │     │     +       │     │     +       │
│ Hazelcast   │◄────┤ Hazelcast   ├────►│ Hazelcast   │
│   Client    │     │   Client    │     │   Client    │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │
       └───────────────────┴───────────────────┘
                           │
                ┌──────────▼──────────┐
                │  Hazelcast Cluster  │
                │   (3-5 members)     │
                └─────────────────────┘
```

### Implementation Details

#### 1. Hazelcast Client Integration
```rust
// Using hazelcast-rust client (hypothetical API)
use hazelcast_rust::{Client, ClientConfig, IMap, ITopic};

pub struct HazelcastCache {
    client: Client,
    cache_map: IMap<String, Vec<u8>>,
    invalidation_topic: ITopic<CacheInvalidation>,
}

impl HazelcastCache {
    pub async fn new(cluster_members: Vec<String>) -> Result<Self> {
        let config = ClientConfig::new()
            .cluster_members(cluster_members)
            .connection_timeout(Duration::from_secs(5))
            .retry_config(RetryConfig::exponential_backoff(3))
            .near_cache_config(NearCacheConfig {
                max_size: 10000,
                eviction_policy: EvictionPolicy::LRU,
                invalidate_on_change: true,
            });
        
        let client = Client::new(config).await?;
        let cache_map = client.get_map("dns-cache").await?;
        let invalidation_topic = client.get_topic("cache-invalidations").await?;
        
        Ok(Self {
            client,
            cache_map,
            invalidation_topic,
        })
    }
    
    pub async fn get(&self, key: &str) -> Option<DNSPacket> {
        // Near cache provides local caching
        if let Some(data) = self.cache_map.get(key).await {
            bincode::deserialize(&data).ok()
        } else {
            None
        }
    }
    
    pub async fn set(&self, key: &str, packet: DNSPacket, ttl: Duration) {
        let data = bincode::serialize(&packet).unwrap();
        
        // Set with TTL
        self.cache_map
            .set_with_ttl(key, data, ttl.as_secs() as i64)
            .await
            .ok();
    }
    
    pub async fn invalidate(&self, pattern: &str) {
        // Publish invalidation event
        self.invalidation_topic
            .publish(CacheInvalidation {
                pattern: pattern.to_string(),
                timestamp: SystemTime::now(),
            })
            .await
            .ok();
    }
}
```

#### 2. Distributed Computing with Entry Processors
```rust
// Process data where it lives to minimize network traffic
pub struct HealthStatsProcessor;

impl EntryProcessor<String, HealthData, HealthStats> for HealthStatsProcessor {
    fn process(&self, entry: &mut Entry<String, HealthData>) -> HealthStats {
        let data = entry.get_value();
        
        // Calculate stats in-place
        let total_queries = data.success_count + data.failure_count;
        let success_rate = if total_queries > 0 {
            (data.success_count as f64 / total_queries as f64) * 100.0
        } else {
            0.0
        };
        
        HealthStats {
            server: entry.get_key().clone(),
            total_queries,
            success_rate,
            avg_response_time: data.total_response_time / data.success_count.max(1),
            last_check: data.last_check,
        }
    }
}

// Usage
let stats = hazelcast.health_map
    .execute_on_key(server_addr, HealthStatsProcessor)
    .await?;
```

#### 3. Distributed Locks and Semaphores
```rust
pub struct HazelcastQueryDedup {
    client: Client,
}

impl HazelcastQueryDedup {
    pub async fn try_acquire_query(&self, query_key: &str) -> Option<QueryHandle> {
        let lock = self.client.get_lock(&format!("query:{}", query_key)).await.ok()?;
        
        if lock.try_lock_with_timeout(Duration::from_millis(100)).await.ok()? {
            Some(QueryHandle { lock })
        } else {
            None
        }
    }
    
    pub async fn rate_limit_check(&self, client_ip: &str) -> bool {
        let semaphore = self.client
            .get_semaphore(&format!("ratelimit:{}", client_ip))
            .await
            .unwrap();
        
        // Initialize with 50 permits per second
        semaphore.init(50).await.ok();
        
        // Try to acquire permit
        semaphore.try_acquire().await.unwrap_or(false)
    }
}
```

### Pros
- **Rich Feature Set**: Distributed data structures, computing, messaging
- **Near Cache**: Automatic local caching with invalidation
- **Entry Processors**: Process data in-place to minimize network traffic
- **Auto-Discovery**: Automatic cluster discovery in Kubernetes
- **Split-Brain Protection**: Built-in split-brain handling

### Cons
- **JVM Dependency**: Hazelcast is Java-based, requires sidecar or embedded JVM
- **Resource Overhead**: Higher memory and CPU usage
- **Complexity**: More complex than simple key-value stores
- **Licensing**: Open source version has limitations
- **Language Barrier**: Limited native Rust support

### Deployment with Hazelcast Sidecar
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: heimdall
spec:
  template:
    spec:
      containers:
      - name: heimdall
        image: heimdall:latest
        env:
        - name: HAZELCAST_SERVICE
          value: "hazelcast-service:5701"
      - name: hazelcast
        image: hazelcast/hazelcast:5.3
        ports:
        - containerPort: 5701
        env:
        - name: JAVA_OPTS
          value: "-Xmx1g -XX:MaxRAMPercentage=80"
        - name: HZ_CLUSTERNAME
          value: "heimdall-cache"
        volumeMounts:
        - name: hazelcast-config
          mountPath: /opt/hazelcast/config
      volumes:
      - name: hazelcast-config
        configMap:
          name: hazelcast-config
```

## Option D: gRPC Mesh Communication

### Architecture
```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Heimdall-1    │────▶│   Heimdall-2    │────▶│   Heimdall-3    │
│  gRPC Server    │◀────│  gRPC Server    │◀────│  gRPC Server    │
└─────────────────┘     └─────────────────┘     └─────────────────┘
         ▲                       ▲                       ▲
         └───────────────────────┴───────────────────────┘
                    Full Mesh gRPC Connections
                         TLS + mTLS
```

### Implementation Details

#### 1. gRPC Service Definition
```protobuf
syntax = "proto3";

package heimdall.coordination;

service ReplicaCoordination {
  // Cache operations
  rpc GetCacheEntry(CacheKey) returns (CacheEntry);
  rpc SetCacheEntry(SetCacheRequest) returns (SetCacheResponse);
  rpc StreamCacheUpdates(CacheStreamRequest) returns (stream CacheUpdate);
  
  // Health sharing
  rpc ShareHealthStats(HealthUpdate) returns (Empty);
  rpc GetClusterHealth(Empty) returns (ClusterHealthResponse);
  
  // Query coordination
  rpc AcquireQueryLock(QueryLockRequest) returns (QueryLockResponse);
  rpc ReleaseQueryLock(QueryLockRelease) returns (Empty);
  
  // Metrics aggregation
  rpc CollectMetrics(MetricsRequest) returns (MetricsResponse);
}

message CacheKey {
  string key = 1;
}

message CacheEntry {
  bytes packet_data = 1;
  int64 expires_at = 2;
  string owner_replica = 3;
}

message HealthUpdate {
  string server_addr = 1;
  int64 response_time_ms = 2;
  bool success = 3;
  int64 timestamp = 4;
}
```

#### 2. gRPC Server Implementation
```rust
use tonic::{transport::Server, Request, Response, Status};

pub struct CoordinationService {
    local_cache: Arc<DnsCache>,
    peer_manager: Arc<PeerManager>,
    health_tracker: Arc<HealthTracker>,
}

#[tonic::async_trait]
impl ReplicaCoordination for CoordinationService {
    async fn get_cache_entry(
        &self,
        request: Request<CacheKey>,
    ) -> Result<Response<CacheEntry>, Status> {
        let key = request.into_inner().key;
        
        if let Some(entry) = self.local_cache.get(&key).await {
            let packet_data = bincode::serialize(&entry.packet)
                .map_err(|e| Status::internal(e.to_string()))?;
            
            Ok(Response::new(CacheEntry {
                packet_data,
                expires_at: entry.expires_at.as_secs() as i64,
                owner_replica: self.peer_manager.local_id().to_string(),
            }))
        } else {
            Err(Status::not_found("Cache entry not found"))
        }
    }
    
    async fn stream_cache_updates(
        &self,
        request: Request<CacheStreamRequest>,
    ) -> Result<Response<Self::StreamCacheUpdatesStream>, Status> {
        let (tx, rx) = mpsc::channel(100);
        let cache = self.local_cache.clone();
        
        // Spawn task to stream updates
        tokio::spawn(async move {
            let mut cache_events = cache.subscribe_events();
            
            while let Some(event) = cache_events.recv().await {
                let update = match event {
                    CacheEvent::Insert { key, packet, ttl } => {
                        CacheUpdate {
                            action: CacheAction::Insert as i32,
                            key,
                            packet_data: Some(bincode::serialize(&packet).unwrap()),
                            ttl_seconds: ttl.as_secs() as i64,
                        }
                    }
                    CacheEvent::Evict { key } => {
                        CacheUpdate {
                            action: CacheAction::Evict as i32,
                            key,
                            packet_data: None,
                            ttl_seconds: 0,
                        }
                    }
                };
                
                if tx.send(Ok(update)).await.is_err() {
                    break;
                }
            }
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
    
    async fn acquire_query_lock(
        &self,
        request: Request<QueryLockRequest>,
    ) -> Result<Response<QueryLockResponse>, Status> {
        let req = request.into_inner();
        let lock_manager = self.peer_manager.distributed_lock_manager();
        
        match lock_manager.try_acquire(&req.query_key, Duration::from_secs(5)).await {
            Ok(lock_id) => {
                Ok(Response::new(QueryLockResponse {
                    acquired: true,
                    lock_id: Some(lock_id),
                    owner: Some(self.peer_manager.local_id().to_string()),
                }))
            }
            Err(_) => {
                // Check who owns the lock
                if let Some(owner) = lock_manager.get_lock_owner(&req.query_key).await {
                    Ok(Response::new(QueryLockResponse {
                        acquired: false,
                        lock_id: None,
                        owner: Some(owner),
                    }))
                } else {
                    Err(Status::internal("Failed to acquire lock"))
                }
            }
        }
    }
}
```

#### 3. Peer Discovery and Management
```rust
pub struct PeerManager {
    local_id: String,
    peers: Arc<RwLock<HashMap<String, PeerConnection>>>,
    endpoints_watcher: EndpointsWatcher,
}

pub struct PeerConnection {
    id: String,
    client: ReplicaCoordinationClient<Channel>,
    health_check: Interval,
    last_seen: Arc<RwLock<Instant>>,
}

impl PeerManager {
    pub async fn new(namespace: &str, service: &str) -> Result<Self> {
        let local_id = std::env::var("POD_NAME")?;
        let endpoints_watcher = EndpointsWatcher::new(namespace, service).await?;
        
        let manager = Self {
            local_id,
            peers: Arc::new(RwLock::new(HashMap::new())),
            endpoints_watcher,
        };
        
        // Start peer discovery
        manager.start_peer_discovery().await;
        
        Ok(manager)
    }
    
    async fn start_peer_discovery(&self) {
        let peers = self.peers.clone();
        let local_id = self.local_id.clone();
        
        tokio::spawn(async move {
            let mut endpoint_stream = self.endpoints_watcher.watch().await;
            
            while let Some(endpoints) = endpoint_stream.next().await {
                let mut current_peers = peers.write().await;
                
                // Remove peers that are no longer in endpoints
                current_peers.retain(|id, _| endpoints.contains(id));
                
                // Add new peers
                for endpoint in endpoints {
                    if endpoint.id != local_id && !current_peers.contains_key(&endpoint.id) {
                        if let Ok(conn) = Self::connect_to_peer(&endpoint).await {
                            current_peers.insert(endpoint.id.clone(), conn);
                        }
                    }
                }
            }
        });
    }
    
    async fn connect_to_peer(endpoint: &Endpoint) -> Result<PeerConnection> {
        let tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(CA_CERT))
            .identity(Identity::from_pem(CLIENT_CERT, CLIENT_KEY));
        
        let channel = Channel::from_shared(format!("https://{}:8443", endpoint.addr))?
            .tls_config(tls_config)?
            .timeout(Duration::from_secs(5))
            .connect()
            .await?;
        
        let client = ReplicaCoordinationClient::new(channel);
        
        Ok(PeerConnection {
            id: endpoint.id.clone(),
            client,
            health_check: interval(Duration::from_secs(30)),
            last_seen: Arc::new(RwLock::new(Instant::now())),
        })
    }
    
    pub async fn broadcast_cache_update(&self, update: CacheUpdate) {
        let peers = self.peers.read().await;
        
        for (_, peer) in peers.iter() {
            let client = peer.client.clone();
            let update = update.clone();
            
            tokio::spawn(async move {
                if let Err(e) = client.cache_update(update).await {
                    warn!("Failed to broadcast cache update: {}", e);
                }
            });
        }
    }
}
```

#### 4. Distributed Cache with Consistent Hashing
```rust
pub struct MeshCache {
    local_cache: Arc<DnsCache>,
    peer_manager: Arc<PeerManager>,
    hash_ring: Arc<RwLock<HashRing<String>>>,
}

impl MeshCache {
    pub async fn get(&self, key: &str) -> Option<DNSPacket> {
        let owner = self.get_key_owner(key).await;
        
        if owner == self.peer_manager.local_id() {
            // Local lookup
            self.local_cache.get(key).await
        } else {
            // Remote lookup
            self.fetch_from_peer(&owner, key).await
        }
    }
    
    pub async fn set(&self, key: &str, packet: DNSPacket, ttl: Duration) {
        let owner = self.get_key_owner(key).await;
        
        if owner == self.peer_manager.local_id() {
            // Store locally
            self.local_cache.set(key, packet.clone(), ttl).await;
            
            // Replicate to N-1 other nodes for redundancy
            self.replicate_to_peers(key, packet, ttl).await;
        } else {
            // Forward to owner
            self.forward_to_peer(&owner, key, packet, ttl).await;
        }
    }
    
    async fn replicate_to_peers(&self, key: &str, packet: DNSPacket, ttl: Duration) {
        let replicas = self.hash_ring.read().await
            .get_replicas(key, 2); // Get 2 replicas
        
        for replica_id in replicas {
            if replica_id == self.peer_manager.local_id() {
                continue;
            }
            
            let peer_manager = self.peer_manager.clone();
            let key = key.to_string();
            let packet = packet.clone();
            
            tokio::spawn(async move {
                if let Some(peer) = peer_manager.get_peer(&replica_id).await {
                    let _ = peer.client.set_cache_entry(SetCacheRequest {
                        key,
                        packet_data: bincode::serialize(&packet).unwrap(),
                        ttl_seconds: ttl.as_secs() as i64,
                        is_replica: true,
                    }).await;
                }
            });
        }
    }
}
```

### Pros
- **Direct Communication**: Low latency peer-to-peer communication
- **Flexible Protocol**: Can evolve the protocol as needed
- **Strong Typing**: Protobuf provides type safety across services
- **Streaming Support**: Efficient for real-time updates
- **Security**: Built-in TLS/mTLS support

### Cons
- **Complex Mesh Management**: O(n²) connections in full mesh
- **Connection Overhead**: Each replica maintains connections to all others
- **Debugging Complexity**: Distributed tracing needed
- **Protocol Evolution**: Requires careful versioning
- **No Built-in Persistence**: Need separate solution for durability

### Deployment Configuration
```yaml
apiVersion: v1
kind: Service
metadata:
  name: heimdall-grpc
spec:
  clusterIP: None
  ports:
  - port: 8443
    name: grpc
  selector:
    app: heimdall
---
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: heimdall
spec:
  serviceName: heimdall-grpc
  replicas: 3
  template:
    spec:
      containers:
      - name: heimdall
        image: heimdall:latest
        ports:
        - containerPort: 1053
          name: dns
        - containerPort: 8443
          name: grpc
        env:
        - name: POD_NAME
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: GRPC_SERVICE
          value: "heimdall-grpc"
        volumeMounts:
        - name: tls-certs
          mountPath: /etc/tls
          readOnly: true
      volumes:
      - name: tls-certs
        secret:
          secretName: heimdall-tls
```

## Recommendation Matrix

| Criteria | Redis | Gossip | Hazelcast | gRPC Mesh |
|----------|-------|---------|-----------|-----------|
| **Implementation Complexity** | Low | High | Medium | High |
| **Operational Complexity** | Medium | Low | High | Medium |
| **Performance Overhead** | 1-5ms | <1ms | 2-10ms | <1ms |
| **Scalability** | High | High | High | Medium |
| **Consistency Model** | Eventual | Eventual | Strong | Flexible |
| **Resource Usage** | Medium | Low | High | Medium |
| **Failure Handling** | Good | Excellent | Good | Manual |
| **Monitoring/Debug** | Excellent | Poor | Good | Good |
| **Community/Support** | Excellent | Limited | Good | Good |

## Phased Implementation Plan

### Phase 1: Redis Shared Cache (1-2 weeks)
1. Implement Redis cache backend
2. Add configuration for Redis connection
3. Implement L1/L2 cache hierarchy
4. Deploy Redis cluster in Kubernetes
5. Monitor cache hit rates and latency

### Phase 2: Health Metrics Sharing (1 week)
1. Store health metrics in Redis
2. Implement cross-replica health queries
3. Update server selection logic
4. Add health aggregation endpoints

### Phase 3: Distributed Query Deduplication (1 week)
1. Implement Redis-based query locks
2. Add wait-for-result mechanism
3. Update resolver to check cluster-wide locks
4. Monitor deduplication effectiveness

### Phase 4: Evaluate Advanced Options (2-4 weeks)
1. Prototype gossip protocol for comparison
2. Benchmark against Redis solution
3. Consider hybrid approach if beneficial
4. Plan migration strategy if needed

## Conclusion

For Heimdall's immediate needs, **Redis** offers the best balance of:
- Quick implementation timeline
- Proven reliability
- Rich ecosystem
- Operational simplicity

The gossip protocol approach becomes attractive as the cluster grows beyond 10-20 replicas, where Redis coordination overhead might become significant.

The gRPC mesh approach is ideal if you need fine-grained control over the coordination protocol and already have gRPC expertise on the team.

Hazelcast should be considered only if you need its advanced distributed computing features and can accept the JVM overhead.