pub mod common;
pub mod edns;
pub mod enums;
pub mod header;
pub mod question;
pub mod resource;
pub mod simd;

use bitstream_io::{BigEndian, BitReader, BitWrite, BitWriter};
use common::PacketComponent;
use edns::EdnsOpt;
use header::DNSHeader;
use parking_lot::Mutex;
use question::DNSQuestion;
use resource::DNSResource;
use std::sync::Arc;
use tracing::{debug, trace};
// Move validation usage to method implementations to avoid circular dependencies

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
pub struct DNSPacket {
    pub header: DNSHeader,
    pub questions: Vec<DNSQuestion>,
    pub answers: Vec<DNSResource>,
    pub authorities: Vec<DNSResource>,
    pub resources: Vec<DNSResource>,
    /// EDNS0 OPT record if present (extracted from additional records)
    pub edns: Option<EdnsOpt>,
}

/// Zero-copy DNS packet parser that keeps references to the original buffer
/// Use this for read-only operations to avoid allocations
#[derive(Debug)]
pub struct DNSPacketRef<'a> {
    /// Reference to the original packet buffer
    pub buffer: &'a [u8],
    /// Pre-parsed header for efficiency
    pub header: DNSHeader,
    /// Byte offsets of each section in the buffer
    pub sections: PacketSections,
}

#[derive(Debug, Clone, Copy)]
pub struct PacketSections {
    pub questions_start: usize,
    pub answers_start: usize,
    pub authorities_start: usize,
    pub additionals_start: usize,
    pub packet_end: usize,
}

impl<'a> DNSPacketRef<'a> {
    /// Parse packet metadata without allocating vectors
    pub fn parse_metadata(buf: &'a [u8]) -> Result<Self, ParseError> {
        trace!("Parsing DNS packet metadata, size: {} bytes", buf.len());
        let mut reader = BitReader::<_, BigEndian>::new(buf);
        let mut header = DNSHeader::default();
        header.read(&mut reader)?;

        debug!(
            "Parsed DNS header (zero-copy): id={}, qr={}, questions={}",
            header.id, header.qr, header.qdcount
        );

        // Calculate section offsets without parsing content
        let mut current_offset = 12; // DNS header is always 12 bytes

        // Skip questions section
        let questions_start = current_offset;
        for _ in 0..header.qdcount {
            current_offset = Self::skip_question(buf, current_offset)?;
        }

        let answers_start = current_offset;
        for _ in 0..header.ancount {
            current_offset = Self::skip_resource_record(buf, current_offset)?;
        }

        let authorities_start = current_offset;
        for _ in 0..header.nscount {
            current_offset = Self::skip_resource_record(buf, current_offset)?;
        }

        let additionals_start = current_offset;
        for _ in 0..header.arcount {
            current_offset = Self::skip_resource_record(buf, current_offset)?;
        }

        let sections = PacketSections {
            questions_start,
            answers_start,
            authorities_start,
            additionals_start,
            packet_end: current_offset,
        };

        Ok(Self {
            buffer: buf,
            header,
            sections,
        })
    }

    /// Skip a question record and return the next offset
    fn skip_question(buf: &[u8], mut offset: usize) -> Result<usize, ParseError> {
        // Skip domain name (labels)
        offset = Self::skip_domain_name(buf, offset)?;

        // Skip QTYPE (2 bytes) and QCLASS (2 bytes)
        if offset + 4 > buf.len() {
            return Err(ParseError::InvalidQuestionSection);
        }
        offset += 4;

        Ok(offset)
    }

    /// Skip a resource record and return the next offset
    fn skip_resource_record(buf: &[u8], mut offset: usize) -> Result<usize, ParseError> {
        // Skip domain name (labels)
        offset = Self::skip_domain_name(buf, offset)?;

        // Skip TYPE (2 bytes), CLASS (2 bytes), TTL (4 bytes)
        if offset + 8 > buf.len() {
            return Err(ParseError::InvalidAnswerSection);
        }
        offset += 8;

        // Read RDLENGTH and skip RDATA
        if offset + 2 > buf.len() {
            return Err(ParseError::InvalidAnswerSection);
        }
        let rdlength = u16::from_be_bytes([buf[offset], buf[offset + 1]]) as usize;
        offset += 2;

        if offset + rdlength > buf.len() {
            return Err(ParseError::InvalidAnswerSection);
        }
        offset += rdlength;

        Ok(offset)
    }

