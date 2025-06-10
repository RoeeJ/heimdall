#![allow(clippy::field_reassign_with_default)]

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
fn test_phase3_network_infrastructure_conversions() {
    // Test all Phase 3 Network & Infrastructure record type mappings
    let test_mappings = vec![
        // Network Infrastructure
        (11, DNSResourceType::WKS),
        (19, DNSResourceType::X25),
        (20, DNSResourceType::ISDN),
        (21, DNSResourceType::RT),
        (22, DNSResourceType::NSAP),
        (23, DNSResourceType::NSAPPTR),
        (26, DNSResourceType::PX),
        (27, DNSResourceType::GPOS),
        // Addressing Extensions
        (38, DNSResourceType::A6),
        (34, DNSResourceType::ATMA),
        (31, DNSResourceType::EID),
        (32, DNSResourceType::NIMLOC),
        (105, DNSResourceType::L32),
        (106, DNSResourceType::L64),
        (107, DNSResourceType::LP),
        // Hardware Identifiers
        (108, DNSResourceType::EUI48),
        (109, DNSResourceType::EUI64),
        (104, DNSResourceType::NID),
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
fn test_phase3_network_infrastructure_types() {
    let network_types = vec![
        ("wks.example.com", DNSResourceType::WKS),
        ("x25.example.com", DNSResourceType::X25),
        ("isdn.example.com", DNSResourceType::ISDN),
        ("route.example.com", DNSResourceType::RT),
        ("nsap.example.com", DNSResourceType::NSAP),
        ("nsapptr.example.com", DNSResourceType::NSAPPTR),
        ("px.example.com", DNSResourceType::PX),
        ("gpos.example.com", DNSResourceType::GPOS),
    ];

    for (domain, record_type) in network_types {
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
fn test_phase3_addressing_extension_types() {
    let addressing_types = vec![
        ("ipv6.example.com", DNSResourceType::A6),
        ("atm.example.com", DNSResourceType::ATMA),
        ("endpoint.example.com", DNSResourceType::EID),
        ("nimrod.example.com", DNSResourceType::NIMLOC),
        ("loc32.example.com", DNSResourceType::L32),
        ("loc64.example.com", DNSResourceType::L64),
        ("locptr.example.com", DNSResourceType::LP),
    ];

    for (domain, record_type) in addressing_types {
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
fn test_phase3_hardware_identifier_types() {
    let hardware_types = vec![
        ("eui48.example.com", DNSResourceType::EUI48),
        ("eui64.example.com", DNSResourceType::EUI64),
        ("node.example.com", DNSResourceType::NID),
    ];

    for (domain, record_type) in hardware_types {
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
fn test_all_phase3_types_serialization() {
    let all_phase3_types = vec![
        // Network Infrastructure
        DNSResourceType::WKS,
        DNSResourceType::X25,
        DNSResourceType::ISDN,
        DNSResourceType::RT,
        DNSResourceType::NSAP,
        DNSResourceType::NSAPPTR,
        DNSResourceType::PX,
        DNSResourceType::GPOS,
        // Addressing Extensions
        DNSResourceType::A6,
        DNSResourceType::ATMA,
        DNSResourceType::EID,
        DNSResourceType::NIMLOC,
        DNSResourceType::L32,
        DNSResourceType::L64,
        DNSResourceType::LP,
        // Hardware Identifiers
        DNSResourceType::EUI48,
        DNSResourceType::EUI64,
        DNSResourceType::NID,
    ];

    for record_type in all_phase3_types {
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
fn test_phase3_record_type_display() {
    // Ensure all new types can be properly displayed/debugged
    let display_tests = vec![
        (DNSResourceType::WKS, "WKS"),
        (DNSResourceType::X25, "X25"),
        (DNSResourceType::ISDN, "ISDN"),
        (DNSResourceType::RT, "RT"),
        (DNSResourceType::NSAP, "NSAP"),
        (DNSResourceType::NSAPPTR, "NSAPPTR"),
        (DNSResourceType::PX, "PX"),
        (DNSResourceType::GPOS, "GPOS"),
        (DNSResourceType::A6, "A6"),
        (DNSResourceType::ATMA, "ATMA"),
        (DNSResourceType::EID, "EID"),
        (DNSResourceType::NIMLOC, "NIMLOC"),
        (DNSResourceType::L32, "L32"),
        (DNSResourceType::L64, "L64"),
        (DNSResourceType::LP, "LP"),
        (DNSResourceType::EUI48, "EUI48"),
        (DNSResourceType::EUI64, "EUI64"),
        (DNSResourceType::NID, "NID"),
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
