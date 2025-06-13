/// Common test utilities
use heimdall::config::DnsConfig;

/// Create a test DNS config that disables all network operations
pub fn test_config() -> DnsConfig {
    DnsConfig {
        blocking_download_psl: false, // Disable PSL download
        blocklist_auto_update: false, // Disable blocklist auto-update
        blocklists: vec![],           // No blocklists to avoid file I/O
        enable_caching: false,        // Disable caching for predictable tests
        upstream_timeout: std::time::Duration::from_secs(2), // Shorter timeout
        max_retries: 0,               // Don't retry in tests
        ..Default::default()
    }
}

/// Create a test DNS config with caching enabled
#[allow(dead_code)]
pub fn test_config_with_cache() -> DnsConfig {
    let mut config = test_config();
    config.enable_caching = true;
    config.max_cache_size = 100;
    config
}

/// Create a test DNS config with specific upstream servers
#[allow(dead_code)]
pub fn test_config_with_upstreams(servers: Vec<std::net::SocketAddr>) -> DnsConfig {
    let mut config = test_config();
    config.upstream_servers = servers;
    config
}
