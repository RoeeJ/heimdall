use std::sync::Arc;
use thiserror::Error;

/// Unified error type for the entire Heimdall DNS server
#[derive(Debug, Clone, Error)]
pub enum HeimdallError {
    // IO errors
    #[error("IO error: {0}")]
    Io(String),
    #[error("IO error: {0}")]
    IoError(Arc<std::io::Error>),

    // Configuration errors
    #[error("Invalid bind address: {0}")]
    InvalidBindAddress(String),
    #[error("Invalid upstream server: {0}")]
    InvalidUpstreamServer(String),
    #[error("Invalid HTTP bind address: {0}")]
    InvalidHttpBindAddress(String),
    #[error("Invalid worker threads: {0}")]
    InvalidWorkerThreads(String),
    #[error("Invalid cache size: {0}")]
    InvalidCacheSize(String),
    #[error("Invalid timeout: {0}")]
    InvalidTimeout(String),
    #[error("Invalid rate limit: {0}")]
    InvalidRateLimit(String),
    #[error("Configuration parse error: {0}")]
    ConfigParseError(String),

    // DNS parsing errors
    #[error("Invalid DNS header")]
    InvalidHeader,
    #[error("Invalid DNS label")]
    InvalidLabel,
    #[error("Invalid question section")]
    InvalidQuestionSection,
    #[error("Invalid answer section")]
    InvalidAnswerSection,
    #[error("Invalid authority section")]
    InvalidAuthoritySection,
    #[error("Invalid additional section")]
    InvalidAdditionalSection,
    #[error("Invalid bit stream: {0}")]
    InvalidBitStream(String),
    #[error("Parse error: {0}")]
    ParseError(String),

    // DNS operation errors
    #[error("Operation timed out")]
    Timeout,
    #[error("Cache error: {0}")]
    Cache(String),
    #[error("Redis error: {0}")]
    Redis(String),

    // Rate limiting errors
    #[error("Rate limit error: {0}")]
    RateLimit(String),
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    #[error("Too many concurrent requests")]
    TooManyRequests,

    // Server state errors
    #[error("Server is shutting down")]
    ServerShutdown,

    // Validation errors
    #[error("Validation error: {0}")]
    ValidationError(String),

    // DNSSEC errors
    #[error("No DNSKEY record found for validation")]
    NoDnsKey,
    #[error("No DS record found at parent zone")]
    NoDs,
    #[error("No RRSIG record found for RRset")]
    NoRrsig,
    #[error("DNSSEC signature has expired")]
    SignatureExpired,
    #[error("DNSSEC signature is not yet valid")]
    SignatureNotYetValid,
    #[error("Key tag does not match")]
    KeyTagMismatch,
    #[error("Unsupported DNSSEC algorithm: {0}")]
    UnsupportedAlgorithm(u8),
    #[error("Unsupported digest type: {0}")]
    UnsupportedDigestType(u8),
    #[error("DNSSEC signature verification failed")]
    SignatureVerificationFailed,
    #[error("DS record digest does not match DNSKEY")]
    DsDigestMismatch,
    #[error("Invalid DNSKEY public key format")]
    InvalidPublicKey,
    #[error("Invalid RRSIG signature format")]
    InvalidSignature,
    #[error("NSEC/NSEC3 denial of existence validation failed")]
    DenialOfExistenceFailed,
    #[error("Too many validation iterations")]
    TooManyIterations,
    #[error("Invalid NSEC3 parameters")]
    InvalidNsec3Parameters,
    #[error("Trust anchor not found for validation")]
    TrustAnchorNotFound,
    #[error("DNSSEC validation error: {0}")]
    DnsSecValidationError(String),

    // Zone errors
    #[error("Zone parse error: {0}")]
    ZoneParseError(String),
    #[error("Invalid record: {0}")]
    InvalidRecord(String),
    #[error("Zone missing required SOA record")]
    MissingSOA,
    #[error("Zone contains duplicate SOA records")]
    DuplicateSOA,
    #[error("Invalid domain name: {0}")]
    InvalidDomainName(String),
    #[error("Zone not found: {0}")]
    ZoneNotFound(String),
    #[error("Zone file exceeds maximum size")]
    ZoneFileTooLarge,
    #[error("Invalid TTL value: {0}")]
    InvalidTTL(String),
    #[error("Invalid resource record type: {0}")]
    InvalidRRType(String),
    #[error("Zone validation error: {0}")]
    ZoneValidationError(String),

