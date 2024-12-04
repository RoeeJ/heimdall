use super::types::*;
use super::DnsWireFormat;
use bitstream_io::{BigEndian, BitRead, BitReader};
use std::io::Cursor;

#[derive(Debug, Clone)]
pub struct DnsHeader {
    pub id: u16,
    pub qr: DnsQr,
    pub opcode: DnsOpcode,
    pub aa: u8,
    pub tc: u8,
    pub rd: u8,
    pub ra: u8,
    pub z: u8,
    pub rcode: DnsResponseCode,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

impl Default for DnsHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl DnsHeader {
    pub fn new() -> Self {
        DnsHeader {
            id: 0,
            qr: DnsQr::Query,
            opcode: DnsOpcode::Query,
            aa: 0,
            tc: 0,
            rd: 0,
            ra: 0,
            z: 0,
            rcode: DnsResponseCode::NoError,
            qdcount: 0,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        }
    }
}

impl DnsWireFormat for DnsHeader {
    fn to_wire(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(12);

        // First 16 bits: ID
        bytes.extend_from_slice(&self.id.to_be_bytes());

        // Second 16 bits: Various flags
        let flags: u16 = (Into::<u8>::into(self.qr) as u16) << 15
            | (Into::<u8>::into(self.opcode) as u16) << 11
            | (self.aa as u16) << 10
            | (self.tc as u16) << 9
            | (self.rd as u16) << 8
            | (self.ra as u16) << 7
            | (self.z as u16) << 4
            | Into::<u8>::into(self.rcode) as u16;

        bytes.extend_from_slice(&flags.to_be_bytes());

        // Add count fields
        bytes.extend_from_slice(&self.qdcount.to_be_bytes());
        bytes.extend_from_slice(&self.ancount.to_be_bytes());
        bytes.extend_from_slice(&self.nscount.to_be_bytes());
        bytes.extend_from_slice(&self.arcount.to_be_bytes());

        bytes
    }

    fn from_wire(reader: &mut BitReader<Cursor<&[u8]>, BigEndian>) -> Result<Self, std::io::Error> {
        Ok(DnsHeader {
            id: reader.read::<u16>(16)?,
            qr: DnsQr::from(reader.read::<u8>(1)?),
            opcode: DnsOpcode::from(reader.read::<u8>(4)?),
            aa: reader.read::<u8>(1)?,
            tc: reader.read::<u8>(1)?,
            rd: reader.read::<u8>(1)?,
            ra: reader.read::<u8>(1)?,
            z: reader.read::<u8>(3)?,
            rcode: DnsResponseCode::from(reader.read::<u8>(4)?),
            qdcount: reader.read::<u16>(16)?,
            ancount: reader.read::<u16>(16)?,
            nscount: reader.read::<u16>(16)?,
            arcount: reader.read::<u16>(16)?,
        })
    }
}
