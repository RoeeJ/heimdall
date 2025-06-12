use crate::dns::enums::{DNSResourceClass, DNSResourceType};
use crate::dns::resource::DNSResource;

/// A zone record represents a single resource record in a zone file
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneRecord {
    /// Domain name (relative to zone origin or FQDN)
    pub name: String,
    /// Time to live in seconds
    pub ttl: Option<u32>,
    /// Record class (usually IN)
    pub class: DNSResourceClass,
    /// Record type (A, AAAA, MX, etc.)
    pub rtype: DNSResourceType,
    /// Record data in text format
    pub rdata: String,
}

impl ZoneRecord {
    /// Create a new zone record
    pub fn new(
        name: String,
        ttl: Option<u32>,
        class: DNSResourceClass,
        rtype: DNSResourceType,
        rdata: String,
    ) -> Self {
        Self {
            name,
            ttl,
            class,
            rtype,
            rdata,
        }
    }

    /// Convert to DNS resource record with given origin and default TTL
    pub fn to_dns_resource(&self, origin: &str, default_ttl: u32) -> Result<DNSResource, String> {
        // Normalize the domain name
        let full_name = self.normalize_name(origin)?;

        // Split into labels
        let labels: Vec<String> = full_name
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Use provided TTL or default
        let ttl = self.ttl.unwrap_or(default_ttl);

        // Parse rdata based on record type
        let (rdata_bytes, parsed_rdata) = self.parse_rdata()?;

        Ok(DNSResource {
            labels,
            rtype: self.rtype,
            rclass: self.class,
            ttl,
            rdlength: rdata_bytes.len() as u16,
            rdata: rdata_bytes,
            parsed_rdata: Some(parsed_rdata),
            raw_class: None,
        })
    }

    /// Normalize domain name relative to origin
    fn normalize_name(&self, origin: &str) -> Result<String, String> {
        let name = self.name.trim();

        if name == "@" {
            // @ represents the zone origin
            Ok(origin.to_string())
        } else if name.ends_with('.') {
            // Already fully qualified
            Ok(name.trim_end_matches('.').to_string())
        } else if name.is_empty() {
            // Empty name means origin
            Ok(origin.to_string())
        } else {
            // Relative name - append origin
            Ok(format!("{}.{}", name, origin.trim_end_matches('.')))
        }
    }

    /// Parse rdata from text format to bytes
    fn parse_rdata(&self) -> Result<(Vec<u8>, String), String> {
        match self.rtype {
            DNSResourceType::A => self.parse_a_record(),
            DNSResourceType::AAAA => self.parse_aaaa_record(),
            DNSResourceType::NS => self.parse_ns_record(),
            DNSResourceType::CNAME => self.parse_cname_record(),
            DNSResourceType::SOA => self.parse_soa_record(),
            DNSResourceType::PTR => self.parse_ptr_record(),
            DNSResourceType::MX => self.parse_mx_record(),
            DNSResourceType::TXT => self.parse_txt_record(),
            DNSResourceType::SRV => self.parse_srv_record(),
            DNSResourceType::CAA => self.parse_caa_record(),
            _ => Err(format!(
                "Unsupported record type for zone files: {:?}",
                self.rtype
            )),
        }
    }

    /// Parse A record (IPv4 address)
    fn parse_a_record(&self) -> Result<(Vec<u8>, String), String> {
        use std::net::Ipv4Addr;

        let addr: Ipv4Addr = self
            .rdata
            .parse()
            .map_err(|_| format!("Invalid IPv4 address: {}", self.rdata))?;

        Ok((addr.octets().to_vec(), self.rdata.clone()))
    }

    /// Parse AAAA record (IPv6 address)
    fn parse_aaaa_record(&self) -> Result<(Vec<u8>, String), String> {
        use std::net::Ipv6Addr;

        let addr: Ipv6Addr = self
            .rdata
            .parse()
            .map_err(|_| format!("Invalid IPv6 address: {}", self.rdata))?;

        Ok((addr.octets().to_vec(), self.rdata.clone()))
    }

    /// Parse NS record (name server)
    fn parse_ns_record(&self) -> Result<(Vec<u8>, String), String> {
        self.encode_domain_name(&self.rdata)
    }

    /// Parse CNAME record (canonical name)
    fn parse_cname_record(&self) -> Result<(Vec<u8>, String), String> {
        self.encode_domain_name(&self.rdata)
    }

    /// Parse PTR record (pointer)
    fn parse_ptr_record(&self) -> Result<(Vec<u8>, String), String> {
        self.encode_domain_name(&self.rdata)
    }

