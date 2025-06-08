use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    header::DNSHeader,
    question::DNSQuestion,
};

fn create_query(domain: &str, qtype: DNSResourceType) -> DNSPacket {
    let labels: Vec<String> = domain.split('.').map(|s| s.to_string()).collect();

    DNSPacket {
        header: DNSHeader {
            id: 0x1234,
            qr: false,
            opcode: 0,
            aa: false,
            tc: false,
            rd: true,
            ra: false,
            z: 0,
            rcode: 0,
            qdcount: 1,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        },
        questions: vec![DNSQuestion {
            labels,
            qtype,
            qclass: DNSResourceClass::IN,
        }],
        answers: vec![],
        authorities: vec![],
        resources: vec![],
        edns: None,
    }
}

#[test]
fn test_all_dns_record_types_serialization() {
    let test_cases = vec![
        ("example.com", DNSResourceType::A),
        ("example.com", DNSResourceType::AAAA),
        ("example.com", DNSResourceType::MX),
        ("example.com", DNSResourceType::NS),
        ("example.com", DNSResourceType::CNAME),
        ("example.com", DNSResourceType::TXT),
        ("example.com", DNSResourceType::SOA),
        ("example.com", DNSResourceType::PTR),
        ("example.com", DNSResourceType::SRV),
        ("example.com", DNSResourceType::CAA),
    ];

    for (domain, record_type) in test_cases {
        let query = create_query(domain, record_type);

        // Test that the query can be serialized
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query for {}",
            record_type,
            domain
        );

        // Test that the serialized query can be parsed back
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query for {}",
            record_type,
            domain
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.header.qdcount, 1);
        assert_eq!(parsed_query.questions.len(), 1);
        assert_eq!(parsed_query.questions[0].qtype, record_type);
        assert_eq!(parsed_query.questions[0].qclass, DNSResourceClass::IN);
    }
}

#[test]
fn test_dns_record_type_conversions() {
    // Test common DNS record type number mappings
    let test_mappings = vec![
        (1, DNSResourceType::A),
        (2, DNSResourceType::NS),
        (5, DNSResourceType::CNAME),
        (6, DNSResourceType::SOA),
        (12, DNSResourceType::PTR),
        (15, DNSResourceType::MX),
        (16, DNSResourceType::TXT),
        (28, DNSResourceType::AAAA),
        (33, DNSResourceType::SRV),
        (257, DNSResourceType::CAA),
    ];

    for (number, expected_type) in test_mappings {
        // Test From<u16> conversion
        let parsed_type: DNSResourceType = number.into();
        assert_eq!(
            parsed_type, expected_type,
            "Failed u16->DNSResourceType conversion for {}",
            number
        );

        // Test Into<u16> conversion
        let back_to_number: u16 = expected_type.into();
        assert_eq!(
            back_to_number, number,
            "Failed DNSResourceType->u16 conversion for {:?}",
            expected_type
        );
    }
}

#[test]
fn test_unsupported_record_type() {
    // Test that unsupported record types map to Unknown
    let unsupported_type: DNSResourceType = 9999.into();
    assert_eq!(unsupported_type, DNSResourceType::Unknown);

    // Test that Unknown maps to 0
    let unknown_number: u16 = DNSResourceType::Unknown.into();
    assert_eq!(unknown_number, 0);
}

#[test]
fn test_packet_validation_with_different_record_types() {
    for record_type in [
        DNSResourceType::A,
        DNSResourceType::AAAA,
        DNSResourceType::MX,
        DNSResourceType::NS,
        DNSResourceType::CNAME,
        DNSResourceType::TXT,
    ] {
        let query = create_query("test.example.com", record_type);
        assert!(
            query.valid(),
            "Query with {:?} record type should be valid",
            record_type
        );
    }
}

#[test]
fn test_complex_domain_names() {
    let complex_domains = vec![
        "a.very.long.subdomain.example.com",
        "mail.subdomain.example.org",
        "www.example.co.uk",
        "test-domain.example.net",
    ];

    for domain in complex_domains {
        let query = create_query(domain, DNSResourceType::A);

        // Test serialization roundtrip
        let serialized = query.serialize().expect("Should serialize complex domain");
        let parsed = DNSPacket::parse(&serialized).expect("Should parse complex domain");

        // Verify domain name is preserved
        let original_domain = query.questions[0].labels.join(".");
        let parsed_domain = parsed.questions[0].labels.join(".");
        assert_eq!(
            original_domain, parsed_domain,
            "Domain name should be preserved through serialization"
        );
    }
}
