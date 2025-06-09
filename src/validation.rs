use crate::dns::{DNSPacket, enums::DNSResourceType};
use std::net::SocketAddr;

/// Comprehensive validation errors for DNS packets
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    // Header validation errors
    InvalidOpcode(u8),
    InvalidQueryRcode(u8),
    InvalidReservedBits(u8),
    ExcessiveRecordCount,

    // Domain name validation errors
    DomainNameTooLong(usize),
    LabelTooLong(usize),
    InvalidLabelCharacters(String),
    InvalidLabelFormat(String),
    EmptyQuestion,

    // Query type validation errors
    UnsupportedQueryType,
    ProhibitedQueryType(DNSResourceType),

    // Packet-level validation errors
    PacketTooLarge(usize),
    PacketTooSmall(usize),
    ExcessiveEDNSPayloadSize(u16),
    MalformedPacket,

    // Rate limiting errors
    RateLimitExceeded,
    SourceBlocked,

    // Resource record validation errors
    InconsistentRdataLength,
    InvalidIPv4Address,
    InvalidIPv6Address,
    InvalidMXRecord,
    MalformedTXTRecord,
    InvalidTXTEncoding,
    ExcessiveTTL(u32),

    // Security-specific errors
    SuspiciousQueryPattern,
    PotentialAmplificationAttack,
    CompressionLoopDetected,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::InvalidOpcode(code) => write!(f, "Invalid opcode: {}", code),
            ValidationError::InvalidQueryRcode(code) => {
                write!(f, "Invalid rcode in query: {}", code)
            }
            ValidationError::InvalidReservedBits(bits) => {
                write!(f, "Invalid reserved bits: {}", bits)
            }
            ValidationError::ExcessiveRecordCount => write!(f, "Too many records in section"),
            ValidationError::DomainNameTooLong(len) => {
                write!(f, "Domain name too long: {} bytes", len)
            }
            ValidationError::LabelTooLong(len) => write!(f, "DNS label too long: {} bytes", len),
            ValidationError::InvalidLabelCharacters(label) => {
                write!(f, "Invalid characters in label: {}", label)
            }
            ValidationError::InvalidLabelFormat(label) => {
                write!(f, "Invalid label format: {}", label)
            }
            ValidationError::EmptyQuestion => write!(f, "Question section cannot be empty"),
            ValidationError::UnsupportedQueryType => write!(f, "Unsupported query type"),
            ValidationError::ProhibitedQueryType(qtype) => {
                write!(f, "Prohibited query type: {:?}", qtype)
            }
            ValidationError::PacketTooLarge(size) => write!(f, "Packet too large: {} bytes", size),
            ValidationError::PacketTooSmall(size) => write!(f, "Packet too small: {} bytes", size),
            ValidationError::ExcessiveEDNSPayloadSize(size) => {
                write!(f, "EDNS payload size too large: {} bytes", size)
            }
            ValidationError::MalformedPacket => write!(f, "Malformed DNS packet"),
            ValidationError::RateLimitExceeded => write!(f, "Rate limit exceeded"),
            ValidationError::SourceBlocked => write!(f, "Source IP blocked"),
            ValidationError::InconsistentRdataLength => write!(f, "RDATA length inconsistent"),
            ValidationError::InvalidIPv4Address => write!(f, "Invalid IPv4 address"),
            ValidationError::InvalidIPv6Address => write!(f, "Invalid IPv6 address"),
            ValidationError::InvalidMXRecord => write!(f, "Invalid MX record"),
            ValidationError::MalformedTXTRecord => write!(f, "Malformed TXT record"),
            ValidationError::InvalidTXTEncoding => write!(f, "Invalid TXT record encoding"),
            ValidationError::ExcessiveTTL(ttl) => write!(f, "TTL too large: {}", ttl),
            ValidationError::SuspiciousQueryPattern => {
                write!(f, "Suspicious query pattern detected")
            }
            ValidationError::PotentialAmplificationAttack => {
                write!(f, "Potential amplification attack")
            }
            ValidationError::CompressionLoopDetected => write!(f, "DNS compression loop detected"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Configuration for DNS packet validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    // Packet size limits
    pub max_packet_size: usize,
    pub min_packet_size: usize,
    pub max_udp_response_size: usize,

    // Domain name limits
    pub max_domain_length: usize,
    pub max_label_length: usize,

    // Record limits
    pub max_records_per_section: u16,
    pub max_ttl: u32,
    pub min_ttl: u32,

    // EDNS limits
    pub max_edns_payload_size: u16,

    // Security settings
    pub allow_zone_transfers: bool,
    pub block_amplification_queries: bool,
    pub max_compression_jumps: u8,

    // Query type restrictions
    pub allowed_query_types: Vec<DNSResourceType>,
    pub blocked_query_types: Vec<DNSResourceType>,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_packet_size: 65535,            // Maximum DNS packet size
            min_packet_size: 12,               // Minimum (header only)
            max_udp_response_size: 1232,       // Safe UDP size (RFC 6891)
            max_domain_length: 255,            // RFC 1035
            max_label_length: 63,              // RFC 1035
            max_records_per_section: 100,      // Prevent resource exhaustion
            max_ttl: 86400 * 7,                // 1 week maximum
            min_ttl: 0,                        // Allow 0 TTL
            max_edns_payload_size: 4096,       // Conservative EDNS limit
            allow_zone_transfers: false,       // Block AXFR/IXFR by default
            block_amplification_queries: true, // Block ANY queries
            max_compression_jumps: 5,          // Prevent compression loops
            allowed_query_types: vec![
                DNSResourceType::A,
                DNSResourceType::AAAA,
                DNSResourceType::CNAME,
                DNSResourceType::MX,
                DNSResourceType::NS,
                DNSResourceType::TXT,
                DNSResourceType::PTR,
                DNSResourceType::SOA,
                DNSResourceType::SRV,
            ],
            blocked_query_types: vec![
                DNSResourceType::AXFR,
                DNSResourceType::IXFR,
                DNSResourceType::MAILB,
            ],
        }
    }
}

