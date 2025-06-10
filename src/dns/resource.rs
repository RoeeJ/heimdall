use bitstream_io::{BitRead, BitReader, BitWrite};

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
pub struct DNSResource {
    pub labels: Vec<String>,
    pub rtype: DNSResourceType,
    pub rclass: DNSResourceClass,
    pub ttl: u32,
    pub rdlength: u16,
    pub rdata: Vec<u8>,               // Raw resource data for now
    pub parsed_rdata: Option<String>, // Parsed string representation for display
    pub raw_class: Option<u16>,       // Raw class value for EDNS where class != standard DNS class
}

impl DNSResource {
    /// Extract SOA record fields if this is an SOA record
    pub fn get_soa_fields(&self) -> Option<(String, String, u32, u32, u32, u32, u32)> {
        if self.rtype != DNSResourceType::SOA {
            return None;
        }

        if let Some(parsed) = &self.parsed_rdata {
            let parts: Vec<&str> = parsed.split(' ').collect();
            if parts.len() == 7 {
                if let (Ok(serial), Ok(refresh), Ok(retry), Ok(expire), Ok(minimum)) = (
                    parts[2].parse::<u32>(),
                    parts[3].parse::<u32>(),
                    parts[4].parse::<u32>(),
                    parts[5].parse::<u32>(),
                    parts[6].parse::<u32>(),
                ) {
                    return Some((
                        parts[0].to_string(), // MNAME
                        parts[1].to_string(), // RNAME
                        serial,
                        refresh,
                        retry,
                        expire,
                        minimum,
                    ));
                }
            }
        }
        None
    }

    /// Get the minimum TTL from an SOA record (for negative caching)
    pub fn get_soa_minimum(&self) -> Option<u32> {
        self.get_soa_fields().map(|fields| fields.6)
    }

    /// Extract SRV record fields if this is an SRV record
    pub fn get_srv_fields(&self) -> Option<(u16, u16, u16, String)> {
        if self.rtype != DNSResourceType::SRV {
            return None;
        }

        if let Some(parsed) = &self.parsed_rdata {
            let parts: Vec<&str> = parsed.split(' ').collect();
            if parts.len() == 4 {
                if let (Ok(priority), Ok(weight), Ok(port)) = (
                    parts[0].parse::<u16>(),
                    parts[1].parse::<u16>(),
                    parts[2].parse::<u16>(),
                ) {
                    return Some((priority, weight, port, parts[3].to_string()));
                }
            }
        }
        None
    }

    /// Extract CAA record fields if this is a CAA record
    pub fn get_caa_fields(&self) -> Option<(u8, String, String)> {
        if self.rtype != DNSResourceType::CAA {
            return None;
        }

        if let Some(parsed) = &self.parsed_rdata {
            let parts: Vec<&str> = parsed.splitn(3, ' ').collect();
            if parts.len() >= 2 {
                if let Ok(flags) = parts[0].parse::<u8>() {
                    let tag = parts[1].to_string();
                    let value = if parts.len() > 2 {
                        parts[2].to_string()
                    } else {
                        String::new()
                    };
                    return Some((flags, tag, value));
                }
            }
        }
        None
    }

    /// Extract DNSKEY record fields if this is a DNSKEY record
    pub fn get_dnskey_fields(&self) -> Option<(u16, u8, u8, String)> {
        if self.rtype != DNSResourceType::DNSKEY {
            return None;
        }

        if let Some(parsed) = &self.parsed_rdata {
            let parts: Vec<&str> = parsed.split(' ').collect();
            if parts.len() == 4 {
                if let (Ok(flags), Ok(protocol), Ok(algorithm)) = (
                    parts[0].parse::<u16>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                ) {
                    return Some((flags, protocol, algorithm, parts[3].to_string()));
                }
            }
        }
        None
    }

