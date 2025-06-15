pub mod cache_config;

use crate::cache::RedisConfig;
use crate::error::ConfigError;
use crate::rate_limiter::RateLimitConfig;
use crate::transport::{TlsConfig, TransportConfig};
use std::net::SocketAddr;
use std::time::Duration;

pub use cache_config::CacheConfig;

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

    /// Whether to enable dynamic DNS updates (RFC 2136)
    pub dynamic_updates_enabled: bool,

    /// Whether to enable DNS blocking
    pub blocking_enabled: bool,

    /// Blocking mode (nxdomain, zero_ip, custom_ip, refused)
    pub blocking_mode: String,

    /// Custom IP to return for blocked domains (if mode is custom_ip)
    pub blocking_custom_ip: Option<String>,

    /// Enable wildcard blocking (*.domain.com)
    pub blocking_enable_wildcards: bool,

    /// Blocklist file paths with format: path:format:name
    pub blocklists: Vec<String>,

    /// Allowlist domains (never blocked)
    pub allowlist: Vec<String>,

    /// Auto-update blocklists
    pub blocklist_auto_update: bool,

    /// Blocklist update interval in seconds
    pub blocklist_update_interval: u64,

    /// Whether to download PSL on startup (disable for tests)
    pub blocking_download_psl: bool,

    /// Transport layer configuration for DNS-over-TLS and other protocols
    pub transport_config: TransportConfig,

    /// Cache configuration with optimizations
    pub cache_config: CacheConfig,
}

