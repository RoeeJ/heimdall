use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DnsConfig {
    /// Address to bind the DNS server to
    pub bind_addr: SocketAddr,
    
    /// Upstream DNS servers to forward queries to
    pub upstream_servers: Vec<SocketAddr>,
    
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
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:1053".parse().unwrap(),
            upstream_servers: vec![
                "1.1.1.1:53".parse().unwrap(),     // Cloudflare
                "8.8.8.8:53".parse().unwrap(),     // Google
                "8.8.4.4:53".parse().unwrap(),     // Google Secondary
            ],
            upstream_timeout: Duration::from_secs(5),
            max_retries: 2,
            enable_caching: true,
            max_cache_size: 10000,
            default_ttl: 300, // 5 minutes
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
        
        config
    }
}