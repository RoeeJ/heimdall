use super::{
    DNSHeader, ParseError,
    enums::{DNSResourceClass, DNSResourceType},
};
use std::borrow::Cow;

/// Zero-copy DNS packet view that avoids allocations
#[derive(Debug)]
pub struct DNSPacketView<'a> {
    /// Original packet buffer
    pub data: &'a [u8],
    /// Pre-parsed header
    pub header: DNSHeader,
    /// Lazy question iterator
    questions: Option<QuestionIterator<'a>>,
    /// Lazy answer iterator  
    answers: Option<ResourceIterator<'a>>,
}

/// Iterator over questions without allocation
#[derive(Debug)]
pub struct QuestionIterator<'a> {
    data: &'a [u8],
    offset: usize,
    remaining: u16,
}

/// Iterator over resources without allocation
#[derive(Debug)]
#[allow(dead_code)] // TODO: Implement resource iteration
pub struct ResourceIterator<'a> {
    data: &'a [u8],
    offset: usize,
    remaining: u16,
}

/// Zero-copy question view
#[derive(Debug)]
pub struct QuestionView<'a> {
    pub labels: DomainLabels<'a>,
    pub qtype: DNSResourceType,
    pub qclass: DNSResourceClass,
}

/// Zero-copy resource view
#[derive(Debug)]
pub struct ResourceView<'a> {
    pub name: DomainLabels<'a>,
    pub rtype: DNSResourceType,
    pub rclass: DNSResourceClass,
    pub ttl: u32,
    pub rdata: &'a [u8],
}

/// Domain name as a series of label views
#[derive(Debug)]
pub struct DomainLabels<'a> {
    data: &'a [u8],
    start_offset: usize,
}

impl<'a> DNSPacketView<'a> {
    /// Create a zero-copy view of a DNS packet
    pub fn new(data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() < 12 {
            return Err(ParseError::InvalidHeader);
        }

        // Parse header directly from bytes
        let header = {
            use bitstream_io::{BigEndian, BitReader};
            let mut reader = BitReader::<_, BigEndian>::new(&data[0..12]);
            let mut h = DNSHeader::default();
            use super::common::PacketComponent;
            h.read(&mut reader)?;
            h
        };

        Ok(Self {
            data,
            header,
            questions: None,
            answers: None,
        })
    }

    /// Get an iterator over questions without parsing them all
    pub fn questions(&mut self) -> Result<&mut QuestionIterator<'a>, ParseError> {
        if self.questions.is_none() {
            self.questions = Some(QuestionIterator {
                data: self.data,
                offset: 12,
                remaining: self.header.qdcount,
            });
        }
        Ok(self.questions.as_mut().unwrap())
    }

    /// Get an iterator over answers without parsing them all
    pub fn answers(&mut self) -> Result<&mut ResourceIterator<'a>, ParseError> {
        if self.answers.is_none() {
            // Need to skip questions first
            let mut offset = 12;
            for _ in 0..self.header.qdcount {
                offset = skip_question(self.data, offset)?;
            }

            self.answers = Some(ResourceIterator {
                data: self.data,
                offset,
                remaining: self.header.ancount,
            });
        }
        Ok(self.answers.as_mut().unwrap())
    }

    /// Check if packet is a query
    #[inline]
    pub fn is_query(&self) -> bool {
        !self.header.qr
    }

    /// Get the first question domain without allocation
    pub fn first_question_domain(&self) -> Result<Cow<'_, str>, ParseError> {
        if self.header.qdcount == 0 {
            return Err(ParseError::InvalidQuestionSection);
        }

        let labels = DomainLabels {
            data: self.data,
            start_offset: 12,
        };

        labels.to_string_cow()
    }

    /// Compare first question domain without allocation
    pub fn first_question_matches(&self, domain: &str) -> Result<bool, ParseError> {
        if self.header.qdcount == 0 {
            return Ok(false);
        }

        let labels = DomainLabels {
            data: self.data,
            start_offset: 12,
        };

        labels.matches_domain(domain)
    }
}

impl<'a> Iterator for QuestionIterator<'a> {
    type Item = Result<QuestionView<'a>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let labels = DomainLabels {
            data: self.data,
            start_offset: self.offset,
        };

        // Skip past the domain name
        match skip_domain_name(self.data, self.offset) {
            Ok(new_offset) => self.offset = new_offset,
            Err(e) => return Some(Err(e)),
        }

        // Parse type and class
        if self.offset + 4 > self.data.len() {
            return Some(Err(ParseError::InvalidQuestionSection));
        }

        let qtype_raw = u16::from_be_bytes([self.data[self.offset], self.data[self.offset + 1]]);
        let qclass_raw =
            u16::from_be_bytes([self.data[self.offset + 2], self.data[self.offset + 3]]);

        self.offset += 4;
        self.remaining -= 1;

        Some(Ok(QuestionView {
            labels,
            qtype: DNSResourceType::from(qtype_raw),
            qclass: DNSResourceClass::from(qclass_raw),
        }))
    }
}

impl<'a> DomainLabels<'a> {
    /// Convert to owned string when needed
    pub fn to_string_cow(&self) -> Result<Cow<'a, str>, ParseError> {
        let mut result = Vec::new();
        let mut offset = self.start_offset;
        let mut jumps = 0;

