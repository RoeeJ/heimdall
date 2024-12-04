pub use crate::dns::{
    decode_domain_name,
    // Utility functions
    encode_domain_name,
    get_cache_key,
    get_cache_value,

    DnsAdditional,
    DnsAnswer,
    DnsAuthority,
    DnsHeader,
    DnsOpcode,
    // Core DNS types
    DnsPacket,
    DnsQClass,
    // Enums
    DnsQType,
    DnsQr,
    DnsQuestion,
    // Resolver
    DnsResolver,
    DnsResourceRecord,
    DnsResponseCode,
    // Traits
    DnsWireFormat,

    EdnsOptionCode,

    RData,
};

// Common external types
pub use bitstream_io::{BigEndian, BitRead, BitReader, BitWrite, BitWriter};
pub use std::io::{self, Cursor};
pub use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
