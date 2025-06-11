# Test Implementation Action Plan

**Objective**: Increase test coverage from 34.96% to 70%+ through systematic implementation  
**Approach**: Test-Driven Development (TDD) for new tests, focusing on high-impact modules first

## Immediate Actions (This Week)

### Day 1: Server.rs Foundation Tests
```rust
// tests/server_unit_tests.rs
#[cfg(test)]
mod server_tests {
    use super::*;
    use tokio::net::UdpSocket;
    use std::sync::Arc;
    
    #[tokio::test]
    async fn test_udp_server_initialization() {
        // Test server can bind to port
        let config = Arc::new(DnsConfig::default());
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        assert!(socket.local_addr().is_ok());
    }
    
    #[tokio::test]
    async fn test_handle_dns_query_valid_packet() {
        // Test valid DNS query processing
    }
    
    #[tokio::test]
    async fn test_handle_dns_query_invalid_packet() {
        // Test malformed packet handling
    }
    
    #[tokio::test]
    async fn test_concurrent_query_handling() {
        // Test multiple simultaneous queries
    }
}
```

### Day 2: Integration Test Framework
```rust
// tests/common/test_server.rs
pub struct TestDnsServer {
    port: u16,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestDnsServer {
    pub async fn start() -> Self {
        // Spawn actual DNS server on random port
    }
    
    pub async fn query(&self, domain: &str) -> Result<DnsResponse> {
        // Send DNS query and wait for response
    }
    
    pub async fn stop(self) {
        // Graceful shutdown
    }
}

// tests/integration/basic_dns_test.rs
#[tokio::test]
async fn test_a_record_query() {
    let server = TestDnsServer::start().await;
    let response = server.query("google.com").await.unwrap();
    assert_eq!(response.rcode, ResponseCode::NoError);
    server.stop().await;
}
```

### Day 3: HTTP Server Tests
```rust
// tests/http_endpoints_test.rs
use axum::test::TestClient;

#[tokio::test]
async fn test_health_endpoint() {
    let app = create_http_app(/* config */);
    let client = TestClient::new(app);
    
    let response = client
        .get("/health")
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    let json: Value = response.json().await.unwrap();
    assert_eq!(json["status"], "healthy");
}

#[tokio::test]
async fn test_metrics_endpoint() {
    // Test Prometheus metrics format
}

#[tokio::test] 
async fn test_config_reload_endpoint() {
    // Test configuration reload
}
```

### Day 4: Resolver Error Path Tests
```rust
// tests/resolver_error_tests.rs
#[tokio::test]
async fn test_all_upstreams_fail() {
    let resolver = create_test_resolver_with_failing_upstreams();
    let result = resolver.resolve("example.com", RecordType::A).await;
    assert!(matches!(result, Err(DnsError::UpstreamTimeout)));
}

#[tokio::test]
async fn test_upstream_returns_servfail() {
    // Test SERVFAIL propagation
}

#[tokio::test]
async fn test_tcp_fallback_on_truncation() {
    // Test automatic TCP retry
}
```

### Day 5: Cache Edge Cases
```rust
// tests/cache_edge_cases_test.rs
#[tokio::test]
async fn test_cache_ttl_expiration() {
    let cache = DnsCache::new(/* config */);
    cache.insert(query, response_with_ttl(1));
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(cache.get(&query).is_none());
}

#[tokio::test]
async fn test_cache_memory_limit() {
    // Test LRU eviction at memory limit
}

#[tokio::test]
async fn test_concurrent_cache_access() {
    // Test thread-safe operations
}
```

## Week 2: Comprehensive Protocol Tests

### DNS Resource Parsing Tests
```rust
// tests/dns/resource_parsing_comprehensive_test.rs
// Generate tests for all 85 DNS record types
macro_rules! test_record_type {
    ($name:ident, $rtype:expr, $rdata:expr) => {
        #[test]
        fn $name() {
            let packet = create_response_with_record($rtype, $rdata);
            let parsed = DNSPacket::parse(&packet).unwrap();
            // Verify parsing correctness
        }
    };
}

test_record_type!(test_parse_a_record, RecordType::A, &[192, 168, 1, 1]);
test_record_type!(test_parse_aaaa_record, RecordType::AAAA, &[/* ipv6 */]);
// ... continue for all types
```

### Compression Pointer Tests
```rust
#[test]
fn test_compression_pointer_circular_reference() {
    // Create packet with circular compression
    let packet = create_circular_compression_packet();
    assert!(matches!(
        DNSPacket::parse(&packet),
        Err(DnsError::CircularCompression)
    ));
}
```

