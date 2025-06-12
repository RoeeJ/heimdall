use std::fmt;

/// DNSSEC Algorithm numbers (RFC 4034, 5155, 5702, 5933, 6605, 7344, 8080, 8624)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DnsSecAlgorithm {
    /// Delete DS (RFC 8078)
    DeleteDS = 0,
    /// RSA/MD5 (deprecated)
    RsaMd5 = 1,
    /// Diffie-Hellman (deprecated)
    DH = 2,
    /// DSA/SHA1 (RFC 2536)
    DSA = 3,
    /// Reserved
    Reserved4 = 4,
    /// RSA/SHA-1 (RFC 3110)
    RsaSha1 = 5,
    /// DSA-NSEC3-SHA1 (RFC 5155)
    DsaNsec3Sha1 = 6,
    /// RSASHA1-NSEC3-SHA1 (RFC 5155)
    RsaSha1Nsec3Sha1 = 7,
    /// RSA/SHA-256 (RFC 5702)
    RsaSha256 = 8,
    /// Reserved
    Reserved9 = 9,
    /// RSA/SHA-512 (RFC 5702)
    RsaSha512 = 10,
    /// Reserved
    Reserved11 = 11,
    /// GOST R 34.10-2001 (RFC 5933)
    EccGost = 12,
    /// ECDSA Curve P-256 with SHA-256 (RFC 6605)
    EcdsaP256Sha256 = 13,
    /// ECDSA Curve P-384 with SHA-384 (RFC 6605)
    EcdsaP384Sha384 = 14,
    /// Ed25519 (RFC 8080)
    Ed25519 = 15,
    /// Ed448 (RFC 8080)
    Ed448 = 16,
    /// Indirect (RFC 4034)
    Indirect = 252,
    /// Private algorithm (RFC 4034)
    PrivateDNS = 253,
    /// Private algorithm OID (RFC 4034)
    PrivateOID = 254,
    /// Reserved
    Reserved255 = 255,
}

impl DnsSecAlgorithm {
    /// Create from algorithm number
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::DeleteDS),
            1 => Some(Self::RsaMd5),
            2 => Some(Self::DH),
            3 => Some(Self::DSA),
            4 => Some(Self::Reserved4),
            5 => Some(Self::RsaSha1),
            6 => Some(Self::DsaNsec3Sha1),
            7 => Some(Self::RsaSha1Nsec3Sha1),
            8 => Some(Self::RsaSha256),
            9 => Some(Self::Reserved9),
            10 => Some(Self::RsaSha512),
            11 => Some(Self::Reserved11),
            12 => Some(Self::EccGost),
            13 => Some(Self::EcdsaP256Sha256),
            14 => Some(Self::EcdsaP384Sha384),
            15 => Some(Self::Ed25519),
            16 => Some(Self::Ed448),
            252 => Some(Self::Indirect),
            253 => Some(Self::PrivateDNS),
            254 => Some(Self::PrivateOID),
            255 => Some(Self::Reserved255),
            _ => None,
        }
    }
    
    /// Convert to algorithm number
    pub fn to_u8(self) -> u8 {
        self as u8
    }
    
    /// Check if algorithm is supported for validation
    pub fn is_supported(&self) -> bool {
        matches!(self,
            Self::RsaSha1 |
            Self::RsaSha256 |
            Self::RsaSha512 |
            Self::EcdsaP256Sha256 |
            Self::EcdsaP384Sha384 |
            Self::Ed25519
        )
    }
    
    /// Check if algorithm is recommended (RFC 8624)
    pub fn is_recommended(&self) -> bool {
        matches!(self,
            Self::RsaSha256 |
            Self::EcdsaP256Sha256 |
            Self::Ed25519
        )
    }
    
    /// Get the signature algorithm name for ring
    pub fn ring_algorithm(&self) -> Option<&'static dyn ring::signature::VerificationAlgorithm> {
        match self {
            Self::RsaSha1 => Some(&ring::signature::RSA_PKCS1_2048_8192_SHA1_FOR_LEGACY_USE_ONLY),
            Self::RsaSha256 => Some(&ring::signature::RSA_PKCS1_2048_8192_SHA256),
            Self::RsaSha512 => Some(&ring::signature::RSA_PKCS1_2048_8192_SHA512),
            Self::EcdsaP256Sha256 => Some(&ring::signature::ECDSA_P256_SHA256_ASN1),
            Self::EcdsaP384Sha384 => Some(&ring::signature::ECDSA_P384_SHA384_ASN1),
            Self::Ed25519 => Some(&ring::signature::ED25519),
            _ => None,
        }
    }
}

impl fmt::Display for DnsSecAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeleteDS => write!(f, "DELETE"),
            Self::RsaMd5 => write!(f, "RSAMD5"),
            Self::DH => write!(f, "DH"),
            Self::DSA => write!(f, "DSA"),
            Self::Reserved4 => write!(f, "RESERVED4"),
            Self::RsaSha1 => write!(f, "RSASHA1"),
            Self::DsaNsec3Sha1 => write!(f, "DSA-NSEC3-SHA1"),
            Self::RsaSha1Nsec3Sha1 => write!(f, "RSASHA1-NSEC3-SHA1"),
            Self::RsaSha256 => write!(f, "RSASHA256"),
            Self::Reserved9 => write!(f, "RESERVED9"),
            Self::RsaSha512 => write!(f, "RSASHA512"),
            Self::Reserved11 => write!(f, "RESERVED11"),
            Self::EccGost => write!(f, "ECC-GOST"),
            Self::EcdsaP256Sha256 => write!(f, "ECDSAP256SHA256"),
            Self::EcdsaP384Sha384 => write!(f, "ECDSAP384SHA384"),
            Self::Ed25519 => write!(f, "ED25519"),
            Self::Ed448 => write!(f, "ED448"),
            Self::Indirect => write!(f, "INDIRECT"),
            Self::PrivateDNS => write!(f, "PRIVATEDNS"),
            Self::PrivateOID => write!(f, "PRIVATEOID"),
            Self::Reserved255 => write!(f, "RESERVED255"),
        }
    }
}