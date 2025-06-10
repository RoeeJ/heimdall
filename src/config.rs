use crate::cache::RedisConfig;
use crate::rate_limiter::RateLimitConfig;
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DnsConfig {
    /// Address to bind the DNS server to
    pub bind_addr: SocketAddr,

    /// Upstream DNS servers to forward queries to
    pub upstream_servers: Vec<SocketAddr>,

    /// Root DNS servers for iterative queries
    pub root_servers: Vec<SocketAddr>,

    /// Whether to enable iterative resolution
    pub enable_iterative: bool,

    /// Maximum number of iterations for iterative queries
    pub max_iterations: u8,

    /// Timeout for upstream queries
    pub upstream_timeout: Duration,

    /// Maximum number of retries for upstream queries
    pub max_retries: u8,

    /// Whether to enable response caching
    pub enable_caching: bool,

    /// Maximum cache size (number of entries)
    pub max_cache_size: usize,

    /// Default TTL for cached responses
    pub default_ttl: u32,

    /// Whether to enable parallel queries to upstream servers
    pub enable_parallel_queries: bool,

    /// Number of worker threads for the Tokio runtime (0 = use default)
    pub worker_threads: usize,

    /// Number of blocking threads for the Tokio runtime (0 = use default)
    pub blocking_threads: usize,

    /// Max number of concurrent DNS queries to handle
    pub max_concurrent_queries: usize,

    /// Rate limiting configuration
    pub rate_limit_config: RateLimitConfig,

    /// Cache persistence file path (None = no persistence)
    pub cache_file_path: Option<String>,

    /// Interval to save cache to disk (in seconds, 0 = disabled)
    pub cache_save_interval: u64,

    /// HTTP server bind address for metrics and health checks (None = disabled)
    pub http_bind_addr: Option<SocketAddr>,

    /// Redis configuration for distributed caching
    pub redis_config: RedisConfig,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:1053".parse().unwrap(),
            upstream_servers: vec![
                "1.1.1.1:53".parse().unwrap(), // Cloudflare
                "8.8.8.8:53".parse().unwrap(), // Google
                "8.8.4.4:53".parse().unwrap(), // Google Secondary
            ],
            root_servers: vec![
                "198.41.0.4:53".parse().unwrap(),   // a.root-servers.net
                "199.9.14.201:53".parse().unwrap(), // b.root-servers.net
                "192.33.4.12:53".parse().unwrap(),  // c.root-servers.net
                "199.7.91.13:53".parse().unwrap(),  // d.root-servers.net
            ],
            enable_iterative: true,
            max_iterations: 16,
            upstream_timeout: Duration::from_secs(5),
            max_retries: 2,
            enable_caching: true,
            max_cache_size: 10000,
            default_ttl: 300, // 5 minutes
            enable_parallel_queries: true,
            worker_threads: 0,     // 0 = use Tokio default (number of CPU cores)
            blocking_threads: 512, // Tokio default
            max_concurrent_queries: 10000,
            rate_limit_config: RateLimitConfig::default(),
            cache_file_path: None,    // No persistence by default
            cache_save_interval: 300, // Save every 5 minutes
            http_bind_addr: Some("127.0.0.1:8080".parse().unwrap()), // HTTP server enabled by default
            redis_config: RedisConfig::default(),
        }
    }
}

