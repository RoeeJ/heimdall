use crate::cache::RedisConfig;
use crate::error::ConfigError;
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

    /// Whether to enable DNSSEC validation
    pub dnssec_enabled: bool,

    /// Whether to enforce strict DNSSEC validation (reject bogus responses)
    pub dnssec_strict: bool,

    /// Zone files to load for authoritative serving
    pub zone_files: Vec<String>,

    /// Whether to enable authoritative DNS serving
    pub authoritative_enabled: bool,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:1053"
                .parse()
                .expect("Default bind address is valid"),
            upstream_servers: vec![
                "1.1.1.1:53".parse().expect("Cloudflare DNS is valid"),
                "8.8.8.8:53".parse().expect("Google DNS is valid"),
                "8.8.4.4:53".parse().expect("Google Secondary DNS is valid"),
            ],
            root_servers: vec![
                "198.41.0.4:53"
                    .parse()
                    .expect("a.root-servers.net is valid"),
                "199.9.14.201:53"
                    .parse()
                    .expect("b.root-servers.net is valid"),
                "192.33.4.12:53"
                    .parse()
                    .expect("c.root-servers.net is valid"),
                "199.7.91.13:53"
                    .parse()
                    .expect("d.root-servers.net is valid"),
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
            http_bind_addr: Some(
                "127.0.0.1:8080"
                    .parse()
                    .expect("Default HTTP bind address is valid"),
            ),
            redis_config: RedisConfig::default(),
            dnssec_enabled: false, // Disabled by default for backward compatibility
            dnssec_strict: false,  // Non-strict by default
            zone_files: vec![],    // No zones by default
            authoritative_enabled: false, // Disabled by default
        }
    }
}

impl DnsConfig {
    /// Create a DnsConfig from environment variables
    /// Returns Err if critical configuration is invalid
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut config = Self::default();

        // Override with environment variables if present
        if let Ok(bind_addr) = std::env::var("HEIMDALL_BIND_ADDR") {
            config.bind_addr = bind_addr
                .parse()
                .map_err(|_| ConfigError::InvalidBindAddress(bind_addr))?;
        }

        if let Ok(upstream_servers) = std::env::var("HEIMDALL_UPSTREAM_SERVERS") {
            let servers: Result<Vec<SocketAddr>, _> = upstream_servers
                .split(',')
                .map(|s| {
                    s.trim()
                        .parse::<SocketAddr>()
                        .map_err(|_| ConfigError::InvalidUpstreamServer(s.to_string()))
                })
                .collect();

            let servers = servers?;
            if servers.is_empty() {
                return Err(ConfigError::InvalidUpstreamServer(
                    "No valid upstream servers provided".to_string(),
                ));
            }
            config.upstream_servers = servers;
        }

        if let Ok(timeout_str) = std::env::var("HEIMDALL_UPSTREAM_TIMEOUT") {
            let timeout_secs = timeout_str
                .parse::<u64>()
                .map_err(|_| ConfigError::InvalidTimeout(timeout_str.clone()))?;
            if timeout_secs == 0 {
                return Err(ConfigError::InvalidTimeout(
                    "Timeout must be greater than 0".to_string(),
                ));
            }
            config.upstream_timeout = Duration::from_secs(timeout_secs);
        }

        if let Ok(enable_iterative) = std::env::var("HEIMDALL_ENABLE_ITERATIVE") {
            config.enable_iterative = parse_bool(&enable_iterative, true);
        }

        if let Ok(max_iterations) = std::env::var("HEIMDALL_MAX_ITERATIONS") {
            config.max_iterations = max_iterations.parse::<u8>().map_err(|_| {
                ConfigError::ParseError(format!("Invalid max iterations: {}", max_iterations))
            })?;
        }

        if let Ok(enable_caching) = std::env::var("HEIMDALL_ENABLE_CACHING") {
            config.enable_caching = parse_bool(&enable_caching, true);
        }

