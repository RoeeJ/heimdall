use std::env;

/// Cache configuration with optimizations
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Use optimized lock-free cache implementation
    pub use_optimized_cache: bool,
    /// Maximum cache size
    pub max_size: usize,
    /// Negative response TTL
    pub negative_ttl: u32,
    /// Hot cache size (percentage of main cache)
    pub hot_cache_percentage: u8,
    /// Access count threshold for hot cache promotion
    pub hot_cache_promotion_threshold: u32,
    /// Enable cache line optimization
    pub cache_line_optimization: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            use_optimized_cache: env::var("HEIMDALL_USE_OPTIMIZED_CACHE")
                .map(|v| v.parse().unwrap_or(true))
                .unwrap_or(true), // Default to optimized cache
            max_size: 10_000,
            negative_ttl: 300,
            hot_cache_percentage: 10,
            hot_cache_promotion_threshold: 3,
            cache_line_optimization: true,
        }
    }
}

impl CacheConfig {
    pub fn from_env() -> Self {
        Self {
            use_optimized_cache: env::var("HEIMDALL_USE_OPTIMIZED_CACHE")
                .map(|v| v.parse().unwrap_or(true))
                .unwrap_or(true),
            max_size: env::var("HEIMDALL_CACHE_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10_000),
            negative_ttl: env::var("HEIMDALL_NEGATIVE_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            hot_cache_percentage: env::var("HEIMDALL_HOT_CACHE_PERCENTAGE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            hot_cache_promotion_threshold: env::var("HEIMDALL_HOT_CACHE_PROMOTION_THRESHOLD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
            cache_line_optimization: env::var("HEIMDALL_CACHE_LINE_OPTIMIZATION")
                .map(|v| v.parse().unwrap_or(true))
                .unwrap_or(true),
        }
    }
}