    /// Parse SOA record
    fn parse_soa_record(&self) -> Result<(Vec<u8>, String), String> {
        // SOA format: mname rname serial refresh retry expire minimum
        let parts: Vec<&str> = self.rdata.split_whitespace().collect();
        if parts.len() != 7 {
            return Err(format!("SOA record requires 7 fields, got {}", parts.len()));
        }

        let mut rdata = Vec::new();

        // Encode MNAME (primary name server)
        let (mname_bytes, _) = self.encode_domain_name(parts[0])?;
        rdata.extend_from_slice(&mname_bytes);

        // Encode RNAME (responsible person email)
        let (rname_bytes, _) = self.encode_domain_name(parts[1])?;
        rdata.extend_from_slice(&rname_bytes);

        // Parse and encode 32-bit values
        for part in parts.iter().take(7).skip(2) {
            let value: u32 = part
                .parse()
                .map_err(|_| format!("Invalid SOA numeric value: {}", part))?;
            rdata.extend_from_slice(&value.to_be_bytes());
        }

        Ok((rdata, self.rdata.clone()))
    }

    /// Parse MX record
    fn parse_mx_record(&self) -> Result<(Vec<u8>, String), String> {
        // MX format: priority exchange
        let parts: Vec<&str> = self.rdata.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(format!("MX record requires 2 fields, got {}", parts.len()));
        }

        let priority: u16 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid MX priority: {}", parts[0]))?;

        let mut rdata = Vec::new();
        rdata.extend_from_slice(&priority.to_be_bytes());

        let (exchange_bytes, _) = self.encode_domain_name(parts[1])?;
        rdata.extend_from_slice(&exchange_bytes);

        Ok((rdata, self.rdata.clone()))
    }

    /// Parse TXT record
    fn parse_txt_record(&self) -> Result<(Vec<u8>, String), String> {
        // TXT records can contain multiple strings
        // For now, treat the entire rdata as a single string
        let text = self.rdata.trim_matches('"');

        let mut rdata = Vec::new();

        // Split into 255-byte chunks if necessary
        for chunk in text.as_bytes().chunks(255) {
            rdata.push(chunk.len() as u8);
            rdata.extend_from_slice(chunk);
        }

        Ok((rdata, format!("\"{}\"", text)))
    }

    /// Parse SRV record
    fn parse_srv_record(&self) -> Result<(Vec<u8>, String), String> {
        // SRV format: priority weight port target
        let parts: Vec<&str> = self.rdata.split_whitespace().collect();
        if parts.len() != 4 {
            return Err(format!("SRV record requires 4 fields, got {}", parts.len()));
        }

        let priority: u16 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid SRV priority: {}", parts[0]))?;
        let weight: u16 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid SRV weight: {}", parts[1]))?;
        let port: u16 = parts[2]
            .parse()
            .map_err(|_| format!("Invalid SRV port: {}", parts[2]))?;

        let mut rdata = Vec::new();
        rdata.extend_from_slice(&priority.to_be_bytes());
        rdata.extend_from_slice(&weight.to_be_bytes());
        rdata.extend_from_slice(&port.to_be_bytes());

        let (target_bytes, _) = self.encode_domain_name(parts[3])?;
        rdata.extend_from_slice(&target_bytes);

        Ok((rdata, self.rdata.clone()))
    }

    /// Parse CAA record
    fn parse_caa_record(&self) -> Result<(Vec<u8>, String), String> {
        // CAA format: flags tag value
        let parts: Vec<&str> = self.rdata.splitn(3, ' ').collect();
        if parts.len() != 3 {
            return Err(format!("CAA record requires 3 fields, got {}", parts.len()));
        }

        let flags: u8 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid CAA flags: {}", parts[0]))?;

        let mut rdata = Vec::new();
        rdata.push(flags);

        // Tag length and tag
        let tag = parts[1];
        rdata.push(tag.len() as u8);
        rdata.extend_from_slice(tag.as_bytes());

        // Value
        let value = parts[2].trim_matches('"');
        rdata.extend_from_slice(value.as_bytes());

        Ok((rdata, self.rdata.clone()))
    }

    /// Encode a domain name to DNS wire format
    fn encode_domain_name(&self, name: &str) -> Result<(Vec<u8>, String), String> {
        let mut encoded = Vec::new();

        let normalized = if name.ends_with('.') {
            name.trim_end_matches('.')
        } else {
            name
        };

        for label in normalized.split('.') {
            if label.is_empty() {
                continue;
            }
            if label.len() > 63 {
                return Err(format!("Label too long: {}", label));
            }
            encoded.push(label.len() as u8);
            encoded.extend_from_slice(label.as_bytes());
        }

        encoded.push(0); // Root label

        Ok((encoded, name.to_string()))
    }
}
