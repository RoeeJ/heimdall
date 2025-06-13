# CI Test Configuration

This document describes how tests are configured in CI to avoid network-related failures and hanging tests.

## Environment Variables

The following environment variables are set globally in all CI workflows to disable network operations:

```yaml
env:
  HEIMDALL_BLOCKING_ENABLED: false       # Disables DNS blocking subsystem
  HEIMDALL_BLOCKING_DOWNLOAD_PSL: false  # Prevents PSL download attempts
  HEIMDALL_BLOCKLIST_AUTO_UPDATE: false  # Prevents blocklist downloads
```

## Test Categories

### 1. Unit Tests (Always Run)
- Run with `cargo test --workspace`
- Do not require network access
- Should use `test_config()` helper or explicitly disable blocking features

### 2. Integration Tests (Conditionally Run)
- Marked with `#[ignore]` if they require network access
- Can be run manually with `cargo test -- --ignored`
- Examples:
  - `test_dns_server_responds_to_query` - requires running server
  - `test_consecutive_failures_mark_unhealthy` - requires network for DNS queries
  - `test_connection_pooling_stats` - requires upstream DNS servers

### 3. Performance Tests (Optional)
- Run in separate CI job that can fail without blocking
- May require network for DNS resolution benchmarks
- Results are archived but don't affect pipeline status

## Test Configuration Best Practices

### For Test Authors

1. **Always use test_config() helper**:
```rust
use common::test_config;

#[tokio::test]
async fn test_something() {
    let config = test_config();
    let resolver = DnsResolver::new(config, None).await.unwrap();
}
```

2. **Or explicitly disable blocking**:
```rust
#[tokio::test]
async fn test_something() {
    let mut config = DnsConfig::default();
    config.blocking_enabled = false;
    config.blocklist_auto_update = false;
    config.blocking_download_psl = false;
    let resolver = DnsResolver::new(config, None).await.unwrap();
}
```

3. **Mark network tests with #[ignore]**:
```rust
#[tokio::test]
#[ignore] // This test requires network access
async fn test_real_dns_resolution() {
    // Test that queries real DNS servers
}
```

## CI Jobs and Network Requirements

| Job | Network Required | Strategy |
|-----|-----------------|----------|
| test | No | Uses environment variables to disable |
| security | No | Only scans dependencies |
| performance | Yes (optional) | Can fail without blocking pipeline |
| build | No | Only compiles code |
| integration | Yes (limited) | Runs local server, no external queries |
| docker | No | Builds from pre-compiled binary |
| coverage | No | Excludes ignored tests |

## Running Network Tests Locally

To run all tests including those that require network access:

```bash
# Run all tests including ignored ones
cargo test -- --include-ignored

# Run only ignored tests
cargo test -- --ignored

# Run with network features enabled
HEIMDALL_BLOCKING_ENABLED=true \
HEIMDALL_BLOCKING_DOWNLOAD_PSL=true \
HEIMDALL_BLOCKLIST_AUTO_UPDATE=true \
cargo test
```

## Troubleshooting CI Test Failures

1. **Test hangs**: Check if test creates DnsResolver without disabling blocking
2. **Network errors**: Verify test is marked with `#[ignore]` if it needs network
3. **Flaky tests**: Add timeouts and proper error handling
4. **Resource cleanup**: Ensure servers/tasks are properly shut down

## Environment Variable Reference

| Variable | Default in Prod | CI Override | Purpose |
|----------|----------------|-------------|---------|
| HEIMDALL_BLOCKING_ENABLED | true | false | Controls DNS blocking subsystem |
| HEIMDALL_BLOCKING_DOWNLOAD_PSL | true | false | Controls PSL download on startup |
| HEIMDALL_BLOCKLIST_AUTO_UPDATE | true | false | Controls blocklist auto-updates |
| SKIP_INTEGRATION_TESTS | unset | "1" | Skips integration tests in some jobs |