# Test Utilities for Heimdall DNS Server

This module consolidates all common test utilities and helper functions that were previously duplicated across various test files.

## Usage

In your test files, import the common module:

```rust
mod common;
use common::*;
```

Or for integration tests:

```rust
use heimdall::tests::common;
```

## Available Functions

### DNS Packet Creation

- `create_test_query(domain, qtype)` - Create a basic DNS query packet
- `create_test_query_with_id(id, domain, qtype)` - Create query with specific ID
- `create_test_query_with_opcode(id, opcode, domain, qtype)` - Create query with specific opcode
- `create_test_response(query, answers)` - Create a DNS response packet
- `create_test_packet_bytes()` - Create raw packet bytes for testing parsing
- `create_test_packet_with_resource(resource)` - Create packet with specific resource record

### DNS Record Creation

- `create_test_a_record(domain, ip, ttl)` - Create an A record
- `create_test_aaaa_record(domain, ipv6, ttl)` - Create an AAAA record

### Cache Testing

- `create_test_cache_entry(ttl_seconds)` - Create a cache entry
- `create_test_local_cache(max_size)` - Create a local cache instance
- `create_test_redis_cache()` - Create a Redis cache instance (requires Redis)

### Component Creation

- `create_test_config()` - Create a default test configuration
- `create_test_components()` - Create resolver, metrics, and rate limiter
- `create_test_resolver()` - Create a minimal test resolver

### Helper Functions

- `parse_domain_labels(domain)` - Parse domain into label vector
- `create_mock_upstream_response(query, rcode)` - Create mock upstream response
- `wait_for_condition(condition, timeout)` - Async wait helper

## Example Usage

```rust
#[cfg(test)]
mod tests {
    use super::common::*;
    use heimdall::dns::enums::DNSResourceType;

    #[test]
    fn test_dns_resolution() {
        let query = create_test_query("example.com", DNSResourceType::A);
        let answer = create_test_a_record("example.com", "192.0.2.1", 3600);
        let response = create_test_response(&query, vec![answer]);
        
        assert_eq!(response.header.id, query.header.id);
        assert_eq!(response.answers.len(), 1);
    }
    
    #[tokio::test]
    async fn test_with_resolver() {
        let resolver = create_test_resolver().await;
        // Test resolver functionality
    }
}
```