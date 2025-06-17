# Heimdall Testing Guide

## Overview

This guide consolidates all testing documentation for the Heimdall DNS server project. It provides the current test coverage status, improvement strategies, and implementation guidelines.

**Current Coverage**: 40.84% (2,146/5,255 lines)  
**Target Coverage**: 70%+ for critical modules, 60%+ for others  
**Last Updated**: June 2024

## Test Coverage Status

### Coverage Breakdown by Category

#### Well-Tested Modules (>70%)
| Module | Coverage | Priority | Notes |
|--------|----------|----------|-------|
| dns/header.rs | 100% | ✅ | Complete coverage |
| dns/enums.rs | 89.9% | ✅ | Well tested |
| rate_limiter.rs | 72.3% | High | Security-critical |
| validation.rs | 71.4% | High | Input validation |

#### Moderately Tested (40-70%)
| Module | Coverage | Priority | Notes |
|--------|----------|----------|-------|
| cache/mod.rs | 69.5% | High | Performance-critical |
| dns/common.rs | 66.7% | Medium | Shared utilities |
| dns/question.rs | 56.5% | Medium | DNS query parsing |
| dns/mod.rs | 53.3% | High | Core DNS packet handling |
| server.rs | 52.5% | **CRITICAL** | Main UDP/TCP server |
| resolver.rs | 49.6% | Critical | DNS resolution logic |
| dns/resource.rs | 40.7% | High | Resource record parsing |

#### Poorly Tested (<40%)
| Module | Coverage | Priority | Notes |
|--------|----------|----------|-------|
| config.rs | 33.1% | Medium | Configuration handling |
| metrics.rs | 24.0% | Medium | Metrics collection |
| http_server.rs | 18.1% | High | Health/metrics endpoints |
| config_reload.rs | 10.96% | Medium | Hot reload feature |
| cache/redis_backend.rs | 7.4% | Low | Redis cache |

#### Untested Modules (0%)
| Module | Coverage | Priority | Notes |
|--------|----------|----------|-------|
| main.rs | 0% | **CRITICAL** | Application entry point |
| cluster_registry.rs | 0% | Low | Distributed features |
| graceful_shutdown.rs | 0% | Medium | Shutdown handling |
| error.rs | 0% | High | Error types |
| cache/local_backend.rs | 0% | High | Cache implementation |

## Recent Progress

### Server Module Tests (June 2024)
- **Coverage Improvement**: 0% → 52.5% (+96 lines)
- **Tests Added**: 11 comprehensive integration tests
- **Key Features Tested**:
  - UDP/TCP server startup and query handling
  - Graceful shutdown signal handling
  - Rate limiting and concurrent query processing
  - Security policy enforcement (AXFR/ANY query refusal)
  - Invalid opcode and malformed packet handling

### HTTP Server Tests (June 2024)
- **Coverage Improvement**: 0% → 18.1% (+63 lines)
- **Tests Added**: 9 HTTP endpoint tests
- **Key Features Tested**:
  - Health check endpoints (/health, /health/detailed)
  - Prometheus metrics export (/metrics)
  - Configuration hot-reload endpoint
  - CORS functionality
  - Concurrent request handling

## Testing Strategy

### Phase 1: Critical Infrastructure (Current Focus)
- [x] Server.rs integration tests
- [x] HTTP server endpoint tests
- [ ] Main.rs application lifecycle tests
- [ ] Error handling framework tests

### Phase 2: DNS Protocol Completeness
- [ ] Resolver error path coverage (target: 80%)
- [ ] Resource record parsing for all 85 DNS types
- [ ] Compression pointer edge cases
- [ ] EDNS0 and TCP fallback mechanisms

### Phase 3: Operational Features
- [ ] Configuration management tests
- [ ] Cache backend implementation tests
- [ ] Metrics accuracy verification
- [ ] Hot reload functionality

### Phase 4: Performance & Reliability
- [ ] Load testing framework
- [ ] Failover scenario tests
- [ ] Memory leak detection
- [ ] Benchmark regression tests

## Test Organization

```
tests/
├── integration/          # End-to-end tests
│   ├── dns_flow_test.rs      # Complete DNS query flows
│   ├── server_test.rs        # Server startup/shutdown
│   └── failover_test.rs      # Upstream failure handling
├── unit/                 # Isolated component tests
│   ├── cache_test.rs         # Cache behavior
│   ├── resolver_test.rs      # Resolver logic
│   └── validation_test.rs    # Input validation
├── common/              # Shared test utilities
│   ├── mod.rs               # Test helpers
│   ├── dns_helpers.rs       # DNS packet builders
│   └── test_server.rs       # Test server harness
└── fixtures/            # Test data
    ├── dns_packets/         # Binary packet samples
    ├── configs/             # Test configurations
    └── responses/           # Expected responses
```

## Writing Tests

### Integration Test Example
```rust
use heimdall::test_utils::{TestDnsServer, create_dns_query};

#[tokio::test]
async fn test_a_record_resolution() {
    // Start test server
    let server = TestDnsServer::start().await;
    
    // Send DNS query
    let response = server.query("google.com", RecordType::A).await.unwrap();
    
    // Verify response
    assert_eq!(response.header.response_code, ResponseCode::NoError);
    assert!(!response.answers.is_empty());
    
    // Cleanup
    server.stop().await;
}
```

