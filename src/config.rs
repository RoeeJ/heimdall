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

        config
    }
}
