# Test Best Practices for Heimdall

This document outlines best practices for writing tests that avoid common pitfalls, particularly those that can cause tests to hang or fail in CI environments.

## Preventing Test Hangs

### 1. Always Disable Blocking Features in Tests

The most common cause of hanging tests is the DNS resolver attempting to download blocklists or the Public Suffix List during initialization. **Always disable these features in tests**.

#### ❌ Bad - Can hang due to network operations
```rust
#[tokio::test]
async fn test_something() {
    let config = DnsConfig::default();  // Has blocking_enabled=true by default!
    let resolver = DnsResolver::new(config, None).await.unwrap();
    // Test hangs here trying to download blocklists...
}
```

#### ✅ Good - Disables all network operations
```rust
#[tokio::test]
async fn test_something() {
    let mut config = DnsConfig::default();
    config.blocking_enabled = false;
    config.blocklist_auto_update = false;
    config.blocking_download_psl = false;
    let resolver = DnsResolver::new(config, None).await.unwrap();
    // Test runs quickly
}
```

#### ✅ Better - Use the test_config() helper
```rust
use common::test_config;

#[tokio::test]
async fn test_something() {
    let config = test_config();  // Already has all blocking disabled
    let resolver = DnsResolver::new(config, None).await.unwrap();
    // Test runs quickly
}
```

### 2. Configuration Fields That Can Cause Hangs

When creating test configurations, be aware of these fields that can trigger network operations:

- `blocking_enabled`: When true, attempts to initialize blocking subsystem
- `blocklist_auto_update`: When true, attempts to download blocklists
- `blocking_download_psl`: When true, downloads the Public Suffix List
- `dnssec_enabled`: When true, may attempt to fetch trust anchors
- `upstream_servers`: Real DNS servers may be slow or unreachable

### 3. Use Mock or Local Resources

For tests that need blocking functionality:

```rust
#[test]
fn test_blocking_functionality() {
    let mut config = test_config();
    config.blocking_enabled = true;
    config.blocking_download_psl = false;  // Don't download
    config.blocklist_auto_update = false;   // Don't auto-update
    config.blocklists = vec![];             // No blocklists to load
    
    // Create blocker and manually add test domains
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, false);
    blocker.add_domain("test.example");
    // Test blocking behavior
}
```

### 4. Mark Network-Dependent Tests

If a test genuinely needs network access, mark it with `#[ignore]`:

```rust
#[tokio::test]
#[ignore] // This test requires network access
async fn test_real_dns_resolution() {
    // Test that actually queries real DNS servers
}
```

Run ignored tests explicitly when needed:
```bash
cargo test -- --ignored
```

## Common Test Patterns

### Creating Test DNS Packets

```rust
fn create_test_query(domain: &str, qtype: DNSResourceType) -> DNSPacket {
    let mut header = DNSHeader::default();
    header.id = rand::random();
    header.qr = false;  // Query
    header.rd = true;   // Recursion desired
    header.qdcount = 1;
    
    let question = DNSQuestion {
        labels: domain.split('.').map(|s| s.to_string()).collect(),
        qtype,
        qclass: DNSResourceClass::IN,
    };
    
    DNSPacket {
        header,
        questions: vec![question],
        answers: vec![],
        authorities: vec![],
        resources: vec![],
        edns: None,
    }
}
```

### Testing Error Responses

```rust
#[tokio::test]
async fn test_error_responses() {
    let config = test_config();
    let resolver = DnsResolver::new(config, None).await.unwrap();
    let query = create_test_query("example.com", DNSResourceType::A);
    
    // Test various error response methods
    let refused = resolver.create_refused_response(&query);
    assert_eq!(refused.header.rcode, ResponseCode::Refused.to_u8());
    
    let servfail = resolver.create_servfail_response(&query);
    assert_eq!(servfail.header.rcode, ResponseCode::ServerFailure.to_u8());
}
```

### Testing with Timeouts

For tests that might hang, add explicit timeouts:

```rust
#[tokio::test]
async fn test_with_timeout() {
    let result = timeout(Duration::from_secs(5), async {
        // Test code that might hang
    }).await;
    
    assert!(result.is_ok(), "Test timed out");
}
```

## CI-Specific Considerations

### 1. Avoid External Dependencies

CI environments may have:
- Restricted network access
- No access to external DNS servers
- Firewall rules blocking certain ports
- Rate limiting on external services

### 2. Use Deterministic Test Data

Avoid:
- Random ports that might conflict
- Time-based tests that can flake
- Tests dependent on external service availability

### 3. Resource Cleanup

Always clean up resources:

```rust
#[tokio::test]
async fn test_server() {
    let (tx, rx) = broadcast::channel(1);
    let server_handle = tokio::spawn(async move {
        // Server code
    });
    
    // Run test
    
    // Clean up
    tx.send(()).unwrap();  // Send shutdown signal
    server_handle.abort(); // Ensure task stops
}
```

## Debugging Hanging Tests

If a test hangs:

1. **Check for network operations**: Look for DnsResolver::new with default config
2. **Add logging**: Use RUST_LOG=debug to see what's happening
3. **Add timeouts**: Wrap test in timeout() to fail fast
4. **Check resource cleanup**: Ensure servers/tasks are properly shut down
5. **Review configuration**: Ensure all network features are disabled

## Test Organization

- Keep unit tests close to the code they test
- Use integration tests for end-to-end scenarios
- Mark slow or network-dependent tests with `#[ignore]`
- Use the common test utilities module for shared helpers

## Running Tests

```bash
# Run all tests (except ignored)
cargo test

# Run specific test
cargo test test_name

# Run ignored tests
cargo test -- --ignored

# Run tests with output
cargo test -- --nocapture

# Run tests in parallel (default)
cargo test

# Run tests sequentially
cargo test -- --test-threads=1
```