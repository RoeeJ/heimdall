use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, Endianness};

use super::ParseError;

pub trait PacketComponent {
    fn write<E: Endianness>(
        &self,
        writer: &mut BitWriter<&mut Vec<u8>, E>,
    ) -> Result<(), ParseError>;
    fn read<E: Endianness>(&mut self, reader: &mut BitReader<&[u8], E>) -> Result<(), ParseError>;

    fn read_labels<E: Endianness>(
        &mut self,
        reader: &mut BitReader<&[u8], E>,
    ) -> Result<Vec<String>, ParseError> {
        let mut labels = Vec::new();
        loop {
            let label_len = reader.read_var::<u8>(8)?;
            if label_len == 0 {
                labels.push(String::new());
                break;
            }
            let mut buf = vec![0; label_len as usize];
            reader.read_bytes(&mut buf)?;
            let label = String::from_utf8(buf).map_err(|_| ParseError::InvalidLabel)?;
            labels.push(label);
        }

        Ok(labels)
    }

    fn write_labels<E: Endianness>(
        &self,
        writer: &mut BitWriter<&mut Vec<u8>, E>,
        labels: &Vec<String>,
    ) -> Result<(), ParseError> {
        for label in labels {
            writer.write_var::<u8>(8, label.len() as u8)?;
            writer.write_bytes(label.as_bytes())?;
        }

        Ok(())
    }
}