        if let Ok(max_cache_size) = std::env::var("HEIMDALL_MAX_CACHE_SIZE") {
            let size = max_cache_size
                .parse::<usize>()
                .map_err(|_| ConfigError::InvalidCacheSize(max_cache_size.clone()))?;
            if size == 0 {
                return Err(ConfigError::InvalidCacheSize(
                    "Cache size must be greater than 0".to_string(),
                ));
            }
            config.max_cache_size = size;
        }

        if let Ok(default_ttl) = std::env::var("HEIMDALL_DEFAULT_TTL") {
            config.default_ttl = default_ttl.parse::<u32>().map_err(|_| {
                ConfigError::ParseError(format!("Invalid default TTL: {}", default_ttl))
            })?;
        }

        if let Ok(enable_parallel) = std::env::var("HEIMDALL_ENABLE_PARALLEL_QUERIES") {
            config.enable_parallel_queries = parse_bool(&enable_parallel, true);
        }

        if let Ok(worker_threads) = std::env::var("HEIMDALL_WORKER_THREADS") {
            config.worker_threads = worker_threads
                .parse::<usize>()
                .map_err(|_| ConfigError::InvalidWorkerThreads(worker_threads))?;
        }

        if let Ok(blocking_threads) = std::env::var("HEIMDALL_BLOCKING_THREADS") {
            let threads = blocking_threads.parse::<usize>().map_err(|_| {
                ConfigError::ParseError(format!("Invalid blocking threads: {}", blocking_threads))
            })?;
            if threads == 0 {
                return Err(ConfigError::ParseError(
                    "Blocking threads must be greater than 0".to_string(),
                ));
            }
            config.blocking_threads = threads;
        }