        loop {
            if offset >= self.data.len() {
                return Err(ParseError::InvalidLabel);
            }

            let len = self.data[offset];

            // Check for compression pointer
            if (len & 0xC0) == 0xC0 {
                if offset + 1 >= self.data.len() {
                    return Err(ParseError::InvalidLabel);
                }

                jumps += 1;
                if jumps > 5 {
                    return Err(ParseError::InvalidLabel);
                }

                let pointer =
                    u16::from_be_bytes([self.data[offset] & 0x3F, self.data[offset + 1]]) as usize;
                offset = pointer;
                continue;
            }

            if len == 0 {
                break;
            }

            if (len as usize) > 63 {
                return Err(ParseError::InvalidLabel);
            }

            offset += 1;
            let label_end = offset + len as usize;

            if label_end > self.data.len() {
                return Err(ParseError::InvalidLabel);
            }

            if !result.is_empty() {
                result.push(b'.');
            }

            result.extend_from_slice(&self.data[offset..label_end]);
            offset = label_end;
        }

        // Try to convert to string without allocation if valid UTF-8
        match std::str::from_utf8(&result) {
            Ok(s) => Ok(Cow::Owned(s.to_lowercase())),
            Err(_) => Err(ParseError::InvalidLabel),
        }
    }

    /// Compare with a domain name without allocation
    pub fn matches_domain(&self, domain: &str) -> Result<bool, ParseError> {
        let mut offset = self.start_offset;
        let mut domain_parts = domain.split('.').filter(|s| !s.is_empty());
        let mut jumps = 0;

        loop {
            if offset >= self.data.len() {
                return Err(ParseError::InvalidLabel);
            }

            let len = self.data[offset];

            // Handle compression
            if (len & 0xC0) == 0xC0 {
                if offset + 1 >= self.data.len() {
                    return Err(ParseError::InvalidLabel);
                }

                jumps += 1;
                if jumps > 5 {
                    return Err(ParseError::InvalidLabel);
                }

                let pointer =
                    u16::from_be_bytes([self.data[offset] & 0x3F, self.data[offset + 1]]) as usize;
                offset = pointer;
                continue;
            }

            if len == 0 {
                // End of labels - check if domain parts are also exhausted
                return Ok(domain_parts.next().is_none());
            }

            if (len as usize) > 63 {
                return Err(ParseError::InvalidLabel);
            }

            // Get the label
            offset += 1;
            let label_end = offset + len as usize;

            if label_end > self.data.len() {
                return Err(ParseError::InvalidLabel);
            }

            let label = &self.data[offset..label_end];

            // Compare with next domain part
            match domain_parts.next() {
                Some(part) => {
                    if !label.eq_ignore_ascii_case(part.as_bytes()) {
                        return Ok(false);
                    }
                }
                None => return Ok(false),
            }

            offset = label_end;
        }
    }
}

/// Skip a question and return the offset after it
fn skip_question(data: &[u8], offset: usize) -> Result<usize, ParseError> {
    let offset = skip_domain_name(data, offset)?;

    // Skip type and class (4 bytes)
    if offset + 4 > data.len() {
        return Err(ParseError::InvalidQuestionSection);
    }

    Ok(offset + 4)
}

/// Skip a domain name and return the offset after it
fn skip_domain_name(data: &[u8], mut offset: usize) -> Result<usize, ParseError> {
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

            jumps += 1;
            if jumps > 5 {
                return Err(ParseError::InvalidLabel);
            }

            // Remember where we found the first pointer
            if first_pointer_offset.is_none() {
                first_pointer_offset = Some(offset + 2);
            }

            let pointer = u16::from_be_bytes([data[offset] & 0x3F, data[offset + 1]]) as usize;
            offset = pointer;
            continue;
        }

        if len == 0 {
            offset += 1;
            break;
        }

        if (len as usize) > 63 {
            return Err(ParseError::InvalidLabel);
        }

        offset += 1 + len as usize;
    }

    // If we followed pointers, return to after the first pointer
    Ok(first_pointer_offset.unwrap_or(offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_copy_packet_view() {
        // Simple DNS query for example.com
        let packet = vec![
            0x12, 0x34, // ID
            0x01, 0x00, // Flags: recursion desired
            0x00, 0x01, // QDCOUNT: 1
            0x00, 0x00, // ANCOUNT: 0
            0x00, 0x00, // NSCOUNT: 0
            0x00, 0x00, // ARCOUNT: 0
            // Question: example.com A IN
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm',
            0x00, // End of name
            0x00, 0x01, // Type A
            0x00, 0x01, // Class IN
        ];

        let view = DNSPacketView::new(&packet).unwrap();
        assert!(view.is_query());
        assert_eq!(view.header.qdcount, 1);

        let domain = view.first_question_domain().unwrap();
        assert_eq!(&*domain, "example.com");

        assert!(view.first_question_matches("example.com").unwrap());
        assert!(!view.first_question_matches("google.com").unwrap());
    }

    #[test]
    fn test_domain_labels_with_compression() {
        // DNS packet with compression pointer
        let packet = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            // First occurrence: example.com
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00,
            // Second occurrence with compression: www + pointer to example.com
            0x03, b'w', b'w', b'w', 0xC0, 0x0C, // Pointer to offset 12 (example.com)
        ];

        let labels = DomainLabels {
            data: &packet,
            start_offset: 12,
        };

        let domain = labels.to_string_cow().unwrap();
        assert_eq!(&*domain, "example.com");

        let labels2 = DomainLabels {
            data: &packet,
            start_offset: 25,
        };

        let domain2 = labels2.to_string_cow().unwrap();
        assert_eq!(&*domain2, "www.example.com");
    }
}