    // TLS errors
    #[error("Failed to read certificate file: {0}")]
    CertificateRead(String),
    #[error("Failed to parse certificate: {0}")]
    CertificateParse(String),
    #[error("Failed to parse private key: {0}")]
    PrivateKeyParse(String),
    #[error("TLS configuration error: {0}")]
    TlsConfigError(String),
    #[error("No valid certificate found in file")]
    NoCertificate,
    #[error("No valid private key found in file")]
    NoPrivateKey,

    // Dynamic update errors
    #[error("Not authoritative: {0}")]
    NotAuth(String),
    #[error("Update refused: {0}")]
    Refused(String),
    #[error("TSIG verification failed: {0}")]
    NotVerified(String),
    #[error("Prerequisite failed: {0}")]
    PrereqFailed(String),
    #[error("Update failed: {0}")]
    UpdateFailed(String),
    #[error("Update server error: {0}")]
    UpdateServerError(String),
}

// Conversion from std::io::Error
impl From<std::io::Error> for HeimdallError {
    fn from(err: std::io::Error) -> Self {
        HeimdallError::IoError(Arc::new(err))
    }
}

// Result type alias
pub type Result<T> = std::result::Result<T, HeimdallError>;

// Conversion helpers for legacy error types
pub mod conversions {
    use super::*;

    // Convert from old ConfigError
    pub fn from_config_error(err: crate::error::ConfigError) -> HeimdallError {
        use crate::error::ConfigError;
        match err {
            ConfigError::InvalidBindAddress(s) => HeimdallError::InvalidBindAddress(s),
            ConfigError::InvalidUpstreamServer(s) => HeimdallError::InvalidUpstreamServer(s),
            ConfigError::InvalidHttpBindAddress(s) => HeimdallError::InvalidHttpBindAddress(s),
            ConfigError::InvalidWorkerThreads(s) => HeimdallError::InvalidWorkerThreads(s),
            ConfigError::InvalidCacheSize(s) => HeimdallError::InvalidCacheSize(s),
            ConfigError::InvalidTimeout(s) => HeimdallError::InvalidTimeout(s),
            ConfigError::InvalidRateLimit(s) => HeimdallError::InvalidRateLimit(s),
            ConfigError::ParseError(s) => HeimdallError::ConfigParseError(s),
        }
    }

    // Convert from old DnsError
    pub fn from_dns_error(err: crate::error::DnsError) -> HeimdallError {
        use crate::error::DnsError;
        match err {
            DnsError::Io(s) => HeimdallError::Io(s),
            DnsError::IoError(e) => HeimdallError::IoError(e),
            DnsError::Parse(s) => HeimdallError::ParseError(s),
            DnsError::ParseError(s) => HeimdallError::ParseError(s),
            DnsError::Timeout => HeimdallError::Timeout,
            DnsError::Config(e) => from_config_error(e),
            DnsError::Cache(s) => HeimdallError::Cache(s),
            DnsError::RateLimit(s) => HeimdallError::RateLimit(s),
            DnsError::RateLimitExceeded(s) => HeimdallError::RateLimitExceeded(s),
            DnsError::Redis(s) => HeimdallError::Redis(s),
            DnsError::TooManyRequests => HeimdallError::TooManyRequests,
            DnsError::ServerShutdown => HeimdallError::ServerShutdown,
            DnsError::ValidationError(s) => HeimdallError::ValidationError(s),
        }
    }

    // Convert from ParseError
    pub fn from_parse_error(err: crate::dns::ParseError) -> HeimdallError {
        use crate::dns::ParseError;
        match err {
            ParseError::InvalidHeader => HeimdallError::InvalidHeader,
            ParseError::InvalidLabel => HeimdallError::InvalidLabel,
            ParseError::InvalidQuestionSection => HeimdallError::InvalidQuestionSection,
            ParseError::InvalidAnswerSection => HeimdallError::InvalidAnswerSection,
            ParseError::InvalidAuthoritySection => HeimdallError::InvalidAuthoritySection,
            ParseError::InvalidAdditionalSection => HeimdallError::InvalidAdditionalSection,
            ParseError::InvalidBitStream(s) => HeimdallError::InvalidBitStream(s),
        }
    }

