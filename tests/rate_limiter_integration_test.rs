use heimdall::rate_limiter::{DnsRateLimiter, RateLimitConfig};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

#[test]
fn test_rate_limiter_integration() {
    // Create a rate limiter with very low limits for testing
    let mut config = RateLimitConfig::default();
    config.queries_per_second_per_ip = 2;
    config.burst_size_per_ip = 2;
    config.global_queries_per_second = 10;
    config.global_burst_size = 10;

    let rate_limiter = DnsRateLimiter::new(config);
    let test_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

    // First few queries should pass
    assert!(rate_limiter.check_query_allowed(test_ip));
    assert!(rate_limiter.check_query_allowed(test_ip));

    // Should be rate limited now
    assert!(!rate_limiter.check_query_allowed(test_ip));

    // Different IP should still work
    let test_ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 101));
    assert!(rate_limiter.check_query_allowed(test_ip2));
}

#[test]
fn test_disabled_rate_limiting() {
    let mut config = RateLimitConfig::default();
    config.enable_rate_limiting = false;

    let rate_limiter = DnsRateLimiter::new(config);
    let test_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

    // Should allow unlimited queries when disabled
    for _ in 0..100 {
        assert!(rate_limiter.check_query_allowed(test_ip));
    }
}

#[test]
fn test_error_response_limiting() {
    let mut config = RateLimitConfig::default();
    config.errors_per_second_per_ip = 1;

    let rate_limiter = DnsRateLimiter::new(config);
    let test_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

    // First error response should be allowed
    assert!(rate_limiter.check_error_response_allowed(test_ip));

    // Second should be blocked
    assert!(!rate_limiter.check_error_response_allowed(test_ip));

    // Regular queries should still work
    assert!(rate_limiter.check_query_allowed(test_ip));
}

#[tokio::test]
async fn test_rate_recovery_over_time() {
    let mut config = RateLimitConfig::default();
    config.queries_per_second_per_ip = 10; // 10 QPS = 100ms per query
    config.burst_size_per_ip = 1;

    let rate_limiter = DnsRateLimiter::new(config);
    let test_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

    // Exhaust rate limit
    assert!(rate_limiter.check_query_allowed(test_ip));
    assert!(!rate_limiter.check_query_allowed(test_ip));

    // Wait for rate to recover
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Should be allowed again
    assert!(rate_limiter.check_query_allowed(test_ip));
}

#[test]
fn test_rate_limiter_stats() {
    let config = RateLimitConfig::default();
    let rate_limiter = DnsRateLimiter::new(config);

    // Initially no active limiters
    let stats = rate_limiter.get_stats();
    assert_eq!(stats.active_ip_limiters, 0);
    assert_eq!(stats.active_error_limiters, 0);
    assert_eq!(stats.active_nxdomain_limiters, 0);

    // After making queries, should have active limiters
    let test_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
    rate_limiter.check_query_allowed(test_ip);
    rate_limiter.check_error_response_allowed(test_ip);
    rate_limiter.check_nxdomain_response_allowed(test_ip);

    let stats = rate_limiter.get_stats();
    assert_eq!(stats.active_ip_limiters, 1);
    assert_eq!(stats.active_error_limiters, 1);
    assert_eq!(stats.active_nxdomain_limiters, 1);
}

#[test]
fn test_multiple_ips() {
    let mut config = RateLimitConfig::default();
    config.queries_per_second_per_ip = 1;
    config.burst_size_per_ip = 1;

    let rate_limiter = DnsRateLimiter::new(config);

    // Test multiple IPs to ensure proper isolation
    for i in 1..=10 {
        let test_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, i));

        // Each IP should get its own rate limit
        assert!(rate_limiter.check_query_allowed(test_ip));
        assert!(!rate_limiter.check_query_allowed(test_ip)); // Should be rate limited
    }

    let stats = rate_limiter.get_stats();
    assert_eq!(stats.active_ip_limiters, 10);
}
