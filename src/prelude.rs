pub use crate::dns::{
    // Core DNS types
    DnsPacket,
    DnsHeader,
    DnsQuestion,
    DnsAnswer,
    DnsAuthority,
    DnsAdditional,
    DnsResourceRecord,
    RData,
    
    // Enums
    DnsQType,
    DnsQClass,
    DnsQr,
    DnsOpcode,
    DnsResponseCode,
    EdnsOptionCode,
    
    // Traits
    DnsWireFormat,
    
    // Utility functions
    encode_domain_name,
    decode_domain_name,
    
    // Resolver
    DnsResolver,
};

// Common external types
pub use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
pub use std::io::{self, Cursor};
pub use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, BigEndian}; 