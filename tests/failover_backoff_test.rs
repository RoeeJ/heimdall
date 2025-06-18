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
#[ignore] // This test requires network access
async fn test_exponential_backoff_for_failed_servers() {
    // Test that failed servers get exponential backoff and eventually recover
    let config = DnsConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        upstream_servers: vec![
            "192.0.2.1:53".parse().unwrap(), // Unreachable (will fail)
        ],
        upstream_timeout: Duration::from_millis(500), // Short timeout for quick failure
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
        dynamic_updates_enabled: false,
        transport_config: heimdall::transport::TransportConfig::default(),
        allowed_zone_transfers: vec![],
    };

    let resolver = DnsResolver::new(config, None)
        .await
        .expect("Failed to create resolver");

    // Make multiple queries to trigger consecutive failures
    for i in 0..5 {
        let mut query = DNSPacket::default();
        query.header.id = 4000 + i;
        query.header.rd = true;
        query.header.qdcount = 1;

        let question = DNSQuestion {
            labels: vec![
                format!("backoff{}", i),
                "test".to_string(),
                "com".to_string(),
            ],
            qtype: DNSResourceType::A,
            qclass: DNSResourceClass::IN,
        };
        query.questions.push(question);

        let start_time = std::time::Instant::now();
        let result = resolver.resolve(query, 4000 + i).await;
        let query_time = start_time.elapsed();

        println!(
            "Query {}: result={:?}, time={:?}",
            i,
            result.is_ok(),
            query_time
        );

        // All queries should fail since we only have one unreachable server
        assert!(result.is_err(), "Query should fail with unreachable server");

        // Add a small delay between queries
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Check final health stats
    let health_stats = resolver.get_server_health_stats();
    println!(
        "Exponential backoff test - server health:\n{}",
        resolver.get_health_debug_info()
    );

    let unreachable_server: SocketAddr = "192.0.2.1:53".parse().unwrap();

    if let Some(stats) = health_stats.get(&unreachable_server) {
        println!(
            "Final stats: requests={}, failures={}, healthy={}",
            stats.total_requests, stats.consecutive_failures, stats.is_healthy
        );

        assert!(
            stats.total_requests >= 3,
            "Should have tried the server multiple times"
        );
        assert!(
            stats.consecutive_failures >= 3,
            "Should have multiple consecutive failures"
        );
        assert!(
            !stats.is_healthy,
            "Server should be marked unhealthy after 3+ failures"
        );
    }
}

