# Observability Strategy - Heimdall DNS Server

## Executive Summary
Heimdall implements a comprehensive observability stack built on Prometheus metrics, structured logging, and multi-tier health checking. The system provides production-ready monitoring capabilities for both standalone and distributed deployments.

## Current Implementation

### 1. Metrics Collection (Production Ready)

#### Core Metrics Categories
```
DNS Operations (13 metrics):
- dns_queries_total (by protocol, record_type)
- dns_response_time_seconds (histogram)
- dns_errors_total (by error_type)
- dns_upstream_queries_total (by server)
- dns_upstream_response_time_seconds (by server)

Cache Performance (12 metrics):
- cache_hits_total, cache_misses_total
- cache_hit_ratio (computed)
- cache_size_entries, cache_size_bytes
- cache_evictions_total
- cache_ttl_adjustments_total

Server Health (8 metrics):
- upstream_server_health_score (by server)
- upstream_server_requests_total (by server)
- upstream_server_failures_total (by server)
- connection_pool_active, connection_pool_idle

Rate Limiting (6 metrics):
- rate_limit_applied_total (by client_ip)
- rate_limit_exceeded_total
- global_rate_limit_active

Runtime Metrics (20 metrics):
- worker_threads_active
- memory_usage_bytes
- cpu_usage_percent
- process_start_time_seconds
```

#### Cluster Aggregation (Redis-based)
```rust
// Cluster-wide metrics endpoint: /cluster/stats
{
  "total_queries": 45231,
  "total_cache_hits": 38447,
  "total_cache_misses": 6784,
  "cluster_cache_hit_rate": 0.85,
  "members": [
    {
      "hostname": "heimdall-0",
      "queries_per_second": 125.3,
      "cache_hit_rate": 0.83,
      "status": "healthy"
    }
  ]
}
```

### 2. Prometheus Integration

#### Native Client Library
```rust
use prometheus::{
    Counter, Gauge, Histogram, Registry,
    Opts, HistogramOpts
};

pub struct DNSMetrics {
    pub queries_total: Counter,
    pub response_time: Histogram,
    pub cache_hits: Counter,
    // ... 59 total metrics
}
```

#### Kubernetes ServiceMonitor
```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: heimdall-metrics
spec:
  selector:
    matchLabels:
      app: heimdall
  endpoints:
  - port: http
    path: /metrics
    interval: 30s
```

#### OpenMetrics Format
```
# HELP dns_queries_total Total number of DNS queries processed
# TYPE dns_queries_total counter
dns_queries_total{protocol="udp",record_type="A"} 12345
dns_queries_total{protocol="tcp",record_type="AAAA"} 6789

# HELP dns_response_time_seconds DNS query response time
# TYPE dns_response_time_seconds histogram
dns_response_time_seconds_bucket{le="0.001"} 8934
dns_response_time_seconds_bucket{le="0.010"} 11234
dns_response_time_seconds_bucket{le="+Inf"} 12345
```

### 3. Structured Logging

#### Tracing Framework
```rust
use tracing::{info, warn, error, debug, trace, instrument};

#[instrument(skip(packet), fields(client_ip = %client_addr))]
async fn handle_dns_query(packet: DNSPacket, client_addr: SocketAddr) {
    info!(
        query_id = packet.header().id,
        question = packet.questions().first().map(|q| q.name.as_str()),
        record_type = ?packet.questions().first().map(|q| q.record_type),
        "Processing DNS query"
    );
}
```

#### Log Levels and Usage
- **ERROR**: Service failures, upstream connection issues
- **WARN**: Individual query failures, rate limiting events
- **INFO**: Query processing, cache events, server lifecycle
- **DEBUG**: Detailed packet parsing, upstream responses
- **TRACE**: Buffer operations, compression pointer resolution

#### Environment Configuration
```bash
# Production
RUST_LOG=heimdall=info

# Development
RUST_LOG=heimdall=debug,dns=trace

# Troubleshooting
RUST_LOG=trace
```

### 4. Health Check System

#### Basic Health Endpoint (`/health`)
```json
{
  "status": "healthy",
  "timestamp": "2025-01-06T10:30:45Z",
  "version": "0.1.0"
}
```

#### Detailed Health Endpoint (`/health/detailed`)
```json
{
  "status": "healthy",
  "timestamp": "2025-01-06T10:30:45Z",
  "components": {
    "cache": {
      "status": "healthy",
      "hit_rate": 0.85,
      "size_entries": 8432,
      "size_bytes": 2097152
    },
    "upstream_servers": {
      "8.8.8.8": {
        "status": "healthy",
        "health_score": 1.0,
        "response_time_ms": 12.5
      },
      "1.1.1.1": {
        "status": "degraded",
        "health_score": 0.7,
        "response_time_ms": 45.2
      }
    },
    "rate_limiter": {
      "status": "healthy",
      "active_limits": 23
    },
    "cluster": {
      "redis_connected": true,
      "member_count": 3,
      "status": "healthy"
    }
  }
}
```

#### Kubernetes Integration
```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 30
  periodSeconds: 10

readinessProbe:
  httpGet:
    path: /health/detailed
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 5
```

## Gaps and Improvement Opportunities

### 1. Distributed Tracing (Not Implemented)

#### Recommended Implementation
```rust
use opentelemetry::{
    trace::{SpanKind, TraceContextExt, Tracer},
    Context,
};
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[instrument]
async fn resolve_query(query: &str) -> Result<Response> {
    let span = tracing::Span::current();
    span.set_attribute("dns.query", query);
    
    // Upstream request with trace propagation
    let upstream_span = span.tracer().start("upstream_request");
    let _guard = upstream_span.enter();
    
    // ... query processing
}
```

