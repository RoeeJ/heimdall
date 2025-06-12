use heimdall::config::DnsConfig;
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    question::DNSQuestion,
};
use heimdall::rate_limiter::RateLimitConfig;
use heimdall::resolver::DnsResolver;
use std::time::Duration;

#[tokio::test]
async fn test_create_truncated_response() {
    let config = DnsConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        upstream_servers: vec!["1.1.1.1:53".parse().unwrap()],
        upstream_timeout: Duration::from_millis(1000),
        max_cache_size: 1000,
        default_ttl: 300,
        enable_caching: false,
        enable_iterative: false,
        enable_parallel_queries: false,
        max_retries: 1,
        max_iterations: 5,
        root_servers: vec![],
        cache_file_path: None,
        cache_save_interval: 300,
        worker_threads: 0,
        blocking_threads: 512,
        max_concurrent_queries: 1000,
        rate_limit_config: RateLimitConfig::default(),
        http_bind_addr: None,
        redis_config: Default::default(),
        dnssec_enabled: false,
        dnssec_strict: false,
        zone_files: vec![],
        authoritative_enabled: false,
    };

    let resolver = DnsResolver::new(config, None)
        .await
        .expect("Failed to create resolver");

    // Create a test query
    let mut query = DNSPacket::default();
    query.header.id = 1234;
    query.header.rd = true;
    query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["example".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    query.questions.push(question);

    // Create truncated response
    let truncated = resolver.create_truncated_response(&query);

    // Verify truncated response properties
    assert_eq!(truncated.header.id, 1234);
    assert!(truncated.header.qr); // Is a response
    assert!(truncated.header.tc); // Truncated flag is set
    assert!(truncated.header.ra); // Recursion available
    assert_eq!(truncated.header.rcode, 0); // NOERROR
    assert_eq!(truncated.header.ancount, 0); // No answers
    assert_eq!(truncated.header.nscount, 0); // No authority records
    assert_eq!(truncated.header.arcount, 0); // No additional records

    // Should maintain the original question
    assert_eq!(truncated.header.qdcount, 1);
    assert_eq!(truncated.questions.len(), 1);
    assert_eq!(truncated.questions[0].labels, vec!["example", "com"]);

    // Answer sections should be empty
    assert!(truncated.answers.is_empty());
    assert!(truncated.authorities.is_empty());
    assert!(truncated.resources.is_empty());
}

#[tokio::test]
async fn test_udp_size_limits_basic() {
    // Test basic UDP size limit functionality
    let config = DnsConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        upstream_servers: vec!["1.1.1.1:53".parse().unwrap()],
        upstream_timeout: Duration::from_millis(1000),
        max_cache_size: 1000,
        default_ttl: 300,
        enable_caching: false,
        enable_iterative: false,
        enable_parallel_queries: false,
        max_retries: 1,
        max_iterations: 5,
        root_servers: vec![],
        cache_file_path: None,
        cache_save_interval: 300,
        worker_threads: 0,
        blocking_threads: 512,
        max_concurrent_queries: 1000,
        rate_limit_config: RateLimitConfig::default(),
        http_bind_addr: None,
        redis_config: Default::default(),
        dnssec_enabled: false,
        dnssec_strict: false,
        zone_files: vec![],
        authoritative_enabled: false,
    };

    let _resolver = DnsResolver::new(config, None)
        .await
        .expect("Failed to create resolver");

    // Create query without EDNS
    let mut query_no_edns = DNSPacket::default();
    query_no_edns.header.id = 1001;
    query_no_edns.header.rd = true;
    query_no_edns.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["test".to_string(), "example".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    query_no_edns.questions.push(question);

    // Without EDNS, max UDP size should be 512
    assert_eq!(query_no_edns.max_udp_payload_size(), 512);

    // EDNS support should be false for basic queries
    assert!(!query_no_edns.supports_edns());

    println!(
        "Query without EDNS max size: {}",
        query_no_edns.max_udp_payload_size()
    );
    println!("EDNS support detected: {}", query_no_edns.supports_edns());
}

#[tokio::test]
async fn test_truncated_response_serialization() {
    // Test that truncated responses can be serialized and are small
    let config = DnsConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        upstream_servers: vec!["1.1.1.1:53".parse().unwrap()],
        upstream_timeout: Duration::from_millis(1000),
        max_cache_size: 1000,
        default_ttl: 300,
        enable_caching: false,
        enable_iterative: false,
        enable_parallel_queries: false,
        max_retries: 1,
        max_iterations: 5,
        root_servers: vec![],
        cache_file_path: None,
        cache_save_interval: 300,
        worker_threads: 0,
        blocking_threads: 512,
        max_concurrent_queries: 1000,
        rate_limit_config: RateLimitConfig::default(),
        http_bind_addr: None,
        redis_config: Default::default(),
        dnssec_enabled: false,
        dnssec_strict: false,
        zone_files: vec![],
        authoritative_enabled: false,
    };

    let resolver = DnsResolver::new(config, None)
        .await
        .expect("Failed to create resolver");

    // Create a query
    let mut query = DNSPacket::default();
    query.header.id = 5678;
    query.header.rd = true;
    query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec![
            "verylongdomainname".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        qtype: DNSResourceType::TXT,
        qclass: DNSResourceClass::IN,
    };
    query.questions.push(question);

    // Create truncated response
    let truncated = resolver.create_truncated_response(&query);

    // Serialize the truncated response
    let serialized = truncated
        .serialize()
        .expect("Should serialize successfully");

    // Truncated response should be small enough for UDP
    assert!(
        serialized.len() <= 512,
        "Truncated response should fit in 512 bytes, got {} bytes",
        serialized.len()
    );

    // Should be able to parse the truncated response
    let parsed = DNSPacket::parse(&serialized).expect("Should parse successfully");
    assert_eq!(parsed.header.id, 5678);
    assert!(parsed.header.tc); // TC flag should be set
    assert_eq!(parsed.header.ancount, 0); // No answers

    println!("Truncated response size: {} bytes", serialized.len());
    println!("TC flag set: {}", parsed.header.tc);
}