/// Main DNS packet validator
#[derive(Debug)]
pub struct DNSValidator {
    config: ValidationConfig,
}

impl DNSValidator {
    pub fn new(config: ValidationConfig) -> Self {
        Self { config }
    }

    /// Comprehensive packet validation
    pub fn validate_packet(
        &self,
        packet: &DNSPacket,
        source_addr: Option<SocketAddr>,
    ) -> Result<(), ValidationError> {
        // 1. Header validation
        self.validate_header(&packet.header)?;

        // 2. Packet structure validation
        self.validate_packet_structure(packet)?;

        // 3. Questions validation
        for question in &packet.questions {
            self.validate_question(question)?;
        }

        // 4. Resource records validation
        for answer in &packet.answers {
            self.validate_resource_record(answer)?;
        }
        for authority in &packet.authorities {
            self.validate_resource_record(authority)?;
        }
        for resource in &packet.resources {
            self.validate_resource_record(resource)?;
        }

        // 5. EDNS validation
        if let Some(edns) = &packet.edns {
            self.validate_edns(edns)?;
        }

        // 6. Security validation
        self.validate_security(packet, source_addr)?;

        Ok(())
    }

    /// Validate DNS header fields
    fn validate_header(
        &self,
        header: &crate::dns::header::DNSHeader,
    ) -> Result<(), ValidationError> {
        // Validate opcode (0=QUERY, 1=IQUERY, 2=STATUS)
        if header.opcode > 2 {
            return Err(ValidationError::InvalidOpcode(header.opcode));
        }

        // For queries, rcode should be 0
        if !header.qr && header.rcode != 0 {
            return Err(ValidationError::InvalidQueryRcode(header.rcode));
        }

        // Validate reserved bits (z field should be 0)
        if header.z != 0 {
            return Err(ValidationError::InvalidReservedBits(header.z));
        }

        // Validate section counts
        if header.qdcount > self.config.max_records_per_section
            || header.ancount > self.config.max_records_per_section
            || header.nscount > self.config.max_records_per_section
            || header.arcount > self.config.max_records_per_section
        {
            return Err(ValidationError::ExcessiveRecordCount);
        }

        Ok(())
    }