### Unit Test Example
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_ttl_expiration() {
        let cache = DnsCache::new(test_config());
        let query = create_test_query("example.com");
        let response = create_test_response(ttl_seconds: 1);
        
        cache.insert(&query, response);
        assert!(cache.get(&query).is_some());
        
        // Wait for TTL expiration
        std::thread::sleep(Duration::from_secs(2));
        assert!(cache.get(&query).is_none());
    }
}
```

### Testing Patterns

#### 1. Table-Driven Tests
```rust
#[test]
fn test_record_type_parsing() {
    let test_cases = vec![
        ("A", RecordType::A, true),
        ("AAAA", RecordType::AAAA, true),
        ("INVALID", RecordType::A, false),
    ];
    
    for (input, expected, should_succeed) in test_cases {
        let result = RecordType::from_str(input);
        if should_succeed {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert!(result.is_err());
        }
    }
}
```

#### 2. Concurrent Testing
```rust
#[tokio::test]
async fn test_concurrent_cache_access() {
    let cache = Arc::new(DnsCache::new(test_config()));
    let mut handles = vec![];
    
    // Spawn 100 concurrent tasks
    for i in 0..100 {
        let cache_clone = cache.clone();
        handles.push(tokio::spawn(async move {
            let query = create_test_query(&format!("test{}.com", i));
            cache_clone.get_or_insert(query, fetch_response).await
        }));
    }
    
    // Wait for all tasks
    let results = futures::future::join_all(handles).await;
    assert!(results.iter().all(|r| r.is_ok()));
}
```

#### 3. Error Injection Testing
```rust
#[tokio::test]
async fn test_upstream_failure_handling() {
    let resolver = create_test_resolver()
        .with_failing_upstream("8.8.8.8")
        .with_failing_upstream("1.1.1.1");
    
    let result = resolver.resolve("example.com", RecordType::A).await;
    assert!(matches!(result, Err(DnsError::AllUpstreamsFailed)));
}
```

## Running Tests

### Basic Commands
```bash
# Run all tests
cargo test

# Run specific test module
cargo test server_tests

# Run with verbose output
cargo test -- --nocapture

# Run integration tests only
cargo test --test '*' 

# Run with coverage
cargo tarpaulin --no-fail-fast --out Html
```

### Performance Testing
```bash
# Run benchmark tests
cargo bench

# Run regression tests
./scripts/check_performance.sh

# Create new performance baseline
./scripts/check_performance.sh --create-baseline
```

### Continuous Integration
The project uses GitHub Actions for automated testing:
- Tests run on every push and pull request
- Coverage reports are uploaded to Codecov
- Minimum coverage threshold: 60%
- Performance regression tests run nightly

## Test Coverage Goals

### Immediate Priorities
1. **main.rs**: Add application lifecycle tests
2. **resolver.rs**: Increase coverage from 49.6% to 80%
3. **http_server.rs**: Increase coverage from 18.1% to 70%
4. **error.rs**: Add comprehensive error handling tests

### Module-Specific Targets
- **Critical modules** (server, resolver, cache): 80% coverage
- **Security modules** (validation, rate_limiter): 80% coverage  
- **Protocol modules** (dns/*): 70% coverage
- **Operational modules** (config, metrics): 60% coverage
- **Optional features** (redis, cluster): 50% coverage

## Best Practices

### Do's
- ✅ Write tests before or alongside implementation
- ✅ Test both success and failure paths
- ✅ Use descriptive test names that explain the scenario
- ✅ Keep tests isolated and independent
- ✅ Mock external dependencies (network, filesystem)
- ✅ Use test fixtures for complex test data
- ✅ Run tests locally before pushing

### Don'ts
- ❌ Don't write tests just for coverage numbers
- ❌ Don't use production configurations in tests
- ❌ Don't rely on external services (use mocks)
- ❌ Don't ignore flaky tests (fix them)
- ❌ Don't test implementation details (test behavior)

## Troubleshooting

### Common Issues

#### Tests Timeout
```rust
// Increase test timeout
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[timeout(30000)] // 30 seconds
async fn test_slow_operation() {
    // test code
}
```

#### Port Conflicts
```rust
// Use port 0 for automatic assignment
let socket = UdpSocket::bind("127.0.0.1:0").await?;
let actual_port = socket.local_addr()?.port();
```

#### Flaky Tests
```rust
// Add retries for inherently flaky operations
#[test_retry(3)]
async fn test_with_network_calls() {
    // test code that might fail due to timing
}
```

## Contributing Tests

When adding new features or fixing bugs:

1. **Write tests first** (TDD approach)
2. **Ensure tests fail** without your changes
3. **Implement the feature/fix**
4. **Verify tests pass**
5. **Check coverage** hasn't decreased
6. **Update this guide** if adding new test patterns

## Metrics and Monitoring

Track testing progress:
- Current coverage: 40.84%
- Tests added this month: 20
- Average test execution time: <5 seconds
- Flaky test rate: <1%

Use `./scripts/track_coverage.sh` to monitor coverage trends.