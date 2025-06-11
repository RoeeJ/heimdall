# Test Coverage Improvement Strategy

**Target**: Achieve 80% coverage for critical modules, 60% for all others  
**Timeline**: 4-phase implementation over 4-6 weeks  
**Current Baseline**: 34.96% (1,837/5,255 lines)

## Phase 1: Critical Infrastructure Tests (Week 1)
**Goal**: Establish integration test framework and cover server.rs

### 1.1 Integration Test Framework
- [ ] Create test harness that spawns actual DNS server
- [ ] Add test utilities for DNS query generation
- [ ] Implement response validation helpers
- [ ] Add concurrent test execution support

### 1.2 Server Module Tests (Target: 80% coverage)
```rust
// tests/server_tests.rs
- Test UDP socket binding and packet reception
- Test TCP connection handling with length prefix
- Test concurrent request processing
- Test malformed packet handling
- Test socket error recovery
- Test maximum packet size limits
```

### 1.3 Main Application Tests
```rust
// tests/main_integration_test.rs
- Test server startup with various configs
- Test graceful shutdown on signals
- Test configuration validation
- Test component initialization order
- Test error propagation during startup
```

**Expected Coverage Gain**: +8-10% (server.rs + main.rs)

## Phase 2: Monitoring & Observability (Week 2)
**Goal**: Ensure production monitoring capabilities work correctly

### 2.1 HTTP Server Tests (Target: 80% coverage)
```rust
// tests/http_server_tests.rs
- Test /health endpoint responses
- Test /metrics Prometheus format
- Test configuration reload endpoints
- Test concurrent HTTP requests
- Test malformed HTTP requests
- Test authentication (if applicable)
```

### 2.2 Metrics Module Tests (Target: 70% coverage)
```rust
// tests/metrics_tests.rs
- Test counter increments
- Test histogram observations
- Test gauge updates
- Test label cardinality
- Test metric serialization
- Test thread-safe updates
```

### 2.3 Error Handling Tests
```rust
// src/error.rs unit tests
- Test error conversions
- Test error display formatting
- Test error propagation
- Test error context preservation
```

**Expected Coverage Gain**: +6-8%

## Phase 3: DNS Protocol Completeness (Week 3)
**Goal**: Achieve comprehensive DNS protocol coverage

### 3.1 Resolver Enhancement (Target: 80% coverage)
```rust
// tests/resolver_advanced_tests.rs
- Test all upstream failure scenarios
- Test query timeout handling
- Test TCP fallback mechanisms
- Test EDNS0 negotiation
- Test recursive query limits
- Test cache poisoning prevention
```

### 3.2 Resource Record Parsing (Target: 70% coverage)
```rust
// tests/resource_parsing_tests.rs
- Test all 85 DNS record types
- Test compression pointer edge cases
- Test malformed RDATA handling
- Test maximum label lengths
- Test circular compression references
- Test buffer overflow scenarios
```

### 3.3 DNS Common Utilities (Target: 80% coverage)
```rust
// src/dns/common.rs tests
- Test domain name parsing edge cases
- Test label validation
- Test compression pointer detection
- Test buffer boundary conditions
```

**Expected Coverage Gain**: +10-12%

## Phase 4: Operational Features (Week 4)
**Goal**: Cover remaining operational components

### 4.1 Configuration Management (Target: 70% coverage)
```rust
// tests/config_tests.rs
- Test all environment variable parsing
- Test configuration validation rules
- Test default value application
- Test invalid configuration rejection
```

### 4.2 Hot Reload Testing (Target: 60% coverage)
```rust
// tests/config_reload_tests.rs
- Test file watcher functionality
- Test SIGHUP signal handling
- Test configuration atomic swaps
- Test reload failure recovery
```

### 4.3 Cache Implementation Tests
```rust
// tests/cache_backend_tests.rs
- Test LRU eviction policies
- Test TTL expiration
- Test concurrent access patterns
- Test persistence/restore cycles
- Test memory limit enforcement
```

**Expected Coverage Gain**: +8-10%

## Testing Best Practices

### 1. Test Organization
```
tests/
├── integration/
│   ├── dns_flow_test.rs      # End-to-end DNS queries
│   ├── failover_test.rs      # Upstream failure handling
│   └── performance_test.rs   # Load testing
├── unit/
│   ├── server_test.rs        # Isolated server tests
│   ├── resolver_test.rs      # Resolver logic tests
│   └── cache_test.rs         # Cache behavior tests
└── common/
    ├── mod.rs               # Shared test utilities
    └── dns_helpers.rs       # DNS packet builders
```

### 2. Testing Patterns

#### Use Test Fixtures
```rust
#[fixture]
fn test_server() -> TestServer {
    TestServer::new()
        .with_config(test_config())
        .spawn()
        .await
}
```

#### Mock External Dependencies
```rust
#[mockall::automock]
trait Upstream {
    async fn query(&self, question: &Question) -> Result<DNSPacket>;
}
```

#### Property-Based Testing
```rust
#[proptest]
fn test_domain_parsing(domain: String) {
    // Test with randomly generated domains
    prop_assert!(validate_domain(&domain).is_ok() || !is_valid_domain(&domain));
}
```

### 3. Coverage Enforcement

#### CI/CD Integration
```yaml
# .github/workflows/test.yml
- name: Run tests with coverage
  run: cargo tarpaulin --no-fail-fast --out Xml
- name: Upload coverage
  uses: codecov/codecov-action@v3
- name: Enforce coverage threshold
  run: |
    coverage=$(cargo tarpaulin --print-summary | grep -oP '\d+\.\d+%')
    if (( $(echo "$coverage < 60.0" | bc -l) )); then
      echo "Coverage $coverage is below 60% threshold"
      exit 1
    fi
```

#### Pre-commit Hooks
```bash
#!/bin/bash
# .git/hooks/pre-commit
cargo test --all
cargo tarpaulin --no-fail-fast --print-summary
```

## Success Metrics

1. **Week 1**: Coverage increases to 45% with server.rs tests
2. **Week 2**: Coverage reaches 55% with monitoring tests
3. **Week 3**: Coverage achieves 65% with DNS protocol tests
4. **Week 4**: Coverage exceeds 70% with operational tests

## Risk Mitigation

1. **Flaky Tests**: Use test isolation and deterministic timing
2. **Long Test Times**: Parallelize tests, use test categories
3. **Complex Mocking**: Prefer integration tests over heavy mocking
4. **Coverage Gaming**: Review tests for actual behavior verification

## Maintenance Strategy

1. **New Code Rule**: All new code must include tests (minimum 80% coverage)
2. **Refactoring Rule**: Improve tests when touching existing code
3. **Review Process**: Test quality review in all PRs
4. **Documentation**: Maintain test documentation and examples
5. **Regular Audits**: Monthly coverage trend analysis