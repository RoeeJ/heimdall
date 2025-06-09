use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    question::DNSQuestion,
    resource::DNSResource,
};
use heimdall::validation::{DNSValidator, ValidationConfig, ValidationError, validate_packet_fast};
use std::net::SocketAddr;

#[test]
fn test_valid_packet_passes() {
    let packet = create_valid_query_packet();

    // Should pass fast validation
    assert!(validate_packet_fast(&packet).is_ok());

    // Should pass comprehensive validation
    assert!(packet.validate_comprehensive(None).is_ok());
}

#[test]
fn test_invalid_opcode_fails() {
    let mut packet = create_valid_query_packet();
    packet.header.opcode = 15; // Invalid opcode

    assert!(matches!(
        validate_packet_fast(&packet),
        Err(ValidationError::InvalidOpcode(15))
    ));
    assert!(packet.validate_comprehensive(None).is_err());
}

#[test]
fn test_empty_query_fails() {
    let mut packet = DNSPacket::default();
    packet.header.qr = false; // This is a query
    packet.questions.clear(); // But no questions

    assert!(matches!(
        validate_packet_fast(&packet),
        Err(ValidationError::EmptyQuestion)
    ));
    assert!(packet.validate_comprehensive(None).is_err());
}

#[test]
fn test_excessive_records_fails() {
    let mut packet = create_valid_query_packet();

    // Add way too many questions
    for i in 0..150 {
        let mut question = DNSQuestion::default();
        question.labels = vec![format!("test{}", i), "com".to_string()];
        question.qtype = DNSResourceType::A;
        packet.questions.push(question);
    }

    assert!(matches!(
        validate_packet_fast(&packet),
        Err(ValidationError::ExcessiveRecordCount)
    ));
}

#[test]
fn test_domain_name_validation() {
    let config = ValidationConfig::default();
    let validator = DNSValidator::new(config);

    // Valid domain names
    assert!(
        validator
            .validate_domain_name(&vec!["google".to_string(), "com".to_string()])
            .is_ok()
    );
    assert!(validator.validate_domain_name(&vec![]).is_ok()); // Root domain
    assert!(
        validator
            .validate_domain_name(&vec!["test123".to_string()])
            .is_ok()
    );
    assert!(
        validator
            .validate_domain_name(&vec!["sub-domain".to_string(), "example".to_string()])
            .is_ok()
    );

    // Invalid domain names
    assert!(matches!(
        validator.validate_domain_name(&vec!["-invalid".to_string()]),
        Err(ValidationError::InvalidLabelFormat(_))
    ));

    assert!(matches!(
        validator.validate_domain_name(&vec!["invalid-".to_string()]),
        Err(ValidationError::InvalidLabelFormat(_))
    ));

    assert!(matches!(
        validator.validate_domain_name(&vec!["invalid@domain".to_string()]),
        Err(ValidationError::InvalidLabelCharacters(_))
    ));

    // Label too long (>63 chars)
    assert!(matches!(
        validator.validate_domain_name(&vec!["a".repeat(64)]),
        Err(ValidationError::LabelTooLong(64))
    ));

    // Domain name too long (>255 chars total)
    let long_labels: Vec<String> = (0..8)
        .map(|i| format!("verylonglabelname{}", i).repeat(2))
        .collect();
    assert!(matches!(
        validator.validate_domain_name(&long_labels),
        Err(ValidationError::DomainNameTooLong(_))
    ));
}

#[test]
fn test_query_type_validation() {
    let mut config = ValidationConfig::default();
    config.block_amplification_queries = true;
    config.allow_zone_transfers = false;
    let validator = DNSValidator::new(config);

    // Allowed types should pass
    assert!(validator.validate_query_type(DNSResourceType::A).is_ok());
    assert!(validator.validate_query_type(DNSResourceType::AAAA).is_ok());
    assert!(
        validator
            .validate_query_type(DNSResourceType::CNAME)
            .is_ok()
    );
    assert!(validator.validate_query_type(DNSResourceType::MX).is_ok());

    // Amplification-prone types should fail
    assert!(matches!(
        validator.validate_query_type(DNSResourceType::ANY),
        Err(ValidationError::PotentialAmplificationAttack)
    ));

    // Zone transfer types should fail
    assert!(matches!(
        validator.validate_query_type(DNSResourceType::AXFR),
        Err(ValidationError::ProhibitedQueryType(DNSResourceType::AXFR))
    ));

    assert!(matches!(
        validator.validate_query_type(DNSResourceType::IXFR),
        Err(ValidationError::ProhibitedQueryType(DNSResourceType::IXFR))
    ));
}

