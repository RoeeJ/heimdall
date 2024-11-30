use super::types::*;
use super::DnsWireFormat;
use super::{decode_domain_name, encode_domain_name};
use bitstream_io::{BitRead, BitReader, BigEndian};
use std::io::Cursor;

#[derive(Debug, Clone)]
pub struct DnsQuestion {
    pub name: String,
    pub qtype: DnsQType,
    pub qclass: DnsQClass,
}

impl DnsWireFormat for DnsQuestion {
    fn to_wire(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Encode the domain name
        bytes.extend(encode_domain_name(&self.name));
        
        // Add QTYPE (16 bits)
        bytes.extend_from_slice(&Into::<u16>::into(self.qtype).to_be_bytes());
        
        // Add QCLASS (16 bits)
        bytes.extend_from_slice(&Into::<u16>::into(self.qclass).to_be_bytes());
        
        bytes
    }

    fn from_wire(reader: &mut BitReader<Cursor<&[u8]>, BigEndian>) -> Result<Self, std::io::Error> {
        Ok(DnsQuestion {
            name: decode_domain_name(reader)?,
            qtype: DnsQType::from(reader.read::<u16>(16)?),
            qclass: DnsQClass::from(reader.read::<u16>(16)?),
        })
    }
} 