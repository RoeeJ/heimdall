//! Common test utilities for Heimdall DNS server tests
//!
//! This module consolidates all test helper functions that were previously
//! duplicated across various test files.

#![allow(dead_code)] // These functions are used by various test files

use heimdall::{
    cache::{
        CacheEntry, DnsCache, RedisConfig, local_backend::LocalCache, redis_backend::RedisCache,
    },
    config::DnsConfig,
    dns::{
        DNSPacket,
        enums::{DNSResourceClass, DNSResourceType},
        header::DNSHeader,
        question::DNSQuestion,
        resource::DNSResource,
    },
    metrics::DnsMetrics,
    rate_limiter::{DnsRateLimiter, RateLimitConfig},
    resolver::DnsResolver,
};
use std::sync::Arc;
use std::time::Duration;

/// Create a basic test DNS query packet
pub fn create_test_query(domain: &str, qtype: DNSResourceType) -> DNSPacket {
    create_test_query_with_id(1234, domain, qtype)
}

/// Create a test DNS query packet with specific ID
pub fn create_test_query_with_id(id: u16, domain: &str, qtype: DNSResourceType) -> DNSPacket {
    let labels: Vec<String> = domain.split('.').map(|s| s.to_string()).collect();

    DNSPacket {
        header: DNSHeader {
            id,
            qr: false,
            opcode: 0,
            aa: false,
            tc: false,
            rd: true,
            ra: false,
            z: 0,
            rcode: 0,
            qdcount: 1,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        },
        questions: vec![DNSQuestion {
            labels,
            qtype,
            qclass: DNSResourceClass::IN,
        }],
        answers: vec![],
        authorities: vec![],
        resources: vec![],
        edns: None,
    }
}

/// Create a test DNS query packet with specific opcode
pub fn create_test_query_with_opcode(
    id: u16,
    opcode: u8,
    domain: &str,
    qtype: DNSResourceType,
) -> DNSPacket {
    let mut packet = create_test_query_with_id(id, domain, qtype);
    packet.header.opcode = opcode;
    packet
}

/// Create a test DNS response packet
pub fn create_test_response(query: &DNSPacket, answers: Vec<DNSResource>) -> DNSPacket {
    DNSPacket {
        header: DNSHeader {
            id: query.header.id,
            qr: true,
            opcode: query.header.opcode,
            aa: false,
            tc: false,
            rd: query.header.rd,
            ra: true,
            z: 0,
            rcode: 0,
            qdcount: query.questions.len() as u16,
            ancount: answers.len() as u16,
            nscount: 0,
            arcount: 0,
        },
        questions: query.questions.clone(),
        answers,
        authorities: vec![],
        resources: vec![],
        edns: None,
    }
}

/// Create a simple test packet as bytes
pub fn create_test_packet_bytes() -> Vec<u8> {
    vec![
        0x12, 0x34, // ID
        0x01, 0x00, // Flags: standard query
        0x00, 0x01, // Questions: 1
        0x00, 0x00, // Answers: 0
        0x00, 0x00, // Authority: 0
        0x00, 0x00, // Additional: 0
        // Question: example.com
        0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm',
        0x00, // End of name
        0x00, 0x01, // Type: A
        0x00, 0x01, // Class: IN
    ]
}

/// Create a test packet with a specific resource record
pub fn create_test_packet_with_resource(resource: DNSResource) -> Vec<u8> {
    let packet = DNSPacket {
        header: DNSHeader {
            id: 1234,
            qr: true,
            opcode: 0,
            aa: false,
            tc: false,
            rd: true,
            ra: true,
            z: 0,
            rcode: 0,
            qdcount: 1,
            ancount: 1,
            nscount: 0,
            arcount: 0,
        },
        questions: vec![DNSQuestion {
            labels: resource.labels.clone(),
            qtype: resource.rtype,
            qclass: resource.rclass,
        }],
        answers: vec![resource],
        authorities: vec![],
        resources: vec![],
        edns: None,
    };

    packet.serialize().unwrap()
}

/// Create a test A record
pub fn create_test_a_record(domain: &str, ip: &str, ttl: u32) -> DNSResource {
    let labels: Vec<String> = domain.split('.').map(|s| s.to_string()).collect();
    let ip_parts: Vec<u8> = ip.split('.').map(|s| s.parse().unwrap()).collect();

    DNSResource {
        labels,
        rtype: DNSResourceType::A,
        rclass: DNSResourceClass::IN,
        ttl,
        rdlength: 4,
        rdata: ip_parts,
        parsed_rdata: Some(ip.to_string()),
        raw_class: None,
    }
}

/// Create a test AAAA record
pub fn create_test_aaaa_record(domain: &str, ipv6: &str, ttl: u32) -> DNSResource {
    let labels: Vec<String> = domain.split('.').map(|s| s.to_string()).collect();
    let addr = ipv6.parse::<std::net::Ipv6Addr>().unwrap();

    DNSResource {
        labels,
        rtype: DNSResourceType::AAAA,
        rclass: DNSResourceClass::IN,
        ttl,
        rdlength: 16,
        rdata: addr.octets().to_vec(),
        parsed_rdata: Some(ipv6.to_string()),
        raw_class: None,
    }
}