#[test]
fn test_txt_record_validation() {
    let config = ValidationConfig::default();
    let validator = DNSValidator::new(config);

    // Valid TXT records
    let valid_txt1 = vec![5, b'h', b'e', b'l', b'l', b'o']; // "hello"
    assert!(validator.validate_txt_record(&valid_txt1).is_ok());

    let valid_txt2 = vec![3, b'f', b'o', b'o', 3, b'b', b'a', b'r']; // "foo" + "bar"
    assert!(validator.validate_txt_record(&valid_txt2).is_ok());

    let valid_empty = vec![0]; // Empty string
    assert!(validator.validate_txt_record(&valid_empty).is_ok());

    // Invalid TXT records
    let invalid_length = vec![10, b'h', b'i']; // Length 10 but only 2 chars
    assert!(matches!(
        validator.validate_txt_record(&invalid_length),
        Err(ValidationError::MalformedTXTRecord)
    ));

    let invalid_empty = vec![]; // No length byte - this is actually invalid
    assert!(matches!(
        validator.validate_txt_record(&invalid_empty),
        Err(ValidationError::MalformedTXTRecord)
    ));

    // Invalid UTF-8 (though this might be too strict for some use cases)
    let invalid_utf8 = vec![2, 0xFF, 0xFE];
    assert!(matches!(
        validator.validate_txt_record(&invalid_utf8),
        Err(ValidationError::InvalidTXTEncoding)
    ));
}

#[test]
fn test_resource_record_validation() {
    let config = ValidationConfig::default();
    let validator = DNSValidator::new(config);

    // Valid A record
    let mut a_record = DNSResource::default();
    a_record.labels = vec!["example".to_string(), "com".to_string()];
    a_record.rtype = DNSResourceType::A;
    a_record.rclass = DNSResourceClass::IN;
    a_record.ttl = 300;
    a_record.rdata = vec![192, 168, 1, 1]; // Valid IPv4
    a_record.rdlength = 4;
    assert!(validator.validate_resource_record(&a_record).is_ok());

    // Invalid A record (wrong data length)
    let mut invalid_a = a_record.clone();
    invalid_a.rdata = vec![192, 168, 1]; // Only 3 bytes for IPv4
    invalid_a.rdlength = 3; // Update rdlength to match
    assert!(matches!(
        validator.validate_resource_record(&invalid_a),
        Err(ValidationError::InvalidIPv4Address)
    ));

    // Valid AAAA record
    let mut aaaa_record = a_record.clone();
    aaaa_record.rtype = DNSResourceType::AAAA;
    aaaa_record.rdata = vec![0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]; // IPv6
    aaaa_record.rdlength = 16;
    assert!(validator.validate_resource_record(&aaaa_record).is_ok());

    // Invalid AAAA record (wrong data length)
    let mut invalid_aaaa = aaaa_record.clone();
    invalid_aaaa.rdata = vec![0x20, 0x01]; // Only 2 bytes for IPv6
    invalid_aaaa.rdlength = 2; // Update rdlength to match
    assert!(matches!(
        validator.validate_resource_record(&invalid_aaaa),
        Err(ValidationError::InvalidIPv6Address)
    ));

    // TTL too large
    let mut large_ttl = a_record.clone();
    large_ttl.ttl = 86400 * 365; // 1 year (too large)
    assert!(matches!(
        validator.validate_resource_record(&large_ttl),
        Err(ValidationError::ExcessiveTTL(_))
    ));

    // RDATA length inconsistency
    let mut inconsistent = a_record.clone();
    inconsistent.rdlength = 8; // Says 8 bytes
    inconsistent.rdata = vec![192, 168, 1, 1]; // But only 4 bytes
    assert!(matches!(
        validator.validate_resource_record(&inconsistent),
        Err(ValidationError::InconsistentRdataLength)
    ));
}

#[test]
fn test_packet_size_validation() {
    let mut config = ValidationConfig::default();
    config.max_packet_size = 100; // Very small limit for testing
    let validator = DNSValidator::new(config);

    let mut packet = create_valid_query_packet();

    // Add many answers to make packet large
    for i in 0..20 {
        let mut answer = DNSResource::default();
        answer.labels = vec![
            format!("answer{}", i),
            "example".to_string(),
            "com".to_string(),
        ];
        answer.rtype = DNSResourceType::A;
        answer.rclass = DNSResourceClass::IN;
        answer.ttl = 300;
        answer.rdata = vec![192, 168, 1, i as u8];
        answer.rdlength = 4;
        packet.answers.push(answer);
    }

    packet.header.ancount = packet.answers.len() as u16;

    // Should fail due to size limit
    assert!(matches!(
        validator.validate_packet(&packet, None),
        Err(ValidationError::PacketTooLarge(_))
    ));
}

