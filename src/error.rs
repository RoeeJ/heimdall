use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum DnsError {
    #[error("IO error: {0}")]
    Io(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Invalid DNS packet: {0}")]
    InvalidPacket(String),

    #[error("Buffer too small: need {need} bytes, have {have} bytes")]
    BufferTooSmall { need: usize, have: usize },

    #[error("Invalid label length: {0}")]
    InvalidLabelLength(u8),

    #[error("DNS name too long")]
    NameTooLong,

    #[error("Too many labels in DNS name")]
    TooManyLabels,

    #[error("Invalid DNS header")]
    InvalidHeader,

    #[error("Unsupported DNS feature: {0}")]
    Unsupported(String),
}

impl From<std::io::Error> for DnsError {
    fn from(err: std::io::Error) -> Self {
        DnsError::Io(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, DnsError>;
