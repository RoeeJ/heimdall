use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, Endianness};
use tracing::trace;

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
        let mut jump_count = 0;

        loop {
            let first_byte = reader.read_var::<u8>(8)?;
            trace!("Reading label byte: 0x{:02x}", first_byte);

            // Check for compression pointer (top 2 bits set)
            if (first_byte & 0xC0) == 0xC0 {
                // This is a compression pointer
                let second_byte = reader.read_var::<u8>(8)?;
                let _pointer = ((first_byte as u16 & 0x3F) << 8) | second_byte as u16;

                trace!("Found compression pointer: 0x{:04x}", _pointer);

                // For now, we'll just break out of compression
                // A full implementation would follow the pointer
                // but that requires access to the full packet buffer
                labels.push(String::new());
                break;
            } else if first_byte == 0 {
                // Null terminator - end of name
                labels.push(String::new());
                break;
            } else {
                // Regular label
                let label_len = first_byte as usize;
                if label_len > 63 {
                    return Err(ParseError::InvalidLabel);
                }

                let mut buf = vec![0; label_len];
                reader.read_bytes(&mut buf)?;
                let label = String::from_utf8(buf).map_err(|_| ParseError::InvalidLabel)?;
                trace!("Read label: {}", label);
                labels.push(label);
            }

            jump_count += 1;
            if jump_count > 100 {
                return Err(ParseError::InvalidLabel); // Prevent infinite loops
            }
        }

        Ok(labels)
    }

    fn write_labels<E: Endianness>(
        &self,
        writer: &mut BitWriter<&mut Vec<u8>, E>,
        labels: &Vec<String>,
    ) -> Result<(), ParseError> {
        for label in labels {
            if label.is_empty() {
                // Write null terminator for root label
                writer.write_var::<u8>(8, 0)?;
                break;
            } else {
                writer.write_var::<u8>(8, label.len() as u8)?;
                writer.write_bytes(label.as_bytes())?;
            }
        }

        // Ensure we always write a null terminator if not already written
        if !labels.is_empty() && !labels.last().unwrap().is_empty() {
            writer.write_var::<u8>(8, 0)?;
        }

        Ok(())
    }
}
