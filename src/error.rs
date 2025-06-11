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
    Io(String), // Changed from std::io::Error to String for Clone compatibility
    Parse(String),
    Timeout,
    Config(ConfigError),
    Cache(String),
    RateLimit(String),
    Redis(String),
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DnsError::Io(err) => write!(f, "IO error: {}", err),
            DnsError::Parse(msg) => write!(f, "Parse error: {}", msg),
            DnsError::Timeout => write!(f, "Operation timed out"),
            DnsError::Config(err) => write!(f, "Configuration error: {}", err),
            DnsError::Cache(msg) => write!(f, "Cache error: {}", msg),
            DnsError::RateLimit(msg) => write!(f, "Rate limit error: {}", msg),
            DnsError::Redis(msg) => write!(f, "Redis error: {}", msg),
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
        DnsError::Io(err.to_string())
    }
}

impl From<ConfigError> for DnsError {
    fn from(err: ConfigError) -> Self {
        DnsError::Config(err)
    }
}