#### Benefits:
- End-to-end request tracking across cluster
- Performance bottleneck identification
- Distributed system debugging
- Correlation with external services

### 2. Advanced Logging Patterns

#### Structured JSON Logging (Optional)
```rust
// Current: human-readable
[2025-01-06T10:30:45Z INFO heimdall] Processing DNS query for google.com

// Proposed: JSON structured
{
  "timestamp": "2025-01-06T10:30:45Z",
  "level": "INFO",
  "target": "heimdall",
  "message": "Processing DNS query",
  "fields": {
    "query": "google.com",
    "record_type": "A",
    "client_ip": "192.168.1.100",
    "query_id": 12345,
    "trace_id": "abc123def456"
  }
}
```

#### Correlation IDs
```rust
use uuid::Uuid;

struct QueryContext {
    correlation_id: Uuid,
    client_ip: IpAddr,
    start_time: Instant,
}

impl QueryContext {
    fn new(client_ip: IpAddr) -> Self {
        Self {
            correlation_id: Uuid::new_v4(),
            client_ip,
            start_time: Instant::now(),
        }
    }
}
```

### 3. SLI/SLO Metrics

#### Proposed Service Level Indicators
```rust
pub struct SLIMetrics {
    // Availability: % of successful queries
    successful_queries: Counter,
    failed_queries: Counter,
    
    // Latency: % of queries under threshold
    queries_under_100ms: Counter,
    queries_under_1s: Counter,
    total_queries: Counter,
    
    // Quality: % of correct responses
    correct_responses: Counter,
    cache_inconsistencies: Counter,
}
```

#### Service Level Objectives
- **Availability**: 99.9% of queries succeed
- **Latency**: 95% of queries complete under 100ms
- **Quality**: 99.99% of responses are correct

## Alerting Strategy

### Critical Alerts (Page Immediately)
```yaml
# All upstream servers failing
- alert: DNSAllUpstreamsDown
  expr: sum(up{job="heimdall-upstream"}) == 0
  for: 1m
  severity: critical

# Cache completely unavailable
- alert: DNSCacheDown
  expr: increase(cache_errors_total[5m]) > 100
  for: 2m
  severity: critical

# Error rate above threshold
- alert: DNSHighErrorRate
  expr: rate(dns_errors_total[5m]) / rate(dns_queries_total[5m]) > 0.05
  for: 5m
  severity: critical
```

### Warning Alerts (Notify)
```yaml
# Individual upstream degraded
- alert: DNSUpstreamDegraded
  expr: upstream_server_health_score < 0.8
  for: 5m
  severity: warning

# Cache hit rate low
- alert: DNSLowCacheHitRate
  expr: cache_hit_ratio < 0.7
  for: 10m
  severity: warning

# High response time
- alert: DNSHighLatency
  expr: histogram_quantile(0.95, dns_response_time_seconds) > 0.1
  for: 5m
  severity: warning
```

## Monitoring Dashboards

### Recommended Grafana Dashboard Panels

#### Overview Dashboard
1. **Query Rate**: Queries per second over time
2. **Response Time**: P50, P90, P95, P99 latencies
3. **Error Rate**: Percentage of failed queries
4. **Cache Performance**: Hit rate, size, evictions
5. **Upstream Health**: Server status and response times

#### Detailed Dashboard
1. **Protocol Breakdown**: UDP vs TCP query distribution
2. **Record Type Analysis**: A, AAAA, MX, etc. query patterns
3. **Client Geography**: Top client IPs and patterns
4. **Cluster Health**: Member status and load distribution
5. **Resource Utilization**: Memory, CPU, network usage

#### Troubleshooting Dashboard
1. **Error Analysis**: Error types and frequencies
2. **Cache Debugging**: Miss reasons, TTL distribution
3. **Network Analysis**: Packet sizes, connection stats
4. **Rate Limiting**: Applied limits and patterns

## Implementation Roadmap

### Phase 1: Enhanced Logging (2 weeks)
- [ ] Add correlation IDs to all log messages
- [ ] Implement optional JSON logging format
- [ ] Add log sampling for high-volume environments
- [ ] Create log aggregation guides

### Phase 2: Distributed Tracing (3 weeks)
- [ ] Integrate OpenTelemetry SDK
- [ ] Add trace spans for major operations
- [ ] Implement trace sampling strategy
- [ ] Create trace visualization guides

### Phase 3: Advanced Metrics (2 weeks)
- [ ] Add SLI/SLO tracking metrics
- [ ] Implement custom business metrics
- [ ] Add performance percentile tracking
- [ ] Create alerting rule templates

### Phase 4: Visualization (1 week)
- [ ] Create Grafana dashboard templates
- [ ] Add dashboard automation scripts
- [ ] Implement dashboard versioning
- [ ] Create monitoring runbooks

## Security Considerations

### Metrics Security
- No sensitive data in metric labels
- Client IP obfuscation options
- Query content sanitization
- Rate limit monitoring data

### Log Security
- PII redaction for client information
- Query content filtering options
- Log retention policies
- Access control for log data

## Performance Impact

### Current Overhead
- **Metrics collection**: <1% CPU overhead
- **Structured logging**: 2-3% CPU overhead
- **Health checks**: Minimal impact (<0.1%)

### Monitoring Best Practices
- **Metric cardinality limits**: Avoid high-cardinality labels
- **Log sampling**: Reduce volume in high-traffic scenarios
- **Async collection**: Non-blocking metrics updates
- **Resource limits**: Monitor monitoring system resources

## References
- Prometheus client library: https://docs.rs/prometheus
- Tracing crate: https://docs.rs/tracing
- OpenTelemetry: https://opentelemetry.io/docs/rust/
- Grafana dashboards: https://grafana.com/docs/
- Internal implementation: `/src/metrics.rs`, `/src/http_server.rs`