#[test]
fn test_edns_validation() {
    let mut config = ValidationConfig::default();
    config.max_edns_payload_size = 1024; // Small limit for testing

    let mut packet = create_valid_query_packet();
    packet.add_edns(8192, false); // Payload size larger than limit

    let validator = DNSValidator::new(config);
    assert!(matches!(
        validator.validate_packet(&packet, None),
        Err(ValidationError::ExcessiveEDNSPayloadSize(8192))
    ));
}

#[test]
fn test_security_validation() {
    let config = ValidationConfig::default();
    let validator = DNSValidator::new(config);

    // Too many questions (suspicious pattern)
    let mut packet = DNSPacket::default();
    for i in 0..15 {
        let mut question = DNSQuestion::default();
        question.labels = vec![format!("test{}", i), "com".to_string()];
        question.qtype = DNSResourceType::A;
        packet.questions.push(question);
    }
    packet.header.qdcount = packet.questions.len() as u16;

    assert!(matches!(
        validator.validate_packet(&packet, None),
        Err(ValidationError::SuspiciousQueryPattern)
    ));
}

#[test]
fn test_malformed_packet_structure() {
    let config = ValidationConfig::default();
    let validator = DNSValidator::new(config);

    // Packet with mismatched header counts
    let mut packet = create_valid_query_packet();
    packet.header.qdcount = 5; // Says 5 questions
    // But only has 1 question in packet.questions

    assert!(matches!(
        validator.validate_packet(&packet, None),
        Err(ValidationError::MalformedPacket)
    ));
}

#[test]
fn test_custom_validation_config() {
    let mut config = ValidationConfig::default();
    config.allowed_query_types = vec![DNSResourceType::A, DNSResourceType::AAAA];
    config.blocked_query_types = vec![DNSResourceType::MX];

    let mut packet = create_valid_query_packet();
    packet.questions[0].qtype = DNSResourceType::MX; // Blocked type

    let source_addr = "192.168.1.100:53".parse::<SocketAddr>().unwrap();
    assert!(matches!(
        packet.validate_with_config(config, Some(source_addr)),
        Err(ValidationError::ProhibitedQueryType(DNSResourceType::MX))
    ));
}

#[test]
fn test_validation_error_display() {
    let error = ValidationError::InvalidOpcode(15);
    assert_eq!(error.to_string(), "Invalid opcode: 15");

    let error = ValidationError::DomainNameTooLong(300);
    assert_eq!(error.to_string(), "Domain name too long: 300 bytes");

    let error = ValidationError::PotentialAmplificationAttack;
    assert_eq!(error.to_string(), "Potential amplification attack");
}

#[test]
fn test_performance_fast_validation() {
    let packet = create_valid_query_packet();

    // Fast validation should be very quick
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _ = validate_packet_fast(&packet);
    }
    let duration = start.elapsed();

    // Should complete 1000 validations in well under 1ms
    assert!(duration.as_millis() < 10);
}

// Helper function to create a valid DNS query packet for testing
fn create_valid_query_packet() -> DNSPacket {
    let mut packet = DNSPacket::default();

    // Valid header
    packet.header.id = 12345;
    packet.header.qr = false; // Query
    packet.header.opcode = 0; // Standard query
    packet.header.rd = true;
    packet.header.qdcount = 1;

    // Valid question
    let mut question = DNSQuestion::default();
    question.labels = vec!["example".to_string(), "com".to_string()];
    question.qtype = DNSResourceType::A;
    question.qclass = DNSResourceClass::IN;
    packet.questions.push(question);

    packet
}

#[test]
fn test_integration_with_parsing() {
    // Test that validation integrates properly with packet parsing
    let valid_query_bytes = create_simple_query_bytes();

    let packet = DNSPacket::parse(&valid_query_bytes).expect("Should parse valid packet");
    assert!(packet.valid(), "Parsed packet should be valid");
    assert!(
        packet.validate_comprehensive(None).is_ok(),
        "Should pass comprehensive validation"
    );
}

// Helper to create raw DNS query bytes for integration testing
fn create_simple_query_bytes() -> Vec<u8> {
    vec![
        // Header (12 bytes)
        0x12, 0x34, // ID: 0x1234
        0x01, 0x00, // Flags: standard query, recursion desired
        0x00, 0x01, // QDCOUNT: 1 question
        0x00, 0x00, // ANCOUNT: 0 answers
        0x00, 0x00, // NSCOUNT: 0 authority
        0x00, 0x00, // ARCOUNT: 0 additional
        // Question section
        0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', // "example"
        0x03, b'c', b'o', b'm', // "com"
        0x00, // End of name
        0x00, 0x01, // QTYPE: A
        0x00, 0x01, // QCLASS: IN
    ]
}