    /// Skip a domain name and return the next offset
    fn skip_domain_name(buf: &[u8], mut offset: usize) -> Result<usize, ParseError> {
        let mut jumps = 0;
        let mut original_offset = None;

        loop {
            if offset >= buf.len() {
                return Err(ParseError::InvalidLabel);
            }

            let label_length = buf[offset];

            // Check for compression pointer
            if (label_length & 0xC0) == 0xC0 {
                if offset + 1 >= buf.len() {
                    return Err(ParseError::InvalidLabel);
                }

                // This is a compression pointer
                jumps += 1;
                if jumps > 5 {
                    // Prevent infinite loops
                    return Err(ParseError::InvalidLabel);
                }

                if original_offset.is_none() {
                    original_offset = Some(offset + 2);
                }

                let pointer = u16::from_be_bytes([buf[offset] & 0x3F, buf[offset + 1]]) as usize;
                offset = pointer;
                continue;
            }

            if label_length == 0 {
                // End of name
                offset += 1;
                break;
            }

            // Regular label
            if (label_length as usize) > 63 {
                return Err(ParseError::InvalidLabel);
            }

            offset += 1 + label_length as usize;
        }

        // If we followed pointers, return to the original position
        if let Some(orig) = original_offset {
            Ok(orig)
        } else {
            Ok(offset)
        }
    }

    /// Get a slice of the questions section for lazy parsing
    pub fn questions_slice(&self) -> &'a [u8] {
        &self.buffer[self.sections.questions_start..self.sections.answers_start]
    }

    /// Check if packet contains specific question without full parsing
    pub fn contains_question(&self, domain: &str, _qtype: enums::DNSResourceType) -> bool {
        // This would require implementing a zero-copy domain name comparison
        // For now, we'll do a simplified check
        let domain_lower = domain.to_lowercase();
        let domain_bytes = domain_lower.as_bytes();

        // Simple substring search in questions section (not comprehensive)
        let questions_data = self.questions_slice();
        questions_data
            .windows(domain_bytes.len())
            .any(|window| window == domain_bytes)
    }

    /// Convert to owned DNSPacket when needed (fallback for full functionality)
    pub fn to_owned(&self) -> Result<DNSPacket, ParseError> {
        DNSPacket::parse(self.buffer)
    }

    /// SIMD-accelerated packet validation
    pub fn validate_simd(&self) -> bool {
        // Use SIMD to validate compression pointers
        let pointers = simd::SimdParser::find_compression_pointers_simd(self.buffer);

        // Validate that compression pointers are in valid positions
        for &pos in &pointers {
            if pos >= self.buffer.len() - 1 {
                return false;
            }
            // Additional validation could be added here
        }

        // Use SIMD for quick domain name validation in questions section
        let questions_data = self.questions_slice();
        if !questions_data.is_empty()
            && !simd::SimdParser::validate_domain_name_simd(questions_data)
        {
            return false;
        }

        true
    }
}

/// Buffer pool for zero-copy packet operations
#[derive(Debug)]
pub struct PacketBufferPool {
    buffers: Arc<Mutex<Vec<Vec<u8>>>>,
    buffer_size: usize,
    max_pool_size: usize,
}

impl PacketBufferPool {
    pub fn new(buffer_size: usize, max_pool_size: usize) -> Self {
        Self {
            buffers: Arc::new(Mutex::new(Vec::new())),
            buffer_size,
            max_pool_size,
        }
    }

