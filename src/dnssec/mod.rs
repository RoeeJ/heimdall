pub mod algorithm;
pub mod denial;
pub mod digest;
pub mod errors;
pub mod key_tag;
pub mod trust_anchor;
pub mod validator;

pub use algorithm::DnsSecAlgorithm;
pub use denial::DenialOfExistenceValidator;
pub use digest::DigestType;
pub use errors::DnsSecError;
pub use key_tag::calculate_key_tag;
pub use trust_anchor::{TrustAnchor, TrustAnchorStore};
pub use validator::DnsSecValidator;

/// DNSSEC validation result
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// The response is secure and validated
    Secure,
    /// The response is insecure (no DNSSEC records)
    Insecure,
    /// The response is bogus (validation failed)
    Bogus(String),
    /// Validation is indeterminate (missing data)
    Indeterminate,
}

/// DNSSEC constants
pub mod constants {
    /// DNS UDP payload size for DNSSEC (RFC 4035)
    pub const DNSSEC_UDP_SIZE: u16 = 4096;

    /// Maximum iterations for NSEC3 (RFC 5155)
    pub const MAX_NSEC3_ITERATIONS: u16 = 2500;

    /// Root trust anchor key tag (2024 KSK)
    pub const ROOT_KSK_KEY_TAG: u16 = 20326;

    /// DNSSEC OK flag for EDNS0
    pub const DO_FLAG: u16 = 0x8000;
}
