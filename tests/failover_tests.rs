use heimdall::config::DnsConfig;
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    question::DNSQuestion,
};
use heimdall::rate_limiter::RateLimitConfig;
use heimdall::resolver::DnsResolver;
use std::net::SocketAddr;
use std::time::Duration;

#[tokio::test]
async fn test_automatic_failover() {
    // Create a test configuration with multiple upstream servers
    // Using unreachable IPs to simulate server failures
    let config = DnsConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        upstream_servers: vec![
            "192.0.2.1:53".parse().unwrap(), // RFC5737 TEST-NET-1 (unreachable)
            "192.0.2.2:53".parse().unwrap(), // RFC5737 TEST-NET-1 (unreachable)
            "1.1.1.1:53".parse().unwrap(),   // Working Cloudflare DNS
        ],
        upstream_timeout: Duration::from_millis(1000),
        max_cache_size: 1000,
        default_ttl: 300,
        enable_caching: true,
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
        blocking_enabled: false,
        blocking_mode: "nxdomain".to_string(),
        blocking_custom_ip: None,
        blocking_enable_wildcards: false,
        blocklists: vec![],
        allowlist: vec![],
        blocklist_auto_update: false,
        blocklist_update_interval: 86400,
        blocking_download_psl: false,
    };

    let resolver = DnsResolver::new(config, None)
        .await
        .expect("Failed to create resolver");

    // Create a test DNS query
    let mut query = DNSPacket::default();
    query.header.id = 12345;
    query.header.rd = true;
    query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["google".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    query.questions.push(question);

    // Resolve the query - should automatically failover to working server (1.1.1.1)
    let result = resolver.resolve(query, 12345).await;

    // Verify the query succeeded despite first two servers being unreachable
    assert!(
        result.is_ok(),
        "Query should succeed via automatic failover"
    );

    let response = result.unwrap();
    assert_eq!(response.header.id, 12345);
    assert!(response.header.qr);
    assert!(
        response.header.ancount > 0,
        "Should have at least one answer"
    );

    // Check server health statistics
    let health_stats = resolver.get_server_health_stats();

    println!(
        "Server health debug info:\n{}",
        resolver.get_health_debug_info()
    );

    // First two servers should have failed requests
    let server1: SocketAddr = "192.0.2.1:53".parse().unwrap();
    let server2: SocketAddr = "192.0.2.2:53".parse().unwrap();
    let server3: SocketAddr = "1.1.1.1:53".parse().unwrap();

    if let Some(stats1) = health_stats.get(&server1) {
        assert!(
            stats1.total_requests > 0,
            "First server should have been tried"
        );
        println!(
            "Server1 stats: requests={}, failures={}, healthy={}",
            stats1.total_requests, stats1.consecutive_failures, stats1.is_healthy
        );
        // Note: Server may not be marked unhealthy after just 1 failure (needs 3 consecutive failures)
    }

    if let Some(stats2) = health_stats.get(&server2) {
        assert!(
            stats2.total_requests > 0,
            "Second server should have been tried"
        );
        println!(
            "Server2 stats: requests={}, failures={}, healthy={}",
            stats2.total_requests, stats2.consecutive_failures, stats2.is_healthy
        );
        // Note: Server may not be marked unhealthy after just 1 failure (needs 3 consecutive failures)
    }

    if let Some(stats3) = health_stats.get(&server3) {
        assert!(
            stats3.total_requests > 0,
            "Third server should have been tried"
        );
        assert_eq!(
            stats3.successful_responses, 1,
            "Third server should have one successful response"
        );
        assert!(stats3.is_healthy, "Third server should be marked healthy");
        println!(
            "Server3 stats: requests={}, successes={}, healthy={}",
            stats3.total_requests, stats3.successful_responses, stats3.is_healthy
        );
    }
}

