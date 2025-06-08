use super::ParseError;

/// EDNS0 OPT pseudo-record implementation
/// RFC 6891: https://tools.ietf.org/html/rfc6891
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EdnsOpt {
    /// UDP payload size that can be handled by the requestor
    pub udp_payload_size: u16,
    /// Extended RCODE (high 8 bits)
    pub extended_rcode: u8,
    /// EDNS version (currently 0)
    pub version: u8,
    /// EDNS flags (16 bits)
    pub flags: u16,
    /// Variable length RDATA containing EDNS options
    pub options: Vec<EdnsOption>,
}

/// EDNS option structure
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdnsOption {
    /// Option code (2 bytes)
    pub code: u16,
    /// Option data
    pub data: Vec<u8>,
}

/// Common EDNS option codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdnsOptionCode {
    /// Local option (RFC 6891)
    Local = 65001,
    /// Name Server Identifier (RFC 5001)
    Nsid = 3,
    /// DNS Authentication of Named Entities (RFC 6844)
    Dau = 5,
    /// DNS Hash Authentication of Named Entities (RFC 6844)
    Dhu = 6,
    /// DNS Signature Authentication of Named Entities (RFC 6844)  
    N3u = 7,
    /// Client Subnet (RFC 7871)
    ClientSubnet = 8,
    /// DNS Cookies (RFC 7873)
    Cookie = 10,
    /// TCP Keepalive (RFC 7828)
    TcpKeepalive = 11,
    /// Padding (RFC 7830)
    Padding = 12,
    /// Chain Query (RFC 7901)
    Chain = 13,
    /// Unknown option
    Unknown,
}

impl From<u16> for EdnsOptionCode {
    fn from(value: u16) -> Self {
        match value {
            3 => EdnsOptionCode::Nsid,
            5 => EdnsOptionCode::Dau,
            6 => EdnsOptionCode::Dhu,
            7 => EdnsOptionCode::N3u,
            8 => EdnsOptionCode::ClientSubnet,
            10 => EdnsOptionCode::Cookie,
            11 => EdnsOptionCode::TcpKeepalive,
            12 => EdnsOptionCode::Padding,
            13 => EdnsOptionCode::Chain,
            65001 => EdnsOptionCode::Local,
            _ => EdnsOptionCode::Unknown,
        }
    }
}

impl From<EdnsOptionCode> for u16 {
    fn from(code: EdnsOptionCode) -> Self {
        match code {
            EdnsOptionCode::Nsid => 3,
            EdnsOptionCode::Dau => 5,
            EdnsOptionCode::Dhu => 6,
            EdnsOptionCode::N3u => 7,
            EdnsOptionCode::ClientSubnet => 8,
            EdnsOptionCode::Cookie => 10,
            EdnsOptionCode::TcpKeepalive => 11,
            EdnsOptionCode::Padding => 12,
            EdnsOptionCode::Chain => 13,
            EdnsOptionCode::Local => 65001,
            EdnsOptionCode::Unknown => 0,
        }
    }
}

impl EdnsOpt {
    /// Create a new EDNS OPT record with default values
    pub fn new() -> Self {
        Self {
            udp_payload_size: 4096, // Default UDP payload size
            extended_rcode: 0,
            version: 0, // EDNS version 0
            flags: 0,
            options: Vec::new(),
        }
    }

    /// Create an EDNS OPT record with specified UDP payload size
    pub fn with_payload_size(payload_size: u16) -> Self {
        Self {
            udp_payload_size: payload_size,
            ..Self::new()
        }
    }

    /// Check if DNSSEC OK (DO) flag is set
    pub fn do_flag(&self) -> bool {
        (self.flags & 0x8000) != 0
    }

    /// Set the DNSSEC OK (DO) flag
    pub fn set_do_flag(&mut self, value: bool) {
        if value {
            self.flags |= 0x8000;
        } else {
            self.flags &= !0x8000;
        }
    }

    /// Get the UDP payload size
    pub fn payload_size(&self) -> u16 {
        self.udp_payload_size
    }

    /// Set the UDP payload size
    pub fn set_payload_size(&mut self, size: u16) {
        self.udp_payload_size = size;
    }

    /// Add an EDNS option
    pub fn add_option(&mut self, code: u16, data: Vec<u8>) {
        self.options.push(EdnsOption { code, data });
    }

    /// Find an option by code
    pub fn find_option(&self, code: u16) -> Option<&EdnsOption> {
        self.options.iter().find(|opt| opt.code == code)
    }

