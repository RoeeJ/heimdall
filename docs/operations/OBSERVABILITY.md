# Heimdall DNS Server Observability Strategy

## Executive Summary

Heimdall DNS Server implements a comprehensive observability strategy with mature metrics collection, health monitoring, and structured logging. The system provides deep visibility into DNS operations, performance characteristics, and distributed system behavior through Prometheus-compatible metrics, multi-level health checks, and structured logging with tracing.

## 1. Current Metrics Collection Strategy

### 1.1 Metrics Architecture

Heimdall uses a centralized metrics registry (`DnsMetrics`) built on the Prometheus client library, providing:

- **Real-time metrics collection** with atomic counters and gauges
- **Hierarchical metric organization** across multiple domains
- **Label-based dimensionality** for detailed analysis
- **Zero-cost abstractions** using Rust's type system

### 1.2 Metric Categories

#### Cache Metrics
- `heimdall_cache_hits_total` - Total cache hits
- `heimdall_cache_misses_total` - Total cache misses  
- `heimdall_cache_evictions_total` - Total evictions (including expired)
- `heimdall_cache_size` - Current cache entry count
- `heimdall_cache_hit_rate` - Cache hit rate percentage (0-100)
- `heimdall_cache_negative_hits_total` - RFC 2308 negative cache hits
- `heimdall_cache_nxdomain_responses_total` - Cached NXDOMAIN responses
- `heimdall_cache_nodata_responses_total` - Cached NODATA responses
- `heimdall_cache_negative_hit_rate` - Negative cache hit rate

#### Query Metrics
- `heimdall_queries_total` - Total queries by protocol, type, and response code
- `heimdall_query_duration_seconds` - Query processing duration histogram
- `heimdall_concurrent_queries` - Current concurrent query count
- `heimdall_malformed_packets_total` - Malformed packets by protocol and error type
- `heimdall_truncated_responses_total` - UDP truncation events
- `heimdall_error_responses_total` - Error responses (REFUSED, NOTIMPL, FORMERR)

#### Upstream Server Metrics
- `heimdall_upstream_requests_total` - Requests per upstream server
- `heimdall_upstream_responses_total` - Responses by server and status
- `heimdall_upstream_response_time_seconds` - Response time histogram
- `heimdall_upstream_health_status` - Health status (1=healthy, 0=unhealthy)
- `heimdall_upstream_consecutive_failures` - Consecutive failure count

#### Rate Limiting Metrics
- `heimdall_rate_limit_drops_total` - Dropped queries by limiter type and IP
- `heimdall_active_rate_limiters` - Active rate limiters by type

#### Connection Pool Metrics
- `heimdall_connection_pool_size` - Active connections per upstream server

#### Runtime Metrics
- `heimdall_worker_threads` - Configured worker thread count
- `heimdall_max_concurrent_queries` - Maximum concurrent query limit

### 1.3 Cluster-Wide Metrics

When running in Kubernetes with Redis coordination:

- `heimdall_cluster_total_queries` - Aggregated queries across all members
- `heimdall_cluster_cache_hits_total` - Cluster-wide cache hits
- `heimdall_cluster_cache_misses_total` - Cluster-wide cache misses
- `heimdall_cluster_cache_hit_rate` - Cluster-wide hit rate
- `heimdall_cluster_cache_size_total` - Total cache entries across cluster
- `heimdall_cluster_errors_total` - Total errors across cluster
- `heimdall_cluster_members_total` - Active cluster member count
- `heimdall_cluster_member_*` - Per-member metrics with hostname/pod labels

## 2. Prometheus Integration

### 2.1 Export Format
- Standard Prometheus text format (OpenMetrics compatible)
- UTF-8 encoded with proper HELP and TYPE annotations
- Histogram buckets for latency measurements
- Counter and gauge semantics properly enforced

### 2.2 Scraping Configuration
```yaml
endpoints:
  - port: http
    path: /metrics
    interval: 30s
    scrapeTimeout: 10s
```

### 2.3 ServiceMonitor Integration
- Kubernetes-native ServiceMonitor CRD support
- Automatic discovery in Prometheus Operator environments
- Configurable scrape intervals and timeouts
- Label-based target selection

## 3. Logging Patterns and Structured Logging

### 3.1 Logging Framework
- **tracing** crate for structured, leveled logging
- **tracing-subscriber** with environment filter support
- Configurable log levels via `RUST_LOG` environment variable
- Default: `heimdall=info,warn`

### 3.2 Log Levels and Usage

#### ERROR Level
- Failed operations that impact functionality
- Upstream server failures
- Cache persistence errors
- Configuration reload failures

#### WARN Level
- Rate limit violations
- Malformed packets (non-parsing errors)
- Failed health checks
- Resource exhaustion (max concurrent queries)

#### INFO Level
- Server startup/shutdown
- Configuration changes
- Health status changes
- Cluster membership updates

#### DEBUG Level
- Cache hit/miss details
- Query deduplication events
- Connection pool operations
- Individual query processing

#### TRACE Level
- Packet parsing details
- Cache save operations
- Detailed protocol interactions

### 3.3 Structured Fields
While using standard tracing macros, the codebase includes contextual information:
- Client IP addresses
- Query types and domains
- Response codes and latencies
- Error details and stack traces

## 4. Health Check Implementations

### 4.1 Basic Health Check (`/health`)
- Simple liveness probe
- Returns 200 OK if server is responsive
- Updates metrics before responding
- Minimal overhead for frequent checks

### 4.2 Detailed Health Check (`/health/detailed`)
Comprehensive health status including:

#### Cache Health
- Current size and capacity
- Hit/miss rates
- Eviction statistics

#### Upstream Server Health
- Per-server health status
- Success rates and failure counts
- Average response times
- Connection pool sizes

#### Rate Limiter Status
- Active limiter counts by type
- Current load indicators

#### Cluster Status (if enabled)
- Member count and health distribution
- Per-member statistics
- Cluster-wide aggregates

### 4.3 Health States
- **healthy** - All upstream servers operational
- **degraded** - Some upstream servers failing
- Returns 503 Service Unavailable when degraded

## 5. Distributed Tracing Capabilities

### 5.1 Current State
- **No OpenTelemetry/Jaeger integration** currently implemented
- Query IDs for request correlation within the system
- Log correlation possible through client IP and timestamp

### 5.2 Tracing Opportunities
- Query flow tracking through cache/resolver/upstream
- Cross-service correlation in Kubernetes environments
- Latency breakdown analysis
- Error propagation visualization

### 5.3 Implementation Considerations
```rust
// Potential tracing integration points:
// 1. Query ingress (server.rs)
// 2. Cache lookups (cache/mod.rs)
// 3. Upstream requests (resolver.rs)
// 4. Response generation
```

## 6. Alerting Considerations

### 6.1 Key Metrics for Alerting

#### Critical Alerts
- All upstream servers unhealthy
- Cache hit rate < 10% (configuration issue)
- Query error rate > 5%
- Worker thread saturation

#### Warning Alerts
- Individual upstream server failures
- High rate limit drops (potential DDoS)
- Cache eviction rate spike
- Cluster member failures

### 6.2 Prometheus Alert Examples
```yaml
groups:
  - name: heimdall
    rules:
      - alert: HeimdallAllUpstreamsFailed
        expr: sum(heimdall_upstream_health_status) == 0
        for: 1m
        annotations:
          summary: "All upstream DNS servers are failing"
          
      - alert: HeimdallHighErrorRate
        expr: rate(heimdall_error_responses_total[5m]) > 0.05
        for: 5m
        annotations:
          summary: "High DNS error response rate"
          
      - alert: HeimdallCacheHitRateLow
        expr: heimdall_cache_hit_rate < 10
        for: 10m
        annotations:
          summary: "DNS cache hit rate critically low"
```

## 7. Monitoring Endpoints

### 7.1 HTTP API Endpoints
- `GET /health` - Basic health check
- `GET /health/detailed` - Comprehensive health status
- `GET /metrics` - Prometheus metrics
- `GET /stats` - JSON server statistics
- `GET /cache/stats` - Cache-specific statistics
- `GET /upstream/stats` - Upstream server details
- `GET /cluster/stats` - Cluster coordination statistics
- `POST /config/reload` - Trigger configuration reload

### 7.2 Response Formats
- Health checks: JSON with status fields
- Metrics: Prometheus text format
- Statistics: Structured JSON with nested objects

## 8. Performance Monitoring

### 8.1 Latency Tracking
- Query duration histograms with protocol and cache hit labels
- Upstream response time histograms per server
- P50, P90, P99 percentiles available via Prometheus

### 8.2 Throughput Monitoring
- Queries per second (calculated from counters)
- Cache operations per second
- Rate limit enforcement metrics

### 8.3 Resource Utilization
- Concurrent query gauge
- Cache size and eviction rates
- Connection pool utilization

## 9. Best Practices and Recommendations

### 9.1 Current Strengths
- Comprehensive metric coverage
- Production-ready Prometheus integration
- Detailed health checking
- Cluster-aware monitoring
- Performance-focused design

### 9.2 Recommended Improvements

#### Short Term
1. **Add OpenTelemetry support** for distributed tracing
2. **Implement structured logging** with JSON output option
3. **Add metric cardinality limits** to prevent explosion
4. **Create Grafana dashboard templates**

#### Medium Term
1. **Add trace sampling** for high-volume environments
2. **Implement metric aggregation** for edge locations
3. **Add SLI/SLO tracking** metrics
4. **Create runbooks** linked to alerts

#### Long Term
1. **Machine learning** for anomaly detection
2. **Predictive scaling** based on metrics
3. **Automated remediation** workflows
4. **Cost attribution** metrics

### 9.3 Monitoring Stack Integration
```yaml
# Example monitoring stack
components:
  - prometheus:     # Metrics collection
  - grafana:       # Visualization
  - alertmanager:  # Alert routing
  - loki:          # Log aggregation (future)
  - tempo:         # Distributed tracing (future)
```

## 10. Operational Dashboards

### 10.1 Key Dashboard Panels

#### Overview Dashboard
- Query rate and latency trends
- Cache hit rate and size
- Error rate by type
- Upstream server health matrix

#### Performance Dashboard
- Query latency percentiles
- Cache performance metrics
- Connection pool efficiency
- Rate limiter activity

#### Cluster Dashboard
- Member health status
- Query distribution
- Cache synchronization metrics
- Cluster-wide aggregates

### 10.2 Example Grafana Queries
```promql
# Query rate
rate(heimdall_queries_total[5m])

# Cache hit rate
heimdall_cache_hit_rate

# P99 query latency
histogram_quantile(0.99, rate(heimdall_query_duration_seconds_bucket[5m]))

# Upstream server availability
avg_over_time(heimdall_upstream_health_status[5m])
```

## Conclusion

Heimdall's observability implementation provides production-grade monitoring capabilities with room for enhancement in distributed tracing and advanced analytics. The current implementation offers excellent visibility into DNS operations, making it suitable for production deployments while maintaining paths for future observability evolution.