    /// Validate packet structure and counts
    fn validate_packet_structure(&self, packet: &DNSPacket) -> Result<(), ValidationError> {
        // Estimate serialized size
        let estimated_size = self.estimate_packet_size(packet);

        if estimated_size > self.config.max_packet_size {
            return Err(ValidationError::PacketTooLarge(estimated_size));
        }

        if estimated_size < self.config.min_packet_size {
            return Err(ValidationError::PacketTooSmall(estimated_size));
        }

        // Validate header counts match actual sections
        if packet.header.qdcount as usize != packet.questions.len()
            || packet.header.ancount as usize != packet.answers.len()
            || packet.header.nscount as usize != packet.authorities.len()
        {
            return Err(ValidationError::MalformedPacket);
        }

        // For queries, there should be at least one question
        if !packet.header.qr && packet.questions.is_empty() {
            return Err(ValidationError::EmptyQuestion);
        }

        Ok(())
    }

    /// Validate a DNS question
    fn validate_question(
        &self,
        question: &crate::dns::question::DNSQuestion,
    ) -> Result<(), ValidationError> {
        // Validate domain name
        self.validate_domain_name(&question.labels)?;

        // Validate query type
        self.validate_query_type(question.qtype)?;

        Ok(())
    }

    /// Validate domain name structure
    pub fn validate_domain_name(&self, labels: &[String]) -> Result<(), ValidationError> {
        if labels.is_empty() {
            return Ok(()); // Root domain is valid
        }

        // Calculate total domain name length
        let total_length: usize = labels
            .iter()
            .map(|l| l.len() + 1) // +1 for length byte
            .sum();

        if total_length > self.config.max_domain_length {
            return Err(ValidationError::DomainNameTooLong(total_length));
        }

        // Validate each label
        for label in labels {
            if label.len() > self.config.max_label_length {
                return Err(ValidationError::LabelTooLong(label.len()));
            }

            // Validate label characters (RFC 1123: alphanumeric + hyphens)
            if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(ValidationError::InvalidLabelCharacters(label.clone()));
            }

            // Labels cannot start or end with hyphen
            if label.starts_with('-') || label.ends_with('-') {
                return Err(ValidationError::InvalidLabelFormat(label.clone()));
            }

            // Labels cannot be empty (except root)
            if label.is_empty() && labels.len() > 1 {
                return Err(ValidationError::InvalidLabelFormat(label.clone()));
            }
        }

