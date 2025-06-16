use dashmap::DashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{debug, warn};

use crate::error::{DnsError, Result};

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub queries_per_second_per_ip: u32,
    pub burst_size: u32,
    pub cleanup_interval: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            queries_per_second_per_ip: 100,
            burst_size: 200,
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

#[derive(Debug)]
struct TokenBucket {
    tokens: AtomicU64,
    last_refill: AtomicU64,
    rate: u32,
    burst_size: u32,
}

impl TokenBucket {
    fn new(rate: u32, burst_size: u32) -> Self {
        Self {
            tokens: AtomicU64::new(burst_size as u64),
            last_refill: AtomicU64::new(Instant::now().elapsed().as_nanos() as u64),
            rate,
            burst_size,
        }
    }

    fn try_consume(&self) -> bool {
        let now = Instant::now().elapsed().as_nanos() as u64;
        let last = self.last_refill.load(Ordering::Relaxed);
        let elapsed_nanos = now.saturating_sub(last);

        // Refill tokens based on elapsed time
        if elapsed_nanos > 0 {
            let elapsed_secs = elapsed_nanos as f64 / 1_000_000_000.0;
            let new_tokens = (elapsed_secs * self.rate as f64) as u64;

            if new_tokens > 0 {
                self.last_refill.store(now, Ordering::Relaxed);
                let current = self.tokens.load(Ordering::Relaxed);
                let updated = (current + new_tokens).min(self.burst_size as u64);
                self.tokens.store(updated, Ordering::Relaxed);
            }
        }

        // Try to consume a token
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current == 0 {
                return false;
            }

            if self
                .tokens
                .compare_exchange(current, current - 1, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
    }
}

pub struct RateLimiter {
    ip_limits: Arc<DashMap<IpAddr, TokenBucket>>,
    config: RateLimitConfig,
    requests_blocked: AtomicU64,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let limiter = Self {
            ip_limits: Arc::new(DashMap::new()),
            config: config.clone(),
            requests_blocked: AtomicU64::new(0),
        };

        // Start cleanup task
        if config.enabled {
            let cleanup_limiter = limiter.ip_limits.clone();
            let cleanup_interval = config.cleanup_interval;

            tokio::spawn(async move {
                let mut interval = interval(cleanup_interval);
                loop {
                    interval.tick().await;

                    // Remove entries that haven't been used recently
                    let now = Instant::now().elapsed().as_nanos() as u64;
                    cleanup_limiter.retain(|_ip, bucket| {
                        let last_used = bucket.last_refill.load(Ordering::Relaxed);
                        let elapsed = now.saturating_sub(last_used);
                        elapsed < cleanup_interval.as_nanos() as u64
                    });

                    debug!("Rate limiter cleanup: {} entries", cleanup_limiter.len());
                }
            });
        }

        limiter
    }

    pub async fn check_and_consume(&self, ip: IpAddr) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let bucket = self.ip_limits.entry(ip).or_insert_with(|| {
            TokenBucket::new(
                self.config.queries_per_second_per_ip,
                self.config.burst_size,
            )
        });

        if bucket.try_consume() {
            Ok(())
        } else {
            self.requests_blocked.fetch_add(1, Ordering::Relaxed);
            warn!("Rate limit exceeded for IP: {}", ip);
            Err(DnsError::RateLimitExceeded(ip.to_string()))
        }
    }

    pub fn get_blocked_count(&self) -> u64 {
        self.requests_blocked.load(Ordering::Relaxed)
    }

    pub fn get_tracked_ips(&self) -> usize {
        self.ip_limits.len()
    }
}