    // Convert from DnsSecError
    pub fn from_dnssec_error(err: crate::dnssec::errors::DnsSecError) -> HeimdallError {
        use crate::dnssec::errors::DnsSecError;
        match err {
            DnsSecError::NoDnsKey => HeimdallError::NoDnsKey,
            DnsSecError::NoDs => HeimdallError::NoDs,
            DnsSecError::NoRrsig => HeimdallError::NoRrsig,
            DnsSecError::SignatureExpired => HeimdallError::SignatureExpired,
            DnsSecError::SignatureNotYetValid => HeimdallError::SignatureNotYetValid,
            DnsSecError::KeyTagMismatch => HeimdallError::KeyTagMismatch,
            DnsSecError::UnsupportedAlgorithm(a) => HeimdallError::UnsupportedAlgorithm(a),
            DnsSecError::UnsupportedDigestType(d) => HeimdallError::UnsupportedDigestType(d),
            DnsSecError::SignatureVerificationFailed => HeimdallError::SignatureVerificationFailed,
            DnsSecError::DsDigestMismatch => HeimdallError::DsDigestMismatch,
            DnsSecError::InvalidPublicKey => HeimdallError::InvalidPublicKey,
            DnsSecError::InvalidSignature => HeimdallError::InvalidSignature,
            DnsSecError::DenialOfExistenceFailed => HeimdallError::DenialOfExistenceFailed,
            DnsSecError::TooManyIterations => HeimdallError::TooManyIterations,
            DnsSecError::InvalidNsec3Parameters => HeimdallError::InvalidNsec3Parameters,
            DnsSecError::TrustAnchorNotFound => HeimdallError::TrustAnchorNotFound,
            DnsSecError::ValidationError(s) => HeimdallError::DnsSecValidationError(s),
        }
    }

    // Convert from ZoneError
    pub fn from_zone_error(err: crate::zone::errors::ZoneError) -> HeimdallError {
        use crate::zone::errors::ZoneError;
        match err {
            ZoneError::ParseError(s) => HeimdallError::ZoneParseError(s),
            ZoneError::InvalidRecord(s) => HeimdallError::InvalidRecord(s),
            ZoneError::MissingSOA => HeimdallError::MissingSOA,
            ZoneError::DuplicateSOA => HeimdallError::DuplicateSOA,
            ZoneError::InvalidDomainName(s) => HeimdallError::InvalidDomainName(s),
            ZoneError::ZoneNotFound(s) => HeimdallError::ZoneNotFound(s),
            ZoneError::IoError(s) => HeimdallError::Io(s),
            ZoneError::FileTooLarge => HeimdallError::ZoneFileTooLarge,
            ZoneError::InvalidTTL(s) => HeimdallError::InvalidTTL(s),
            ZoneError::InvalidRRType(s) => HeimdallError::InvalidRRType(s),
            ZoneError::ValidationError(s) => HeimdallError::ZoneValidationError(s),
        }
    }

    // Convert from UpdateError
    pub fn from_update_error(err: crate::dynamic_update::UpdateError) -> HeimdallError {
        use crate::dynamic_update::UpdateError;
        match err {
            UpdateError::NotAuth(s) => HeimdallError::NotAuth(s),
            UpdateError::Refused(s) => HeimdallError::Refused(s),
            UpdateError::NotVerified(s) => HeimdallError::NotVerified(s),
            UpdateError::PrereqFailed(s) => HeimdallError::PrereqFailed(s),
            UpdateError::UpdateFailed(s) => HeimdallError::UpdateFailed(s),
            UpdateError::ServerError(s) => HeimdallError::UpdateServerError(s),
        }
    }

    // Convert from TlsError (if needed)
    pub fn from_tls_error(err: crate::transport::tls::TlsError) -> HeimdallError {
        use crate::transport::tls::TlsError;
        match err {
            TlsError::CertificateRead(e) => HeimdallError::CertificateRead(e.to_string()),
            TlsError::CertificateParse(s) => HeimdallError::CertificateParse(s),
            TlsError::PrivateKeyParse(s) => HeimdallError::PrivateKeyParse(s),
            TlsError::ConfigError(e) => HeimdallError::TlsConfigError(e.to_string()),
            TlsError::NoCertificate => HeimdallError::NoCertificate,
            TlsError::NoPrivateKey => HeimdallError::NoPrivateKey,
        }
    }
}
