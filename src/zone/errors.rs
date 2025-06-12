use std::fmt;

/// Zone-related errors
#[derive(Debug, Clone)]
pub enum ZoneError {
    /// Zone file parsing error
    ParseError(String),
    /// Invalid record format
    InvalidRecord(String),
    /// Missing SOA record
    MissingSOA,
    /// Duplicate SOA record
    DuplicateSOA,
    /// Invalid domain name
    InvalidDomainName(String),
    /// Zone not found
    ZoneNotFound(String),
    /// IO error
    IoError(String),
    /// Zone file too large
    FileTooLarge,
    /// Invalid TTL value
    InvalidTTL(String),
    /// Invalid resource record type
    InvalidRRType(String),
    /// Validation error
    ValidationError(String),
}

impl fmt::Display for ZoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "Zone parse error: {}", msg),
            Self::InvalidRecord(msg) => write!(f, "Invalid record: {}", msg),
            Self::MissingSOA => write!(f, "Zone missing required SOA record"),
            Self::DuplicateSOA => write!(f, "Zone contains duplicate SOA records"),
            Self::InvalidDomainName(name) => write!(f, "Invalid domain name: {}", name),
            Self::ZoneNotFound(zone) => write!(f, "Zone not found: {}", zone),
            Self::IoError(msg) => write!(f, "IO error: {}", msg),
            Self::FileTooLarge => write!(f, "Zone file exceeds maximum size"),
            Self::InvalidTTL(ttl) => write!(f, "Invalid TTL value: {}", ttl),
            Self::InvalidRRType(rtype) => write!(f, "Invalid resource record type: {}", rtype),
            Self::ValidationError(msg) => write!(f, "Zone validation error: {}", msg),
        }
    }
}

impl std::error::Error for ZoneError {}

pub type Result<T> = std::result::Result<T, ZoneError>;
