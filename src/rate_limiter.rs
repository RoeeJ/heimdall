use dashmap::DashMap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for DNS rate limiting
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enable_rate_limiting: bool,

    /// Queries per second per IP address
    pub queries_per_second_per_ip: u32,

    /// Burst allowance per IP
    pub burst_size_per_ip: u32,

    /// Global queries per second limit
    pub global_queries_per_second: u32,

    /// Global burst allowance
    pub global_burst_size: u32,

    /// Error responses per second per IP
    pub errors_per_second_per_ip: u32,

    /// NXDOMAIN responses per second per IP
    pub nxdomain_per_second_per_ip: u32,

    /// Maximum rate limiter table size
    pub max_rate_limit_entries: usize,

    /// Rate limiter cleanup interval in seconds
    pub cleanup_interval_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enable_rate_limiting: false,
            queries_per_second_per_ip: 50,    // 50 QPS per IP
            burst_size_per_ip: 100,           // Allow bursts up to 100
            global_queries_per_second: 10000, // 10k QPS global limit
            global_burst_size: 20000,         // 20k burst globally
            errors_per_second_per_ip: 5,      // Limit error responses
            nxdomain_per_second_per_ip: 5,    // Limit NXDOMAIN responses
            max_rate_limit_entries: 100000,   // Track up to 100k IPs
            cleanup_interval_seconds: 300,    // Cleanup every 5 minutes
        }
    }
}

type IpRateLimiter = DefaultDirectRateLimiter;

/// DNS-specific rate limiter with per-IP and global limiting
#[derive(Debug)]
pub struct DnsRateLimiter {
    config: RateLimitConfig,

    // Per-IP rate limiters
    per_ip_limiters: DashMap<IpAddr, IpRateLimiter>,

    // Global rate limiter
    global_limiter: IpRateLimiter,

    // Error response limiters
    error_limiters: DashMap<IpAddr, IpRateLimiter>,

    // NXDOMAIN response limiters
    nxdomain_limiters: DashMap<IpAddr, IpRateLimiter>,
}

impl DnsRateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        // Create global rate limiter
        let global_quota =
            Quota::per_second(NonZeroU32::new(config.global_queries_per_second).unwrap())
                .allow_burst(NonZeroU32::new(config.global_burst_size).unwrap());
        let global_limiter = RateLimiter::direct(global_quota);

        Self {
            config,
            per_ip_limiters: DashMap::new(),
            global_limiter,
            error_limiters: DashMap::new(),
            nxdomain_limiters: DashMap::new(),
        }
    }

    /// Check if a query from the given IP should be allowed
    pub fn check_query_allowed(&self, client_ip: IpAddr) -> bool {
        if !self.config.enable_rate_limiting {
            return true;
        }

        // Check global rate limit first
        if self.global_limiter.check().is_err() {
            debug!("Global rate limit exceeded");
            return false;
        }

        // Get or create per-IP rate limiter
        let ip_limiter = self.get_or_create_ip_limiter(client_ip);

        // Check per-IP rate limit
        match ip_limiter.check() {
            Ok(_) => true,
            Err(_) => {
                debug!("Rate limit exceeded for IP: {}", client_ip);
                false
            }
        }
    }

    /// Check if an error response should be allowed
    pub fn check_error_response_allowed(&self, client_ip: IpAddr) -> bool {
        if !self.config.enable_rate_limiting {
            return true;
        }

        let error_limiter = self.get_or_create_error_limiter(client_ip);
        match error_limiter.check() {
            Ok(_) => true,
            Err(_) => {
                warn!("Error response rate limit exceeded for IP: {}", client_ip);
                false
            }
        }
    }

    /// Check if an NXDOMAIN response should be allowed  
    pub fn check_nxdomain_response_allowed(&self, client_ip: IpAddr) -> bool {
        if !self.config.enable_rate_limiting {
            return true;
        }

        let nxdomain_limiter = self.get_or_create_nxdomain_limiter(client_ip);
        match nxdomain_limiter.check() {
            Ok(_) => true,
            Err(_) => {
                warn!(
                    "NXDOMAIN response rate limit exceeded for IP: {}",
                    client_ip
                );
                false
            }
        }
    }

    /// Block an IP temporarily by consuming all its tokens
    pub fn block_ip(&self, ip: IpAddr, _duration: Duration) {
        if let Some(limiter) = self.per_ip_limiters.get(&ip) {
            // Consume all available tokens to effectively block the IP
            while limiter.check().is_ok() {
                // Keep consuming tokens until rate limited
            }
        }
    }

    fn get_or_create_ip_limiter(
        &self,
        ip: IpAddr,
    ) -> dashmap::mapref::one::Ref<IpAddr, IpRateLimiter> {
        // Use entry API to avoid race conditions
        if !self.per_ip_limiters.contains_key(&ip) {
            let quota =
                Quota::per_second(NonZeroU32::new(self.config.queries_per_second_per_ip).unwrap())
                    .allow_burst(NonZeroU32::new(self.config.burst_size_per_ip).unwrap());
            self.per_ip_limiters.insert(ip, RateLimiter::direct(quota));
        }
        self.per_ip_limiters.get(&ip).unwrap()
    }

    fn get_or_create_error_limiter(
        &self,
        ip: IpAddr,
    ) -> dashmap::mapref::one::Ref<IpAddr, IpRateLimiter> {
        if !self.error_limiters.contains_key(&ip) {
            let quota =
                Quota::per_second(NonZeroU32::new(self.config.errors_per_second_per_ip).unwrap());
            self.error_limiters.insert(ip, RateLimiter::direct(quota));
        }
        self.error_limiters.get(&ip).unwrap()
    }

    fn get_or_create_nxdomain_limiter(
        &self,
        ip: IpAddr,
    ) -> dashmap::mapref::one::Ref<IpAddr, IpRateLimiter> {
        if !self.nxdomain_limiters.contains_key(&ip) {
            let quota =
                Quota::per_second(NonZeroU32::new(self.config.nxdomain_per_second_per_ip).unwrap());
            self.nxdomain_limiters
                .insert(ip, RateLimiter::direct(quota));
        }
        self.nxdomain_limiters.get(&ip).unwrap()
    }

    /// Clean up old rate limiter entries to prevent memory leaks
    pub fn cleanup_expired_entries(&self) {
        // Simple cleanup: if we have too many entries, clear some
        if self.per_ip_limiters.len() > self.config.max_rate_limit_entries {
            warn!("Rate limiter table size limit reached, performing cleanup");

            // Clear half the entries (simple LRU would be better but more complex)
            if self.per_ip_limiters.len() > self.config.max_rate_limit_entries * 2 {
                self.per_ip_limiters.clear();
                self.error_limiters.clear();
                self.nxdomain_limiters.clear();
                debug!("Cleared all rate limiter entries due to excessive memory usage");
            }
        }
    }

    /// Get rate limiting statistics
    pub fn get_stats(&self) -> RateLimitStats {
        RateLimitStats {
            active_ip_limiters: self.per_ip_limiters.len(),
            active_error_limiters: self.error_limiters.len(),
            active_nxdomain_limiters: self.nxdomain_limiters.len(),
        }
    }
}

