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
fn test_phase2_dnssec_type_conversions() {
    // Test all Phase 2 DNSSEC & Security record type mappings
    let test_mappings = vec![
        // DNSSEC Core
        (25, DNSResourceType::KEY),
        (24, DNSResourceType::SIG),
        (30, DNSResourceType::NXT),
        (49, DNSResourceType::DHCID),
        (45, DNSResourceType::IPSECKEY),
        (55, DNSResourceType::HIP),
        // Trust & Validation
        (62, DNSResourceType::CSYNC),
        (63, DNSResourceType::ZONEMD),
        (61, DNSResourceType::OPENPGPKEY),
        // Certificate Management
        (37, DNSResourceType::CERT),
        (36, DNSResourceType::KX),
        (249, DNSResourceType::TKEY),
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
fn test_phase2_dnssec_core_types() {
    let dnssec_types = vec![
        ("secure.example.com", DNSResourceType::KEY),
        ("legacy.example.com", DNSResourceType::SIG),
        ("next.example.com", DNSResourceType::NXT),
        ("dhcp.example.com", DNSResourceType::DHCID),
        ("ipsec.example.com", DNSResourceType::IPSECKEY),
        ("hip.example.com", DNSResourceType::HIP),
    ];

    for (domain, record_type) in dnssec_types {
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
fn test_phase2_trust_validation_types() {
    let trust_types = vec![
        ("sync.example.com", DNSResourceType::CSYNC),
        ("zone.example.com", DNSResourceType::ZONEMD),
        ("pgp.example.com", DNSResourceType::OPENPGPKEY),
    ];

    for (domain, record_type) in trust_types {
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
fn test_phase2_certificate_types() {
    let cert_types = vec![
        ("cert.example.com", DNSResourceType::CERT),
        ("kx.example.com", DNSResourceType::KX),
        ("tkey.example.com", DNSResourceType::TKEY),
    ];

    for (domain, record_type) in cert_types {
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
fn test_all_phase2_types_serialization() {
    let all_phase2_types = vec![
        DNSResourceType::KEY,
        DNSResourceType::SIG,
        DNSResourceType::NXT,
        DNSResourceType::DHCID,
        DNSResourceType::IPSECKEY,
        DNSResourceType::HIP,
        DNSResourceType::CSYNC,
        DNSResourceType::ZONEMD,
        DNSResourceType::OPENPGPKEY,
        DNSResourceType::CERT,
        DNSResourceType::KX,
        DNSResourceType::TKEY,
    ];

    for record_type in all_phase2_types {
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
fn test_phase2_record_type_display() {
    // Ensure all new types can be properly displayed/debugged
    let display_tests = vec![
        (DNSResourceType::KEY, "KEY"),
        (DNSResourceType::SIG, "SIG"),
        (DNSResourceType::NXT, "NXT"),
        (DNSResourceType::DHCID, "DHCID"),
        (DNSResourceType::IPSECKEY, "IPSECKEY"),
        (DNSResourceType::HIP, "HIP"),
        (DNSResourceType::CSYNC, "CSYNC"),
        (DNSResourceType::ZONEMD, "ZONEMD"),
        (DNSResourceType::OPENPGPKEY, "OPENPGPKEY"),
        (DNSResourceType::CERT, "CERT"),
        (DNSResourceType::KX, "KX"),
        (DNSResourceType::TKEY, "TKEY"),
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