    /// Extract DS record fields if this is a DS record
    pub fn get_ds_fields(&self) -> Option<(u16, u8, u8, String)> {
        if self.rtype != DNSResourceType::DS {
            return None;
        }

        if let Some(parsed) = &self.parsed_rdata {
            let parts: Vec<&str> = parsed.split(' ').collect();
            if parts.len() == 4 {
                if let (Ok(key_tag), Ok(algorithm), Ok(digest_type)) = (
                    parts[0].parse::<u16>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                ) {
                    return Some((key_tag, algorithm, digest_type, parts[3].to_string()));
                }
            }
        }
        None
    }

    /// Extract TLSA record fields if this is a TLSA record
    pub fn get_tlsa_fields(&self) -> Option<(u8, u8, u8, String)> {
        if self.rtype != DNSResourceType::TLSA {
            return None;
        }

        if let Some(parsed) = &self.parsed_rdata {
            let parts: Vec<&str> = parsed.split(' ').collect();
            if parts.len() == 4 {
                if let (Ok(cert_usage), Ok(selector), Ok(matching_type)) = (
                    parts[0].parse::<u8>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                ) {
                    return Some((cert_usage, selector, matching_type, parts[3].to_string()));
                }
            }
        }
        None
    }

    /// Extract SSHFP record fields if this is an SSHFP record
    pub fn get_sshfp_fields(&self) -> Option<(u8, u8, String)> {
        if self.rtype != DNSResourceType::SSHFP {
            return None;
        }

        if let Some(parsed) = &self.parsed_rdata {
            let parts: Vec<&str> = parsed.split(' ').collect();
            if parts.len() == 3 {
                if let (Ok(algorithm), Ok(fp_type)) =
                    (parts[0].parse::<u8>(), parts[1].parse::<u8>())
                {
                    return Some((algorithm, fp_type, parts[2].to_string()));
                }
            }
        }
        None
    }

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
                    }
                    super::enums::DNSResourceType::NS
                    | super::enums::DNSResourceType::CNAME
                    | super::enums::DNSResourceType::PTR => {
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
                    }
                    super::enums::DNSResourceType::TXT => {
                        // TXT records: reconstruct length-prefixed strings
                        let mut rdata = Vec::new();

                        // Remove quotes and split by spaces to get individual strings
                        let txt_parts: Vec<&str> =
                            parsed.split(' ').map(|s| s.trim_matches('"')).collect();

                        for part in txt_parts {
                            if part.len() <= 255 {
                                rdata.push(part.len() as u8);
                                rdata.extend_from_slice(part.as_bytes());
                            }
                        }

                        Ok(rdata)
                    }
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
                    }
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
                    }
                    super::enums::DNSResourceType::CAA => {
                        // CAA format: flags tag value
                        let parts: Vec<&str> = parsed.splitn(3, ' ').collect();
                        if parts.len() >= 2 {
                            if let Ok(flags) = parts[0].parse::<u8>() {
                                let mut rdata = Vec::new();

                                // Write flags
                                rdata.push(flags);

                                // Write tag length and tag
                                let tag_bytes = parts[1].as_bytes();
                                if tag_bytes.len() <= 255 {
                                    rdata.push(tag_bytes.len() as u8);
                                    rdata.extend_from_slice(tag_bytes);

                                    // Write value if present
                                    if parts.len() > 2 {
                                        rdata.extend_from_slice(parts[2].as_bytes());
                                    }

                                    Ok(rdata)
                                } else {
                                    Ok(self.rdata.clone())
                                }
                            } else {
                                Ok(self.rdata.clone())
                            }
                        } else {
                            Ok(self.rdata.clone())
                        }
                    }
                    super::enums::DNSResourceType::SRV => {
                        // SRV format: priority weight port target
                        let parts: Vec<&str> = parsed.split(' ').collect();
                        if parts.len() == 4 {
                            if let (Ok(priority), Ok(weight), Ok(port)) = (
                                parts[0].parse::<u16>(),
                                parts[1].parse::<u16>(),
                                parts[2].parse::<u16>(),
                            ) {
                                let mut rdata = Vec::new();
                                let mut writer = BitWriter::<_, BigEndian>::new(&mut rdata);

                                // Write priority, weight, port
                                writer.write_var::<u16>(16, priority)?;
                                writer.write_var::<u16>(16, weight)?;
                                writer.write_var::<u16>(16, port)?;

                                // Write target domain
                                let target_labels: Vec<String> = if parts[3].is_empty() {
                                    vec![]
                                } else {
                                    parts[3].split('.').map(|s| s.to_string()).collect()
                                };
                                self.write_labels(&mut writer, &target_labels)?;

                                Ok(rdata)
                            } else {
                                Ok(self.rdata.clone())
                            }
                        } else {
                            Ok(self.rdata.clone())
                        }
                    }
                    super::enums::DNSResourceType::SOA => {
                        // SOA format: MNAME RNAME SERIAL REFRESH RETRY EXPIRE MINIMUM
                        let parts: Vec<&str> = parsed.split(' ').collect();
                        if parts.len() == 7 {
                            let mut rdata = Vec::new();
                            let mut writer = BitWriter::<_, BigEndian>::new(&mut rdata);

                            // Write MNAME
                            let mname_labels: Vec<String> = if parts[0].is_empty() {
                                vec![]
                            } else {
                                parts[0].split('.').map(|s| s.to_string()).collect()
                            };
                            self.write_labels(&mut writer, &mname_labels)?;

                            // Write RNAME
                            let rname_labels: Vec<String> = if parts[1].is_empty() {
                                vec![]
                            } else {
                                parts[1].split('.').map(|s| s.to_string()).collect()
                            };
                            self.write_labels(&mut writer, &rname_labels)?;

                            // Write the 5 32-bit values
                            if let (Ok(serial), Ok(refresh), Ok(retry), Ok(expire), Ok(minimum)) = (
                                parts[2].parse::<u32>(),
                                parts[3].parse::<u32>(),
                                parts[4].parse::<u32>(),
                                parts[5].parse::<u32>(),
                                parts[6].parse::<u32>(),
                            ) {
                                writer.write_var::<u32>(32, serial)?;
                                writer.write_var::<u32>(32, refresh)?;
                                writer.write_var::<u32>(32, retry)?;
                                writer.write_var::<u32>(32, expire)?;
                                writer.write_var::<u32>(32, minimum)?;
                                Ok(rdata)
                            } else {
                                Ok(self.rdata.clone())
                            }
                        } else {
                            Ok(self.rdata.clone())
                        }
                    }
                    super::enums::DNSResourceType::DNSKEY => {
                        // DNSKEY format: flags protocol algorithm public_key_base64
                        let parts: Vec<&str> = parsed.split(' ').collect();
                        if parts.len() == 4 {
                            if let (Ok(flags), Ok(protocol), Ok(algorithm)) = (
                                parts[0].parse::<u16>(),
                                parts[1].parse::<u8>(),
                                parts[2].parse::<u8>(),
                            ) {
                                let mut rdata = Vec::new();
                                let mut writer = BitWriter::<_, BigEndian>::new(&mut rdata);

                                writer.write_var::<u16>(16, flags)?;
                                writer.write_var::<u8>(8, protocol)?;
                                writer.write_var::<u8>(8, algorithm)?;

                                // Decode base64 public key
                                use base64::Engine;
                                if let Ok(key_bytes) =
                                    base64::engine::general_purpose::STANDARD.decode(parts[3])
                                {
                                    writer.write_bytes(&key_bytes)?;
                                    Ok(rdata)
                                } else {
                                    Ok(self.rdata.clone())
                                }
                            } else {
                                Ok(self.rdata.clone())
                            }
                        } else {
                            Ok(self.rdata.clone())
                        }
                    }
                    super::enums::DNSResourceType::DS => {
                        // DS format: key_tag algorithm digest_type digest_hex
                        let parts: Vec<&str> = parsed.split(' ').collect();
                        if parts.len() == 4 {
                            if let (Ok(key_tag), Ok(algorithm), Ok(digest_type)) = (
                                parts[0].parse::<u16>(),
                                parts[1].parse::<u8>(),
                                parts[2].parse::<u8>(),
                            ) {
                                let mut rdata = Vec::new();
                                let mut writer = BitWriter::<_, BigEndian>::new(&mut rdata);

                                writer.write_var::<u16>(16, key_tag)?;
                                writer.write_var::<u8>(8, algorithm)?;
                                writer.write_var::<u8>(8, digest_type)?;

                                // Decode hex digest
                                if let Ok(digest_bytes) = hex::decode(parts[3]) {
                                    writer.write_bytes(&digest_bytes)?;
                                    Ok(rdata)
                                } else {
                                    Ok(self.rdata.clone())
                                }
                            } else {
                                Ok(self.rdata.clone())
                            }
                        } else {
                            Ok(self.rdata.clone())
                        }
                    }
                    super::enums::DNSResourceType::TLSA => {
                        // TLSA format: cert_usage selector matching_type cert_data_hex
                        let parts: Vec<&str> = parsed.split(' ').collect();
                        if parts.len() == 4 {
                            if let (Ok(cert_usage), Ok(selector), Ok(matching_type)) = (
                                parts[0].parse::<u8>(),
                                parts[1].parse::<u8>(),
                                parts[2].parse::<u8>(),
                            ) {
                                let mut rdata = Vec::new();
                                let mut writer = BitWriter::<_, BigEndian>::new(&mut rdata);

                                writer.write_var::<u8>(8, cert_usage)?;
                                writer.write_var::<u8>(8, selector)?;
                                writer.write_var::<u8>(8, matching_type)?;

                                // Decode hex certificate data
                                if let Ok(cert_bytes) = hex::decode(parts[3]) {
                                    writer.write_bytes(&cert_bytes)?;
                                    Ok(rdata)
                                } else {
                                    Ok(self.rdata.clone())
                                }
                            } else {
                                Ok(self.rdata.clone())
                            }
                        } else {
                            Ok(self.rdata.clone())
                        }
                    }
                    super::enums::DNSResourceType::SSHFP => {
                        // SSHFP format: algorithm fp_type fingerprint_hex
                        let parts: Vec<&str> = parsed.split(' ').collect();
                        if parts.len() == 3 {
                            if let (Ok(algorithm), Ok(fp_type)) =
                                (parts[0].parse::<u8>(), parts[1].parse::<u8>())
                            {
                                let mut rdata = Vec::new();
                                let mut writer = BitWriter::<_, BigEndian>::new(&mut rdata);

                                writer.write_var::<u8>(8, algorithm)?;
                                writer.write_var::<u8>(8, fp_type)?;

                                // Decode hex fingerprint
                                if let Ok(fp_bytes) = hex::decode(parts[2]) {
                                    writer.write_bytes(&fp_bytes)?;
                                    Ok(rdata)
                                } else {
                                    Ok(self.rdata.clone())
                                }
                            } else {
                                Ok(self.rdata.clone())
                            }
                        } else {
                            Ok(self.rdata.clone())
                        }
                    }
                    _ => {
                        // For other record types, use original rdata
                        Ok(self.rdata.clone())
                    }
                }
            }
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
                    Some(format!(
                        "{}.{}.{}.{}",
                        self.rdata[0], self.rdata[1], self.rdata[2], self.rdata[3]
                    ))
                } else {
                    None
                }
            }
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
            }
            DNSResourceType::MX => {
                // MX record: priority (2 bytes) + domain name
                if self.rdata.len() >= 3 {
                    let priority = ((self.rdata[0] as u16) << 8) | (self.rdata[1] as u16);

                    // Debug: log the first few bytes to understand the pattern
                    let rdata_debug = if self.rdata.len() >= 6 {
                        format!(
                            "{:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                            self.rdata[0],
                            self.rdata[1],
                            self.rdata[2],
                            self.rdata[3],
                            self.rdata[4],
                            self.rdata[5]
                        )
                    } else {
                        format!("{:02x?}", &self.rdata)
                    };

                    // Parse the domain name starting from byte 2 (after priority)
                    // The domain name may contain compression pointers anywhere within it
                    let domain = {
                        let mut reader =
                            BitReader::<_, bitstream_io::BigEndian>::new(&self.rdata[2..]);
                        let mut temp_component = Self::default();

                        match temp_component.read_labels_with_buffer(&mut reader, Some(packet_buf))
                        {
                            Ok(labels) => labels
                                .iter()
                                .filter(|l| !l.is_empty())
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("."),
                            Err(_) => {
                                // Fall back to simple parsing
                                match self.parse_simple_domain(&self.rdata[2..]) {
                                    Ok(domain) => domain,
                                    Err(_) => format!("[unparseable_{}]", rdata_debug),
                                }
                            }
                        }
                    };

                    Some(format!("{} {}", priority, domain))
                } else {
                    None
                }
            }
            DNSResourceType::NS | DNSResourceType::CNAME | DNSResourceType::PTR => {
                // These contain just a domain name
                if self.rdata.len() >= 2 && self.rdata[0] & 0xC0 == 0xC0 {
                    // This looks like a compression pointer
                    let pointer_val = ((self.rdata[0] as u16 & 0x3F) << 8) | (self.rdata[1] as u16);

                    if (pointer_val as usize) < packet_buf.len() {
                        let mut reader = BitReader::<_, bitstream_io::BigEndian>::new(
                            &packet_buf[pointer_val as usize..],
                        );
                        let mut temp_component = Self::default();

                        match temp_component.read_labels_with_buffer(&mut reader, Some(packet_buf))
                        {
                            Ok(labels) => {
                                let domain = labels
                                    .iter()
                                    .filter(|l| !l.is_empty())
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(".");
                                Some(domain)
                            }
                            Err(_) => Some("[parse_error]".to_string()),
                        }
                    } else {
                        Some("[invalid_pointer]".to_string())
                    }
                } else {
                    // Regular domain name parsing
                    self.parse_simple_domain(&self.rdata).ok()
                }
            }
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
            }
            DNSResourceType::CAA => {
                // CAA record: Flags Tag Value
                if self.rdata.len() < 3 {
                    // Minimum: 1 byte flags + 1 byte tag length + tag
                    return Ok(());
                }

                let flags = self.rdata[0];
                let tag_length = self.rdata[1] as usize;

                if self.rdata.len() < 2 + tag_length {
                    return Ok(());
                }

                // Extract tag
                let tag = String::from_utf8(self.rdata[2..2 + tag_length].to_vec())
                    .unwrap_or_else(|_| "[invalid_tag]".to_string());

                // Extract value (rest of the data)
                let value = if self.rdata.len() > 2 + tag_length {
                    String::from_utf8(self.rdata[2 + tag_length..].to_vec())
                        .unwrap_or_else(|_| "[invalid_value]".to_string())
                } else {
                    "".to_string()
                };

                Some(format!("{} {} {}", flags, tag, value))
            }
            DNSResourceType::SRV => {
                // SRV record: Priority Weight Port Target
                if self.rdata.len() < 8 {
                    // Minimum: 2 + 2 + 2 bytes + domain
                    return Ok(());
                }

                let priority = u16::from_be_bytes([self.rdata[0], self.rdata[1]]);
                let weight = u16::from_be_bytes([self.rdata[2], self.rdata[3]]);
                let port = u16::from_be_bytes([self.rdata[4], self.rdata[5]]);

                // Parse the target domain starting from byte 6
                let target = if self.rdata.len() > 6 {
                    let mut reader = BitReader::<_, bitstream_io::BigEndian>::new(&self.rdata[6..]);
                    let mut temp_component = Self::default();

                    match temp_component.read_labels_with_buffer(&mut reader, Some(packet_buf)) {
                        Ok(labels) => labels
                            .iter()
                            .filter(|l| !l.is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("."),
                        Err(_) => {
                            // Fall back to simple parsing
                            match self.parse_simple_domain(&self.rdata[6..]) {
                                Ok(domain) => domain,
                                Err(_) => "".to_string(),
                            }
                        }
                    }
                } else {
                    "".to_string()
                };

                Some(format!("{} {} {} {}", priority, weight, port, target))
            }
            DNSResourceType::SOA => {
                // SOA record: MNAME RNAME SERIAL REFRESH RETRY EXPIRE MINIMUM
                if self.rdata.len() < 22 {
                    // Minimum: 2 domain names + 5 * 4 bytes
                    return Ok(());
                }

                let mut reader = BitReader::<_, bitstream_io::BigEndian>::new(&self.rdata[..]);
                let mut temp_component = Self::default();

                // Parse MNAME (primary name server)
                let mname =
                    match temp_component.read_labels_with_buffer(&mut reader, Some(packet_buf)) {
                        Ok(labels) => labels
                            .iter()
                            .filter(|l| !l.is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("."),
                        Err(_) => return Ok(()),
                    };

                // Parse RNAME (responsible person email)
                let rname =
                    match temp_component.read_labels_with_buffer(&mut reader, Some(packet_buf)) {
                        Ok(labels) => labels
                            .iter()
                            .filter(|l| !l.is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("."),
                        Err(_) => return Ok(()),
                    };

                // Find the position after the two domain names by parsing them manually
                let mut pos = 0;

                // Skip MNAME - parse until we hit a null terminator or compression pointer
                while pos < self.rdata.len() {
                    let len = self.rdata[pos];
                    if len == 0 {
                        pos += 1; // Skip null terminator
                        break;
                    } else if len & 0xC0 == 0xC0 {
                        pos += 2; // Skip compression pointer
                        break;
                    } else {
                        pos += 1 + (len as usize); // Skip length byte + label
                    }
                }

                // Skip RNAME - parse until we hit a null terminator or compression pointer
                while pos < self.rdata.len() {
                    let len = self.rdata[pos];
                    if len == 0 {
                        pos += 1; // Skip null terminator
                        break;
                    } else if len & 0xC0 == 0xC0 {
                        pos += 2; // Skip compression pointer
                        break;
                    } else {
                        pos += 1 + (len as usize); // Skip length byte + label
                    }
                }

                // Check we have enough data for the 5 integers
                if self.rdata.len() < pos + 20 {
                    return Ok(());
                }

                // Read the 5 32-bit integers
                let serial = u32::from_be_bytes([
                    self.rdata[pos],
                    self.rdata[pos + 1],
                    self.rdata[pos + 2],
                    self.rdata[pos + 3],
                ]);
                let refresh = u32::from_be_bytes([
                    self.rdata[pos + 4],
                    self.rdata[pos + 5],
                    self.rdata[pos + 6],
                    self.rdata[pos + 7],
                ]);
                let retry = u32::from_be_bytes([
                    self.rdata[pos + 8],
                    self.rdata[pos + 9],
                    self.rdata[pos + 10],
                    self.rdata[pos + 11],
                ]);
                let expire = u32::from_be_bytes([
                    self.rdata[pos + 12],
                    self.rdata[pos + 13],
                    self.rdata[pos + 14],
                    self.rdata[pos + 15],
                ]);
                let minimum = u32::from_be_bytes([
                    self.rdata[pos + 16],
                    self.rdata[pos + 17],
                    self.rdata[pos + 18],
                    self.rdata[pos + 19],
                ]);

                Some(format!(
                    "{} {} {} {} {} {} {}",
                    mname, rname, serial, refresh, retry, expire, minimum
                ))
            }
            DNSResourceType::DNSKEY => {
                // DNSKEY record: Flags (2) Protocol (1) Algorithm (1) Public Key
                if self.rdata.len() < 4 {
                    return Ok(());
                }

                let flags = u16::from_be_bytes([self.rdata[0], self.rdata[1]]);
                let protocol = self.rdata[2];
                let algorithm = self.rdata[3];

                // Convert public key to base64
                let public_key = if self.rdata.len() > 4 {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.encode(&self.rdata[4..])
                } else {
                    String::new()
                };

                Some(format!(
                    "{} {} {} {}",
                    flags, protocol, algorithm, public_key
                ))
            }
            DNSResourceType::RRSIG => {
                // RRSIG record: Type Covered (2) Algorithm (1) Labels (1) Original TTL (4)
                // Sig Expiration (4) Sig Inception (4) Key Tag (2) Signer's Name + Signature
                if self.rdata.len() < 18 {
                    return Ok(());
                }

                let type_covered = u16::from_be_bytes([self.rdata[0], self.rdata[1]]);
                let algorithm = self.rdata[2];
                let labels = self.rdata[3];
                let original_ttl = u32::from_be_bytes([
                    self.rdata[4],
                    self.rdata[5],
                    self.rdata[6],
                    self.rdata[7],
                ]);
                let sig_expiration = u32::from_be_bytes([
                    self.rdata[8],
                    self.rdata[9],
                    self.rdata[10],
                    self.rdata[11],
                ]);
                let sig_inception = u32::from_be_bytes([
                    self.rdata[12],
                    self.rdata[13],
                    self.rdata[14],
                    self.rdata[15],
                ]);
                let key_tag = u16::from_be_bytes([self.rdata[16], self.rdata[17]]);

                // Parse signer's name starting at byte 18
                let mut reader = BitReader::<_, bitstream_io::BigEndian>::new(&self.rdata[18..]);
                let mut temp_component = Self::default();

                let signer_name =
                    match temp_component.read_labels_with_buffer(&mut reader, Some(packet_buf)) {
                        Ok(labels) => labels
                            .iter()
                            .filter(|l| !l.is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("."),
                        Err(_) => return Ok(()),
                    };

                // Find position after signer's name
                let mut pos = 18;
                while pos < self.rdata.len() {
                    let len = self.rdata[pos];
                    if len == 0 {
                        pos += 1;
                        break;
                    } else if len & 0xC0 == 0xC0 {
                        pos += 2;
                        break;
                    } else {
                        pos += 1 + (len as usize);
                    }
                }

                // Signature is the rest
                let signature = if pos < self.rdata.len() {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.encode(&self.rdata[pos..])
                } else {
                    String::new()
                };

                Some(format!(
                    "{} {} {} {} {} {} {} {} {}",
                    type_covered,
                    algorithm,
                    labels,
                    original_ttl,
                    sig_expiration,
                    sig_inception,
                    key_tag,
                    signer_name,
                    signature
                ))
            }
            DNSResourceType::DS => {
                // DS record: Key Tag (2) Algorithm (1) Digest Type (1) Digest
                if self.rdata.len() < 4 {
                    return Ok(());
                }

                let key_tag = u16::from_be_bytes([self.rdata[0], self.rdata[1]]);
                let algorithm = self.rdata[2];
                let digest_type = self.rdata[3];

                // Convert digest to hex
                let digest = if self.rdata.len() > 4 {
                    hex::encode(&self.rdata[4..])
                } else {
                    String::new()
                };

                Some(format!(
                    "{} {} {} {}",
                    key_tag, algorithm, digest_type, digest
                ))
            }
            DNSResourceType::NSEC => {
                // NSEC record: Next Domain Name + Type Bit Maps
                if self.rdata.is_empty() {
                    return Ok(());
                }

                // Parse next domain name
                let mut reader = BitReader::<_, bitstream_io::BigEndian>::new(&self.rdata[..]);
                let mut temp_component = Self::default();

                let next_domain =
                    match temp_component.read_labels_with_buffer(&mut reader, Some(packet_buf)) {
                        Ok(labels) => labels
                            .iter()
                            .filter(|l| !l.is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("."),
                        Err(_) => return Ok(()),
                    };

                // Find position after domain name
                let mut pos = 0;
                while pos < self.rdata.len() {
                    let len = self.rdata[pos];
                    if len == 0 {
                        pos += 1;
                        break;
                    } else if len & 0xC0 == 0xC0 {
                        pos += 2;
                        break;
                    } else {
                        pos += 1 + (len as usize);
                    }
                }

                // Parse type bit maps
                let mut types = Vec::new();
                while pos < self.rdata.len() {
                    if pos + 2 > self.rdata.len() {
                        break;
                    }

                    let window = self.rdata[pos];
                    let bitmap_len = self.rdata[pos + 1] as usize;
                    pos += 2;

                    if pos + bitmap_len > self.rdata.len() {
                        break;
                    }

                    // Process bitmap
                    for i in 0..bitmap_len {
                        let byte = self.rdata[pos + i];
                        for bit in 0..8 {
                            if byte & (0x80 >> bit) != 0 {
                                let type_num = (window as u16) * 256 + (i as u16) * 8 + bit;
                                types.push(type_num);
                            }
                        }
                    }
                    pos += bitmap_len;
                }

                let types_str = types
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");

                Some(format!("{} {}", next_domain, types_str))
            }
            DNSResourceType::NSEC3 => {
                // NSEC3 record: Hash Algorithm (1) Flags (1) Iterations (2) Salt Length (1) Salt
                // Next Hashed Owner Name Length (1) Next Hashed Owner Name + Type Bit Maps
                if self.rdata.len() < 5 {
                    return Ok(());
                }

                let hash_algorithm = self.rdata[0];
                let flags = self.rdata[1];
                let iterations = u16::from_be_bytes([self.rdata[2], self.rdata[3]]);
                let salt_length = self.rdata[4] as usize;

                if self.rdata.len() < 5 + salt_length + 1 {
                    return Ok(());
                }

                let salt = if salt_length > 0 {
                    hex::encode(&self.rdata[5..5 + salt_length])
                } else {
                    "-".to_string()
                };

                let hash_length_pos = 5 + salt_length;
                let hash_length = self.rdata[hash_length_pos] as usize;

                if self.rdata.len() < hash_length_pos + 1 + hash_length {
                    return Ok(());
                }

                let next_hashed = base32::encode(
                    base32::Alphabet::Rfc4648 { padding: false },
                    &self.rdata[hash_length_pos + 1..hash_length_pos + 1 + hash_length],
                )
                .to_lowercase();

                // Parse type bit maps
                let mut pos = hash_length_pos + 1 + hash_length;
                let mut types = Vec::new();

                while pos < self.rdata.len() {
                    if pos + 2 > self.rdata.len() {
                        break;
                    }

                    let window = self.rdata[pos];
                    let bitmap_len = self.rdata[pos + 1] as usize;
                    pos += 2;

                    if pos + bitmap_len > self.rdata.len() {
                        break;
                    }

                    // Process bitmap
                    for i in 0..bitmap_len {
                        let byte = self.rdata[pos + i];
                        for bit in 0..8 {
                            if byte & (0x80 >> bit) != 0 {
                                let type_num = (window as u16) * 256 + (i as u16) * 8 + bit;
                                types.push(type_num);
                            }
                        }
                    }
                    pos += bitmap_len;
                }

                let types_str = types
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");

                Some(format!(
                    "{} {} {} {} {} {}",
                    hash_algorithm, flags, iterations, salt, next_hashed, types_str
                ))
            }
            DNSResourceType::TLSA => {
                // TLSA record: Certificate Usage (1) Selector (1) Matching Type (1) Certificate Association Data
                if self.rdata.len() < 3 {
                    return Ok(());
                }

                let cert_usage = self.rdata[0];
                let selector = self.rdata[1];
                let matching_type = self.rdata[2];

                // Certificate association data (hash or full certificate)
                let cert_data = if self.rdata.len() > 3 {
                    hex::encode(&self.rdata[3..])
                } else {
                    String::new()
                };

                Some(format!(
                    "{} {} {} {}",
                    cert_usage, selector, matching_type, cert_data
                ))
            }
            DNSResourceType::SSHFP => {
                // SSHFP record: Algorithm (1) Fingerprint Type (1) Fingerprint
                if self.rdata.len() < 2 {
                    return Ok(());
                }

                let algorithm = self.rdata[0];
                let fp_type = self.rdata[1];

                // Fingerprint (hex encoded)
                let fingerprint = if self.rdata.len() > 2 {
                    hex::encode(&self.rdata[2..])
                } else {
                    String::new()
                };

                Some(format!("{} {} {}", algorithm, fp_type, fingerprint))
            }
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