/// Create a test cache entry
pub fn create_test_cache_entry(ttl_seconds: u64) -> CacheEntry {
    let packet = create_test_response(
        &create_test_query("example.com", DNSResourceType::A),
        vec![create_test_a_record("example.com", "192.0.2.1", 3600)],
    );

    CacheEntry::new(packet, ttl_seconds as u32, false)
}

/// Create a test local cache
pub fn create_test_local_cache(max_size: usize) -> LocalCache {
    use heimdall::cache::CacheStats;
    let stats = Arc::new(CacheStats::new());
    LocalCache::new(max_size, stats)
}

/// Create a test Redis cache (requires Redis connection)
pub async fn create_test_redis_cache() -> Option<RedisCache> {
    RedisCache::new("redis://127.0.0.1:6379", "heimdall_test:".to_string(), 3600)
        .await
        .ok()
}

/// Create a default test configuration
pub fn create_test_config() -> DnsConfig {
    DnsConfig {
        bind_addr: "127.0.0.1:10053".parse().unwrap(),
        upstream_servers: vec!["8.8.8.8:53".parse().unwrap()],
        root_servers: vec![],
        enable_iterative: false,
        max_iterations: 10,
        upstream_timeout: Duration::from_secs(5),
        max_retries: 3,
        enable_caching: true,
        max_cache_size: 1000,
        default_ttl: 300,
        enable_parallel_queries: false,
        worker_threads: 0,
        blocking_threads: 0,
        max_concurrent_queries: 100,
        rate_limit_config: RateLimitConfig {
            enable_rate_limiting: false,
            queries_per_second_per_ip: 10,
            burst_size_per_ip: 20,
            global_queries_per_second: 1000,
            global_burst_size: 2000,
            cleanup_interval_seconds: 60,
            errors_per_second_per_ip: 50,
            max_rate_limit_entries: 10000,
            nxdomain_per_second_per_ip: 20,
        },
        cache_file_path: None,
        cache_save_interval: 0,
        http_bind_addr: None,
        redis_config: RedisConfig {
            enabled: false,
            url: None,
            key_prefix: String::new(),
            connection_timeout: Duration::from_secs(5),
            max_retries: 3,
        },
        dnssec_enabled: false,
        dnssec_strict: false,
        zone_files: vec![],
        authoritative_enabled: false,
        dynamic_updates_enabled: false,
        blocking_enabled: false,
        blocking_mode: "nxdomain".to_string(),
        blocking_custom_ip: None,
        blocking_enable_wildcards: false,
        blocklists: vec![],
        allowlist: vec![],
        transport_config: Default::default(),
        blocklist_auto_update: false,
        blocklist_update_interval: 0,
        blocking_download_psl: false,
    }
}

/// Create test components (resolver, metrics, rate limiter)
pub async fn create_test_components() -> (Arc<DnsResolver>, Arc<DnsMetrics>, Arc<DnsRateLimiter>) {
    let config = create_test_config();
    let _cache = Arc::new(DnsCache::new(config.max_cache_size, config.default_ttl));
    let metrics = Arc::new(DnsMetrics::new().unwrap());
    let rate_limiter = Arc::new(DnsRateLimiter::new(config.rate_limit_config.clone()).unwrap());

    let resolver = Arc::new(
        DnsResolver::new(config.clone(), Some(metrics.clone()))
            .await
            .unwrap(),
    );

    (resolver, metrics, rate_limiter)
}

/// Create a minimal test resolver
pub async fn create_test_resolver() -> Arc<DnsResolver> {
    let (resolver, _, _) = create_test_components().await;
    resolver
}

/// Helper to parse domain name into labels
pub fn parse_domain_labels(domain: &str) -> Vec<String> {
    domain.split('.').map(|s| s.to_string()).collect()
}

/// Helper to create a mock upstream server response
pub fn create_mock_upstream_response(query: &DNSPacket, rcode: u8) -> DNSPacket {
    DNSPacket {
        header: DNSHeader {
            id: query.header.id,
            qr: true,
            opcode: query.header.opcode,
            aa: false,
            tc: false,
            rd: query.header.rd,
            ra: true,
            z: 0,
            rcode,
            qdcount: query.questions.len() as u16,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        },
        questions: query.questions.clone(),
        answers: vec![],
        authorities: vec![],
        resources: vec![],
        edns: None,
    }
}

/// Helper to wait for a condition with timeout
pub async fn wait_for_condition<F>(mut condition: F, timeout: Duration) -> bool
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_query() {
        let query = create_test_query("example.com", DNSResourceType::A);
        assert_eq!(query.header.id, 1234);
        assert_eq!(query.questions.len(), 1);
        assert_eq!(query.questions[0].labels, vec!["example", "com"]);
    }

    #[test]
    fn test_create_test_response() {
        let query = create_test_query("example.com", DNSResourceType::A);
        let answer = create_test_a_record("example.com", "192.0.2.1", 3600);
        let response = create_test_response(&query, vec![answer]);

        assert_eq!(response.header.id, query.header.id);
        assert!(response.header.qr);
        assert_eq!(response.answers.len(), 1);
    }

    #[test]
    fn test_create_test_packet_bytes() {
        let bytes = create_test_packet_bytes();
        assert_eq!(bytes.len(), 29); // Header(12) + Question(17)
        assert_eq!(bytes[0], 0x12); // ID high byte
        assert_eq!(bytes[1], 0x34); // ID low byte
    }
}