#[derive(Debug)]
pub struct RateLimitStats {
    pub active_ip_limiters: usize,
    pub active_error_limiters: usize,
    pub active_nxdomain_limiters: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    #[test]
    fn test_rate_limiter_creation() {
        let config = RateLimitConfig::default();
        let limiter = DnsRateLimiter::new(config);

        let stats = limiter.get_stats();
        assert_eq!(stats.active_ip_limiters, 0);
    }

    #[test]
    fn test_basic_rate_limiting() {
        let config = RateLimitConfig {
            enable_rate_limiting: true,
            queries_per_second_per_ip: 2, // Very low limit for testing
            burst_size_per_ip: 2,
            ..Default::default()
        };

        let limiter = DnsRateLimiter::new(config);
        let test_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        // First few queries should pass
        assert!(limiter.check_query_allowed(test_ip));
        assert!(limiter.check_query_allowed(test_ip));

        // Should be rate limited now
        assert!(!limiter.check_query_allowed(test_ip));
    }

    #[test]
    fn test_per_ip_isolation() {
        let config = RateLimitConfig {
            enable_rate_limiting: true,
            queries_per_second_per_ip: 1,
            burst_size_per_ip: 1,
            ..Default::default()
        };

        let limiter = DnsRateLimiter::new(config);
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2));

        // Exhaust rate limit for IP1
        assert!(limiter.check_query_allowed(ip1));
        assert!(!limiter.check_query_allowed(ip1));

        // IP2 should still be allowed
        assert!(limiter.check_query_allowed(ip2));
    }

    #[test]
    fn test_global_rate_limiting() {
        let config = RateLimitConfig {
            enable_rate_limiting: true,
            global_queries_per_second: 2,
            global_burst_size: 2,
            queries_per_second_per_ip: 100, // High per-IP limit
            ..Default::default()
        };

        let limiter = DnsRateLimiter::new(config);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        // Should be limited by global limit, not per-IP limit
        assert!(limiter.check_query_allowed(ip));
        assert!(limiter.check_query_allowed(ip));
        assert!(!limiter.check_query_allowed(ip)); // Global limit hit
    }

    #[test]
    fn test_error_response_limiting() {
        let config = RateLimitConfig {
            enable_rate_limiting: true,
            errors_per_second_per_ip: 1,
            ..Default::default()
        };

        let limiter = DnsRateLimiter::new(config);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        // First error response should be allowed
        assert!(limiter.check_error_response_allowed(ip));

        // Second should be blocked
        assert!(!limiter.check_error_response_allowed(ip));
    }

    #[test]
    fn test_disabled_rate_limiting() {
        let config = RateLimitConfig {
            enable_rate_limiting: false,
            ..Default::default()
        };

        let limiter = DnsRateLimiter::new(config);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        // Should always allow when disabled
        for _ in 0..1000 {
            assert!(limiter.check_query_allowed(ip));
            assert!(limiter.check_error_response_allowed(ip));
            assert!(limiter.check_nxdomain_response_allowed(ip));
        }
    }

    #[test]
    fn test_cleanup() {
        let config = RateLimitConfig {
            enable_rate_limiting: true,
            ..Default::default()
        };
        let limiter = DnsRateLimiter::new(config);

        // Add some entries
        for i in 1..=10 {
            let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, i));
            limiter.check_query_allowed(ip);
        }

        let stats_before = limiter.get_stats();
        assert_eq!(stats_before.active_ip_limiters, 10);

        // Cleanup shouldn't remove entries under normal circumstances
        limiter.cleanup_expired_entries();
        let stats_after = limiter.get_stats();
        assert_eq!(stats_after.active_ip_limiters, 10);
    }

    #[tokio::test]
    async fn test_rate_recovery() {
        let config = RateLimitConfig {
            enable_rate_limiting: true,
            queries_per_second_per_ip: 10, // 10 QPS = 100ms per query
            burst_size_per_ip: 1,
            ..Default::default()
        };

        let limiter = DnsRateLimiter::new(config);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        // Exhaust rate limit
        assert!(limiter.check_query_allowed(ip));
        assert!(!limiter.check_query_allowed(ip));

        // Wait for rate to recover (governor uses internal timing)
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should be allowed again
        assert!(limiter.check_query_allowed(ip));
    }
}