impl DnsConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        // Override with environment variables if present
        if let Ok(bind_addr) = std::env::var("HEIMDALL_BIND_ADDR") {
            if let Ok(addr) = bind_addr.parse() {
                config.bind_addr = addr;
            }
        }

        if let Ok(upstream_servers) = std::env::var("HEIMDALL_UPSTREAM_SERVERS") {
            let servers: Vec<SocketAddr> = upstream_servers
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if !servers.is_empty() {
                config.upstream_servers = servers;
            }
        }

        if let Ok(timeout_str) = std::env::var("HEIMDALL_UPSTREAM_TIMEOUT") {
            if let Ok(timeout_secs) = timeout_str.parse::<u64>() {
                config.upstream_timeout = Duration::from_secs(timeout_secs);
            }
        }

        if let Ok(enable_iterative) = std::env::var("HEIMDALL_ENABLE_ITERATIVE") {
            config.enable_iterative = enable_iterative.parse().unwrap_or(true);
        }

        if let Ok(max_iterations) = std::env::var("HEIMDALL_MAX_ITERATIONS") {
            if let Ok(iterations) = max_iterations.parse::<u8>() {
                config.max_iterations = iterations;
            }
        }

        if let Ok(enable_caching) = std::env::var("HEIMDALL_ENABLE_CACHING") {
            config.enable_caching = enable_caching.parse().unwrap_or(true);
        }

        if let Ok(max_cache_size) = std::env::var("HEIMDALL_MAX_CACHE_SIZE") {
            if let Ok(size) = max_cache_size.parse::<usize>() {
                config.max_cache_size = size;
            }
        }

        if let Ok(default_ttl) = std::env::var("HEIMDALL_DEFAULT_TTL") {
            if let Ok(ttl) = default_ttl.parse::<u32>() {
                config.default_ttl = ttl;
            }
        }

        if let Ok(enable_parallel) = std::env::var("HEIMDALL_ENABLE_PARALLEL_QUERIES") {
            config.enable_parallel_queries = enable_parallel.parse().unwrap_or(true);
        }

        if let Ok(worker_threads) = std::env::var("HEIMDALL_WORKER_THREADS") {
            if let Ok(threads) = worker_threads.parse::<usize>() {
                config.worker_threads = threads;
            }
        }

        if let Ok(blocking_threads) = std::env::var("HEIMDALL_BLOCKING_THREADS") {
            if let Ok(threads) = blocking_threads.parse::<usize>() {
                config.blocking_threads = threads;
            }
        }

        if let Ok(max_concurrent) = std::env::var("HEIMDALL_MAX_CONCURRENT_QUERIES") {
            if let Ok(max) = max_concurrent.parse::<usize>() {
                config.max_concurrent_queries = max;
            }
        }

        // Rate limiting configuration
        if let Ok(enable_rate_limiting) = std::env::var("HEIMDALL_ENABLE_RATE_LIMITING") {
            config.rate_limit_config.enable_rate_limiting =
                enable_rate_limiting.parse().unwrap_or(false);
        }

        if let Ok(qps_per_ip) = std::env::var("HEIMDALL_QUERIES_PER_SECOND_PER_IP") {
            if let Ok(qps) = qps_per_ip.parse::<u32>() {
                config.rate_limit_config.queries_per_second_per_ip = qps;
            }
        }

        if let Ok(burst_per_ip) = std::env::var("HEIMDALL_BURST_SIZE_PER_IP") {
            if let Ok(burst) = burst_per_ip.parse::<u32>() {
                config.rate_limit_config.burst_size_per_ip = burst;
            }
        }

        if let Ok(global_qps) = std::env::var("HEIMDALL_GLOBAL_QUERIES_PER_SECOND") {
            if let Ok(qps) = global_qps.parse::<u32>() {
                config.rate_limit_config.global_queries_per_second = qps;
            }
        }

        if let Ok(global_burst) = std::env::var("HEIMDALL_GLOBAL_BURST_SIZE") {
            if let Ok(burst) = global_burst.parse::<u32>() {
                config.rate_limit_config.global_burst_size = burst;
            }
        }

        // Cache persistence configuration
        if let Ok(cache_file_path) = std::env::var("HEIMDALL_CACHE_FILE_PATH") {
            if !cache_file_path.is_empty() {
                config.cache_file_path = Some(cache_file_path);
            }
        }

        if let Ok(cache_save_interval) = std::env::var("HEIMDALL_CACHE_SAVE_INTERVAL") {
            if let Ok(interval) = cache_save_interval.parse::<u64>() {
                config.cache_save_interval = interval;
            }
        }

        // HTTP server configuration
        if let Ok(http_bind_addr) = std::env::var("HEIMDALL_HTTP_BIND_ADDR") {
            if http_bind_addr.to_lowercase() == "disabled" || http_bind_addr.is_empty() {
                config.http_bind_addr = None;
            } else if let Ok(addr) = http_bind_addr.parse() {
                config.http_bind_addr = Some(addr);
            }
        }

        // Redis configuration (auto-detected)
        config.redis_config = RedisConfig::from_env();

        config
    }
}
