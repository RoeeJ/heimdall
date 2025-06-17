# Redis Integration Guide

## Overview

Heimdall supports Redis as an optional L2 (second-level) cache to enable cache sharing across multiple replicas. This improves overall cache hit rates and provides a consistent view of cached DNS responses across your deployment.

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Heimdall   │     │  Heimdall   │     │  Heimdall   │
│  Replica 1  │     │  Replica 2  │     │  Replica 3  │
│ ┌─────────┐ │     │ ┌─────────┐ │     │ ┌─────────┐ │
│ │L1 Cache │ │     │ │L1 Cache │ │     │ │L1 Cache │ │
│ └────┬────┘ │     │ └────┬────┘ │     │ └────┬────┘ │
└──────┼──────┘     └──────┼──────┘     └──────┼──────┘
       │                   │                   │
       └───────────────────┴───────────────────┘
                           │
                    ┌──────▼──────┐
                    │    Redis    │
                    │  (L2 Cache) │
                    └─────────────┘
```

## Auto-Detection

Heimdall automatically detects and configures Redis in the following order:

1. **Explicit Configuration**: `HEIMDALL_REDIS_URL` environment variable
2. **Standard Redis URL**: `REDIS_URL` environment variable  
3. **Kubernetes Service**: Auto-detects `HEIMDALL_REDIS_SERVICE_HOST` and `HEIMDALL_REDIS_SERVICE_PORT`
4. **Default Kubernetes**: Tries `redis://heimdall-redis:6379` if running in Kubernetes
5. **Disabled**: If no Redis is detected, runs with local cache only

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `HEIMDALL_REDIS_URL` | Full Redis connection URL | Auto-detected |
| `HEIMDALL_REDIS_ENABLED` | Enable/disable Redis integration | `true` if Redis detected |
| `HEIMDALL_REDIS_KEY_PREFIX` | Prefix for all Redis keys | `heimdall:dns:cache` |

### Local Development

For local development, Redis is disabled by default. To enable:

```bash
# Start Redis locally
docker run -d -p 6379:6379 redis:7-alpine

# Set Redis URL
export HEIMDALL_REDIS_URL="redis://localhost:6379"

# Run Heimdall
cargo run
```

### Kubernetes Deployment

#### Using Helm Chart

Redis is enabled by default in the Helm chart:

```yaml
# values.yaml
redis:
  enabled: true
  persistence:
    enabled: true
    size: 10Gi
  auth:
    enabled: false  # Enable for production
```

To deploy:
```bash
helm install heimdall ./helm/heimdall
```

#### Manual Kubernetes Deployment

Apply the Redis deployment first:
```bash
kubectl apply -f k8s/redis-deployment.yaml
```

Then deploy Heimdall - it will auto-detect Redis.

### Docker Compose

```yaml
version: '3.8'
services:
  heimdall:
    image: heimdall:latest
    environment:
      - HEIMDALL_REDIS_URL=redis://redis:6379
    depends_on:
      - redis
    ports:
      - "53:1053/udp"
      - "53:1053/tcp"
  
  redis:
    image: redis:7-alpine
    volumes:
      - redis-data:/data
    command: redis-server --appendonly yes

volumes:
  redis-data:
```

## Cache Behavior

### Two-Tier Caching

1. **L1 Cache (Local)**:
   - In-memory cache on each replica
   - Sub-millisecond access time
   - Limited by pod memory
   - Lost on pod restart

2. **L2 Cache (Redis)**:
   - Shared across all replicas
   - ~1-5ms access time
   - Survives pod restarts
   - Size limited by Redis memory

### Cache Flow

1. **Cache Hit Flow**:
   ```
   Query → Check L1 → Found? Return
                  ↓ Not Found
             Check L2 (Redis) → Found? Store in L1 & Return
                              ↓ Not Found
                         Query Upstream DNS
   ```

2. **Cache Store Flow**:
   ```
   Response → Store in L1
           → Store in L2 (Redis) with TTL
   ```

### TTL Management

- DNS TTLs are respected and stored with each cache entry
- Redis automatically expires entries using native TTL support
- L1 cache performs periodic cleanup of expired entries

## Performance Impact

### Benefits
- **Higher Cache Hit Rate**: Shared cache across replicas
- **Reduced Upstream Queries**: Better deduplication
- **Consistent Responses**: All replicas serve same cached data
- **Persistent Cache**: Survives pod restarts with Redis persistence

### Overhead
- **Additional Latency**: ~1-5ms for Redis operations
- **Network Traffic**: Redis protocol overhead
- **Memory Usage**: Duplicate data in L1 and L2

### Benchmarks

| Scenario | Without Redis | With Redis |
|----------|--------------|------------|
| Cache Hit (L1) | <0.1ms | <0.1ms |
| Cache Hit (L2) | N/A | ~2ms |
| Cache Miss | ~50ms | ~52ms |
| Overall Hit Rate* | 60% | 85% |

*With 3 replicas under typical workload

## Monitoring

### Redis Metrics

Monitor Redis health and performance:

```bash
# Redis CLI
redis-cli INFO stats
redis-cli INFO memory

# Prometheus metrics (if redis-exporter deployed)
curl http://redis-exporter:9121/metrics
```

### Heimdall Metrics

Heimdall exposes cache metrics at `/metrics`:

- `heimdall_cache_hits_total` - L1 cache hits
- `heimdall_cache_misses_total` - Cache misses
- `heimdall_cache_size` - Current cache size

## Troubleshooting

### Redis Connection Issues

Check Redis connectivity:
```bash
# From Heimdall pod
redis-cli -h heimdall-redis ping

# Check logs
kubectl logs -l app=heimdall | grep -i redis
```

### Cache Inconsistencies

If experiencing cache inconsistencies:
1. Check Redis memory usage: `redis-cli INFO memory`
2. Verify TTLs are being set: `redis-cli TTL "heimdall:dns:cache:*"`
3. Clear cache if needed: `redis-cli FLUSHDB`

### Disable Redis

To disable Redis and use local-only cache:
```bash
export HEIMDALL_REDIS_ENABLED=false
```

Or in Helm:
```yaml
redis:
  enabled: false
```

## Security Considerations

### Authentication

For production, enable Redis authentication:

```yaml
# Helm values
redis:
  auth:
    enabled: true
    password: "generate-secure-password"
```

### Network Policies

Restrict Redis access to Heimdall pods only:

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: redis-access
spec:
  podSelector:
    matchLabels:
      app: redis
  ingress:
  - from:
    - podSelector:
        matchLabels:
          app: heimdall
    ports:
    - port: 6379
```

### Encryption

For sensitive environments, consider:
- TLS encryption for Redis connections (Redis 6+)
- Encrypting Redis persistence files
- Using encrypted storage classes for PVCs

## Best Practices

1. **Size Redis Appropriately**: Plan for ~2x your expected cache size
2. **Monitor Memory Usage**: Set up alerts for Redis memory usage
3. **Use Persistence**: Enable Redis persistence for production
4. **Set Resource Limits**: Prevent Redis from consuming all node memory
5. **Regular Backups**: Backup Redis data for disaster recovery
6. **Connection Pooling**: Heimdall uses connection pooling by default
7. **Key Expiration**: Let Redis handle TTL expiration (don't manually delete)

## Future Enhancements

Planned improvements for Redis integration:

1. **Pub/Sub for Cache Invalidation**: Real-time cache updates
2. **Redis Cluster Support**: For larger deployments
3. **Cache Warming**: Pre-populate cache from Redis on startup
4. **Metrics Export**: Detailed Redis operation metrics
5. **Compression**: Compress large DNS responses before storing