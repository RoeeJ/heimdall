use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DnsOpcode {
    Query = 0,
    IQuery = 1,
    Status = 2,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DnsResponseCode {
    NoError = 0,
    FormatError = 1,
    ServerFailure = 2,
    NameError = 3,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DnsQr {
    Query = 0,
    Response = 1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u16)]
pub enum DnsQType {
    A = 1,       // IPv4 address
    NS = 2,      // Nameserver
    MD = 3,      // Mail destination (Obsolete)
    MF = 4,      // Mail forwarder (Obsolete)
    CNAME = 5,   // Canonical name
    SOA = 6,     // Start of authority
    MB = 7,      // Mailbox domain name
    MG = 8,      // Mail group member
    MR = 9,      // Mail rename domain name
    NULL = 10,   // Null resource record
    WKS = 11,    // Well known service
    PTR = 12,    // Domain name pointer
    HINFO = 13,  // Host information
    MINFO = 14,  // Mailbox information
    MX = 15,     // Mail exchange
    TXT = 16,    // Text strings
    AAAA = 28,   // IPv6 address
    SRV = 33,    // Service locator
    AXFR = 252,  // Transfer of entire zone
    MAILB = 253, // Mailbox-related records
    MAILA = 254, // Mail agent RRs
    ANY = 255,   // All records
    OPT = 41,    // EDNS(0) OPT record
    Other(u16),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u16)]
pub enum DnsQClass {
    IN = 1,    // Internet
    CS = 2,    // CSNET (Obsolete)
    CH = 3,    // CHAOS
    HS = 4,    // Hesiod
    ANY = 255, // Any class
    Other(u16),
}

impl From<u8> for DnsOpcode {
    fn from(value: u8) -> Self {
        match value {
            0 => DnsOpcode::Query,
            1 => DnsOpcode::IQuery,
            2 => DnsOpcode::Status,
            _ => panic!("Invalid DnsOpcode value: {}", value),
        }
    }
}

impl From<DnsOpcode> for u8 {
    fn from(val: DnsOpcode) -> Self {
        val as u8
    }
}

impl From<u8> for DnsResponseCode {
    fn from(value: u8) -> Self {
        match value {
            0 => DnsResponseCode::NoError,
            1 => DnsResponseCode::FormatError,
            2 => DnsResponseCode::ServerFailure,
            3 => DnsResponseCode::NameError,
            _ => panic!("Invalid DnsResponseCode value: {}", value),
        }
    }
}

impl From<DnsResponseCode> for u8 {
    fn from(val: DnsResponseCode) -> Self {
        val as u8
    }
}

impl From<u8> for DnsQr {
    fn from(value: u8) -> Self {
        match value {
            0 => DnsQr::Query,
            1 => DnsQr::Response,
            _ => panic!("Invalid DnsQr value: {}", value),
        }
    }
}

impl From<DnsQr> for u8 {
    fn from(val: DnsQr) -> Self {
        val as u8
    }
}

impl From<u16> for DnsQType {
    fn from(value: u16) -> Self {
        match value {
            1 => DnsQType::A,
            2 => DnsQType::NS,
            3 => DnsQType::MD,
            4 => DnsQType::MF,
            5 => DnsQType::CNAME,
            6 => DnsQType::SOA,
            7 => DnsQType::MB,
            8 => DnsQType::MG,
            9 => DnsQType::MR,
            10 => DnsQType::NULL,
            11 => DnsQType::WKS,
            12 => DnsQType::PTR,
            13 => DnsQType::HINFO,
            14 => DnsQType::MINFO,
            15 => DnsQType::MX,
            16 => DnsQType::TXT,
            28 => DnsQType::AAAA,
            33 => DnsQType::SRV,
            252 => DnsQType::AXFR,
            253 => DnsQType::MAILB,
            254 => DnsQType::MAILA,
            255 => DnsQType::ANY,
            41 => DnsQType::OPT,
            _ => DnsQType::Other(value),
        }
    }
}

impl From<DnsQType> for u16 {
    fn from(val: DnsQType) -> Self {
        match val {
            DnsQType::A => 1,
            DnsQType::NS => 2,
            DnsQType::MD => 3,
            DnsQType::MF => 4,
            DnsQType::CNAME => 5,
            DnsQType::SOA => 6,
            DnsQType::MB => 7,
            DnsQType::MG => 8,
            DnsQType::MR => 9,
            DnsQType::NULL => 10,
            DnsQType::WKS => 11,
            DnsQType::PTR => 12,
            DnsQType::HINFO => 13,
            DnsQType::MINFO => 14,
            DnsQType::MX => 15,
            DnsQType::TXT => 16,
            DnsQType::AAAA => 28,
            DnsQType::SRV => 33,
            DnsQType::AXFR => 252,
            DnsQType::MAILB => 253,
            DnsQType::MAILA => 254,
            DnsQType::ANY => 255,
            DnsQType::OPT => 41,
            DnsQType::Other(value) => value,
        }
    }
}

impl From<u16> for DnsQClass {
    fn from(value: u16) -> Self {
        match value {
            1 => DnsQClass::IN,
            2 => DnsQClass::CS,
            3 => DnsQClass::CH,
            4 => DnsQClass::HS,
            255 => DnsQClass::ANY,
            _ => DnsQClass::Other(value),
        }
    }
}

impl From<DnsQClass> for u16 {
    fn from(val: DnsQClass) -> Self {
        match val {
            DnsQClass::IN => 1,
            DnsQClass::CS => 2,
            DnsQClass::CH => 3,
            DnsQClass::HS => 4,
            DnsQClass::ANY => 255,
            DnsQClass::Other(value) => value,
        }
    }
}
