use super::{
    DNSHeader, DNSPacket, ParseError,
    common::PacketComponent,
    enums::{DNSResourceClass, DNSResourceType},
};
use bitstream_io::{BigEndian, BitReader};

/// Unified DNS parser that combines the best of all parsing approaches
/// - Zero-copy views for read-only operations
/// - Efficient domain name parsing with compression support
/// - Optional full packet parsing when modifications are needed
pub struct UnifiedDnsParser;

impl UnifiedDnsParser {
    /// Parse only packet metadata without allocating (fastest)
    pub fn parse_metadata(buf: &[u8]) -> Result<PacketMetadata, ParseError> {
        if buf.len() < 12 {
            return Err(ParseError::InvalidHeader);
        }

        let mut reader = BitReader::<_, BigEndian>::new(&buf[0..12]);
        let mut header = DNSHeader::default();
        header.read(&mut reader)?;

        Ok(PacketMetadata {
            header,
            buffer: buf,
        })
    }

    /// Parse packet lazily with zero-copy views (fast, read-only)
    pub fn parse_lazy(buf: &[u8]) -> Result<LazyDnsPacket<'_>, ParseError> {
        let metadata = Self::parse_metadata(buf)?;

        // Calculate section offsets
        let mut offset = 12;
        let questions_start = offset;

        // Skip questions
        for _ in 0..metadata.header.qdcount {
            offset = Self::skip_question(buf, offset)?;
        }
        let answers_start = offset;

        // Skip answers
        for _ in 0..metadata.header.ancount {
            offset = Self::skip_resource(buf, offset)?;
        }
        let authorities_start = offset;

        // Skip authorities
        for _ in 0..metadata.header.nscount {
            offset = Self::skip_resource(buf, offset)?;
        }
        let additionals_start = offset;

