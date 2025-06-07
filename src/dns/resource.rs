use bitstream_io::{BitRead, BitWrite, BitReader};

use super::{
    common::PacketComponent,
    enums::{DNSResourceClass, DNSResourceType},
    ParseError,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DNSResource {
    pub labels: Vec<String>,
    pub rtype: DNSResourceType,
    pub rclass: DNSResourceClass,
    pub ttl: u32,
    pub rdlength: u16,
    pub rdata: Vec<u8>, // Raw resource data for now
    pub parsed_rdata: Option<String>, // Parsed string representation for display
    pub raw_class: Option<u16>, // Raw class value for EDNS where class != standard DNS class
}

impl DNSResource {
    /// Rebuild rdata from parsed data, expanding compression pointers
    fn rebuild_rdata(&self) -> Result<Vec<u8>, ParseError> {
        use bitstream_io::{BigEndian, BitWriter};
        
        match &self.parsed_rdata {
            Some(parsed) => {
                // If we have parsed data, rebuild the rdata based on record type
                match self.rtype {
                    super::enums::DNSResourceType::MX => {
                        // MX format: priority (2 bytes) + domain name
                        if let Some(space_pos) = parsed.find(' ') {
                            let priority_str = &parsed[..space_pos];
                            let domain_str = &parsed[space_pos + 1..];
                            
                            if let Ok(priority) = priority_str.parse::<u16>() {
                                let mut rdata = Vec::new();
                                let mut writer = BitWriter::<_, BigEndian>::new(&mut rdata);
                                
                                // Write priority
                                writer.write_var::<u16>(16, priority)?;
                                
                                // Write domain as labels
                                let labels: Vec<String> = if domain_str.is_empty() {
                                    vec![]
                                } else {
                                    domain_str.split('.').map(|s| s.to_string()).collect()
                                };
                                self.write_labels(&mut writer, &labels)?;
                                
                                return Ok(rdata);
                            }
                        }
                        // Fall back to original rdata if parsing fails
                        Ok(self.rdata.clone())
                    },
                    super::enums::DNSResourceType::NS | 
                    super::enums::DNSResourceType::CNAME | 
                    super::enums::DNSResourceType::PTR => {
                        // These just contain a domain name
                        let mut rdata = Vec::new();
                        let mut writer = BitWriter::<_, BigEndian>::new(&mut rdata);
                        
                        let labels: Vec<String> = if parsed.is_empty() {
                            vec![]
                        } else {
                            parsed.split('.').map(|s| s.to_string()).collect()
                        };
                        self.write_labels(&mut writer, &labels)?;
                        
                        Ok(rdata)
                    },
                    super::enums::DNSResourceType::TXT => {
                        // TXT records: reconstruct length-prefixed strings
                        let mut rdata = Vec::new();
                        
                        // Remove quotes and split by spaces to get individual strings
                        let txt_parts: Vec<&str> = parsed
                            .split(' ')
                            .map(|s| s.trim_matches('"'))
                            .collect();
                        
                        for part in txt_parts {
                            if part.len() <= 255 {
                                rdata.push(part.len() as u8);
                                rdata.extend_from_slice(part.as_bytes());
                            }
                        }
                        
                        Ok(rdata)
                    },
                    super::enums::DNSResourceType::A => {
                        // Parse IPv4 address
                        let parts: Vec<&str> = parsed.split('.').collect();
                        if parts.len() == 4 {
                            let mut rdata = Vec::new();
                            for part in parts {
                                if let Ok(byte) = part.parse::<u8>() {
                                    rdata.push(byte);
                                } else {
                                    return Ok(self.rdata.clone());
                                }
                            }
                            Ok(rdata)
                        } else {
                            Ok(self.rdata.clone())
                        }
                    },
                    super::enums::DNSResourceType::AAAA => {
                        // Parse IPv6 address
                        let parts: Vec<&str> = parsed.split(':').collect();
                        if parts.len() == 8 {
                            let mut rdata = Vec::new();
                            for part in parts {
                                if let Ok(word) = u16::from_str_radix(part, 16) {
                                    rdata.extend_from_slice(&word.to_be_bytes());
                                } else {
                                    return Ok(self.rdata.clone());
                                }
                            }
                            Ok(rdata)
                        } else {
                            Ok(self.rdata.clone())
                        }
                    },
                    _ => {
                        // For other record types, use original rdata
                        Ok(self.rdata.clone())
                    }
                }
            },
            None => {
                // No parsed data, use original
                Ok(self.rdata.clone())
            }
        }
    }

    /// Parse rdata based on the record type, handling compression pointers where applicable
    fn parse_rdata_with_compression(&mut self, packet_buf: &[u8]) -> Result<(), ParseError> {
        if self.rdata.is_empty() {
            return Ok(());
        }

        let parsed = match self.rtype {
            DNSResourceType::A => {
                // IPv4 address
                if self.rdata.len() == 4 {
                    Some(format!("{}.{}.{}.{}", 
                        self.rdata[0], self.rdata[1], self.rdata[2], self.rdata[3]))
                } else {
                    None
                }
            },
            DNSResourceType::AAAA => {
                // IPv6 address
                if self.rdata.len() == 16 {
                    let mut ipv6_parts = Vec::new();
                    for i in (0..16).step_by(2) {
                        let part = ((self.rdata[i] as u16) << 8) | (self.rdata[i + 1] as u16);
                        ipv6_parts.push(format!("{:x}", part));
                    }
                    Some(ipv6_parts.join(":"))
                } else {
                    None
                }
            },
            DNSResourceType::MX => {
                // MX record: priority (2 bytes) + domain name
                if self.rdata.len() >= 3 {
                    let priority = ((self.rdata[0] as u16) << 8) | (self.rdata[1] as u16);
                    
                    // Debug: log the first few bytes to understand the pattern
                    let rdata_debug = if self.rdata.len() >= 6 {
                        format!("{:02x} {:02x} {:02x} {:02x} {:02x} {:02x}", 
                            self.rdata[0], self.rdata[1], self.rdata[2], 
                            self.rdata[3], self.rdata[4], self.rdata[5])
                    } else {
                        format!("{:02x?}", &self.rdata)
                    };
                    
                    // Parse the domain name starting from byte 2 (after priority)
                    // The domain name may contain compression pointers anywhere within it
                    let domain = {
                        let mut reader = BitReader::<_, bitstream_io::BigEndian>::new(&self.rdata[2..]);
                        let mut temp_component = Self::default();
                        
                        match temp_component.read_labels_with_buffer(&mut reader, Some(packet_buf)) {
                            Ok(labels) => {
                                labels.iter()
                                    .filter(|l| !l.is_empty())
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(".")
                            },
                            Err(_) => {
                                // Fall back to simple parsing
                                match self.parse_simple_domain(&self.rdata[2..]) {
                                    Ok(domain) => domain,
                                    Err(_) => format!("[unparseable_{}]", rdata_debug)
                                }
                            }
                        }
                    };
                    
                    Some(format!("{} {}", priority, domain))
                } else {
                    None
                }
            },
            DNSResourceType::NS | DNSResourceType::CNAME | DNSResourceType::PTR => {
                // These contain just a domain name
                if self.rdata.len() >= 2 && self.rdata[0] & 0xC0 == 0xC0 {
                    // This looks like a compression pointer
                    let pointer_val = ((self.rdata[0] as u16 & 0x3F) << 8) | (self.rdata[1] as u16);
                    
                    if (pointer_val as usize) < packet_buf.len() {
                        let mut reader = BitReader::<_, bitstream_io::BigEndian>::new(&packet_buf[pointer_val as usize..]);
                        let mut temp_component = Self::default();
                        
                        match temp_component.read_labels_with_buffer(&mut reader, Some(packet_buf)) {
                            Ok(labels) => {
                                let domain = labels.iter()
                                    .filter(|l| !l.is_empty())
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(".");
                                Some(domain)
                            },
                            Err(_) => Some("[parse_error]".to_string())
                        }
                    } else {
                        Some("[invalid_pointer]".to_string())
                    }
                } else {
                    // Regular domain name parsing
                    self.parse_simple_domain(&self.rdata).ok()
                }
            },
            DNSResourceType::TXT => {
                // TXT record: length-prefixed strings
                let mut result = Vec::new();
                let mut pos = 0;
                
                while pos < self.rdata.len() {
                    if pos >= self.rdata.len() {
                        break;
                    }
                    let len = self.rdata[pos] as usize;
                    pos += 1;
                    
                    if pos + len > self.rdata.len() {
                        break;
                    }
                    
                    if let Ok(text) = String::from_utf8(self.rdata[pos..pos + len].to_vec()) {
                        result.push(format!("\"{}\"", text));
                    }
                    pos += len;
                }
                
                if result.is_empty() {
                    None
                } else {
                    Some(result.join(" "))
                }
            },
            _ => None, // For other record types, don't parse for now
        };

        self.parsed_rdata = parsed;
        Ok(())
    }

    /// Simple domain name parser without compression support (fallback)
    fn parse_simple_domain(&self, data: &[u8]) -> Result<String, ParseError> {
        let mut labels = Vec::new();
        let mut pos = 0;

        while pos < data.len() {
            let len = data[pos] as usize;
            if len == 0 {
                break;
            }
            
            pos += 1;
            if pos + len > data.len() {
                return Err(ParseError::InvalidLabel);
            }
            
            let label = String::from_utf8(data[pos..pos + len].to_vec())
                .map_err(|_| ParseError::InvalidLabel)?;
            labels.push(label);
            pos += len;
        }

        Ok(labels.join("."))
    }
}

impl PacketComponent for DNSResource {
    fn write<E: bitstream_io::Endianness>(
        &self,
        writer: &mut bitstream_io::BitWriter<&mut Vec<u8>, E>,
    ) -> Result<(), super::ParseError> {
        self.write_labels(writer, &self.labels)?;
        writer.write_var::<u16>(16, self.rtype.into())?;
        writer.write_var::<u16>(16, self.rclass.into())?;
        writer.write_var::<u32>(32, self.ttl)?;
        
        // Rebuild rdata from parsed data if available (to expand compression pointers)
        let rdata_to_write = self.rebuild_rdata()?;
        let actual_length = rdata_to_write.len() as u16;
        
        // Write the actual length of the rebuilt rdata
        writer.write_var::<u16>(16, actual_length)?;
        writer.write_bytes(&rdata_to_write)?;
        Ok(())
    }

    fn read<E: bitstream_io::Endianness>(
        &mut self,
        reader: &mut bitstream_io::BitReader<&[u8], E>,
    ) -> Result<(), super::ParseError> {
        // Read the name (labels)
        self.labels = self.read_labels(reader)?;

        // Read type, class, TTL, and data length
        self.rtype = reader.read_var::<u16>(16)?.into();
        let raw_class_value = reader.read_var::<u16>(16)?;
        self.rclass = raw_class_value.into();
        self.raw_class = Some(raw_class_value); // Store raw value for EDNS
        self.ttl = reader.read_var::<u32>(32)?;
        self.rdlength = reader.read_var::<u16>(16)?;

        // Read the resource data
        if self.rdlength > 0 {
            self.rdata = vec![0u8; self.rdlength as usize];
            match reader.read_bytes(&mut self.rdata) {
                Ok(_) => {}
                Err(e) => {
                    // If we can't read the full rdata, just set it to empty
                    // This is more forgiving for malformed packets
                    self.rdata = Vec::new();
                    self.rdlength = 0;
                    return Err(super::ParseError::InvalidBitStream(e.to_string()));
                }
            }
        } else {
            self.rdata = Vec::new();
        }

        Ok(())
    }

    fn read_with_buffer<E: bitstream_io::Endianness>(
        &mut self,
        reader: &mut bitstream_io::BitReader<&[u8], E>,
        packet_buf: &[u8],
    ) -> Result<(), super::ParseError> {
        // Read the name (labels) with compression support
        self.labels = self.read_labels_with_buffer(reader, Some(packet_buf))?;

        // Read type, class, TTL, and data length
        self.rtype = reader.read_var::<u16>(16)?.into();
        let raw_class_value = reader.read_var::<u16>(16)?;
        self.rclass = raw_class_value.into();
        self.raw_class = Some(raw_class_value); // Store raw value for EDNS
        self.ttl = reader.read_var::<u32>(32)?;
        self.rdlength = reader.read_var::<u16>(16)?;

        // Read the resource data
        if self.rdlength > 0 {
            self.rdata = vec![0u8; self.rdlength as usize];
            match reader.read_bytes(&mut self.rdata) {
                Ok(_) => {
                    // Parse the rdata with compression support
                    self.parse_rdata_with_compression(packet_buf)?;
                }
                Err(e) => {
                    // If we can't read the full rdata, just set it to empty
                    // This is more forgiving for malformed packets
                    self.rdata = Vec::new();
                    self.rdlength = 0;
                    return Err(super::ParseError::InvalidBitStream(e.to_string()));
                }
            }
        } else {
            self.rdata = Vec::new();
        }

        Ok(())
    }
}
