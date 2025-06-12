use std::fmt;

/// DNSSEC validation errors
#[derive(Debug, Clone, PartialEq)]
pub enum DnsSecError {
    /// No DNSKEY found for validation
    NoDnsKey,
    /// No DS record found at parent
    NoDs,
    /// No RRSIG found for RRset
    NoRrsig,
    /// Signature expired
    SignatureExpired,
    /// Signature not yet valid
    SignatureNotYetValid,
    /// Key tag mismatch
    KeyTagMismatch,
    /// Algorithm not supported
    UnsupportedAlgorithm(u8),
    /// Digest type not supported
    UnsupportedDigestType(u8),
    /// Signature verification failed
    SignatureVerificationFailed,
    /// DS digest mismatch
    DsDigestMismatch,
    /// Invalid public key format
    InvalidPublicKey,
    /// Invalid signature format
    InvalidSignature,
    /// NSEC/NSEC3 denial failed
    DenialOfExistenceFailed,
    /// Too many validation iterations
    TooManyIterations,
    /// Invalid NSEC3 parameters
    InvalidNsec3Parameters,
    /// Trust anchor not found
    TrustAnchorNotFound,
    /// Generic validation error
    ValidationError(String),
}

impl fmt::Display for DnsSecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDnsKey => write!(f, "No DNSKEY record found for validation"),
            Self::NoDs => write!(f, "No DS record found at parent zone"),
            Self::NoRrsig => write!(f, "No RRSIG record found for RRset"),
            Self::SignatureExpired => write!(f, "DNSSEC signature has expired"),
            Self::SignatureNotYetValid => write!(f, "DNSSEC signature is not yet valid"),
            Self::KeyTagMismatch => write!(f, "Key tag does not match"),
            Self::UnsupportedAlgorithm(alg) => write!(f, "Unsupported DNSSEC algorithm: {}", alg),
            Self::UnsupportedDigestType(digest) => write!(f, "Unsupported digest type: {}", digest),
            Self::SignatureVerificationFailed => write!(f, "DNSSEC signature verification failed"),
            Self::DsDigestMismatch => write!(f, "DS record digest does not match DNSKEY"),
            Self::InvalidPublicKey => write!(f, "Invalid DNSKEY public key format"),
            Self::InvalidSignature => write!(f, "Invalid RRSIG signature format"),
            Self::DenialOfExistenceFailed => {
                write!(f, "NSEC/NSEC3 denial of existence validation failed")
            }
            Self::TooManyIterations => write!(f, "Too many validation iterations"),
            Self::InvalidNsec3Parameters => write!(f, "Invalid NSEC3 parameters"),
            Self::TrustAnchorNotFound => write!(f, "Trust anchor not found for validation"),
            Self::ValidationError(msg) => write!(f, "DNSSEC validation error: {}", msg),
        }
    }
}

impl std::error::Error for DnsSecError {}

pub type Result<T> = std::result::Result<T, DnsSecError>;
