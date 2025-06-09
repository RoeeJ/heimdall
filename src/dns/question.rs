use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, Endianness};

use super::{
    ParseError,
    common::PacketComponent,
    enums::{DNSResourceClass, DNSResourceType},
};

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
        writer.write_var::<u16>(16, self.qtype.into())?;
        writer.write_var::<u16>(16, self.qclass.into())?;
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

    fn read_with_buffer<E: Endianness>(
        &mut self,
        reader: &mut BitReader<&[u8], E>,
        packet_buf: &[u8],
    ) -> Result<(), ParseError> {
        let labels = self.read_labels_with_buffer(reader, Some(packet_buf))?;
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