    /// Parse EDNS OPT record from DNS resource record data
    /// The OPT record uses the following format:
    /// - NAME: Root domain (empty)
    /// - TYPE: OPT (41)
    /// - CLASS: UDP payload size (16 bits)
    /// - TTL: Extended RCODE (8 bits) | Version (8 bits) | Flags (16 bits)
    /// - RDLENGTH: Length of option data
    /// - RDATA: Option data
    pub fn parse_from_resource(class: u16, ttl: u32, rdata: &[u8]) -> Result<Self, ParseError> {
        let udp_payload_size = class;
        let extended_rcode = ((ttl >> 24) & 0xFF) as u8;
        let version = ((ttl >> 16) & 0xFF) as u8;
        let flags = (ttl & 0xFFFF) as u16;

        // Parse options from RDATA
        let mut options = Vec::new();
        let mut pos = 0;

        while pos < rdata.len() {
            if pos + 4 > rdata.len() {
                break; // Not enough data for option header
            }

            let option_code = ((rdata[pos] as u16) << 8) | (rdata[pos + 1] as u16);
            let option_length = ((rdata[pos + 2] as u16) << 8) | (rdata[pos + 3] as u16);
            pos += 4;

            if pos + option_length as usize > rdata.len() {
                return Err(ParseError::InvalidLabel); // Invalid option length
            }

            let option_data = rdata[pos..pos + option_length as usize].to_vec();
            pos += option_length as usize;

            options.push(EdnsOption {
                code: option_code,
                data: option_data,
            });
        }

        Ok(EdnsOpt {
            udp_payload_size,
            extended_rcode,
            version,
            flags,
            options,
        })
    }

    /// Serialize EDNS OPT record to resource record format
    pub fn to_resource_format(&self) -> (u16, u32, Vec<u8>) {
        let class = self.udp_payload_size;
        let ttl = ((self.extended_rcode as u32) << 24)
            | ((self.version as u32) << 16)
            | (self.flags as u32);

        // Serialize options to RDATA
        let mut rdata = Vec::new();
        for option in &self.options {
            rdata.extend_from_slice(&option.code.to_be_bytes());
            rdata.extend_from_slice(&(option.data.len() as u16).to_be_bytes());
            rdata.extend_from_slice(&option.data);
        }

        (class, ttl, rdata)
    }

    /// Check if this is a valid EDNS0 record
    pub fn is_valid(&self) -> bool {
        // EDNS version must be 0 for EDNS0
        self.version == 0
    }

    /// Get a human-readable description of EDNS flags
    pub fn flags_description(&self) -> String {
        let mut flags = Vec::new();

        if self.do_flag() {
            flags.push("DO".to_string());
        }

        if flags.is_empty() {
            "none".to_string()
        } else {
            flags.join(",")
        }
    }

    /// Get a debug string representation
    pub fn debug_info(&self) -> String {
        format!(
            "EDNS0: payload_size={}, version={}, flags=0x{:04x} ({}), options={}",
            self.udp_payload_size,
            self.version,
            self.flags,
            self.flags_description(),
            self.options.len()
        )
    }
}

impl std::fmt::Display for EdnsOpt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.debug_info())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edns_opt_creation() {
        let opt = EdnsOpt::new();
        assert_eq!(opt.udp_payload_size, 4096);
        assert_eq!(opt.version, 0);
        assert_eq!(opt.extended_rcode, 0);
        assert_eq!(opt.flags, 0);
        assert!(!opt.do_flag());
    }

    #[test]
    fn test_do_flag() {
        let mut opt = EdnsOpt::new();
        assert!(!opt.do_flag());

        opt.set_do_flag(true);
        assert!(opt.do_flag());
        assert_eq!(opt.flags & 0x8000, 0x8000);

        opt.set_do_flag(false);
        assert!(!opt.do_flag());
        assert_eq!(opt.flags & 0x8000, 0);
    }

    #[test]
    fn test_resource_format_conversion() {
        let mut opt = EdnsOpt::with_payload_size(1232);
        opt.set_do_flag(true);
        opt.add_option(3, vec![0x01, 0x02, 0x03]); // NSID option

        let (class, ttl, rdata) = opt.to_resource_format();

        assert_eq!(class, 1232);
        assert_eq!(ttl & 0xFFFF, 0x8000); // DO flag set
        assert!(!rdata.is_empty());

        // Test round-trip
        let parsed = EdnsOpt::parse_from_resource(class, ttl, &rdata).unwrap();
        assert_eq!(parsed.udp_payload_size, 1232);
        assert!(parsed.do_flag());
        assert_eq!(parsed.options.len(), 1);
        assert_eq!(parsed.options[0].code, 3);
        assert_eq!(parsed.options[0].data, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_option_handling() {
        let mut opt = EdnsOpt::new();
        opt.add_option(8, vec![1, 2, 3, 4]); // Client Subnet
        opt.add_option(10, vec![5, 6]); // Cookie

        assert_eq!(opt.options.len(), 2);

        let client_subnet = opt.find_option(8).unwrap();
        assert_eq!(client_subnet.data, vec![1, 2, 3, 4]);

        let cookie = opt.find_option(10).unwrap();
        assert_eq!(cookie.data, vec![5, 6]);

        assert!(opt.find_option(99).is_none());
    }
}