        if let Ok(max_concurrent) = std::env::var("HEIMDALL_MAX_CONCURRENT_QUERIES") {
            let max = max_concurrent.parse::<usize>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid max concurrent queries: {}",
                    max_concurrent
                ))
            })?;
            if max == 0 {
                return Err(ConfigError::ParseError(
                    "Max concurrent queries must be greater than 0".to_string(),
                ));
            }
            config.max_concurrent_queries = max;
        }

        // Rate limiting configuration
        if let Ok(enable_rate_limiting) = std::env::var("HEIMDALL_ENABLE_RATE_LIMITING") {
            config.rate_limit_config.enable_rate_limiting =
                parse_bool(&enable_rate_limiting, false);
        }

        if let Ok(qps_per_ip) = std::env::var("HEIMDALL_QUERIES_PER_SECOND_PER_IP") {
            config.rate_limit_config.queries_per_second_per_ip =
                qps_per_ip.parse::<u32>().map_err(|_| {
                    ConfigError::InvalidRateLimit(format!("Invalid QPS per IP: {}", qps_per_ip))
                })?;
        }

        if let Ok(burst_per_ip) = std::env::var("HEIMDALL_BURST_SIZE_PER_IP") {
            config.rate_limit_config.burst_size_per_ip =
                burst_per_ip.parse::<u32>().map_err(|_| {
                    ConfigError::InvalidRateLimit(format!(
                        "Invalid burst size per IP: {}",
                        burst_per_ip
                    ))
                })?;
        }

        if let Ok(global_qps) = std::env::var("HEIMDALL_GLOBAL_QUERIES_PER_SECOND") {
            config.rate_limit_config.global_queries_per_second =
                global_qps.parse::<u32>().map_err(|_| {
                    ConfigError::InvalidRateLimit(format!("Invalid global QPS: {}", global_qps))
                })?;
        }

        if let Ok(global_burst) = std::env::var("HEIMDALL_GLOBAL_BURST_SIZE") {
            config.rate_limit_config.global_burst_size =
                global_burst.parse::<u32>().map_err(|_| {
                    ConfigError::InvalidRateLimit(format!(
                        "Invalid global burst size: {}",
                        global_burst
                    ))
                })?;
        }

        // Cache persistence configuration
        if let Ok(cache_file_path) = std::env::var("HEIMDALL_CACHE_FILE_PATH") {
            if !cache_file_path.is_empty() {
                config.cache_file_path = Some(cache_file_path);
            }
        }

        if let Ok(cache_save_interval) = std::env::var("HEIMDALL_CACHE_SAVE_INTERVAL") {
            config.cache_save_interval = cache_save_interval.parse::<u64>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid cache save interval: {}",
                    cache_save_interval
                ))
            })?;
        }

        // HTTP server configuration
        if let Ok(http_bind_addr) = std::env::var("HEIMDALL_HTTP_BIND_ADDR") {
            if http_bind_addr.to_lowercase() == "disabled" || http_bind_addr.is_empty() {
                config.http_bind_addr = None;
            } else {
                config.http_bind_addr = Some(
                    http_bind_addr
                        .parse()
                        .map_err(|_| ConfigError::InvalidHttpBindAddress(http_bind_addr))?,
                );
            }
        }

        // DNSSEC configuration
        if let Ok(dnssec_enabled) = std::env::var("HEIMDALL_DNSSEC_ENABLED") {
            config.dnssec_enabled = parse_bool(&dnssec_enabled, false);
        }

        if let Ok(dnssec_strict) = std::env::var("HEIMDALL_DNSSEC_STRICT") {
            config.dnssec_strict = parse_bool(&dnssec_strict, false);
        }

        // Zone file configuration
        if let Ok(zone_files) = std::env::var("HEIMDALL_ZONE_FILES") {
            config.zone_files = zone_files
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        if let Ok(authoritative_enabled) = std::env::var("HEIMDALL_AUTHORITATIVE_ENABLED") {
            config.authoritative_enabled = parse_bool(&authoritative_enabled, false);
        }

        // Redis configuration (auto-detected)
        config.redis_config = RedisConfig::from_env();

        // Validate the final configuration
        config.validate()?;

        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Worker threads validation (0 is allowed for default)
        // No upper limit validation as the system can handle it

        // Cache size must be reasonable
        if self.max_cache_size > 10_000_000 {
            return Err(ConfigError::InvalidCacheSize(
                "Cache size too large (max 10 million entries)".to_string(),
            ));
        }

        // Timeout must be reasonable
        if self.upstream_timeout.as_secs() > 300 {
            return Err(ConfigError::InvalidTimeout(
                "Timeout too large (max 300 seconds)".to_string(),
            ));
        }

        // Max iterations must be reasonable
        if self.max_iterations > 32 {
            return Err(ConfigError::ParseError(
                "Max iterations too large (max 32)".to_string(),
            ));
        }

        // Rate limiting validation
        if self.rate_limit_config.enable_rate_limiting {
            if self.rate_limit_config.queries_per_second_per_ip == 0 {
                return Err(ConfigError::InvalidRateLimit(
                    "QPS per IP must be greater than 0 when rate limiting is enabled".to_string(),
                ));
            }
            if self.rate_limit_config.global_queries_per_second == 0 {
                return Err(ConfigError::InvalidRateLimit(
                    "Global QPS must be greater than 0 when rate limiting is enabled".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Parse a boolean from a string, with a default value for invalid input
fn parse_bool(s: &str, default: bool) -> bool {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = DnsConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_cache_size() {
        let config = DnsConfig {
            max_cache_size: 20_000_000,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_timeout() {
        let config = DnsConfig {
            upstream_timeout: Duration::from_secs(400),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_parse_bool() {
        assert!(parse_bool("true", false));
        assert!(parse_bool("TRUE", false));
        assert!(parse_bool("1", false));
        assert!(parse_bool("yes", false));
        assert!(parse_bool("on", false));

        assert!(!parse_bool("false", true));
        assert!(!parse_bool("FALSE", true));
        assert!(!parse_bool("0", true));
        assert!(!parse_bool("no", true));
        assert!(!parse_bool("off", true));

        assert!(parse_bool("invalid", true));
        assert!(!parse_bool("invalid", false));
    }
}