        Ok(())
    }

    /// Validate query type
    pub fn validate_query_type(&self, qtype: DNSResourceType) -> Result<(), ValidationError> {
        // Check blocked types first
        if self.config.blocked_query_types.contains(&qtype) {
            return Err(ValidationError::ProhibitedQueryType(qtype));
        }

        // Check for zone transfer requests
        if !self.config.allow_zone_transfers {
            match qtype {
                DNSResourceType::AXFR | DNSResourceType::IXFR => {
                    return Err(ValidationError::ProhibitedQueryType(qtype));
                }
                _ => {}
            }
        }

        // Check for amplification-prone queries
        if self.config.block_amplification_queries {
            match qtype {
                DNSResourceType::ANY => {
                    return Err(ValidationError::PotentialAmplificationAttack);
                }
                _ => {}
            }
        }

        // Check allowed types (if list is not empty)
        if !self.config.allowed_query_types.is_empty()
            && !self.config.allowed_query_types.contains(&qtype)
        {
            return Err(ValidationError::UnsupportedQueryType);
        }

        Ok(())
    }

    /// Validate a resource record
    pub fn validate_resource_record(
        &self,
        record: &crate::dns::resource::DNSResource,
    ) -> Result<(), ValidationError> {
        // Validate domain name
        self.validate_domain_name(&record.labels)?;

        // Validate TTL
        if record.ttl > self.config.max_ttl {
            return Err(ValidationError::ExcessiveTTL(record.ttl));
        }

        // Validate RDATA length consistency
        if record.rdlength as usize != record.rdata.len() {
            return Err(ValidationError::InconsistentRdataLength);
        }

        // Type-specific validation
        match record.rtype {
            DNSResourceType::A => {
                if record.rdata.len() != 4 {
                    return Err(ValidationError::InvalidIPv4Address);
                }
            }
            DNSResourceType::AAAA => {
                if record.rdata.len() != 16 {
                    return Err(ValidationError::InvalidIPv6Address);
                }
            }
            DNSResourceType::MX => {
                if record.rdata.len() < 3 {
                    // 2 bytes priority + at least 1 byte domain
                    return Err(ValidationError::InvalidMXRecord);
                }
            }
            DNSResourceType::TXT => {
                self.validate_txt_record(&record.rdata)?;
            }
            _ => {} // Other types validated during parsing
        }

        Ok(())
    }

    /// Validate TXT record format
    pub fn validate_txt_record(&self, rdata: &[u8]) -> Result<(), ValidationError> {
        if rdata.is_empty() {
            return Err(ValidationError::MalformedTXTRecord);
        }

        let mut pos = 0;

        while pos < rdata.len() {
            if pos >= rdata.len() {
                return Err(ValidationError::MalformedTXTRecord);
            }

            let len = rdata[pos] as usize;
            pos += 1;

            if pos + len > rdata.len() {
                return Err(ValidationError::MalformedTXTRecord);
            }

            // Validate text content (basic UTF-8 check)
            if std::str::from_utf8(&rdata[pos..pos + len]).is_err() {
                return Err(ValidationError::InvalidTXTEncoding);
            }

            pos += len;
        }

        Ok(())
    }

    /// Validate EDNS options
    fn validate_edns(&self, edns: &crate::dns::edns::EdnsOpt) -> Result<(), ValidationError> {
        if edns.payload_size() > self.config.max_edns_payload_size {
            return Err(ValidationError::ExcessiveEDNSPayloadSize(
                edns.payload_size(),
            ));
        }

        Ok(())
    }

    /// Security-specific validation
    fn validate_security(
        &self,
        packet: &DNSPacket,
        _source_addr: Option<SocketAddr>,
    ) -> Result<(), ValidationError> {
        // Check for suspicious query patterns
        if packet.questions.len() > 10 {
            return Err(ValidationError::SuspiciousQueryPattern);
        }

        // Check for potential amplification attacks
        if !packet.questions.is_empty() {
            let question = &packet.questions[0];
            if matches!(
                question.qtype,
                DNSResourceType::ANY | DNSResourceType::DNSKEY | DNSResourceType::RRSIG
            ) {
                return Err(ValidationError::PotentialAmplificationAttack);
            }
        }

        Ok(())
    }

    /// Estimate packet size for validation
    fn estimate_packet_size(&self, packet: &DNSPacket) -> usize {
        // DNS header is always 12 bytes
        let mut size = 12;

        // Estimate questions size
        for question in &packet.questions {
            size += self.estimate_domain_size(&question.labels);
            size += 4; // QTYPE + QCLASS
        }

        // Estimate resource records size
        for answer in &packet.answers {
            size += self.estimate_resource_size(answer);
        }
        for authority in &packet.authorities {
            size += self.estimate_resource_size(authority);
        }
        for resource in &packet.resources {
            size += self.estimate_resource_size(resource);
        }

        // EDNS OPT record
        if packet.edns.is_some() {
            size += 11; // Minimum OPT record size
        }

        size
    }

    fn estimate_domain_size(&self, labels: &[String]) -> usize {
        if labels.is_empty() {
            return 1; // Root domain (single 0 byte)
        }

        labels.iter().map(|l| l.len() + 1).sum::<usize>() + 1 // +1 for final 0 byte
    }

    fn estimate_resource_size(&self, record: &crate::dns::resource::DNSResource) -> usize {
        self.estimate_domain_size(&record.labels) + 10 + record.rdata.len() // NAME + TYPE + CLASS + TTL + RDLENGTH + RDATA
    }
}