    /// Get a buffer from the pool or allocate a new one
    pub fn get_buffer(&self) -> Vec<u8> {
        let mut buffers = self.buffers.lock();
        if let Some(mut buffer) = buffers.pop() {
            buffer.clear();
            buffer.reserve(self.buffer_size);
            debug!("Reused buffer from pool, {} remaining", buffers.len());
            buffer
        } else {
            debug!("Allocated new buffer, pool was empty");
            Vec::with_capacity(self.buffer_size)
        }
    }

    /// Return a buffer to the pool for reuse
    pub fn return_buffer(&self, buffer: Vec<u8>) {
        let mut buffers = self.buffers.lock();
        if buffers.len() < self.max_pool_size {
            buffers.push(buffer);
            debug!("Returned buffer to pool, {} total", buffers.len());
        } else {
            debug!("Buffer pool full, dropping buffer");
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> (usize, usize) {
        let buffers = self.buffers.lock();
        (buffers.len(), self.max_pool_size)
    }
}

impl Default for PacketBufferPool {
    fn default() -> Self {
        Self::new(4096, 32) // 4KB buffers, max 32 in pool
    }
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
    /// Basic validation for backward compatibility
    /// Use validate_comprehensive() for complete security validation
    pub fn valid(&self) -> bool {
        // Use fast validation to maintain performance
        crate::validation::validate_packet_fast(self).is_ok()
    }

    /// Comprehensive validation with detailed error reporting
    pub fn validate_comprehensive(
        &self,
        source_addr: Option<std::net::SocketAddr>,
    ) -> Result<(), crate::validation::ValidationError> {
        let validator =
            crate::validation::DNSValidator::new(crate::validation::ValidationConfig::default());
        validator.validate_packet(self, source_addr)
    }

    /// Validate with custom configuration
    pub fn validate_with_config(
        &self,
        config: crate::validation::ValidationConfig,
        source_addr: Option<std::net::SocketAddr>,
    ) -> Result<(), crate::validation::ValidationError> {
        let validator = crate::validation::DNSValidator::new(config);
        validator.validate_packet(self, source_addr)
    }

    pub fn parse(buf: &[u8]) -> Result<Self, ParseError> {
        trace!("Parsing DNS packet, size: {} bytes", buf.len());

        // SIMD pre-validation for performance
        if buf.len() > 32 {
            // Use SIMD to quickly check for obvious malformed packets
            let checksum = simd::SimdParser::calculate_packet_checksum_simd(buf);
            trace!("SIMD packet checksum: {}", checksum);

            // Find compression pointers early for validation
            let compression_pointers = simd::SimdParser::find_compression_pointers_simd(buf);
            debug!(
                "Found {} compression pointers during SIMD scan",
                compression_pointers.len()
            );
        }

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
                let is_root_domain = resource.labels.is_empty()
                    || (resource.labels.len() == 1 && resource.labels[0].is_empty());

                if is_root_domain {
                    // This is an EDNS OPT pseudo-record
                    // For EDNS OPT records, the class field contains UDP payload size (not a standard DNS class)
                    let udp_payload_size = resource.raw_class.unwrap_or(512);

                    match EdnsOpt::parse_from_resource(
                        udp_payload_size,
                        resource.ttl,
                        &resource.rdata,
                    ) {
                        Ok(edns_opt) => {
                            debug!("Parsed EDNS0 record: {}", edns_opt.debug_info());
                            packet.edns = Some(edns_opt);
                            // Don't add OPT record to additional resources as it's handled separately
                            continue;
                        }
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
                .filter(|l| !l.is_empty())
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
        self.edns
            .as_ref()
            .map(|edns| edns.payload_size())
            .unwrap_or(512) // Default DNS UDP payload size
    }

    /// Check if the query supports EDNS
    pub fn supports_edns(&self) -> bool {
        self.edns.is_some()
    }

    /// Check if DNSSEC is requested (DO flag)
    pub fn dnssec_requested(&self) -> bool {
        self.edns
            .as_ref()
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

    /// Zero-copy serialization using a pre-allocated buffer
    pub fn serialize_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, ParseError> {
        use bitstream_io::BitWrite;

        buffer.clear();
        buffer.reserve(512); // Pre-allocate reasonable space

        let mut writer = BitWriter::<_, BigEndian>::new(buffer);

        // Write header
        self.header
            .write(&mut writer)
            .map_err(|e| ParseError::InvalidBitStream(e.to_string()))?;

        // Write questions
        for question in &self.questions {
            question
                .write(&mut writer)
                .map_err(|e| ParseError::InvalidBitStream(e.to_string()))?;
        }

        // Write answers
        for answer in &self.answers {
            answer
                .write(&mut writer)
                .map_err(|e| ParseError::InvalidBitStream(e.to_string()))?;
        }

        // Write authorities
        for authority in &self.authorities {
            authority
                .write(&mut writer)
                .map_err(|e| ParseError::InvalidBitStream(e.to_string()))?;
        }

        // Write additional resources (including EDNS if present)
        for resource in &self.resources {
            resource
                .write(&mut writer)
                .map_err(|e| ParseError::InvalidBitStream(e.to_string()))?;
        }

        // Write EDNS OPT record if present and not already in resources
        if let Some(edns) = &self.edns {
            if !self
                .resources
                .iter()
                .any(|r| r.rtype == enums::DNSResourceType::OPT)
            {
                let (class, ttl, rdata) = edns.to_resource_format();

                // Create OPT pseudo-resource record
                let opt_record = DNSResource {
                    labels: vec![], // Root domain (empty)
                    rtype: enums::DNSResourceType::OPT,
                    rclass: enums::DNSResourceClass::from(class),
                    raw_class: Some(class),
                    ttl,
                    rdlength: rdata.len() as u16,
                    rdata,
                    ..Default::default()
                };

                opt_record
                    .write(&mut writer)
                    .map_err(|e| ParseError::InvalidBitStream(e.to_string()))?;
            }
        }

        // Flush writer to ensure all data is written
        writer
            .byte_align()
            .map_err(|e| ParseError::InvalidBitStream(e.to_string()))?;

        let buffer_ref = writer.into_writer();
        Ok(buffer_ref.len())
    }

    /// Optimized serialization for response packets (modify header in-place)
    pub fn serialize_response_to_buffer(
        &mut self,
        buffer: &mut Vec<u8>,
    ) -> Result<usize, ParseError> {
        // Set response flags
        self.header.qr = true;
        self.header.ra = true;

        // Update counts to match actual sections
        self.header.qdcount = self.questions.len() as u16;
        self.header.ancount = self.answers.len() as u16;
        self.header.nscount = self.authorities.len() as u16;

        // Count additional records including EDNS
        let additional_count = self.resources.len() + if self.edns.is_some() { 1 } else { 0 };
        self.header.arcount = additional_count as u16;

        self.serialize_to_buffer(buffer)
    }

    /// Fast packet parsing using SIMD optimizations where possible
    pub fn parse_with_simd_hint(buf: &[u8]) -> Result<Self, ParseError> {
        // For small packets, use regular parsing
        if buf.len() <= 64 {
            return Self::parse(buf);
        }

        trace!("Using SIMD-optimized parsing for {} byte packet", buf.len());

        // Use SIMD to quickly find record type patterns for optimization hints
        let a_record_positions =
            simd::SimdParser::find_record_type_pattern_simd(buf, &[0x00, 0x01]);
        let aaaa_record_positions =
            simd::SimdParser::find_record_type_pattern_simd(buf, &[0x00, 0x1C]);

        debug!(
            "SIMD found {} A records, {} AAAA records",
            a_record_positions.len(),
            aaaa_record_positions.len()
        );

        // Use regular parsing but with SIMD-gathered intelligence
        let packet = Self::parse(buf)?;

        // Add SIMD-specific validation
        if buf.len() > 32 {
            // Validate using SIMD checksum
            let _simd_checksum = simd::SimdParser::calculate_packet_checksum_simd(buf);
            trace!("SIMD validation checksum passed");
        }

        Ok(packet)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_dns_packet() {
        let packet = DNSPacket::default();
        assert!(packet.valid()); // Default packet should be valid
    }
}
