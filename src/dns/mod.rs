pub mod common;
pub mod edns;
pub mod enums;
pub mod header;
pub mod question;
pub mod resource;

use bitstream_io::{BigEndian, BitReader, BitWriter, BitWrite};
use common::PacketComponent;
use edns::EdnsOpt;
use header::DNSHeader;
use question::DNSQuestion;
use resource::DNSResource;
use tracing::{debug, trace};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DNSPacket {
    pub header: DNSHeader,
    pub questions: Vec<DNSQuestion>,
    pub answers: Vec<DNSResource>,
    pub authorities: Vec<DNSResource>,
    pub resources: Vec<DNSResource>,
    /// EDNS0 OPT record if present (extracted from additional records)
    pub edns: Option<EdnsOpt>,
}

#[derive(Debug)]
pub enum ParseError {
    InvalidHeader,
    InvalidLabel,
    InvalidQuestionSection,
    InvalidAnswerSection,
    InvalidAuthoritySection,
    InvalidAdditionalSection,
    InvalidBitStream(String),
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::InvalidBitStream(e.to_string())
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidHeader => write!(f, "Invalid DNS header"),
            ParseError::InvalidLabel => write!(f, "Invalid DNS label"),
            ParseError::InvalidQuestionSection => write!(f, "Invalid question section"),
            ParseError::InvalidAnswerSection => write!(f, "Invalid answer section"),
            ParseError::InvalidAuthoritySection => write!(f, "Invalid authority section"),
            ParseError::InvalidAdditionalSection => write!(f, "Invalid additional section"),
            ParseError::InvalidBitStream(e) => write!(f, "Invalid bit stream: {}", e),
        }
    }
}

impl std::error::Error for ParseError {}

impl DNSPacket {
    pub fn valid(&self) -> bool {
        // Basic validation checks

        // Check header counts match actual sections
        if self.header.qdcount as usize != self.questions.len() {
            return false;
        }
        if self.header.ancount as usize != self.answers.len() {
            return false;
        }
        if self.header.nscount as usize != self.authorities.len() {
            return false;
        }
        if self.header.arcount as usize != self.resources.len() {
            return false;
        }

        // Check that questions have valid labels
        for question in &self.questions {
            if question.labels.is_empty() {
                return false;
            }

            // Check for valid domain name structure
            let total_length: usize = question.labels.iter().map(|l| l.len() + 1).sum();
            if total_length > 255 {
                // DNS names can't exceed 255 octets
                return false;
            }

            // Check individual label lengths
            for label in &question.labels {
                if label.len() > 63 {
                    // Individual labels can't exceed 63 octets
                    return false;
                }
            }
        }

        // Check opcode is valid (0-2 are standard)
        if self.header.opcode > 2 {
            return false;
        }

        // Check rcode is valid (0-5 are standard response codes)
        if self.header.rcode > 5 {
            return false;
        }

        true
    }

    pub fn parse(buf: &[u8]) -> Result<Self, ParseError> {
        trace!("Parsing DNS packet, size: {} bytes", buf.len());
        let mut reader = BitReader::<_, BigEndian>::new(buf);
        let mut packet = DNSPacket::default();
        packet.header.read(&mut reader)?;
        debug!(
            "Parsed DNS header: id={}, qr={}, opcode={}, questions={}",
            packet.header.id, packet.header.qr, packet.header.opcode, packet.header.qdcount
        );
        for _ in 0..packet.header.qdcount {
            let mut question = DNSQuestion::default();
            question.read_with_buffer(&mut reader, buf)?;
            packet.questions.push(question);
        }

        for _ in 0..packet.header.ancount {
            let mut answer = DNSResource::default();
            answer.read_with_buffer(&mut reader, buf)?;
            packet.answers.push(answer);
        }

        for _ in 0..packet.header.nscount {
            let mut authority = DNSResource::default();
            authority.read_with_buffer(&mut reader, buf)?;
            packet.authorities.push(authority);
        }

        for _ in 0..packet.header.arcount {
            let mut resource = DNSResource::default();
            resource.read_with_buffer(&mut reader, buf)?;
            
            // Check if this is an EDNS OPT record
            if resource.rtype == enums::DNSResourceType::OPT {
                // Check for proper OPT record format (root domain - can be empty array or array with empty string)
                let is_root_domain = resource.labels.is_empty() || 
                    (resource.labels.len() == 1 && resource.labels[0].is_empty());
                
                if is_root_domain {
                    // This is an EDNS OPT pseudo-record
                    // For EDNS OPT records, the class field contains UDP payload size (not a standard DNS class)
                    let udp_payload_size = resource.raw_class.unwrap_or(512);
                    
                    match EdnsOpt::parse_from_resource(
                        udp_payload_size,
                        resource.ttl,
                        &resource.rdata
                    ) {
                        Ok(edns_opt) => {
                            debug!("Parsed EDNS0 record: {}", edns_opt.debug_info());
                            packet.edns = Some(edns_opt);
                            // Don't add OPT record to additional resources as it's handled separately
                            continue;
                        },
                        Err(e) => {
                            debug!("Failed to parse EDNS OPT record: {:?}", e);
                            // Fall back to treating it as a regular resource
                        }
                    }
                }
            }
            
            packet.resources.push(resource);
        }

        Ok(packet)
    }

