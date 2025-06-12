use heimdall::error::{ConfigError, DnsError, Result};
use std::error::Error as StdError;
use std::io;

#[test]
fn test_config_error_display() {
    // Test all ConfigError variants
    let cases = vec![
        (
            ConfigError::InvalidBindAddress("127.0.0.1:999999".to_string()),
            "Invalid bind address: 127.0.0.1:999999",
        ),
        (
            ConfigError::InvalidUpstreamServer("not.a.valid.server".to_string()),
            "Invalid upstream server: not.a.valid.server",
        ),
        (
            ConfigError::InvalidHttpBindAddress(":::99999".to_string()),
            "Invalid HTTP bind address: :::99999",
        ),
        (
            ConfigError::InvalidWorkerThreads("abc".to_string()),
            "Invalid worker threads: abc",
        ),
        (
            ConfigError::InvalidCacheSize("-100".to_string()),
            "Invalid cache size: -100",
        ),
        (
            ConfigError::InvalidTimeout("0ms".to_string()),
            "Invalid timeout: 0ms",
        ),
        (
            ConfigError::InvalidRateLimit("unlimited".to_string()),
            "Invalid rate limit: unlimited",
        ),
        (
            ConfigError::ParseError("unexpected EOF".to_string()),
            "Parse error: unexpected EOF",
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn test_config_error_std_error_impl() {
    let error = ConfigError::InvalidBindAddress("test".to_string());
    // Verify it implements std::error::Error
    let _: &dyn StdError = &error;
}

#[test]
fn test_dns_error_display() {
    // Test all DnsError variants
    let cases = vec![
        (
            DnsError::Io("connection refused".to_string()),
            "IO error: connection refused",
        ),
        (
            DnsError::Parse("invalid DNS packet".to_string()),
            "Parse error: invalid DNS packet",
        ),
        (DnsError::Timeout, "Operation timed out"),
        (
            DnsError::Config(ConfigError::InvalidBindAddress("test".to_string())),
            "Configuration error: Invalid bind address: test",
        ),
        (
            DnsError::Cache("cache full".to_string()),
            "Cache error: cache full",
        ),
        (
            DnsError::RateLimit("limit exceeded".to_string()),
            "Rate limit error: limit exceeded",
        ),
        (
            DnsError::Redis("connection failed".to_string()),
            "Redis error: connection failed",
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn test_dns_error_source() {
    // Test source() method for DnsError
    let config_error = ConfigError::InvalidBindAddress("test".to_string());
    let dns_error = DnsError::Config(config_error.clone());

    // Config errors should have source
    assert!(dns_error.source().is_some());
    assert_eq!(
        dns_error.source().unwrap().to_string(),
        config_error.to_string()
    );

    // Other error types should not have source
    let io_error = DnsError::Io("test".to_string());
    assert!(io_error.source().is_none());

    let parse_error = DnsError::Parse("test".to_string());
    assert!(parse_error.source().is_none());

    let timeout_error = DnsError::Timeout;
    assert!(timeout_error.source().is_none());

    let cache_error = DnsError::Cache("test".to_string());
    assert!(cache_error.source().is_none());

    let rate_limit_error = DnsError::RateLimit("test".to_string());
    assert!(rate_limit_error.source().is_none());

    let redis_error = DnsError::Redis("test".to_string());
    assert!(redis_error.source().is_none());
}

#[test]
fn test_dns_error_from_io_error() {
    let io_error = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
    let dns_error: DnsError = io_error.into();

    match dns_error {
        DnsError::Io(msg) => assert!(msg.contains("connection refused")),
        _ => panic!("Expected DnsError::Io"),
    }
}

#[test]
fn test_dns_error_from_config_error() {
    let config_error = ConfigError::InvalidBindAddress("127.0.0.1:999999".to_string());
    let dns_error: DnsError = config_error.into();

    match dns_error {
        DnsError::Config(err) => {
            assert_eq!(err.to_string(), "Invalid bind address: 127.0.0.1:999999");
        }
        _ => panic!("Expected DnsError::Config"),
    }
}

#[test]
fn test_result_type_alias() {
    // Test that Result<T> works as expected
    fn success_fn() -> Result<String> {
        Ok("success".to_string())
    }

    fn error_fn() -> Result<String> {
        Err(DnsError::Timeout)
    }

    assert_eq!(success_fn().unwrap(), "success");
    assert!(error_fn().is_err());

    match error_fn() {
        Err(DnsError::Timeout) => (),
        _ => panic!("Expected timeout error"),
    }
}

#[test]
fn test_error_clone() {
    // Test that all error types implement Clone
    let config_error = ConfigError::InvalidBindAddress("test".to_string());
    let config_clone = config_error.clone();
    assert_eq!(config_error.to_string(), config_clone.to_string());

    let dns_error = DnsError::Parse("test".to_string());
    let dns_clone = dns_error.clone();
    assert_eq!(dns_error.to_string(), dns_clone.to_string());
}

#[test]
fn test_error_debug() {
    // Test Debug implementation
    let config_error = ConfigError::InvalidBindAddress("test".to_string());
    let debug_str = format!("{:?}", config_error);
    assert!(debug_str.contains("InvalidBindAddress"));
    assert!(debug_str.contains("test"));

    let dns_error = DnsError::Timeout;
    let debug_str = format!("{:?}", dns_error);
    assert!(debug_str.contains("Timeout"));
}

#[test]
fn test_nested_config_error() {
    // Test nested error handling
    let config_error = ConfigError::ParseError("malformed config".to_string());
    let dns_error = DnsError::from(config_error);

    // Check the full error chain
    assert_eq!(
        dns_error.to_string(),
        "Configuration error: Parse error: malformed config"
    );

    // Check source chain
    assert!(dns_error.source().is_some());
}
