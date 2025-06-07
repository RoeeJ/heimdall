use bitstream_io::{BitRead, BitWrite};

use super::{
    common::PacketComponent,
    enums::{DNSResourceClass, DNSResourceType},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DNSResource {
    pub labels: Vec<String>,
    pub rtype: DNSResourceType,
    pub rclass: DNSResourceClass,
    pub ttl: u32,
    pub rdlength: u16,
    pub rdata: Vec<u8>,  // Raw resource data for now
}

impl PacketComponent for DNSResource {
    fn write<E: bitstream_io::Endianness>(
        &self,
        writer: &mut bitstream_io::BitWriter<&mut Vec<u8>, E>,
    ) -> Result<(), super::ParseError> {
        self.write_labels(writer, &self.labels)?;
        writer.write_var::<u16>(16, self.rtype as u16)?;
        writer.write_var::<u16>(16, self.rclass as u16)?;
        writer.write_var::<u32>(32, self.ttl)?;
        writer.write_var::<u16>(16, self.rdlength)?;
        writer.write_bytes(&self.rdata)?;
        Ok(())
    }

    fn read<E: bitstream_io::Endianness>(
        &mut self,
        reader: &mut bitstream_io::BitReader<&[u8], E>,
    ) -> Result<(), super::ParseError> {
        self.labels = self.read_labels(reader)?;
        self.rtype = reader.read_var::<u16>(16)?.into();
        self.rclass = reader.read_var::<u16>(16)?.into();
        self.ttl = reader.read_var::<u32>(32)?;
        self.rdlength = reader.read_var::<u16>(16)?;
        self.rdata = vec![0u8; self.rdlength as usize];
        reader.read_bytes(&mut self.rdata)?;
        Ok(())
    }
}
