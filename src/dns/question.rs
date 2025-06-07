use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, Endianness};

use super::{
    ParseError,
    common::PacketComponent,
    enums::{DNSResourceClass, DNSResourceType},
};

#[derive(Clone, Debug, Default)]
pub struct DNSQuestion {
    pub labels: Vec<String>,
    pub qtype: DNSResourceType,
    pub qclass: DNSResourceClass,
}

impl PacketComponent for DNSQuestion {
    fn write<E: Endianness>(
        &self,
        writer: &mut BitWriter<&mut Vec<u8>, E>,
    ) -> Result<(), ParseError> {
        self.write_labels(writer, &self.labels)?;
        writer.write_var::<u16>(16, self.qtype as u16)?;
        writer.write_var::<u16>(16, self.qclass as u16)?;
        Ok(())
    }

    fn read<E: Endianness>(&mut self, reader: &mut BitReader<&[u8], E>) -> Result<(), ParseError> {
        let labels = self.read_labels(reader)?;
        let qtype = reader.read_var::<u16>(16)?.into();
        let qclass = reader.read_var::<u16>(16)?.into();
        *self = DNSQuestion {
            labels,
            qtype,
            qclass,
        };
        Ok(())
    }
}
