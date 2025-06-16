use std::error::Error as StdError;
use std::fmt;

pub type Result<T> = std::result::Result<T, DnsError>;

#[derive(Debug, Clone)]
pub enum ConfigError {
    InvalidBindAddress(String),
    InvalidUpstreamServer(String),
    InvalidHttpBindAddress(String),
    InvalidWorkerThreads(String),
    InvalidCacheSize(String),
    InvalidTimeout(String),
    InvalidRateLimit(String),
    ParseError(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::InvalidBindAddress(addr) => write!(f, "Invalid bind address: {}", addr),
            ConfigError::InvalidUpstreamServer(server) => {
                write!(f, "Invalid upstream server: {}", server)
            }
            ConfigError::InvalidHttpBindAddress(addr) => {
                write!(f, "Invalid HTTP bind address: {}", addr)
            }
            ConfigError::InvalidWorkerThreads(threads) => {
                write!(f, "Invalid worker threads: {}", threads)
            }
            ConfigError::InvalidCacheSize(size) => write!(f, "Invalid cache size: {}", size),
            ConfigError::InvalidTimeout(timeout) => write!(f, "Invalid timeout: {}", timeout),
            ConfigError::InvalidRateLimit(limit) => write!(f, "Invalid rate limit: {}", limit),
            ConfigError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl StdError for ConfigError {}

#[derive(Debug, Clone)]
pub enum DnsError {
    Io(String),                              // Generic IO error message
    IoError(std::sync::Arc<std::io::Error>), // Preserved IO error for better context
    Parse(String),
    ParseError(String), // Alternative name for compatibility
    Timeout,
    Config(ConfigError),
    Cache(String),
    RateLimit(String),
    RateLimitExceeded(String), // More specific rate limit error
    Redis(String),
    TooManyRequests,         // When semaphore/connection limit reached
    ServerShutdown,          // When server is shutting down
    ValidationError(String), // Query validation errors
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DnsError::Io(msg) => write!(f, "IO error: {}", msg),
            DnsError::IoError(err) => write!(f, "IO error: {}", err),
            DnsError::Parse(msg) => write!(f, "Parse error: {}", msg),
            DnsError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            DnsError::Timeout => write!(f, "Operation timed out"),
            DnsError::Config(err) => write!(f, "Configuration error: {}", err),
            DnsError::Cache(msg) => write!(f, "Cache error: {}", msg),
            DnsError::RateLimit(msg) => write!(f, "Rate limit error: {}", msg),
            DnsError::RateLimitExceeded(msg) => write!(f, "Rate limit exceeded: {}", msg),
            DnsError::Redis(msg) => write!(f, "Redis error: {}", msg),
            DnsError::TooManyRequests => write!(f, "Too many concurrent requests"),
            DnsError::ServerShutdown => write!(f, "Server is shutting down"),
            DnsError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl StdError for DnsError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            DnsError::Config(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DnsError {
    fn from(err: std::io::Error) -> Self {
        DnsError::IoError(std::sync::Arc::new(err))
    }
}

impl From<ConfigError> for DnsError {
    fn from(err: ConfigError) -> Self {
        DnsError::Config(err)
    }
}