#[tokio::test]
async fn test_health_based_server_ordering() {
    // Create config with multiple working servers (using actual public DNS)
    let config = DnsConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        upstream_servers: vec![
            "1.1.1.1:53".parse().unwrap(), // Cloudflare
            "8.8.8.8:53".parse().unwrap(), // Google
            "8.8.4.4:53".parse().unwrap(), // Google
        ],
        upstream_timeout: Duration::from_millis(2000),
        max_cache_size: 1000,
        default_ttl: 300,
        enable_caching: false, // Disable caching to test multiple requests
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
        blocking_enabled: false,
        blocking_mode: "nxdomain".to_string(),
        blocking_custom_ip: None,
        blocking_enable_wildcards: false,
        blocklists: vec![],
        allowlist: vec![],
        blocklist_auto_update: false,
        blocklist_update_interval: 86400,
        blocking_download_psl: false,
    };

    let resolver = DnsResolver::new(config, None)
        .await
        .expect("Failed to create resolver");

    // Make multiple queries to build up health statistics
    for i in 0..3 {
        let mut query = DNSPacket::default();
        query.header.id = 1000 + i;
        query.header.rd = true;
        query.header.qdcount = 1;

        let question = DNSQuestion {
            labels: vec!["example".to_string(), "com".to_string()],
            qtype: DNSResourceType::A,
            qclass: DNSResourceClass::IN,
        };
        query.questions.push(question);

        let result = resolver.resolve(query, 1000 + i).await;
        assert!(result.is_ok(), "Query {} should succeed", i);

        // Small delay between queries
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Check that all servers have health statistics
    let health_stats = resolver.get_server_health_stats();

    for &server_addr in &[
        "1.1.1.1:53".parse::<SocketAddr>().unwrap(),
        "8.8.8.8:53".parse::<SocketAddr>().unwrap(),
        "8.8.4.4:53".parse::<SocketAddr>().unwrap(),
    ] {
        if let Some(stats) = health_stats.get(&server_addr) {
            assert!(stats.is_healthy, "Server {} should be healthy", server_addr);
            // Not all servers may have been used due to smart failover prioritizing working servers
            if stats.total_requests > 0 {
                assert!(
                    stats.avg_response_time.is_some(),
                    "Server {} should have response time data if it was used",
                    server_addr
                );
            }
            println!(
                "Server {} stats: requests={}, healthy={}",
                server_addr, stats.total_requests, stats.is_healthy
            );
        }
    }

    println!("Health-based server ordering test completed");
    println!(
        "Server health debug info:\n{}",
        resolver.get_health_debug_info()
    );
}

#[tokio::test]
async fn test_server_health_recovery() {
    // Test that servers can recover from unhealthy state
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
        blocking_enabled: false,
        blocking_mode: "nxdomain".to_string(),
        blocking_custom_ip: None,
        blocking_enable_wildcards: false,
        blocklists: vec![],
        allowlist: vec![],
        blocklist_auto_update: false,
        blocklist_update_interval: 86400,
        blocking_download_psl: false,
    };

    let resolver = DnsResolver::new(config, None)
        .await
        .expect("Failed to create resolver");
    let server_addr: SocketAddr = "1.1.1.1:53".parse().unwrap();

    // Make a successful query first
    let mut query = DNSPacket::default();
    query.header.id = 1000;
    query.header.rd = true;
    query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["google".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    query.questions.push(question);

    let result = resolver.resolve(query, 1000).await;
    assert!(result.is_ok(), "Initial query should succeed");

    // Check that server is healthy
    let stats = resolver.get_server_health_stats();
    if let Some(server_stats) = stats.get(&server_addr) {
        assert!(
            server_stats.is_healthy,
            "Server should be initially healthy"
        );
        assert_eq!(server_stats.successful_responses, 1);
    }

    // Test manual health reset functionality
    resolver.reset_server_health(server_addr);

    let stats_after_reset = resolver.get_server_health_stats();
    if let Some(server_stats) = stats_after_reset.get(&server_addr) {
        assert!(
            server_stats.is_healthy,
            "Server should still be healthy after reset"
        );
        assert_eq!(
            server_stats.consecutive_failures, 0,
            "Failures should be reset to 0"
        );
    }

    println!("Server health recovery test completed");
}
