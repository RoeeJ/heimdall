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
fn test_phase1_record_type_conversions() {
    // Test all Phase 1 record type mappings
    let test_mappings = vec![
        // Location & Service Discovery
        (29, DNSResourceType::LOC),
        (35, DNSResourceType::NAPTR),
        (42, DNSResourceType::APL),
        // Mail & Communication
        (99, DNSResourceType::SPF),
        // Security Extensions
        (50, DNSResourceType::NSEC3),
        (51, DNSResourceType::NSEC3PARAM),
        (60, DNSResourceType::CDNSKEY),
        (59, DNSResourceType::CDS),
        // Modern Web
        (64, DNSResourceType::SVCB),
        (53, DNSResourceType::SMIMEA),
        // Experimental/Legacy
        (17, DNSResourceType::RP),
        (18, DNSResourceType::AFSDB),
        // Additional Essential Types
        (39, DNSResourceType::DNAME),
        (256, DNSResourceType::URI),
    ];

    for (number, expected_type) in test_mappings {
        // Test From<u16> conversion
        let parsed_type: DNSResourceType = number.into();
        assert_eq!(
            parsed_type, expected_type,
            "Failed u16->DNSResourceType conversion for {} ({:?})",
            number, expected_type
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
fn test_phase1_location_service_types() {
    let location_types = vec![
        ("geo.example.com", DNSResourceType::LOC),
        ("_service._tcp.example.com", DNSResourceType::NAPTR),
        ("prefix.example.com", DNSResourceType::APL),
    ];

    for (domain, record_type) in location_types {
        let query = create_query(domain, record_type);

        // Test serialization
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query for {}",
            record_type,
            domain
        );

        // Test parsing
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query for {}",
            record_type,
            domain
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.questions[0].qtype, record_type);
    }
}

#[test]
fn test_phase1_security_types() {
    let security_types = vec![
        ("secure.example.com", DNSResourceType::NSEC3),
        ("params.example.com", DNSResourceType::NSEC3PARAM),
        ("child.example.com", DNSResourceType::CDNSKEY),
        ("child.example.com", DNSResourceType::CDS),
        ("cert.example.com", DNSResourceType::SMIMEA),
    ];

    for (domain, record_type) in security_types {
        let query = create_query(domain, record_type);

        // Test serialization
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query for {}",
            record_type,
            domain
        );

        // Test parsing
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query for {}",
            record_type,
            domain
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.questions[0].qtype, record_type);
    }
}

#[test]
fn test_phase1_modern_web_types() {
    let modern_types = vec![
        ("_https._tcp.example.com", DNSResourceType::SVCB),
        ("redirect.example.com", DNSResourceType::DNAME),
        ("resource.example.com", DNSResourceType::URI),
    ];

    for (domain, record_type) in modern_types {
        let query = create_query(domain, record_type);

        // Test serialization
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query for {}",
            record_type,
            domain
        );

        // Test parsing
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query for {}",
            record_type,
            domain
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.questions[0].qtype, record_type);
    }
}

#[test]
fn test_phase1_mail_types() {
    let mail_types = vec![
        ("mail.example.com", DNSResourceType::SPF),
        ("admin.example.com", DNSResourceType::RP),
        ("afs.example.com", DNSResourceType::AFSDB),
    ];

    for (domain, record_type) in mail_types {
        let query = create_query(domain, record_type);

        // Test serialization
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query for {}",
            record_type,
            domain
        );

        // Test parsing
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query for {}",
            record_type,
            domain
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.questions[0].qtype, record_type);
    }
}

#[test]
fn test_all_phase1_types_serialization() {
    let all_phase1_types = vec![
        DNSResourceType::LOC,
        DNSResourceType::NAPTR,
        DNSResourceType::APL,
        DNSResourceType::SPF,
        DNSResourceType::NSEC3,
        DNSResourceType::NSEC3PARAM,
        DNSResourceType::CDNSKEY,
        DNSResourceType::CDS,
        DNSResourceType::SVCB,
        DNSResourceType::SMIMEA,
        DNSResourceType::RP,
        DNSResourceType::AFSDB,
        DNSResourceType::DNAME,
        DNSResourceType::URI,
    ];

    for record_type in all_phase1_types {
        let query = create_query("test.example.com", record_type);

        // Test that the query can be serialized
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query",
            record_type
        );

        // Test that the serialized query can be parsed back
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query",
            record_type
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.header.qdcount, 1);
        assert_eq!(parsed_query.questions.len(), 1);
        assert_eq!(parsed_query.questions[0].qtype, record_type);
        assert_eq!(parsed_query.questions[0].qclass, DNSResourceClass::IN);
    }
}

#[test]
fn test_phase1_record_type_display() {
    // Ensure all new types can be properly displayed/debugged
    let display_tests = vec![
        (DNSResourceType::LOC, "LOC"),
        (DNSResourceType::NAPTR, "NAPTR"),
        (DNSResourceType::APL, "APL"),
        (DNSResourceType::SPF, "SPF"),
        (DNSResourceType::NSEC3, "NSEC3"),
        (DNSResourceType::NSEC3PARAM, "NSEC3PARAM"),
        (DNSResourceType::CDNSKEY, "CDNSKEY"),
        (DNSResourceType::CDS, "CDS"),
        (DNSResourceType::SVCB, "SVCB"),
        (DNSResourceType::SMIMEA, "SMIMEA"),
        (DNSResourceType::RP, "RP"),
        (DNSResourceType::AFSDB, "AFSDB"),
        (DNSResourceType::DNAME, "DNAME"),
        (DNSResourceType::URI, "URI"),
    ];

    for (record_type, expected_name) in display_tests {
        let debug_str = format!("{:?}", record_type);
        assert_eq!(
            debug_str, expected_name,
            "Display name mismatch for {:?}",
            record_type
        );
    }
}
