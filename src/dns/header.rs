use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, Endianness};

use super::{ParseError, common::PacketComponent};

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub struct DNSHeader {
    pub id: u16,
    pub qr: bool,
    pub opcode: u8,
    pub aa: bool,
    pub tc: bool,
    pub rd: bool,
    pub ra: bool,
    pub z: u8,
    pub rcode: u8,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

impl PacketComponent for DNSHeader {
    fn write<E: Endianness>(
        &self,
        writer: &mut BitWriter<&mut Vec<u8>, E>,
    ) -> Result<(), ParseError> {
        writer.write_var::<u16>(16, self.id)?;
        writer.write_var::<u8>(1, self.qr as u8)?;
        writer.write_var::<u8>(4, self.opcode)?;
        writer.write_var::<u8>(1, self.aa as u8)?;
        writer.write_var::<u8>(1, self.tc as u8)?;
        writer.write_var::<u8>(1, self.rd as u8)?;
        writer.write_var::<u8>(1, self.ra as u8)?;
        writer.write_var::<u8>(3, self.z)?;
        writer.write_var::<u8>(4, self.rcode)?;
        writer.write_var::<u16>(16, self.qdcount)?;
        writer.write_var::<u16>(16, self.ancount)?;
        writer.write_var::<u16>(16, self.nscount)?;
        writer.write_var::<u16>(16, self.arcount)?;
        Ok(())
    }

    fn read<E: Endianness>(&mut self, reader: &mut BitReader<&[u8], E>) -> Result<(), ParseError> {
        self.id = reader.read_var::<u16>(16)?;
        self.qr = reader.read_var::<u8>(1)? == 1;
        self.opcode = reader.read_var::<u8>(4)?;
        self.aa = reader.read_var::<u8>(1)? == 1;
        self.tc = reader.read_var::<u8>(1)? == 1;
        self.rd = reader.read_var::<u8>(1)? == 1;
        self.ra = reader.read_var::<u8>(1)? == 1;
        self.z = reader.read_var::<u8>(3)?;
        self.rcode = reader.read_var::<u8>(4)?;
        self.qdcount = reader.read_var::<u16>(16)?;
        self.ancount = reader.read_var::<u16>(16)?;
        self.nscount = reader.read_var::<u16>(16)?;
        self.arcount = reader.read_var::<u16>(16)?;
        Ok(())
    }
}