/// Fast validation for high-performance scenarios
pub fn validate_packet_fast(packet: &DNSPacket) -> Result<(), ValidationError> {
    // Minimal validation for performance-critical paths

    // Basic header validation
    if packet.header.opcode > 2 {
        return Err(ValidationError::InvalidOpcode(packet.header.opcode));
    }

    // Basic structure validation - only check for queries, not responses or default packets
    if !packet.header.qr && packet.header.qdcount > 0 && packet.questions.is_empty() {
        return Err(ValidationError::EmptyQuestion);
    }

    // Basic size validation
    if packet.questions.len() > 100 {
        return Err(ValidationError::ExcessiveRecordCount);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::{enums::DNSResourceType, header::DNSHeader};

    #[test]
    fn test_validator_creation() {
        let config = ValidationConfig::default();
        let validator = DNSValidator::new(config);
        assert!(validator.config.max_packet_size > 0);
    }

    #[test]
    fn test_header_validation() {
        let validator = DNSValidator::new(ValidationConfig::default());
        let mut header = DNSHeader::default();

        // Valid header should pass
        assert!(validator.validate_header(&header).is_ok());

        // Invalid opcode should fail
        header.opcode = 15;
        assert!(matches!(
            validator.validate_header(&header),
            Err(ValidationError::InvalidOpcode(15))
        ));
    }

    #[test]
    fn test_domain_name_validation() {
        let validator = DNSValidator::new(ValidationConfig::default());

        // Valid domain names
        assert!(
            validator
                .validate_domain_name(&vec!["google".to_string(), "com".to_string()])
                .is_ok()
        );
        assert!(validator.validate_domain_name(&vec![]).is_ok()); // Root domain

        // Invalid domain names
        assert!(
            validator
                .validate_domain_name(&vec!["-invalid".to_string()])
                .is_err()
        );
        assert!(
            validator
                .validate_domain_name(&vec!["invalid-".to_string()])
                .is_err()
        );
        assert!(
            validator
                .validate_domain_name(&vec!["a".repeat(64)])
                .is_err()
        ); // Too long label

        // Domain name too long
        let long_labels: Vec<String> = (0..10).map(|i| format!("label{}", i).repeat(6)).collect();
        assert!(validator.validate_domain_name(&long_labels).is_err());
    }

    #[test]
    fn test_query_type_validation() {
        let mut config = ValidationConfig::default();
        config.block_amplification_queries = true;
        let validator = DNSValidator::new(config);

        // Allowed types should pass
        assert!(validator.validate_query_type(DNSResourceType::A).is_ok());
        assert!(validator.validate_query_type(DNSResourceType::AAAA).is_ok());

        // Blocked amplification types should fail
        assert!(matches!(
            validator.validate_query_type(DNSResourceType::ANY),
            Err(ValidationError::PotentialAmplificationAttack)
        ));
    }

    #[test]
    fn test_txt_record_validation() {
        let validator = DNSValidator::new(ValidationConfig::default());

        // Valid TXT record
        let valid_txt = vec![5, b'h', b'e', b'l', b'l', b'o'];
        assert!(validator.validate_txt_record(&valid_txt).is_ok());

        // Invalid TXT record (length extends beyond data)
        let invalid_txt = vec![10, b'h', b'i'];
        assert!(matches!(
            validator.validate_txt_record(&invalid_txt),
            Err(ValidationError::MalformedTXTRecord)
        ));
    }

    #[test]
    fn test_fast_validation() {
        let mut packet = DNSPacket::default();

        // Default packet should pass (not a query, so no questions required)
        assert!(validate_packet_fast(&packet).is_ok());

        // Invalid opcode should fail
        packet.header.opcode = 15;
        assert!(matches!(
            validate_packet_fast(&packet),
            Err(ValidationError::InvalidOpcode(15))
        ));

        // Query with qdcount > 0 but no questions should fail
        packet.header.opcode = 0;
        packet.header.qr = false;
        packet.header.qdcount = 1;
        packet.questions.clear();
        assert!(matches!(
            validate_packet_fast(&packet),
            Err(ValidationError::EmptyQuestion)
        ));
    }
}
