use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, Endianness};

use super::ParseError;

pub trait PacketComponent {
    fn write<E: Endianness>(
        &self,
        writer: &mut BitWriter<&mut Vec<u8>, E>,
    ) -> Result<(), ParseError>;
    fn read<E: Endianness>(&mut self, reader: &mut BitReader<&[u8], E>) -> Result<(), ParseError>;

    /// Read with access to the full packet buffer for compression support
    fn read_with_buffer<E: Endianness>(
        &mut self,
        reader: &mut BitReader<&[u8], E>,
        _packet_buf: &[u8],
    ) -> Result<(), ParseError> {
        // Default implementation just calls read for backwards compatibility
        self.read(reader)
    }

    fn read_labels<E: Endianness>(
        &mut self,
        reader: &mut BitReader<&[u8], E>,
    ) -> Result<Vec<String>, ParseError> {
        // Default implementation without compression support
        // Override this method in implementations that need compression
        self.read_labels_with_buffer(reader, None)
    }

    fn read_labels_with_buffer<E: Endianness>(
        &mut self,
        reader: &mut BitReader<&[u8], E>,
        packet_buf: Option<&[u8]>,
    ) -> Result<Vec<String>, ParseError> {
        use crate::dns::unified_parser::UnifiedDnsParser;

        if let Some(buf) = packet_buf {
            // We need to peek at the current position to use the unified parser
            // Since we can't get position from BitReader, we'll read byte by byte and handle compression
            let mut labels = Vec::new();
            let mut jump_count = 0;

            loop {
                let first_byte = reader.read_var::<u8>(8)?;

                if first_byte == 0 {
                    break;
                }

                if (first_byte & 0xC0) == 0xC0 {
                    // This is a compression pointer
                    let second_byte = reader.read_var::<u8>(8)?;
                    let pointer = ((first_byte as u16 & 0x3F) << 8) | second_byte as u16;

                    // Use unified parser to read from the pointer location
                    let (pointer_labels, _) =
                        UnifiedDnsParser::parse_domain_name(buf, pointer as usize)?;
                    labels.extend(pointer_labels);
                    break;
                }

                if first_byte > 63 {
                    return Err(ParseError::InvalidLabel);
                }

                let mut label_buf = vec![0; first_byte as usize];
                reader.read_bytes(&mut label_buf)?;
                let label = String::from_utf8(label_buf).map_err(|_| ParseError::InvalidLabel)?;
                labels.push(label);

                jump_count += 1;
                if jump_count > 100 {
                    return Err(ParseError::InvalidLabel);
                }
            }

            Ok(labels)
        } else {
            // Fallback to simple parsing without compression support
            let mut labels = Vec::new();

            loop {
                let first_byte = reader.read_var::<u8>(8)?;

                if first_byte == 0 {
                    break;
                }

                if (first_byte & 0xC0) == 0xC0 {
                    // Compression pointer without buffer - can't follow
                    return Err(ParseError::InvalidLabel);
                }

                if first_byte > 63 {
                    return Err(ParseError::InvalidLabel);
                }

                let mut buf = vec![0; first_byte as usize];
                reader.read_bytes(&mut buf)?;
                let label = String::from_utf8(buf).map_err(|_| ParseError::InvalidLabel)?;
                labels.push(label);
            }

            Ok(labels)
        }
    }

    fn write_labels<E: Endianness>(
        &self,
        writer: &mut BitWriter<&mut Vec<u8>, E>,
        labels: &Vec<String>,
    ) -> Result<(), ParseError> {
        // Handle root zone (empty labels)
        if labels.is_empty() {
            writer.write_var::<u8>(8, 0)?;
            return Ok(());
        }

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
        if !labels.last().unwrap().is_empty() {
            writer.write_var::<u8>(8, 0)?;
        }

        Ok(())
    }
}
