# Heimdall DNS Performance Tuning Guide

This guide covers performance tuning options, benchmarking, and optimization techniques for Heimdall DNS server.

## Table of Contents

1. [Runtime Configuration](#runtime-configuration)
2. [Performance Monitoring](#performance-monitoring)
3. [Benchmarking & Regression Testing](#benchmarking--regression-testing)
4. [Optimization Techniques](#optimization-techniques)
5. [Hardware Recommendations](#hardware-recommendations)
6. [Troubleshooting Performance Issues](#troubleshooting-performance-issues)

## Runtime Configuration

### Worker Thread Pool Settings

Heimdall uses a configurable Tokio runtime with custom thread pool settings:

```bash
# Set number of worker threads (default: number of CPU cores)
export HEIMDALL_WORKER_THREADS=4

# Set number of blocking threads (default: 512)
export HEIMDALL_BLOCKING_THREADS=256

# Set maximum concurrent queries (default: 10000)
export HEIMDALL_MAX_CONCURRENT_QUERIES=5000
```

**Recommendations:**
- **Worker Threads**: Set to number of CPU cores for CPU-bound workloads, or 2x cores for I/O-heavy workloads
- **Blocking Threads**: Increase if you see "blocking pool exhausted" warnings
- **Max Concurrent Queries**: Tune based on available memory and upstream server capacity

### DNS Cache Configuration

```bash
# Enable/disable caching (default: true)
export HEIMDALL_ENABLE_CACHING=true

# Maximum cache size (default: 10000 entries)
export HEIMDALL_MAX_CACHE_SIZE=50000

# Default TTL for cached responses (default: 300 seconds)
export HEIMDALL_DEFAULT_TTL=600
```

**Cache Performance:**
- **Cache Hit Rate**: Aim for >90% for optimal performance
- **Memory Usage**: ~100MB for 1 million entries
- **Lookup Time**: Sub-microsecond for cache hits

### Network Configuration

```bash
# Upstream DNS servers (comma-separated)
export HEIMDALL_UPSTREAM_SERVERS="1.1.1.1:53,8.8.8.8:53,8.8.4.4:53"

# Query timeout (default: 5 seconds)
export HEIMDALL_UPSTREAM_TIMEOUT=3

# Enable parallel upstream queries (default: true)
export HEIMDALL_ENABLE_PARALLEL_QUERIES=true

# Maximum retry attempts (default: 2)
export HEIMDALL_MAX_RETRIES=3
```

## Performance Monitoring

### Built-in Metrics

Heimdall provides comprehensive performance metrics:

```bash
# Enable debug logging to see performance metrics
export RUST_LOG=heimdall=debug

# Example output:
# Cache hit rate: 92.5% (1850/2000 queries)
# Average response time: 1.2ms (cached: 0.8ms, upstream: 45ms)
# Queries per second: 1,250
```

### Key Performance Indicators (KPIs)

| Metric | Good | Acceptable | Poor |
|--------|------|------------|------|
| Cache Hit Rate | >90% | 80-90% | <80% |
| Response Time (Cached) | <1ms | 1-5ms | >5ms |
| Response Time (Upstream) | <50ms | 50-200ms | >200ms |
| Queries per Second | >1000 | 500-1000 | <500 |
| Memory Usage | <100MB | 100-500MB | >500MB |

### System Monitoring

Monitor these system-level metrics:

```bash
# CPU usage (should be <80% under normal load)
htop

# Memory usage
free -h

# Network connections
ss -tuln | grep 1053

# DNS query statistics
./target/release/stress_test --clients 10 --queries 1000
```

## Benchmarking & Regression Testing

### Running Performance Benchmarks

```bash
# Run comprehensive benchmarks
cargo bench

# Run specific benchmark
cargo bench dns_parsing

# Compare with baseline performance
./scripts/check_performance.sh

# Create new performance baseline
./scripts/check_performance.sh --create-baseline
```

### Regression Testing

Heimdall includes an automated regression testing suite:

```bash
# Quick regression check (100 iterations)
./scripts/check_performance.sh --iterations 100

# Production-grade regression test (10,000 iterations)
./scripts/check_performance.sh --iterations 10000

# Strict regression threshold (5% maximum regression)
./scripts/check_performance.sh --max-regression 5.0
```

**Benchmark Results (as of v0.1.0):**
- Zero-copy parsing: **6.8x faster** than regular parsing
- Zero-copy serialization: **1.47x faster** than regular serialization
- Cache operations: **257ns per lookup** with pre-computed hashing
- SIMD pattern matching: **10-95ns per operation**

### CI/CD Integration

Add to your CI pipeline:

```yaml
# .github/workflows/performance.yml
- name: Performance Regression Test
  run: |
    ./scripts/check_performance.sh --iterations 1000 --max-regression 10.0
```

## Optimization Techniques

### DNS Query Optimization

1. **Query Deduplication**: Automatically enabled - prevents duplicate upstream requests
2. **Connection Pooling**: Reuses UDP sockets to reduce connection overhead
3. **Parallel Queries**: Races multiple upstream servers for fastest response
4. **Zero-Copy Processing**: Minimizes memory allocations with buffer pooling

### Cache Optimization

```rust
// Pre-computed hash keys for faster lookups
let key = CacheKey::new(domain, record_type, record_class);

// Domain trie for efficient prefix matching
cache.find_related_entries("*.example.com");

// LRU eviction with configurable size limits
let cache = DnsCache::new(max_size, negative_ttl);
```

### Network Optimization

1. **UDP + TCP Support**: Automatic fallback for large responses
2. **EDNS0 Support**: Larger UDP packet sizes (up to 4096 bytes)
3. **DNS Compression**: Reduces packet size by 20-30%
4. **Concurrent Processing**: Handles multiple queries simultaneously

### Memory Optimization

```rust
// Buffer pooling reduces allocation overhead
let pool = PacketBufferPool::new(4096, 32);

// Zero-copy packet references
let packet_ref = DNSPacketRef::parse_metadata(&buffer);

// Pre-allocated vector pools
let mut response_buffer = Vec::with_capacity(512);
```

## Hardware Recommendations

### Minimum Requirements

- **CPU**: 2 cores, 2.0GHz
- **RAM**: 512MB
- **Network**: 100Mbps
- **Storage**: 100MB

**Performance**: ~500 queries/second

### Recommended Configuration

- **CPU**: 4 cores, 3.0GHz
- **RAM**: 2GB
- **Network**: 1Gbps
- **Storage**: 1GB SSD

**Performance**: ~2,000 queries/second

### High-Performance Configuration

- **CPU**: 8+ cores, 3.5GHz+
- **RAM**: 8GB+
- **Network**: 10Gbps
- **Storage**: NVMe SSD

**Performance**: ~10,000+ queries/second

### Environment-Specific Tuning

#### Docker Container

```dockerfile
FROM rust:1.85-slim
# Allocate sufficient memory for cache
ENV HEIMDALL_MAX_CACHE_SIZE=50000
ENV HEIMDALL_WORKER_THREADS=0  # Use container CPU allocation
COPY . .
RUN cargo build --release
EXPOSE 1053/udp 1053/tcp
CMD ["./target/release/heimdall"]
```

```bash
# Run with resource limits
docker run -m 1g --cpus="2.0" -p 1053:1053/udp heimdall
```

#### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: heimdall-dns
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: heimdall
        image: heimdall:latest
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2000m"
        env:
        - name: HEIMDALL_MAX_CACHE_SIZE
          value: "25000"
        - name: HEIMDALL_WORKER_THREADS
          value: "2"
```

## Troubleshooting Performance Issues

### Common Performance Problems

#### High Latency (>100ms responses)

**Symptoms:**
- Slow DNS resolution
- User complaints about slow browsing

**Diagnosis:**
```bash
# Check upstream server response times
dig @8.8.8.8 google.com +time=5 +tries=1

# Monitor Heimdall logs
RUST_LOG=heimdall=debug cargo run
```

**Solutions:**
1. Add faster upstream servers
2. Enable parallel queries
3. Increase cache size and TTL
4. Check network connectivity

#### Low Cache Hit Rate (<80%)

**Symptoms:**
- High upstream query volume
- Inconsistent response times

**Diagnosis:**
```bash
# Check cache statistics in logs
grep "Cache hit rate" /var/log/heimdall.log
```

**Solutions:**
1. Increase cache size: `HEIMDALL_MAX_CACHE_SIZE=50000`
2. Increase default TTL: `HEIMDALL_DEFAULT_TTL=600`
3. Analyze query patterns for optimization

#### Memory Usage Issues

**Symptoms:**
- Out of memory errors
- System swap usage

**Diagnosis:**
```bash
# Monitor memory usage
ps aux | grep heimdall
cat /proc/$(pgrep heimdall)/status | grep VmRSS
```

**Solutions:**
1. Reduce cache size: `HEIMDALL_MAX_CACHE_SIZE=10000`
2. Limit concurrent queries: `HEIMDALL_MAX_CONCURRENT_QUERIES=1000`
3. Adjust worker threads: `HEIMDALL_WORKER_THREADS=2`

#### CPU Bottlenecks

**Symptoms:**
- High CPU usage (>90%)
- Slow query processing

**Diagnosis:**
```bash
# Profile CPU usage
perf record -g ./target/release/heimdall
perf report
```

**Solutions:**
1. Increase worker threads: `HEIMDALL_WORKER_THREADS=8`
2. Enable zero-copy optimizations (already enabled)
3. Use release build: `cargo build --release`

### Performance Testing

#### Load Testing with Custom Domains

```bash
# Test with realistic query patterns
./target/release/stress_test \
  --clients 50 \
  --queries 10000 \
  --query-types "A,AAAA,MX,TXT,NS" \
  --scenario "heavy"
```

#### Capacity Planning

```bash
# Find maximum sustainable load
for clients in 10 20 50 100 200; do
  echo "Testing with $clients clients..."
  ./target/release/stress_test --clients $clients --queries 1000
  sleep 5
done
```

#### Benchmark Specific Features

```bash
# Test zero-copy performance
cargo test benchmark_parsing_comparison --release -- --nocapture

# Test cache performance
cargo test benchmark_cache_operations --release -- --nocapture

# Test SIMD optimizations
cargo test benchmark_simd_operations --release -- --nocapture
```

### Monitoring Integration

#### Prometheus Metrics (Future Feature)

```bash
# Metrics endpoint (planned for Phase 3)
curl http://localhost:9090/metrics

# Key metrics to monitor:
# - heimdall_queries_total
# - heimdall_cache_hit_rate
# - heimdall_response_time_seconds
# - heimdall_upstream_errors_total
```

#### Log Analysis

```bash
# Parse performance metrics from logs
grep "Cache hit rate" /var/log/heimdall.log | tail -10
grep "Average response time" /var/log/heimdall.log | tail -10

# Count queries per minute
grep "Query:" /var/log/heimdall.log | \
  awk '{print $1 " " $2}' | \
  cut -c1-16 | \
  uniq -c
```

## Performance Optimization Checklist

### Before Deployment

- [ ] Run performance regression tests
- [ ] Validate cache hit rate >80% with realistic data
- [ ] Test with expected query load
- [ ] Configure appropriate worker thread count
- [ ] Set up monitoring and alerting

### Production Monitoring

- [ ] Monitor cache hit rate (target: >90%)
- [ ] Track average response time (target: <50ms)
- [ ] Watch memory usage (target: <80% of available)
- [ ] Monitor CPU usage (target: <80%)
- [ ] Set up automated performance regression testing

### Regular Optimization

- [ ] Review and update upstream server list
- [ ] Analyze query patterns for cache optimization
- [ ] Run monthly performance regression tests
- [ ] Update performance baselines after optimizations
- [ ] Document any configuration changes

## Conclusion

Heimdall DNS is designed for high performance with extensive tuning options. The key to optimal performance is:

1. **Proper Configuration**: Set appropriate worker threads and cache size
2. **Regular Monitoring**: Track KPIs and system metrics
3. **Regression Testing**: Ensure performance doesn't degrade over time
4. **Incremental Optimization**: Make small, measured improvements

For additional performance questions or optimization advice, refer to the project's documentation or open an issue on GitHub.