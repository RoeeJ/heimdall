# Heimdall DNS Performance Testing Guide

This guide describes how to use the performance testing scripts to measure and identify bottlenecks in the Heimdall DNS server.

## Overview

The performance testing suite includes:

1. **performance_bottleneck_test.sh** - Comprehensive performance testing with multiple scenarios
2. **profile_memory_locks.sh** - Memory allocation and lock contention profiling
3. **heimdall_perf_test** - Performance test binary for generating DNS query loads

## Quick Start

### Basic Performance Test

```bash
# Run all performance tests with default settings
./scripts/performance_bottleneck_test.sh

# Results will be saved to ./performance_results/<timestamp>/
```

### Memory and Lock Profiling

```bash
# Profile both memory and locks
./scripts/profile_memory_locks.sh --type all

# Profile only memory allocations
./scripts/profile_memory_locks.sh --type memory

# Profile only lock contention
./scripts/profile_memory_locks.sh --type locks
```

## Performance Test Scenarios

The performance test script runs five different scenarios:

### 1. Cache Hit Performance
- Tests performance when most queries hit the cache
- Pre-warms cache with common domains
- Measures sub-millisecond response times
- Identifies cache lookup bottlenecks

### 2. Cache Miss Performance
- Tests performance with unique queries that miss the cache
- Measures upstream query performance
- Identifies network I/O bottlenecks

### 3. Mixed Workload
- Realistic 80/20 cache hit/miss ratio
- Tests typical production scenarios
- Measures overall system performance

### 4. High Concurrency
- Tests with 200+ concurrent clients
- Identifies lock contention issues
- Measures scalability limits

### 5. Blocking Domain Performance
- Tests blocklist lookup performance
- Measures impact of large blocklists
- Identifies blocking algorithm bottlenecks

## Key Metrics Collected

### Throughput Metrics
- Queries per second (QPS)
- Response rate
- Success rate
- Error rate

### Latency Metrics
- Minimum latency
- Average latency
- Maximum latency
- Latency percentiles (P50, P95, P99)
- Latency distribution histogram

### Resource Metrics
- CPU usage
- Memory usage (RSS, VSZ)
- Memory allocation patterns
- Lock contention statistics

## Running Advanced Tests

### With CPU Profiling

```bash
# Generate flame graphs for CPU usage
./scripts/performance_bottleneck_test.sh --profile

# Output: performance_results/<timestamp>/heimdall_flame.svg
```

### With Memory Profiling

```bash
# Track memory allocations
./scripts/performance_bottleneck_test.sh --memory-profile

# Uses jemalloc profiling on Linux
# Falls back to process monitoring on macOS
```

### With Lock Profiling

```bash
# Analyze lock contention (Linux only)
./scripts/performance_bottleneck_test.sh --lock-profile

# Uses perf lock for detailed analysis
```

### All Profiling Enabled

```bash
# Enable all profiling options
./scripts/performance_bottleneck_test.sh --all-profiles
```

## Custom Performance Tests

You can run specific test scenarios with custom parameters:

```bash
# Test with specific client count and duration
./target/release/heimdall_perf_test \
    --test-type mixed \
    --duration 60 \
    --clients 100 \
    --server 127.0.0.1:1053

# Test cache performance with high concurrency
./target/release/heimdall_perf_test \
    --test-type cache-hit \
    --duration 30 \
    --clients 500 \
    --timeout 100
```

## Interpreting Results

### Performance Report

After running tests, view the generated report:

```bash
cat performance_results/<timestamp>/performance_report.md
```

The report includes:
- Test results for each scenario
- Bottleneck analysis
- Optimization recommendations

### Key Performance Indicators

1. **Good Performance**:
   - Cache hits: < 100µs average latency
   - Cache misses: < 50ms average latency
   - QPS: > 10,000 for cache hits
   - Memory: Linear growth with cache size

2. **Performance Issues**:
   - High P99 latency (> 100ms)
   - Low QPS (< 1,000)
   - High error rate (> 1%)
   - Exponential memory growth

### Common Bottlenecks

1. **Cache Contention**
   - Symptom: Degraded performance at high concurrency
   - Cause: Lock contention on cache access
   - Solution: Implement sharded locking

2. **Memory Allocations**
   - Symptom: High CPU usage in allocator
   - Cause: Frequent small allocations
   - Solution: Object pooling, arena allocators

3. **Blocking List Lookup**
   - Symptom: Slow performance with large blocklists
   - Cause: Linear search through domains
   - Solution: Trie or radix tree implementation

4. **Network I/O**
   - Symptom: High latency for cache misses
   - Cause: Synchronous upstream queries
   - Solution: Connection pooling, async I/O

## Performance Tuning

Based on test results, you can tune Heimdall with environment variables:

```bash
# Increase cache size for better hit rates
HEIMDALL_CACHE_CAPACITY=100000 cargo run

# Adjust worker threads for concurrency
HEIMDALL_WORKER_THREADS=8 cargo run

# Enable rate limiting to prevent overload
HEIMDALL_ENABLE_RATE_LIMITING=true \
HEIMDALL_QUERIES_PER_SECOND_PER_IP=1000 cargo run
```

## Continuous Performance Testing

For regression testing:

```bash
# Create performance baseline
./scripts/check_performance.sh --create-baseline

# Check for regressions (fails if > 10% regression)
./scripts/check_performance.sh --max-regression 10.0
```

## Troubleshooting

### Server Won't Start
- Check if port 1053 is already in use
- Ensure you have permissions for the port
- Check server logs in results directory

### Low Performance Numbers
- Ensure server is built in release mode
- Check for other processes using CPU
- Verify network connectivity
- Review server logs for errors

### Profiling Tools Missing
- macOS: `brew install valgrind`
- Ubuntu: `apt-get install linux-tools-generic heaptrack`
- Fedora: `dnf install perf valgrind heaptrack`

## Best Practices

1. **Baseline Testing**
   - Always create a baseline before optimizations
   - Run tests multiple times for consistency
   - Test in isolation (no other heavy processes)

2. **Realistic Workloads**
   - Use domain names similar to production
   - Test with appropriate concurrency levels
   - Include both cache hits and misses

3. **Profiling**
   - Start with basic performance tests
   - Use profiling to identify specific bottlenecks
   - Focus optimization efforts on measured issues

4. **Validation**
   - Verify optimizations improve performance
   - Check for regressions in other scenarios
   - Monitor resource usage changes

## Example Workflow

```bash
# 1. Create baseline
./scripts/check_performance.sh --create-baseline

# 2. Run comprehensive performance tests
./scripts/performance_bottleneck_test.sh

# 3. Identify bottlenecks from report
cat performance_results/*/performance_report.md

# 4. Run targeted profiling
./scripts/profile_memory_locks.sh --type locks

# 5. Implement optimizations
# ... make code changes ...

# 6. Verify improvements
./scripts/check_performance.sh --max-regression 5.0

# 7. Run full test suite again
./scripts/performance_bottleneck_test.sh
```

## Performance Goals

Target metrics for Heimdall DNS server:

- **Throughput**: > 50,000 QPS for cached queries
- **Latency**: < 100µs P99 for cache hits
- **Concurrency**: Support 1,000+ concurrent clients
- **Memory**: < 1GB for 1M cached entries
- **CPU**: < 50% utilization at 10,000 QPS

## Contributing

When adding new features or optimizations:

1. Run performance tests before changes
2. Implement changes
3. Run performance tests after changes
4. Include performance impact in PR description
5. Add new test scenarios if needed