    pub fn serialize(&self) -> Result<Vec<u8>, ParseError> {
        let mut buf = Vec::new();
        let mut writer: BitWriter<&mut Vec<u8>, BigEndian> = BitWriter::new(&mut buf);

        // Calculate actual header counts including EDNS
        let mut header = self.header.clone();
        if self.edns.is_some() {
            header.arcount = self.resources.len() as u16 + 1; // +1 for EDNS OPT record
        }

        // Write header
        header.write(&mut writer)?;

        // Write questions
        for question in self.questions.iter() {
            question.write(&mut writer)?;
        }

        // Write answers
        for answer in self.answers.iter() {
            answer.write(&mut writer)?;
        }

        // Write authorities
        for authority in self.authorities.iter() {
            authority.write(&mut writer)?;
        }

        // Write additional resources
        for resource in self.resources.iter() {
            resource.write(&mut writer)?;
        }

        // Write EDNS OPT record if present
        if let Some(edns) = &self.edns {
            let (udp_payload_size, ttl, rdata) = edns.to_resource_format();
            
            // Write EDNS OPT record directly (bypass normal resource write to handle special class field)
            // NAME: Root domain (empty) - just write a zero byte
            writer.write_var::<u8>(8, 0)?;
            
            // TYPE: OPT (41)
            writer.write_var::<u16>(16, 41)?;
            
            // CLASS: UDP payload size (not a standard DNS class)
            writer.write_var::<u16>(16, udp_payload_size)?;
            
            // TTL: Contains extended RCODE, version, and flags
            writer.write_var::<u32>(32, ttl)?;
            
            // RDLENGTH: Length of option data
            writer.write_var::<u16>(16, rdata.len() as u16)?;
            
            // RDATA: Option data
            writer.write_bytes(&rdata)?;
        }

        Ok(buf)
    }

    pub fn generate_response(&self) -> Self {
        let mut packet = self.clone();
        packet.header.qr = true;
        packet.header.ra = true;
        for question in &packet.questions {
            let name = question
                .labels
                .iter()
                .filter(|l| l.len() > 0)
                .map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if name.is_empty() {
                continue;
            }
            debug!("DNS query for: {}", name);
        }
        packet
    }

    /// Get the maximum UDP payload size from EDNS or use default
    pub fn max_udp_payload_size(&self) -> u16 {
        self.edns.as_ref()
            .map(|edns| edns.payload_size())
            .unwrap_or(512) // Default DNS UDP payload size
    }

    /// Check if the query supports EDNS
    pub fn supports_edns(&self) -> bool {
        self.edns.is_some()
    }

    /// Check if DNSSEC is requested (DO flag)
    pub fn dnssec_requested(&self) -> bool {
        self.edns.as_ref()
            .map(|edns| edns.do_flag())
            .unwrap_or(false)
    }

    /// Add or update EDNS support in the packet
    pub fn add_edns(&mut self, payload_size: u16, do_flag: bool) {
        let mut edns = EdnsOpt::with_payload_size(payload_size);
        edns.set_do_flag(do_flag);
        self.edns = Some(edns);
    }

    /// Remove EDNS support from the packet
    pub fn remove_edns(&mut self) {
        self.edns = None;
    }

    /// Get EDNS extended RCODE if available
    pub fn extended_rcode(&self) -> Option<u8> {
        self.edns.as_ref().map(|edns| edns.extended_rcode)
    }

    /// Set EDNS extended RCODE
    pub fn set_extended_rcode(&mut self, rcode: u8) {
        if let Some(edns) = &mut self.edns {
            edns.extended_rcode = rcode;
        }
    }

    /// Get a debug string for EDNS information
    pub fn edns_debug_info(&self) -> String {
        match &self.edns {
            Some(edns) => edns.debug_info(),
            None => "No EDNS support".to_string(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_dns_packet() {
        let packet = DNSPacket::default();
        assert_eq!(packet.valid(), true); // Default packet should be valid
    }
}