impl Default for DnsConfig {
    fn default() -> Self {
        // Check environment variables for blocking settings (useful for CI/testing)
        let blocking_enabled = std::env::var("HEIMDALL_BLOCKING_ENABLED")
            .map(|v| parse_bool(&v, true))
            .unwrap_or(true); // Default to true if not set

        let blocking_download_psl = std::env::var("HEIMDALL_BLOCKING_DOWNLOAD_PSL")
            .map(|v| parse_bool(&v, true))
            .unwrap_or(true); // Default to true if not set

        let blocklist_auto_update = std::env::var("HEIMDALL_BLOCKLIST_AUTO_UPDATE")
            .map(|v| parse_bool(&v, true))
            .unwrap_or(true); // Default to true if not set

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
            dynamic_updates_enabled: false, // Disabled by default for security
            blocking_enabled,
            blocking_mode: "zero_ip".to_string(), // Use zero_ip as default (common choice)
            blocking_custom_ip: None,
            blocking_enable_wildcards: true,
            blocklists: vec![
                // Default blocklists in path:format:name format
                "blocklists/stevenblack-hosts.txt:hosts:StevenBlack".to_string(),
                "blocklists/malware-domains.txt:hosts:MalwareDomains".to_string(),
            ],
            allowlist: vec![],
            blocklist_auto_update,
            blocklist_update_interval: 86400, // 24 hours
            blocking_download_psl,
            transport_config: TransportConfig::default(),
            cache_config: CacheConfig::default(),
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

        if let Ok(dynamic_updates_enabled) = std::env::var("HEIMDALL_DYNAMIC_UPDATES_ENABLED") {
            config.dynamic_updates_enabled = parse_bool(&dynamic_updates_enabled, false);
        }

        // Blocking configuration
        if let Ok(blocking_enabled) = std::env::var("HEIMDALL_BLOCKING_ENABLED") {
            config.blocking_enabled = parse_bool(&blocking_enabled, false);
        }

        if let Ok(blocking_mode) = std::env::var("HEIMDALL_BLOCKING_MODE") {
            config.blocking_mode = blocking_mode.to_lowercase();
        }

        if let Ok(custom_ip) = std::env::var("HEIMDALL_BLOCKING_CUSTOM_IP") {
            config.blocking_custom_ip = Some(custom_ip);
        }

        if let Ok(enable_wildcards) = std::env::var("HEIMDALL_BLOCKING_ENABLE_WILDCARDS") {
            config.blocking_enable_wildcards = parse_bool(&enable_wildcards, true);
        }

        if let Ok(download_psl) = std::env::var("HEIMDALL_BLOCKING_DOWNLOAD_PSL") {
            config.blocking_download_psl = parse_bool(&download_psl, true);
        }

        if let Ok(blocklists) = std::env::var("HEIMDALL_BLOCKLISTS") {
            config.blocklists = blocklists
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        if let Ok(allowlist) = std::env::var("HEIMDALL_ALLOWLIST") {
            config.allowlist = allowlist
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        if let Ok(auto_update) = std::env::var("HEIMDALL_BLOCKLIST_AUTO_UPDATE") {
            config.blocklist_auto_update = parse_bool(&auto_update, false);
        }

        // Load cache configuration from environment
        config.cache_config = CacheConfig::from_env();

        if let Ok(update_interval) = std::env::var("HEIMDALL_BLOCKLIST_UPDATE_INTERVAL") {
            config.blocklist_update_interval = update_interval.parse::<u64>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid blocklist update interval: {}",
                    update_interval
                ))
            })?;
        }

        // DNS-over-TLS configuration
        if let Ok(enable_dot) = std::env::var("HEIMDALL_ENABLE_DOT") {
            config.transport_config.enable_dot = parse_bool(&enable_dot, false);
        }

        if let Ok(dot_bind_addr) = std::env::var("HEIMDALL_DOT_BIND_ADDR") {
            if dot_bind_addr.to_lowercase() == "disabled" || dot_bind_addr.is_empty() {
                config.transport_config.dot_bind_addr = None;
            } else {
                config.transport_config.dot_bind_addr =
                    Some(dot_bind_addr.parse().map_err(|_| {
                        ConfigError::ParseError(format!(
                            "Invalid DoT bind address: {}",
                            dot_bind_addr
                        ))
                    })?);
            }
        }

        if let Ok(cert_path) = std::env::var("HEIMDALL_TLS_CERT_PATH") {
            let key_path = std::env::var("HEIMDALL_TLS_KEY_PATH").map_err(|_| {
                ConfigError::ParseError("TLS cert path specified but key path missing".to_string())
            })?;

            let mut tls_config = TlsConfig::new(cert_path, key_path);

            if let Ok(server_name) = std::env::var("HEIMDALL_TLS_SERVER_NAME") {
                tls_config = tls_config.with_server_name(server_name);
            }

            if let Ok(require_client_cert) = std::env::var("HEIMDALL_TLS_REQUIRE_CLIENT_CERT") {
                if parse_bool(&require_client_cert, false) {
                    let ca_path = std::env::var("HEIMDALL_TLS_CLIENT_CA_PATH").map_err(|_| {
                        ConfigError::ParseError(
                            "Client cert required but CA path missing".to_string(),
                        )
                    })?;
                    tls_config = tls_config.with_client_cert_required(ca_path);
                }
            }

            config.transport_config.tls_config = Some(tls_config);
        }

        if let Ok(max_connections) = std::env::var("HEIMDALL_DOT_MAX_CONNECTIONS") {
            config.transport_config.max_connections =
                max_connections.parse::<usize>().map_err(|_| {
                    ConfigError::ParseError(format!(
                        "Invalid DoT max connections: {}",
                        max_connections
                    ))
                })?;
        }

        if let Ok(connection_timeout) = std::env::var("HEIMDALL_DOT_CONNECTION_TIMEOUT") {
            let timeout_secs = connection_timeout.parse::<u64>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid DoT connection timeout: {}",
                    connection_timeout
                ))
            })?;
            config.transport_config.connection_timeout = Duration::from_secs(timeout_secs);
        }

        if let Ok(keepalive_timeout) = std::env::var("HEIMDALL_DOT_KEEPALIVE_TIMEOUT") {
            let timeout_secs = keepalive_timeout.parse::<u64>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "Invalid DoT keepalive timeout: {}",
                    keepalive_timeout
                ))
            })?;
            config.transport_config.keepalive_timeout = Duration::from_secs(timeout_secs);
        }

        // DNS-over-HTTPS configuration
        if let Ok(enable_doh) = std::env::var("HEIMDALL_ENABLE_DOH") {
            config.transport_config.enable_doh = parse_bool(&enable_doh, false);
        }

        if let Ok(doh_bind_addr) = std::env::var("HEIMDALL_DOH_BIND_ADDR") {
            if doh_bind_addr.to_lowercase() == "disabled" || doh_bind_addr.is_empty() {
                config.transport_config.doh_bind_addr = None;
            } else {
                config.transport_config.doh_bind_addr =
                    Some(doh_bind_addr.parse().map_err(|_| {
                        ConfigError::ParseError(format!(
                            "Invalid DoH bind address: {}",
                            doh_bind_addr
                        ))
                    })?);
            }
        }

        if let Ok(doh_path) = std::env::var("HEIMDALL_DOH_PATH") {
            config.transport_config.doh_path = doh_path;
        }

        if let Ok(enable_well_known) = std::env::var("HEIMDALL_DOH_ENABLE_WELL_KNOWN") {
            config.transport_config.doh_enable_well_known = parse_bool(&enable_well_known, true);
        }

        if let Ok(enable_json_api) = std::env::var("HEIMDALL_DOH_ENABLE_JSON_API") {
            config.transport_config.doh_enable_json_api = parse_bool(&enable_json_api, true);
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

        // Blocking configuration validation
        if self.blocking_enabled {
            match self.blocking_mode.as_str() {
                "nxdomain" | "zero_ip" | "refused" => {}
                "custom_ip" => {
                    if self.blocking_custom_ip.is_none() {
                        return Err(ConfigError::ParseError(
                            "Blocking mode 'custom_ip' requires HEIMDALL_BLOCKING_CUSTOM_IP to be set".to_string(),
                        ));
                    }
                    // Validate IP address
                    if let Some(ref ip) = self.blocking_custom_ip {
                        use std::net::IpAddr;
                        ip.parse::<IpAddr>().map_err(|_| {
                            ConfigError::ParseError(format!("Invalid custom blocking IP: {}", ip))
                        })?;
                    }
                }
                _ => {
                    return Err(ConfigError::ParseError(format!(
                        "Invalid blocking mode: {}. Must be one of: nxdomain, zero_ip, custom_ip, refused",
                        self.blocking_mode
                    )));
                }
            }

            // Validate blocklist format
            for blocklist in &self.blocklists {
                let parts: Vec<&str> = blocklist.split(':').collect();
                if parts.len() != 3 {
                    return Err(ConfigError::ParseError(format!(
                        "Invalid blocklist format: {}. Expected: path:format:name",
                        blocklist
                    )));
                }
                // Validate format
                match parts[1] {
                    "domain_list" | "hosts" | "adblock" | "pihole" | "dnsmasq" | "unbound" => {}
                    _ => {
                        return Err(ConfigError::ParseError(format!(
                            "Invalid blocklist format '{}'. Must be one of: domain_list, hosts, adblock, pihole, dnsmasq, unbound",
                            parts[1]
                        )));
                    }
                }
            }
        }

        // DoT configuration validation
        if self.transport_config.enable_dot {
            if self.transport_config.dot_bind_addr.is_none() {
                return Err(ConfigError::ParseError(
                    "DoT enabled but no bind address specified".to_string(),
                ));
            }
            if self.transport_config.tls_config.is_none() {
                return Err(ConfigError::ParseError(
                    "DoT enabled but no TLS configuration provided".to_string(),
                ));
            }

            // Validate TLS configuration if present
            if let Some(ref tls_config) = self.transport_config.tls_config {
                if let Err(e) = tls_config.validate() {
                    return Err(ConfigError::ParseError(format!(
                        "TLS configuration invalid: {}",
                        e
                    )));
                }
            }
        }

        // DoH configuration validation
        if self.transport_config.enable_doh {
            if self.transport_config.doh_bind_addr.is_none() {
                return Err(ConfigError::ParseError(
                    "DoH enabled but no bind address specified".to_string(),
                ));
            }

            // DoH path validation
            if !self.transport_config.doh_path.starts_with('/') {
                return Err(ConfigError::ParseError(
                    "DoH path must start with '/'".to_string(),
                ));
            }

            // If TLS is enabled for DoH, validate TLS configuration
            if let Some(ref tls_config) = self.transport_config.tls_config {
                if let Err(e) = tls_config.validate() {
                    return Err(ConfigError::ParseError(format!(
                        "TLS configuration invalid for DoH: {}",
                        e
                    )));
                }
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