## Week 3: Operational Components

### Configuration Tests
```rust
// tests/config_comprehensive_test.rs
#[test]
fn test_env_var_parsing_edge_cases() {
    env::set_var("HEIMDALL_UPSTREAM_SERVERS", "8.8.8.8;1.1.1.1;");
    let config = DnsConfig::from_env().unwrap();
    assert_eq!(config.upstream_servers.len(), 2);
}

#[test]
fn test_invalid_config_rejection() {
    env::set_var("HEIMDALL_UPSTREAM_TIMEOUT", "-1");
    assert!(matches!(
        DnsConfig::from_env(),
        Err(ConfigError::InvalidTimeout(_))
    ));
}
```

### Metrics Accuracy Tests
```rust
// tests/metrics_accuracy_test.rs
#[tokio::test]
async fn test_query_counter_accuracy() {
    let metrics = DnsMetrics::new();
    
    // Send 100 queries
    for _ in 0..100 {
        metrics.record_query("A");
    }
    
    let output = metrics.export_prometheus();
    assert!(output.contains("dns_queries_total{type=\"A\"} 100"));
}
```

## Test Data Management

### Create Test Fixtures
```bash
tests/fixtures/
├── dns_packets/
│   ├── valid_a_query.bin
│   ├── malformed_header.bin
│   ├── compressed_response.bin
│   └── large_response.bin
├── configs/
│   ├── minimal.toml
│   ├── full_featured.toml
│   └── invalid.toml
└── responses/
    ├── google_com_a.json
    ├── nxdomain.json
    └── servfail.json
```

### Test Data Generators
```rust
// tests/common/generators.rs
pub fn generate_random_domain() -> String {
    // Generate valid random domains for property testing
}

pub fn generate_dns_packet(qtype: RecordType) -> Vec<u8> {
    // Generate valid DNS query packets
}

pub fn generate_malformed_packet() -> Vec<u8> {
    // Generate various malformed packets
}
```

## Continuous Integration Setup

### GitHub Actions Workflow
```yaml
# .github/workflows/test-coverage.yml
name: Test Coverage

on: [push, pull_request]

jobs:
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        
      - name: Install tarpaulin
        run: cargo install cargo-tarpaulin
        
      - name: Run tests with coverage
        run: cargo tarpaulin --no-fail-fast --out Xml --all-features
        
      - name: Upload to codecov
        uses: codecov/codecov-action@v3
        with:
          file: ./cobertura.xml
          
      - name: Comment PR with coverage
        uses: 5monkeys/cobertura-action@master
        with:
          minimum_coverage: 60
```

## Success Tracking

### Weekly Coverage Goals
- Week 1 End: 45% (+10%)
- Week 2 End: 55% (+10%) 
- Week 3 End: 65% (+10%)
- Week 4 End: 70% (+5%)

### Daily Progress Tracking
```bash
# Create coverage tracking script
#!/bin/bash
# scripts/track_coverage.sh
DATE=$(date +%Y-%m-%d)
COVERAGE=$(cargo tarpaulin --print-summary | grep -oP '\d+\.\d+%' | head -1)
echo "$DATE: $COVERAGE" >> coverage_history.log
echo "Current coverage: $COVERAGE"
```

## Common Testing Patterns

### 1. Table-Driven Tests
```rust
#[test]
fn test_record_type_parsing() {
    let test_cases = vec![
        ("A", RecordType::A),
        ("AAAA", RecordType::AAAA),
        ("CNAME", RecordType::CNAME),
        // ... more cases
    ];
    
    for (input, expected) in test_cases {
        assert_eq!(RecordType::from_str(input).unwrap(), expected);
    }
}
```

### 2. Timeout Testing
```rust
#[tokio::test(flavor = "multi_thread")]
async fn test_query_timeout() {
    let result = timeout(
        Duration::from_millis(100),
        slow_resolver.resolve("example.com")
    ).await;
    
    assert!(result.is_err());
}
```

### 3. Concurrent Testing
```rust
#[tokio::test]
async fn test_concurrent_queries() {
    let futures = (0..100).map(|i| {
        let server = server.clone();
        async move {
            server.query(&format!("test{}.com", i)).await
        }
    });
    
    let results = future::join_all(futures).await;
    assert!(results.iter().all(|r| r.is_ok()));
}
```

## Next Steps

1. **Immediate**: Start with Day 1 server tests
2. **This Week**: Complete Days 1-5 foundation tests
3. **Next Week**: Begin comprehensive protocol tests
4. **Ongoing**: Update progress daily in coverage_history.log
5. **Review**: Weekly coverage review and plan adjustment