        Ok(LazyDnsPacket {
            metadata,
            questions_start,
            answers_start,
            authorities_start,
            additionals_start,
        })
    }

    /// Parse packet fully for modification (slower, but complete)
    pub fn parse_full(buf: &[u8]) -> Result<DNSPacket, ParseError> {
        DNSPacket::parse(buf)
    }

    /// Unified domain name parsing function (replaces all duplicate implementations)
    pub fn parse_domain_name(
        data: &[u8],
        start: usize,
    ) -> Result<(Vec<String>, usize), ParseError> {
        let mut labels = Vec::new();
        let mut offset = start;
        let mut jumps = 0;
        let mut first_pointer_offset = None;

        loop {
            if offset >= data.len() {
                return Err(ParseError::InvalidLabel);
            }

            let len = data[offset];

            // Check for compression pointer
            if (len & 0xC0) == 0xC0 {
                if offset + 1 >= data.len() {
                    return Err(ParseError::InvalidLabel);
                }

                // Remember first pointer location for return offset
                if first_pointer_offset.is_none() {
                    first_pointer_offset = Some(offset + 2);
                }

                jumps += 1;
                if jumps > 5 {
                    return Err(ParseError::InvalidLabel);
                }

                let pointer = u16::from_be_bytes([data[offset] & 0x3F, data[offset + 1]]) as usize;

                // Recursively parse from pointer location
                let (pointer_labels, _) = Self::parse_domain_name(data, pointer)?;
                labels.extend(pointer_labels);

                // Return offset after first pointer
                return Ok((labels, first_pointer_offset.unwrap_or(offset + 2)));
            }

            if len == 0 {
                // End of domain name
                return Ok((labels, offset + 1));
            }

            if len > 63 {
                return Err(ParseError::InvalidLabel);
            }

            offset += 1;
            let label_end = offset + len as usize;

            if label_end > data.len() {
                return Err(ParseError::InvalidLabel);
            }

            let label = String::from_utf8(data[offset..label_end].to_vec())
                .map_err(|_| ParseError::InvalidLabel)?;
            labels.push(label);

            offset = label_end;
        }
    }

    /// Fast domain name comparison without allocation
    pub fn compare_domain_name(
        data: &[u8],
        start: usize,
        domain: &str,
    ) -> Result<bool, ParseError> {
        let mut offset = start;
        let domain_parts: Vec<&str> = domain.split('.').filter(|s| !s.is_empty()).collect();
        let mut part_index = 0;
        let mut jumps = 0;

        loop {
            if offset >= data.len() {
                return Err(ParseError::InvalidLabel);
            }

            let len = data[offset];

            // Handle compression
            if (len & 0xC0) == 0xC0 {
                if offset + 1 >= data.len() {
                    return Err(ParseError::InvalidLabel);
                }

                jumps += 1;
                if jumps > 5 {
                    return Err(ParseError::InvalidLabel);
                }

                let pointer = u16::from_be_bytes([data[offset] & 0x3F, data[offset + 1]]) as usize;
                offset = pointer;
                continue;
            }

            if len == 0 {
                // End of labels - check if domain parts are also exhausted
                return Ok(part_index == domain_parts.len());
            }

            if len > 63 {
                return Err(ParseError::InvalidLabel);
            }

            // Get the label
            offset += 1;
            let label_end = offset + len as usize;

            if label_end > data.len() {
                return Err(ParseError::InvalidLabel);
            }

            let label = &data[offset..label_end];

            // Compare with next domain part
            if part_index >= domain_parts.len() {
                return Ok(false);
            }

            if !label.eq_ignore_ascii_case(domain_parts[part_index].as_bytes()) {
                return Ok(false);
            }

            part_index += 1;
            offset = label_end;
        }
    }

    /// Skip a domain name and return the new offset
    pub fn skip_domain_name(data: &[u8], mut offset: usize) -> Result<usize, ParseError> {
        let mut jumps = 0;
        let mut first_pointer_offset = None;

        loop {
            if offset >= data.len() {
                return Err(ParseError::InvalidLabel);
            }

            let len = data[offset];

            // Check for compression pointer
            if (len & 0xC0) == 0xC0 {
                if offset + 1 >= data.len() {
                    return Err(ParseError::InvalidLabel);
                }

                // Remember where we found the first pointer
                if first_pointer_offset.is_none() {
                    first_pointer_offset = Some(offset + 2);
                }

                jumps += 1;
                if jumps > 5 {
                    return Err(ParseError::InvalidLabel);
                }

                let pointer = u16::from_be_bytes([data[offset] & 0x3F, data[offset + 1]]) as usize;
                offset = pointer;
                continue;
            }

            if len == 0 {
                offset += 1;
                break;
            }

            if len > 63 {
                return Err(ParseError::InvalidLabel);
            }

            offset += 1 + len as usize;
        }

        // If we followed pointers, return to after the first pointer
        Ok(first_pointer_offset.unwrap_or(offset))
    }

    /// Skip a question section
    fn skip_question(data: &[u8], offset: usize) -> Result<usize, ParseError> {
        let offset = Self::skip_domain_name(data, offset)?;

        // Skip type and class (4 bytes)
        if offset + 4 > data.len() {
            return Err(ParseError::InvalidQuestionSection);
        }

        Ok(offset + 4)
    }

    /// Skip a resource record
    fn skip_resource(data: &[u8], offset: usize) -> Result<usize, ParseError> {
        let offset = Self::skip_domain_name(data, offset)?;

        // Skip type, class, ttl, rdlength (10 bytes)
        if offset + 10 > data.len() {
            return Err(ParseError::InvalidAnswerSection);
        }

        let rdlength = u16::from_be_bytes([data[offset + 8], data[offset + 9]]) as usize;
        let new_offset = offset + 10 + rdlength;

        if new_offset > data.len() {
            return Err(ParseError::InvalidAnswerSection);
        }

        Ok(new_offset)
    }
}

/// Minimal packet metadata for fast operations
pub struct PacketMetadata<'a> {
    pub header: DNSHeader,
    pub buffer: &'a [u8],
}

/// Lazy packet view with calculated offsets
pub struct LazyDnsPacket<'a> {
    pub metadata: PacketMetadata<'a>,
    pub questions_start: usize,
    pub answers_start: usize,
    pub authorities_start: usize,
    pub additionals_start: usize,
}

impl<'a> LazyDnsPacket<'a> {
    /// Get the first question without parsing all
    pub fn first_question(&self) -> Result<LazyQuestion<'a>, ParseError> {
        if self.metadata.header.qdcount == 0 {
            return Err(ParseError::InvalidQuestionSection);
        }

