use std::fmt;

/// DS digest type algorithms (RFC 4034, 4509, 5155, 6605, 7344)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DigestType {
    /// Reserved
    Reserved = 0,
    /// SHA-1 (RFC 3658)
    Sha1 = 1,
    /// SHA-256 (RFC 4509)
    Sha256 = 2,
    /// GOST R 34.11-94 (RFC 5933)
    Gost94 = 3,
    /// SHA-384 (RFC 6605)
    Sha384 = 4,
}

impl DigestType {
    /// Create from digest type number
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Reserved),
            1 => Some(Self::Sha1),
            2 => Some(Self::Sha256),
            3 => Some(Self::Gost94),
            4 => Some(Self::Sha384),
            _ => None,
        }
    }
    
    /// Convert to digest type number
    pub fn to_u8(self) -> u8 {
        self as u8
    }
    
    /// Check if digest type is supported
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Sha1 | Self::Sha256 | Self::Sha384)
    }
    
    /// Check if digest type is recommended (RFC 8624)
    pub fn is_recommended(&self) -> bool {
        matches!(self, Self::Sha256)
    }
    
    /// Get the expected digest length in bytes
    pub fn digest_len(&self) -> usize {
        match self {
            Self::Reserved => 0,
            Self::Sha1 => 20,
            Self::Sha256 => 32,
            Self::Gost94 => 32,
            Self::Sha384 => 48,
        }
    }
    
    /// Calculate digest of data using this algorithm
    pub fn digest(&self, data: &[u8]) -> Option<Vec<u8>> {
        match self {
            Self::Sha1 => {
                use ring::digest;
                Some(digest::digest(&digest::SHA1_FOR_LEGACY_USE_ONLY, data).as_ref().to_vec())
            }
            Self::Sha256 => {
                use ring::digest;
                Some(digest::digest(&digest::SHA256, data).as_ref().to_vec())
            }
            Self::Sha384 => {
                use ring::digest;
                Some(digest::digest(&digest::SHA384, data).as_ref().to_vec())
            }
            _ => None,
        }
    }
}

impl fmt::Display for DigestType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reserved => write!(f, "RESERVED"),
            Self::Sha1 => write!(f, "SHA1"),
            Self::Sha256 => write!(f, "SHA256"),
            Self::Gost94 => write!(f, "GOST94"),
            Self::Sha384 => write!(f, "SHA384"),
        }
    }
}