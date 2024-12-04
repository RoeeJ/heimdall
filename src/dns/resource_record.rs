use bitstream_io::{BigEndian, BitRead, BitReader};
use serde::{Deserialize, Serialize};

use crate::constants::SERVER_COOKIE;

use super::{decode_domain_name, encode_domain_name, types::*, DnsWireFormat};
use std::{
    io::Cursor,
    net::{Ipv4Addr, Ipv6Addr},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RData {
    A(Ipv4Addr),
    AAAA(Ipv6Addr),
    NS(String),
    CNAME(String),
    PTR(String),
    MX {
        preference: u16,
        exchange: String,
    },
    TXT(Vec<String>),
    SOA {
        mname: String,
        rname: String,
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
    },
    SRV {
        priority: u16,
        weight: u16,
        port: u16,
        target: String,
    },
    Unknown(Vec<u8>),
    OPT {
        udp_payload_size: u16,
        extended_rcode: u8,
        version: u8,
        dnssec_ok: bool,
        options: Vec<EdnsOption>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsResourceRecord {
    pub name: String,
    pub qtype: DnsQType,
    pub qclass: DnsQClass,
    pub ttl: u32,
    pub length: u16,
    pub rdata: RData,
}

// Type aliases for different RR sections
pub type DnsAnswer = DnsResourceRecord;
pub type DnsAuthority = DnsResourceRecord;
pub type DnsAdditional = DnsResourceRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(u16)]
pub enum EdnsOptionCode {
    Cookie = 10,
    Other(u16),
}

impl From<u16> for EdnsOptionCode {
    fn from(value: u16) -> Self {
        match value {
            10 => EdnsOptionCode::Cookie,
            other => EdnsOptionCode::Other(other),
        }
    }
}

impl Into<u16> for EdnsOptionCode {
    fn into(self) -> u16 {
        match self {
            EdnsOptionCode::Cookie => 10,
            EdnsOptionCode::Other(code) => code,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdnsOption {
    pub code: EdnsOptionCode,
    pub data: Vec<u8>,
}

impl RData {
    // Add a new method to parse RData based on record type
    fn from_wire_with_type(
        reader: &mut BitReader<Cursor<&[u8]>, BigEndian>,
        record_type: DnsQType,
        length: u16,
    ) -> Result<Self, std::io::Error> {
        match record_type {
            DnsQType::A => {
                let mut octets = [0u8; 4];
                for i in 0..4 {
                    octets[i] = reader.read::<u8>(8)?;
                }
                Ok(RData::A(Ipv4Addr::from(octets)))
            }
            DnsQType::AAAA => {
                let mut octets = [0u8; 16];
                for i in 0..16 {
                    octets[i] = reader.read::<u8>(8)?;
                }
                Ok(RData::AAAA(Ipv6Addr::from(octets)))
            }
            DnsQType::NS | DnsQType::CNAME | DnsQType::PTR => {
                let name = decode_domain_name(reader)?;
                match record_type {
                    DnsQType::NS => Ok(RData::NS(name)),
                    DnsQType::CNAME => Ok(RData::CNAME(name)),
                    DnsQType::PTR => Ok(RData::PTR(name)),
                    _ => unreachable!(),
                }
            }
            DnsQType::MX => {
                let preference = reader.read::<u16>(16)?;
                let exchange = decode_domain_name(reader)?;
                Ok(RData::MX {
                    preference,
                    exchange,
                })
            }
            DnsQType::TXT => {
                let mut strings = Vec::new();
                let mut bytes_read = 0;
                while bytes_read < length {
                    let str_len = reader.read::<u8>(8)? as u16;
                    bytes_read += 1;
                    let mut string_bytes = Vec::new();
                    for _ in 0..str_len {
                        string_bytes.push(reader.read::<u8>(8)?);
                        bytes_read += 1;
                    }
                    strings.push(String::from_utf8_lossy(&string_bytes).to_string());
                }
                Ok(RData::TXT(strings))
            }
            DnsQType::SOA => {
                let mname = decode_domain_name(reader)?;
                let rname = decode_domain_name(reader)?;
                let serial = reader.read::<u32>(32)?;
                let refresh = reader.read::<u32>(32)?;
                let retry = reader.read::<u32>(32)?;
                let expire = reader.read::<u32>(32)?;
                let minimum = reader.read::<u32>(32)?;
                Ok(RData::SOA {
                    mname,
                    rname,
                    serial,
                    refresh,
                    retry,
                    expire,
                    minimum,
                })
            }
            DnsQType::SRV => {
                let priority = reader.read::<u16>(16)?;
                let weight = reader.read::<u16>(16)?;
                let port = reader.read::<u16>(16)?;
                let target = decode_domain_name(reader)?;
                Ok(RData::SRV {
                    priority,
                    weight,
                    port,
                    target,
                })
            }
            DnsQType::OPT => {
                let mut options = Vec::new();
                let mut bytes_read = 0;

                while bytes_read < length {
                    let option_code = reader.read::<u16>(16)?;
                    let option_len = reader.read::<u16>(16)?;
                    bytes_read += 4;

                    let mut option_data = Vec::new();
                    for _ in 0..option_len {
                        option_data.push(reader.read::<u8>(8)?);
                        bytes_read += 1;
                    }

                    options.push(EdnsOption {
                        code: EdnsOptionCode::from(option_code),
                        data: option_data,
                    });
                }

                Ok(RData::OPT {
                    udp_payload_size: 0,
                    extended_rcode: 0,
                    version: 0,
                    dnssec_ok: false,
                    options,
                })
            }
            _ => {
                let mut data = Vec::new();
                for _ in 0..length {
                    data.push(reader.read::<u8>(8)?);
                }
                Ok(RData::Unknown(data))
            }
        }
    }
}

impl DnsWireFormat for RData {
    fn to_wire(&self) -> Vec<u8> {
        match self {
            RData::A(addr) => addr.octets().to_vec(),
            RData::AAAA(addr) => addr.octets().to_vec(),
            RData::NS(name) | RData::CNAME(name) | RData::PTR(name) => encode_domain_name(name),
            RData::MX {
                preference,
                exchange,
            } => {
                let mut bytes = Vec::new();
                bytes.extend_from_slice(&preference.to_be_bytes());
                bytes.extend(encode_domain_name(exchange));
                bytes
            }
            RData::TXT(strings) => {
                let mut bytes = Vec::new();
                for s in strings {
                    let bytes_str = s.as_bytes();
                    if bytes_str.len() > 255 {
                        continue;
                    }
                    bytes.push(bytes_str.len() as u8);
                    bytes.extend(bytes_str);
                }
                bytes
            }
            RData::SOA {
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
            } => {
                let mut bytes = Vec::new();
                bytes.extend(encode_domain_name(mname));
                bytes.extend(encode_domain_name(rname));
                bytes.extend_from_slice(&serial.to_be_bytes());
                bytes.extend_from_slice(&refresh.to_be_bytes());
                bytes.extend_from_slice(&retry.to_be_bytes());
                bytes.extend_from_slice(&expire.to_be_bytes());
                bytes.extend_from_slice(&minimum.to_be_bytes());
                bytes
            }
            RData::SRV {
                priority,
                weight,
                port,
                target,
            } => {
                let mut bytes = Vec::new();
                bytes.extend_from_slice(&priority.to_be_bytes());
                bytes.extend_from_slice(&weight.to_be_bytes());
                bytes.extend_from_slice(&port.to_be_bytes());
                bytes.extend(encode_domain_name(target));
                bytes
            }
            RData::Unknown(data) => data.clone(),
            RData::OPT { options, .. } => {
                let mut bytes = Vec::new();
                for option in options {
                    let code: u16 = option.code.clone().into();
                    bytes.extend_from_slice(&code.to_be_bytes());
                    let option_len = option.data.len() as u16;
                    bytes.extend_from_slice(&option_len.to_be_bytes());
                    bytes.extend(&option.data);
                }
                bytes
            }
        }
    }

    fn from_wire(
        _reader: &mut BitReader<Cursor<&[u8]>, BigEndian>,
    ) -> Result<Self, std::io::Error> {
        // This should not be called directly - use from_wire_with_type instead
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "RData::from_wire called without record type",
        ))
    }
}

impl DnsWireFormat for DnsResourceRecord {
    fn to_wire(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Encode name
        if let DnsQType::OPT = self.qtype {
            // OPT record name must be a single zero byte
            bytes.push(0);
        } else {
            bytes.extend(encode_domain_name(&self.name));
        }

        // Type and Class
        bytes.extend_from_slice(&Into::<u16>::into(self.qtype).to_be_bytes());
        bytes.extend_from_slice(&Into::<u16>::into(self.qclass).to_be_bytes());

        // TTL
        bytes.extend_from_slice(&self.ttl.to_be_bytes());

        // Get RDATA first to calculate length
        let rdata = self.rdata.to_wire();
        bytes.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        bytes.extend(rdata);

        bytes
    }

    fn from_wire(reader: &mut BitReader<Cursor<&[u8]>, BigEndian>) -> Result<Self, std::io::Error> {
        let name = decode_domain_name(reader)?;
        let qtype = DnsQType::from(reader.read::<u16>(16)?);
        let qclass = DnsQClass::from(reader.read::<u16>(16)?);
        let ttl = reader.read::<u32>(32)?;
        let length = reader.read::<u16>(16)?;

        // Parse RDATA using the record type
        let mut rdata = RData::from_wire_with_type(reader, qtype, length)?;

        // Special handling for OPT records
        if let DnsQType::OPT = qtype {
            if let RData::OPT {
                ref mut udp_payload_size,
                ref mut extended_rcode,
                ref mut version,
                ref mut dnssec_ok,
                ..
            } = rdata
            {
                // CLASS field is UDP payload size for OPT records
                match qclass {
                    DnsQClass::Other(size) => *udp_payload_size = size,
                    _ => *udp_payload_size = 512, // Default value
                }

                // Extract fields from TTL
                *extended_rcode = ((ttl >> 24) & 0xFF) as u8;
                *version = ((ttl >> 16) & 0xFF) as u8;
                *dnssec_ok = (ttl & (1 << 15)) != 0;
            }
        }

        Ok(DnsResourceRecord {
            name,
            qtype,
            qclass,
            ttl,
            length,
            rdata,
        })
    }
}

impl DnsResourceRecord {
    pub fn new_opt_with_cookie(
        udp_payload_size: u16,
        extended_rcode: u8,
        version: u8,
        dnssec_ok: bool,
        client_cookie: &[u8],
    ) -> Self {
        let mut options = Vec::new();

        // Create cookie option - only include client cookie (8 bytes)
        let mut cookie_data = Vec::with_capacity(16);
        cookie_data.extend_from_slice(client_cookie); // Client cookie (8 bytes)
        cookie_data.extend_from_slice(&SERVER_COOKIE); // Server cookie (8 bytes)

        options.push(EdnsOption {
            code: EdnsOptionCode::Cookie,
            data: cookie_data,
        });

        let rdata = RData::OPT {
            udp_payload_size,
            extended_rcode,
            version,
            dnssec_ok,
            options,
        };

        // TTL field for OPT records:
        // - Extended RCODE (top 8 bits)
        // - Version (8 bits)
        // - DO bit (bit 15)
        // - Rest must be zero
        let ttl = ((extended_rcode as u32) << 24)  // Extended RCODE in highest byte
            | ((version as u32) << 16)             // Version in next byte
            | (if dnssec_ok { 1 } else { 0 } << 15) // DO bit
            | 0; // Rest must be zero

        DnsResourceRecord {
            name: String::from("."),
            qtype: DnsQType::OPT,
            qclass: DnsQClass::from(udp_payload_size), // CLASS field holds UDP payload size
            ttl,
            length: 0, // Will be calculated from RDATA
            rdata,
        }
    }

    pub fn new_opt(
        udp_payload_size: u16,
        extended_rcode: u8,
        version: u8,
        dnssec_ok: bool,
    ) -> Self {
        let rdata = RData::OPT {
            udp_payload_size,
            extended_rcode,
            version,
            dnssec_ok,
            options: Vec::new(),
        };

        let wire_rdata = rdata.to_wire();
        let length = wire_rdata.len() as u16;

        let ttl = ((extended_rcode as u32) << 24)
            | ((version as u32) << 16)
            | (if dnssec_ok { 1 } else { 0 } << 15)
            | 0;

        DnsResourceRecord {
            name: String::from("."),
            qtype: DnsQType::OPT,
            qclass: DnsQClass::from(udp_payload_size),
            ttl,
            length,
            rdata,
        }
    }
}