        LazyQuestion::parse(self.metadata.buffer, self.questions_start)
    }

    /// Check if packet matches a domain (for cache lookups)
    pub fn matches_domain(&self, domain: &str) -> Result<bool, ParseError> {
        if self.metadata.header.qdcount == 0 {
            return Ok(false);
        }

        UnifiedDnsParser::compare_domain_name(self.metadata.buffer, self.questions_start, domain)
    }

    /// Get question type without full parsing
    pub fn question_type(&self) -> Result<DNSResourceType, ParseError> {
        if self.metadata.header.qdcount == 0 {
            return Err(ParseError::InvalidQuestionSection);
        }

        let offset =
            UnifiedDnsParser::skip_domain_name(self.metadata.buffer, self.questions_start)?;

        if offset + 2 > self.metadata.buffer.len() {
            return Err(ParseError::InvalidQuestionSection);
        }

        let qtype = u16::from_be_bytes([
            self.metadata.buffer[offset],
            self.metadata.buffer[offset + 1],
        ]);

        Ok(DNSResourceType::from(qtype))
    }

    /// Convert to full packet when needed
    pub fn to_owned(&self) -> Result<DNSPacket, ParseError> {
        UnifiedDnsParser::parse_full(self.metadata.buffer)
    }
}

/// Lazy question view
pub struct LazyQuestion<'a> {
    pub buffer: &'a [u8],
    pub name_offset: usize,
    pub qtype: DNSResourceType,
    pub qclass: DNSResourceClass,
}

impl<'a> LazyQuestion<'a> {
    fn parse(buffer: &'a [u8], offset: usize) -> Result<Self, ParseError> {
        let name_end = UnifiedDnsParser::skip_domain_name(buffer, offset)?;

        if name_end + 4 > buffer.len() {
            return Err(ParseError::InvalidQuestionSection);
        }

        let qtype = u16::from_be_bytes([buffer[name_end], buffer[name_end + 1]]);
        let qclass = u16::from_be_bytes([buffer[name_end + 2], buffer[name_end + 3]]);

        Ok(LazyQuestion {
            buffer,
            name_offset: offset,
            qtype: DNSResourceType::from(qtype),
            qclass: DNSResourceClass::from(qclass),
        })
    }

    /// Get domain name when needed
    pub fn domain(&self) -> Result<String, ParseError> {
        let (labels, _) = UnifiedDnsParser::parse_domain_name(self.buffer, self.name_offset)?;
        Ok(labels.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_domain_parsing() {
        // Test data with compression
        let packet = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            // example.com at offset 12
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00,
            // www.example.com with compression at offset 25
            0x03, b'w', b'w', b'w', 0xC0, 0x0C,
        ];

        // Parse first domain
        let (labels1, offset1) = UnifiedDnsParser::parse_domain_name(&packet, 12).unwrap();
        assert_eq!(labels1, vec!["example", "com"]);
        assert_eq!(offset1, 25);

        // Parse compressed domain
        let (labels2, offset2) = UnifiedDnsParser::parse_domain_name(&packet, 25).unwrap();
        assert_eq!(labels2, vec!["www", "example", "com"]);
        assert_eq!(offset2, 31);
    }

    #[test]
    fn test_domain_comparison() {
        let packet = vec![
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00,
        ];

        assert!(UnifiedDnsParser::compare_domain_name(&packet, 0, "example.com").unwrap());
        assert!(UnifiedDnsParser::compare_domain_name(&packet, 0, "EXAMPLE.COM").unwrap());
        assert!(!UnifiedDnsParser::compare_domain_name(&packet, 0, "google.com").unwrap());
    }

    #[test]
    fn test_lazy_parsing() {
        // Simple DNS query
        let packet = vec![
            0x12, 0x34, // ID
            0x01, 0x00, // Flags
            0x00, 0x01, // QDCOUNT
            0x00, 0x00, // ANCOUNT
            0x00, 0x00, // NSCOUNT
            0x00, 0x00, // ARCOUNT
            // Question
            0x06, b'g', b'o', b'o', b'g', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00,
            0x01, // Type A
            0x00, 0x01, // Class IN
        ];

        let lazy = UnifiedDnsParser::parse_lazy(&packet).unwrap();
        assert_eq!(lazy.metadata.header.qdcount, 1);
        assert!(lazy.matches_domain("google.com").unwrap());
        assert_eq!(lazy.question_type().unwrap(), DNSResourceType::A);
    }
}