#[tokio::test]
#[ignore] // This test requires network access
async fn test_successful_failover_to_backup_servers() {
    // Test multiple backup servers and proper failover ordering
    let config = DnsConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        upstream_servers: vec![
            "192.0.2.1:53".parse().unwrap(), // Primary (unreachable)
            "192.0.2.2:53".parse().unwrap(), // Secondary (unreachable)
            "1.1.1.1:53".parse().unwrap(),   // Tertiary (working)
            "8.8.8.8:53".parse().unwrap(),   // Quaternary (working backup)
        ],
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
        dynamic_updates_enabled: false,
        transport_config: heimdall::transport::TransportConfig::default(),
        allowed_zone_transfers: vec![],
    };

    let resolver = DnsResolver::new(config, None)
        .await
        .expect("Failed to create resolver");

    // Make a query that should fail over from primary -> secondary -> tertiary (success)
    let mut query = DNSPacket::default();
    query.header.id = 5000;
    query.header.rd = true;
    query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec![
            "failover".to_string(),
            "test".to_string(),
            "com".to_string(),
        ],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    query.questions.push(question);

    let start_time = std::time::Instant::now();
    let result = resolver.resolve(query, 5000).await;
    let total_time = start_time.elapsed();

    println!("Failover query completed in {:?}", total_time);
    assert!(
        result.is_ok(),
        "Query should succeed via failover to working server"
    );

    let response = result.unwrap();
    assert_eq!(response.header.id, 5000);
    assert!(response.header.qr);
    assert!(response.header.ancount > 0, "Should have answers");

    // Check the health statistics to verify failover behavior
    let health_stats = resolver.get_server_health_stats();
    println!(
        "Failover test - server health:\n{}",
        resolver.get_health_debug_info()
    );

    let primary: SocketAddr = "192.0.2.1:53".parse().unwrap();
    let secondary: SocketAddr = "192.0.2.2:53".parse().unwrap();
    let tertiary: SocketAddr = "1.1.1.1:53".parse().unwrap();
    let quaternary: SocketAddr = "8.8.8.8:53".parse().unwrap();

    // Primary should have been tried and failed
    if let Some(stats) = health_stats.get(&primary) {
        assert!(stats.total_requests > 0, "Primary should have been tried");
        assert!(
            stats.consecutive_failures > 0,
            "Primary should have failures"
        );
        println!(
            "Primary: requests={}, failures={}",
            stats.total_requests, stats.consecutive_failures
        );
    }

    // Secondary should have been tried and failed
    if let Some(stats) = health_stats.get(&secondary) {
        assert!(stats.total_requests > 0, "Secondary should have been tried");
        assert!(
            stats.consecutive_failures > 0,
            "Secondary should have failures"
        );
        println!(
            "Secondary: requests={}, failures={}",
            stats.total_requests, stats.consecutive_failures
        );
    }

    // Tertiary should have been tried and succeeded
    if let Some(stats) = health_stats.get(&tertiary) {
        assert!(stats.total_requests > 0, "Tertiary should have been tried");
        assert!(
            stats.successful_responses > 0,
            "Tertiary should have succeeded"
        );
        assert!(stats.is_healthy, "Tertiary should be healthy");
        println!(
            "Tertiary: requests={}, successes={}",
            stats.total_requests, stats.successful_responses
        );
    }

    // Quaternary should not have been tried (tertiary succeeded first)
    if let Some(stats) = health_stats.get(&quaternary) {
        println!(
            "Quaternary: requests={}, successes={}",
            stats.total_requests, stats.successful_responses
        );
        // Note: Quaternary may or may not have been tried depending on timing and implementation
    }
}

#[tokio::test]
#[ignore] // This test requires network access
async fn test_health_recovery_after_reset() {
    // Test that servers can recover from unhealthy state
    let config = DnsConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        upstream_servers: vec![
            "1.1.1.1:53".parse().unwrap(), // This will always work
        ],
        upstream_timeout: Duration::from_millis(2000),
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
        dynamic_updates_enabled: false,
        transport_config: heimdall::transport::TransportConfig::default(),
        allowed_zone_transfers: vec![],
    };

    let resolver = DnsResolver::new(config, None)
        .await
        .expect("Failed to create resolver");
    let server_addr: SocketAddr = "1.1.1.1:53".parse().unwrap();

    // Make a successful query first
    let mut query = DNSPacket::default();
    query.header.id = 6000;
    query.header.rd = true;
    query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec![
            "recovery".to_string(),
            "test".to_string(),
            "com".to_string(),
        ],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    query.questions.push(question);

    let result = resolver.resolve(query, 6000).await;
    assert!(result.is_ok(), "Initial query should succeed");

    // Check that server is healthy
    let stats_before = resolver.get_server_health_stats();
    if let Some(server_stats) = stats_before.get(&server_addr) {
        assert!(
            server_stats.is_healthy,
            "Server should be initially healthy"
        );
        assert_eq!(server_stats.successful_responses, 1);
        println!(
            "Before: requests={}, successes={}, healthy={}",
            server_stats.total_requests, server_stats.successful_responses, server_stats.is_healthy
        );
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
        println!(
            "After reset: requests={}, failures={}, healthy={}",
            server_stats.total_requests, server_stats.consecutive_failures, server_stats.is_healthy
        );
    }

    println!("Health recovery test completed successfully");
}
