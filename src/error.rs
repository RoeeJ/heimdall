use thiserror::Error;

#[derive(Error, Debug)]
pub enum DnsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
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

pub type Result<T> = std::result::Result<T, DnsError>;